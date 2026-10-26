#!/usr/bin/env bash
set -euo pipefail
project_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"
source "$project_root/scripts/project-env.sh"
cargo fmt --manifest-path service/Cargo.toml --check
cargo clippy --locked --manifest-path service/Cargo.toml --all-targets -- -D warnings
dbus-run-session -- cargo test --locked --manifest-path service/Cargo.toml
python3 -m unittest discover -s tests -v
cargo fmt --manifest-path Cargo.toml --check
cargo clippy --locked --manifest-path Cargo.toml --all-targets -- -D warnings
dbus-run-session -- cargo test --locked --manifest-path Cargo.toml
if command -v appstreamcli >/dev/null; then
  appstreamcli validate --no-net resources/io.github.hasimzr.CepDesk.metainfo.xml
fi
if command -v desktop-file-validate >/dev/null; then
  desktop-file-validate resources/io.github.hasimzr.CepDesk.desktop resources/io.github.hasimzr.CepDesk.Manager.desktop
fi
