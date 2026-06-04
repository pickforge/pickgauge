# ForgeGauge Implementation Plan

## Status

This is the canonical consolidated plan for ForgeGauge. It merges the previous product spec and phased implementation plan into one checklist-driven document.

Current app state: **early Tauri/Svelte MVP with fake fallback data, local Claude/Codex providers with optional manual calibration, persisted settings, branded tray wiring, app icons, AppImage build support, and a verified release workflow on remote `main`.**

Last readiness review: source checked against this plan after consolidation. The plan is now intended to be executable as the active backlog, with unchecked items representing the remaining implementation work.

Latest progress, 2026-06-03: completed the Phase 4 core data plumbing milestone for the fake-provider path. Validation passed with `npm run check`, `npm run build`, `cargo fmt --check`, `cargo check`, `cargo clippy -- -D warnings`, `cargo test`, and `npm run build:appimage`. Browser preview smoke checks covered desktop and mobile layouts, settings checkbox interaction, and overflow checks via Playwright; full KDE/Wayland tray smoke testing remains unchecked.

Tray/window progress, 2026-06-03: decided on tray-first startup for normal runtime, configured the Tauri `main` window to start hidden, added close-to-tray handling for normal window close requests, kept the tray menu `Quit` action as the explicit full-exit path, and rebuilt the AppImage successfully. Full KDE/Wayland tray visibility, tray-click, close-button, and quit-behavior confirmation remains unchecked.

Release workflow progress, 2026-06-03: verified successful GitHub Actions release run `26882140665` on remote `main` commit `4861da642752be3e0ea61282d45bf8b850bb5170`. The run created tag `forgegauge-v0.1.0-4.1`, uploaded Linux AppImage, Windows installer/MSI, macOS Intel DMG, and macOS Apple Silicon DMG assets, then published the release after all build matrix jobs succeeded. This verifies the remote mainline workflow and asset uploads, but not the current feature branch or Windows/macOS install behavior.

Config progress, 2026-06-03: added a raw JSON config load boundary, default filling before typed deserialization, future-version rejection, atomic temp-file/fsync/rename persistence, restrictive config-file permissions on Unix, startup config-error surfacing, a manual web-refresh cooldown settings control, `v1 -> v2` migration support, browser profile root/override config fields, browser profile path UI controls, browser profile path validation with ownership markers, and path-level tests for missing/current/partial/malformed/future configs, write-failure preservation, failed migration rollback, web-provider opt-out, interval/cooldown clamping, v1 migration, and safe/unsafe browser profile paths. Validation passed with `npm run check`, `npm run build`, `cargo fmt --check`, `cargo check`, `cargo clippy -- -D warnings`, `cargo test`, and `npm run build:appimage`. Playwright browser-preview checks covered desktop/mobile settings layout and overflow after the browser profile controls were added.

Calibration config progress, 2026-06-03: added config version `3` with per-service local quota settings for enablement, plan label, limit kind, rolling-window duration, usage unit, and user-entered token limit. The settings UI now persists those values, and config tests cover `v1 -> v2 -> v3` migration, `v2 -> v3` migration, round trips, and normalization.

Calibration provider progress, 2026-06-03: local Claude and Codex providers now consume enabled token-limit calibration settings, map timestamped records into the configured rolling window, and emit low-confidence local percentages only when at least one parsed record maps to that window. Unmapped or disabled calibration continues to return `remaining_percent = None`. The merge engine can combine a fresh official web baseline with compatible calibrated local deltas.

Local provider discovery progress, 2026-06-03: completed privacy-limited read-only shape discovery for local Claude Code and Codex roots and recorded sanitized findings in `docs/discovery/local-provider-data-shapes.md`. Discovery covered file locations, aggregate counts, JSON keys, SQLite schemas, candidate source precedence, machine-local scope, fixture strategy, scan policy, statusline availability, and safe metadata boundaries without committing raw local records or authenticated data.

Claude local provider progress, 2026-06-03: added an injectable Claude local data root and a synthetic-fixture JSONL parser for `~/.claude/projects/**/*.jsonl`, then wired the Claude local provider into the usage registry when local providers are enabled. The provider emits local low-confidence snapshots with token/cache/session/model counts and `remaining_percent = None` when uncalibrated, emits sanitized unknown snapshots for missing project data or invalid records, and enforces bounded JSONL file/record scans with sanitized skip counters. Cost/block aggregation and full local-provider completion remain unchecked.

Claude server-tool progress, 2026-06-04: Claude local parsing now aggregates numeric `message.usage.server_tool_use` values into a sanitized `serverToolUseCount` and the frontend local activity summary can display that aggregate. Raw server-tool keys, payloads, content, IDs, costs, and block data remain excluded.

Codex local provider progress, 2026-06-03: added an injectable Codex local data root, a sanitized `state_5.sqlite` fixture, and a read-only parser for local thread token counts, then wired the Codex local provider into the usage registry when local providers are enabled. The provider emits local low-confidence snapshots with aggregate thread/token/model counts and `remaining_percent = None` when uncalibrated, emits sanitized unknown snapshots when the state database is missing or unreadable, enforces bounded thread scans, and supports calibrated web-baseline deltas.

Local provider policy progress, 2026-06-03: hardened local-provider edge cases for malformed and truncated records. Claude scans only exact `.jsonl` files, ignores `.jsonl.1` style rotations, counts truncated lines as sanitized invalid records, and reports source RFC3339 timestamp metadata. Codex reads only `state_5.sqlite`, treats corrupt or schema-incompatible databases as sanitized parse failures, counts malformed token rows without leaking row data, and reports Unix epoch millisecond metadata. Without active calibration, both local providers keep machine-local activity clearly uncalibrated and avoid inferring rolling-window percentages.

Web parser fallback progress, 2026-06-03: extended the sanitized visible-state parser contract and fixtures to fail closed for `network_unavailable` and `timed_out` states, in addition to logged-out, MFA, CAPTCHA/bot-check, unexpected UI, missing visible data, parse failure, invalid reset timestamps, and unsupported visible fields. Real browser-backed provider launch, authenticated refresh, and network/manual smoke tests remain unchecked.

Browser-preview validation progress, 2026-06-03: reran the Vite browser preview at `http://127.0.0.1:1420/` with Playwright. Desktop validation confirmed web-provider opt-in enables official refresh/login controls and profile path inputs, the Start login action returns the browser-preview fallback without navigation or crashes, and the hide-to-tray button returns the browser-preview fallback status. Mobile validation at `390px` width found no document or element horizontal overflow, with usage cards, settings controls, profile path inputs, local calibration controls, and maintenance buttons fitting the viewport. KDE/Wayland tray behavior and real desktop-only Tauri APIs remain unchecked.

Frontend status-note coverage progress, 2026-06-03: added Vitest coverage for provider status notes shown on usage cards, including missing local data, unavailable local/provider state, network unavailable, timed out, login required, CAPTCHA/bot-check, unexpected UI, and hidden parsed/placeholder/unknown raw status values. Manual missing-data, network, and expired-login smoke tests remain unchecked because they require end-to-end desktop/provider state validation.

Browser-preview state fixture progress, 2026-06-04: added browser-preview-only query states for missing local data, network unavailable, expired login, MFA required, CAPTCHA/bot-check, unexpected UI, timeout, parse failure, stale data, provider unavailable, permission denied, unsafe profile path, and disabled provider. Vitest covers the query-state mapping and rendered status-note snapshots, and Playwright browser-preview smoke checks verified those states plus the default preview at desktop `1280x900` and mobile `390x900` without horizontal overflow. Real desktop/provider smoke tests remain unchecked.

Browser-preview validator progress, 2026-06-04: added `npm run test:browser-preview`, a repeatable Playwright validation script that starts the Vite browser preview, checks the default and failure/graceful-provider query states at desktop `1280x900` and mobile `390x900`, verifies expected usage cards and status notes, checks horizontal overflow, and exercises the experimental web-provider opt-in desktop-only fallback path. Real desktop/provider smoke tests remain unchecked.

