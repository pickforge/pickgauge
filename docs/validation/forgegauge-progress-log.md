# ForgeGauge Progress Validation Log

## 2026-06-04 America/Sao_Paulo

Branch: `forgegauge-implementation`

Official network fail-closed coverage:

- Extended `npm run test:official-fail-closed` so forced dead-proxy headless `refreshUsage` checks run for both Codex and Claude temporary profiles.
- The smoke now validates blank-profile `logged_out` and forced `network_unavailable` states for both services with `headlessRefresh = true`, `visibleBrowserRequired = false`, and sanitized sidecar output.
- Validation: `node --check scripts/validate-playwright-official-fail-closed.mjs`, `npm run lint`, `npm run check`, `npm test`, `npm run build`, `npm run test:official-fail-closed`, `npm run test:browser-preview`, and `git diff --check` passed.
- Remaining caveat: this proves real official blank-profile and network-unavailable fail-closed behavior; authenticated official parsing and real provider interruption pages still require logged-in app-owned profiles.

Authenticated helper fail-fast coverage:

- Moved strict authenticated-profile smoke storage/preference checks before Playwright refresh, while keeping a second post-refresh inspection.
- Added `npm run test:auth-profile-helper` to validate strict blank-profile refresh, pre-launch credential/autofill/default-profile-reference failures, session-artifact strict failure, and sensitive-log rejection with sanitized output.
- Validation: `node --check scripts/validate-playwright-authenticated-profile.mjs`, `node --check scripts/validate-playwright-auth-profile-helper.mjs`, `npm run lint`, `npm run check`, `npm test`, `npm run build`, `npm run test:auth-profile-helper`, `npm run test:official-fail-closed`, `npm run test:synthetic-fail-closed`, `npm run test:browser-preview`, and `git diff --check` passed.
- Remaining caveat: this proves disposable marker-owned profile and helper behavior; real saved-credential absence and authenticated log cleanliness still require logged-in app-owned profile smoke.

Sidecar parse-failure bridge coverage:

- Extended Rust bridge tests for sidecar `usage` responses with missing visible percentages, inconsistent percentages, invalid reset timestamps, and unsupported visible fields.
- The tests verify sanitized `missing_data` and `parse_failed` snapshots, and ensure raw invalid timestamp and unsupported field values are not echoed in snapshot details.
- Validation: `cargo fmt --check`, `cargo check`, `cargo test --lib`, `cargo clippy -- -D warnings`, `npm run lint`, `npm run check`, `npm test`, `npm run build`, `npm run test:synthetic-fail-closed`, `npm run test:browser-preview`, `npm run test:official-fail-closed`, and `git diff --check` passed.
- Remaining caveat: this proves app-side parse-failure handling for sanitized sidecar responses; real official authenticated page fields still require logged-in app-owned profiles.

Sidecar-to-snapshot bridge coverage:

- Added Rust unit tests for `usage_snapshot_from_sidecar_usage_response`, covering sidecar page states for MFA, CAPTCHA/bot-check, network unavailable, timeout, unexpected UI, successful usage, and unsupported sidecar state rejection.
- The unsupported-state check verifies the raw sidecar state is not echoed in the sanitized error path.
- Validation: `cargo fmt --check`, `cargo check`, `cargo test --lib`, `cargo clippy -- -D warnings`, `npm run lint`, `npm run check`, `npm test`, `npm run build`, `npm run test:synthetic-fail-closed`, `npm run test:browser-preview`, `npm run test:official-fail-closed`, and `git diff --check` passed.
- Remaining caveat: this proves the app-side mapping boundary; real official authenticated and interruption pages still require logged-in app-owned profiles.

Synthetic web fail-closed smoke:

- Added `npm run test:synthetic-fail-closed`, which starts a temporary local HTTPS server, generates a temporary certificate, and runs the real headless Playwright sidecar against synthetic usage, logged-out, MFA, CAPTCHA/bot-check, and authenticated unexpected-UI pages.
- The smoke covers both Codex and Claude profile labels, keeps `visibleBrowserRequired = false`, verifies sanitized sidecar output, and removes temporary profiles/server material after the run.
- Validation: `node --check scripts/validate-playwright-synthetic-fail-closed.mjs`, `npm run lint`, `npm run check`, `npm test`, `npm run build`, `npm run test:browser-preview`, `npm run test:official-fail-closed`, `npm run test:synthetic-fail-closed`, and `git diff --check` passed.
- Remaining caveat: this is browser-backed synthetic page proof; real official authenticated page fields and real official interruption pages still require logged-in app-owned profiles.

Sidecar visible-state classifier coverage:

- Added Node unit tests for Playwright sidecar visible usage extraction and synthetic page-state classification without real authenticated page content.
- The tests cover remaining/used percentage extraction, reset timestamp normalization, plan/window field detection, closest-label matching so `used` does not attach to the wrong percentage, and logged-out/CAPTCHA/MFA/auth-gate/usage/no-cookie/unexpected-UI classifier states.
- Validation: `node --test sidecars/playwright/*.node-test.mjs`, `npm run prepare:sidecar`, `npm run lint`, `npm run check`, `npm test`, `npm run build`, `npm run test:official-fail-closed`, `npm run test:browser-preview`, and `git diff --check` passed.
- Remaining caveat: this proves sidecar classifier behavior against synthetic pages; real official authenticated page fields still require logged-in app-owned profile smoke.

Visible login launch guard:

- `Start login` still performs a headless usage preflight before any headed Playwright launch.
- The headed login browser now launches only for explicit user-action preflight states: `logged_out`, `mfa_required`, or `captcha_or_bot_check`.
- Authenticated `usage` returns `already_authenticated`; network, timeout, unexpected-UI, and failed preflight checks return a sanitized non-launching `preflight_unavailable` status instead of flashing Chromium.
- Validation: `cargo fmt --check`, `cargo check`, `cargo test --lib`, `cargo clippy -- -D warnings`, `npm run lint`, `npm run check`, `npm test`, `npm run build`, `npm run test:browser-preview`, `npm run test:official-fail-closed`, and `git diff --check` passed.
- Remaining caveat: real post-login authenticated profile evidence still requires a logged-in app-owned profile smoke run.

Release caveat preflight:

