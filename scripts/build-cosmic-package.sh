#!/usr/bin/env bash
set -euo pipefail
# Package directories must remain traversable even under a restrictive caller umask.
umask 022
project_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"
source "$project_root/scripts/project-env.sh"

cargo build --locked --release --manifest-path service/Cargo.toml
cargo build --locked --release --manifest-path Cargo.toml -j 4
stage="$(mktemp -d "${TMPDIR:-/tmp}/cepdesk-package.XXXXXX")"
chmod 755 "$stage"
trap 'rm -rf -- "$stage"' EXIT
install -Dm755 service/target/release/cepdesk "$stage/usr/bin/cepdesk"
install -Dm755 target/release/cepdesk-applet "$stage/usr/bin/cepdesk-applet"
install -Dm644 packaging/debian/cepdesk.service "$stage/usr/lib/systemd/user/cepdesk.service"
mkdir -p "$stage/etc/systemd/user/graphical-session.target.wants"
ln -s /usr/lib/systemd/user/cepdesk.service "$stage/etc/systemd/user/graphical-session.target.wants/cepdesk.service"
for file in io.github.hasimzr.CepDesk.desktop io.github.hasimzr.CepDesk.Manager.desktop; do
  install -Dm644 "resources/$file" "$stage/usr/share/applications/$file"
done
install -Dm644 resources/io.github.hasimzr.CepDesk.metainfo.xml "$stage/usr/share/metainfo/io.github.hasimzr.CepDesk.metainfo.xml"
install -Dm644 resources/io.github.hasimzr.CepDesk.png "$stage/usr/share/icons/hicolor/512x512/apps/io.github.hasimzr.CepDesk.png"
install -Dm644 LICENSE "$stage/usr/share/doc/cepdesk/copyright"
mkdir -p "$stage/DEBIAN"
for script in postinst prerm postrm; do
  install -m755 "packaging/debian/$script" "$stage/DEBIAN/$script"
done
arch="$(dpkg --print-architecture)"
cat > "$stage/DEBIAN/control" <<CONTROL
Package: cepdesk
Version: 0.1.0
Section: utils
Priority: optional
Architecture: $arch
Maintainer: CepDesk contributors
Depends: libc6 (>= 2.39), libfontconfig1, libxkbcommon0, libwayland-client0, libegl1, libgles2, libgbm1, systemd, libglib2.0-0t64, libgdk-pixbuf-2.0-0, libglib2.0-bin
Recommends: cosmic-panel, ffmpeg, pulseaudio-utils, network-manager, librsvg2-common
Description: Phone remote service and COSMIC management applet
 Manage connected devices, command jobs and the CepDesk user service.
 Includes the native host service required by Android and Flatpak clients.
CONTROL
mkdir -p release
package_output="$(mktemp "$project_root/release/.cepdesk-package.XXXXXX.deb")"
trap 'rm -rf -- "$stage"; rm -f -- "$package_output"' EXIT
dpkg-deb --root-owner-group --build "$stage" "$package_output"
mv -- "$package_output" "release/cepdesk_0.1.0_${arch}.deb"
install -Dm755 target/release/cepdesk-applet release/cepdesk-applet
install -Dm755 service/target/release/cepdesk release/cepdesk