Manual smoke preflight progress, 2026-06-04: added `npm run smoke:preflight`, a sanitized metadata collector for future manual KDE/auth/platform smoke notes. It reports commit, app/package metadata, OS/session signals, Playwright package version, and repo-relative AppImage/sidecar artifact status without browser profile contents, account data, secrets, or full local paths. Manual observed-behavior and authenticated-profile evidence remains unchecked.

Manual evidence template progress, 2026-06-04: extended `npm run smoke:preflight` with sanitized pending-observation templates for KDE tray behavior, authenticated web/session checks, and Windows/macOS platform smoke. The templates list the required manual evidence fields and pending observation categories while still excluding cookies, tokens, auth headers, browser profile contents, account identifiers, authenticated page content, and full local paths. The preflight now also records sanitized KDE smoke dependency availability for `qdbus`, `gdbus`, `xdotool`, `xprop`, and `xmessage`, plus StatusNotifier host registration status when queryable.

KDE tray registration progress, 2026-06-04: added `npm run smoke:kde-tray`, a limited KDE/Wayland StatusNotifier smoke that launches the AppImage with isolated XDG directories, verifies ForgeGauge registers an active `org.kde.StatusNotifierItem` through KDE's watcher, verifies the DBusMenu exposes `Show ForgeGauge` and `Quit`, dispatches the tray `Quit` menu event, confirms the AppImage exits successfully and unregisters the tray item, then removes temporary dirs. This proves D-Bus tray registration and tray-menu quit handling in the current session, but not user-visible tray placement, tray click behavior, popup open/close, settings persistence, or visual quit-menu interaction.

KDE window lifecycle progress, 2026-06-04: the main Tauri window is now configured as non-closable where supported, the run loop prevents implicit all-windows-closed exits while preserving explicit tray `Quit`, and tray `Show ForgeGauge` recreates the main webview if KDE/XWayland destroys it after a close request. `npm run smoke:kde-tray` now also verifies the AppImage starts with no visible ForgeGauge window, the tray menu `Show ForgeGauge` event opens a visible window, an XWayland window-close request removes the visible window while the process and tray item remain alive, `Show ForgeGauge` can reopen/recreate the window afterward, and tray `Quit` still exits cleanly. This proves an automated XWayland close/reopen fallback, but not user-visible tray placement, physical tray click behavior, popup positioning, or human-visible settings-form persistence.

KDE popup utility-window progress, 2026-06-04: the popup window now applies skip-taskbar and always-on-top hints when created or shown, hides on focus loss, and left tray click toggles visibility instead of only showing an already-visible popup. `npm run smoke:kde-tray` now requires `xprop` and `xmessage`, verifies the packaged XWayland popup exposes `_NET_WM_STATE_SKIP_TASKBAR` plus an above/stays-on-top state, moves focus to a throwaway X11 window, and verifies focus loss hides the ForgeGauge popup while the process and tray item remain alive. This proves automated utility-window hints and focus-loss dismissal for the packaged popup, but not physical tray-click behavior, exact tray-relative placement, focus-loss behavior under every compositor path, or multi-monitor placement.

KDE popup positioning progress, 2026-06-04: tray-click popup opening now positions the window near the tray interaction point when the platform provides click coordinates, prefers above/right-aligned placement, and clamps the popup inside the active monitor work area with a primary-monitor fallback. Rust tests cover bottom-right tray anchors, top-edge fallback, negative-origin monitor layouts, and work areas smaller than the popup. The rebuilt AppImage still passes `npm run smoke:kde-tray`, preserving the KDE DBusMenu fallback path. Physical tray-click behavior and human-visible single/multi-monitor placement remain unchecked.

KDE settings persistence smoke progress, 2026-06-04: `npm run smoke:kde-tray` now restarts the packaged AppImage with isolated XDG directories, verifies ForgeGauge creates a current-schema config on first launch, writes sanitized non-secret service-toggle and gauge-interval settings into that isolated config, restarts the AppImage from the same isolated root, and verifies those persisted settings survive the packaged restart before tray `Quit` cleanup. This proves packaged config persistence across an isolated restart, but not a human-visible settings-form save inside the KDE webview.

KDE gauge rotation smoke progress, 2026-06-04: `npm run smoke:kde-tray` now also restarts the packaged AppImage with deterministic local providers disabled, observes StatusNotifier `IconName` updates over D-Bus, decodes the exported tray PNGs, and verifies the rendered icon rotation includes both the `Codex` and `Claude Code` service accent colors. The smoke records only sanitized service labels and booleans, not raw D-Bus payloads, icon paths, or screen captures. This proves automated packaged tray icon rotation for enabled services under deterministic data, but not physical tray placement or human-visible icon animation.

Browser session manager reconciliation progress, 2026-06-04: reconciled stale checklist state for the isolated browser session manager. `cargo test browser_session --lib` and `cargo test browser_profile --lib` verify app-owned marker-guarded profile roots, default app-owned profile paths, distinct/non-nested service profile paths, browser process tracking, shutdown/orphan recovery, Playwright persistent-context launch request construction, disabled password/autofill preferences, redacted diagnostics, sanitized profile inspection, and safe profile clearing. `npm run test:sidecar-launch` now emits sanitized CachyOS/KDE/Wayland JSON evidence while verifying headed Playwright launches to both official URLs with temporary isolated Codex/Claude profiles, profile persistence across relaunch, disabled storage preferences, no seeded default-profile import, sanitized stdout/stderr, process-group cleanup, and temporary profile root removal. Authenticated cookie/session contents, saved-credential absence after login, and real authenticated page parsing remain unchecked.

Fail-closed web boundary progress, 2026-06-03: explicit web-provider opt-in now registers fail-closed Codex and Claude web provider boundaries. Until a browser backend is selected and manually validated, official web refreshes return sanitized `login_required` web snapshots instead of `Provider is not configured`; local or fake display data remains visible when present, with the official web failure carried as sanitized `webStatus` and optional sanitized `webReason` metadata. Display merging is covered for login, MFA, CAPTCHA/bot-check, unexpected UI, parse failure, network unavailable, and timeout web failures. Real browser-backed provider launch, authenticated refresh, cookie/session validation, and password-manager validation remain unchecked.

Browser launch policy progress, 2026-06-03: added a backend-agnostic Chromium launch plan helper that binds each service to a service-specific profile path, includes password-manager/autofill suppression flags and disabled storage preferences, initializes on-disk Chromium `Default/Preferences` with those disabled storage preferences during managed profile preparation and the fail-closed login-start boundary, and exposes only sanitized diagnostics with redacted `--user-data-dir` profile labels. The launch plan's debug output also redacts raw profile paths and raw `--user-data-dir` arguments. Real browser process launch integration, manual login flow, and authenticated profile validation remain unchecked.

Profile inspection progress, 2026-06-03: added a sanitized managed Chromium profile storage inspector for future login validation. It reports credential-store artifact counts, autofill-store artifact counts, symlink counts, password/autofill preference booleans, inspected entry counts, and limit status without returning raw paths, cookies, browser storage, authenticated page content, or preference file contents. The inspector is exposed through sanitized IPC and maintenance UI actions for future validation. Manual authenticated profile inspection remains unchecked.

Profile isolation progress, 2026-06-04: canonical managed profile resolution now rejects identical, nested, and root-overlapping Codex/Claude profile paths before creating profile directories. This prevents configured overrides from sharing Chromium user-data-dir storage between services. Manual authenticated cookie/session validation remains unchecked.

Playwright backend decision progress, 2026-06-04: user approved the Playwright headed Chromium sidecar backend. Added an internal Playwright launch request contract that maps the existing managed Chromium launch policy to Playwright's persistent user-data-dir shape while keeping raw profile paths out of diagnostics. Added a tested sidecar JSON launch protocol and dry-run validation boundary that emits only sanitized status metadata, plus Rust serialization, sanitized response parsing, a backend-owned Tauri shell sidecar spawn path for the `launchLogin` request, Linux target-triple sidecar packaging verified through AppImage bundling, and local headed sidecar launch validation for both official URLs with temporary isolated profiles that persist across relaunch, preserve disabled password/autofill preferences, avoid importing seeded fake default Chrome/Chromium profile data, and keep sidecar stdout/stderr free of raw launch data, fake profile sentinels, auth-looking material, and page markup. Manual authenticated login flow and authenticated profile validation remain unchecked.