- `npm run smoke:preflight` now reports sanitized release-readiness booleans for configured Linux AppImage, Windows, macOS Intel, and macOS Apple Silicon release artifacts.
- The preflight also checks that README and release workflow notes still mark Windows/macOS builds as untested while leaving platform runtime smoke marked as required.
- Validation: `node --check scripts/collect-smoke-preflight.mjs`, `npm run smoke:preflight`, `npm run lint`, `npm run check`, and `git diff --check` passed.
- Remaining caveat: this only verifies release metadata and caveat text; it does not replace Windows/macOS runtime installation or launch smoke.

Lint baseline:

- Added `npm run lint` with ESLint flat config for Svelte, TypeScript, browser code, Node scripts, sidecar code, and Vite config while ignoring generated/build outputs.
- The lint pass keys the usage-card Svelte each block and keeps cleanup validation scripts from throwing inside `finally` blocks so cleanup failures do not mask earlier validation failures.
- Validation: `npm run lint`, `npm run check`, `npm test`, `npm run test:official-fail-closed`, `npm run test:browser-preview`, `npm run test:sidecar-launch`, and `git diff --check` passed.

Login prompt and headed-launch preflight:

- `Refresh official` remains the silent/headless check after web-provider opt-in. The frontend renders `Start login` only when the current web snapshot, or a local fallback carrying `webStatus`, reports `login_required`.
- The desktop `start_provider_login` command now performs a headless Playwright usage preflight before launching headed Chromium. If the app-owned profile already reaches the usage state, the command returns sanitized `already_authenticated` status without opening a visible browser.
- Evidence: Vitest covers login-prompt visibility for direct web `login_required`, fallback `webStatus = login_required`, parsed states, MFA, and network-unavailable states. Rust tests cover the preflight decision boundary and the sanitized `already_authenticated` IPC status shape.
- Validation: `cargo fmt --check`, `cargo test --lib`, `cargo clippy -- -D warnings`, `npm run check`, `npm test`, `npm run test:browser-preview`, `npm run test:official-fail-closed`, `npm run build`, and `git diff --check` passed.
- Remaining caveat: real post-login preflight evidence still requires authenticated app-owned Codex and Claude profiles.

Manual evidence template:

- `npm run smoke:preflight` now includes sanitized pending-observation templates for KDE tray behavior, authenticated web/session checks, and Windows/macOS platform smoke.
- The templates list required manual fields such as date/session, OS/session type, artifact used, observed KDE behavior, authenticated refresh outcome, visible fields, saved-credential artifact absence, sanitized-log absence, platform launch/tray/settings/quit behavior, and automated KDE smoke dependency availability.
- The preflight output now includes sanitized booleans for `qdbus`, `gdbus`, `xdotool`, `xprop`, and `xmessage` availability, plus StatusNotifier host registration status when it can be queried.
- The preflight output still excludes cookies, tokens, auth headers, browser profile contents, account identifiers, authenticated page content, and full local paths.
- Validation: `node --check scripts/collect-smoke-preflight.mjs`, `npm run smoke:preflight`, `npm run check`, and `git diff --check` passed.
- Remaining caveat: template output is not a substitute for the actual user-observed KDE, authenticated web, or Windows/macOS smoke results.

Headless official refresh and visible-browser suppression:

- Normal official refresh checks now use the Playwright sidecar `refreshUsage` action in headless mode with the app-owned persistent profile. Visible Chromium remains reserved for explicit `Start login` requests.
- Desktop `Refresh usage`, service-specific `Refresh official`, and scheduled due-refresh web checks use the headless sidecar result path and keep the existing provider cache/merge behavior. Scheduled headless web checks do not consume the manual web-refresh cooldown.
- Headless navigation failures after a persistent context is created now map to sanitized visible page states: `timed_out` for Playwright timeout errors and `network_unavailable` for other refresh navigation failures. Sidecar launch/protocol failures still fail closed as sanitized sidecar errors.
- Evidence: `npm run test:official-fail-closed` passed on CachyOS KDE/Wayland. Blank Codex and Claude profiles returned sanitized `logged_out` states with `headlessRefresh = true` and `visibleBrowserRequired = false`; forced dead-proxy Codex and Claude refreshes returned sanitized `network_unavailable` with the same headless/no-visible-browser flags.
- Validation: `npm test`, `npm run test:official-fail-closed`, `npm run check`, `npm run build`, `npm run test:browser-preview`, `git diff --check`, and cleanup checks for leftover sidecar processes and temporary official-fail-closed profile roots passed for the implementation slice.
- Remaining caveat: authenticated official parsing, post-login session persistence, real MFA/CAPTCHA/unexpected-UI browser states, and saved-credential absence after login still require manual authenticated validation.

Authenticated profile smoke helper:

- Added `npm run smoke:auth-profile`, a manual post-login helper for future authenticated validation of app-owned Codex and Claude Playwright profiles.
- The helper accepts `--codex-profile`, `--claude-profile`, `FORGEGAUGE_AUTH_CODEX_PROFILE_ROOT`, or `FORGEGAUGE_AUTH_CLAUDE_PROFILE_ROOT`, requires the same ForgeGauge `.forgegauge-profile.json` ownership marker used by the app unless `--allow-unmarked-test-profile` is explicitly passed, then performs headless `refreshUsage` checks against those existing profile roots. It never launches a visible browser.
- It emits sanitized JSON with stable service/profile labels, visible field names if `usage` is reached, fail-closed page state when not authenticated, profile marker booleans, profile storage artifact counts, symlink counts, disabled preference booleans, desktop/session metadata, and `visibleBrowserRequired = false`.
- It verifies its own stdout/stderr/output do not include raw profile paths, official URLs, launch args, cookies, tokens, auth headers, browser storage contents, page markup, or raw page content. Real profile runs should use `npm --silent run` or environment variables so npm does not echo CLI path arguments before the helper starts. Strict `--require-usage`, `--require-session-storage-artifacts`, `--require-disabled-storage-preferences`, `--require-no-credential-store-files`, `--require-no-autofill-store-files`, `--require-no-default-profile-references`, and `--require-sanitized-log-file` flags are available for real post-login checks. Failures emit sanitized JSON codes instead of Node stacks.
- Evidence: `npm run smoke:auth-profile -- --help` passed without launching a browser. A missing-marker profile failed with sanitized `missing_profile_marker` output and no raw profile path from the helper. A temporary marker-owned blank Codex profile run with `npm --silent run smoke:auth-profile -- --require-no-credential-store-files --require-no-autofill-store-files --require-no-default-profile-references` passed and returned sanitized `logged_out`, `headlessRefresh = true`, `visibleBrowserRequired = false`, `credentialStoreFilesAbsent = true`, `autofillStoreFilesAbsent = true`, default-profile-reference absence, marker booleans, profile storage counts, and no raw profile path or official URL in helper output. Marker-owned disposable profiles containing fake `Default/Login Data`, `Default/Web Data`, or a default-browser path in `Default/Preferences` failed with sanitized `credential_store_detected`, `autofill_store_detected`, or `default_profile_reference_detected` output and no raw profile path.
- Evidence: the new `--require-session-storage-artifacts` mode fails sanitized future post-login checks unless the headless refresh reaches `usage` and the app-owned profile inspection reports cookie-store or site-storage artifacts. Normal output also reports `authenticatedSessionEvidencePresent` for the combined usage-plus-storage condition. A disposable marker-owned blank profile returned sanitized `session_artifacts_missing` rather than raw profile paths, URLs, browser output, or page content.
- Evidence: the new `--log-file`, `FORGEGAUGE_AUTH_LOG_PATH`, and `--require-sanitized-log-file` path scans the normal app log after the headless profile refresh. A disposable marker-owned blank Codex profile accepted a safe app-style log and rejected a log containing auth/page material with sanitized `sensitive_log_detected` output; neither run exposed temporary profile or log paths.
- The future manual browser-profile login inspection checklist now requires the same strict authenticated smoke command, including a normal ForgeGauge log file and `logInspection.sensitiveContentAbsent = true` evidence.
- Remaining caveat: this adds the repeatable authenticated smoke workflow but does not prove real authenticated profile persistence, parseable official usage fields, saved-credential absence after login, or authenticated refresh logging until run against real logged-in dedicated profiles and the normal app log.

