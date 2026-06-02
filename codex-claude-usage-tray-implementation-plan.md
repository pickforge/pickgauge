# Codex + Claude Code Usage Tray App Implementation Plan

## Goal

Implement a personal CachyOS KDE/Linux tray app that displays remaining usage for Codex and Claude Code using:

1. local CLI-derived estimates, and
2. opt-in browser-based readings from official usage pages.

The app must be privacy-conscious, explicit about data confidence, and useful even when web scraping fails.

## Source Spec

This plan implements:

- `codex-claude-usage-tray-spec.md`

Official usage pages:

- Codex: <https://chatgpt.com/codex/cloud/settings/analytics>
- Claude Code: <https://claude.ai/new#settings/usage>

## Success Criteria

- A tray icon runs persistently on KDE Linux.
- The tray icon alternates between Codex and Claude usage every configurable interval.
- The popup shows both services with remaining percentage, source, confidence, and last update time.
- Local usage providers work without account credentials.
- Web providers are opt-in, use a dedicated browser profile, and never store passwords.
- If any provider fails, the app degrades to `unknown` or lower-confidence estimates instead of crashing.
- Settings allow enabling/disabling providers and adjusting refresh behavior.
- The implementation can be validated with lint/type checks/tests and a manual KDE tray smoke test.

## Constraints

- No password storage.
- No CAPTCHA bypass.
- No default website scraping.
- No assumption that Codex/Claude expose stable private APIs.
- Web provider parsing must rely on visible UI/state and be treated as best-effort.
- Local provider data must be labeled as estimated unless proven exact.
- App must not upload usage/session/account data.

## Technical Choices

### App Shell

- **Tauri v2**
- **Rust backend**
- **Svelte frontend**
- Linux/KDE-first packaging initially.

### Persistence

- App config: versioned local JSON file under the app config directory.
- Cached snapshots: local JSON files under the app data directory for latest state only; defer SQLite unless historical snapshots become a requirement.
- Browser profiles: separate isolated app-owned directories per service under the default app data root, with optional per-service path overrides.
- Browser profile path overrides: canonicalize paths, reject known browser/default profile directories, reject non-app-owned or non-empty directories without an app ownership marker, and create/check that marker before use.
- Secrets/session protection: use system secret storage where practical; otherwise clearly document local profile/session storage behavior in-app.
- Data minimization: do not persist raw page HTML, auth headers, cookies outside the browser profile, or unsanitized logs/fixtures.
- Data cleanup: provide clear/delete actions for cached usage data and browser session data; only delete marker-owned profile directories after stopping the managed browser, then re-checking canonical paths, symlinks, ownership markers, and live process state at deletion time.

### Browser Automation

Preferred approach for web providers:

- Use an app-controlled browser automation layer with a dedicated persistent profile.
- Open visible login/setup windows when authentication is required.
- Run usage reads on demand or at low frequency.
- Fail closed when login, MFA, CAPTCHA, or unexpected UI is detected.

Before implementing web providers, run a dedicated automation spike to choose and validate the backend. Candidate backends include Playwright, WebDriver, or another browser-control layer that can use a persistent isolated profile reliably on KDE/Wayland. Tauri's own webview should not be assumed suitable for logged-in website scraping until proven.

The spike must verify:

- visible manual login works for both services;
- persistent isolated profile survives app restart;
- each official URL exposes enough visible usage fields to parse the required snapshot fields;
- the parser contract and partial/no-data fallback behavior are defined before implementation;
- required browser/runtime dependencies are known for packaging;
- backend choice is recorded in a decision matrix covering KDE/Wayland support, persistent per-service profiles, dependency/packaging cost, parser access, security controls, and maintainability; if candidates tie, prefer the one with the least runtime footprint that still satisfies all security requirements;
- password manager, autofill, and save-password prompts can be disabled or the web providers are deferred;
- login/MFA/CAPTCHA/unexpected UI states fail closed;
- no saved credentials are present in dedicated profiles after login tests;
- no sensitive page content is written to normal logs.

## Architecture

