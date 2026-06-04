# Browser Automation Spike

Date: 2026-06-03

Scope: compare browser automation backends for future opt-in official usage providers. Backend selection was approved on 2026-06-04: ForgeGauge will proceed with the Playwright headed Chromium sidecar path, with real web-provider implementation still gated on manual KDE/Wayland login/profile validation.

Docs checked through Context7:

- Playwright `/microsoft/playwright.dev`
- Selenium `/seleniumhq/seleniumhq.github.io`
- Tauri `/tauri-apps/tauri-docs`

## Decision Matrix

Scores are 1-5, where 5 is strongest for ForgeGauge's constraints.

| Backend | KDE/Wayland | Persistent app profiles | Packaging cost | Parser access | Security controls | Maintainability | Notes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| Playwright headed Chromium sidecar | 4 | 5 | 2 | 5 | 4 | 4 | Best parser access. Supports explicit `userDataDir`, headed login, and ARIA snapshots. Packaging must handle browser binaries and a Node or sidecar runtime. |
| Selenium WebDriver | 3 | 4 | 2 | 3 | 3 | 3 | Mature browser/profile support, but driver/browser management adds runtime complexity. Parser access is mostly DOM/text oriented. |
| Raw Chromium/CDP control | 3 | 4 | 3 | 3 | 3 | 2 | Can be small if using a system browser, but protocol and browser lifecycle ownership become app-maintained. |
| Tauri opener/system browser only | 4 | 1 | 5 | 1 | 2 | 5 | Good for opening official pages manually, already implemented. Not sufficient for automated reading because it cannot guarantee isolated app-owned session state or parse fields. |

Decision: proceed with Playwright headed Chromium sidecar. Local headed sidecar validation proves distinct temporary profile persistence, disabled password/autofill preference preservation across relaunch, and no import from seeded fake default Chrome/Chromium profiles. Web provider implementation remains deferred until manual tests prove app-owned authenticated profile persistence, visible login, no saved credentials after login, and parseable official page fields.

## Required Runtime Dependencies

Playwright:

- Browser binaries live in OS cache locations by default, including `~/.cache/ms-playwright` on Linux.
- `PLAYWRIGHT_BROWSERS_PATH` can redirect the browser-binary cache for packaging or test runs.
- The workspace now depends on the Playwright npm package for local sidecar runtime validation. Local Chromium installation can be checked with `npx playwright install --dry-run chromium` and installed with `npx playwright install chromium`.
- The Linux AppImage build now includes a target-triple sidecar executable generated from the checked-in Node source and registered through `tauri.linux.conf.json` `externalBin`.
- The generated sidecar is Node-based and still depends on an available Node/Playwright runtime and browser installation for real headed launches; dry-run packaging validation does not prove authenticated login.
- Rust attempts a backend-owned sidecar launch through `tauri-plugin-shell`, writes one JSON `launchLogin` payload to stdin, validates one sanitized stdout response, and tracks the child through the existing browser session manager.

Selenium:

- Selenium Manager can locate or download browsers and browser drivers into its cache.
- Linux deployments still need executable drivers and a compatible browser path or browser download policy.

Raw CDP:

- Requires locating a compatible Chromium/Chrome binary or bundling one.
- Requires app-owned process startup, remote debugging socket/port ownership, timeout handling, and protocol compatibility management.

Tauri:

- `tauri-plugin-opener` is enough for the already implemented "open official page" action.
- The Playwright sidecar launch is Rust-owned through `tauri-plugin-shell`; no frontend shell permission is granted in the current capabilities file. If future frontend shell access is added, capabilities must scope the exact sidecar and arguments with `shell:allow-spawn` or `shell:allow-execute`; no arbitrary shell commands should be exposed to the frontend.

## Parser Contract

Future web providers may parse only one of these sanitized inputs:

- visible text extracted from whitelisted locators;
- structured accessibility or ARIA snapshots;
- a provider-specific structured state object built from visible page fields.

Future web providers must not parse, store, log, or fixture raw authenticated HTML, full page text, network responses, cookies, tokens, auth headers, account identifiers, screenshots, or browser error dumps.

Provider parsers must return lower-confidence or `unknown` snapshots when:

- the user is logged out;
- MFA, CAPTCHA, bot checks, or interstitials appear;
- required fields are absent;
- visible data is partial or ambiguous;
- a page layout changes unexpectedly;
- parsing times out.

Successful parser output must include:

- service and provider ID;
- source `web`;
- confidence;
- last official check timestamp;
- visible fields used for the snapshot contract;
- sanitized status and reason codes.

Partial visible data must not be used to invent missing percentages or reset times.

## Security Controls

The automation session manager must:

- use separate marker-owned profile directories per service;
- launch with no default browser profile import;
- disable or avoid save-password, autofill, and password-manager prompts where the backend supports it;
- refuse to clear profile data unless the marker and canonical path checks pass immediately before deletion;
- stop any managed browser process before deleting browser session data;
- sanitize lifecycle logs to provider ID, source, status code, timestamps, and redacted profile labels only.

Authenticated official pages must never be loaded in the main Tauri webview. The main app webview should only call local commands and render sanitized IPC models. No CSP expansion for official remote usage pages is needed unless a separate, non-main login window design is explicitly approved later.

## Proceed/Defer Decision

Proceed with the Playwright headed Chromium sidecar spike. The first implementation boundary is an internal launch request contract based on Playwright's `chromium.launchPersistentContext(userDataDir, { headless: false, args })` shape, with raw profile paths kept out of diagnostics. The second boundary is a tested sidecar JSON protocol that accepts raw `userDataDir` only on stdin for the sidecar process and emits only sanitized status metadata. Rust now serializes the matching `launchLogin` request shape, spawns the sidecar through the Tauri shell plugin when available, validates the sanitized launch response, and fails closed with `login_required` when the sidecar is unavailable. Linux `externalBin` AppImage packaging is verified for the generated Node sidecar, and local headed sidecar launch has been validated against both official URLs with distinct temporary profiles that persist across relaunch, preserve disabled password/autofill preferences, and do not import seeded fake default Chrome/Chromium profile sentinels. Real authenticated login, app-owned profile persistence, and profile-content validation still require manual CachyOS KDE/Wayland validation.

Defer implementation of Codex and Claude web providers until these manual checks pass on CachyOS KDE/Wayland:

- headed login opens visibly for both services;
- app-owned profile persistence survives restart;
- service profiles remain separate;
- no default browser cookies or credentials are imported;
- no saved credentials are present after login;
- official pages expose stable parseable visible fields;
- logs contain no sensitive page content.
