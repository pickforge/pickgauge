# Unreleased

Working draft for the next PickGauge release. Keep this current while PRs land.
At release time, copy and polish it into the GitHub release description, then
reset this file.

## User-facing changes

- Expanded provider coverage to Grok and Ollama through safe local/official
  sources only (no browser cookie/profile import, no scraping):
  - **Grok** reads the Grok CLI bearer from `~/.grok/auth.json` and calls
    `GET grok.com/rest/subscriptions`. Truthfully plan-only today — active
    tier + optional billing-period end, with `remainingPercent`/`usedPercent`
    left null (`remainingPercentReason: grok_cli_plan_only`). Tokens are
    never refreshed or written back; sign in with the Grok CLI when they
    expire.
  - **Ollama** probes the local daemon (honors loopback `OLLAMA_HOST`,
    default `127.0.0.1:11434`; non-loopback hosts are rejected as
    `unsafe_path`). Reports running/installed/loaded model counts and an
    optional Cloud plan from `/api/me` when present. Ollama has no account
    quota percentage — gauges stay null (`quotaSupported: false`).
  - Settings toggles for both services; defaults on for fresh installs and
    missing keys. Headless `pickgauge usage --json` includes them under the
    existing schema v1 (additive rows only). Floating capsule and tray gauge
    still show percentage gauges only — healthy plan-only / availability-only
    readings (Grok CLI plan, Ollama daemon with no Cloud plan) stay on the
    dashboard and no longer pollute the tray rotation as empty rings.
- Fixed the floating capsule still appearing in KDE's Alt+Tab switcher when
  running under the `PICKGAUGE_X11=1` XWayland fallback. The KWin
  `skipswitcher` window rule now applies to that session the same way it
  already did for native Wayland; `ensure_float_rule` previously bailed out
  whenever `GDK_BACKEND=x11` was set, so the XWayland path never received
  any switcher exclusion (X11 has no standards-based Alt+Tab hint — only the
  KWin rule can hide a window from it). README documents the honest per-
  platform support boundaries: KDE Wayland/XWayland is fully handled;
  Windows gets Tauri's native `skip_taskbar`; other Linux window managers
  and macOS only get the standards-based/no-op hints Tauri provides, with no
  app-managed desktop configuration.
- Added a headless `pickgauge --version` command that prints the installed
  package version without starting the tray, GTK, or Tauri.
- Fixed unsaved Settings actions disappearing at compact window sizes. The
  save/discard controls now render at the app overlay layer instead of
  inside the scrolling Settings surface (whose entrance animation made it a
  `position: fixed` containing block), so they stay visible and unclipped at
  every supported window size. The header Save button is now visible and
  disabled while clean, and hidden without shifting layout while dirty;
  exactly one primary Save action is presented at a time. The dirty-state
  overlay shows the full unsaved-indicator/Discard/Save dock at wide widths
  and a labeled Save pill (discard remains reachable via the existing
  navigation-away guard) at compact widths, matching the app's existing
  700px sidebar breakpoint.

## Internal/release changes

- Added the shared `@pickforge/tauri-updater` dialog behind the `studioUpdateDialog`
  flag (default off; `@pickforge/flags`), part of
  pickforge/pickforge-platform#36. While off, the legacy `window.confirm`
  updater in `src/lib/updater.ts` is untouched. When on, `src/lib/updateDialog.ts`
  mounts the shared controller only on the visible main window: a Tauri
  window-label check excludes the floating capsule outright, and a hidden
  main window (tray/login-start) defers the check until its first focus
  event, mirroring the pre-existing `checkForUpdatesWhenVisible` deferral.
  One controller per process enforces a single check. A dev-only fixture
  (`?updateDialogFixture=available|downloading`, tree-shaken from production
  builds) stands in for a visual baseline since PickGauge has no VRT harness.
- Headless `usage --json` refreshes independent providers concurrently
  (join-all, emit in fixed service order) so an offline credentialed install
  pays ~max per-provider timeout instead of the sequential sum.
- Grok CLI transport gained injectable auth-path + subscriptions-URL seams so
  missing/malformed auth, 401, non-200, malformed body, and timeout map to the
  existing NotConfigured/ParseFailed/LoginRequired/NetworkUnavailable taxonomy
  under loopback mock coverage.
- Added real-binary headless CLI coverage, deterministic AppImage installer
  forwarding checks, and a Linux release gate that rejects broken headless
  commands after AppImage repair, including the human `usage` table header.
- Added a `usage_model` module that concentrates a service's validated quota
  windows, official status, plan, and headline selection into one typed
  model, replacing ad hoc `details`-bag re-parsing in the headless `usage
  --json` projection and the persisted snapshot cache. Headless-v1 JSON is
  schema-identical (golden test compares parsed Values); the persisted
  snapshot cache now stores the sanitized model instead of each provider's
  unrestricted `details` bag (cache version bumped, self-healing on next
  refresh).
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

