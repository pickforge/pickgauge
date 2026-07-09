# Unreleased

Working draft for the next PickGauge release. Keep this current while PRs land.
At release time, copy and polish it into the GitHub release description, then
reset this file.

## User-facing changes

- Added zero-setup Grok plan detection through the local Grok CLI login. PickGauge shows the active
  plan and billing-period end without reporting a usage percentage.
- Added zero-setup Ollama plan detection from the signed-in local daemon. Usage limits remain unavailable.

## Internal/release changes

- Switched AppImage libwayland post-processing to `pickforge-tauri-release fix-appimage`.
- Release CI now caches Rust builds (`Swatinem/rust-cache`).
- Grok reads its CLI bearer without refreshing, storing, or writing it.

## Validation

### Tested

- Workflow YAML parse check:
  `python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/release.yml'))"`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --all-targets`
- `bun run check`
- `bun run test`

### Not tested yet

- App build.
- Installer or updater flow.
- Platform smoke checks.

### Release blockers

- None known.
