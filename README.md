# PickGauge

### Local AI usage visibility from the tray

![PickGauge brand preview](assets/branding/social-card.png)

PickGauge is a privacy-conscious Linux tray app for tracking remaining AI usage across Codex and Claude Code. It keeps quota awareness visible without storing passwords, uploading account data, or pretending best-effort estimates are exact.

> **Status:** Tauri/Svelte desktop app with a branded dashboard, usage history, floating button, sound cues, persisted settings, tray wiring, app icons, and release automation. Web providers remain opt-in and await authenticated validation.

## Desktop app

PickGauge ships a full Tauri 2 + Svelte 5 GUI in the Pickforge "one ember on a cold canvas" design system:

- **Dashboard** — half-arc gauges per service with confidence, source, and freshness labels, plus local activity stats and a 14-day token chart.
- **History** — local Codex and Claude Code usage grouped by **days, weeks, or months** (scanned from local activity files, up to a year back), with per-period totals and a gauge trail of the lowest remaining percentage per day (stored in a local SQLite history at `~/.local/share/com.pickforge.pickgauge/history.db`).
- **Floating button** — a draggable always-on-top capsule with live mini-gauges. Click it to open the app, right-click to refresh. It never takes keyboard focus. On Wayland the app runs under XWayland so always-on-top works (set `PICKGAUGE_NATIVE_WAYLAND=1` to opt out).
- **Sounds, not notifications** — short synthesized chimes when a gauge crosses below the low-usage threshold and when it recovers (toggle in Settings). PickGauge never posts desktop notifications.
- **Settings** — services, providers, refresh rhythm, quota calibration, browser profiles, autostart, sounds, and the floating button, all persisted locally.

## What it will do

- Show Codex and Claude Code usage from a KDE/Linux system tray icon.
- Alternate the tray gauge between services on a configurable interval.
- Open a compact popup with remaining percentage, source, confidence, and last update time.
- Persist basic provider/service settings locally.
- Combine local CLI usage estimates with optional official-page readings.
- Clearly label data as `high`, `medium`, `low`, or `unknown` confidence.
- Fail gracefully when local files are missing, login expires, MFA appears, or official pages change.

## Privacy principles

| Principle | Behavior |
| --- | --- |
| No password storage | Users authenticate manually in isolated browser profiles. |
| Opt-in web reads | Browser-based official usage checks are disabled by default. |
| Local-first estimates | CLI/session files are used passively where possible. |
| Data minimization | No raw page HTML, auth headers, cookies, tokens, or account identifiers in app logs or fixtures. |
| Honest confidence | The UI shows whether usage is official, estimated, merged, stale, or unavailable. |

## Planned services

| Service | Source | Confidence |
| --- | --- | --- |
| Codex | Local CLI/session data and optional official analytics page | Low to high |
| Claude Code | Local JSONL/status data and optional official usage page | Low to high |

Official usage pages:

- Codex: <https://chatgpt.com/codex/cloud/settings/analytics>
- Claude Code: <https://claude.ai/new#settings/usage>

## Architecture at a glance

```text
Tray controller
├─ Dynamic gauge icon
├─ Compact popup
└─ Settings actions

Usage engine
├─ Codex local provider
├─ Claude local provider
├─ Optional Codex web provider
├─ Optional Claude web provider
└─ Merger and confidence model

Privacy boundary
├─ Dedicated browser profiles
├─ Sanitized provider results
└─ Guarded cache/session cleanup
```

## Proposed stack

- **Rust** for the backend, usage engine, tray control, config, and provider logic.
- **Tauri v2** for the lightweight desktop shell and tray integration.
- **Svelte** for the popup and settings UI.
- **KDE/Wayland first**, with CachyOS Linux as the target validation environment.

## Branding

Brand assets live in `assets/branding/`. The app uses the Pickforge Studio v2 dark/ember system:

- `logo-mark.svg` and `logo-lockup-on-dark.svg` in the popup UI.
- `brand-pattern.svg` and `hero-art.png` for the app surface.
- `app-icon.svg` for generated Tauri app icons.
- `tray-codex-64.png`, `tray-claude-64.png`, `tray-low-64.png`, and `tray-unknown-64.png` for tray states.

After changing the source app icon, regenerate platform icons with:

```bash
npm run tauri -- icon assets/branding/app-icon.svg
```

## Current MVP behavior

- Branded Tauri tray app shell.
- Branded popup with fake Codex and Claude Code snapshots.
- Local settings persistence for enabled services, provider toggles, refresh intervals, tray switch interval, and low-usage threshold.
- Branded tray icons that rotate between Codex and Claude Code and can switch to low/unknown states.

## Why Tauri?

PickGauge needs a real desktop shell: a persistent tray icon, native windows, local filesystem access for CLI usage data, isolated browser/session handling, and packaged installers. Tauri gives the app a Rust backend for the privacy-sensitive work while keeping the popup/settings UI lightweight with Svelte instead of shipping a full Electron runtime.

## Releases and platform support

GitHub Actions is configured to create queued releases from `main` pushes once the Tauri app source exists. Release notes are generated automatically and artifacts are uploaded for:

- Linux AppImage
- Windows installers
- macOS Intel builds
- macOS Apple Silicon builds

PickGauge is still **Linux/KDE-first**. Windows and macOS builds are produced automatically, but they are currently **untested**. If you try them, personal experience reports, issues, and pull requests are very welcome so cross-platform support can improve.

On CachyOS/Arch-like systems, local AppImage bundling can fail because the linuxdeploy `strip` binary does not understand newer `.relr.dyn` ELF sections. Use the project script, which disables linuxdeploy stripping:

```bash
npm run build:appimage
```

The AppImage script also prepares the Linux Playwright sidecar executable under `src-tauri/binaries/` before invoking Tauri. Real headed web-provider login still requires a working local Node/Playwright runtime.

For local sidecar launch validation:

```bash
npx playwright install chromium
npm run test:sidecar-launch
```

## Roadmap

1. Bootstrap the Tauri + Svelte app shell.
2. Validate KDE/Wayland tray and popup behavior.
3. Render alternating branded Codex/Claude tray icons.
4. Add persistent settings and provider toggles.
5. Build local usage providers.
6. Spike browser automation for official-page reads.
7. Add opt-in web providers with isolated sessions.
8. Merge official baselines with local usage deltas.
9. Package for daily Linux desktop use, then expand automated Windows and macOS release artifacts.

## Project documents

- [`docs/plans/pickgauge-implementation-plan.md`](docs/plans/pickgauge-implementation-plan.md) — consolidated product spec, implementation plan, validation gates, and security checklist.
