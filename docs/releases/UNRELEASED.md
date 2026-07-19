# Unreleased

Working draft for the next PickGauge release. Keep this current while PRs land.
At release time, copy and polish it into the GitHub release description, then
reset this file.

## User-facing changes

- Added Claude's separate Fable weekly allowance to official usage readings.
- Reworked the dashboard into a compact Codex and Claude Code quota board with responsive meters and reduced-motion-aware transitions.
- Corrected Codex window labels so disabled, missing, or invalid primary windows are not shown as five-hour quota.
- Settings save and supported provider login work now run off the UI thread, keeping the app responsive while refreshes and browser launches continue.
- Fixed concurrent Settings and floating-button updates losing configuration changes.
- PickGauge now stops managed browser process trees when refreshing, resetting sessions, or quitting.
- Fixed provider action alignment, removed the Settings grid's blank wells, and resized the floating capsule for two provider rings.

- The float capsule's glow now fades out smoothly instead of being clipped
  into a hard rectangle by the window edge; the transparent margin around
  the capsule is click-through (#38).

## Internal/release changes

- Switched AppImage libwayland post-processing to `pickforge-tauri-release fix-appimage`.
- Release CI now caches Rust builds (`Swatinem/rust-cache`).
- Managed browser profiles and web refreshes are restricted to Codex and Claude Code.
- Grok and Ollama are deferred from the runtime and product surface; their browser automation, harvested-session HTTP requests, and managed profile actions remain removed.
- Claude web reads preserve available weekly and Fable quotas when the session meter is unavailable, while keeping fallback percentage labels fail-closed.
- OAuth refresh tokens and expiry are retained in memory only, with one shared HTTP client; credentials are never written by PickGauge.
- Registered usage providers are retained and executed directly through the provider module instead of being reconstructed and dispatched again by the engine.
- Daily history statistics are aggregated in SQLite, and obsolete uncalled IPC commands were removed.

## Validation

### Tested

- Workflow YAML parse check:
  `python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/release.yml'))"`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --all-targets` (261 Rust tests)
- Filtered provider execution, config serialization, browser process-tree, OAuth cache, and SQLite aggregation tests.
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets` with strict warnings after allowing four pre-existing lint classes.
- `bun run build`
- `bun run check`
- `bun run test` (71 frontend tests and 18 Playwright sidecar tests)
- `bun run test:browser-preview` (Codex and Claude Code across 1000px, 820px, 680px, and 390px widths, Settings column breakpoints, and the 168×56 two-ring capsule)
- Browser-rendered visual checks at 1000×700 and 820×600, including the compact dashboard, Settings layout, and exact floating-capsule geometry.

### Not tested yet

- App build.
- Installer or updater flow.
- Platform smoke checks.
- `cargo fmt --check` (`rustfmt` is not installed in the current toolchain).

### Release blockers

- None known.