Profile storage artifact progress, 2026-06-04: extended sanitized managed-profile inspection to report cookie-store and site-storage artifact counts in addition to credential/autofill counts, symlink counts, disabled preference booleans, inspected entry counts, and limit status. IPC and maintenance UI summaries expose only counts/booleans/labels/timestamps. `npm run test:sidecar-launch` now records sanitized headed Playwright evidence that both temporary Codex and Claude persistent profiles produced cookie-store artifacts under distinct service profiles without symlinks, default-profile import, raw profile paths, cookies, auth-looking material, or page markup in sidecar output. Authenticated cookie/session contents, saved-credential absence after login, and real authenticated page parsing remain unchecked.

Headless official refresh progress, 2026-06-04: split the Playwright sidecar into headed `launchLogin` for explicit user login and headless `refreshUsage` for normal official usage refresh checks. The desktop `Refresh usage`, `Refresh official`, and scheduled due-refresh paths now use the headless sidecar with the existing app-owned persistent profile, convert sanitized sidecar page state/visible fields through the web parser, update the normal provider cache/merge path, and report login-required states without flashing a visible browser. Scheduled headless web refreshes do not consume the manual web-refresh cooldown. `npm run test:official-fail-closed` validates blank Codex/Claude profiles return sanitized `logged_out` fail-closed states and a forced dead-proxy Codex refresh returns sanitized `network_unavailable`, all with `headless: true` and `visibleBrowserRequired: false` on CachyOS KDE/Wayland. Authenticated official parsing, post-login session persistence, and saved-credential validation remain unchecked.

Authenticated profile smoke helper progress, 2026-06-04: added `npm run smoke:auth-profile`, a manual post-login validation helper that accepts user-supplied dedicated Codex/Claude profile roots through CLI args or environment variables, requires ForgeGauge app-owned profile markers by default, performs only headless Playwright `refreshUsage` checks, inspects profile storage counts and disabled preference booleans without reading browser storage contents, and emits sanitized JSON without profile paths, official URLs, cookies, tokens, auth headers, page markup, or raw page content. Real profile runs should use `npm --silent run` or environment variables so npm does not echo CLI path arguments before the helper starts. The helper supports strict `--require-usage`, `--require-disabled-storage-preferences`, `--require-no-credential-store-files`, `--require-no-autofill-store-files`, `--require-no-default-profile-references`, and `--require-sanitized-log-file` modes for future authenticated validation. A temporary marker-owned blank Codex profile run validated the command path and returned sanitized `logged_out` with `visibleBrowserRequired: false`; a missing-marker profile fails with sanitized `missing_profile_marker` output; marker-owned profiles containing `Login Data`, `Web Data`, or a default-browser profile reference fail with sanitized `credential_store_detected`, `autofill_store_detected`, or `default_profile_reference_detected` output. Real authenticated profile evidence remains unchecked.

Authenticated session-artifact smoke progress, 2026-06-04: extended `npm run smoke:auth-profile` with strict `--require-session-storage-artifacts` mode for future post-login persistence validation. The mode requires the headless refresh to reach `usage` and the sanitized profile inspection to report cookie-store or site-storage artifact counts; logged-out or artifact-empty profiles fail with sanitized `session_artifacts_missing` output. Normal helper output also reports `authenticatedSessionEvidencePresent` so storage artifacts created by a logged-out page are not mistaken for authenticated persistence. This adds a repeatable future evidence gate but does not prove real authenticated persistence until run against logged-in app-owned profiles.

Authenticated log sanitization smoke progress, 2026-06-04: extended `npm run smoke:auth-profile` with `--log-file`, `FORGEGAUGE_AUTH_LOG_PATH`, and strict `--require-sanitized-log-file` mode. The helper scans the normal app log after headless profile refresh, emits only sanitized log-inspection booleans/counts, and fails with stable sanitized codes for missing, invalid, symlinked, oversized, or sensitive logs. Disposable marker-owned blank-profile smoke accepted a safe app-style log and rejected a log containing auth/page material with `sensitive_log_detected` without exposing temporary profile or log paths. Real authenticated app-log proof remains unchecked until run against logged-in app-owned profiles.

Refresh visibility regression progress, 2026-06-04: added a Rust app-boundary regression test proving the official web refresh sidecar request builder emits `refreshUsage` with `headless: true`, uses the service-specific app-owned profile label, omits `--user-data-dir` from Chromium args, and redacts the raw profile root from request debug diagnostics. This guards against future refresh paths accidentally opening headed Chromium; manual login remains the only headed sidecar action.

Login prompt visibility progress, 2026-06-04: the frontend now keeps `Refresh official` as the always-available silent check after web-provider opt-in and renders the headed `Start login` action only when the current web snapshot, or local fallback carrying `webStatus`, reports `login_required`. Vitest covers the prompt-visibility helper, and browser-preview Playwright validation now asserts default preview cards do not expose `Start login` while the expired-login state does after experimental web providers are enabled.

Login preflight progress, 2026-06-04: the desktop `Start login` command now performs a headless Playwright usage preflight before launching headed Chromium and returns sanitized `already_authenticated` status without opening a visible browser when the app-owned profile already reaches the usage page. Rust tests cover the preflight decision boundary and sanitized IPC status shape; real post-login preflight evidence still requires authenticated profiles.

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
- [x] Real local usage providers work without account credentials.
- [ ] Opt-in web providers use dedicated browser profiles and never store passwords.
Blocked: requires authenticated login/profile validation before real opt-in web providers can be claimed to use dedicated profiles and never store passwords.
- [x] Provider failures degrade to `unknown` or lower-confidence estimates instead of crashing.
- [x] Merged usage values combine official web baselines with calibrated local deltas.
- [ ] Full KDE/Wayland tray smoke test is confirmed by the user.
Blocked: requires user-visible CachyOS KDE/Wayland desktop smoke testing that cannot be proven through browser-preview tooling.
- [x] Remote release workflow is verified after a mainline push.

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
  - [x] Packaged KDE/XWayland smoke verifies the popup requests skip-taskbar and above/stays-on-top utility-window hints.
  - [x] Tray-click popup opening positions near the provided click coordinates and clamps to the active monitor work area when the platform supplies tray coordinates.
Blocked: remaining KDE/Wayland confirmation requires user-visible tray click and popup placement smoke.
- [ ] Full KDE/Wayland smoke test for tray visibility, popup open/close, settings persistence after restart, and quit behavior.
Blocked: requires user-visible CachyOS KDE/Wayland desktop smoke testing outside browser-preview tooling.
- [x] GitHub release workflow remote/mainline run verification.
- [ ] Windows artifact testing.
- [ ] macOS Intel artifact testing.
- [ ] macOS Apple Silicon artifact testing.
Blocked: requires access to Windows and macOS runtime environments or user-provided platform smoke results.
- [ ] Claude Code local provider calibration/statusline/ccusage completion.
Blocked: requires an explicit product decision for ccusage-style cost/block precision: embedded pricing source, shell out to `ccusage`, or keep cost/block precision out of ForgeGauge.
- [x] Codex local provider calibration/statusline/fixture completion.
- [x] Provider registry with scheduled refresh, backoff, and event streaming.
- [x] Shared display-state cache used by both tray rotation and frontend snapshots.
- [x] Snapshot cache for latest provider results.
- [x] Calibrated local quota/window merge deltas.
- [x] Config migration and atomic persistence layer.
- [x] Browser profile path configuration, validation, and ownership markers.
- [x] Browser profile cleanup guardrails.
- [ ] Browser automation spike for official usage pages.
- [x] Isolated browser session manager.
- [ ] Opt-in Codex web provider.
- [ ] Opt-in Claude web provider.
Blocked: requires manual authenticated profile/login validation before real web-provider parsing can be claimed complete.
- [x] Merge engine for web baselines plus local deltas.
- [x] Autostart setting.
- [x] Clear/delete actions for cached snapshots.
- [x] Clear/delete actions for browser session data.
- [x] Basic failure logging view or log file location.
- [x] Browser preview falls back cleanly when desktop-only Tauri APIs are unavailable.

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

