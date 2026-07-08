# Unreleased

Working draft for the next PickGauge release. Keep this current while PRs land.
At release time, copy and polish it into the GitHub release description, then
reset this file.

## User-facing changes

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
- `bun run test`
- `sh -n scripts/install.sh`
- `git diff --check`

### Not tested yet

- App build.
- Updater flow.
- Windows and macOS bundles.

### Release blockers

- None known.
