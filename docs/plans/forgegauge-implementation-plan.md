# ForgeGauge Implementation Plan

## Status

This is the canonical consolidated plan for ForgeGauge. It merges the previous product spec and phased implementation plan into one checklist-driven document.

Current app state: **early Tauri/Svelte MVP with fake usage data, persisted settings, branded tray wiring, app icons, AppImage build support, and release workflow scaffolding.**

Last readiness review: source checked against this plan after consolidation. The plan is now intended to be executable as the active backlog, with unchecked items representing the remaining implementation work.

Latest progress, 2026-06-03: completed the Phase 4 core data plumbing milestone for the fake-provider path. Validation passed with `npm run check`, `npm run build`, `cargo fmt --check`, `cargo check`, `cargo clippy -- -D warnings`, `cargo test`, and `npm run build:appimage`. Browser preview smoke checks covered desktop and mobile layouts, settings checkbox interaction, and overflow checks via Playwright; full KDE/Wayland tray smoke testing remains unchecked.

Tray/window progress, 2026-06-03: decided on tray-first startup for normal runtime, configured the Tauri `main` window to start hidden, added close-to-tray handling for normal window close requests, kept the tray menu `Quit` action as the explicit full-exit path, and rebuilt the AppImage successfully. Full KDE/Wayland tray visibility, tray-click, close-button, and quit-behavior confirmation remains unchecked.

Config progress, 2026-06-03: added a raw JSON config load boundary, default filling before typed deserialization, future-version rejection, atomic temp-file/fsync/rename persistence, restrictive config-file permissions on Unix, startup config-error surfacing, a manual web-refresh cooldown settings control, `v1 -> v2` migration support, browser profile root/override config fields, browser profile path UI controls, browser profile path validation with ownership markers, and path-level tests for missing/current/partial/malformed/future configs, write-failure preservation, failed migration rollback, web-provider opt-out, interval/cooldown clamping, v1 migration, and safe/unsafe browser profile paths. Validation passed with `npm run check`, `npm run build`, `cargo fmt --check`, `cargo check`, `cargo clippy -- -D warnings`, `cargo test`, and `npm run build:appimage`. Playwright browser-preview checks covered desktop/mobile settings layout and overflow after the browser profile controls were added. Manual quota/window configuration remains unchecked.

Local provider discovery progress, 2026-06-03: completed privacy-limited read-only shape discovery for local Claude Code and Codex roots and recorded sanitized findings in `docs/discovery/local-provider-data-shapes.md`. Discovery covered file locations, aggregate counts, JSON keys, SQLite schemas, candidate source precedence, machine-local scope, fixture strategy, and safe metadata boundaries without committing raw local records or authenticated data. Parser implementation, scan limits, calibration schema, and real local snapshots remain unchecked.

Claude local provider progress, 2026-06-03: added an injectable Claude local data root and a synthetic-fixture JSONL parser for `~/.claude/projects/**/*.jsonl`. The provider emits local low-confidence snapshots with token/cache/session/model counts and `remaining_percent = None` when uncalibrated, and emits sanitized unknown snapshots for missing project data or invalid records. Registry wiring, scan limits, calibration, and full local-provider completion remain unchecked.

Supersedes:

- `docs/specs/codex-claude-usage-tray-spec.md`
- `docs/plans/codex-claude-usage-tray-implementation-plan.md`

## Goal

Build a personal CachyOS KDE/Linux tray app that displays remaining usage for:

- Codex: <https://chatgpt.com/codex/cloud/settings/analytics>
- Claude Code: <https://claude.ai/new#settings/usage>

The app combines local CLI-derived estimates with opt-in browser-based readings from official usage pages. It must be privacy-conscious, explicit about confidence, and useful even when local files or web parsing are unavailable.

## Success Criteria

- [x] A Tauri/Svelte desktop app scaffold exists.
- [x] A persistent tray icon is wired.
- [x] The tray alternates between Codex and Claude states on a configurable interval.
- [x] A compact popup shows both services with remaining percentage, source, confidence, and last update.
- [x] Basic settings persist locally.
- [x] Settings allow enabling/disabling services and provider classes.
- [x] Web providers are disabled by default.
- [x] AppImage bundling works locally on CachyOS/Arch-like systems through `npm run build:appimage`.
- [ ] Real local usage providers work without account credentials.
- [ ] Opt-in web providers use dedicated browser profiles and never store passwords.
- [ ] Provider failures degrade to `unknown` or lower-confidence estimates instead of crashing.
- [ ] Merged usage values combine official web baselines with calibrated local deltas.
- [ ] Full KDE/Wayland tray smoke test is confirmed by the user.
- [ ] Remote release workflow is verified after a mainline push.

## Constraints

- No password storage.
- No CAPTCHA bypass.
- No default website scraping.
- No assumption that Codex or Claude expose stable private APIs.
- Web provider parsing must rely on visible UI/state and be treated as best-effort.
- Local provider data must be labeled as estimated unless proven exact.
- The app must not upload usage, session, account, auth, or profile data.
- Provider details, logs, fixtures, and events must be sanitized before display or persistence.

## Current Feature Status

### Implemented