Packaged settings persistence smoke:

- `npm run smoke:kde-tray` now validates packaged config persistence in addition to KDE StatusNotifier registration, DBusMenu show/quit, and XWayland close/reopen behavior.
- The smoke launches the AppImage with isolated XDG directories, verifies ForgeGauge creates a current-schema config on first launch, writes sanitized non-secret service-toggle and gauge-interval values into that isolated config, restarts the AppImage from the same isolated root, verifies those persisted values survive restart, dispatches tray `Quit`, and removes the isolated directories.
- Evidence from the passing smoke: `currentDesktop = KDE`, `xdgSessionType = wayland`, AppImage path reported repo-relatively as `src-tauri/target/release/bundle/appimage/ForgeGauge_0.1.0_amd64.AppImage`, `configCreatedOnFirstLaunch = true`, `persistedServiceTogglesPreservedAfterRestart = true`, `persistedGaugeIntervalPreservedAfterRestart = true`, and `persistedConfigSurvivesPackagedRestart = true`.
- Validation: `node --check scripts/validate-kde-tray-registration.mjs`, `git diff --check`, `npm run check`, `npm run smoke:kde-tray`, and cleanup checks for leftover ForgeGauge processes and `/tmp/forgegauge-kde-tray-smoke-*` dirs passed.
- Remaining caveat: this proves packaged config survival across an isolated restart, but not a human-visible settings-form save inside the KDE webview or physical tray placement/click behavior.

KDE gauge rotation smoke:

- `npm run smoke:kde-tray` now validates packaged tray icon rotation through KDE StatusNotifier D-Bus before the settings-persistence restart.
- The smoke restarts the AppImage with deterministic local providers disabled, polls ForgeGauge StatusNotifier `IconName` updates, decodes the exported tray PNGs, and passes only after observing both the `Codex` and `Claude Code` service accent colors. It reports only sanitized service labels and booleans in the JSON evidence.
- Validation: `node --check scripts/validate-kde-tray-registration.mjs`, `npm run smoke:kde-tray`, `npm run check`, and `git diff --check` passed.
- Remaining caveat: this proves D-Bus icon updates and rendered service-color rotation in the current KDE/Wayland session, but not physical tray placement, visible icon animation, physical tray click behavior, or tooltip text exposure.

KDE popup utility-window smoke:

- The main popup now applies skip-taskbar and always-on-top hints when created or shown, hides on focus loss, and left tray click toggles visibility instead of only showing the popup.
- `npm run smoke:kde-tray` now requires `xprop` and `xmessage`, validates the packaged XWayland popup exposes `_NET_WM_STATE_SKIP_TASKBAR` and an above/stays-on-top window state after tray-menu `Show ForgeGauge`, moves focus to a throwaway X11 window, and verifies focus loss hides the ForgeGauge popup while the process and tray item remain alive.
- Validation: `cargo fmt --check`, `cargo check`, `cargo test --lib`, `cargo clippy -- -D warnings`, `npm run check`, `npm run build:appimage`, `node --check scripts/validate-kde-tray-registration.mjs`, `npm run smoke:kde-tray`, and `git diff --check` passed.
- Remaining caveat: this proves utility-window hints and focus-loss dismissal in the current KDE/Wayland XWayland path, but not physical tray-click behavior, exact tray-relative placement, focus-loss behavior under every compositor path, or multi-monitor placement.

Browser session manager status reconciliation:

- Marked the isolated browser session manager complete in the plan while leaving authenticated login, authenticated cookie/session validation, saved-credential absence after login, and real provider refresh parsing unchecked.
- Evidence: `cargo test browser_session --lib` passed 31 tests covering process tracking, graceful stop, orphan recovery, Playwright persistent-context request construction, sidecar request/response validation, disabled password/autofill preferences, redacted diagnostics, and sanitized profile inspection.
- Evidence: `cargo test browser_profile --lib` passed 24 tests covering app-owned default profile paths, ownership markers, restrictive permissions, default-browser path rejection, configured path preservation, distinct/non-nested service paths, and safe profile clearing.
- Evidence: `npm run test:sidecar-launch` passed for Codex and Claude, emitted sanitized JSON evidence with `os.id = cachyos`, `currentDesktop = KDE`, and `xdgSessionType = wayland`, preserved temporary isolated profiles across relaunch, kept service profiles distinct, preserved disabled storage preferences, avoided seeded default-profile import, sanitized stdout/stderr, and asserted process-group cleanup plus temporary profile root removal.

KDE tray registration smoke:

- Added `npm run smoke:kde-tray`, which requires a Linux user session with `qdbus`, `gdbus`, `xdotool`, `xprop`, `xmessage`, and an active KDE StatusNotifier host.
- The smoke launches the built AppImage with temporary isolated XDG config/data/cache/state directories, waits for a new ForgeGauge `org.kde.StatusNotifierItem`, verifies title `forgegauge`, id `tray-icon tray app main`, status `Active`, verifies the DBusMenu exposes `Show ForgeGauge` and `Quit`, verifies `Show ForgeGauge` opens a visible XWayland window, verifies a window-close request removes the visible window while the process and tray item remain alive, verifies `Show ForgeGauge` reopens or recreates the window, dispatches the tray `Quit` menu event, confirms the process exits successfully, confirms the tray item unregisters, and then removes temporary dirs.
- The main Tauri window is now configured non-closable where supported, implicit all-windows-closed exits are prevented, and the tray Show path recreates the main webview if KDE/XWayland destroyed it after close.
- Tray-click popup opening now positions near provided click coordinates and clamps to the active monitor work area when the platform supplies tray coordinates. Rust tests cover bottom-right tray anchors, top-edge fallback, negative-origin monitor layouts, and constrained work areas; KDE DBusMenu smoke continues to cover the packaged fallback path.
- This proves AppImage tray registration, tray-menu show/quit handling, automated XWayland close/reopen fallback through KDE's StatusNotifier/DBusMenu interfaces, and coordinate-based popup placement rules in the current code, but not visual tray placement, physical tray-click behavior, human-visible single/multi-monitor popup position, or visual quit-menu interaction.
- Validation: `npm run smoke:kde-tray`, `npm run build:appimage`, `npm run check`, `npm test`, `cargo fmt --check`, `cargo check`, `cargo test`, `cargo clippy -- -D warnings`, `git diff --check`, and cleanup checks for leftover ForgeGauge processes, ForgeGauge tray registrations, visible ForgeGauge windows, and `/tmp/forgegauge-kde-tray-smoke-*` dirs passed.

Manual smoke preflight:

- Added `npm run smoke:preflight`, which emits sanitized JSON for future manual smoke notes: commit, package/app metadata, OS/session signals, Playwright package version, and repo-relative AppImage/sidecar artifact status.
- The preflight output excludes cookies, tokens, auth headers, browser profile contents, account identifiers, authenticated page content, and full local paths, and it fails if the home directory path appears in the emitted JSON.
- This supports the manual evidence checklist but does not replace user-observed KDE tray behavior, authenticated login/profile checks, provider refresh smoke, or Windows/macOS runtime smoke.
- Validation: `npm run smoke:preflight`, `npm run check`, `npm test`, and `git diff --check` passed.

Repeatable browser-preview validation:

- Added `npm run test:browser-preview`, which starts Vite at `http://127.0.0.1:1420/`, launches Chromium through Playwright, and closes both browser and server after validation.
- The script checks the default preview plus browser-preview query states for missing local data, network unavailable, expired login, MFA, CAPTCHA/bot-check, unexpected UI, timeout, parse failure, stale data, provider unavailable, permission denied, unsafe profile path, and disabled provider at desktop `1280x900` and mobile `390x900`.
- It verifies two usage cards, expected status/stale notes, no horizontal overflow, disabled web controls before opt-in, enabled official refresh/login/profile controls after opt-in, and browser-preview desktop-only fallback messages for Start login, Refresh official, and Hide popup to tray.
- Validation: `npm run test:browser-preview`, `npm run check`, `npm test`, and `git diff --check` passed.

Tracker reconciliation:

- Narrowed stale Playwright blockers now that sidecar implementation, packaging, local headed launch, profile isolation, password/autofill preference, default-profile isolation, and sanitized-output validation are complete.
- Recorded fail-closed parser/display/browser-preview evidence under the remaining logged-out/MFA/CAPTCHA/unexpected-UI proof item while leaving real browser-backed provider failure validation unchecked.
- Remaining blockers are manual authenticated login/profile validation, real provider refresh smoke tests, KDE/Wayland tray smoke, and Windows/macOS runtime smoke.
- Validation: `cargo test web_provider`, `cargo test web_interruption`, `cargo test web_provider_fails_closed`, `npm run check`, and `git diff --check` passed.

Browser-preview state fixtures:

- Added browser-preview-only query states for `missing-local-data`, `network-unavailable`, `expired-login`, `mfa-required`, `captcha-or-bot-check`, `unexpected-ui`, `timed-out`, `parse-failed`, `stale-data`, `provider-unavailable`, `permission-denied`, `unsafe-profile-path`, and `provider-disabled`.
- Vitest now covers preview-state query parsing, stale snapshot handling for `stale-data`, and the rendered status-note snapshots for `No usage data found`, `Network unavailable`, `Login required`, `MFA required`, `Additional verification required`, `Unexpected usage page`, `Usage refresh timed out`, `Usage data could not be parsed`, `Provider unavailable`, `Usage data is not readable`, `Profile path blocked`, and `Provider disabled`.
- Browser-preview validation: Vite at `http://127.0.0.1:1420/` loaded the default preview plus all query states at desktop `1280x900` and mobile `390x900`. Each state rendered two usage articles, the expected service status or stale-data notes, and no horizontal overflow.
- Validation: `npm test` (`19` Vitest tests, `4` Node sidecar protocol tests, and generated sidecar dry-run passed), `npm run check`, `npm run build`, `npm run test:sidecar-launch`, `cargo fmt --check`, `cargo check`, `cargo test` (`168 passed`), `cargo clippy -- -D warnings`, and `npm run build:appimage` passed.
- This proves browser-preview rendering for the local, provider interruption, and graceful provider states, but does not replace real desktop/provider smoke tests for network outages, missing local data, expired authenticated sessions, MFA, CAPTCHA/bot checks, unexpected official UI, timeouts, parse failures, stale real data, unavailable providers, permission-denied local data, unsafe profile paths, or disabled real providers.

Playwright sidecar runtime launch:

