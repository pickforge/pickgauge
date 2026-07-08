# Unreleased

Working draft for the next PickGauge release. Keep this current while PRs land.
At release time, copy and polish it into the GitHub release description, then
reset this file.

## User-facing changes

- Adopted the Pickforge Studio shared chrome: the main window is now frameless
  with a single 38px titlebar (brand mark + PickGauge wordmark on the left,
  watching/syncing status pill and window controls on the right). This removes
  the doubled-up native frame + decorative header that showed odd borders.
- Status indicators switched to the bracket motif: the titlebar pill and float
  capsule use a hairline ring pulse instead of a glowing blob, section labels
  gain a bracket tick, and the unsaved-settings marker is a small ember bracket.
- Unified 24px status bar: transient status on the left, `© Pickforge ·
  pickforge.dev · MIT` on the right.
- Linux curl installs now use a rootless AppImage wrapper that falls back on
  FUSE3-only systems and installs a launcher icon/menu entry.
- Fixed the white/blank window on distros with recent Mesa/Wayland (Arch,
  CachyOS, Fedora): the AppImage no longer bundles the build host's
  libwayland, which crashed WebKit's EGL setup (#15).
- Added anonymous crash and error reporting with a Settings → Crash reports
  opt-out. Reports are disabled in development builds unless explicitly enabled.

## Internal/release changes

- Added repo-local release tracking in `docs/releases/UNRELEASED.md`.
- Linux release CI now strips bundled `libwayland-*` from the AppImage,
  repacks it, and re-signs the updater artifact.
- Release CI uploads Rust debug symbols and frontend sourcemaps to Sentry when
  `SENTRY_AUTH_TOKEN` is configured.
- Sentry events strip hostnames and breadcrumbs before upload.
- Added installer smoke tests for AppImage desktop integration and symlink-safe
  upgrades.
- Renamed brand assets to the canonical `pickgauge-*` scheme.

## Validation

### Tested

- Reviewed the release tracking docs.
- `bun run test:installer`
- `bun run check`
- `bun run test` (incl. coverage thresholds via `test:coverage`)
- `bun run lint`
- `bun run build`
- `cargo check --workspace --all-targets` from `src-tauri/`
- `cargo test --workspace --locked --all-targets` from `src-tauri/`
- `sh -n scripts/install.sh`
- `git diff --check`
- Live run on an isolated Linux X11 display: frameless main window renders
  without the doubled border, titlebar + WATCHING pill draw correctly,
  dashboard/history navigate, float capsule opens the main window.

### Not tested yet

- Frameless drag/resize on Wayland-native compositors, and window-control
  order on Windows/macOS.
- App build.
- Updater flow.
- Windows and macOS bundles.

### Release blockers

- None known.
