# ForgeGauge

### The Pickforge AI Usage Tray

ForgeGauge is a privacy-conscious Linux tray app concept for tracking remaining AI usage across Codex and Claude Code. It is designed to keep quota awareness visible without storing passwords, uploading account data, or pretending best-effort estimates are exact.

> **Status:** planning and implementation blueprint. This repository currently contains the product spec and implementation plan for the app.

## Why this name?

**ForgeGauge** is my recommended name: it ties naturally to **Pickforge**, describes the tray gauge experience, and feels more brandable than the purely descriptive “AI Usage Tray.” The repository can still keep `ai-usage-tray` for discoverability.

## What it will do

- Show Codex and Claude Code usage from a KDE/Linux system tray icon.
- Alternate the tray gauge between services on a configurable interval.
- Open a compact popup with remaining percentage, source, confidence, and last update time.
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

## Roadmap

1. Bootstrap the Tauri + Svelte app shell.
2. Validate KDE/Wayland tray and popup behavior.
3. Render the alternating Codex/Claude gauge icon.
4. Add persistent settings and provider toggles.
5. Build local usage providers.
6. Spike browser automation for official-page reads.
7. Add opt-in web providers with isolated sessions.
8. Merge official baselines with local usage deltas.
9. Package for daily Linux desktop use.

## Project documents

- [`codex-claude-usage-tray-spec.md`](codex-claude-usage-tray-spec.md) — product and architecture spec.
- [`codex-claude-usage-tray-implementation-plan.md`](codex-claude-usage-tray-implementation-plan.md) — phased implementation plan, validation gates, and security checklist.
