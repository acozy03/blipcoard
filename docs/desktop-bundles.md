# Desktop Bundle Notes

Phase 9.2 packages the desktop app as a complete runtime bundle. Desktop
installers must include the daemon and CLI so users never install a GUI that
bypasses `blipd`.

## Bundle Shape

The desktop bundle is a Tauri app under `apps/desktop/src-tauri`.

- `apps/desktop/src-tauri/tauri.conf.json` enables Tauri bundling for all
  supported desktop targets.
- `bundle.externalBin` includes `binaries/blipd` and `binaries/blip`.
- `npm run bundle:prepare-sidecars` builds the Rust `blipd` and `blip` binaries
  and copies them to Tauri's target-triple sidecar filenames.
- `npm run bundle` prepares sidecars and runs `tauri build`.
- `npm run bundle:check` runs the frontend bridge tests, builds the frontend,
  prepares sidecars, and type-checks the Tauri shell.

The Tauri frontend uses command handlers in the Rust shell. Those handlers load
the shared `blipcoard` config, talk to the configured daemon IPC socket, and try
to start the bundled `blipd --ipc-only` sidecar when the socket is missing or
stale. The desktop shell does not watch the clipboard directly.

Current limit: Unix socket IPC is implemented for macOS and Linux. Windows
bundles can be configured, but Windows runtime acceptance remains blocked until
the daemon and desktop bridge use a current-user named pipe or equivalent
Windows IPC.

## macOS

Required bundle contents:

- `blipcoard.app`
- bundled `blipd` sidecar
- bundled `blip` sidecar

Signing and notarization requirements:

- Sign the app bundle, Tauri executable, and sidecar binaries with a Developer
  ID Application certificate.
- Keep hardened runtime enabled.
- Submit signed artifacts for Apple notarization before distribution outside the
  Mac App Store.
- Preserve `blipd` and `blip` executable permissions inside the app bundle.

Smoke checks:

- Install the app on a clean user account.
- Launch the desktop app and confirm it starts or connects to `blipd`.
- Run `blip health` from the installed CLI and confirm it reaches the same
  daemon/store.
- Copy text and verify the desktop inbox updates through daemon ingestion.
- Quit and relaunch the app; confirm it reconnects without creating a second
  daemon instance.

## Linux

Expected bundle formats:

- AppImage for portable desktop installs.
- Debian package for apt-based distributions.
- RPM package for rpm-based distributions.

Linux release hosts need the native packaging tools for the target formats they
build. A Debian-only smoke can be run with:

```sh
npm run bundle:prepare-sidecars
npx tauri build --debug --bundles deb
```

RPM packaging requires `rpmbuild`/`rpm`; AppImage packaging requires the
AppImage tooling used by Tauri on the release host. If those tools are missing,
build explicit supported formats instead of relying on `targets = "all"`.

Required bundle contents:

- desktop launcher entry for `blipcoard`
- bundled `blipd` sidecar
- bundled `blip` sidecar
- package metadata declaring WebKit/GTK runtime dependencies required by Tauri

Smoke checks:

- Install or run the bundle on a clean Linux user account.
- Launch the desktop app under both X11 and Wayland where available.
- Confirm the desktop app starts or connects to `blipd` over the configured Unix
  socket.
- Run `blip health` from the installed CLI and confirm it reaches the same
  daemon/store.
- Copy text and verify the desktop inbox updates through daemon ingestion.
- Confirm uninstall removes package files but preserves user config, database,
  and blob data unless a documented purge path is used.

## Windows

Expected installer formats:

- NSIS installer for general distribution.
- MSI only if enterprise deployment requires it.

Installer and update expectations:

- Install `blipcoard`, `blipd`, and `blip` together.
- Add the CLI install location to the user PATH or document the exact shell
  profile step.
- Use signed installer artifacts before public distribution.
- Use signed update metadata if the Tauri updater is enabled; unsigned updates
  must not be accepted.
- Replace sidecar binaries during upgrades so `blipcoard`, `blipd`, and `blip`
  versions stay aligned.

Current blocker:

- The daemon IPC layer currently reports Windows IPC as unavailable. Windows
  desktop bundle smoke checks cannot pass until named-pipe IPC and desktop
  bridge support are implemented.

Planned smoke checks after Windows IPC lands:

- Install through the Windows installer on a clean user account.
- Launch the desktop app and confirm it starts or connects to `blipd`.
- Run `blip health` from PowerShell and confirm it reaches the same
  daemon/store.
- Copy text and verify the desktop inbox updates through daemon ingestion.
- Upgrade over an older install and verify the sidecar versions match the app
  version.

## Release Checklist

- Run `npm run bundle:check` from `apps/desktop`.
- Run `npm run bundle` on each release platform.
- For Linux release packaging, confirm `rpmbuild`/`rpm` and AppImage tooling are
  installed before building those formats.
- Inspect bundle contents and confirm `blipd` and `blip` are included.
- Confirm the desktop shell connects to the configured daemon socket.
- Confirm launching desktop starts the bundled daemon when no daemon is running.
- Confirm the CLI uses the same config and store as the desktop app.
- Confirm uninstall behavior preserves config, SQLite, and `blobs/` by default.
- Record platform, architecture, package format, signing state, and smoke-check
  result in release notes.
