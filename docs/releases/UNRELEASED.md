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
- Concentrated startup, manual, scheduled, targeted, and cache-clear refresh
  publication behind one lifecycle policy. Accepted display states now share
  one ordered snapshot-event, history, cache, cue, provider-error, and
  terminal-event chain; nonfatal effects remain best-effort, cache clearing
  remains emit-only, and shutdown rejects later publication.
- Refactored Claude JSONL and Codex SQLite local usage into one bounded source
  observation per provider. Live calibration and daily buckets now project
  from the same normalized records, while a desktop-owned brief reuse policy
  provides single-flight loading and expiry. Existing snapshot and
  headless-v1 payloads remain unchanged.

## Validation

### Tested

- `cargo test --locked --all-targets` (299 Rust tests).
- Filtered local-provider tests (27), observation-reuse tests (3), and
  refresh-publication policy tests (10).
- `cargo clippy --all-targets --locked` with strict warnings (only the four
  documented pre-existing lint classes allowed).
- Headless `usage --json` v1 golden fixture (byte-identical).
- `bun run test` (71 frontend tests plus install/sidecar suites) and `bun run
  check`.
- `bun run build`.

### Not yet tested

- Manual desktop smoke test of the dashboard, tray, and Settings.

### Known blockers

- None.
