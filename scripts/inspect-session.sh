#!/usr/bin/env bash
set -euo pipefail
busctl --user list --no-pager | rg '^(org\.mpris|org\.freedesktop\.portal\.Desktop|com\.system76)'
systemctl --user list-units --type=target --all --no-pager
systemctl --user show-environment | rg '^(WAYLAND_DISPLAY|XDG_SESSION_TYPE|XDG_CURRENT_DESKTOP)='
gdbus introspect --session --dest org.freedesktop.portal.Desktop --object-path /org/freedesktop/portal/desktop | rg '^  interface'
