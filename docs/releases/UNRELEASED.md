# Unreleased

Working draft for the next PickGauge release. Keep this current while PRs land.
At release time, copy and polish it into the GitHub release description, then
reset this file.

## User-facing changes

(none yet)

## Internal/release changes

- Added a `usage_model` module that concentrates a service's validated quota
  windows, official status, plan, and headline selection into one typed
  model, replacing ad hoc `details`-bag re-parsing in the headless `usage
  --json` projection and the persisted snapshot cache. Headless-v1 JSON is
  unchanged (golden test stays byte-identical); the persisted snapshot cache
  now stores the sanitized model instead of each provider's unrestricted
  `details` bag (cache version bumped, self-healing on next refresh).

## Validation

### Tested

- `cargo test --locked --all-targets` (281 Rust tests).
- Filtered usage-focused Rust tests (96).
- `cargo clippy --all-targets --locked` (only the four documented
  pre-existing lint classes).
- Headless `usage --json` v1 golden fixture (byte-identical).
- `bun run test` (frontend/unit/sidecar suites) and `bun run check`.
- `bun run build`.

### Not yet tested

- Manual desktop smoke test of the dashboard, tray, and Settings.

### Known blockers

- None.