```text
src-tauri/
├─ capabilities/
│  └─ default.json
│
├─ tray/
│  ├─ icon_renderer.rs
│  ├─ tray_controller.rs
│  └─ popup_controller.rs
│
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
│
├─ browser/
│  ├─ session.rs
│  ├─ login_flow.rs
│  └─ scraper.rs
│
├─ config/
│  ├─ model.rs
│  ├─ migrations.rs
│  └─ store.rs
│
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

This layout is a target structure; the first implementation can start smaller and split modules when the shell is stable.

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

`details` must be sanitized metadata only. Expected keys include provider status/error codes, parse-field presence, baseline timestamps, stale age, and local delta metadata. It must not contain raw page text/HTML, account identifiers, cookies, tokens, auth headers, or unsanitized browser errors.

## Provider Contract

Each provider should implement the same conceptual contract:

```rust
trait UsageProvider {
    fn service(&self) -> Service;
    fn source(&self) -> UsageSource;
    async fn refresh(&self) -> Result<UsageSnapshot, UsageProviderError>;
}
```

Provider failures should be converted into user-safe states:

- not configured
- login required
- unavailable
- parse failed
- stale data

Local providers that participate in merged estimates must expose whether they can produce a calibrated percentage delta for a given service/window. If `can_produce_percent_delta()` is false or the calibration window is incompatible with the latest web baseline, the merger must not subtract local usage from the web baseline.

## Tauri IPC Boundary

Define a concise IPC table before wiring the frontend, explicitly separating invoke commands from emitted events:

- read current usage snapshots/display state;
- trigger refresh for one provider or all enabled providers;
- read/update settings;
- start manual login and reset isolated sessions;
- clear cached snapshots/profile data through guarded cleanup paths.
- emitted usage/settings/login-required/error events and their payload schemas.

Commands/events must return sanitized app models only, never raw browser/profile content.

## Merge Strategy

The usage engine keeps the latest snapshots per service/source.

Priority:

1. Fresh web snapshot.
2. Fresh web snapshot adjusted by local usage delta.
3. Local-only estimate.
4. Unknown.

Initial merge rule:

```text
merged_remaining = web_remaining_at_baseline - estimated_local_consumption_since_baseline
```

Baseline semantics:

- Treat a web snapshot as the exact visible value at `web.last_updated`.
- Apply calibrated local deltas only for local records strictly after `web.last_updated`.
- Never apply a local delta twice across refreshes.
- Clamp merged percentages to `0..=100`.
- If the web baseline is stale or local consumption cannot be mapped to a reliable percentage, show the web baseline unchanged with lower confidence/stale-age warning instead of inventing precision.

## Phase Plan

### Phase 0 — Repository Bootstrap

Deliverables:

- Initialize a Tauri v2 + Svelte project.
- Add Tauri v2 capabilities/permissions with least-privilege command/window/tray access.
- Add baseline formatting/lint/test scripts.
- Add project structure and minimal app entrypoint.

Validation:

- `npm run lint` if configured.
- `npm run check` or `svelte-check` if configured.
- `npm run build`.
- Rust validators run from the Cargo workspace root when one exists, otherwise with `--manifest-path src-tauri/Cargo.toml`:
  - `cargo fmt --check`.
  - `cargo clippy -- -D warnings` once clippy is configured.
  - `cargo test`.
  - `cargo check`.

Acceptance:

- App builds successfully with no feature behavior yet.

### Phase 0.5 — KDE/Wayland Platform Compatibility Gate

Deliverables:

- Verify Tauri v2 tray support on the target CachyOS KDE/Wayland session before provider work.
- Document required runtime packages and packaging prerequisites discovered during the tray test.
- Choose fallback behavior if native tray or popup behavior is unreliable, such as explicit close controls, click-outside-region handling, or a small utility window.

Validation:

- Manual CachyOS KDE/Wayland smoke test for launch, tray visibility, tray click, popup open, and popup close.

Acceptance:

- Tray and popup behavior are proven on the target desktop, or provider implementation is paused until a fallback path is selected.

### Phase 1 — Tray Shell With Fake Data

Deliverables:

- System tray icon appears on KDE.
- Tray click opens a compact popup window.
- Popup dismisses when it loses focus where supported, and always has a reliable Wayland fallback such as explicit close/click-outside-region handling.
- Backend exposes fake Codex/Claude snapshots through a fake provider to frontend.

Validation:

- `npm run build`
- `cargo check` from the Cargo workspace root, or `cargo check --manifest-path src-tauri/Cargo.toml` if the root has no `Cargo.toml`
- Manual run on KDE session, including popup close fallback.

Acceptance:

- Tray icon is visible and popup can be opened/closed reliably.

### Phase 2 — Dynamic Gauge Icon

Deliverables:

- Render gauge icons for known, low, and unknown usage states using discrete cached runtime icons for percentage steps plus low/unknown states.
- Use deterministic icon cache keys by service, percentage bucket, low/unknown state, size, and scale; render RGBA tray bitmaps at the platform-required size, starting with 32x32 plus higher scale variants if KDE requires them.
- Bucket percentages in fixed steps, such as 5% increments from 0 to 100, and map colors consistently before cache lookup.
- Alternate Codex/Claude display every configured `5–10s`.
- Tray controller owns the gauge switch timer, reads the current config, and emits alternation/update events without competing frontend timers.
- Use color mapping:
  - Codex: blue
  - Claude: orange
  - unknown: gray
  - low threshold: red accent

Validation:

- Unit tests for gauge state/color selection.
- Manual visual smoke test.

Acceptance:

- Tray icon visibly changes between both services and reflects fake percentages.

### Phase 3 — Config Store

Deliverables:

- Persist settings locally.
- Add settings UI for:
  - enabled services
  - enabled local providers
  - enabled web providers
  - local refresh interval
  - web refresh interval, validated/clamped to `15–60min`
  - manual web-refresh cooldown, defaulting to at least `60s`
  - gauge switch interval, validated/clamped to `5–10s`
  - low-usage threshold
  - browser profile root and optional per-service profile path overrides
  - optional manual plan/quota/window configuration for local estimates
- Include a monotonic integer config `version` field and sequential migration layer; failed migrations must leave the previous config file intact and surface a recoverable error.
- Validate browser profile path settings with canonicalization, known-profile rejection, non-empty directory checks, symlink rejection, per-service app-owned directories, and app ownership markers.

Validation:

- Unit tests for config defaults and round-trip serialization.
- Unit tests for config version migration, sequential migration ordering, and failed migration preserving the previous config.
- Unit tests proving web providers are disabled by default.
- Unit tests for refresh interval clamping/validation and manual-refresh cooldown defaults.
- Unit tests for safe/unsafe browser profile path handling.
- Manual restart confirms settings persist.

Acceptance:

- Settings survive app restart and drive fake/provider behavior.

### Phase 4 — Usage Engine and Scheduler

Deliverables:

- Central usage engine.
- Provider registry.
- Refresh scheduler for local and web providers.
- Snapshot cache.
- Tauri commands/events for frontend updates.
- One active refresh per provider; skip overlapping ticks and cancel pending refreshes when a provider is disabled.
- Enforce local refresh cadence within the configured low-cost range and web refresh cadence within `15–60min`; manual web refreshes must respect the configured cooldown and never bypass provider opt-in.
- Tokio task ownership documented in the scheduler module.
- Per-provider failure counters with bounded retry/backoff that reset on success.
- Tauri command/event table for usage, settings, refresh, login, reset, and cleanup actions.

Validation:

- Unit tests for scheduler timing boundaries where practical.
- Unit tests for web interval clamping/enforcement and manual-refresh cooldown.
- Unit tests for overlap skipping, disable cancellation, and retry/backoff reset.
- Unit tests for stale snapshot handling.

Acceptance:

- Popup updates automatically from scheduled fake provider refreshes.

### Phase 5 — Claude Local Provider

Deliverables:

- Discover Claude Code local usage files.
- Parse available JSONL/session data.
- Produce local estimated Claude usage snapshot.
- Support manual quota/window calibration when available.
- Expose `can_produce_percent_delta()` only when calibrated local records can be mapped to a percentage delta for the current plan/window.
- Return `remaining_percent = None` instead of inventing precision when logs cannot be mapped to a reliable percentage.
- Gracefully handle missing files or unexpected log shapes.

Validation:

- Parser tests with small fixture JSONL files.
- Missing-directory test.
- Tests for calibrated and uncalibrated local estimates.

Acceptance:

- Claude local provider returns `low` or `unknown` confidence instead of failing hard.

### Phase 6 — Codex Local Provider

Deliverables:

- Inspect and support available Codex local/session/status files.
- Produce local estimated Codex usage snapshot when possible.
- Mark confidence conservatively.
- Support manual quota/window calibration when available.
- Expose `can_produce_percent_delta()` only when calibrated local records can be mapped to a percentage delta for the current plan/window.
- Return `remaining_percent = None` instead of inventing precision when local data is incomplete or stale.

Validation:

- Parser tests using captured/sanitized fixture data.
- Missing-directory test.
- Tests for calibrated and uncalibrated local estimates.

Acceptance:

- Codex local provider is useful when data exists and honest when it does not.

### Phase 6.5 — Browser Automation Spike

Deliverables:

- Select browser automation backend.
- Validate persistent isolated profile on CachyOS KDE/Wayland.
- Validate separate app-owned profile directories/cookie jars per service with no import from default browser profiles.
- Prove manual login flow can be launched visibly.
- Prove each official usage page exposes parseable visible fields for the snapshot contract.
- Document runtime/package dependencies.
- Record the chosen backend, rejected alternatives, decision-matrix scores, and explicit proceed/defer decision before implementation starts.
- Prove password manager, autofill, and save-password prompts can be disabled, or defer web providers.
- Prove fail-closed handling for logged-out, MFA, CAPTCHA, and unexpected UI states.

Validation:

- Manual login smoke test for each service.
- Manual app restart confirms isolated session persistence.
- Profile inspection confirms no saved credentials and no shared cookie jar between services.
- Parser viability note for each URL lists visible fields used and fallback behavior for partial/no data.
- Recorded backend decision includes the tiebreaker result if more than one backend passes the mandatory checks.
- Log review confirms no cookies, tokens, auth headers, or raw sensitive page content are logged.

Acceptance:

- Web-provider implementation can proceed with a proven automation backend, or web providers are deferred if the spike fails.

### Phase 7 — Browser Session Manager

Deliverables:

- Dedicated app-owned browser profile directory per service.
- Default profile paths under the app data directory.
- Path ownership guardrails: canonicalize, reject known default browser profile paths, reject non-app-owned/non-empty directories without marker, and require marker before use.
- No import from the user's default browser profile.
- Separate cookie jar/session state per service.
- Track managed child process ownership per service with PID/handle metadata, graceful shutdown, timeout/kill fallback, and orphan detection on startup before opening or deleting profiles.
- Password manager, autofill, and save-password prompts disabled for managed browser sessions where supported.
- Manual login window flow.
- Login-required state surfaced to UI.
- Session reset/logout action.
- Clear/delete action for browser profile and cached web snapshots that stops the managed browser first and deletes only marker-owned paths after deletion-time canonicalization, symlink rejection, ownership-marker verification, and live-process checks.

Validation:

- Manual login/logout smoke test.
- Ensure no password fields are stored by the app and no saved credentials are present in dedicated profiles.
- Negative tests for unsafe browser profile paths.
- Tests for graceful browser shutdown, orphaned process detection, and cleanup refusing symlinked or marker-missing paths.
- Verify profile/cache paths use restrictive local permissions where supported.

Acceptance:

- User can authenticate each service in isolated browser profiles, and unsafe profile paths are refused.

### Phase 8 — Web Providers

Deliverables:

- Codex web provider reads visible usage data from the Codex analytics URL.
- Claude web provider reads visible usage data from the Claude usage URL.
- Each web parser implements the documented visible-data contract for required fields and supported fallbacks.
- Parse failures are visible in UI and do not crash the app.
- Partial/no visible usage data returns `unknown` or a lower-confidence snapshot with sanitized details; it must not invent precision.
- Manual "Refresh official usage" action.
- Sanitized parser fixtures for every implemented web provider.
- Fixture update workflow: regenerate fixtures only from user-consented manual captures through an explicit developer command, sanitize before writing, and reject raw page HTML, account identifiers, cookies, tokens, auth headers, and unsanitized browser errors.

Validation:

- Manual authenticated refresh for each service.
- Confirm each URL still exposes the visible fields required by its parser contract.
- Mandatory parser tests against saved sanitized page text/snapshots for:
  - successful usage read
  - partial visible data
  - logged-out page
  - MFA/CAPTCHA/interruption page
  - unexpected UI
  - parse failure
- Fixture sanitization tests or review checks verifying no secrets, account identifiers, or raw auth/session artifacts are present.

Acceptance:

- Fresh web snapshots appear with `high` confidence when parsing succeeds.

### Phase 9 — Merge Engine

Deliverables:

- Merge web baseline with local deltas.
- Detect stale web baselines.
- Expose final per-service display state.
- Explain source/confidence in popup.
- Preserve baseline timestamp semantics, apply only post-baseline local deltas, avoid double-counting, and clamp output percentages.
- Apply local deltas only when the source provider explicitly reports a calibrated percentage delta for the relevant baseline window; otherwise leave the web baseline unchanged and lower confidence/stale messaging as needed.

Validation:

- Unit tests for:
  - web-only
  - local-only
  - web + local delta
  - no double-count across refreshed baselines
  - stale web baseline
  - unavailable `can_produce_percent_delta()` fallback keeps web baseline unchanged
  - stale baseline plus local delta behavior
  - unknown data

Acceptance:

- Popup and tray use merged data consistently.

### Phase 10 — KDE Polish and Packaging

Deliverables:

- KDE/Wayland manual testing.
- AppImage or local binary packaging.
- Optional autostart setting.
- Basic failure logging view or log file location.

Validation:

- Manual CachyOS KDE smoke test:
  - launch app
  - tray appears
  - popup opens/closes
  - gauge alternates
  - settings persist
  - providers fail gracefully

Acceptance:

- App is usable as a daily personal tray utility.

## Testing Strategy

Baseline validators should include `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `cargo check`, `npm run lint`, `npm run check`/`svelte-check`, and `npm run build` as they become available. Run Rust validators from the workspace root when a root `Cargo.toml` exists; otherwise use `--manifest-path src-tauri/Cargo.toml`.