- [x] Initial repository docs and product plan created.
- [x] MIT license added for Pickforge.
- [x] Tauri v2 + Svelte app scaffold created.
- [x] Rust backend and Svelte frontend wiring added.
- [x] Tauri capabilities file added.
- [x] ForgeGauge product naming, bundle identifier, and app metadata configured.
- [x] `assets/branding/` added with app icon, logo, lockups, tray icons, hero, social card, palette, favicon, and pattern assets.
- [x] Platform app icons generated from `assets/branding/app-icon.svg`.
- [x] Branded popup UI added with fake Codex and Claude Code snapshots.
- [x] Branded tray icon rotation added for Codex and Claude Code.
- [x] Low-usage and unknown tray icon assets wired for provider states.
- [x] Local persisted app config added under the app config directory.
- [x] Settings UI added for service toggles, provider toggles, local/web refresh intervals, tray switch interval, and low-usage threshold.
- [x] Settings UI added for manual web-refresh cooldown.
- [x] Config normalization clamps local refresh, web refresh, manual web cooldown, tray switch interval, and low-usage threshold.
- [x] Web providers default to disabled.
- [x] Fake usage snapshot command added and driven by enabled-service settings.
- [x] Tray tooltip reflects the active service and remaining percentage.
- [x] Central `UsageEngine` added with a fake provider registry, latest snapshot cache, shared display-state cache, and snapshot update event.
- [x] Tray rotation and frontend snapshot commands now read from the same display-state cache.
- [x] AppImage build fixed on CachyOS/Arch-like systems with `NO_STRIP=1`.
- [x] `npm run build:appimage` added.
- [x] GitHub Actions release workflow added for queued Linux AppImage, Windows, macOS Intel, and macOS Apple Silicon artifacts.
- [x] Release notes include Windows/macOS untested caveats and invite reports/issues/PRs.
- [x] README updated with branding, Tauri rationale, release support, AppImage workaround, and current MVP behavior.
- [x] AppImage built successfully locally at `src-tauri/target/release/bundle/appimage/ForgeGauge_0.1.0_amd64.AppImage`.
- [x] AppImage launched manually once from the generated artifact.

### Verified Previously

- [x] `npm run check`
- [x] `npm run build`
- [x] `cargo fmt --check`
- [x] `cargo check`
- [x] `cargo clippy -- -D warnings`
- [x] `cargo test`
- [x] `npm run build:appimage`

### Not Fully Implemented

- [ ] Remaining tray-first window lifecycle polish: tray-relative popup behavior where practical, popup dismissal fallback, and KDE/Wayland confirmation.
- [ ] Full KDE/Wayland smoke test for tray visibility, popup open/close, settings persistence after restart, and quit behavior.
- [ ] GitHub release workflow remote/mainline run verification.
- [ ] Windows artifact testing.
- [ ] macOS Intel artifact testing.
- [ ] macOS Apple Silicon artifact testing.
- [ ] Claude Code local provider.
- [ ] Codex local provider.
- [ ] Provider registry with scheduled refresh, backoff, and event streaming.
- [x] Shared display-state cache used by both tray rotation and frontend snapshots.
- [x] Snapshot cache for latest provider results.
- [ ] Calibrated local quota/window estimates.
- [x] Config migration and atomic persistence layer.
- [x] Browser profile path configuration, validation, and ownership markers.
- [ ] Browser profile cleanup guardrails.
- [ ] Browser automation spike for official usage pages.
- [ ] Isolated browser session manager.
- [ ] Opt-in Codex web provider.
- [ ] Opt-in Claude web provider.
- [ ] Merge engine for web baselines plus local deltas.
- [ ] Autostart setting.
- [ ] Clear/delete actions for cached snapshots.
- [ ] Clear/delete actions for browser session data.
- [ ] Basic failure logging view or log file location.

## Core Concept

ForgeGauge uses two data sources and one display pipeline:

1. Local providers
   - Read local Claude Code and Codex CLI/session data.
   - Run frequently.
   - Require no account credentials.
   - Are marked estimated unless calibrated against a known plan/window.

2. Experimental web providers
   - Disabled by default.
   - Use isolated app-owned browser profiles.
   - Require manual user login.
   - Read visible usage data from official pages.
   - Run less frequently and fail closed on login/MFA/CAPTCHA/unexpected UI.

3. Usage merger
   - Uses web readings as official baselines.
   - Applies calibrated local usage deltas after the baseline.
   - Surfaces source and confidence clearly.

## Implementation Readiness Notes

- Current source has a **central `UsageEngine` with a fake provider registry, latest snapshot cache, shared display-state cache, and snapshot update event**; real providers are not implemented yet.
- Current tray values read from the shared display-state cache instead of hard-coded fake values in `src-tauri/src/lib.rs`.
- Current app window starts hidden from `src-tauri/tauri.conf.json`, can be shown from tray/menu actions, and hides back to tray on normal close requests; Phase 0.5/10 must still confirm KDE/Wayland tray behavior manually.
- Current Rust dependencies are intentionally minimal: `serde`, `serde_json`, and `tauri`. Any async runtime, time, logging, filesystem walking, browser automation, opener, or path-dialog dependencies must be added deliberately with validation.
- Current Tauri permissions are only `core:default`; browser/session/open-url/path features will require explicit capability review before implementation.
- Current CSP is `null`; web-provider UI and any opener/browser integration must include a security review before release.

## Implementation Sequence

Build in this order to avoid rework:

1. **Tray/window hardening**
   - [x] Decide tray-first window lifecycle.
   - [ ] Validate KDE/Wayland tray and popup behavior.
   - [ ] Add fallback close/dismiss behavior if focus-loss dismissal is unreliable.

2. **Core data plumbing**
   - [x] Introduce shared Rust/TypeScript app models.
   - [x] Introduce `UsageEngine` Tauri state.
   - [x] Introduce provider registry, fake provider, scheduler, and shared display-state cache.
   - [x] Wire tray and frontend to the same display state.

3. **Persistence hardening**
- [x] Add config migrations, atomic writes, rollback, and tests.
   - [x] Add browser profile config fields only after migration support exists.
   - [ ] Add quota/window config fields only after migration support exists.

4. **Local providers**
   - [ ] Discover Claude and Codex local data formats.
   - [ ] Build parsers with sanitized fixtures.
   - [ ] Return honest `unknown` or uncalibrated snapshots before adding calibrated percentages.
   - [ ] Add manual calibration config and percent-delta support.

5. **Browser automation and web providers**
   - [ ] Complete automation spike and backend decision matrix.
   - [ ] Implement isolated session manager.
   - [ ] Implement opt-in web providers and parser contracts.

6. **Merge and release readiness**
   - [ ] Implement merge engine.
   - [ ] Complete KDE smoke test.
   - [ ] Verify remote release workflow and platform artifacts.

## Definition of Done for Each Phase

Each phase is complete only when:

