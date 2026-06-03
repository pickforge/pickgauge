# IPC Safety Contract

Scope: implemented Tauri invoke commands and emitted events as of this checkpoint. Browser login/session commands are not implemented yet and must update this contract when added.

Command return models:

| Command | Success model | Sensitive data intentionally excluded |
| --- | --- | --- |
| `get_app_config` | `AppConfig` | Browser profile contents, cookies, tokens, local provider records |
| `update_app_config` | `AppConfig` | Browser profile contents, cookies, tokens, local provider records |
| `get_usage_snapshots` | `UsageSnapshot[]` | Raw local logs, raw page HTML/text, account identifiers |
| `get_display_state` | `UsageDisplayState` | Raw local logs, raw page HTML/text, account identifiers |
| `refresh_usage` | `UsageDisplayState` | Raw provider errors, raw local logs, raw page HTML/text |
| `refresh_provider` | `UsageDisplayState` | Raw provider errors, raw local logs, raw page HTML/text |
| `open_official_usage_page` | `OfficialUsagePage` | Authenticated page content and browser profile data |
| `clear_cached_snapshots` | `UsageDisplayState` | Raw local logs, raw page HTML/text, account identifiers |
| `clear_provider_profile` | `ClearedProviderProfile` | Browser profile paths and contents |
| `get_log_location` | `LogLocation` | Log contents |

Command errors use `CommandError` with stable `code` and sanitized `message` fields. Command errors must not include raw filesystem paths, raw browser errors, raw provider records, page HTML/text, account identifiers, cookies, tokens, or auth headers.

Event payloads:

| Event | Payload | Sensitive data intentionally excluded |
| --- | --- | --- |
| `usage://snapshots-updated` | `UsageDisplayState` | Raw local logs, raw page HTML/text, account identifiers |
| `usage://refresh-started` | `UsageRefreshEvent` | Provider internals and raw errors |
| `usage://refresh-finished` | `UsageRefreshEvent` | Provider internals and raw errors |
| `usage://provider-error` | `UsageProviderErrorEvent` | Raw provider errors and raw source data |
| `settings://updated` | `AppConfig` | Browser profile contents, cookies, tokens, local provider records |

`UsageSnapshot.details` may contain stable status codes, provider IDs, aggregate counts, window metadata, timestamps, and sanitized reason codes. It must not contain raw Claude Code JSONL rows, raw Codex SQLite rows, prompts, responses, account identifiers, cookies, tokens, auth headers, browser profile contents, or authenticated page HTML/text.
