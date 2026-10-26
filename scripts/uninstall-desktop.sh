#!/usr/bin/env bash
set -euo pipefail
systemctl --user disable --now cepdesk.service || true
rm -f -- "$HOME/.local/bin/cepdesk" "$HOME/.config/systemd/user/cepdesk.service"
systemctl --user daemon-reload
printf '%s\n' 'Servis kaldırıldı. Eşleşmeler, anahtarlar ve geçmiş korundu.' 'Verileri de silmek için docs/kurulum.md içindeki açık temizleme adımını kullanın.'
