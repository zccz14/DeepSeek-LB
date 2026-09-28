#!/usr/bin/env bash
set -euo pipefail

tag="${1:?release tag is required}"
archive_url="${2:?archive download URL is required}"
checksum_url="${3:?checksum download URL is required}"
archive="deepseek-lb-x86_64-unknown-linux-gnu.tar.gz"
release_dir="/opt/deepseek-lb/releases/$tag"
health_url="http://127.0.0.1:8080/api/health"
temporary_dir="$(mktemp -d)"
previous_release="$(readlink -f /opt/deepseek-lb/current 2>/dev/null || true)"

cleanup() {
  rm -rf "$temporary_dir"
}
trap cleanup EXIT

curl --fail --location --retry 5 --retry-all-errors \
  --output "$temporary_dir/$archive" \
  "$archive_url"
curl --fail --location --retry 5 --retry-all-errors \
  --output "$temporary_dir/$archive.sha256" \
  "$checksum_url"

cd "$temporary_dir"
sha256sum --check "$archive.sha256"
mkdir package
tar -xzf "$archive" -C package

install -d -m 0755 "$release_dir"
install -m 0755 package/deepseek-lb "$release_dir/deepseek-lb"
ln -sfn "$release_dir" /opt/deepseek-lb/current
systemctl restart deepseek-lb.service

healthy=0
for _ in $(seq 1 150); do
  if curl --fail --silent "$health_url" >/dev/null; then
    healthy=1
    break
  fi
  sleep 2
done

if [ "$healthy" -ne 1 ]; then
  if [ -n "$previous_release" ]; then
    ln -sfn "$previous_release" /opt/deepseek-lb/current
    systemctl restart deepseek-lb.service
  fi
  systemctl status deepseek-lb.service --no-pager
  exit 1
fi

find /opt/deepseek-lb/releases -mindepth 1 -maxdepth 1 -type d -printf '%T@ %p\n' \
  | sort -rn \
  | tail -n +4 \
  | cut -d' ' -f2- \
  | xargs --no-run-if-empty rm -rf

systemctl status deepseek-lb.service --no-pager
