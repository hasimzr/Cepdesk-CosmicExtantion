# Publishing CepDesk in the COSMIC Store

CepDesk is submitted as a source-built Flatpak to [pop-os/cosmic-flatpak](https://github.com/pop-os/cosmic-flatpak). Publishing the source or a `.deb` on GitHub does not submit the applet to the store.

## Upstream and application identity

- Source: https://github.com/hasimzr/Cepdesk-CosmicExtantion
- Website and ready-to-install Android APK: https://cepdesk.hasimozer.com
- Android source and build instructions: https://github.com/hasimzr/CepdeskMobilApp
- Store application ID: `io.github.hasimzr.CepDesk`
- License: MIT; AppStream metadata: CC0-1.0.
- Runtime and SDK: Freedesktop 25.08, matching the stable COSMIC BaseApp available on Flathub.

The former local ID was `dev.cepdesk.Applet`. Existing native users may need to remove the old panel entry and add the new one after upgrading. Existing Flatpak test installations use a different application ID and are not automatically upgraded by the store package. The first-launch helper migrates a unit bearing CepDesk's managed-unit marker and the old launch command, preserving its enabled/disabled state. Custom/native units and masks are preserved. Language preferences and service data paths are unchanged for native installations; Flatpak app configuration belongs to its application ID.

## Submission files

`packaging/flatpak/io.github.hasimzr.CepDesk.json.in` builds both the Rust service and applet from the same immutable upstream commit. The service, ELF loader, shared libraries and image loaders are bundled for the build architecture. Do not submit the prebuilt local `.flatpak` bundle in place of this manifest.

The applet registers through `X-CosmicApplet=true`. Its AppStream metadata provides `com.system76.CosmicApplet`; the management window has a separate desktop launcher.

## Permissions and host integration

The applet uses Wayland, IPC and GPU rendering; read-only COSMIC configuration; and the user runtime directory `cepdesk`, created at launch for first-install socket access. It talks to the user systemd manager and COSMIC Settings Daemon.

`org.freedesktop.Flatpak` permits host execution as the logged-in user. This is required to prepare and run the bundled desktop service outside the sandbox. It is a broad permission and must be explained in the submission PR. No root service, separate `.deb`, or second service download is used. Store acceptance is subject to maintainer review.

On first launch, the helper prepares `cepdesk.service` in the user systemd configuration. Subsequent enabled-service startup uses D-Bus. Polling does not restart a stopped service. Custom/native services, masks and disabled startup preferences are preserved. Updates to the bundled service take effect on its next start.

The Android APK remains a separate download. Users set the initial pairing password in the management window. Host uinput permission, ffmpeg/libx264, PulseAudio tools and NetworkManager remain optional host requirements for the respective features.

Before uninstalling, stop the service and disable automatic startup in the management window. The managed user unit can remain after Flatpak removal; its `ExecCondition` prevents startup when the application is absent. Remove only a user unit carrying `# CepDesk Flatpak managed unit v1` if manually cleaning it up, then reload the user systemd manager.

## Prepare and test

1. Review the prepared images under `resources/screenshots/` and their privacy notes. The gallery includes desktop management screenshots and one separately labeled Android companion illustration, with English captions. Sensitive identifiers are redacted and the Android illustration is localized into English. AppStream image URLs target the upstream `main` branch; publish these files there (or update the URLs for the final public ref) and verify that every URL resolves before submission. Commit and push the images together with the packaging changes. Do not publish the private originals.
2. Obtain [flatpak-cargo-generator.py](https://github.com/flatpak/flatpak-builder-tools/tree/master/cargo) from the official Flatpak tools repository. Use Python 3.11 or later and install the generator's declared dependencies in a virtual environment. Generate both source lists from the final lockfiles:

   ```sh
   mkdir -p release/store-sources
   python3 /path/to/flatpak-cargo-generator.py Cargo.lock -o release/store-sources/cargo-sources.json
   python3 /path/to/flatpak-cargo-generator.py service/Cargo.lock -o release/store-sources/service-cargo-sources.json
   python3 scripts/prepare-cosmic-store.py \
     --repository https://github.com/hasimzr/Cepdesk-CosmicExtantion.git \
     --commit "$(git rev-parse HEAD)" \
     --cargo-sources release/store-sources/cargo-sources.json \
     --service-cargo-sources release/store-sources/service-cargo-sources.json
   ```

   The preparation script preserves all Git source variants from each lockfile, including unqualified URLs used alongside an explicit revision of the same repository. This avoids missing-source failures in offline builds.

   The chosen commit must be public and contain the final metadata and screenshots. Regenerate the submission after any upstream source change. The prepared sibling metainfo under `release/` is for review; the build installs the committed upstream metadata.
3. Install `flatpak`, `flatpak-builder` and `just`, and enable the Flathub source for the SDK, Rust SDK extension and COSMIC BaseApp. Fork and clone `pop-os/cosmic-flatpak`. Copy `release/store-submission/app/io.github.hasimzr.CepDesk/` to `app/io.github.hasimzr.CepDesk/` in the fork.
4. From the fork root, run:

   ```sh
   just build io.github.hasimzr.CepDesk
   ```

   Complete source compilation, AppStream and desktop-file validation. In a clean COSMIC user session, verify panel discovery/popup, management window, service first launch, password setup, pairing, device permissions/revocation, Stop, restart and automatic-start preferences. A native build or a locally prebuilt Flatpak alone does not establish that the store source manifest works.
5. Use [cosmic-store-pr.md](cosmic-store-pr.md) as the PR description. Replace pending validation entries with actual results. Open a PR from the fork to `pop-os/cosmic-flatpak`'s default branch after completing the checks, or explicitly mark an incomplete submission as a draft.

## User installation after publication

If the COSMIC source is not enabled:

```sh
flatpak remote-add --if-not-exists --user cosmic https://apt.pop-os.org/cosmic/cosmic.flatpakrepo
```

After maintainer acceptance and repository publication, users can find CepDesk in the COSMIC Store. They then add the installed applet to the panel through COSMIC Settings.

## Validation records

Local validation output is saved under `release/` and is excluded from Git. Record the exact tested commit, runtime, architecture and remaining checks in the PR; do not describe earlier native or prebuilt-bundle tests as verification of the final source manifest.