- [ ] Source implementation is finished.
- [ ] Unit tests for new logic are added where practical.
- [ ] Frontend and backend model changes are kept in sync.
- [ ] Security/privacy constraints relevant to that phase are reviewed.
- [ ] `npm run check` passes.
- [ ] Rust validators pass when Rust code changed.
- [ ] Manual smoke tests listed for the phase are completed or explicitly deferred with reason.
- [ ] This plan's checkboxes are updated to match the implementation state.

## Target Architecture

```text
src-tauri/
├─ capabilities/
│  └─ default.json
├─ tray/
│  ├─ icon_renderer.rs
│  ├─ tray_controller.rs
│  └─ popup_controller.rs
├─ usage/
│  ├─ model.rs
│  ├─ engine.rs
│  ├─ merger.rs
│  ├─ providers/
│  │  ├─ fake.rs
│  │  ├─ claude_local.rs
│  │  ├─ codex_local.rs
│  │  ├─ claude_web.rs
│  │  └─ codex_web.rs
│  └─ scheduler.rs
├─ browser/
│  ├─ session.rs
│  ├─ login_flow.rs
│  └─ scraper.rs
├─ config/
│  ├─ model.rs
│  ├─ migrations.rs
│  └─ store.rs
└─ commands/
   ├─ usage_commands.rs
   └─ settings_commands.rs

src/
├─ App.svelte
├─ lib/
│  ├─ usage.ts
│  ├─ settings.ts
│  └─ formatting.ts
└─ routes/
   ├─ Popup.svelte
   ├─ Settings.svelte
   └─ LoginRequired.svelte
```

The current implementation starts smaller with `config.rs`, `usage.rs`, `lib.rs`, `App.svelte`, and `src/lib/usage.ts`. Split modules when real providers and scheduling make it worthwhile.

## Source-of-Truth Rules

- Usage display state must have one backend source of truth.
  - [x] Tray tooltip/icon selection reads the same display state as the popup.
  - [x] Frontend snapshots are returned from the shared display cache, not recomputed separately.
  - [x] Fake provider remains a provider implementation, not special-case UI state.
- Frontend and backend models must evolve together.
  - [ ] Rust enum/string serialization is documented.
  - [x] TypeScript types mirror the IPC payloads.
  - [x] Any new field has a default, migration path, and UI fallback.
- User-facing precision must be justified.
  - [ ] Show percentages only for official web values or calibrated local estimates.
  - [ ] Show token/cost/activity summaries without percentages when local data cannot be mapped to plan limits.
  - [ ] Never derive account-wide remaining usage from machine-local logs unless clearly labeled as partial.

## Config Migration and Persistence Contract

Config changes must be implemented before adding browser profile paths, quota/window settings, autostart, or other persisted fields.

- [x] Keep `version` as a monotonic integer.
- [x] Parse raw JSON first so older configs can migrate before typed `AppConfig` deserialization.
- [x] Migrate sequentially: `v1 -> v2 -> v3`, never by skipping unknown versions.
- [x] Reject future config versions with a recoverable UI error.
- [x] Fill defaults for newly introduced fields during migration.
- [x] Preserve the previous config file if migration fails.
- [x] Write atomically through a temporary file and rename.
- [x] Avoid partially written config files on crash where practical.
- [x] Use restrictive file permissions for config and profile marker files where supported.
- [x] Add tests for:
  - [x] missing config file
  - [x] current config round trip
  - [x] old config migration
  - [x] malformed config
  - [x] failed migration rollback
  - [x] future version rejection
  - [x] web providers disabled by default after migration

## Data Model

```rust
enum Service {
    Codex,
    Claude,
}

enum UsageSource {
    Local,
    Web,
    Merged,
}

enum UsageConfidence {
    High,
    Medium,
    Low,
    Unknown,
}

struct UsageSnapshot {
    service: Service,
    remaining_percent: Option<f32>,
    used_percent: Option<f32>,
    reset_at: Option<DateTime<Utc>>,
    source: UsageSource,
    confidence: UsageConfidence,
    last_updated: DateTime<Utc>,
    details: serde_json::Value,
}
```

`details` must contain sanitized metadata only, such as provider status codes, parse-field presence, baseline timestamps, stale age, and local delta metadata. It must not contain raw page text/HTML, account identifiers, cookies, tokens, auth headers, or unsanitized browser errors.

Current frontend and backend models include a temporary `fake` source while real providers are not implemented.

### Model Readiness Checklist

- [x] Define canonical Rust `Service`, `UsageSource`, `UsageConfidence`, and `UsageSnapshot` modules.
- [x] Define matching TypeScript types in `src/lib/usage.ts`.
- [x] Use RFC3339 timestamps for machine-readable `last_updated` values.
- [x] Add separate human-readable formatting in the frontend instead of storing display strings in backend models.
- [x] Add provider identifier fields such as `provider_id`, `source`, and sanitized `status`.
- [ ] Add stale metadata: `stale`, `stale_seconds`, `baseline_at`, and `last_official_check_at` where relevant.
- [x] Add error metadata as stable codes, not raw error strings.
- [x] Add serialization tests for expected IPC JSON payload shapes.

## Provider Contract

Each provider should implement the same conceptual contract:

```rust
trait UsageProvider {
    fn service(&self) -> Service;
    fn source(&self) -> UsageSource;
    async fn refresh(&self) -> Result<UsageSnapshot, UsageProviderError>;
}
```

Provider failures should become user-safe states:

- not configured
- login required
- unavailable
- parse failed
- stale data

Local providers that participate in merged estimates must expose whether they can produce a calibrated percentage delta for a given service/window. If `can_produce_percent_delta()` is false or the calibration window is incompatible with the latest web baseline, the merger must not subtract local usage from the web baseline.

### Provider Error Contract

- [x] Define `UsageProviderError` with stable variants:
  - [x] `NotConfigured`
  - [x] `Disabled`
  - [x] `MissingData`
  - [x] `PermissionDenied`
  - [x] `ParseFailed`
  - [x] `LoginRequired`
  - [x] `MfaRequired`
  - [x] `CaptchaOrBotCheck`
  - [x] `NetworkUnavailable`
  - [x] `TimedOut`
  - [x] `UnexpectedUi`
  - [x] `UnsafePath`
  - [x] `Internal`
