# ForgeGauge Progress Validation Log

## 2026-06-04 America/Sao_Paulo

Branch: `forgegauge-implementation`

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