### Automated Tests

- Rust unit tests:
  - config serialization
  - config migration ordering and failure rollback
  - default web-provider opt-out behavior
  - refresh interval validation/clamping and manual web-refresh cooldown
  - provider enable/disable scheduler behavior
  - provider parsing
  - local quota/window calibration
  - merge logic
  - merge fallback when local providers cannot produce a percent delta
  - no-double-count and stale-baseline merge behavior
  - stale data handling
  - gauge state mapping

- Frontend tests if added:
  - display formatting
  - confidence/source labels
  - settings form behavior
  - web-provider opt-in toggles and disabled states

### Manual Tests

- KDE tray visibility.
- Popup position/dismiss behavior.
- Dedicated browser login.
- Official page refresh.
- Network unavailable state.
- Missing local data state.
- Expired login state.

## Security and Privacy Review Checklist

- No password storage.
- Managed browser launch disables password manager/autofill/save-password prompts where supported.
- Dedicated browser profiles are separate per service, app-owned, marker-guarded, and never the user's default browser profile.
- Clear/delete actions stop managed browser processes first and only delete marker-owned paths.
- Clear/delete actions reject symlinked paths and re-verify canonical app-owned marker paths immediately before deletion.
- Dedicated profiles contain no saved credentials after login validation.
- No logging cookies, session tokens, auth headers, or page HTML containing sensitive account data.
- Browser profile is isolated from the main browser profile.
- Web scraping is opt-in and can be disabled per service.
- Default config disables web providers.
- Scheduler does not start web refreshes until explicit opt-in.
- Disabling a web provider cancels future scheduled reads.
- Clear UI label for experimental web provider.
- User can reset/delete provider session data and cached snapshots.
- Local app data uses restrictive file permissions where supported.
- `details` metadata is sanitized and never contains raw page content or secrets.
- Test fixtures are sanitized before being committed or shared.
- Fixture regeneration requires explicit user-consented captures and sanitization before persisted test data is updated.
- Provider errors are sanitized before display/logging.

