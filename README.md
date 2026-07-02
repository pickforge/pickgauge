<p align="center">
  <img src="assets/branding/pickgauge-lockup-horizontal.svg" alt="PickGauge" width="560">
</p>

# PickGauge

A fuel gauge for your AI subscriptions. PickGauge is a privacy-conscious Linux tray app that tracks remaining Codex and Claude Code usage — keeping quota awareness visible without storing passwords, uploading account data, or pretending best-effort estimates are exact.

PickForge builds the app. PickGauge tells you how much agent fuel is left while you do it.

Local-first. Open source. Built for people who ship.

> **Status:** Tauri/Svelte desktop app with a branded dashboard, usage history, floating button, sound cues, persisted settings, tray wiring, app icons, and release automation. Web providers remain opt-in and await authenticated validation.

## Install

Release artifacts are built from `main` by GitHub Actions: Linux AppImage, Windows installers, macOS Intel and Apple Silicon builds. Download the latest from [Releases](https://github.com/pickforge/pickgauge/releases/latest). PickGauge is still **Linux/KDE-first** — Linux is the tested platform; the Windows and macOS builds are produced automatically but currently **untested**; experience reports are welcome.

On CachyOS/Arch-like systems, local AppImage bundling can fail because the linuxdeploy `strip` binary does not understand newer `.relr.dyn` ELF sections. Use the project script, which disables linuxdeploy stripping:

```bash
bun run build:appimage
```

The AppImage script also prepares the Linux Playwright sidecar executable under `src-tauri/binaries/` before invoking Tauri. Real headed web-provider login still requires a working local Node/Playwright runtime. For local sidecar launch validation:

```bash
bunx playwright install chromium
bun run test:sidecar-launch
```

## The desktop app

PickGauge ships a full Tauri 2 + Svelte 5 GUI in the Pickforge "one ember on a cold canvas" design system:

- **Dashboard** — half-arc gauges per service with confidence, source, and freshness labels, plus local activity stats and a 14-day token chart.
- **History** — local Codex and Claude Code usage grouped by **days, weeks, or months** (scanned from local activity files, up to a year back), with per-period totals and a gauge trail of the lowest remaining percentage per day (stored in a local SQLite history at `~/.local/share/com.pickforge.pickgauge/history.db`).
- **Floating button** — a draggable always-on-top capsule with live mini-gauges. Click it to open the app, right-click to refresh. It never takes keyboard focus. On Wayland the app runs under XWayland so always-on-top works (set `PICKGAUGE_NATIVE_WAYLAND=1` to opt out).
- **Sounds, not notifications** — short synthesized chimes when a gauge crosses below the low-usage threshold and when it recovers (toggle in Settings). PickGauge never posts desktop notifications.
- **Settings** — services, providers, refresh rhythm, quota calibration, browser profiles, autostart, sounds, and the floating button, all persisted locally.

<p align="center">
  <img src="assets/branding/pickgauge-dashboard-mock.svg" alt="PICKGAUGE · DASHBOARD — half-arc gauges with confidence labels, 14-day history, floating button, and the privacy boundary" width="900">
</p>

## What it will do

- Show Codex and Claude Code usage from a KDE/Linux system tray icon.
- Alternate the tray gauge between services on a configurable interval.
- Open a compact popup with remaining percentage, source, confidence, and last update time.
- Persist basic provider/service settings locally.
- Combine local CLI usage estimates with optional official-page readings.
- Clearly label data as `high`, `medium`, `low`, or `unknown` confidence.
- Fail gracefully when local files are missing, login expires, MFA appears, or official pages change.

Planned services:

| Service | Source | Confidence |
| --- | --- | --- |
| Codex | Local CLI/session data and optional official analytics page | Low to high |
| Claude Code | Local JSONL/status data and optional official usage page | Low to high |

Official usage pages:

- Codex: <https://chatgpt.com/codex/cloud/settings/analytics>
- Claude Code: <https://claude.ai/new#settings/usage>

## Security / Privacy

PickGauge reads how much quota you have left without ever holding your account.

- **No passwords, ever.** PickGauge never asks for, sees, or stores provider passwords. For its default readings it reuses the OAuth tokens the Codex and Claude Code CLIs already wrote to disk (`~/.codex/auth.json`, `~/.claude/.credentials.json`).
- **Tokens stay in memory.** Tokens are read at refresh time, used to call the provider's own usage endpoint, refreshed in memory when they expire, and never copied into PickGauge's config, cache, logs, or local history.
- **What leaves the machine, and only this.** To compute real remaining quota, PickGauge calls the same official endpoints the CLIs use — `chatgpt.com/backend-api/codex/usage` and `api.anthropic.com/api/oauth/usage`, plus the providers' OAuth refresh endpoints. No telemetry, no analytics, no third parties.
- **Web reads are opt-in and isolated.** Browser-based reading of the official usage pages is disabled by default. When enabled, it runs only in dedicated, app-owned browser profiles (under `com.pickforge.pickgauge`) that you log into yourself — never your personal browser, never a shared cookie jar.
- **Data minimization.** No raw page HTML, auth headers, cookies, tokens, or account identifiers are written to PickGauge logs, fixtures, or its local SQLite history — only computed percentages, confidence, source, and timestamps.
- **Honest confidence.** Every reading is labeled official, estimated, merged, stale, or unavailable, so you always know how much to trust the number.

## Architecture

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

Stack: **Rust** for the backend, usage engine, tray control, config, and provider logic; **Tauri v2** for the lightweight desktop shell and tray integration; **Svelte** for the popup and settings UI; **KDE/Wayland first**, with CachyOS Linux as the target validation environment.

PickGauge needs a real desktop shell: a persistent tray icon, native windows, local filesystem access for CLI usage data, isolated browser/session handling, and packaged installers. Tauri gives the app a Rust backend for the privacy-sensitive work while keeping the popup/settings UI lightweight with Svelte instead of shipping a full Electron runtime.

## Branding

Brand assets live in `assets/branding/`. The app uses the Pickforge Studio v2 dark/ember system:

- `logo-mark.svg` and `logo-lockup-on-dark.svg` in the popup UI.
- `brand-pattern.svg` and `hero-art.png` for the app surface.
- `app-icon.svg` for generated Tauri app icons.
- `tray-codex-64.png`, `tray-claude-64.png`, `tray-low-64.png`, and `tray-unknown-64.png` for tray states.

After changing the source app icon, regenerate platform icons with:

```bash
bun run tauri icon assets/branding/app-icon.svg
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
9. Packaged releases exist for Linux, Windows, and macOS; validate the macOS and Windows builds on native hosts.

Project documents: [`docs/plans/pickgauge-implementation-plan.md`](docs/plans/pickgauge-implementation-plan.md) — consolidated product spec, implementation plan, validation gates, and security checklist.

## Development

```bash
bun install              # install dependencies
bun run dev              # Vite dev server on 127.0.0.1:1420
bun run build            # build the Svelte front-end
bun run build:appimage   # bundle the Linux AppImage (prepares the Playwright sidecar)
bun run test             # unit tests, sidecar node tests, and sidecar package checks
bun run lint             # ESLint over src, scripts, and sidecars
bun run check            # svelte-check type checking
```

## License

MIT — see [LICENSE](LICENSE).

---

<p align="center">
  <a href="https://pickforge.dev">
    <img src="assets/branding/pickforge-studio-footer.svg" alt="Pickforge Studio — local-first, open source, built for people who ship" width="560">
  </a>
</p>