- Current source has a **central `UsageEngine` with fake, Claude local, and Codex local providers, latest snapshot cache, shared display-state cache, and snapshot update event**; web providers and merge behavior are not implemented yet.
- Current tray values read from the shared display-state cache instead of hard-coded fake values in `src-tauri/src/lib.rs`.
- Current app window starts hidden from `src-tauri/tauri.conf.json`, can be shown from tray/menu actions, and hides back to tray on normal close requests; Phase 0.5/10 must still confirm KDE/Wayland tray behavior manually.
- Current Rust dependencies remain narrow: `serde`, `serde_json`, `rusqlite`, `tauri`, and `time`. Any async runtime, logging, filesystem walking, browser automation, opener, or path-dialog dependencies must be added deliberately with validation.
- Current Tauri permissions are only `core:default`; browser/session/open-url/path features will require explicit capability review before implementation.
- Current CSP is `null`; web-provider UI and any opener/browser integration must include a security review before release.

## Implementation Sequence

Build in this order to avoid rework:

1. **Tray/window hardening**
   - [x] Decide tray-first window lifecycle.
   - [ ] Validate KDE/Wayland tray and popup behavior.
   - [x] Add fallback close/dismiss behavior if focus-loss dismissal is unreliable.

2. **Core data plumbing**
   - [x] Introduce shared Rust/TypeScript app models.
   - [x] Introduce `UsageEngine` Tauri state.
   - [x] Introduce provider registry, fake provider, scheduler, and shared display-state cache.
   - [x] Wire tray and frontend to the same display state.

3. **Persistence hardening**
- [x] Add config migrations, atomic writes, rollback, and tests.
   - [x] Add browser profile config fields only after migration support exists.
   - [x] Add quota/window config fields only after migration support exists.

4. **Local providers**
   - [x] Discover Claude and Codex local data formats.
   - [x] Build parsers with sanitized fixtures.
   - [x] Return honest `unknown` or uncalibrated snapshots before adding calibrated percentages.
   - [x] Add manual calibration config and percent-delta support.

5. **Browser automation and web providers**
   - [ ] Complete automation spike and backend decision matrix.
   - [x] Implement isolated session manager.
   - [ ] Implement opt-in web providers and parser contracts.

6. **Merge and release readiness**
- [x] Implement merge engine.
   - [ ] Complete KDE smoke test.
   - [x] Verify remote release workflow and platform artifact uploads.

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
  - [x] Rust enum/string serialization is documented.
  - [x] TypeScript types mirror the IPC payloads.
  - [x] Any new field has a default, migration path, and UI fallback.
- User-facing precision must be justified.
  - [x] Show percentages only for official web values or calibrated local estimates.
  - [x] Show token/cost/activity summaries without percentages when local data cannot be mapped to plan limits.
  - [x] Never derive account-wide remaining usage from machine-local logs unless clearly labeled as partial.

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
- [x] Add stale metadata: `stale`, `stale_seconds`, `baseline_at`, and `last_official_check_at` where relevant.
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
- [x] `refresh_provider`
- [x] `open_official_usage_page`
- [x] `start_provider_login`
- [x] `hide_main_window`
- [x] `reset_provider_session`
- [x] `inspect_provider_profile`
- [x] `clear_cached_snapshots`
- [x] `clear_provider_profile`
- [x] `get_log_location`

### Emitted Events

- [x] `usage://snapshots-updated`
- [x] `usage://refresh-started`
- [x] `usage://refresh-finished`
- [x] `usage://provider-error`
- [x] `settings://updated`
- [x] `login://required`
- [x] `session://reset`

### IPC Safety Rules

- [x] IPC returns app models only.
- [x] IPC never returns raw browser profile content.
- [x] IPC never returns raw local log contents.
- [x] IPC never returns raw page HTML/text from authenticated pages.
- [x] Every command has a stable error shape for frontend rendering.
- [x] Profile inspection IPC returns only counts, booleans, timestamps, service values, and sanitized profile labels.

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
- [x] Which records are stable enough to parse?
- [x] Which fields are timestamps, model names, token counts, costs, sessions, or status values?
- [x] Which records are machine-local only versus account-wide?
- [x] What source precedence should apply when multiple local sources exist?
- [x] How should missing directories, unreadable files, invalid records, large files, and rotated logs behave?
- [x] Which test fixtures can be safely sanitized and committed?
- [x] Which user-entered calibration fields are required to map local activity to percentages?
- [x] What does the provider return when calibration is absent?
- [x] What exact `details` metadata is safe and useful for debugging?

Default local-provider output before calibration:

- [x] `source = "local"`
- [x] `confidence = "low"` when activity is parsed but no reliable percentage exists.
- [x] `confidence = "unknown"` when files are missing, unsupported, unreadable, or stale.
- [x] `remaining_percent = None` unless a configured/calibrated quota makes a percentage defensible.
- [x] `details` contains sanitized counts, window metadata, and status codes only.

## Browser and Web Provider Integration Contract

Web providers are allowed only after the automation spike proves a safe backend.

- [x] The chosen backend supports visible manual login.
  - [x] Generated Playwright sidecar launches both official URLs in headed mode with temporary isolated profiles.
- [x] The chosen backend supports persistent isolated profiles per service.
  - [x] Generated Playwright sidecar relaunches Codex and Claude with distinct temporary profile directories and preserves profile sentinels across relaunch.
- [x] The chosen backend does not import default browser cookies, credentials, or profiles.
  - [x] Generated Playwright sidecar launches with seeded fake default Chrome/Chromium profiles under a temporary HOME and verifies cookie, credential, autofill, preference, and profile sentinels are absent from the generated `userDataDir`.
- [x] The chosen backend can disable or avoid password saving/autofill prompts.
  - [x] Generated Playwright sidecar preserves disabled Chromium password/autofill preferences across real headed relaunch.
- [x] Authenticated official pages are never loaded in the main Tauri webview.
- [x] Normal official refresh checks use headless Playwright; visible Chromium is reserved for explicit login.
  - [x] `npm run test:official-fail-closed` validates blank profiles are checked with `headless: true` and `visibleBrowserRequired: false`.
  - [x] Scheduled due-refresh web checks use the same headless Playwright sidecar without consuming manual refresh cooldown.
  - [x] Rust unit coverage asserts the app-side official refresh request builder uses `refreshUsage` with `headless: true` and never passes `--user-data-dir` through browser args.
  - [x] Frontend only renders `Start login` after a web status of `login_required`; default/parsed/non-login web states keep the visible browser action hidden.
  - [x] Desktop `Start login` performs a headless preflight and skips headed Chromium when the usage page is already reachable.
- [x] Browser launch arguments and profile paths are logged only in sanitized form.
  - [x] Backend-agnostic Chromium launch diagnostics redact raw `--user-data-dir` paths to service profile labels.
  - [x] Browser launch plan debug output redacts raw profile paths and raw `--user-data-dir` arguments.
- [x] Parser input is sanitized visible text or structured state, not raw authenticated HTML.
- [x] Fixture regeneration requires explicit user-consented capture.
- [x] Fixtures are sanitized before writing.
- [ ] Web providers fail closed on login, MFA, CAPTCHA, bot checks, unexpected UI, or parse failure.
  - [x] Parser contract returns unknown snapshots for logged-out, MFA, CAPTCHA/bot-check, network unavailable, timeout, unexpected UI, missing visible data, and parse failures.
  - [x] Display merge keeps local data visible and carries sanitized `webStatus`/`webReason` metadata for fail-closed web states.
  - [x] Real headless Playwright official refresh smoke validates blank Codex/Claude profiles return sanitized `logged_out` fail-closed states without opening a visible browser.
  - [x] Real headless Playwright official refresh smoke validates a forced network failure returns sanitized `network_unavailable` without opening a visible browser.