## Known Risks and Mitigations

| Risk | Mitigation |
| --- | --- |
| Official UI changes break web parsing | Keep provider best-effort, show parse failures, support manual official-page opening |
| Login expires/MFA/CAPTCHA appears | Stop scraping and request manual re-login |
| Codex local data is incomplete | Mark low confidence or unknown |
| Claude local data misses web/app usage | Merge with web baseline when available |
| KDE/Wayland tray behavior is inconsistent | Test in Phase 0.5 before investing in providers |
| Browser automation backend is unsuitable on KDE/Wayland | Run Phase 6.5 automation spike before web providers |
| Browser profile/session storage is sensitive | Use isolated profile, avoid password storage, add reset session and cache deletion actions |
| False precision in merged estimates | Show source, confidence, and last official check time |
| Popup focus-loss dismissal is unreliable on Wayland | Require explicit close/click-outside/utility-window fallback in Phase 1 |
| Scheduler refreshes overlap or continue after disable | Enforce one active refresh per provider, skip overlaps, and cancel on disable |

## Review Gates

Before moving past each gate:

1. **After Phase 0.5**
   - Confirm Tauri tray/popup behavior and required runtime packages on CachyOS KDE/Wayland.

2. **After Phase 1**
   - Confirm tray behavior and popup close fallback work on CachyOS KDE.

3. **After Phase 4**
   - Confirm app architecture is stable before real providers.

4. **After Phase 6**
   - Confirm local providers provide enough value to keep.

5. **After Phase 6.5**
   - Confirm browser automation backend is viable before implementing web providers.

6. **After Phase 8**
   - Confirm web provider reliability is acceptable for personal use.

7. **Before packaging**
   - Run automated checks and complete the KDE manual smoke test.

## MVP Cut Line

The first usable MVP should include:

- tray shell
- popup
- dynamic gauge
- config
- usage engine
- fake provider
- at least one real local provider

Web providers and merge logic can follow once KDE tray behavior and the core UI are stable.
