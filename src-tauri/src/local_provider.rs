use crate::usage::{Service, UsageConfidence, UsageProviderId, UsageSnapshot, UsageSource};
use rusqlite::{types::ValueRef, Connection, OpenFlags, Row};
use serde::Deserialize;
use std::{
    collections::HashSet,
    env,
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

const CLAUDE_PROJECTS_DIR: &str = "projects";
const JSONL_EXTENSION: &str = "jsonl";
const MAX_CLAUDE_JSONL_FILES: usize = 512;
const MAX_CLAUDE_RECORDS_PER_REFRESH: u64 = 100_000;
const CODEX_STATE_DB_FILE: &str = "state_5.sqlite";
const MAX_CODEX_THREADS_PER_REFRESH: u64 = 10_000;
const LOCAL_WINDOW_POLICY: &str = "all_scanned_local_activity";
const CLAUDE_TIMESTAMP_SEMANTICS: &str = "source_rfc3339";
const CODEX_TIMESTAMP_SEMANTICS: &str = "unix_epoch_ms";

#[derive(Clone, Debug)]
pub struct ClaudeLocalProvider {
    data_root: PathBuf,
}

#[derive(Clone, Debug)]
pub struct CodexLocalProvider {
    data_root: PathBuf,
}

#[derive(Debug, Default)]
struct ClaudeUsageSummary {
    files_scanned: u64,
    records_read: u64,
    usage_records: u64,
    invalid_records: u64,
    unreadable_files: u64,
    files_skipped: u64,
    records_skipped: u64,
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_input_tokens: u64,
    cache_read_input_tokens: u64,
    first_timestamp: Option<String>,
    last_timestamp: Option<String>,
    models: HashSet<String>,
    sessions: HashSet<String>,
}

#[derive(Debug, Default)]
struct CodexUsageSummary {
    threads_read: u64,
    usage_threads: u64,
    invalid_records: u64,
    threads_skipped: u64,
    total_tokens: u64,
    first_updated_at_ms: Option<i64>,
    last_updated_at_ms: Option<i64>,
    models: HashSet<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeJsonlRecord {
    timestamp: Option<String>,
    session_id: Option<String>,
    message: Option<ClaudeMessage>,
}

#[derive(Debug, Deserialize)]
struct ClaudeMessage {
    model: Option<String>,
    usage: Option<ClaudeUsage>,
}

#[derive(Debug, Deserialize)]
struct ClaudeUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
}

impl ClaudeLocalProvider {
    pub fn new(data_root: impl Into<PathBuf>) -> Self {
        Self {
            data_root: data_root.into(),
        }
    }

    pub fn from_default_root() -> Option<Self> {
        env::var_os("HOME").map(|home| Self::new(PathBuf::from(home).join(".claude")))
    }

    pub fn data_root(&self) -> &Path {
        &self.data_root
    }

    pub fn refresh_snapshot(&self, now: &str) -> UsageSnapshot {
        let provider_id = UsageProviderId::ClaudeLocal;

        match self.scan_usage_summary() {
            Ok(summary) if summary.usage_records > 0 => UsageSnapshot {
                service: Service::Claude,
                remaining_percent: None,
                used_percent: None,
                reset_at: None,
                source: UsageSource::Local,
                confidence: UsageConfidence::Low,
                last_updated: now.to_string(),
                details: serde_json::json!({
                    "status": "parsed",
                    "providerId": provider_id.code(),
                    "source": UsageSource::Local.code(),
                    "filesScanned": summary.files_scanned,
                    "recordsRead": summary.records_read,
                    "usageRecords": summary.usage_records,
                    "invalidRecords": summary.invalid_records,
                    "unreadableFiles": summary.unreadable_files,
                    "filesSkipped": summary.files_skipped,
                    "recordsSkipped": summary.records_skipped,
                    "fileLimit": MAX_CLAUDE_JSONL_FILES,
                    "recordLimit": MAX_CLAUDE_RECORDS_PER_REFRESH,
                    "windowPolicy": LOCAL_WINDOW_POLICY,
                    "timestampSemantics": CLAUDE_TIMESTAMP_SEMANTICS,
                    "inputTokens": summary.input_tokens,
                    "outputTokens": summary.output_tokens,
                    "cacheCreationInputTokens": summary.cache_creation_input_tokens,
                    "cacheReadInputTokens": summary.cache_read_input_tokens,
                    "firstTimestamp": summary.first_timestamp,
                    "lastTimestamp": summary.last_timestamp,
                    "modelCount": summary.models.len(),
                    "sessionCount": summary.sessions.len(),
                    "remainingPercentReason": "uncalibrated_local_activity",
                }),
            },
            Ok(summary) => {
                let status = if summary.records_read > 0 && summary.invalid_records > 0 {
                    "parse_failed"
                } else {
                    "missing_data"
                };

                unknown_snapshot(
                    now,
                    status,
                    serde_json::json!({
                    "filesScanned": summary.files_scanned,
                    "recordsRead": summary.records_read,
                    "usageRecords": summary.usage_records,
                    "invalidRecords": summary.invalid_records,
                    "unreadableFiles": summary.unreadable_files,
                    "filesSkipped": summary.files_skipped,
                    "recordsSkipped": summary.records_skipped,
                    "fileLimit": MAX_CLAUDE_JSONL_FILES,
                    "recordLimit": MAX_CLAUDE_RECORDS_PER_REFRESH,
                    "windowPolicy": LOCAL_WINDOW_POLICY,
                    "timestampSemantics": CLAUDE_TIMESTAMP_SEMANTICS,
                    }),
                )
            }
            Err(error) => unknown_snapshot(
                now,
                claude_error_status(&error),
                serde_json::json!({
                    "reason": error,
                    "filesScanned": 0,
                    "recordsRead": 0,
                    "usageRecords": 0,
                    "fileLimit": MAX_CLAUDE_JSONL_FILES,
                    "recordLimit": MAX_CLAUDE_RECORDS_PER_REFRESH,
                    "windowPolicy": LOCAL_WINDOW_POLICY,
                    "timestampSemantics": CLAUDE_TIMESTAMP_SEMANTICS,
                }),
            ),
        }
    }

    fn scan_usage_summary(&self) -> Result<ClaudeUsageSummary, String> {
        let projects_dir = self.data_root.join(CLAUDE_PROJECTS_DIR);

        if !projects_dir.is_dir() {
            return Err("claude_projects_missing".to_string());
        }

        let mut summary = ClaudeUsageSummary::default();
        let mut files = Vec::new();
        collect_jsonl_files(&projects_dir, &mut files)?;

        if files.len() > MAX_CLAUDE_JSONL_FILES {
            summary.files_skipped = (files.len() - MAX_CLAUDE_JSONL_FILES) as u64;
            files.truncate(MAX_CLAUDE_JSONL_FILES);
        }

        let mut record_limit_reached = false;

        for file_path in files {
            if record_limit_reached {
                summary.files_skipped += 1;
                continue;
            }

            summary.files_scanned += 1;

            let Ok(file) = File::open(&file_path) else {
                summary.unreadable_files += 1;
                continue;
            };

            for line in BufReader::new(file).lines() {
                let Ok(line) = line else {
                    summary.invalid_records += 1;
                    continue;
                };
                let line = line.trim();

                if line.is_empty() {
                    continue;
                }

                if summary.records_read >= MAX_CLAUDE_RECORDS_PER_REFRESH {
                    summary.records_skipped += 1;
                    record_limit_reached = true;
                    break;
                }

                summary.records_read += 1;
                match serde_json::from_str::<ClaudeJsonlRecord>(line) {
                    Ok(record) => summary.record(record),
                    Err(_) => summary.invalid_records += 1,
                }
            }
        }

        Ok(summary)
    }
}

impl CodexLocalProvider {
    pub fn new(data_root: impl Into<PathBuf>) -> Self {
        Self {
            data_root: data_root.into(),
        }
    }

    pub fn from_default_root() -> Option<Self> {
        env::var_os("HOME").map(|home| Self::new(PathBuf::from(home).join(".codex")))
    }

    pub fn data_root(&self) -> &Path {
        &self.data_root
    }

    pub fn refresh_snapshot(&self, now: &str) -> UsageSnapshot {
        match self.scan_usage_summary() {
            Ok(summary) if summary.usage_threads > 0 => UsageSnapshot {
                service: Service::Codex,
                remaining_percent: None,
                used_percent: None,
                reset_at: None,
                source: UsageSource::Local,
                confidence: UsageConfidence::Low,
                last_updated: now.to_string(),
                details: serde_json::json!({
                    "status": "parsed",
                    "providerId": UsageProviderId::CodexLocal.code(),
                    "source": UsageSource::Local.code(),
                    "threadsRead": summary.threads_read,
                    "usageThreads": summary.usage_threads,
                    "invalidRecords": summary.invalid_records,
                    "threadsSkipped": summary.threads_skipped,
                    "threadLimit": MAX_CODEX_THREADS_PER_REFRESH,
                    "windowPolicy": LOCAL_WINDOW_POLICY,
                    "timestampSemantics": CODEX_TIMESTAMP_SEMANTICS,
                    "totalTokens": summary.total_tokens,
                    "firstUpdatedAtMs": summary.first_updated_at_ms,
                    "lastUpdatedAtMs": summary.last_updated_at_ms,
                    "modelCount": summary.models.len(),
                    "remainingPercentReason": "uncalibrated_local_activity",
                }),
            },
            Ok(summary) => {
                let status = if summary.threads_read > 0 && summary.invalid_records > 0 {
                    "parse_failed"
                } else {
                    "missing_data"
                };

                unknown_codex_snapshot(
                    now,
                    status,
                    serde_json::json!({
                    "threadsRead": summary.threads_read,
                    "usageThreads": summary.usage_threads,
                    "invalidRecords": summary.invalid_records,
                    "threadsSkipped": summary.threads_skipped,
                    "threadLimit": MAX_CODEX_THREADS_PER_REFRESH,
                    "windowPolicy": LOCAL_WINDOW_POLICY,
                    "timestampSemantics": CODEX_TIMESTAMP_SEMANTICS,
                    "totalTokens": summary.total_tokens,
                    }),
                )
            }
            Err(error) => unknown_codex_snapshot(
                now,
                codex_error_status(&error),
                serde_json::json!({
                    "reason": error,
                    "threadsRead": 0,
                    "usageThreads": 0,
                    "invalidRecords": 0,
                    "threadsSkipped": 0,
                    "threadLimit": MAX_CODEX_THREADS_PER_REFRESH,
                    "windowPolicy": LOCAL_WINDOW_POLICY,
                    "timestampSemantics": CODEX_TIMESTAMP_SEMANTICS,
                    "totalTokens": 0,
                }),
            ),
        }
    }

    fn scan_usage_summary(&self) -> Result<CodexUsageSummary, String> {
        let db_path = self.data_root.join(CODEX_STATE_DB_FILE);

        if !db_path.is_file() {
            return Err("codex_state_db_missing".to_string());
        }

        let connection = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|_| "codex_state_db_unreadable".to_string())?;
        let total_threads = connection
            .query_row("SELECT COUNT(*) FROM threads", [], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(|_| "codex_threads_query_failed".to_string())?;
        let total_threads = u64::try_from(total_threads).unwrap_or_default();
        let mut summary = CodexUsageSummary {
            threads_skipped: total_threads.saturating_sub(MAX_CODEX_THREADS_PER_REFRESH),
            ..CodexUsageSummary::default()
        };
        let mut statement = connection
            .prepare(
                "SELECT tokens_used, updated_at_ms, updated_at, model
                 FROM threads
                 ORDER BY COALESCE(updated_at_ms, updated_at * 1000, 0) DESC
                 LIMIT ?1",
            )
            .map_err(|_| "codex_threads_query_failed".to_string())?;
        let rows = statement
            .query_map([MAX_CODEX_THREADS_PER_REFRESH as i64], |row| {
                let tokens_used = optional_i64_column(row, 0);
                let updated_at_ms = optional_i64_column(row, 1);
                let updated_at = optional_i64_column(row, 2);
                let model = optional_string_column(row, 3);

                Ok(CodexThreadRecord {
                    tokens_used,
                    updated_at_ms: updated_at_ms.or_else(|| updated_at.map(|value| value * 1000)),
                    model,
                })
            })
            .map_err(|_| "codex_threads_query_failed".to_string())?;

        for row in rows {
            let record = row.map_err(|_| "codex_threads_query_failed".to_string())?;
            summary.record(record);
        }

        Ok(summary)
    }
}

#[derive(Debug)]
struct CodexThreadRecord {
    tokens_used: Option<i64>,
    updated_at_ms: Option<i64>,
    model: Option<String>,
}

impl ClaudeUsageSummary {
    fn record(&mut self, record: ClaudeJsonlRecord) {
        let Some(message) = record.message else {
            return;
        };
        let Some(usage) = message.usage else {
            return;
        };

        self.usage_records += 1;
        self.input_tokens += usage.input_tokens.unwrap_or_default();
        self.output_tokens += usage.output_tokens.unwrap_or_default();
        self.cache_creation_input_tokens += usage.cache_creation_input_tokens.unwrap_or_default();
        self.cache_read_input_tokens += usage.cache_read_input_tokens.unwrap_or_default();

        if let Some(timestamp) = record.timestamp {
            match &self.first_timestamp {
                Some(current) if current <= &timestamp => {}
                _ => self.first_timestamp = Some(timestamp.clone()),
            }

            match &self.last_timestamp {
                Some(current) if current >= &timestamp => {}
                _ => self.last_timestamp = Some(timestamp),
            }
        }

        if let Some(model) = message.model {
            self.models.insert(model);
        }

        if let Some(session_id) = record.session_id {
            self.sessions.insert(session_id);
        }
    }
}

impl CodexUsageSummary {
    fn record(&mut self, record: CodexThreadRecord) {
        self.threads_read += 1;

        let Some(tokens_used) = record
            .tokens_used
            .and_then(|value| u64::try_from(value).ok())
        else {
            self.invalid_records += 1;
            return;
        };

        self.usage_threads += 1;
        self.total_tokens = self.total_tokens.saturating_add(tokens_used);

        if let Some(updated_at_ms) = record.updated_at_ms {
            match self.first_updated_at_ms {
                Some(current) if current <= updated_at_ms => {}
                _ => self.first_updated_at_ms = Some(updated_at_ms),
            }

            match self.last_updated_at_ms {
                Some(current) if current >= updated_at_ms => {}
                _ => self.last_updated_at_ms = Some(updated_at_ms),
            }
        }

        if let Some(model) = record.model {
            self.models.insert(model);
        }
    }
}

fn unknown_snapshot(now: &str, status: &str, extra_details: serde_json::Value) -> UsageSnapshot {
    let mut details = serde_json::json!({
        "status": status,
        "providerId": UsageProviderId::ClaudeLocal.code(),
        "source": UsageSource::Local.code(),
    });

    merge_json_objects(&mut details, extra_details);

    UsageSnapshot {
        service: Service::Claude,
        remaining_percent: None,
        used_percent: None,
        reset_at: None,
        source: UsageSource::Local,
        confidence: UsageConfidence::Unknown,
        last_updated: now.to_string(),
        details,
    }
}

fn unknown_codex_snapshot(
    now: &str,
    status: &str,
    extra_details: serde_json::Value,
) -> UsageSnapshot {
    let mut details = serde_json::json!({
        "status": status,
        "providerId": UsageProviderId::CodexLocal.code(),
        "source": UsageSource::Local.code(),
    });

    merge_json_objects(&mut details, extra_details);

    UsageSnapshot {
        service: Service::Codex,
        remaining_percent: None,
        used_percent: None,
        reset_at: None,
        source: UsageSource::Local,
        confidence: UsageConfidence::Unknown,
        last_updated: now.to_string(),
        details,
    }
}

fn merge_json_objects(target: &mut serde_json::Value, source: serde_json::Value) {
    if let (Some(target), Some(source)) = (target.as_object_mut(), source.as_object()) {
        for (key, value) in source {
            target.insert(key.clone(), value.clone());
        }
    }
}

fn collect_jsonl_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(root).map_err(|_| "claude_projects_unreadable".to_string())?;

    for entry in entries {
        let entry = entry.map_err(|_| "claude_project_entry_unreadable".to_string())?;
        let path = entry.path();
        let metadata = entry
            .metadata()
            .map_err(|_| "claude_project_metadata_unreadable".to_string())?;

        if metadata.is_dir() {
            collect_jsonl_files(&path, files)?;
        } else if metadata.is_file()
            && path.extension().and_then(|extension| extension.to_str()) == Some(JSONL_EXTENSION)
        {
            files.push(path);
        }
    }

    files.sort();
    Ok(())
}

