#!/usr/bin/env bash
set -euo pipefail
# Run with sudo from the desktop account that runs CepDesk.
if [[ "$EUID" -ne 0 || -z "${SUDO_USER:-}" || "$SUDO_USER" == root ]]; then
  echo 'Masaüstü kullanıcınızdan çalıştırın: sudo ./scripts/setup-input.sh' >&2
  exit 1
fi
input_uid="$(id -u "$SUDO_USER")"
install -d /etc/udev/rules.d /etc/modules-load.d
printf 'uinput\n' > /etc/modules-load.d/cepdesk-input.conf
printf 'KERNEL=="uinput", SUBSYSTEM=="misc", OWNER="%s", MODE="0600"\n' "$input_uid" > /etc/udev/rules.d/99-cepdesk-input.rules
modprobe uinput
udevadm control --reload-rules
udevadm trigger --subsystem-match=misc --sysname-match=uinput
udevadm settle
echo 'Giriş izni hazır. CepDesk servisini yeniden başlatın ve telefondan yeniden bağlanın.'
