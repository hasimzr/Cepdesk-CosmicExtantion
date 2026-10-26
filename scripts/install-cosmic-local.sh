#!/usr/bin/env bash
set -euo pipefail
project_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"
[[ -x release/cepdesk-applet ]] || { echo 'Önce scripts/build-cosmic-package.sh çalıştırın.' >&2; exit 1; }
install -Dm755 release/cepdesk-applet "$HOME/.local/bin/cepdesk-applet"
install -Dm755 service/target/release/cepdesk "$HOME/.local/bin/cepdesk"
install -Dm644 service/cepdesk.service "$HOME/.config/systemd/user/cepdesk.service"
desktop_stage="$(mktemp -d "${TMPDIR:-/tmp}/cepdesk-desktop.XXXXXX")"
trap 'rm -rf -- "$desktop_stage"' EXIT
for file in dev.cepdesk.Applet.desktop dev.cepdesk.Applet.Manager.desktop; do
  # Absolute path works even when the session has not refreshed ~/.local/bin in PATH.
  sed "s|Exec=cepdesk-applet|Exec=$HOME/.local/bin/cepdesk-applet|" "resources/$file" > "$desktop_stage/$file"
  install -Dm644 "$desktop_stage/$file" "$HOME/.local/share/applications/$file"
done
install -Dm644 resources/dev.cepdesk.Applet.png "$HOME/.local/share/icons/hicolor/512x512/apps/dev.cepdesk.Applet.png"
install -Dm644 resources/dev.cepdesk.Applet.metainfo.xml "$HOME/.local/share/metainfo/dev.cepdesk.Applet.metainfo.xml"
systemctl --user daemon-reload
systemctl --user enable --now cepdesk.service
systemctl --user restart cepdesk.service
printf '%s\n' 'COSMIC Ayarları → Masaüstü → Panel → Eklentileri düzenle bölümünden CepDesk ekleyin.'
