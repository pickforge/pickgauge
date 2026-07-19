# Provider Refresh Lifecycle

This document defines the current PickGauge provider refresh policy. It must be updated before adding a browser automation backend or async provider runtime.

## Publication Ownership

`RefreshPublicationPolicy` in `src-tauri/src/refresh_publication.rs` owns publication after the usage engine accepts a display state. Startup, configuration-triggered, manual, scheduled, and targeted refreshes all use the same ordered policy:

1. emit `usage://refresh-started`;
2. run the engine/provider refresh;
3. emit `usage://snapshots-updated` for the accepted display state;
4. record history, persist the sanitized raw-snapshot projection, and evaluate sound cues;
5. surface sanitized `usage://provider-error` events;
6. emit exactly one terminal `usage://refresh-finished` event with `finished` or `failed`.

History, snapshot-cache, cue, and provider-error failures are nonfatal. A snapshot-update emit failure fails publication but still attempts the one `failed` terminal event. Cache clearing uses the policy's emit-only mode after clearing engine state and deleting the persisted cache, so it cannot recreate the cache through normal refresh effects.

The publication policy is separate from `ConfigMutationCoordinator`: configuration mutation and refresh publication serialize different state transitions. On app exit, publication shuts down before managed browser sessions, and no later refresh operation or publication effect is accepted.

## Scheduler Ownership

The current app does not create or own a Tokio runtime. Scheduled refresh is owned by the Tauri app process through `start_usage_scheduler` in `src-tauri/src/lib.rs`, which starts one background `std::thread`.

The scheduler thread calls `refresh_due_with_headless_web` through `RefreshPublicationPolicy`, then sleeps for `UsageEngine::scheduler_sleep_duration`. Provider work remains bounded and outside the usage-engine mutex.

Future async provider runtimes must define explicit task ownership before implementation. They must not create detached tasks whose lifecycle can outlive provider disablement, profile deletion, or app shutdown.

## Timeout Behavior

Local providers are bounded by scan limits instead of wall-clock timeouts:

- Claude JSONL scans use fixed file and record limits.
- Codex SQLite scans use fixed row limits and read-only connections.
- Overlapping refreshes for a provider key are skipped.
- Failed refreshes enter bounded retry/backoff.

Managed web refreshes are opt-in and use the app-owned Playwright sidecar. They use bounded sidecar acknowledgement and browser shutdown timeouts, provider retry/backoff, and stable sanitized statuses such as `timed_out`, `login_required`, `mfa_required`, `captcha_or_bot_check`, `unexpected_ui`, and `network_unavailable`.

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