Blocked: real browser-backed MFA, CAPTCHA, authenticated-expiry, and unexpected-UI validation still requires authenticated/manual provider smoke tests.
- [x] Web refreshes never run until the user explicitly enables experimental web providers.

## Logging and Diagnostics Policy

- [x] Add structured provider status/error codes.
- [x] Prefer stable status codes over raw error messages.
- [x] Redact home directory paths when they are not needed for debugging.
- [x] Never log cookies, tokens, auth headers, raw page HTML, raw authenticated text, account identifiers, or full browser errors.
- [x] Add a UI-visible log location only after log redaction policy exists.
- [x] Include enough diagnostics to distinguish disabled, missing data, stale, parse failed, login required, and unavailable states.

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
  - [x] KDE DBusMenu smoke verifies `Show ForgeGauge` opens a visible ForgeGauge window.
- [ ] Confirm popup closes reliably or has acceptable fallback behavior.
  - [x] KDE/XWayland smoke verifies a window close request removes the visible window while keeping the process and tray item alive.
  - [x] KDE/XWayland smoke verifies focus loss hides the popup while keeping the process and tray item alive.
- [ ] Confirm app can run as tray-first utility without an always-visible main window.
  - [x] KDE/XWayland smoke verifies the isolated AppImage starts with no visible ForgeGauge window before `Show ForgeGauge`.
- [ ] Confirm tray gauge alternates between enabled services.
  - [x] KDE StatusNotifier smoke verifies the AppImage tray `IconName` updates and exported PNG colors rotate between `Codex` and `Claude Code` with deterministic enabled services.
- [ ] Confirm close button either hides to tray or exits only when explicitly intended.
  - [x] KDE/XWayland smoke verifies a close request does not exit the app and `Show ForgeGauge` can reopen/recreate the window afterward.
- [ ] Confirm popup/window position is acceptable on single-monitor and multi-monitor KDE setups.
  - [x] KDE/XWayland smoke verifies the popup requests skip-taskbar and above/stays-on-top window-manager hints.
  - [x] Rust tests cover tray-anchor popup placement for bottom-right, top-edge, negative-origin, and constrained work-area layouts.
- [ ] Confirm settings persist after restart.
  - [x] KDE/AppImage smoke verifies current-schema config creation and persisted service-toggle/gauge-interval values survive an isolated packaged restart.
- [ ] Confirm quit behavior.
  - [x] KDE DBusMenu smoke verifies the tray `Quit` item exits the isolated AppImage process and unregisters the tray item.
- [x] Document runtime packages and packaging prerequisites discovered during testing.
- [x] Choose fallback behavior if native tray/popup behavior is unreliable.

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
- [x] Add explicit popup close/click-outside fallback if KDE/Wayland smoke test requires it.

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
- [x] Add optional manual plan/quota/window configuration for local estimates.
- [x] Define quota/window schema per service:
  - [x] plan label
  - [x] limit kind
  - [x] reset/window duration
  - [x] usage unit
  - [x] user-entered limit
  - [x] enabled flag
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
- [x] Add refresh scheduler for local and web providers.
- [x] Add latest snapshot cache.
- [x] Add shared display-state cache consumed by both tray rotation and frontend commands.
- [x] Replace hard-coded tray fake values in `lib.rs` with cached display state.
- [x] Add Tauri commands/events for frontend usage updates.
- [x] Define provider IDs for `codex.local`, `codex.web`, `claude.local`, `claude.web`, and `fake`.
- [x] Define provider timeout behavior.
- [x] Define provider cancellation behavior.
- [x] Define mockable clock/time source for tests.
- [x] Ensure one active refresh per provider.
- [x] Skip overlapping scheduled refresh ticks.
- [x] Cancel pending refreshes when a provider is disabled.
- [x] Enforce local and web refresh cadence from config.
- [x] Enforce manual web-refresh cooldown and provider opt-in.
- [x] Document Tokio task ownership in scheduler module.
- [x] Add per-provider failure counters with bounded retry/backoff.
- [x] Reset retry/backoff state on provider success.
- [x] Add sanitized tracing/logging policy for provider lifecycle events.
- [x] Add unit tests for scheduler timing boundaries.
- [x] Add unit tests for overlap skipping, disable cancellation, retry/backoff reset, and stale snapshots.

### Phase 5 — Claude Code Local Provider

- [x] Complete read-only discovery of available Claude Code local data shapes.
- [x] Record source precedence order for Claude local data.
- [x] Add injectable Claude data root for tests and development.
- [x] Discover Claude Code local usage files.
- [x] Parse `~/.claude/projects/**/*.jsonl` where available.
- [x] Inspect Claude Code statusline-compatible data if available.
- [x] Support ccusage-compatible parsing where practical.
- [ ] Parse timestamps, model, input/output/cache tokens, session blocks, estimated cost/usage, and rolling window activity.
  - [x] Aggregate numeric Claude `server_tool_use` usage counts without exposing raw server-tool fields.
Blocked: current local Claude JSONL parsing covers timestamps, model/session counts, token classes, and calibrated rolling-window activity, but ccusage-style cost and billing-block output needs an explicit decision to add a pricing source, shell out to `ccusage`, or keep cost/block precision out of ForgeGauge.
- [x] Define file scanning limits for large logs and many project directories.
- [x] Define rotated/truncated file behavior.
- [x] Define invalid JSONL line behavior.
- [x] Define timezone and rolling-window semantics.
- [x] Produce local estimated Claude usage snapshot.
- [x] Support manual quota/window calibration.
- [x] Expose calibrated percentage deltas only when records map to the current plan/window.
- [x] Return `remaining_percent = None` instead of inventing precision when logs cannot be mapped reliably.
- [x] Gracefully handle missing files and unexpected log shapes.
- [x] Add parser tests with sanitized JSONL fixtures.
- [x] Add missing-directory test.
- [x] Add calibrated and uncalibrated local estimate tests.

### Phase 6 — Codex Local Provider

- [x] Complete read-only discovery of available Codex local data shapes.
- [x] Record source precedence order for Codex local data.
- [x] Add injectable Codex data root for tests and development.
- [x] Inspect available `~/.codex/*` local/session/status files.
- [x] Inspect Codex statusline or `/status`-derived data if available.
- [x] Define file scanning limits for large logs and many sessions.
- [x] Define rotated/truncated file behavior.
- [x] Define invalid record behavior.
- [x] Define timezone and rolling-window semantics.
- [x] Produce local estimated Codex usage snapshot when possible.
- [x] Mark confidence conservatively.
- [x] Support manual quota/window calibration.
- [x] Expose calibrated percentage deltas only when records map to the current plan/window.
- [x] Return `remaining_percent = None` instead of inventing precision when local data is incomplete or stale.
- [x] Add parser tests with captured/sanitized fixture data.
- [x] Add missing-directory test.
- [x] Add calibrated and uncalibrated local estimate tests.

### Phase 6.5 — Browser Automation Spike

- [x] Select browser automation backend.
  - [x] User approved Playwright headed Chromium sidecar on 2026-06-04.
- [x] Compare Playwright, WebDriver, and lightweight browser-control alternatives.
- [x] Record decision matrix scores for KDE/Wayland support, persistent profiles, packaging cost, parser access, security controls, and maintainability.
- [x] Validate persistent isolated profile on CachyOS KDE/Wayland.
  - [x] `npm run test:sidecar-launch` validates headed Playwright profile persistence across relaunch for both official URLs with temporary isolated profiles in the current CachyOS KDE/Wayland session.
- [x] Validate separate app-owned profile directories/cookie jars per service.
  - [x] `cargo test browser_profile --lib` verifies app-owned default paths, ownership markers, restrictive permissions, default-browser path rejection, and distinct/non-nested service profile roots.
  - [x] `cargo test browser_session --lib` verifies service launch plans use distinct profile paths/labels and profile storage inspection remains sanitized.
  - [x] `npm run test:sidecar-launch` reports `cookieStoreArtifactsDetectedForAllServices: true` and per-service cookie-store artifact counts for distinct temporary Codex/Claude persistent profiles.
- [x] Prove there is no import from default browser profiles.
  - [x] Validate headed Playwright sidecar launches against fake default Chrome/Chromium profile sentinels without reading real browser profile contents.
