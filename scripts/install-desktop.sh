#!/usr/bin/env bash
set -euo pipefail
project_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
source "$project_root/scripts/project-env.sh"
if [[ ! -x "$project_root/service/target/release/cepdesk" ]]; then
  cargo build --locked --release --manifest-path "$project_root/service/Cargo.toml"
fi
install -Dm755 "$project_root/service/target/release/cepdesk" "$HOME/.local/bin/cepdesk"
install -Dm644 "$project_root/service/cepdesk.service" "$HOME/.config/systemd/user/cepdesk.service"
if [[ ! -f "${XDG_CONFIG_HOME:-$HOME/.config}/cepdesk/password.json" ]]; then
  "$HOME/.local/bin/cepdesk" password
fi
systemctl --user import-environment WAYLAND_DISPLAY XDG_CURRENT_DESKTOP XDG_SESSION_TYPE
systemctl --user daemon-reload
systemctl --user enable --now cepdesk.service
"$HOME/.local/bin/cepdesk" status
