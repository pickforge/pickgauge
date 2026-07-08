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

## Internal/release changes

- Added repo-local release tracking in `docs/releases/UNRELEASED.md`.
- Linux release CI now strips bundled `libwayland-*` from the AppImage,
  repacks it, and re-signs the updater artifact.
- Added installer smoke tests for AppImage desktop integration and symlink-safe
  upgrades.

## Validation

### Tested

- Reviewed the release tracking docs.
- `bun run test:installer`
- `bun run check`
- `bun run test` (incl. coverage thresholds via `test:coverage`)
- `bun run lint`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --all-targets`
- `sh -n scripts/install.sh`
- `git diff --check`

### Not tested yet

- Live app run of the frameless titlebar (window controls, drag, resize
  handles, close-to-tray) on Linux/Windows/macOS.
- App build.
- Updater flow.
- Windows and macOS bundles.

### Release blockers

- None known.