- [ ] Prove visible manual login works for both services.
- [ ] Prove isolated session persistence survives app restart.
  - [x] Add `npm run smoke:auth-profile -- --require-session-storage-artifacts` strict mode to fail sanitized future post-login checks when usage is not reached or no cookie/site-storage artifacts are present.
- [ ] Prove each official URL exposes parseable visible fields for the snapshot contract.
- [x] Define parser contract and partial/no-data fallback behavior.
- [x] Document runtime/package dependencies.
- [x] Record chosen backend, rejected alternatives, decision matrix, and proceed/defer decision.
  - [x] Chosen backend is Playwright headed Chromium sidecar; web providers remain gated on manual authenticated CachyOS KDE/Wayland testing.
- [x] Disable password manager, autofill, and save-password prompts or defer web providers.
- [ ] Prove fail-closed handling for logged-out, MFA, CAPTCHA, and unexpected UI states.
  - [x] Parser fixtures and tests cover logged-out, MFA, CAPTCHA/bot-check, unexpected UI, network unavailable, timeout, missing visible data, and parse-failure states.
  - [x] Display merge and browser-preview fixtures keep local data visible and surface sanitized fail-closed web status notes without horizontal overflow.
  - [x] `npm run test:official-fail-closed` validates real headless Playwright official refreshes for blank Codex/Claude profiles return sanitized `logged_out` states without opening a visible browser.
  - [x] `npm run test:official-fail-closed` validates a real headless Playwright official refresh with a forced dead proxy returns sanitized `network_unavailable` without opening a visible browser.
Blocked: real browser-backed MFA, CAPTCHA, authenticated-expiry, and unexpected-UI validation still requires authenticated/manual provider smoke tests.
- [ ] Confirm no saved credentials are present in dedicated profiles after login tests.
  - [x] Add `npm run smoke:auth-profile -- --require-no-credential-store-files` strict mode to fail sanitized future post-login checks when Chromium credential-store files are present.
  - [x] Add `npm run smoke:auth-profile -- --require-no-autofill-store-files` strict mode to fail sanitized future post-login checks when Chromium autofill-store files are present.
  - [x] Add `npm run smoke:auth-profile -- --require-no-default-profile-references` strict mode to fail sanitized future post-login checks when Chromium preferences reference default browser profile paths.
- [ ] Confirm no sensitive page content is written to normal logs.
  - [x] Validate real headed Playwright sidecar stdout/stderr omit raw profile paths, official URLs, launch flags, default-profile sentinels, auth/cookie-looking material, and page markup.
  - [x] Add `npm run smoke:auth-profile` to validate future authenticated app-owned profile refreshes with sanitized headless output and no raw paths, URLs, auth material, browser storage contents, or page markup.
  - [x] Add `npm run smoke:auth-profile -- --require-sanitized-log-file` to validate future authenticated runs against the normal app log without returning log paths or log contents.
- [x] Confirm authenticated official pages are never loaded in the main Tauri webview.
- [x] Identify required Tauri capabilities/plugins for opening URLs, launching child processes, choosing paths, and showing login windows.
- [x] Review CSP and permissions needed before implementing provider UI.

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
- [x] Maintain separate cookie jar/session state per service.
  - [x] Reject identical, nested, or root-overlapping configured service profile paths before profile creation.
- [x] Track managed child process ownership per service with PID/handle metadata.
- [x] Add graceful browser shutdown with timeout/kill fallback.
- [x] Detect orphaned managed browser processes on startup.
- [x] Disable password manager, autofill, and save-password prompts where supported.
  - [x] Add backend-agnostic Chromium launch policy with password-manager/autofill suppression flags and disabled storage preferences.
  - [x] Initialize Chromium profile preferences with disabled password, autosign-in, profile autofill, and card autofill settings.
  - [x] Wire Chromium preference initialization into managed browser profile preparation.
  - [x] Count Chromium autofill store artifacts without reading store contents.
  - [x] Map Chromium launch policy to Playwright persistent-context launch request with sanitized diagnostics.
  - [x] Validate real headed Playwright sidecar launches preserve disabled password/autofill preferences across relaunch.
- [ ] Add manual login window flow.
Blocked: requires manual CachyOS KDE/Wayland login validation with installed Node/Playwright runtime before claiming the real managed browser launch/login UI is complete.
  - [x] Prepare managed browser profiles and Chromium preferences before returning the fail-closed login-required boundary.
  - [x] Return sanitized Playwright backend/profile metadata from login-start IPC without raw profile paths.
  - [x] Add tested Playwright sidecar JSON launch protocol with sanitized dry-run responses.
  - [x] Add Rust serializer for the Playwright sidecar `launchLogin` stdin request.
  - [x] Wire `start_provider_login` to a Rust-owned Tauri shell sidecar spawn path with sanitized response parsing and fail-closed fallback.
  - [x] Register and package the Linux target-triple Playwright sidecar executable through Tauri `externalBin`.
  - [x] Validate headed Playwright sidecar launch to both official URLs with temporary isolated profiles.
  - [x] Validate generated sidecar profile persistence across relaunch for distinct Codex and Claude temporary profile directories.
- [x] Surface login-required state to UI.
- [x] Add session reset/logout action.
- [x] Add guarded clear/delete action for browser profile data.
- [x] Stop managed browser before deleting browser session data.
- [x] Delete only marker-owned paths after deletion-time canonicalization, symlink rejection, marker verification, and live-process checks.
- [x] Add negative tests for unsafe browser profile paths.
- [x] Add tests for browser shutdown, orphan detection, and cleanup refusal.
- [x] Verify profile/cache paths use restrictive local permissions where supported.
- [x] Add manual inspection checklist proving profile directories contain no saved credentials after login tests.
  - [x] Add sanitized managed-profile storage inspector for credential artifact and preference checks.
  - [x] Expose sanitized profile inspection through IPC and maintenance UI.

### Phase 8 — Web Providers

- [ ] Add Codex web provider for the Codex analytics URL.
- [ ] Add Claude web provider for the Claude usage URL.
  - [x] Add headless Playwright `refreshUsage` sidecar action for normal official refresh checks.
  - [x] Wire desktop `Refresh official` through headless sidecar results and the existing sanitized web parser/cache path.
  - [x] Wire scheduled due-refresh web checks through headless sidecar results without visible browser launch.
  - [x] Keep headed Chromium limited to explicit `Start login`.
  - [x] Add app-boundary regression coverage for the headless official refresh request shape.
  - [x] Hide `Start login` until a silent official refresh/fallback web status reports `login_required`.
  - [x] Add a headless `Start login` preflight that returns `already_authenticated` without launching headed Chromium when usage is reachable.
- [x] Add fail-closed web provider boundary before browser backend selection.
- [x] Parse visible usage fields only.
- [x] Define exact visible fields required for each provider before parsing implementation.
- [x] Define fallback behavior when only partial visible data exists.
- [x] Define parser input format as sanitized visible text/structured accessibility snapshot, not raw authenticated HTML.
- [x] Implement documented visible-data parser contract for each provider.
- [x] Return `unknown` or lower-confidence snapshot for partial/no visible usage data.
- [x] Avoid inventing precision on parse failures.
- [x] Surface parse failures in UI without crashing.
- [x] Add manual "Refresh official usage" action.
- [x] Add sanitized parser fixtures for every implemented web provider.
- [x] Add fixture update workflow based on explicit user-consented manual captures.
- [x] Reject raw page HTML, account identifiers, cookies, tokens, auth headers, and unsanitized browser errors from fixtures.
- [ ] Add manual authenticated refresh smoke test for each service.
  - [x] Add `npm run smoke:auth-profile` as a repeatable sanitized manual post-login refresh/profile smoke helper.
- [x] Add parser tests for successful usage read.
- [x] Add parser tests for partial visible data.
- [x] Add parser tests for logged-out page.
- [x] Add parser tests for MFA/CAPTCHA/interruption page.
- [x] Add parser tests for network-unavailable and timeout states.
- [x] Add parser tests for unexpected UI.
- [x] Add parser tests for parse failure.
- [x] Add fixture sanitization tests or review checks.
- [x] Add provider-level tests proving web providers do not run unless explicitly enabled.

