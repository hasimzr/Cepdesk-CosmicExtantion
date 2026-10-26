#!/usr/bin/env bash
set -euo pipefail
project_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"
source "$project_root/scripts/project-env.sh"
session_dir="$(mktemp -d /tmp/cepdesk-session-XXXXXX)"
trap 'rm -rf -- "$session_dir"' EXIT
export CEP_DESK_TEST_ROOT="$session_dir"
cargo test --locked --manifest-path service/Cargo.toml cosmic_session_clipboard_and_window_roundtrip -- --ignored --nocapture
