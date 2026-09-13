#!/usr/bin/env bash
set -euo pipefail

role=${1:?usage: install-host.sh nyc|sfo key-directory}
keys=${2:?usage: install-host.sh nyc|sfo key-directory}
case "$role" in nyc|sfo) ;; *) exit 64 ;; esac

install -d -m 0755 /usr/local/libexec /etc/ronitnath-delivery
install -d -m 0700 /data/crypt/ronitnath/releases-v2
install -m 0755 deploy/host-entry.py /usr/local/libexec/ronitnath-host-entry
install -m 0755 deploy/host-dispatch.py /usr/local/libexec/ronitnath-host-dispatch

if [[ $role == nyc ]]; then
  accounts=(delivery-status delivery-submit)
else
  accounts=(delivery-replica)
fi
for account in "${accounts[@]}"; do
  id "$account" >/dev/null 2>&1 || useradd --system --create-home --shell /bin/bash "$account"
  usermod --shell /bin/bash "$account"
  home=$(getent passwd "$account" | cut -d: -f6)
  install -d -o "$account" -g "$account" -m 0700 "$home/.ssh"
  install -o "$account" -g "$account" -m 0600 /dev/null "$home/.ssh/authorized_keys"
  printf '%s %s\n' 'restrict,command="/usr/local/libexec/ronitnath-host-dispatch"' "$(cat "$keys/$account.pub")" > "$home/.ssh/authorized_keys"
done

cat >/etc/sudoers.d/ronitnath-delivery <<'EOF'
delivery-status ALL=(root) NOPASSWD: /usr/local/libexec/ronitnath-host-entry status *
delivery-submit ALL=(root) NOPASSWD: /usr/local/libexec/ronitnath-host-entry submit *
delivery-replica ALL=(root) NOPASSWD: /usr/local/libexec/ronitnath-host-entry replica *
EOF
chmod 0440 /etc/sudoers.d/ronitnath-delivery
visudo -cf /etc/sudoers.d/ronitnath-delivery

if [[ $role == nyc ]]; then
  install -m 0755 deploy/coordinator.py /usr/local/libexec/ronitnath-coordinator
  install -m 0755 deploy/cleanup.py /usr/local/libexec/ronitnath-delivery-cleanup
  install -m 0644 deploy/ronitnath-delivery@.service /etc/systemd/system/ronitnath-delivery@.service
  install -m 0644 deploy/ronitnath-delivery-cleanup.service /etc/systemd/system/ronitnath-delivery-cleanup.service
  install -m 0644 deploy/ronitnath-delivery-cleanup.timer /etc/systemd/system/ronitnath-delivery-cleanup.timer
  systemctl daemon-reload
  systemctl enable --now ronitnath-delivery-cleanup.timer
fi
