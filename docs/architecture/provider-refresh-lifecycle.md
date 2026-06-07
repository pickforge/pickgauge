# Provider Refresh Lifecycle

This document defines the current PickGauge provider refresh policy. It must be updated before adding a browser automation backend or async provider runtime.

## Scheduler Ownership

The current app does not create or own a Tokio runtime. Scheduled refresh is owned by the Tauri app process through `start_usage_scheduler` in `src-tauri/src/lib.rs`, which starts one background `std::thread`.

The scheduler thread:

- emits `usage://refresh-started` before a scheduled cycle;
- calls `UsageEngine::refresh_due_and_emit`;
- emits `usage://refresh-finished` with `finished` or `failed`;
- sleeps for `UsageEngine::scheduler_sleep_duration`;
- never holds raw provider data outside `UsageEngine`.

Future async or browser-backed providers must define explicit task ownership before implementation. They must not create detached tasks whose lifecycle can outlive provider disablement, profile deletion, or app shutdown.

## Timeout Behavior

Local providers are bounded by scan limits instead of wall-clock timeouts:

- Claude JSONL scans use fixed file and record limits.
- Codex SQLite scans use fixed row limits and read-only connections.
- Overlapping refreshes for a provider key are skipped.
- Failed refreshes enter bounded retry/backoff.

Web providers are not implemented yet. Before a web provider is added, it must define:

- navigation timeout;
- visible-state extraction timeout;
- browser shutdown timeout;
- retry/backoff interaction;
- failure mapping to stable provider statuses such as `timed_out`, `login_required`, `mfa_required`, `captcha_or_bot_check`, `unexpected_ui`, or `network_unavailable`.

Until those web timeout rules exist in code and tests, web providers remain opt-in placeholders and direct web refresh commands fail closed.

## Cancellation Behavior

Current local refresh work is synchronous and short-bounded. Cancellation is cooperative at the scheduler/registry boundary:

- disabled providers are removed from the registry;
- active tracking keys for removed providers are discarded on config update;
- scheduled refreshes skip disabled or cooling-down providers;
- app-owned browser profile clearing verifies the marker before deletion.

Future browser-backed providers must add stronger cancellation:

- stop managed browser processes before profile deletion;
- track process handles per service;
- abort pending navigation/extraction on provider disablement;
- use timeout plus kill fallback for browser shutdown;
- avoid writing raw browser errors or authenticated page content during cancellation.

## Sanitized Lifecycle Logging

Provider lifecycle diagnostics may include:

- provider key and source;
- service;
- stable status code;
- refresh start/finish timestamps;
- consecutive failure count;
- backoff seconds;
- retry-after timestamp;
- bounded aggregate parser counters.

Provider lifecycle diagnostics must not include:

- raw local log rows;
- prompts, responses, tool inputs, or file contents;
- browser profile contents;
- cookies, tokens, auth headers, session IDs, or passwords;
- raw authenticated page HTML/text, screenshots, network responses, or account identifiers.
