# Codex + Claude Code Usage Tray App Spec

## Goal

Build a personal KDE/Linux tray app that shows remaining usage for:

- Codex: <https://chatgpt.com/codex/cloud/settings/analytics>
- Claude Code: <https://claude.ai/new#settings/usage>

The app should combine local CLI usage estimates with opt-in scraping of official usage pages.

## Recommended Stack

- **Rust** for backend, tray control, providers, config, and parsing.
- **Tauri v2** for lightweight desktop/tray app behavior.
- **Svelte** for the compact popup and settings UI.

## Core Concept

The app uses two data sources:

1. **Local providers**
   - Read local Claude Code and Codex CLI usage/session data.
   - Fast and passive.
   - Updated frequently.
   - Marked as estimated.

2. **Experimental web providers**
   - Use a dedicated browser profile.
   - User logs in manually.
   - App reads visible usage data from official usage pages.
   - Updated less frequently.
   - Disabled by default and explicitly opt-in.

The merged result should be shown honestly with a confidence label.

## Architecture

```text
App
├─ Tray controller
│  ├─ Alternating gauge icon
│  └─ Click opens compact popup
│
├─ Usage engine
│  ├─ Claude local provider
│  ├─ Codex local provider
│  ├─ Claude web provider
│  ├─ Codex web provider
│  └─ Usage merger/estimator
│
├─ Browser session manager
│  ├─ Dedicated browser profile
│  ├─ Manual login
│  └─ Scrape visible usage UI
│
├─ Config store
└─ Popup/settings UI
```

## Data Model

```ts
type UsageSnapshot = {
  service: "codex" | "claude";
  remainingPercent: number | null;
  usedPercent: number | null;
  resetAt?: string;
  source: "local" | "web" | "merged";
  confidence: "high" | "medium" | "low" | "unknown";
  lastUpdated: string;
  details?: Record<string, unknown>;
};
```

## Local Provider Behavior

Runs every `30–60s`.

### Claude Code

Likely data sources:

```text
~/.claude/projects/**/*.jsonl
Claude Code statusline data
ccusage-compatible parsing
```

Expected parsed data:

- timestamps
- model
- input/output/cache tokens
- session blocks
- estimated cost/usage
- rolling window activity

Limitation: local data only sees Claude Code usage on this machine, not Claude web/app usage.

### Codex

Likely data sources:

```text
~/.codex/*
Codex CLI session/history/status files
Codex statusline or /status-derived information if available
```

Codex local usage is likely lower-confidence until the available local state is inspected.

## Web Provider Behavior

Runs every `15–60min`, plus manual refresh.

Flow:

1. Open a dedicated browser profile.
2. User manually logs in to OpenAI/Claude.
3. Store only isolated session state, preferably via system secret storage where applicable.
4. Navigate to the official usage page.
5. Read visible usage text/progress indicators.
6. If login expires, MFA/CAPTCHA appears, or parsing fails, mark the provider as `unknown` and ask for re-login/manual refresh.

No password storage, no CAPTCHA bypass, and no scraping as a default behavior.

## Merge Logic

Use web data as the baseline and local data as the recent delta.

```text
web baseline + local usage delta = merged estimate
```

Example:

```text
10:00 web says Claude has 80% remaining
10:00–10:30 local usage estimates 8% consumed
10:30 app shows ~72% remaining
```

Confidence examples:

- `high`: fresh web reading from official page.
- `medium`: web baseline plus recent local delta.
- `low`: local-only estimate.
- `unknown`: no usable data.

## Tray UI

- One circular gauge icon.
- Alternates every `5–10s`.
- Blue = Codex.
- Orange = Claude.
- Gray = unknown.
- Red/low indicator when below threshold.

## Popup UI

The popup opens when clicking the tray icon and dismisses when clicking outside.

Example:

```text
Codex
72% remaining
Source: web baseline + local estimate
Last official check: 18 min ago

Claude Code
41% remaining
Source: local estimate
Last official check: unavailable
```

Actions:

- Refresh now
- Open official Codex page
- Open official Claude usage page
- Settings

## Settings

Minimum settings:

- Enable/disable Codex.
- Enable/disable Claude.
- Enable/disable local providers.
- Enable/disable experimental web providers.
- Refresh interval.
- Gauge switch interval.
- Browser profile/session path.
- Low-usage warning threshold.
- Optional manual plan/limit configuration.

## Implementation Plan

### Phase 1 — App Shell

- Bootstrap Tauri v2 + Svelte.
- Add tray icon.
- Add popup window.
- Add static fake usage data.

### Phase 2 — Gauge

- Generate dynamic tray gauge icons.
- Add alternating Codex/Claude display.
- Add low/unknown states.

### Phase 3 — Config

- Store settings locally.
- Enable/disable providers.
- Configure refresh intervals.
- Store browser profile path.

### Phase 4 — Local Providers

- Implement Claude local usage parser.
- Implement Codex local usage parser.
- Add confidence labels.

### Phase 5 — Web Providers

- Add dedicated browser-session manager.
- Add manual login flow.
- Scrape visible usage text/progress from the two official URLs.
- Store only isolated session state.

### Phase 6 — Merge Engine

- Combine web baseline with local delta.
- Detect stale data.
- Surface confidence clearly.

### Phase 7 — KDE Polish

- Test on CachyOS KDE/Wayland.
- Add autostart option.
- Package as AppImage or local binary first.

## Main Risks

- Official website UI can change and break scraping.
- Website scraping requires careful session handling.
- Codex local data may be incomplete or stale.
- Claude local data cannot see Claude web/app usage.
- Tauri tray behavior on KDE/Wayland needs testing.
- Web provider should remain explicit opt-in.

## MVP Recommendation

Build in this order:

```text
tray + fake data
→ popup
→ local providers
→ web providers
→ merge logic
→ KDE packaging/autostart
```