fn optional_i64_column(row: &Row<'_>, column_index: usize) -> Option<i64> {
    match row.get_ref(column_index).ok()? {
        ValueRef::Integer(value) => Some(value),
        ValueRef::Null => None,
        _ => None,
    }
}

fn optional_string_column(row: &Row<'_>, column_index: usize) -> Option<String> {
    match row.get_ref(column_index).ok()? {
        ValueRef::Text(value) => std::str::from_utf8(value).ok().map(str::to_string),
        ValueRef::Null => None,
        _ => None,
    }
}

fn claude_error_status(error: &str) -> &'static str {
    match error {
        "claude_projects_missing" => "missing_data",
        "claude_projects_unreadable"
        | "claude_project_entry_unreadable"
        | "claude_project_metadata_unreadable" => "unavailable",
        _ => "parse_failed",
    }
}

fn codex_error_status(error: &str) -> &'static str {
    match error {
        "codex_state_db_missing" => "missing_data",
        "codex_state_db_unreadable" => "unavailable",
        _ => "parse_failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    static TEST_ID: AtomicU64 = AtomicU64::new(1);

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new() -> Self {
            let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
            let path = env::temp_dir().join(format!(
                "forgegauge-claude-local-test-{}-{id}",
                std::process::id()
            ));

            fs::create_dir_all(&path).expect("test directory is created");
            Self { path }
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/claude-local")
    }

    fn create_codex_state_db(root: &Path, rows: &[(i64, i64, Option<&str>)]) {
        fs::create_dir_all(root).expect("codex root is created");
        let connection =
            Connection::open(root.join(CODEX_STATE_DB_FILE)).expect("state db is created");
        connection
            .execute(
                "CREATE TABLE threads (
                    tokens_used INTEGER NOT NULL DEFAULT 0,
                    updated_at INTEGER NOT NULL,
                    updated_at_ms INTEGER,
                    model TEXT,
                    title TEXT,
                    cwd TEXT,
                    preview TEXT
                )",
                [],
            )
            .expect("threads table is created");

        for (tokens_used, updated_at_ms, model) in rows {
            connection
                .execute(
                    "INSERT INTO threads (tokens_used, updated_at, updated_at_ms, model, title, cwd, preview)
                     VALUES (?1, ?2, ?3, ?4, 'redacted title', '/redacted/path', 'redacted preview')",
                    (
                        tokens_used,
                        updated_at_ms / 1000,
                        updated_at_ms,
                        model,
                    ),
                )
                .expect("thread row is inserted");
        }
    }

    fn claude_usage_record(input_tokens: u64) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"2026-06-03T10:00:00Z","sessionId":"fixture-session","message":{{"role":"assistant","model":"claude-fixture","usage":{{"input_tokens":{input_tokens},"output_tokens":5}}}}}}"#
        )
    }

    #[test]
    fn claude_local_provider_parses_synthetic_usage_fixture() {
        let provider = ClaudeLocalProvider::new(fixture_root());

        let snapshot = provider.refresh_snapshot("2026-06-03T22:00:00Z");

        assert_eq!(snapshot.service, Service::Claude);
        assert_eq!(snapshot.source, UsageSource::Local);
        assert_eq!(snapshot.confidence, UsageConfidence::Low);
        assert_eq!(snapshot.remaining_percent, None);
        assert_eq!(snapshot.used_percent, None);
        assert_eq!(snapshot.details["status"], "parsed");
        assert_eq!(snapshot.details["providerId"], "claude.local");
        assert_eq!(snapshot.details["filesScanned"], 1);
        assert_eq!(snapshot.details["recordsRead"], 4);
        assert_eq!(snapshot.details["usageRecords"], 2);
        assert_eq!(snapshot.details["invalidRecords"], 1);
        assert_eq!(snapshot.details["inputTokens"], 320);
        assert_eq!(snapshot.details["outputTokens"], 70);
        assert_eq!(snapshot.details["cacheCreationInputTokens"], 10);
        assert_eq!(snapshot.details["cacheReadInputTokens"], 20);
        assert_eq!(snapshot.details["modelCount"], 1);
        assert_eq!(snapshot.details["sessionCount"], 1);
        assert_eq!(
            snapshot.details["remainingPercentReason"],
            "uncalibrated_local_activity"
        );
        assert!(snapshot.details.get("content").is_none());
        assert!(snapshot.details.get("sessionId").is_none());
        assert!(snapshot.details.get("cwd").is_none());
    }

    #[test]
    fn claude_local_provider_reports_missing_projects_directory_as_unknown() {
        let dir = TestDir::new();
        let provider = ClaudeLocalProvider::new(&dir.path);

        let snapshot = provider.refresh_snapshot("2026-06-03T22:00:00Z");

        assert_eq!(snapshot.source, UsageSource::Local);
        assert_eq!(snapshot.confidence, UsageConfidence::Unknown);
        assert_eq!(snapshot.remaining_percent, None);
        assert_eq!(snapshot.details["status"], "missing_data");
        assert_eq!(snapshot.details["reason"], "claude_projects_missing");
        assert_eq!(snapshot.details["usageRecords"], 0);
    }

    #[test]
    fn claude_local_provider_limits_project_jsonl_file_scans() {
        let dir = TestDir::new();
        let projects_dir = dir.path.join(CLAUDE_PROJECTS_DIR).join("project-a");
        fs::create_dir_all(&projects_dir).expect("projects directory is created");

        for index in 0..=MAX_CLAUDE_JSONL_FILES {
            fs::write(projects_dir.join(format!("{index:03}.jsonl")), "")
                .expect("empty fixture file is written");
        }

        let provider = ClaudeLocalProvider::new(&dir.path);
        let snapshot = provider.refresh_snapshot("2026-06-03T22:00:00Z");

        assert_eq!(snapshot.confidence, UsageConfidence::Unknown);
        assert_eq!(snapshot.details["filesScanned"], MAX_CLAUDE_JSONL_FILES);
        assert_eq!(snapshot.details["filesSkipped"], 1);
        assert_eq!(snapshot.details["fileLimit"], MAX_CLAUDE_JSONL_FILES);
        assert_eq!(snapshot.details["usageRecords"], 0);
    }

    #[test]
    fn claude_local_provider_ignores_rotated_files_and_counts_truncated_lines() {
        let dir = TestDir::new();
        let projects_dir = dir.path.join(CLAUDE_PROJECTS_DIR).join("project-a");
        fs::create_dir_all(&projects_dir).expect("projects directory is created");
        fs::write(
            projects_dir.join("current.jsonl"),
            format!("{}\n{{\"type\":\"assistant\"", claude_usage_record(12)),
        )
        .expect("current fixture file is written");
        fs::write(
            projects_dir.join("current.jsonl.1"),
            claude_usage_record(900),
        )
        .expect("rotated fixture file is written");
        let provider = ClaudeLocalProvider::new(&dir.path);

        let snapshot = provider.refresh_snapshot("2026-06-03T22:00:00Z");

        assert_eq!(snapshot.confidence, UsageConfidence::Low);
        assert_eq!(snapshot.details["filesScanned"], 1);
        assert_eq!(snapshot.details["recordsRead"], 2);
        assert_eq!(snapshot.details["usageRecords"], 1);
        assert_eq!(snapshot.details["invalidRecords"], 1);
        assert_eq!(snapshot.details["inputTokens"], 12);
        assert_eq!(snapshot.details["windowPolicy"], LOCAL_WINDOW_POLICY);
        assert_eq!(
            snapshot.details["timestampSemantics"],
            CLAUDE_TIMESTAMP_SEMANTICS
        );
    }

    #[test]
    fn codex_local_provider_parses_synthetic_state_database() {
        let dir = TestDir::new();
        create_codex_state_db(
            &dir.path,
            &[
                (1200, 1_780_000_000_000, Some("codex-fixture")),
                (800, 1_780_000_010_000, Some("codex-fixture")),
            ],
        );
        let provider = CodexLocalProvider::new(&dir.path);

        let snapshot = provider.refresh_snapshot("2026-06-03T22:00:00Z");

        assert_eq!(snapshot.service, Service::Codex);
        assert_eq!(snapshot.source, UsageSource::Local);
        assert_eq!(snapshot.confidence, UsageConfidence::Low);
        assert_eq!(snapshot.remaining_percent, None);
        assert_eq!(snapshot.used_percent, None);
        assert_eq!(snapshot.details["status"], "parsed");
        assert_eq!(snapshot.details["providerId"], "codex.local");
        assert_eq!(snapshot.details["threadsRead"], 2);
        assert_eq!(snapshot.details["usageThreads"], 2);
        assert_eq!(snapshot.details["invalidRecords"], 0);
        assert_eq!(snapshot.details["threadsSkipped"], 0);
        assert_eq!(snapshot.details["totalTokens"], 2000);
        assert_eq!(snapshot.details["modelCount"], 1);
        assert_eq!(snapshot.details["windowPolicy"], LOCAL_WINDOW_POLICY);
        assert_eq!(
            snapshot.details["timestampSemantics"],
            CODEX_TIMESTAMP_SEMANTICS
        );
        assert_eq!(
            snapshot.details["remainingPercentReason"],
            "uncalibrated_local_activity"
        );
        assert!(snapshot.details.get("title").is_none());
        assert!(snapshot.details.get("cwd").is_none());
        assert!(snapshot.details.get("preview").is_none());
    }

    #[test]
    fn codex_local_provider_reports_missing_state_database_as_unknown() {
        let dir = TestDir::new();
        let provider = CodexLocalProvider::new(&dir.path);

        let snapshot = provider.refresh_snapshot("2026-06-03T22:00:00Z");

        assert_eq!(snapshot.service, Service::Codex);
        assert_eq!(snapshot.source, UsageSource::Local);
        assert_eq!(snapshot.confidence, UsageConfidence::Unknown);
        assert_eq!(snapshot.remaining_percent, None);
        assert_eq!(snapshot.details["status"], "missing_data");
        assert_eq!(snapshot.details["reason"], "codex_state_db_missing");
        assert_eq!(snapshot.details["threadsRead"], 0);
        assert_eq!(snapshot.details["usageThreads"], 0);
        assert_eq!(snapshot.details["invalidRecords"], 0);
        assert_eq!(snapshot.details["totalTokens"], 0);
    }

    #[test]
    fn codex_local_provider_counts_malformed_thread_rows() {
        let dir = TestDir::new();
        fs::create_dir_all(&dir.path).expect("codex root is created");
        let connection =
            Connection::open(dir.path.join(CODEX_STATE_DB_FILE)).expect("state db is created");
        connection
            .execute(
                "CREATE TABLE threads (
                    tokens_used INTEGER,
                    updated_at INTEGER,
                    updated_at_ms INTEGER,
                    model TEXT
                )",
                [],
            )
            .expect("threads table is created");
        connection
            .execute(
                "INSERT INTO threads (tokens_used, updated_at, updated_at_ms, model)
                 VALUES (500, 1780000000, 1780000000000, 'codex-fixture')",
                [],
            )
            .expect("valid thread row is inserted");
        connection
            .execute(
                "INSERT INTO threads (tokens_used, updated_at, updated_at_ms, model)
                 VALUES ('not tokens', 1780000010, 1780000010000, 'codex-fixture')",
                [],
            )
            .expect("malformed thread row is inserted");
        let provider = CodexLocalProvider::new(&dir.path);

        let snapshot = provider.refresh_snapshot("2026-06-03T22:00:00Z");

        assert_eq!(snapshot.confidence, UsageConfidence::Low);
        assert_eq!(snapshot.details["status"], "parsed");
        assert_eq!(snapshot.details["threadsRead"], 2);
        assert_eq!(snapshot.details["usageThreads"], 1);
        assert_eq!(snapshot.details["invalidRecords"], 1);
        assert_eq!(snapshot.details["totalTokens"], 500);
    }

    #[test]
    fn codex_local_provider_reports_corrupt_state_database_as_parse_failed() {
        let dir = TestDir::new();
        fs::write(dir.path.join(CODEX_STATE_DB_FILE), "not a sqlite database")
            .expect("corrupt state db fixture is written");
        let provider = CodexLocalProvider::new(&dir.path);

        let snapshot = provider.refresh_snapshot("2026-06-03T22:00:00Z");

        assert_eq!(snapshot.confidence, UsageConfidence::Unknown);
        assert_eq!(snapshot.details["status"], "parse_failed");
        assert_eq!(snapshot.details["reason"], "codex_threads_query_failed");
        assert_eq!(snapshot.details["threadsRead"], 0);
        assert_eq!(snapshot.details["usageThreads"], 0);
        assert_eq!(snapshot.details["invalidRecords"], 0);
    }

    #[test]
    fn codex_local_provider_limits_thread_scans() {
        let dir = TestDir::new();
        fs::create_dir_all(&dir.path).expect("codex root is created");
        let connection =
            Connection::open(dir.path.join(CODEX_STATE_DB_FILE)).expect("state db is created");
        connection
            .execute(
                "CREATE TABLE threads (
                    tokens_used INTEGER NOT NULL DEFAULT 0,
                    updated_at INTEGER NOT NULL,
                    updated_at_ms INTEGER,
                    model TEXT
                )",
                [],
            )
            .expect("threads table is created");
        let transaction = connection
            .unchecked_transaction()
            .expect("transaction starts");

        for index in 0..=MAX_CODEX_THREADS_PER_REFRESH {
            transaction
                .execute(
                    "INSERT INTO threads (tokens_used, updated_at, updated_at_ms, model)
                     VALUES (1, ?1, ?2, 'codex-fixture')",
                    (index as i64, index as i64 * 1000),
                )
                .expect("thread row is inserted");
        }

        transaction.commit().expect("transaction commits");
        let provider = CodexLocalProvider::new(&dir.path);
        let snapshot = provider.refresh_snapshot("2026-06-03T22:00:00Z");

        assert_eq!(snapshot.confidence, UsageConfidence::Low);
        assert_eq!(
            snapshot.details["threadsRead"],
            MAX_CODEX_THREADS_PER_REFRESH
        );
        assert_eq!(snapshot.details["threadsSkipped"], 1);
        assert_eq!(
            snapshot.details["threadLimit"],
            MAX_CODEX_THREADS_PER_REFRESH
        );
    }
}
