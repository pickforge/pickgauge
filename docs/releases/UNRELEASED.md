# Unreleased

Working draft for the next PickGauge release. Keep this current while PRs land.
At release time, copy and polish it into the GitHub release description, then
reset this file.

## User-facing changes

- None yet.

## Internal/release changes

- Switched AppImage libwayland post-processing to `pickforge-tauri-release fix-appimage`.

## Validation

### Tested

- Workflow YAML parse check:
  `python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/release.yml'))"`

### Not tested yet

- App build.
- Installer or updater flow.
- Platform smoke checks.

### Release blockers

- None known.
