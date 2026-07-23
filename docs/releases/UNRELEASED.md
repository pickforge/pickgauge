# Unreleased

Working draft for the next PickGauge release. Keep this current while PRs land.
At release time, copy and polish it into the GitHub release description, then
reset this file.

## User-facing changes

- Added a headless `pickgauge --version` command that prints the installed
  package version without starting the tray, GTK, or Tauri.

## Internal/release changes

- Added real-binary headless CLI coverage, deterministic AppImage installer
  forwarding checks, and a Linux release gate that rejects broken headless
  commands after AppImage repair.
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

- Earlier unreleased work: `cargo test --locked --all-targets` (299 Rust
  tests); filtered local-provider tests (27), observation-reuse tests (3), and
  refresh-publication policy tests (10); strict `cargo clippy`; headless JSON
  golden fixture; `bun run test`, `bun run check`, and `bun run build`.
- Focused Rust headless CLI tests (4 unit tests and 1 real-binary integration
  test with display and user configuration paths isolated).
- `node tests/install-script-smoke.mjs` (4 deterministic installer tests).
- Headless AppImage validator smoke tests against deterministic executable
  fixtures, covering valid output, GTK panic-like stderr, malformed JSON, and
  an invalid usage schema.
- Node syntax checks, `rustfmt --check` on the new integration test, a
  formatter comparison confirming only restored pre-existing formatting differs
  in `usage_cli.rs`, and `git diff --check`.
- Backpressure regression comparison with a physical worktree-local `TMPDIR`:
  `stderr_backpressure_does_not_block_a_valid_response` passed 1/5 current-worktree
  runs (pass 51.763s; failures 1.526s, 1.287s, 1.303s, 1.305s) and 2/5
  clean-base `e31d074` runs (passes 41.085s, 3.081s; failures 1.318s, 1.296s,
  1.294s). The earlier timeout is
  therefore classified as a non-reproduced baseline-adjacent flake rather than
  a branch regression.
- Filtered full Rust suite passed in 1.853s: 309 library tests plus the
  real-binary integration test, with only
  `startup_detects_and_stops_orphaned_process_from_registry` and
  `startup_ignores_legacy_deferred_browser_sessions` skipped. The backpressure
  test was not skipped and passed in this run. The two skipped tests remain
  proven unchanged at base commit `e31d074` on macOS due `/proc` process-marker
  visibility.

### Not yet tested

- Repaired packaged Linux AppImage headless gate in the release workflow.
- Installed AppImage verification on Elberte-PC.
- Manual desktop smoke test of the dashboard, tray, and Settings.
- An unfiltered full Rust suite; the two macOS `/proc` process-marker failures
  above remain unresolved.

### Known blockers

- Required packaged AppImage and installed Elberte-PC validation is still
  pending. The unfiltered full Rust suite is also not clean because of the two
  macOS process-marker failures above.