### Phase 9 — Merge Engine

- [x] Merge web baseline with local deltas.
- [x] Detect stale web baselines.
- [x] Expose final per-service display state.
- [x] Explain source/confidence in popup.
- [x] Preserve baseline timestamp semantics.
- [x] Apply only post-baseline local deltas.
- [x] Avoid double-counting local deltas.
- [x] Clamp output percentages to `0..=100`.
- [x] Apply local deltas only when the provider reports a calibrated percentage delta for the relevant baseline window.
- [x] Keep web baseline unchanged with lower confidence/stale messaging when local deltas are unavailable or incompatible.
- [x] Add unit tests for web-only data.
- [x] Add unit tests for local-only data.
- [x] Add unit tests for web plus local delta.
- [x] Add unit tests for no double-count across refreshed baselines.
- [x] Add unit tests for stale web baseline behavior.
- [x] Add unit tests for unavailable `can_produce_percent_delta()` fallback.
- [x] Add unit tests for unknown data.
- [x] Ensure popup and tray use merged data consistently.

### Phase 10 — KDE Polish and Cross-Platform Packaging

- [x] Add Linux AppImage packaging path.
- [x] Add automated release workflow for Linux AppImage, Windows, macOS Intel, and macOS Apple Silicon artifacts.
- [x] Mark Windows/macOS artifacts as untested.
- [ ] Complete manual CachyOS KDE smoke test.
- [ ] Verify launch app.
- [ ] Verify tray appears.
- [ ] Verify popup opens/closes.
- [ ] Verify gauge alternates.
  - [x] KDE StatusNotifier smoke verifies sanitized D-Bus icon updates and rendered PNG color rotation between `Codex` and `Claude Code` under deterministic data.
- [ ] Verify settings persist.
- [ ] Verify providers fail gracefully.
Blocked: requires user-visible CachyOS KDE/Wayland desktop smoke testing.
- [x] Verify queued release workflow runs on mainline push.
- [x] Run or trigger release workflow on `main` or through `workflow_dispatch`.
- [x] Confirm draft release is created with expected `forgegauge-v<version>-<run>.<attempt>` tag.
- [x] Verify Linux AppImage artifact uploads.
- [x] Verify Windows artifact uploads.
- [x] Verify macOS Intel artifact uploads.
- [x] Verify macOS Apple Silicon artifact uploads.
- [x] Confirm release is published only after all build matrix jobs succeed.
- [x] Record any failing runner labels, action versions, package dependencies, or upload paths.
- [x] Add optional autostart setting.
- [x] Add basic failure logging view or log file location.

## UI Requirements

### Tray UI

- [x] One tray icon is present.
- [x] Tray alternates between Codex and Claude.
- [x] Blue/brand Codex asset is available.
- [x] Orange/brand Claude asset is available.
- [x] Gray/unknown state asset is available.
- [x] Red/low state asset is available.
- [x] Dynamic percentage gauge icons.
- [ ] Confirm tray behavior on CachyOS KDE/Wayland.

### Popup UI

- [x] Shows Codex usage card.
- [x] Shows Claude Code usage card.
- [x] Shows remaining percentage.
- [x] Shows source.
- [x] Shows confidence.
- [x] Shows last update text.
- [x] Shows settings controls.
- [x] Shows last official check when web provider exists.
- [x] Shows stale data messaging.
- [x] Shows login-required state.
- [x] Adds "Refresh now" action.
- [x] Adds "Open official Codex page" action.
- [x] Adds "Open official Claude usage page" action.
- [x] Adds guarded reset/clear actions.

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
- [x] Configure optional manual plan/limit/window values.
- [x] Configure autostart.
- [x] Reset browser session data.
- [x] Clear cached usage data.
- [x] Inspect dedicated browser profile state without exposing paths or contents.

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
- [x] Add `npm run lint` if linting is configured later.

### Validation Command Matrix

Use the smallest relevant set during iteration, then run the milestone set before marking a phase complete.

| Change type | Commands |
| --- | --- |
| Documentation only | `git diff --check`, `npm run check` |
| Frontend/Svelte | `npm run lint`, `npm run check`, `npm run build` |
| Browser preview/UI smoke | `npm run test:browser-preview` |
| Headless official web smoke | `npm run test:official-fail-closed` |
| Authenticated profile smoke | `npm --silent run smoke:auth-profile -- --codex-profile <profile> --claude-profile <profile> --log-file <forgegauge-log> --require-usage --require-session-storage-artifacts --require-sanitized-log-file --require-disabled-storage-preferences --require-no-credential-store-files --require-no-autofill-store-files --require-no-default-profile-references` |
| Manual smoke preflight | `npm run smoke:preflight` |
| KDE tray D-Bus registration/menu/window smoke | `npm run smoke:kde-tray` |
| Rust backend | `cd src-tauri && cargo fmt --check`, `cd src-tauri && cargo check`, `cd src-tauri && cargo clippy -- -D warnings`, `cd src-tauri && cargo test` |
| Tauri integration | `npm run check`, `npm run build`, `cd src-tauri && cargo check`, `npm run tauri -- build --bundles appimage` or `npm run build:appimage` on CachyOS/Arch-like systems |
| Release workflow | Local validators plus a real GitHub Actions `workflow_dispatch` or mainline run |

### Required Evidence Before Checking Items

- [x] For implemented code: commit/diff evidence exists in source.
- [x] For automated validation: command and pass/fail result are recorded in the session or relevant commit notes.
- [ ] For manual KDE checks: date/session, OS/session type, artifact/binary used, and observed behavior are recorded.
  - [x] Add sanitized `npm run smoke:preflight` command to collect date/session, OS/session, commit, artifact, and runtime metadata without full local paths or secrets.
  - [x] Add sanitized preflight templates listing the required KDE/auth/platform manual observation fields without collecting secrets or raw local paths.
  - [x] Add sanitized preflight booleans for KDE smoke dependency availability and StatusNotifier host registration.
- [x] For release checks: workflow run URL, release tag, and artifact names are recorded.
- [ ] For web/session security checks: sanitized inspection notes confirm no secrets or raw authenticated page content are persisted outside browser profiles.
  - [x] Add `npm run smoke:auth-profile` to emit sanitized post-login app-owned profile/refresh evidence without raw paths, URLs, auth material, browser storage contents, or page markup.
  - [x] Update the authenticated-login inspection checklist to require sanitized normal app-log inspection alongside profile/storage checks.

### Automated Tests To Add

- [x] Config serialization.
- [x] Config migration ordering and failed migration rollback.
- [x] Default web-provider opt-out behavior.
- [x] Refresh interval validation/clamping.
- [x] Manual web-refresh cooldown enforcement.
- [x] Provider enable/disable scheduler behavior.
- [x] Provider parsing.
- [x] Local quota/window calibration.
- [x] Merge logic.
- [x] Merge fallback when local providers cannot produce a percentage delta.
- [x] No-double-count and stale-baseline merge behavior.
- [x] Stale data handling.
- [x] Gauge state mapping.
- [x] Frontend display formatting.
- [x] Frontend confidence/source labels.
- [x] Frontend settings form behavior.
- [x] Frontend web-provider opt-in toggles and disabled states.
- [x] Frontend browser-preview status fixtures for missing local data, network unavailable, expired login, MFA, CAPTCHA/bot-check, unexpected UI, timeout, parse failure, stale data, provider unavailable, permission denied, unsafe profile path, and disabled-provider states.
- [x] Repeatable Playwright browser-preview validation script for desktop/mobile preview states, status notes, overflow, and web-control fallback behavior.
- [x] Sanitized manual-smoke preflight command for future KDE/auth/platform evidence metadata.
- [x] KDE StatusNotifier tray registration, DBusMenu quit, XWayland show/close/reopen, and isolated packaged-restart config-persistence smoke command for AppImage launch validation.
- [x] Sanitized authenticated-profile smoke helper for future post-login headless refresh and marker-owned dedicated-profile evidence.

