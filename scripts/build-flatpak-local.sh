#!/usr/bin/env bash
# Local integration bundle from this machine's release binaries.
# Store submissions use the source-building manifest, not these host prebuilts.
set -euo pipefail
project_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"
[[ -x service/target/release/cepdesk && -x release/cepdesk-applet ]] || {
  echo 'Önce scripts/build-cosmic-package.sh çalıştırın.' >&2
  exit 1
}
stage="$(mktemp -d "${TMPDIR:-/tmp}/cepdesk-flatpak.XXXXXX")"
repo="$(mktemp -d "${TMPDIR:-/tmp}/cepdesk-flatpak-repo.XXXXXX")"
trap 'rm -rf -- "$stage" "$repo"' EXIT
flatpak build-init "$stage" io.github.hasimzr.CepDesk org.freedesktop.Platform org.freedesktop.Platform 25.08
python3 scripts/bundle-flatpak-service.py --binary service/target/release/cepdesk --output "$stage/files/libexec/cepdesk-host"
install -Dm755 release/cepdesk-applet "$stage/files/bin/cepdesk-applet"
for helper in cepdesk-flatpak-setup cepdesk-host-setup cepdesk-host-run; do
  install -Dm755 "packaging/flatpak/helpers/$helper" "$stage/files/libexec/$helper"
done
install -Dm755 packaging/flatpak/helpers/cepdesk-host-launch "$stage/files/bin/cepdesk-host-launch"
for desktop in io.github.hasimzr.CepDesk.desktop io.github.hasimzr.CepDesk.Manager.desktop; do
  install -Dm644 "resources/$desktop" "$stage/files/share/applications/$desktop"
done
install -Dm644 resources/io.github.hasimzr.CepDesk.metainfo.xml "$stage/files/share/metainfo/io.github.hasimzr.CepDesk.metainfo.xml"
mkdir -p "$stage/files/share/icons/hicolor/512x512/apps"
gdk-pixbuf-thumbnailer -s 512 resources/io.github.hasimzr.CepDesk.png "$stage/files/share/icons/hicolor/512x512/apps/io.github.hasimzr.CepDesk.png"
install -Dm644 LICENSE "$stage/files/share/licenses/cepdesk/LICENSE"
mapfile -t permissions < <(python3 -c 'import json; print("\n".join(json.load(open("packaging/flatpak/io.github.hasimzr.CepDesk.json.in"))["finish-args"]))')
flatpak build-finish --command=cepdesk-applet "${permissions[@]}" "$stage"
if [[ ${CEPDESK_LOW_DISK:-0} == 1 ]]; then
  # Only this disposable export repo; never alter installed Flatpak repos.
  /usr/bin/python3 - "$repo" <<'PY'
import gi
import shutil
import sys
gi.require_version('OSTree', '1.0')
from gi.repository import Gio, OSTree
if shutil.disk_usage(sys.argv[1]).free < 512 * 1024 * 1024:
    raise SystemExit('Yerel test paketi için en az 512 MiB boş alan gerekiyor.')
repo = OSTree.Repo.new(Gio.File.new_for_path(sys.argv[1]))
repo.create(OSTree.RepoMode.ARCHIVE_Z2, None)
config = repo.copy_config()
config.set_string('core', 'min-free-space-percent', '0')
repo.write_config(config)
PY
fi
flatpak build-export "$repo" "$stage" stable
mkdir -p release
output="$(mktemp "$project_root/release/.cepdesk-flatpak.XXXXXX")"
trap 'rm -rf -- "$stage" "$repo"; rm -f -- "$output"' EXIT
flatpak build-bundle "$repo" "$output" io.github.hasimzr.CepDesk stable
mv -- "$output" release/CepDesk-cosmic-0.1.0.flatpak
