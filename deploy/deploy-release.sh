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

remove_registrar_downloads() {
  local nginx_site="/etc/nginx/sites-available/deepseek-lb"

  if [ -f "$nginx_site" ]; then
    sed -i '/^[[:space:]]*location \/downloads\/ {/,/^[[:space:]]*}/d' "$nginx_site"
    nginx -t
    systemctl reload nginx
  fi

  rm -rf /opt/deepseek-lb/downloads
}

ensure_realtime_websocket_proxy() {
  local nginx_site="/etc/nginx/sites-available/deepseek-lb"

  [ -f "$nginx_site" ] || return 0
  python3 - "$nginx_site" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()
if "    location = /api/realtime {" in text and "    location = /v1/realtime {" in text:
    raise SystemExit(0)

marker = "    location / {\n"
if marker not in text:
    raise SystemExit("Nginx site is missing the root proxy location")

block = """    location = /api/realtime {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_buffering off;
        proxy_read_timeout 3600s;
        proxy_send_timeout 3600s;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection \"upgrade\";
    }

    location = /v1/realtime {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_buffering off;
        proxy_read_timeout 3600s;
        proxy_send_timeout 3600s;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection \"upgrade\";
    }

"""
path.write_text(text.replace(marker, block + marker, 1))
PY
  nginx -t
  systemctl reload nginx
}

remove_registrar_downloads
ensure_realtime_websocket_proxy

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