- [x] Map every provider error to a sanitized snapshot or UI event.
- [x] Keep raw filesystem paths, account identifiers, tokens, cookies, auth headers, and raw page text out of errors.
- [x] Add tests for error-to-snapshot mapping.

## Tauri IPC and Event Contract

Before real providers are wired, define and test the IPC boundary.

### Invoke Commands

- [x] `get_app_config`
- [x] `update_app_config`
- [x] `get_usage_snapshots`
- [x] `get_display_state`
- [x] `refresh_usage`
- [ ] `refresh_provider`
- [ ] `open_official_usage_page`
- [ ] `start_provider_login`
- [ ] `reset_provider_session`
- [ ] `clear_cached_snapshots`
- [ ] `clear_provider_profile`
- [ ] `get_log_location`

### Emitted Events

- [x] `usage://snapshots-updated`
- [ ] `usage://refresh-started`
- [ ] `usage://refresh-finished`
- [ ] `usage://provider-error`
- [ ] `settings://updated`
- [ ] `login://required`
- [ ] `session://reset`

### IPC Safety Rules

- [ ] IPC returns app models only.
- [ ] IPC never returns raw browser profile content.
- [ ] IPC never returns raw local log contents.
- [ ] IPC never returns raw page HTML/text from authenticated pages.
- [ ] Every command has a stable error shape for frontend rendering.

## Merge Strategy

Priority:

1. Fresh web snapshot.
2. Fresh web snapshot adjusted by calibrated local usage delta.
3. Local-only estimate.
4. Unknown.

Initial rule:

```text
merged_remaining = web_remaining_at_baseline - estimated_local_consumption_since_baseline
```

Baseline semantics:

- Treat a web snapshot as the exact visible value at `web.last_updated`.
- Apply calibrated local deltas only for local records strictly after `web.last_updated`.
- Never apply a local delta twice across refreshes.
- Clamp merged percentages to `0..=100`.
- If the web baseline is stale or local consumption cannot be mapped to a reliable percentage, show the web baseline unchanged with lower confidence/stale-age messaging instead of inventing precision.

Confidence labels:

- `high`: fresh official web reading.
- `medium`: web baseline plus reliable recent local delta.
- `low`: local-only estimate.
- `unknown`: no usable data.

## Local Provider Discovery Exit Criteria

Do not implement local percentage estimates until discovery answers these questions per service.

- [x] Which files/directories exist on the target machine?
- [ ] Which records are stable enough to parse?
- [x] Which fields are timestamps, model names, token counts, costs, sessions, or status values?
- [x] Which records are machine-local only versus account-wide?
- [x] What source precedence should apply when multiple local sources exist?
- [ ] How should missing directories, unreadable files, invalid records, large files, and rotated logs behave?
- [x] Which test fixtures can be safely sanitized and committed?
- [ ] Which user-entered calibration fields are required to map local activity to percentages?
- [x] What does the provider return when calibration is absent?
- [x] What exact `details` metadata is safe and useful for debugging?

Default local-provider output before calibration:

- [ ] `source = "local"`
- [ ] `confidence = "low"` when activity is parsed but no reliable percentage exists.
- [ ] `confidence = "unknown"` when files are missing, unsupported, unreadable, or stale.
- [ ] `remaining_percent = None` unless a configured/calibrated quota makes a percentage defensible.
- [ ] `details` contains sanitized counts, window metadata, and status codes only.

## Browser and Web Provider Integration Contract

Web providers are allowed only after the automation spike proves a safe backend.

- [ ] The chosen backend supports visible manual login.
- [ ] The chosen backend supports persistent isolated profiles per service.
- [ ] The chosen backend does not import default browser cookies, credentials, or profiles.
- [ ] The chosen backend can disable or avoid password saving/autofill prompts.
- [ ] Authenticated official pages are never loaded in the main Tauri webview.
- [ ] Browser launch arguments and profile paths are logged only in sanitized form.
- [ ] Parser input is sanitized visible text or structured state, not raw authenticated HTML.
- [ ] Fixture regeneration requires explicit user-consented capture.
- [ ] Fixtures are sanitized before writing.
- [ ] Web providers fail closed on login, MFA, CAPTCHA, bot checks, unexpected UI, or parse failure.
- [ ] Web refreshes never run until the user explicitly enables experimental web providers.

## Logging and Diagnostics Policy

- [ ] Add structured provider status/error codes.
- [ ] Prefer stable status codes over raw error messages.
- [ ] Redact home directory paths when they are not needed for debugging.
- [ ] Never log cookies, tokens, auth headers, raw page HTML, raw authenticated text, account identifiers, or full browser errors.
- [ ] Add a UI-visible log location only after log redaction policy exists.
- [ ] Include enough diagnostics to distinguish disabled, missing data, stale, parse failed, login required, and unavailable states.

## Phase Checklist

### Phase 0 — Repository Bootstrap

- [x] Initialize Tauri v2 + Svelte project.
- [x] Add Rust backend entrypoint.
- [x] Add Svelte frontend entrypoint.
- [x] Add Tauri v2 capabilities/permissions.
- [x] Add baseline build/check scripts.
- [x] Add app metadata and bundle identifier.
- [x] Validate app builds.

### Phase 0.5 — KDE/Wayland Platform Compatibility Gate

- [x] Build local AppImage.
- [x] Launch AppImage manually once.
- [ ] Confirm tray icon visibility on CachyOS KDE/Wayland.
- [ ] Confirm tray click opens popup.
- [ ] Confirm popup closes reliably or has acceptable fallback behavior.
- [ ] Confirm app can run as tray-first utility without an always-visible main window.
- [ ] Confirm close button either hides to tray or exits only when explicitly intended.
- [ ] Confirm popup/window position is acceptable on single-monitor and multi-monitor KDE setups.
- [ ] Confirm settings persist after restart.
- [ ] Confirm quit behavior.
- [ ] Document runtime packages and packaging prerequisites discovered during testing.
- [ ] Choose fallback behavior if native tray/popup behavior is unreliable.