### Manual Tests To Complete

- [ ] KDE tray visibility.
  - [x] KDE StatusNotifier smoke verifies the AppImage registers an active ForgeGauge tray item over D-Bus with isolated XDG dirs.
- [ ] Popup position and dismissal behavior.
  - [x] KDE/XWayland smoke verifies tray `Show ForgeGauge` opens a visible window, close removes it without exiting, and `Show ForgeGauge` reopens/recreates it.
  - [x] KDE/XWayland smoke verifies the popup requests skip-taskbar and above/stays-on-top window-manager hints.
  - [x] KDE/XWayland smoke verifies focus loss hides the popup while keeping the process and tray item alive.
  - [x] Rust tests cover tray-anchor popup placement and work-area clamping when tray click coordinates are available.
- [ ] Settings persistence after restart.
  - [x] KDE/AppImage smoke verifies isolated config creation and persisted service-toggle/gauge-interval values survive packaged restart.
- [ ] Dedicated browser login.
  - [x] `npm run smoke:auth-profile -- --help` documents the manual post-login profile validation command, including `npm --silent` guidance for real profile paths, without opening a visible browser.
- [ ] Official Codex page refresh.
- [ ] Official Claude usage page refresh.
- [ ] Network unavailable state.
  - [x] Headless official smoke validates a forced network-unavailable Codex refresh without visible browser launch.
  - [x] Browser-preview fixture renders `Network unavailable` notes for both services at desktop and mobile widths without horizontal overflow.
- [ ] Missing local data state.
  - [x] Browser-preview fixture renders `No usage data found` notes for both services at desktop and mobile widths without horizontal overflow.
- [ ] Expired login state.
  - [x] Browser-preview fixture renders `Login required` notes for both services at desktop and mobile widths without horizontal overflow.
- [ ] Provider interruption states.
  - [x] Browser-preview fixtures render `MFA required`, `Additional verification required`, `Unexpected usage page`, `Usage refresh timed out`, and `Usage data could not be parsed` notes for both services at desktop and mobile widths without horizontal overflow.
- [ ] Provider unavailable/blocked states.
  - [x] Browser-preview fixtures render `Stale data`, `Provider unavailable`, `Usage data is not readable`, `Profile path blocked`, and `Provider disabled` notes for both services at desktop and mobile widths without horizontal overflow.
- [ ] Quit behavior.
  - [x] KDE DBusMenu smoke verifies the tray `Quit` item exits the isolated AppImage process and unregisters the tray item.
- [ ] Windows tray/install smoke test.
- [ ] macOS tray/install smoke test.
Blocked: KDE checks require user-visible CachyOS KDE/Wayland interaction, browser checks require approved backend plus authenticated provider state, and Windows/macOS checks require those platform runtimes.

## Security and Privacy Checklist

- [x] Web scraping is opt-in in default config.
- [x] Web providers can be disabled.
- [x] Web providers default to disabled.
- [x] Current fake provider does not read or upload account data.
- [ ] No password storage.
  - [x] Chromium managed-profile initialization disables password saving and autosign-in preferences before a future launch.
  - [x] Sanitized profile inspection counts Chromium password and autofill store artifacts without reading store contents.
- [x] Managed browser launch disables password manager/autofill/save-password prompts where supported.
  - [x] Chromium managed-profile initialization writes disabled autofill/password preferences with restrictive permissions.
  - [x] Real headed Playwright sidecar launch preserves disabled password/autofill preferences across relaunch.
- [x] Dedicated browser profiles are separate per service.
  - [x] Configured profile overrides cannot make Codex and Claude share or nest service profile paths.
  - [x] Headed Playwright sidecar validation reports cookie-store artifacts in distinct temporary Codex/Claude persistent profiles.
- [x] Dedicated browser profiles are app-owned and marker-guarded.
- [x] Dedicated browser profiles never use the user's default browser profile.
- [x] Clear/delete actions stop managed browser processes first.
- [x] Clear/delete actions only delete marker-owned paths.
- [x] Clear/delete actions reject symlinked paths.
- [x] Clear/delete actions re-verify canonical app-owned marker paths immediately before deletion.
- [ ] Dedicated profiles contain no saved credentials after login validation.
  - [x] Add sanitized credential/autofill-artifact and preference inspector for future login validation evidence.
Blocked: requires authenticated login validation with the selected browser backend.
- [ ] No logging cookies, session tokens, auth headers, or sensitive page HTML.
  - [x] Profile inspection IPC returns only sanitized counts, booleans, timestamps, service values, and profile labels.
  - [x] Profile inspection and sidecar launch evidence count cookie/site-storage artifacts without returning artifact names, cookie rows, storage contents, raw paths, or page content.
  - [x] Real headed Playwright sidecar launch validation checks stdout/stderr for sanitized output without raw launch data, seeded profile sentinels, auth/cookie-looking material, or page markup.
  - [x] Real headless official refresh validation checks sidecar stdout/stderr for sanitized output without raw URLs, launch flags, profile paths, auth/cookie-looking material, or page markup.
  - [x] Authenticated-profile smoke helper is available for future real-profile validation, requires app-owned profile markers by default, and its marker-owned blank-profile test run emitted sanitized headless output without raw paths, URLs, auth material, browser storage contents, or page markup.
  - [x] Authenticated-profile smoke helper can require a sanitized normal app log file and fails closed on sensitive auth/page material without returning log paths or log contents.
Blocked: real authenticated refresh logging proof requires logged-in app-owned profiles and manual authenticated provider smoke.
- [x] Browser profile is isolated from the main browser profile.
- [x] Scheduler does not start web refreshes until explicit opt-in.
- [x] Disabling a web provider cancels future scheduled reads.
- [x] Clear UI label for experimental web provider.
- [x] User can reset/delete provider session data and cached snapshots.
- [x] Local app data uses restrictive file permissions where supported.
- [x] `details` metadata is sanitized and never contains raw page content or secrets.
- [x] Test fixtures are sanitized before being committed or shared.
- [x] Fixture regeneration requires explicit user-consented captures.
- [x] Provider errors are sanitized before display/logging.

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
- [x] After Phase 4: Confirm app architecture is stable before real providers.
- [ ] After Phase 6: Confirm local providers provide enough value to keep.
- [ ] After Phase 6.5: Confirm browser automation backend is viable before implementing web providers.
- [ ] After Phase 8: Confirm web provider reliability is acceptable for personal use.
- [ ] Before packaging: Run automated checks, complete KDE manual smoke test, and confirm release notes mark Windows/macOS artifacts as untested.
Blocked: review gates require user approval, user-visible KDE validation, authenticated web-provider validation, or platform smoke results as applicable.

## MVP Cut Line

- [x] Tray shell.
- [x] Popup.
- [x] Branded tray state icons.
- [x] Config.
- [x] Usage snapshot model and fake snapshot command.
- [x] Fake provider.
- [x] At least one real local provider.
- [x] Central usage engine, provider registry, scheduler, shared display cache, and event stream.

Web providers should follow once KDE tray behavior and manual login/profile isolation checks are stable; backend selection is complete.

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
- [x] Add explicit popup close/click-outside fallback if KDE/Wayland smoke test requires it.
- [ ] Complete KDE/Wayland smoke checks for tray visibility, popup open/close, settings persistence after restart, and quit behavior.
- [x] Record runtime packages and packaging prerequisites discovered during testing.

Blocked: KDE/Wayland tray visibility, tray click, close-button, and quit-behavior confirmation requires user-visible desktop interaction and cannot be verified through the available Playwright/browser-preview tooling in this session.

Blocked: Playwright sidecar implementation and local launch validation are complete, but manual CachyOS KDE/Wayland login/profile validation remains required before implementing real web-provider refresh flows. The backend-agnostic process stop guard and startup orphan detection exist; authenticated app-owned profile persistence validation remains unchecked.

Blocked: current-feature release verification still requires pushing or dispatching this feature branch through the release workflow, and Windows/macOS install behavior still requires manual platform smoke testing. Remote `main` workflow execution and artifact upload verification are recorded.
