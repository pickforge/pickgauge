# ForgeGauge Progress Validation Log

## 2026-06-04 America/Sao_Paulo

Branch: `forgegauge-implementation`

KDE tray registration smoke:

- Added `npm run smoke:kde-tray`, which requires a Linux user session with `qdbus`, `gdbus`, `xdotool`, and an active KDE StatusNotifier host.
- The smoke launches the built AppImage with temporary isolated XDG config/data/cache/state directories, waits for a new ForgeGauge `org.kde.StatusNotifierItem`, verifies title `forgegauge`, id `tray-icon tray app main`, status `Active`, verifies the DBusMenu exposes `Show ForgeGauge` and `Quit`, verifies `Show ForgeGauge` opens a visible XWayland window, verifies a window-close request removes the visible window while the process and tray item remain alive, verifies `Show ForgeGauge` reopens or recreates the window, dispatches the tray `Quit` menu event, confirms the process exits successfully, confirms the tray item unregisters, and then removes temporary dirs.
- The main Tauri window is now configured non-closable where supported, implicit all-windows-closed exits are prevented, and the tray Show path recreates the main webview if KDE/XWayland destroyed it after close.
- This proves AppImage tray registration, tray-menu show/quit handling, and automated XWayland close/reopen fallback through KDE's StatusNotifier/DBusMenu interfaces in the current Wayland/KDE session, but not visual tray placement, physical tray-click behavior, popup position, settings persistence, or visual quit-menu interaction.
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