### Phase 1 — Tray Shell With Fake Data

- [x] Add system tray icon.
- [x] Add tray menu with show and quit actions.
- [x] Show compact app window from tray click.
- [x] Expose fake Codex and Claude snapshots from backend.
- [x] Render fake snapshots in the popup.
- [x] Show empty state when all services are disabled.
- [x] Decide whether the Tauri `main` window starts hidden, starts minimized, or remains visible during development only.
- [x] Implement close-to-tray behavior if the app should persist after window close.
- [x] Add explicit quit path that fully exits background tray process.
- [ ] Add explicit popup close/click-outside fallback if KDE/Wayland smoke test requires it.

### Phase 2 — Branded Tray State Icons

- [x] Add branded Codex tray asset.
- [x] Add branded Claude tray asset.
- [x] Add low-usage tray asset.
- [x] Add unknown-state tray asset.
- [x] Alternate Codex/Claude tray state every configured `5–10s`.
- [x] Use configured low-usage threshold for low icon selection.
- [x] Use unknown icon when no service has a known remaining value.
- [x] Keep dynamic percentage gauges deferred until provider values are calibrated.
- [x] Add unit tests for tray state/icon selection.
- [ ] Run manual visual smoke test on KDE.

### Phase 3 — Config Store

- [x] Persist settings locally.
- [x] Add config `version` field.
- [x] Add enabled-service settings for Codex and Claude.
- [x] Add local provider toggle.
- [x] Add experimental web provider toggle.
- [x] Add local refresh interval with `30–60s` clamp.
- [x] Add web refresh interval with `15–60min` clamp.
- [x] Add manual web-refresh cooldown with minimum `60s`.
- [x] Add gauge switch interval with `5–10s` clamp.
- [x] Add low-usage threshold clamp.
- [x] Add settings UI for service toggles, provider toggles, local/web refresh intervals, tray switch interval, and low-usage threshold.
- [x] Add raw config loader that can parse unknown/old versions before typed `AppConfig` deserialization.
- [x] Add defaults for fields introduced after config version `1`.
- [x] Add atomic writes using temp-file + fsync/rename where practical.
- [x] Preserve the previous config on parse, migration, serialization, or write failure.
- [x] Surface recoverable config errors in the UI without crashing startup.
- [x] Add settings UI for manual web-refresh cooldown if user-facing control is needed.
- [x] Add browser profile root setting.
- [x] Add optional per-service browser profile path overrides.
- [ ] Add optional manual plan/quota/window configuration for local estimates.
- [ ] Define quota/window schema per service:
  - [ ] plan label
  - [ ] limit kind
  - [ ] reset/window duration
  - [ ] usage unit
  - [ ] user-entered limit
  - [ ] enabled flag
- [x] Add sequential config migrations.
- [x] Preserve previous config file on failed migrations.
- [x] Add browser profile path validation and ownership markers.
- [x] Add unit tests for config defaults and round-trip serialization.
- [x] Add unit tests for migrations and failed migration rollback.
- [x] Add unit tests proving web providers are disabled by default.
- [x] Add unit tests for refresh interval and cooldown validation.
- [x] Add unit tests for safe/unsafe browser profile path handling.

### Phase 4 — Usage Engine and Scheduler

- [x] Add usage snapshot model skeleton.
- [x] Add fake usage snapshot command.
- [x] Drive fake snapshots from enabled-service settings.
- [x] Add central usage engine.
- [x] Store `UsageEngine` in Tauri managed state.
- [x] Add provider registry.
- [x] Re-register providers when settings change.
- [ ] Add refresh scheduler for local and web providers.
- [x] Add latest snapshot cache.
- [x] Add shared display-state cache consumed by both tray rotation and frontend commands.
- [x] Replace hard-coded tray fake values in `lib.rs` with cached display state.
- [x] Add Tauri commands/events for frontend usage updates.
- [x] Define provider IDs for `codex.local`, `codex.web`, `claude.local`, `claude.web`, and `fake`.
- [ ] Define provider timeout behavior.
- [ ] Define provider cancellation behavior.
- [x] Define mockable clock/time source for tests.
- [x] Ensure one active refresh per provider.
- [x] Skip overlapping scheduled refresh ticks.
- [x] Cancel pending refreshes when a provider is disabled.
- [ ] Enforce local and web refresh cadence from config.
- [ ] Enforce manual web-refresh cooldown and provider opt-in.
- [ ] Document Tokio task ownership in scheduler module.
- [ ] Add per-provider failure counters with bounded retry/backoff.
- [ ] Reset retry/backoff state on provider success.
- [ ] Add sanitized tracing/logging policy for provider lifecycle events.
- [ ] Add unit tests for scheduler timing boundaries.
- [ ] Add unit tests for overlap skipping, disable cancellation, retry/backoff reset, and stale snapshots.

### Phase 5 — Claude Code Local Provider

- [x] Complete read-only discovery of available Claude Code local data shapes.
- [x] Record source precedence order for Claude local data.
- [x] Add injectable Claude data root for tests and development.
- [x] Discover Claude Code local usage files.
- [x] Parse `~/.claude/projects/**/*.jsonl` where available.
- [ ] Inspect Claude Code statusline-compatible data if available.
- [ ] Support ccusage-compatible parsing where practical.
- [ ] Parse timestamps, model, input/output/cache tokens, session blocks, estimated cost/usage, and rolling window activity.
- [ ] Define file scanning limits for large logs and many project directories.
- [ ] Define rotated/truncated file behavior.
- [x] Define invalid JSONL line behavior.
- [ ] Define timezone and rolling-window semantics.
- [x] Produce local estimated Claude usage snapshot.
- [ ] Support manual quota/window calibration.
- [ ] Expose calibrated percentage deltas only when records map to the current plan/window.
- [x] Return `remaining_percent = None` instead of inventing precision when logs cannot be mapped reliably.
- [x] Gracefully handle missing files and unexpected log shapes.
- [x] Add parser tests with sanitized JSONL fixtures.
- [x] Add missing-directory test.
- [ ] Add calibrated and uncalibrated local estimate tests.

