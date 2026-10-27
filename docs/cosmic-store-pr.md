# Add io.github.hasimzr.CepDesk

CepDesk connects Android phones and tablets to a COSMIC desktop over the local network. This submission includes the panel applet, a separate management window and the required desktop service, built from the same pinned source commit.

- Upstream: https://github.com/hasimzr/Cepdesk-CosmicExtantion
- Website / ready-to-install Android APK: https://cepdesk.hasimozer.com
- Android source and APK build instructions: https://github.com/hasimzr/CepdeskMobilApp
- License: MIT
- Languages: English and Turkish, with system-language detection and a language selector.

The management window supports pairing, authenticated device connections and permissions, saved commands, job output/cancellation and service controls. The Android app exposes clipboard, windows, launcher, media, system controls, remote input and screen sharing.

## Android companion and screenshots

The Android companion app is required for phone controls and is installed separately. Users who want a ready-to-install APK can download it from https://cepdesk.hasimozer.com. Users who want to inspect the mobile code or build it themselves can follow the README at https://github.com/hasimzr/CepdeskMobilApp.

The first three store images show the COSMIC desktop application. The final image shows the separately installed Android companion app. Captions and promotional copy are in English. Certificate and device identifiers are redacted; the mobile illustration has been localized into English and private window titles have been redacted. Original screenshots containing personal data are not included in this repository.

## Host service and permissions

The package includes the service and its architecture-matched ELF loader, shared libraries and GdkPixbuf loaders. `org.freedesktop.Flatpak` host execution is necessary to install a user systemd unit on first launch and run the bundled service on the host as the logged-in user. This gives the host service access to desktop operations and user commands. No root service or additional service download is required.

Wayland, IPC and DRI are used for the applet UI. Read-only COSMIC configuration and the COSMIC Settings Daemon provide desktop integration. The user runtime `cepdesk` directory contains the management Unix socket, restricted by directory/socket modes and peer UID checks. User systemd D-Bus access supports start/stop/restart and automatic-start controls. Network connections are handled by the host service.

Existing native/custom units, masks and startup preferences are preserved. Status polling does not restart a stopped service. The managed unit can remain after uninstall; its ExecCondition prevents startup without the installed Flatpak. Touchpad/keyboard access requires host uinput setup; some optional features require host ffmpeg/libx264, PulseAudio tools or NetworkManager.

## Validation

- [ ] Final upstream commit, metadata and real screenshots are public.
- [x] Local source snapshot builds offline in the Freedesktop 25.08 SDK, completes AppStream composition and exports to a local Flatpak repository (x86_64).
- [ ] Rebuild the final public commit using `just build io.github.hasimzr.CepDesk` in the COSMIC repository fork.
- [x] AppStream metadata and both desktop entries validate.
- [x] Native format/Clippy checks, 37 Rust tests and 4 Python packaging tests pass (3 COSMIC/session tests remain ignored in the automated suite).
- [x] Isolated host-setup tests cover first launch, old-ID migration, disabled startup, native/custom units and masks.
- [ ] Fresh COSMIC installation discovers the applet and opens its popup and management window.
- [ ] First-launch service setup, initial password and Android pairing work.
- [ ] Stop/restart, disabled startup, masks and custom/native-unit preservation work.
- [ ] Device disconnect/revocation and job cancellation work.

Complete this list with actual test results before submitting for review. A native build or prebuilt local bundle does not validate this source manifest.
