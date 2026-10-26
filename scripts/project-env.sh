#!/usr/bin/env bash
# Sourced by build/check scripts. Local tools are optional; system tools work in a clean clone.
cepdesk_tools="$project_root/.tools"
if [[ ! -d "$cepdesk_tools" && -d "$project_root/../.tools" ]]; then
  cepdesk_tools="$project_root/../.tools"
fi
if [[ -x "$cepdesk_tools/cargo/bin/cargo" ]]; then
  export CARGO_HOME="$cepdesk_tools/cargo" RUSTUP_HOME="$cepdesk_tools/rustup"
  export PATH="$CARGO_HOME/bin:$PATH"
fi
if [[ -d "$cepdesk_tools/native-sysroot/usr/lib/x86_64-linux-gnu/pkgconfig" ]]; then
  export PKG_CONFIG_PATH="$cepdesk_tools/native-sysroot/usr/lib/x86_64-linux-gnu/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
fi