- Added the Playwright npm package as the local runtime dependency for the Node sidecar.
- Added `npm run test:sidecar-launch`, which launches the generated sidecar against the Codex and Claude official URLs with `headless: false`, distinct temporary isolated profiles under `/tmp`, sanitized stdout-only response checks, relaunch persistence checks for each service profile, launch-time disabled password/autofill preference checks, seeded fake default Chrome/Chromium profile import checks, and stdout/stderr privacy checks.
- Validation: `npm test`, `npm run test:sidecar-launch`, `npm run check`, `npm run build`, `cargo fmt --check`, `cargo check`, `cargo test` (`168 passed`), `cargo clippy -- -D warnings`, and `npm run build:appimage` passed. `npm run test:sidecar-launch` passed for both services, preserved profile sentinel files and disabled password/autofill preferences across relaunch, verified seeded fake default browser profile sentinels were not imported, verified sidecar stdout/stderr omitted raw launch data, fake profile sentinels, auth/cookie-looking material, and page markup, removed the temporary profile directories afterward, and left no ForgeGauge sidecar processes running.
- Packaging evidence: `npm run build:appimage` produced `src-tauri/target/release/bundle/appimage/ForgeGauge_0.1.0_amd64.AppImage` (`106M`) with `ForgeGauge.AppDir/usr/bin/forgegauge-playwright-sidecar` still present.
- Browser-preview validation: Vite at `http://127.0.0.1:1420/` loaded with title `ForgeGauge`; Playwright desktop `1280x900` and mobile `390x900` checks found no horizontal overflow, with two usage articles, service-specific Start login actions, and both profile inspection actions visible.
- This proves the local sidecar can start headed Playwright sessions for both official URLs with distinct persistent temporary profiles, preserved disabled password/autofill preferences, no import from seeded fake default browser profiles, and sanitized sidecar launch output, but it does not prove authenticated login, app-owned profile persistence, saved-credential absence after login, parseable authenticated fields, authenticated refresh logging, or AppImage runtime behavior outside the source workspace.

Playwright Linux sidecar packaging:

- Added a Linux-only Tauri config that registers `binaries/forgegauge-playwright-sidecar` as an `externalBin`.
- Added `scripts/prepare-playwright-sidecar.mjs` to generate `src-tauri/binaries/forgegauge-playwright-sidecar-x86_64-unknown-linux-gnu` from the checked-in sidecar source and keep it executable.
- Added package validation to `npm test` that checks the generated sidecar is current, executable, accepts the dry-run `launchLogin` protocol, and does not echo raw `userDataDir` or launch args.
- Validation: `npm test` (`16` Vitest tests, `4` Node sidecar protocol tests, and generated sidecar dry-run passed), `npm run check`, `npm run build`, `cargo fmt --check`, `cargo check`, `cargo test` (`168 passed`), `cargo clippy -- -D warnings`, and `npm run build:appimage` passed.
- Packaging evidence: `npm run build:appimage` produced `src-tauri/target/release/bundle/appimage/ForgeGauge_0.1.0_amd64.AppImage` (`106M`) and included `ForgeGauge.AppDir/usr/bin/forgegauge-playwright-sidecar`.
- Browser-preview validation: Vite at `http://127.0.0.1:1420/` loaded with title `ForgeGauge`; Playwright desktop `1280x900` and mobile `390x900` checks found no horizontal overflow, with two usage articles, service-specific Start login actions, and both profile inspection actions visible.
- The generated sidecar is still Node-based; real headed Playwright login requires an available Node/Playwright runtime and manual authenticated CachyOS KDE/Wayland validation.

Playwright sidecar process boundary:

- Added `tauri-plugin-shell` and wired `start_provider_login` to attempt a Rust-owned Playwright sidecar launch when managed web profiles are enabled.
- The launch path stops any existing managed process for the service, resolves the sidecar name `forgegauge-playwright-sidecar`, writes the serialized `launchLogin` JSON payload to stdin, waits for one sanitized stdout acknowledgment, validates a `launched` response against the original request, and tracks the child through the existing browser session manager.
- If the sidecar binary is missing, does not acknowledge launch, or rejects the request, the command fails closed to sanitized `login_required` status and emits `login://required` with reason `sidecar_unavailable`; raw paths, raw `userDataDir`, launch args, process errors, and Playwright errors remain excluded from IPC.
- Tests cover sanitized sidecar response parsing, mismatch/rejection handling without echoing raw paths, sidecar request planning without exposing paths to login-start IPC, web-disabled fallback, and the new login-required reason.
- Validation: `cargo fmt --check`, `cargo check`, `cargo test` (`168 passed`), `cargo clippy -- -D warnings`, `npm test` (`16` Vitest tests and `4` Node sidecar tests passed), `npm run check`, `npm run build`, and `git diff --check` passed.
- Browser-preview validation: Vite at `http://127.0.0.1:1420/` loaded with title `ForgeGauge`; Playwright desktop `1280x900` and mobile `390x900` checks found no horizontal overflow, with two usage articles, service-specific Start login actions, and both profile inspection actions visible.
- Tauri `externalBin` registration is now covered by the Linux sidecar packaging entry above. Manual login and authenticated profile validation remain unchecked.

Playwright sidecar request serialization:

- Rust now serializes a `PlaywrightSidecarLaunchRequest` matching the sidecar `launchLogin` protocol shape, including `protocolVersion`, backend id, service, HTTPS URL, profile label, raw `userDataDir`, headed mode, and launch args.
- Raw `userDataDir` is serialized only for the future sidecar stdin payload; debug output and diagnostics use the profile label placeholder, hide launch args, and skip diagnostics from JSON serialization.
- Tests cover the protocol JSON shape, redaction of raw paths and launch flags from debug output, and HTTPS-only login URL rejection without echoing rejected URLs.
- Validation: `cargo fmt --check`, `cargo check`, `cargo test` (`162 passed`), `cargo clippy -- -D warnings`, `npm test` (`16` Vitest tests and `4` Node sidecar tests passed), `npm run check`, `npm run build`, and `git diff --check` passed.
- Browser-preview validation: Vite at `http://127.0.0.1:1420/` loaded with title `ForgeGauge`; Playwright desktop `1280x900` and mobile `390x900` checks found no horizontal overflow, with two usage articles, service-specific Start login actions, and both profile inspection actions visible.
- Real Tauri sidecar process spawning, `externalBin` packaging, manual login, and authenticated profile validation remain unchecked.

Playwright backend approval and launch contract:

- User approved the Playwright headed Chromium sidecar backend on 2026-06-04.
- The architecture spike now records Playwright as the selected backend while keeping real web-provider implementation gated on manual CachyOS KDE/Wayland login/profile validation.
- Added an internal `PlaywrightLaunchRequest` contract that maps the existing Chromium launch policy to Playwright's persistent user-data-dir shape: raw `userDataDir` stays internal, Playwright args exclude `--user-data-dir`, headed mode is explicit, and diagnostics/debug output uses only profile labels.
- Tests cover persistent-context request construction and redaction of raw profile paths from Playwright launch diagnostics.
- Validation: `cargo fmt --check`, `cargo check`, `cargo test` (`157 passed`), `cargo clippy -- -D warnings`, `npm test` (`16 passed`), `npm run check`, `npm run build`, and `git diff --check` passed.
- Browser-preview validation: Vite at `http://127.0.0.1:1420/` loaded with title `ForgeGauge`; Playwright desktop `1280x900` and mobile `390x900` checks found no horizontal overflow, with two usage articles and both profile inspection actions visible.
- Real Playwright sidecar packaging, process launch integration, manual login flow, and authenticated profile validation remain unchecked.

Playwright login-start metadata:

- `start_provider_login` now prepares managed profiles when web providers are enabled and returns sanitized Playwright backend metadata: backend id, profile label, and profile-prepared state.
- The login-start IPC payload still excludes raw profile paths, raw `userDataDir`, launch arguments, cookies, tokens, and authenticated page content.
- Tests cover sanitized IPC serialization, prepared profile metadata, and the web-disabled unprepared profile path.
- Validation: `cargo fmt --check`, `cargo check`, `cargo test` (`159 passed`), `cargo clippy -- -D warnings`, `npm test` (`16 passed`), `npm run check`, `npm run build`, and `git diff --check` passed.
- Browser-preview validation: Vite at `http://127.0.0.1:1420/` loaded with title `ForgeGauge`; Playwright desktop `1280x900` and mobile `390x900` checks found no horizontal overflow, with two usage articles, Start login actions, and both profile inspection actions visible.
- Real Playwright sidecar process launch and manual login remain unchecked.

Playwright sidecar protocol:

- Added a Playwright sidecar source scaffold with a JSON `launchLogin` request protocol and `--dry-run` validation path.
- The sidecar accepts raw `userDataDir` only as process input for the future sidecar launch path; dry-run and rejected responses emit only backend id, service, profile label, headed/headless mode, argument count, stable status, and stable error codes.
- The protocol rejects invalid actions, unsupported services/backends, non-HTTPS URLs, headless mode, non-string args, and `--user-data-dir` launch args because Playwright receives the user data directory separately.
- `npm test` now runs Vitest plus Node sidecar protocol tests.
- Validation: `cargo fmt --check`, `cargo check`, `cargo test` (`159 passed`), `cargo clippy -- -D warnings`, `npm test` (`16` Vitest tests and `4` Node sidecar tests passed), `npm run check`, `npm run build`, and `git diff --check` passed.
- Browser-preview validation: Vite at `http://127.0.0.1:1420/` loaded with title `ForgeGauge`; Playwright desktop `1280x900` and mobile `390x900` checks found no horizontal overflow, with two usage articles, Start login actions, and both profile inspection actions visible.
- Tauri `externalBin` registration and real Playwright sidecar process launch remain unchecked until a target-triple sidecar binary exists under `src-tauri/binaries`.

Profile autofill-store inspection:

- Managed Chromium profile inspection now counts Chromium `Web Data` autofill store artifacts and `Web Data-*` sidecars without opening or reading browser database contents.
- The sanitized `ProviderProfileInspection` IPC payload exposes only the aggregate `autofillStoreFiles` count, and the frontend summary reports autofill store artifacts separately from password credential files.
- Tests cover autofill-store artifact detection, sanitized IPC serialization, and frontend profile-inspection summary copy.
- Validation: `cargo fmt --check`, `cargo check`, `cargo test` (`155 passed`), `cargo clippy -- -D warnings`, `npm test` (`16 passed`), `npm run check`, `npm run build`, and `git diff --check` passed.
- Browser-preview validation: Vite at `http://127.0.0.1:1420/` loaded with title `ForgeGauge`; Playwright desktop `1280x900` and mobile `390x900` checks found no horizontal overflow, with two usage articles and both profile inspection actions visible.
- Manual authenticated saved-credential and autofill-store validation remains unchecked until a browser backend and login flow are selected and tested.

Profile storage isolation:

- Managed browser profile resolution now rejects identical, nested, and root-overlapping Codex/Claude profile paths after canonicalization and before creating profile directories.
- This prevents configured browser profile overrides from sharing one Chromium `--user-data-dir` between services or making one service profile contain the other service's session storage.
- Tests cover shared service paths, nested service paths, profile root inside a service path, and profile root equal to a service path.
- Validation: `cargo fmt --check`, `cargo check`, `cargo test` (`150 passed`), `cargo clippy -- -D warnings`, `npm test` (`16 passed`), `npm run check`, `npm run build`, and `git diff --check` passed.
- Browser-preview validation: Vite at `http://127.0.0.1:1420/` loaded with title `ForgeGauge`; Playwright desktop `1280x900` and mobile `390x900` checks found no horizontal overflow, and the maintenance controls remained visible.
- Manual authenticated cookie/session validation remains unchecked until a browser backend and login flow are selected and tested.

Browser launch logging redaction:

- `BrowserLaunchPlan` debug output now uses the sanitized launch diagnostics args and a profile label placeholder instead of raw profile paths or raw `--user-data-dir` values.
- The log redaction policy now requires browser launch diagnostics and debug output to use sanitized profile labels.
- Tests cover both sanitized diagnostics and full launch-plan debug output.
- Validation: `cargo fmt --check`, `cargo check`, `cargo test` (`151 passed`), `cargo clippy -- -D warnings`, `npm test` (`16 passed`), `npm run check`, `npm run build`, and `git diff --check` passed.
- Browser-preview validation: Vite at `http://127.0.0.1:1420/` loaded with title `ForgeGauge`; Playwright desktop `1280x900` and mobile `390x900` checks found no horizontal overflow, and the maintenance controls remained visible.

Frontend unavailable status note:

- Frontend provider status-note mapping now recognizes the backend `unavailable` status emitted by local providers for unreadable data roots.
- Vitest coverage confirms `unavailable` renders as `Provider unavailable` instead of being hidden as an unsupported raw status.
- Validation: `cargo fmt --check`, `cargo check`, `cargo test` (`151 passed`), `cargo clippy -- -D warnings`, `npm test` (`16 passed`), `npm run check`, `npm run build`, and `git diff --check` passed.
- Browser-preview validation: Vite at `http://127.0.0.1:1420/` loaded with title `ForgeGauge`; Playwright desktop `1280x900` and mobile `390x900` checks found no horizontal overflow, with two usage cards and the maintenance controls visible.

