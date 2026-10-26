# CepDesk — COSMIC desktop applet

Control your COSMIC desktop from an Android phone or tablet over your local network. This repository includes the panel applet, management window, and required desktop service.

[Website and Android APK](https://cepdesk.hasimozer.com) · [Installation](docs/kurulum.md) · [Store submission](docs/cosmic-magaza.md) · [MIT license](LICENSE)

## User installation

Download the desktop `.deb` package from the website or this repository's GitHub Releases section. On Pop!_OS 24.04 / COSMIC, run the following in the download directory:

```sh
sudo apt install ./cepdesk_0.1.0_amd64.deb
cepdesk-applet --settings
```

Set the initial connection password in the management window. Install the Android APK on your phone, then select your computer on the same local network to pair it. Add the panel applet through COSMIC Settings → Desktop → Panel. Publication in the COSMIC Store is not yet complete.

## Repository layout

- `src/`, `resources/`: COSMIC applet, management window, and store metadata.
- `service/`: the Rust service that handles phone connections and desktop operations; this is a required component.
- `packaging/`, `scripts/`, `tests/`, `docs/`: packaging, builds, tests, and documentation.
- `release/`: local build outputs; excluded from Git. Use GitHub Releases for distribution.

Android and web sources are not included in this repository and are not required to build the desktop components. Keep `Cargo.lock` and `service/Cargo.lock` in the source repository. The paths below work regardless of the name of your cloned directory.

## Desktop components

A libcosmic-based panel applet and a separate management window for the local computer service. `cepdesk-applet` runs in the panel, while `cepdesk-applet --settings` opens a regular window. The panel applet registers in COSMIC's applet list with `X-CosmicApplet=true`. Dependencies are pinned through a fixed libcosmic commit and Cargo.lock; the project has been built with Rust 1.99.

The service is a subcomponent of this project under `service/`, rather than a separate top-level project. `src/` contains the applet and management interface; `service/src/` contains phone connection and desktop operation logic. `scripts/`, `packaging/`, `tests/`, `docs/`, and `release/` are shared by both components. Run the commands in this document from the root of this desktop repository, which contains Cargo.toml and service/.

## Language and interface

The panel popup shows status, quick service controls, and settings. Detailed device, command, job, and API management is available in a separate window opened through **Management center**. The selected page is highlighted in the sidebar, and the heading remains visible while navigating.

The interface supports Turkish and English. On first launch, the system language is detected using `LC_ALL`, `LC_MESSAGES`, then `LANG`, in that order of precedence; English is used for languages other than Turkish. Under **Settings → Language**, select **Use device language**, **Türkçe**, or **English**. The preference is stored in `$XDG_CONFIG_HOME/cepdesk/applet-language` (default: `~/.config/cepdesk/applet-language`); open panel and management windows use the same preference on their next refresh. In the Flatpak version, configuration is stored in the user directory inside the sandbox.

The Status and Settings pages provide links to the [mobile app and project website](https://cepdesk.hasimozer.com) and [developer Haşim Özer's GitHub profile](https://github.com/hasimzr). Links open in the default browser through the desktop portal.

## Management

- Status: service state, registered/online device counts, port, discovery, and certificate; start/stop/restart and automatic startup.
- Devices: authenticated connections/IP addresses, permission to run arbitrary commands, disconnecting the current connection, and permanent key revocation.
- Commands: add/update/delete definitions and run commands with or without saving them.
- Jobs: jobs from all devices and local management, selected output, terminal responses, cancellation, and history deletion.
- Settings: set/change the initial connection password, clipboard monitoring, and pairing with a temporary code, including approval/rejection.
- Service API: method + JSON for clipboard/window/media/system/command calls. Examples are in the main protocol document. The phone manages the network session lifecycle for monitor streaming, uinput, and audio streaming.

Online status is based on active sessions, rather than the presence of a registered device: a session is added after authentication and removed through RAII on disconnect or error. A 15-second ping interval and a 90-second timeout clean up lost network connections. Disconnecting a device does not permanently block it; the phone can reconnect. Revoking authorization revokes the device's key and cancels its running jobs.

## Build and installation

System dependencies (Pop!_OS/Ubuntu): Rust, pkg-config, a C/C++ compiler, cmake, libxkbcommon-dev, libfontconfig-dev, libfreetype-dev, libexpat1-dev, libwayland-dev, libegl-dev, libgles-dev, and libgbm-dev. The system Rust toolchain is used. Scripts can also use optional tools in `.tools` if present; this directory is not required in a clean clone. Packaging additionally requires `dpkg-deb`; `appstreamcli` and `desktop-file-validate` can be used to validate metadata.

From the root of this repository:

```sh
./scripts/build-cosmic-package.sh
```

Package: `release/cepdesk_0.1.0_amd64.deb` (amd64 on this machine). The service and applet are included in the same package. The target is Pop!_OS 24.04 or an equivalent system with glibc 2.39+. Install the package with:

```sh
sudo apt install ./release/cepdesk_0.1.0_amd64.deb
cepdesk-applet --settings
```

Installing the package automatically starts the service in active graphical user sessions; it also starts in subsequent graphical sessions. Opening the applet or management window starts the service once if automatic startup is enabled. Regular status refreshes do not restart the service, so choosing Stop remains effective. The button to disable automatic startup overrides the package default with a per-user systemd mask; enabling it again removes the mask and starts the service. Removing the package stops the service in active sessions. User configuration is preserved.

The service supports starting for the first time under systemd without a terminal. Until a password is set, password-based pairing from phones is rejected; set the initial password in the management window. Phone/server keys and existing data are preserved.

If you previously performed a user installation with `scripts/install-desktop.sh`, the user unit file takes precedence over the system package. To update an existing user installation, run `./scripts/install-cosmic-local.sh` from the source root. The script installs the service/applet binaries in the user directory and restarts the service.

Add CepDesk through COSMIC Settings → Desktop → Panel → Edit applets. Labels may vary by COSMIC version. The same applet can be added to multiple panels; the management window can also be opened from the application launcher.

## Trust boundary

The management Unix socket at `$XDG_RUNTIME_DIR/cepdesk/admin.sock` is restricted to the same user through directory permissions of 0700, socket permissions of 0600, and a peer UID check. Phone keys/passwords are never included in list responses. The password is passed only through the user socket, never as a shell argument. Service operations use user systemd D-Bus calls for the fixed `cepdesk.service` unit; no shell concatenation is used. Sudo commands explicitly prompt for a password in the user's terminal session; no root service is installed.

## Store

[Store preparation and publishing steps](docs/cosmic-magaza.md). The Flatpak template includes both the service and applet. When the applet/management window is first opened, the bundled host service is set up and started as a user systemd service; a separate `.deb` installation is not required. This flow uses the `org.freedesktop.Flatpak` permission to run commands on the host. The `.deb` version also includes both components. The project will not be marked as store-ready until the source repository, screenshots, and actual Flatpak build validation are complete.

## Development checks

```sh
cargo build --locked --release --manifest-path service/Cargo.toml
cargo build --locked --release
bash scripts/check.sh
```

`check.sh` runs formatting checks, clippy, and tests for both Rust packages; Python packaging tests; and AppStream/desktop file validation if the required tools are installed. D-Bus tests require `dbus-run-session`. Tests that require an actual COSMIC session are run separately with `bash scripts/verify-session.sh`; these perform clipboard and window operations.

For touchpad/keyboard support, the user must run `sudo ./scripts/setup-input.sh` once. H.264 video may require `ffmpeg`/libx264, audio may require PulseAudio tools, and Wi-Fi may require NetworkManager. Installing the Flatpak does not automatically provide these host dependencies.

## Publishing to GitHub

Place the **contents** of this directory at the repository root: `Cargo.toml`, `service/`, `scripts/`, `resources/`, `README.md`, `LICENSE`, and `.gitignore` should appear at the root. `.gitignore` excludes build outputs, local tools, and common key/test files; review `git status --short` and staged changes before running `git add`. Do not copy passwords, certificate private keys, or device state into the source directory. Upload packages to GitHub Releases. The store manifest builds from this standalone repository layout.
# Depdesk-CosmicExtantion
