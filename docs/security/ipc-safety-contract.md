# IPC Safety Contract

Scope: implemented Tauri invoke commands and emitted events as of this checkpoint. The managed browser login flow is not implemented yet; `start_provider_login` defines the sanitized login-required IPC boundary only.

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
| `start_provider_login` | `ProviderLoginStart` | Authenticated page content, browser profile data, cookies, tokens, and account identifiers |
| `inspect_provider_profile` | `ProviderProfileInspection` | Browser profile paths and contents, cookies, tokens, auth data, account identifiers, storage contents, and preference file contents |
| `hide_main_window` | `WindowVisibility` | Usage data, browser profile data, cookies, tokens, and account identifiers |
| `clear_cached_snapshots` | `UsageDisplayState` | Raw local logs, raw page HTML/text, account identifiers |
| `clear_provider_profile` | `ClearedProviderProfile` | Browser profile paths and contents |
| `reset_provider_session` | `ClearedProviderProfile` | Browser profile paths and contents |
| `get_log_location` | `LogLocation` | Log contents |

Command errors use `CommandError` with stable `code` and sanitized `message` fields. Command errors must not include raw filesystem paths, raw browser errors, raw provider records, page HTML/text, account identifiers, cookies, tokens, or auth headers.

Managed browser session failures are mapped to `browser_session_unavailable` before reaching the frontend. They must not expose process IDs, launch arguments, profile paths, raw process errors, cookies, tokens, account identifiers, or authenticated page content.

Browser profile inspection failures are mapped to `browser_profile_inspection_unavailable` before reaching the frontend. They must not expose profile paths, raw filesystem errors, preference file contents, cookie databases, local storage, tokens, account identifiers, or authenticated page content.

Rust IPC models use serde `camelCase` fields for structs and lowercase strings for enum values. The current stable string values are:

| Model | Field | Values |
| --- | --- | --- |
| `Service` | `service` | `codex`, `claude` |
| `UsageSource` | `source` | `local`, `web`, `merged`, `fake` |
| `UsageConfidence` | `confidence` | `high`, `medium`, `low`, `unknown` |
| `UsageRefreshStatus` | `status` | `started`, `finished`, `failed` |

Event payloads:

| Event | Payload | Sensitive data intentionally excluded |
| --- | --- | --- |
| `usage://snapshots-updated` | `UsageDisplayState` | Raw local logs, raw page HTML/text, account identifiers |
| `usage://refresh-started` | `UsageRefreshEvent` | Provider internals and raw errors |
| `usage://refresh-finished` | `UsageRefreshEvent` | Provider internals and raw errors |
| `usage://provider-error` | `UsageProviderErrorEvent` | Raw provider errors and raw source data |
| `settings://updated` | `AppConfig` | Browser profile contents, cookies, tokens, local provider records |
| `login://required` | `LoginRequiredEvent` | Authenticated page content, browser profile data, cookies, tokens, and account identifiers |
| `session://reset` | `ClearedProviderProfile` | Browser profile paths and contents |

`UsageSnapshot.details` may contain stable status codes, provider IDs, aggregate counts, window metadata, timestamps, and sanitized reason codes. It must not contain raw Claude Code JSONL rows, raw Codex SQLite rows, prompts, responses, account identifiers, cookies, tokens, auth headers, browser profile contents, or authenticated page HTML/text.