Fail-closed web merge coverage:

- Display merging now carries a sanitized `webReason` code when a failed web snapshot falls back to local data.
- The `webReason` copy accepts only short lowercase code strings with digits or underscores and drops unsanitized strings such as paths, HTML, or raw error text.
- Rust tests cover local-data fallback for login required, MFA, CAPTCHA/bot-check, unexpected UI, parse failure, network unavailable, and timeout web failures.
- Real browser-backed provider failure validation remains unchecked until the backend/login flow exists.
- Validation: `cargo fmt --check`, `cargo check`, `cargo test` (`153 passed`), `cargo clippy -- -D warnings`, `npm test` (`16 passed`), `npm run check`, `npm run build`, and `git diff --check` passed.
- Browser-preview validation: Vite at `http://127.0.0.1:1420/` loaded with title `ForgeGauge`; Playwright desktop `1280x900` and mobile `390x900` checks found no horizontal overflow, with two usage cards and the maintenance controls visible.

Claude server-tool usage aggregation:

- Claude local parsing now sums numeric `message.usage.server_tool_use` values recursively into a sanitized `serverToolUseCount`.
- The frontend local activity summary includes `serverToolUseCount` as a compact activity segment when present.
- Tests cover sanitized fixture parsing, nested server-tool count aggregation, and local summary display.
- Raw server-tool field names, payloads, content, IDs, cost, and block data remain excluded from snapshot details.
- The broader Claude cost/block decision remains blocked.
- Validation: `cargo fmt --check`, `cargo check`, `cargo test` (`154 passed`), `cargo clippy -- -D warnings`, `npm test` (`16 passed`), `npm run check`, `npm run build`, and `git diff --check` passed.
- Browser-preview validation: Vite at `http://127.0.0.1:1420/` loaded with title `ForgeGauge`; Playwright desktop `1280x900` and mobile `390x900` checks found no horizontal overflow, with two usage cards and the maintenance controls visible.

## 2026-06-03 America/Sao_Paulo

Branch: `forgegauge-implementation`

Recent evidence commits:

- `d7671a5 feat: detect managed browser orphans`
- `c51b062 feat: add managed browser session stop guard`
- `743087e test: cover frontend display helpers`
- `06dc454 docs: record validation evidence`
- `2a8c2e5 feat: add provider login ipc boundary`
- `ddce3e6 docs: note claude local cost blocker`
- `6131d0c fix: guard browser preview desktop APIs`

Automated validation passed:

- `cargo fmt --check`
- `cargo check`
- `cargo test` (`124 passed`)
- `cargo clippy -- -D warnings`
- `npm test` (`11 passed`)
- `npm run check`
- `npm run build`
- `git diff --check`
- `npm run build:appimage`

Browser-preview validation passed with Playwright against `http://127.0.0.1:1420/`:

- Desktop layout loaded without Tauri API preview errors.
- Mobile layout loaded without overlapping usage cards or settings controls.
- Experimental web-provider toggle enabled web refresh, profile path, and start-login controls.
- Desktop-only Start login action returned a browser-preview fallback message instead of throwing.
- Hide-to-tray button rendered without overlapping the brand lockup and returned the browser-preview fallback status.
- Mobile DOM overflow check passed at `390px` width after web-provider controls were enabled.

Additional browser-preview validation on 2026-06-03:

- Ran `npm run dev` and loaded `http://127.0.0.1:1420/` through Playwright MCP.
- Desktop `1280x900` snapshot showed the hide button, usage cards, provider controls, disabled web controls before opt-in, and no horizontal overflow.
- Experimental web-provider opt-in enabled official refresh buttons, Start login buttons, web refresh/cooldown inputs, and browser profile path inputs.
- Start Codex login returned `Codex login starts from the desktop app` in browser preview without navigation or a thrown error.
- Hide-to-tray returned `Popup hides to tray in the desktop app` in browser preview.
- Mobile `390x900` DOM overflow check found `scrollWidth == clientWidth == 390` and zero overflowing elements.
- Mobile snapshot showed usage cards, enabled web controls, profile inputs, local calibration controls, and maintenance actions fitting inside the viewport.
- Captured viewport screenshot through Playwright MCP as `forgegauge-validation-mobile.png`.

Frontend status-note test coverage:

- `providerStatusMessage` maps missing local data, network unavailable, timeout, login-required, CAPTCHA/bot-check, and unexpected UI codes to stable user-facing notes.
- Parsed, placeholder, and unsupported raw status strings do not render a provider status note.
- Validation: `npm test` (`14 passed`) and `npm run check`.

Fail-closed web boundary validation:

- Explicit web-provider opt-in registers Codex and Claude web provider boundaries.
- Until a browser backend is selected, targeted official web refresh returns a sanitized `login_required` web snapshot instead of throwing `Provider is not configured`.
- If a local or fake snapshot is already available, display merging keeps that snapshot visible and adds sanitized `webStatus`, `webProviderId`, and `lastOfficialCheckAt` metadata.
- `providerStatusMessage` reads fallback `webStatus` metadata so the usage card can still show `Login required` while local data remains visible.
- Browser preview confirmed the official-refresh fallback still renders without horizontal overflow at desktop `1280x900` and mobile `390x900`.
- Validation: `cargo fmt --check`, `cargo check`, `cargo test` (`127 passed`), `cargo clippy -- -D warnings`, `npm test` (`15 passed`), `npm run check`, and `git diff --check`.

Local artifact:

- `src-tauri/target/release/bundle/appimage/ForgeGauge_0.1.0_amd64.AppImage`
- Size: `105M`
- Local timestamp: `Jun 3 23:16`

Runtime and packaging prerequisites observed:

- Local development/build validation used the checked-in npm scripts and Rust/Tauri toolchain from this workspace.
- Linux AppImage packaging uses `npm run build:appimage`, which sets `NO_STRIP=1` for the Arch/CachyOS linuxdeploy `.relr.dyn` strip incompatibility documented in the README.
- Browser-preview and AppImage build validation did not reveal additional package prerequisites.
- KDE/Wayland tray runtime package confirmation still requires manual desktop smoke testing with the AppImage artifact.

Remote release workflow evidence:

- GitHub Actions run: <https://github.com/pickforge/ai-usage-tray/actions/runs/26882140665>
- Event: `push` on `main`
- Head commit: `4861da642752be3e0ea61282d45bf8b850bb5170`
- Conclusion: `success`
- Release tag: `forgegauge-v0.1.0-4.1`
- Release URL: <https://github.com/pickforge/ai-usage-tray/releases/tag/forgegauge-v0.1.0-4.1>
- Release state: published, not draft, not prerelease
- Uploaded assets:
  - `linux-appimage-ForgeGauge_0.1.0_amd64.AppImage`
  - `windows-ForgeGauge_0.1.0_x64-setup.exe`
  - `windows-ForgeGauge_0.1.0_x64_en-US.msi`
  - `macos-intel-ForgeGauge_0.1.0_x64.dmg`
  - `macos-apple-silicon-ForgeGauge_0.1.0_aarch64.dmg`
- Jobs `Preflight`, `Create draft release`, `Build linux-appimage`, `Build windows`, `Build macos-intel`, `Build macos-apple-silicon`, and `Publish release` all completed successfully.
- `Publish release` started after the last build matrix job completed and used `gh release edit "$RELEASE_TAG" --draft=false`.
- No failing runner labels, action versions, package dependencies, or upload paths were observed in this successful run.
- Scope caveat: this verifies remote `main` at `4861da6`, not feature branch `forgegauge-implementation` at `096e7c1`, and it verifies uploads/build success rather than Windows or macOS install/runtime behavior.

Phase 4 architecture review:

- The frontend and tray read from the shared backend display-state cache.
- Usage snapshots, refresh events, provider error events, profile reset results, and login-required events have stable serialized IPC shapes covered by tests.
- Web providers remain behind explicit opt-in, parser contracts, and the login-required IPC boundary.
- Proceeding into provider work still depends on the separate browser backend/manual-login gate.

Managed browser session safety:

- `clear_provider_profile` and `reset_provider_session` stop the service's managed browser process before deleting profile data.
- `BrowserSessionManager` tracks one managed child process per service with the process handle and PID.
- Shutdown requests graceful termination first, then falls back to kill and reap after a timeout.
- Startup recovery reads a restrictive app-data registry, keeps only marker-verified orphaned browser processes, discards stale/unverified entries, and can stop verified orphans before profile deletion.
- Backend selection, password-manager controls, and authenticated login validation remain separate unchecked gates.

Browser launch policy safety:

- Added a backend-agnostic Chromium launch plan helper for future managed browser integrations.
- The launch plan binds each service to its provided service-specific profile path through `--user-data-dir`.
- The launch plan includes password-manager/autofill suppression flags, `--no-first-run`, and disabled Chromium storage preferences for password saving, autosign-in, profile autofill, and card autofill.
- Added a Chromium profile-preferences initializer that creates or merges `Default/Preferences` under the managed service profile, writes the disabled storage preferences, preserves unrelated preference keys, rejects malformed preference JSON, rejects symlinked paths, and applies restrictive permissions where supported.
- Wired managed browser profile preparation to initialize Chromium preferences for both Codex and Claude when experimental web profiles are prepared.
- The fail-closed `start_provider_login` boundary now prepares managed browser profiles and Chromium preferences before returning the existing login-required response.
- Sanitized launch diagnostics redact raw profile paths to service profile labels such as `codex-profile` and `claude-profile`.
- Validation: targeted `cargo test browser_session --lib` and `cargo test prepare_managed_browser_profiles --lib` passed with launch-policy, preference-initialization, and profile-preparation wiring tests.
- Browser-preview smoke: Vite at `http://127.0.0.1:1420/` loaded with title `ForgeGauge`; Playwright desktop `1280x900` and mobile `390x900` checks found no horizontal overflow after the launch-policy, preference-initialization, profile-preparation wiring, login-start preparation, and profile-inspection changes.
- Real backend selection, process launch integration, manual login flow, and authenticated profile inspection remain unchecked.

Profile inspection safety:

- Added a sanitized managed Chromium profile storage inspector for future login validation.
- The inspector reports only credential-store artifact counts, autofill-store artifact counts, symlink counts, password/autofill preference booleans, inspected entry counts, and limit status.
- Exposed the inspector through the `inspect_provider_profile` IPC command with a `ProviderProfileInspection` payload that omits raw paths and browser storage contents.
- Added maintenance actions for inspecting the Codex and Claude dedicated browser profile state from the UI.
- It reads Chromium `Default/Preferences` booleans but does not read cookie databases, token stores, browser storage, authenticated page content, screenshots, raw page text, or local profile contents.
- Tests cover missing profiles, prepared disabled profiles, credential-store file detection, autofill-store file detection, symlink detection without following symlinks, enabled preference detection, and malformed preference rejection without leaking raw paths or file contents.
- Validation: `cargo fmt --check`, `cargo check`, `cargo test` (`146 passed`), `cargo clippy -- -D warnings`, `npm test` (`16 passed`), `npm run check`, `npm run build`, and `git diff --check` passed for the IPC/UI exposure slice.
- Browser-preview validation: Vite at `http://127.0.0.1:1420/` loaded with title `ForgeGauge`; Playwright desktop `1280x900` and mobile `390x900` checks found no horizontal overflow, both profile inspection actions were visible, and the Codex inspect action returned the browser-preview desktop-only fallback without throwing.
- Manual authenticated profile inspection remains unchecked.

Web parser fallback coverage:

- Sanitized visible-state inputs now include `network_unavailable` and `timed_out`.
- Parser fixtures and tests cover logged-out, MFA, CAPTCHA/bot-check, network unavailable, timeout, unexpected UI, partial visible data, parse failure, and unsupported visible fields.
- This is parser-contract coverage only; real browser-backed provider launch and authenticated network/manual smoke tests remain deferred.

Tracker reconciliation:

- Marked the high-level fallback close/dismiss and Phase 0.5 fallback-choice checklist items complete to match the implemented explicit popup hide-to-tray fallback and the already-completed detailed Phase 1/milestone entries.
- Manual KDE/Wayland tray visibility, popup open/close behavior, close-button confirmation, settings persistence, and quit behavior remain unchecked.

Deferred evidence:

- KDE/Wayland tray checks require user-visible desktop interaction.
- Web/session security checks require browser backend selection and manual authenticated login/profile validation.
- Current-feature release verification requires pushing or dispatching this feature branch through the release workflow.
- Windows/macOS install/runtime checks require manual testing on those platforms.
