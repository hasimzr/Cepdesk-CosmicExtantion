# Store gallery

These publication copies are intended for the English COSMIC Store listing and desktop README. The original private screenshots remain outside this repository.

| File | Content | Publication treatment |
| --- | --- | --- |
| `desktop-overview.png` | Desktop service and startup overview | Certificate fingerprint fully covered with an opaque REDACTED label. |
| `desktop-devices.png` | Device permissions and access revocation | Both device UUIDs fully covered; device names changed to generic English labels. |
| `desktop-settings.png` | Password setup, clipboard tracking and temporary pairing | Original English screenshot; password fields are empty and no pairing code is displayed. |
| `android-controls.png` | Separately installed Android companion: commands, launcher and system controls | English localization of the supplied Turkish screenshot; private window titles covered; phone status information removed. |

The three edited images were processed with the built-in imagegen tool and visually reviewed. They are edited promotional illustrations derived from supplied screenshots, not pixel-identical captures or verification of a running English Android session. The tool rescaled the desktop copies; AppStream records the actual output dimensions. The settings screenshot is unmodified.

## Editing instructions

- Overview: preserve the interface and redact only the full certificate fingerprint with a solid opaque rectangle labeled REDACTED. Do not blur or leave original characters visible.
- Devices: preserve controls, replace device names with Android device, and cover both complete UUID lines using solid opaque REDACTED labels.
- Android: preserve layout, icons and controls; translate the Turkish UI labels into English; cover all secondary window titles under the editor, browser and Spotify icons; remove the phone status bar information.

## Excluded originals

The panel popup contains a certificate fingerprint; the commands screenshot contains a personal home-directory path and a desktop notification. The mobile main canvas contains clipboard history, local URLs and a device model. The screen-sharing capture contains a personal media library and profile image. Those originals are intentionally not copied into the public gallery.

## Publication

AppStream image URLs target `resources/screenshots/` on the desktop repository's `main` branch. Push the reviewed images to that branch, or update all URLs to a public immutable ref containing them, before submission. Verify every URL after publishing; local validation with `--no-net` cannot confirm remote availability.

The Android app is a separate download at https://cepdesk.hasimozer.com. Its source and APK build instructions are at https://github.com/hasimzr/CepdeskMobilApp.
