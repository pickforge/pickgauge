# `pickgauge usage --json`

`pickgauge usage --json` refreshes the local CLI, local-file, and daemon
providers, overlays the latest sanitized browser readings saved by the running
tray app, prints one JSON object, and exits. It never starts Tauri, opens a
window, or launches a browser.

On Windows, invoke the command from Command Prompt or PowerShell so PickGauge
can attach to the parent console and write its output.

## Version 1

```json
{
  "version": 1,
  "generatedAt": "2026-07-10T12:00:00Z",
  "services": [
    {
      "service": "codex",
      "label": "Codex",
      "status": "parsed",
      "plan": "Pro",
      "remainingPercent": 72,
      "usedPercent": 28,
      "resetAt": "2026-07-10T17:00:00Z",
      "windows": {
        "fiveHour": { "remainingPercent": 72, "usedPercent": 28, "resetAt": "2026-07-10T17:00:00Z" },
        "week": { "remainingPercent": 41, "usedPercent": 59, "resetAt": "2026-07-15T12:00:00Z" }
      },
      "source": "web",
      "confidence": "high",
      "lastUpdated": "2026-07-10T12:00:00Z",
      "staleSeconds": 0
    }
  ]
}
```

`version` is the schema version. Consumers should reject unknown versions.
`generatedAt`, `resetAt`, and `lastUpdated` are RFC 3339 timestamps.

Each enabled service appears once, ordered Codex, Claude Code, Grok, Ollama.
Disabled services are omitted. `status` is the sanitized provider status:
`parsed`, `login_required`, `not_configured`, `missing_data`,
`network_unavailable`, and related error codes are all valid values. Provider
errors still produce a row and the command exits successfully.

`remainingPercent`, `usedPercent`, and each rate-limit window may be `null`.
A `null` percentage means the provider has no gauge, not that the pool is
empty. `windows.fiveHour` and `windows.week` are copied from provider window
data when available; they are otherwise `null`. Claude may also provide
`windows.fable` for its separate Fable weekly allowance. `plan` may be present
without any percentage.

`source` is `web`, `local`, or `merged`; `confidence` is `high`, `medium`,
`low`, or `unknown`. `staleSeconds` is the non-negative age of `lastUpdated`
when the command generated the document, or `null` if the timestamp cannot be
read.

Without `--json`, `pickgauge usage` prints the same services in a compact
human-readable table. Invalid `usage` flags print `Usage: pickgauge usage
[--json]` to stderr and exit 2.
