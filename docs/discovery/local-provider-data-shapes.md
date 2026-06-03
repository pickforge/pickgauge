# Local Provider Data Shape Discovery

Date: 2026-06-03

Scope: read-only inspection of local Claude Code and Codex data roots on the target development machine. This note records file shapes, aggregate counts, JSON keys, and SQLite schema fields only. It intentionally does not include prompt text, response text, raw JSONL rows, account identifiers, auth data, or full local paths.

## Claude Code

Observed roots:

- `~/.claude/projects/**/*.jsonl`
- `~/.claude/history.jsonl`
- `~/.claude/settings.json` backups
- `~/.claude/cache/`, `~/.claude/plugins/`, `~/.claude/debug/`, and IDE lock files

Usage-relevant files:

- `~/.claude/projects/**/*.jsonl` is the primary local usage source.
- The inspected machine had 46 project JSONL files, about 164 MB total, with the largest file about 57 MB.
- JSONL records include top-level fields such as `timestamp`, `type`, `uuid`, `sessionId`, `cwd`, `gitBranch`, `message`, `requestId`, `durationMs`, and several tool/plugin metadata keys.
- Message records include nested `message` fields such as `id`, `type`, `role`, `model`, `content`, `usage`, `stop_reason`, and `stop_details`.
- Nested `message.usage` keys include `input_tokens`, `output_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`, `cache_creation`, `server_tool_use`, `service_tier`, `inference_geo`, `iterations`, and `speed`.
- Aggregate shape inspection found 3,194 records with `message.usage`, matching the count of records with `message.model`.
- No cost fields were observed in the structural scan.

Non-primary files:

- `~/.claude/history.jsonl` contains prompt/history text and timestamps, but not enough usage fields for token accounting.
- `~/.claude/settings.json` backups, plugin catalogs, caches, debug text, and IDE lock files are not suitable usage sources.

Source precedence:

1. `~/.claude/projects/**/*.jsonl` assistant message records with `message.usage`.
2. `~/.claude/history.jsonl` only for optional activity recency metadata, not token accounting.
3. Settings, plugin, cache, debug, and lock files are ignored for usage estimates.

Parser implications:

- Treat records as machine-local activity, not account-wide usage.
- Extract timestamps, model, session, and usage token fields only.
- Do not persist or surface `content`, `cwd`, `gitBranch`, raw IDs, tool payloads, or debug text.
- Return `remaining_percent = None` until manual calibration maps local token activity to a plan/window.
- Missing or unreadable roots should produce `unknown` snapshots with stable status codes.

## Codex

Observed roots:

- `~/.codex/history.jsonl`
- `~/.codex/session_index.jsonl`
- `~/.codex/state_5.sqlite`
- `~/.codex/logs_2.sqlite`
- `~/.codex/goals_1.sqlite`
- `~/.codex/memories_1.sqlite`
- `~/.codex/auth.json`
- `~/.codex/cache/` and plugin/tooling data

Usage-relevant candidates:

- `~/.codex/state_5.sqlite` has a `threads` table with columns including `id`, `created_at`, `updated_at`, `source`, `model_provider`, `cwd`, `title`, `tokens_used`, `has_user_event`, `archived`, `git_sha`, `git_branch`, `model`, `reasoning_effort`, `created_at_ms`, `updated_at_ms`, and `preview`.
- `~/.codex/history.jsonl` exposes `session_id`, `ts`, and `text`; it is useful for activity recency only and should not be used for usage totals because it stores text rather than token accounting.
- `~/.codex/session_index.jsonl` exposes `id`, `thread_name`, and `updated_at`; it is useful for session recency/indexing only.
- `~/.codex/logs_2.sqlite` has a `logs` table with timestamp, level, target, thread/process fields, and `estimated_bytes`; it is not a primary usage source.
- No stable local cost field was observed in the structural scan.

Non-primary files:

- `auth.json` must never be read by providers.
- cache, app connector, plugin, marketplace, memory, goal, shell snapshot, and generated image files are not usage sources.

Source precedence:

1. `~/.codex/state_5.sqlite` `threads.tokens_used` with timestamp/model metadata, if it proves stable across Codex versions.
2. `~/.codex/session_index.jsonl` and `~/.codex/history.jsonl` only for optional recency metadata, not usage totals.
3. `logs_2.sqlite` only for diagnostics if needed and sanitized; do not use `estimated_bytes` as token usage.
4. Auth, cache, plugin, memory, goal, and generated asset files are ignored.

Parser implications:

- Treat `tokens_used` as local thread activity, not account-wide remaining usage.
- Do not read or persist `auth.json`.
- Redact or omit `cwd`, `title`, `preview`, raw thread IDs, git metadata, and text fields from details.
- Return `remaining_percent = None` until manual calibration maps local token activity to a plan/window.
- Missing or unreadable roots should produce `unknown` snapshots with stable status codes.

## Fixture Strategy

- Prefer synthetic fixtures that preserve only key structure and numeric usage fields.
- `src-tauri/tests/fixtures/codex-local/sanitized-state.sql` preserves the discovered Codex `threads` schema shape and numeric token/timestamp fields with redacted placeholder text only.
- Captured real fixtures require explicit user consent before capture.
- Any captured fixture must remove prompt/response text, paths, account identifiers, auth data, IDs, git metadata, and raw tool payloads before commit.

## Local Scan Policy

Before manual quota/window calibration exists, local providers aggregate all bounded machine-local activity they can parse and return `remaining_percent = None`. They do not apply a rolling usage window or infer account-wide remaining usage.

Claude Code policy:

- Scan only `~/.claude/projects/**/*.jsonl` files.
- Use exact `.jsonl` extensions only; rotated or backup files such as `.jsonl.1` are ignored unless they are restored as normal `.jsonl` files.
- Bound each refresh to 512 JSONL files and 100,000 non-empty records.
- Count malformed, truncated, or non-JSON lines as `invalidRecords` without surfacing raw content.
- Count unreadable files and continue scanning other files where possible.
- Preserve source RFC3339 timestamps as metadata and expose first/last timestamp strings only.

Codex policy:

- Read only `~/.codex/state_5.sqlite` and never read `auth.json`.
- Bound each refresh to the 10,000 most recently updated `threads` rows.
- Treat `tokens_used` as usable only when it is a non-negative SQLite integer.
- Count malformed token rows as `invalidRecords` without failing the whole snapshot when at least one usable row exists.
- Treat missing, unreadable, corrupt, or schema-incompatible state databases as sanitized `unknown` snapshots.
- Use Unix epoch milliseconds from `updated_at_ms`, falling back to `updated_at * 1000` for metadata.

## Open Implementation Decisions

- Incremental scan/cache strategy.
- Manual calibration schema for mapping local tokens to plan/window percentages.
- Whether Codex `threads.tokens_used` remains stable enough across versions to become the primary Codex local source.
