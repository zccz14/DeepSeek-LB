#!/usr/bin/env bash
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install --yes ca-certificates certbot curl nginx python3-certbot-nginx

id deepseek-lb >/dev/null 2>&1 || useradd \
  --system \
  --home-dir /var/lib/deepseek-lb \
  --shell /usr/sbin/nologin \
  deepseek-lb

install -d -m 0755 /opt/deepseek-lb/releases
install -d -o deepseek-lb -g deepseek-lb -m 0700 /var/lib/deepseek-lb

install -m 0644 /dev/stdin /etc/systemd/system/deepseek-lb.service <<'UNIT'
[Unit]
Description=DeepSeek-LB
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=deepseek-lb
Group=deepseek-lb
Environment=HOME=/var/lib/deepseek-lb
WorkingDirectory=/var/lib/deepseek-lb
ExecStart=/opt/deepseek-lb/current/deepseek-lb
Restart=on-failure
RestartSec=5s
UMask=0077
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
RestrictSUIDSGID=true
LockPersonality=true
ReadWritePaths=/var/lib/deepseek-lb

[Install]
WantedBy=multi-user.target
UNIT

install -m 0644 /dev/stdin /etc/nginx/sites-available/deepseek-lb <<'NGINX'
server {
    listen 80;
    listen [::]:80;
    server_name deepseek.ntnl.io;

    client_max_body_size 512m;

    location = /api/realtime {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_buffering off;
        proxy_read_timeout 3600s;
        proxy_send_timeout 3600s;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
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
        proxy_set_header Connection "upgrade";
    }

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_request_buffering off;
        proxy_buffering off;
        proxy_read_timeout 3600s;
        proxy_send_timeout 3600s;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
NGINX

ln -sfn /etc/nginx/sites-available/deepseek-lb /etc/nginx/sites-enabled/deepseek-lb
rm -f /etc/nginx/sites-enabled/default
systemctl daemon-reload
systemctl enable deepseek-lb.service
nginx -t
systemctl restart nginx