### Phase 6 — Codex Local Provider

- [x] Complete read-only discovery of available Codex local data shapes.
- [x] Record source precedence order for Codex local data.
- [ ] Add injectable Codex data root for tests and development.
- [x] Inspect available `~/.codex/*` local/session/status files.
- [ ] Inspect Codex statusline or `/status`-derived data if available.
- [ ] Define file scanning limits for large logs and many sessions.
- [ ] Define rotated/truncated file behavior.
- [ ] Define invalid record behavior.
- [ ] Define timezone and rolling-window semantics.
- [ ] Produce local estimated Codex usage snapshot when possible.
- [ ] Mark confidence conservatively.
- [ ] Support manual quota/window calibration.
- [ ] Expose calibrated percentage deltas only when records map to the current plan/window.
- [ ] Return `remaining_percent = None` instead of inventing precision when local data is incomplete or stale.
- [ ] Add parser tests with captured/sanitized fixture data.
- [ ] Add missing-directory test.
- [ ] Add calibrated and uncalibrated local estimate tests.

### Phase 6.5 — Browser Automation Spike

- [ ] Select browser automation backend.
- [ ] Compare Playwright, WebDriver, and lightweight browser-control alternatives.
- [ ] Record decision matrix scores for KDE/Wayland support, persistent profiles, packaging cost, parser access, security controls, and maintainability.
- [ ] Validate persistent isolated profile on CachyOS KDE/Wayland.
- [ ] Validate separate app-owned profile directories/cookie jars per service.
- [ ] Prove there is no import from default browser profiles.
- [ ] Prove visible manual login works for both services.
- [ ] Prove isolated session persistence survives app restart.
- [ ] Prove each official URL exposes parseable visible fields for the snapshot contract.
- [ ] Define parser contract and partial/no-data fallback behavior.
- [ ] Document runtime/package dependencies.
- [ ] Record chosen backend, rejected alternatives, decision matrix, and proceed/defer decision.
- [ ] Disable password manager, autofill, and save-password prompts or defer web providers.
- [ ] Prove fail-closed handling for logged-out, MFA, CAPTCHA, and unexpected UI states.
- [ ] Confirm no saved credentials are present in dedicated profiles after login tests.
- [ ] Confirm no sensitive page content is written to normal logs.
- [ ] Confirm authenticated official pages are never loaded in the main Tauri webview.
- [ ] Identify required Tauri capabilities/plugins for opening URLs, launching child processes, choosing paths, and showing login windows.
- [ ] Review CSP and permissions needed before implementing provider UI.

### Phase 7 — Browser Session Manager

- [x] Add dedicated app-owned browser profile directory per service.
- [x] Add default profile paths under app data directory.
- [x] Define profile ownership marker filename and JSON schema.
- [x] Store marker with app identifier, service, created timestamp, and schema version.
- [x] Canonicalize configured profile paths.
- [x] Reject known default browser profile paths.
- [x] Reject non-app-owned or non-empty directories without ownership marker.
- [x] Require ownership marker before use.
- [x] Prevent import from user's default browser profile.
- [ ] Maintain separate cookie jar/session state per service.
- [ ] Track managed child process ownership per service with PID/handle metadata.
- [ ] Add graceful browser shutdown with timeout/kill fallback.
- [ ] Detect orphaned managed browser processes on startup.
- [ ] Disable password manager, autofill, and save-password prompts where supported.
- [ ] Add manual login window flow.
- [ ] Surface login-required state to UI.
- [ ] Add session reset/logout action.
- [ ] Add guarded clear/delete action for browser profile data.
- [ ] Stop managed browser before deleting browser session data.
- [ ] Delete only marker-owned paths after deletion-time canonicalization, symlink rejection, marker verification, and live-process checks.
- [x] Add negative tests for unsafe browser profile paths.
- [ ] Add tests for browser shutdown, orphan detection, and cleanup refusal.
- [ ] Verify profile/cache paths use restrictive local permissions where supported.
- [ ] Add manual inspection checklist proving profile directories contain no saved credentials after login tests.

### Phase 8 — Web Providers

- [ ] Add Codex web provider for the Codex analytics URL.
- [ ] Add Claude web provider for the Claude usage URL.
- [ ] Parse visible usage fields only.
- [ ] Define exact visible fields required for each provider before parsing implementation.
- [ ] Define fallback behavior when only partial visible data exists.
- [ ] Define parser input format as sanitized visible text/structured accessibility snapshot, not raw authenticated HTML.
- [ ] Implement documented visible-data parser contract for each provider.
- [ ] Return `unknown` or lower-confidence snapshot for partial/no visible usage data.
- [ ] Avoid inventing precision on parse failures.
- [ ] Surface parse failures in UI without crashing.
- [ ] Add manual "Refresh official usage" action.
- [ ] Add sanitized parser fixtures for every implemented web provider.
- [ ] Add fixture update workflow based on explicit user-consented manual captures.
- [ ] Reject raw page HTML, account identifiers, cookies, tokens, auth headers, and unsanitized browser errors from fixtures.
- [ ] Add manual authenticated refresh smoke test for each service.
- [ ] Add parser tests for successful usage read.
- [ ] Add parser tests for partial visible data.
- [ ] Add parser tests for logged-out page.
- [ ] Add parser tests for MFA/CAPTCHA/interruption page.
- [ ] Add parser tests for unexpected UI.
- [ ] Add parser tests for parse failure.
- [ ] Add fixture sanitization tests or review checks.
- [ ] Add provider-level tests proving web providers do not run unless explicitly enabled.

### Phase 9 — Merge Engine

