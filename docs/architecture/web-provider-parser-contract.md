# Web Provider Parser Contract

This contract defines the parser boundary for official web providers. It does not select or launch a browser automation backend. A session manager may feed this parser only sanitized visible fields gathered from official pages or derived inside the sidecar from a provider response.

## Sanitized Input Format

Parser input is a provider-specific structured state object with these fields:

- `service`: `codex`, `claude`, `grok`, or `ollama`.
- `pageState`: `usage`, `logged_out`, `mfa_required`, `captcha_or_bot_check`, `network_unavailable`, `timed_out`, or `unexpected_ui`.
- `remainingPercent`: number from a visible remaining-percent field, or `null`.
- `usedPercent`: number from a visible used-percent field, or `null`.
- `resetAt`: RFC3339 timestamp derived from visible reset text, or `null`.
- `visibleFields`: names of visible fields used by the parser.
- `products`: optional Grok product entries with a known product enum and numeric `usagePercent` only.

The parser must never receive raw authenticated HTML, full page text, screenshots, network responses, cookies, tokens, auth headers, account identifiers, browser storage, or raw browser errors.

## Required Visible Fields

Codex accepted field names:

- `remaining_percent`
- `used_percent`
- `reset_at`
- `quota_window`
- `plan_label`

Claude accepted field names:

- `remaining_percent`
- `used_percent`
- `reset_at`
- `quota_window`
- `plan_label`

Ollama accepted field names:

- `remaining_percent`
- `used_percent`
- `reset_at`
- `quota_window`
- `plan_label`

Grok accepted field names:

- `remaining_percent`
- `used_percent`
- `reset_at`
- `quota_window`

Grok's sidecar-only HTTP parser treats an absent proto3 `creditUsagePercent` as zero used. It receives only the managed profile's `GET grok.com/rest/grok/credits` response, derives the weekly window and optional product percentages, and never forwards its raw JSON or cookies to Rust. Grok has no five-hour window. On-demand dollar credits are not read.

At least one of `remaining_percent` or `used_percent` is required for a successful web usage snapshot. `reset_at`, `quota_window`, and `plan_label` are optional sanitized context fields.

## Fallback Behavior

- If the page is logged out, return `login_required` with unknown confidence.
- If MFA is required, return `mfa_required` with unknown confidence.
- If CAPTCHA or bot checks appear, return `captcha_or_bot_check` with unknown confidence.
- If the network is unavailable, return `network_unavailable` with unknown confidence.
- If parsing or page loading times out, return `timed_out` with unknown confidence.
- If the UI is unexpected, return `unexpected_ui` with unknown confidence.
- If required visible percentage fields are absent, return `missing_data` with unknown confidence.
- If visible percentage fields are inconsistent, out of range, or non-finite, return `parse_failed` with unknown confidence.
- If a reset timestamp is present but not RFC3339, return `parse_failed` with unknown confidence.
- If an unsupported visible field name appears, return `parse_failed` without echoing the unsupported field.

Successful parser output uses `source = "web"`, `confidence = "high"`, a provider-specific web ID, `lastOfficialCheckAt`, and sanitized `visibleFields`. Grok uses `providerId = "grok.web"`, a single `windows.week` entry, and a sanitized `products` array.

## Fixture Workflow

Committed fixtures must use the sanitized structured input format above. Fixture regeneration from real official pages requires explicit user consent before capture.

Allowed fixture content:

- field names from the accepted lists;
- numeric percentages;
- RFC3339 timestamps;
- provider service and page-state codes.

Disallowed fixture content:

- raw page HTML or full visible text;
- account identifiers, email addresses, organization names, or billing identifiers;
- cookies, session tokens, auth headers, browser storage, local storage, or IndexedDB values;
- screenshots, network responses, raw browser errors, or full local paths.

Before committing fixture updates, run the web-visible parser fixture sanitization test and inspect the diff for disallowed content.
