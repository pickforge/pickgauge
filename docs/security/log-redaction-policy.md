# Log Redaction Policy

ForgeGauge logs must be safe to show to the app user and safe to attach to a bug report after review.

Allowed log fields:

- Stable provider, command, and event status codes.
- Service names, provider IDs, source types, and confidence levels.
- Sanitized counts, durations, timestamps, and retry/backoff metadata.
- App-owned log file names and app-owned profile labels.

Disallowed log fields:

- Cookies, tokens, auth headers, passwords, session IDs, or browser storage values.
- Raw authenticated page HTML, visible page text, network responses, or screenshots.
- Account identifiers, email addresses, organization names, or billing identifiers.
- Raw local Claude Code JSONL rows, raw Codex SQLite rows, prompts, responses, or tool inputs.
- Default browser profile paths, imported browser profile paths, or full browser launch errors.

Path handling:

- Prefer status codes over paths.
- Redact the home directory as `~` when a path is needed for user-visible diagnostics.
- Do not log browser profile contents. App-owned profile deletion reports only service, result, and timestamp.

Failure handling:

- Provider failures should log or expose stable codes such as `missing_data`, `parse_failed`, `login_required`, `timed_out`, or `unexpected_ui`.
- Raw parser, browser, filesystem, and network errors may be used internally for debugging only after they are mapped to a sanitized public code/message.
