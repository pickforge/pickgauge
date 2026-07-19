# Unreleased

Working draft for PickGauge v0.2.0. At release time, copy and polish this into
the GitHub release description, then reset this file.

## User-facing changes

- Official Codex and Claude quota readings are now resolved per service: when a service's CLI reading is unavailable, that service falls through to its opted-in managed-web reading without hiding healthy CLI readings or launching the browser unnecessarily.
- Added Claude's separate Fable weekly allowance to official usage readings.
- Reworked the dashboard into a compact Codex and Claude Code quota board with responsive meters and reduced-motion-aware transitions.
- Added a headless `pickgauge usage --json` command and agent skill for reading the latest local usage snapshot.
- Corrected Codex window labels so disabled, missing, or invalid primary windows are not shown as five-hour quota.
- Kept Settings saves and provider login work off the UI thread, preventing refreshes and browser launches from freezing the app.
- Fixed concurrent Settings and floating-button updates losing configuration changes.
- PickGauge now stops managed browser process trees when refreshing, resetting sessions, or quitting.
- Restored titlebar double-click maximize and consolidated titlebar actions through one event path.
- Fixed provider action alignment, removed blank wells from the Settings grid, and resized the floating capsule for two provider rings.
- Fixed the floating capsule glow so it fades smoothly instead of clipping at the window edge.

## Internal/release changes

- Switched AppImage libwayland post-processing to `pickforge-tauri-release fix-appimage`.
- Added Rust build caching to release CI and pinned CI to Bun 1.3.12.
- Kept tag-triggered releases as drafts after artifacts and `latest.json` are uploaded, preserving the manual publish gate.
- Added registered Grok CLI, Grok web, and Ollama provider adapters while keeping Grok and Ollama deferred from the current runtime and product surface.
- Registered usage providers are now executed directly through the provider module instead of being reconstructed and dispatched again by the engine.
- Added an `official_reading` module that owns per-service CLI-versus-managed-web precedence while keeping the adapters independently registered.
- Preserved available Claude weekly and Fable quotas when the session meter is unavailable while keeping fallback percentage labels fail-closed.
- Retained OAuth refresh tokens and expiry in memory only, using one shared HTTP client; PickGauge never writes credentials.
- Added SQLite daily-history aggregation and removed obsolete IPC commands.
- Tightened config serialization, browser sidecar lifecycle handling, and CLI snapshot persistence.
- Added a project run skill for normal development and isolated lab launches.
- Replaced duplicated repo workspace policy with a pointer to the canonical workspace instructions.

## Validation

### Tested

- Release workflow YAML parse check.
- `bun run lint`.
- `bun run check` (0 errors and 0 warnings).
- `bun run test` (71 frontend tests, 18 Playwright sidecar tests, and installer/sidecar checks).
- `bun run test:coverage` (frontend coverage ratchet passed).
- `bun run build`.
- `cargo check --manifest-path src-tauri/Cargo.toml`.
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --all-targets` (273 Rust tests).
- Filtered `usage::`, `cli_provider::`, `web_provider::`, and `official_reading::` test suites.
- `bun run test:synthetic-fail-closed` and `bun run test:official-fail-closed`.
- Browser preview checks for Codex and Claude Code across dashboard, Settings, and floating-capsule layouts.

### Not tested yet

- Packaged app build.
- Installer or updater flow.
- Platform smoke checks.

### Release blockers

- None known.