- [ ] Merge web baseline with local deltas.
- [ ] Detect stale web baselines.
- [ ] Expose final per-service display state.
- [ ] Explain source/confidence in popup.
- [ ] Preserve baseline timestamp semantics.
- [ ] Apply only post-baseline local deltas.
- [ ] Avoid double-counting local deltas.
- [ ] Clamp output percentages to `0..=100`.
- [ ] Apply local deltas only when the provider reports a calibrated percentage delta for the relevant baseline window.
- [ ] Keep web baseline unchanged with lower confidence/stale messaging when local deltas are unavailable or incompatible.
- [ ] Add unit tests for web-only data.
- [ ] Add unit tests for local-only data.
- [ ] Add unit tests for web plus local delta.
- [ ] Add unit tests for no double-count across refreshed baselines.
- [ ] Add unit tests for stale web baseline behavior.
- [ ] Add unit tests for unavailable `can_produce_percent_delta()` fallback.
- [ ] Add unit tests for unknown data.
- [ ] Ensure popup and tray use merged data consistently.

### Phase 10 — KDE Polish and Cross-Platform Packaging

- [x] Add Linux AppImage packaging path.
- [x] Add automated release workflow for Linux AppImage, Windows, macOS Intel, and macOS Apple Silicon artifacts.
- [x] Mark Windows/macOS artifacts as untested.
- [ ] Complete manual CachyOS KDE smoke test.
- [ ] Verify launch app.
- [ ] Verify tray appears.
- [ ] Verify popup opens/closes.
- [ ] Verify gauge alternates.
- [ ] Verify settings persist.
- [ ] Verify providers fail gracefully.
- [ ] Verify queued release workflow runs on mainline push.
- [ ] Run or trigger release workflow on `main` or through `workflow_dispatch`.
- [ ] Confirm draft release is created with expected `forgegauge-v<version>-<run>.<attempt>` tag.
- [ ] Verify Linux AppImage artifact uploads.
- [ ] Verify Windows artifact uploads.
- [ ] Verify macOS Intel artifact uploads.
- [ ] Verify macOS Apple Silicon artifact uploads.
- [ ] Confirm release is published only after all build matrix jobs succeed.
- [ ] Record any failing runner labels, action versions, package dependencies, or upload paths.
- [ ] Add optional autostart setting.
- [ ] Add basic failure logging view or log file location.

## UI Requirements

### Tray UI

- [x] One tray icon is present.
- [x] Tray alternates between Codex and Claude.
- [x] Blue/brand Codex asset is available.
- [x] Orange/brand Claude asset is available.
- [x] Gray/unknown state asset is available.
- [x] Red/low state asset is available.
- [ ] Dynamic percentage gauge icons.
- [ ] Confirm tray behavior on CachyOS KDE/Wayland.

### Popup UI

- [x] Shows Codex usage card.
- [x] Shows Claude Code usage card.
- [x] Shows remaining percentage.
- [x] Shows source.
- [x] Shows confidence.
- [x] Shows last update text.
- [x] Shows settings controls.
- [ ] Shows last official check when web provider exists.
- [ ] Shows stale data messaging.
- [ ] Shows login-required state.
- [ ] Adds "Refresh now" action.
- [ ] Adds "Open official Codex page" action.
- [ ] Adds "Open official Claude usage page" action.
- [ ] Adds guarded reset/clear actions.

### Settings

- [x] Enable/disable Codex.
- [x] Enable/disable Claude.
- [x] Enable/disable local providers.
- [x] Enable/disable experimental web providers.
- [x] Configure local refresh interval.
- [x] Configure web refresh interval.
- [x] Configure manual web refresh cooldown.
- [x] Configure gauge switch interval.
- [x] Configure low-usage warning threshold.
- [x] Configure browser profile/session path.
- [x] Configure optional per-service browser profile path overrides.
- [ ] Configure optional manual plan/limit/window values.
- [ ] Configure autostart.
- [ ] Reset browser session data.
- [ ] Clear cached usage data.

## Testing Strategy

### Baseline Validators

Run the relevant subset before completing implementation work:

- [x] `npm run check`
- [x] `npm run build`
- [x] `cargo fmt --check`
- [x] `cargo check`
- [x] `cargo clippy -- -D warnings`
- [x] `cargo test`
- [x] `npm run build:appimage`
- [ ] Add `npm run lint` if linting is configured later.

### Validation Command Matrix

Use the smallest relevant set during iteration, then run the milestone set before marking a phase complete.

| Change type | Commands |
| --- | --- |
| Documentation only | `git diff --check`, `npm run check` |
| Frontend/Svelte | `npm run check`, `npm run build` |
| Rust backend | `cd src-tauri && cargo fmt --check`, `cd src-tauri && cargo check`, `cd src-tauri && cargo clippy -- -D warnings`, `cd src-tauri && cargo test` |
| Tauri integration | `npm run check`, `npm run build`, `cd src-tauri && cargo check`, `npm run tauri -- build --bundles appimage` or `npm run build:appimage` on CachyOS/Arch-like systems |
| Release workflow | Local validators plus a real GitHub Actions `workflow_dispatch` or mainline run |

### Required Evidence Before Checking Items

- [ ] For implemented code: commit/diff evidence exists in source.
- [ ] For automated validation: command and pass/fail result are recorded in the session or relevant commit notes.
- [ ] For manual KDE checks: date/session, OS/session type, artifact/binary used, and observed behavior are recorded.
- [ ] For release checks: workflow run URL, release tag, and artifact names are recorded.
- [ ] For web/session security checks: sanitized inspection notes confirm no secrets or raw authenticated page content are persisted outside browser profiles.

### Automated Tests To Add

- [x] Config serialization.
- [ ] Config migration ordering and failed migration rollback.
- [x] Default web-provider opt-out behavior.
- [x] Refresh interval validation/clamping.
- [x] Manual web-refresh cooldown enforcement.
- [ ] Provider enable/disable scheduler behavior.
- [ ] Provider parsing.
- [ ] Local quota/window calibration.
- [ ] Merge logic.
- [ ] Merge fallback when local providers cannot produce a percentage delta.
- [ ] No-double-count and stale-baseline merge behavior.
- [ ] Stale data handling.
- [ ] Gauge state mapping.
- [ ] Frontend display formatting.
- [ ] Frontend confidence/source labels.
- [ ] Frontend settings form behavior.
- [ ] Frontend web-provider opt-in toggles and disabled states.

### Manual Tests To Complete