- Issue #49 (KDE floating capsule stayed in the Alt+Tab switcher under the
  XWayland fallback): added `kwin::is_kde_wayland_session` characterization
  tests (native Wayland, compound `XDG_CURRENT_DESKTOP` values, the
  `GDK_BACKEND=x11` XWayland-fallback regression case, plain X11, missing
  session type, non-KDE Wayland compositors) and `group_has_key` scoping
  tests; `cargo test --manifest-path src-tauri/Cargo.toml --locked --lib
  kwin::` (7 passed). Full `cargo test --manifest-path src-tauri/Cargo.toml
  --locked --all-targets` run: 290 passed, the same 28 pre-existing failures
  as `origin/main` (macOS `/tmp` symlink and `/proc` process-marker
  environment artifacts, unrelated to this change — confirmed by running the
  same command against `origin/main` directly). `bun run test` (74 vitest +
  18 Node tests), `bun run check` (0 errors), `bun run lint` (clean), and
  `bun run build` all passed. `bun run smoke:kde-tray` requires a live KDE
  session with a registered StatusNotifier host and a built AppImage, so it
  was not run in this sandboxed environment; real KDE Alt+Tab validation is
  deferred to Elberte-PC.
- Issue #47 (unsaved Settings actions at compact sizes): `bun run check`,
  `bun run lint`, `bun run test` (74 frontend tests, including a new
  `settingsSaveDisplayState` characterization suite covering clean/dirty
  header-visibility and single-action rules), `bun run test:coverage`
  (ratchet holds), `bun run build`, and `bun run test:browser-preview`
  (extended with a headless-Chromium regression at the exact 937×747 and
  1344×951 repro sizes from the issue, asserting the save overlay stays
  fully inside the viewport bounds, plus a wide/compact content check for
  the Discard button and Save label). Confirmed this new smoke fails against
  the pre-fix code and passes against the fix.
- Earlier unreleased work: `cargo test --locked --all-targets` (299 Rust
  tests); filtered local-provider tests (27), observation-reuse tests (3), and
  refresh-publication policy tests (10); strict `cargo clippy`; headless JSON
  golden fixture; `bun run test`, `bun run check`, and `bun run build`.
- Focused Rust headless CLI tests (4 unit tests and 1 real-binary integration
  test with display and user configuration paths isolated), including bare
  human `usage` output.
- `node tests/install-script-smoke.mjs` (4 deterministic installer tests,
  including bare `usage` forwarding).
- Headless AppImage validator smoke tests against deterministic executable
  fixtures, covering valid human/JSON output, an invalid human header, GTK
  panic-like stderr, malformed JSON, and an invalid usage schema.
- `bun run test`, `bun run check`, and `bun run lint`; Node syntax checks for
  the headless scripts; and `git diff --check` passed for this coverage update.
- Earlier validation also included `rustfmt --check` on the new integration
  test and a formatter comparison confirming only restored pre-existing
  formatting differs in `usage_cli.rs`.
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
- Release workflow run `29970550204` passed at exact candidate SHA `a68859a`:
  the repaired Linux AppImage headless gate passed, as did the Windows, macOS
  Intel, and macOS Apple Silicon build jobs. The exact Linux artifact then
  passed direct and installed-wrapper `--version`, bare `usage`, and `usage
  --json` checks on Elberte-PC with empty stderr and no GTK/panic markers.
  Before/after process attribution confirmed that the headless commands left
  no new process; the two observed PickGauge-named processes predated the
  candidate by several days. The validated candidate remains installed with a
  checksum-verified rollback backup.

- pickforge/pickforge-platform#36 (PR 5, PickGauge integration): `bun run
  lint`, `bun run check`, `bun run test` (85 vitest tests including
  `flags.test.ts` and `updateDialog.test.ts`, covering flag-off default,
  eligibility for an already-visible main window, capsule exclusion by
  window label, and hidden-main-window deferral until focus), and `bun run
  test:coverage` (ratchet holds). No Rust touched.

### Not yet tested

- pickforge/pickforge-platform#36 (PR 5): owner-gated packaged-update smoke
  (old build to staged newer build) and design-lead visual acceptance from
  the dev-only fixture; both deferred to a later PR per the issue's PR plan.
- Issue #49: real KDE Wayland/XWayland Alt+Tab, taskbar, and pager behavior
  for the float capsule — deferred to Elberte-PC, a live KDE session with
  `qdbus`/`kwriteconfig6` available.
- Manual desktop smoke test of the dashboard, tray, and Settings.
- An unfiltered full Rust suite on macOS; the two `/proc` process-marker
  failures above remain unresolved and are unchanged from the base commit.

### Known blockers

- None for issue #53. The macOS-only baseline test limitation above remains
  tracked separately from the Linux headless CLI regression.
