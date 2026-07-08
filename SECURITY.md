# Security policy

PickGauge is a local-first Linux tray app that tracks how much Codex and Claude Code quota you have left. It is built to do that without ever holding your account.

## Privacy and security model

- **No passwords.** PickGauge never asks for, sees, or stores provider passwords. For its default readings it reuses the OAuth tokens the Codex and Claude Code CLIs already wrote to disk (`~/.codex/auth.json`, `~/.claude/.credentials.json`).
- **Tokens stay in memory.** Tokens are read at refresh time, used to call the provider's own usage endpoint, refreshed in memory when they expire, and never copied into PickGauge's config, cache, logs, or local history.
- **Opt-in, isolated web reads.** Browser-based reading of the official usage pages is disabled by default. When enabled, it runs only in dedicated, app-owned browser profiles under `com.pickforge.pickgauge` that you log into yourself — never your personal browser, never a shared cookie jar.
- **Anonymous crash reports.** Crash and error reporting is on by default in release builds and can be turned off in Settings → Crash reports. Reports go to Sentry with crash stack traces, OS version, and app version. Native crash dumps include a snapshot of process memory, which may contain fragments of recent in-memory data. Reports never intentionally include usage measurements, project files, or personal data; the hostname is stripped. Crash reporting is disabled in development builds unless `PICKGAUGE_SENTRY_DEBUG=1` is set.
- **Data minimization.** No raw page HTML, auth headers, cookies, tokens, or account identifiers are written to logs, fixtures, or the local SQLite history — only computed percentages, confidence, source, and timestamps.
- **Honest confidence.** Every reading is labeled official, estimated, merged, stale, or unavailable.

## What touches the network

PickGauge makes no analytics calls. Usage-data requests only go to the providers' own endpoints, to compute your real remaining quota. Release builds also send anonymous crash and error reports to Sentry unless disabled in Settings.

| Purpose | Endpoint |
| --- | --- |
| Codex usage | `GET https://chatgpt.com/backend-api/codex/usage` |
| Codex token refresh | `POST https://auth.openai.com/oauth/token` |
| Claude usage | `GET https://api.anthropic.com/api/oauth/usage` |
| Claude token refresh | `POST https://platform.claude.com/v1/oauth/token` |
| Crash and error reports | `https://o4511699702317056.ingest.us.sentry.io/` |

When opt-in web reads are enabled, the browser sidecar additionally loads the providers' official usage pages inside the dedicated profile, using the session you logged in there yourself.

## Reporting a vulnerability

Please report security issues privately:

- GitHub security advisories: <https://github.com/pickforge/pickgauge/security/advisories/new>
- Email: <security@pickforge.dev>

Do not open public issues for security reports. We aim to acknowledge within a few days.