- [ ] KDE tray visibility.
- [ ] Popup position and dismissal behavior.
- [ ] Settings persistence after restart.
- [ ] Dedicated browser login.
- [ ] Official Codex page refresh.
- [ ] Official Claude usage page refresh.
- [ ] Network unavailable state.
- [ ] Missing local data state.
- [ ] Expired login state.
- [ ] Windows tray/install smoke test.
- [ ] macOS tray/install smoke test.

## Security and Privacy Checklist

- [x] Web scraping is opt-in in default config.
- [x] Web providers can be disabled.
- [x] Web providers default to disabled.
- [x] Current fake provider does not read or upload account data.
- [ ] No password storage.
- [ ] Managed browser launch disables password manager/autofill/save-password prompts where supported.
- [x] Dedicated browser profiles are separate per service.
- [x] Dedicated browser profiles are app-owned and marker-guarded.
- [x] Dedicated browser profiles never use the user's default browser profile.
- [ ] Clear/delete actions stop managed browser processes first.
- [ ] Clear/delete actions only delete marker-owned paths.
- [ ] Clear/delete actions reject symlinked paths.
- [ ] Clear/delete actions re-verify canonical app-owned marker paths immediately before deletion.
- [ ] Dedicated profiles contain no saved credentials after login validation.
- [ ] No logging cookies, session tokens, auth headers, or sensitive page HTML.
- [ ] Browser profile is isolated from the main browser profile.
- [ ] Scheduler does not start web refreshes until explicit opt-in.
- [ ] Disabling a web provider cancels future scheduled reads.
- [ ] Clear UI label for experimental web provider.
- [ ] User can reset/delete provider session data and cached snapshots.
- [x] Local app data uses restrictive file permissions where supported.
- [ ] `details` metadata is sanitized and never contains raw page content or secrets.
- [ ] Test fixtures are sanitized before being committed or shared.
- [ ] Fixture regeneration requires explicit user-consented captures.
- [ ] Provider errors are sanitized before display/logging.

## Known Risks and Mitigations

| Risk | Mitigation |
| --- | --- |
| Official UI changes break web parsing | Keep providers best-effort, show parse failures, support manual official-page opening |
| Login expires/MFA/CAPTCHA appears | Stop scraping and request manual re-login |
| Codex local data is incomplete | Mark low confidence or unknown |
| Claude local data misses web/app usage | Merge with web baseline when available |
| KDE/Wayland tray behavior is inconsistent | Complete KDE smoke gate before investing heavily in providers |
| Browser automation backend is unsuitable on KDE/Wayland | Run automation spike before web providers |
| Browser profile/session storage is sensitive | Use isolated profiles, avoid password storage, add guarded reset/delete actions |
| False precision in merged estimates | Show source, confidence, and last official check time |
| Popup focus-loss dismissal is unreliable on Wayland | Add explicit close/click-outside/utility-window fallback if needed |
| Scheduler refreshes overlap or continue after disable | Enforce one active refresh per provider, skip overlaps, and cancel on disable |
| Windows/macOS artifacts are initially untested | Mark clearly in release notes and invite reports, issues, and pull requests |

## Review Gates

- [ ] After Phase 0.5: Confirm Tauri tray/popup behavior and required runtime packages on CachyOS KDE/Wayland.
- [ ] After Phase 1: Confirm tray behavior and popup close fallback work on CachyOS KDE.
- [ ] After Phase 4: Confirm app architecture is stable before real providers.
- [ ] After Phase 6: Confirm local providers provide enough value to keep.
- [ ] After Phase 6.5: Confirm browser automation backend is viable before implementing web providers.
- [ ] After Phase 8: Confirm web provider reliability is acceptable for personal use.
- [ ] Before packaging: Run automated checks, complete KDE manual smoke test, and confirm release notes mark Windows/macOS artifacts as untested.

## MVP Cut Line

- [x] Tray shell.
- [x] Popup.
- [x] Branded tray state icons.
- [x] Config.
- [x] Usage snapshot model and fake snapshot command.
- [x] Fake provider.
- [ ] At least one real local provider.
- [x] Central usage engine, provider registry, scheduler, shared display cache, and event stream.

Web providers and merge logic should follow once KDE tray behavior and core UI are stable.

## Next Implementation Milestone

The Phase 4 core data plumbing milestone is complete for the fake-provider path. The next recommended milestone is **tray/window hardening and KDE/Wayland smoke validation** before real provider work.

### Completed Phase 4 Milestone Scope

- [x] Move fake provider behind the same provider contract real providers will use.
- [x] Introduce a backend `UsageEngine` owned by Tauri state.
- [x] Introduce a shared display-state cache.
- [x] Make tray rotation read from the shared display-state cache.
- [x] Make `get_usage_snapshots` read from the shared display-state cache.
- [x] Add event emission for snapshot updates.
- [x] Add tests for display-state mapping and scheduler behavior.

### Completed Phase 4 Milestone Acceptance

- [x] Popup and tray always show the same current fake provider data.
- [x] Disabling Codex or Claude updates both tray and popup through the same state path.
- [x] Scheduler cannot start overlapping refreshes for the same provider.
- [x] Disabling a provider cancels or skips future refreshes.
- [x] Provider errors become sanitized `unknown` snapshots or events.
- [x] `npm run check` passes.
- [x] Rust format, check, clippy, and tests pass.

### Next Milestone Scope

- [x] Decide whether the Tauri `main` window starts hidden, starts minimized, or remains visible during development only.
- [x] Implement close-to-tray behavior if the app should persist after window close.
- [x] Add explicit quit path that fully exits background tray process.
- [ ] Add explicit popup close/click-outside fallback if KDE/Wayland smoke test requires it.
- [ ] Complete KDE/Wayland smoke checks for tray visibility, popup open/close, settings persistence after restart, and quit behavior.
- [ ] Record runtime packages and packaging prerequisites discovered during testing.

Blocked: KDE/Wayland tray visibility, tray click, close-button, and quit-behavior confirmation requires user-visible desktop interaction and cannot be verified through the available Playwright/browser-preview tooling in this session.
