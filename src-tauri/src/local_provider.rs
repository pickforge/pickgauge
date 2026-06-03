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
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};

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
    calibration: Option<LocalQuotaCalibration>,
}

#[derive(Clone, Debug)]
pub struct CodexLocalProvider {
    data_root: PathBuf,
    calibration: Option<LocalQuotaCalibration>,
}

#[derive(Clone, Debug)]
pub struct LocalQuotaCalibration {
    limit: f64,
    window_hours: u64,
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
    window_usage_records: u64,
    records_outside_window: u64,
    records_without_timestamp: u64,
    window_tokens: u64,
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
    window_usage_threads: u64,
    threads_outside_window: u64,
    threads_without_timestamp: u64,
    window_tokens: u64,
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
            calibration: None,
        }
    }

    pub fn from_default_root() -> Option<Self> {
        env::var_os("HOME").map(|home| Self::new(PathBuf::from(home).join(".claude")))
    }

    pub fn data_root(&self) -> &Path {
        &self.data_root
    }

    pub fn with_calibration(mut self, calibration: Option<LocalQuotaCalibration>) -> Self {
        self.calibration = calibration;
        self
    }

    pub fn calibration(&self) -> Option<LocalQuotaCalibration> {
        self.calibration.clone()
    }

    pub fn refresh_snapshot(&self, now: &str) -> UsageSnapshot {
        let provider_id = UsageProviderId::ClaudeLocal;
        let window = self
            .calibration
            .as_ref()
            .and_then(|calibration| calibration.window(now));

        match self.scan_usage_summary(window) {
            Ok(summary) if summary.usage_records > 0 => {
                let calibration = calibration_snapshot_values(
                    self.calibration.as_ref(),
                    window,
                    summary.window_tokens,
                    summary.window_usage_records + summary.records_outside_window,
                );

                UsageSnapshot {
                    service: Service::Claude,
                    remaining_percent: calibration.remaining_percent,
                    used_percent: calibration.used_percent,
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
                        "windowUsageRecords": summary.window_usage_records,
                        "recordsOutsideWindow": summary.records_outside_window,
                        "recordsWithoutTimestamp": summary.records_without_timestamp,
                        "windowTokens": summary.window_tokens,
                        "firstTimestamp": summary.first_timestamp,
                        "lastTimestamp": summary.last_timestamp,
                        "modelCount": summary.models.len(),
                        "sessionCount": summary.sessions.len(),
                        "calibrationStatus": calibration.status,
                        "quotaWindowHours": calibration.window_hours,
                        "quotaLimit": calibration.limit,
                        "quotaUsageUnit": calibration.usage_unit,
                        "remainingPercentReason": calibration.reason,
                    }),
                }
            }
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
                    "windowUsageRecords": summary.window_usage_records,
                    "recordsOutsideWindow": summary.records_outside_window,
                    "recordsWithoutTimestamp": summary.records_without_timestamp,
                    "windowTokens": summary.window_tokens,
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
                    "windowUsageRecords": 0,
                    "recordsOutsideWindow": 0,
                    "recordsWithoutTimestamp": 0,
                    "windowTokens": 0,
                }),
            ),
        }
    }

    fn scan_usage_summary(
        &self,
        window: Option<LocalUsageWindow>,
    ) -> Result<ClaudeUsageSummary, String> {
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
                    Ok(record) => summary.record(record, window),
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
            calibration: None,
        }
    }

    pub fn from_default_root() -> Option<Self> {
        env::var_os("HOME").map(|home| Self::new(PathBuf::from(home).join(".codex")))
    }

    pub fn data_root(&self) -> &Path {
        &self.data_root
    }

    pub fn with_calibration(mut self, calibration: Option<LocalQuotaCalibration>) -> Self {
        self.calibration = calibration;
        self
    }

    pub fn calibration(&self) -> Option<LocalQuotaCalibration> {
        self.calibration.clone()
    }

    pub fn refresh_snapshot(&self, now: &str) -> UsageSnapshot {
        let window = self
            .calibration
            .as_ref()
            .and_then(|calibration| calibration.window(now));

        match self.scan_usage_summary(window) {
            Ok(summary) if summary.usage_threads > 0 => {
                let calibration = calibration_snapshot_values(
                    self.calibration.as_ref(),
                    window,
                    summary.window_tokens,
                    summary.window_usage_threads + summary.threads_outside_window,
                );

                UsageSnapshot {
                    service: Service::Codex,
                    remaining_percent: calibration.remaining_percent,
                    used_percent: calibration.used_percent,
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
                        "windowUsageThreads": summary.window_usage_threads,
                        "threadsOutsideWindow": summary.threads_outside_window,
                        "threadsWithoutTimestamp": summary.threads_without_timestamp,
                        "windowTokens": summary.window_tokens,
                        "firstUpdatedAtMs": summary.first_updated_at_ms,
                        "lastUpdatedAtMs": summary.last_updated_at_ms,
                        "modelCount": summary.models.len(),
                        "calibrationStatus": calibration.status,
                        "quotaWindowHours": calibration.window_hours,
                        "quotaLimit": calibration.limit,
                        "quotaUsageUnit": calibration.usage_unit,
                        "remainingPercentReason": calibration.reason,
                    }),
                }
            }
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
                    "windowUsageThreads": summary.window_usage_threads,
                    "threadsOutsideWindow": summary.threads_outside_window,
                    "threadsWithoutTimestamp": summary.threads_without_timestamp,
                    "windowTokens": summary.window_tokens,
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
                    "windowUsageThreads": 0,
                    "threadsOutsideWindow": 0,
                    "threadsWithoutTimestamp": 0,
                    "windowTokens": 0,
                }),
            ),
        }
    }

    fn scan_usage_summary(
        &self,
        window: Option<LocalUsageWindow>,
    ) -> Result<CodexUsageSummary, String> {
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
                    updated_at_ms: updated_at_ms
                        .or_else(|| updated_at.map(|value| value.saturating_mul(1000))),
                    model,
                })
            })
            .map_err(|_| "codex_threads_query_failed".to_string())?;

        for row in rows {
            let record = row.map_err(|_| "codex_threads_query_failed".to_string())?;
            summary.record(record, window);
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

#[derive(Clone, Copy, Debug)]
struct LocalUsageWindow {
    start_ms: i128,
    end_ms: i128,
}

#[derive(Clone, Debug)]
struct CalibrationSnapshotValues {
    remaining_percent: Option<f32>,
    used_percent: Option<f32>,
    status: &'static str,
    reason: &'static str,
    window_hours: Option<u64>,
    limit: Option<f64>,
    usage_unit: Option<&'static str>,
}

impl ClaudeUsageSummary {
    fn record(&mut self, record: ClaudeJsonlRecord, window: Option<LocalUsageWindow>) {
        let Some(message) = record.message else {
            return;
        };
        let Some(usage) = message.usage else {
            return;
        };

        let input_tokens = usage.input_tokens.unwrap_or_default();
        let output_tokens = usage.output_tokens.unwrap_or_default();
        let cache_creation_input_tokens = usage.cache_creation_input_tokens.unwrap_or_default();
        let cache_read_input_tokens = usage.cache_read_input_tokens.unwrap_or_default();
        let record_tokens = input_tokens
            .saturating_add(output_tokens)
            .saturating_add(cache_creation_input_tokens)
            .saturating_add(cache_read_input_tokens);

        self.usage_records += 1;
        self.input_tokens = self.input_tokens.saturating_add(input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(output_tokens);
        self.cache_creation_input_tokens = self
            .cache_creation_input_tokens
            .saturating_add(cache_creation_input_tokens);
        self.cache_read_input_tokens = self
            .cache_read_input_tokens
            .saturating_add(cache_read_input_tokens);

        if let Some(timestamp) = record.timestamp {
            let timestamp_ms = parse_rfc3339_ms(&timestamp);

            if let Some(window) = window {
                match timestamp_ms {
                    Some(timestamp_ms) if window.contains(timestamp_ms) => {
                        self.window_usage_records += 1;
                        self.window_tokens = self.window_tokens.saturating_add(record_tokens);
                    }
                    Some(_) => self.records_outside_window += 1,
                    None => self.records_without_timestamp += 1,
                }
            }

            match &self.first_timestamp {
                Some(current) if current <= &timestamp => {}
                _ => self.first_timestamp = Some(timestamp.clone()),
            }

            match &self.last_timestamp {
                Some(current) if current >= &timestamp => {}
                _ => self.last_timestamp = Some(timestamp),
            }
        } else if window.is_some() {
            self.records_without_timestamp += 1;
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
    fn record(&mut self, record: CodexThreadRecord, window: Option<LocalUsageWindow>) {
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
            if let Some(window) = window {
                if window.contains(i128::from(updated_at_ms)) {
                    self.window_usage_threads += 1;
                    self.window_tokens = self.window_tokens.saturating_add(tokens_used);
                } else {
                    self.threads_outside_window += 1;
                }
            }

            match self.first_updated_at_ms {
                Some(current) if current <= updated_at_ms => {}
                _ => self.first_updated_at_ms = Some(updated_at_ms),
            }

            match self.last_updated_at_ms {
                Some(current) if current >= updated_at_ms => {}
                _ => self.last_updated_at_ms = Some(updated_at_ms),
            }
        } else if window.is_some() {
            self.threads_without_timestamp += 1;
        }

        if let Some(model) = record.model {
            self.models.insert(model);
        }
    }
}

impl LocalQuotaCalibration {
    pub fn new(limit: f64, window_hours: u64) -> Option<Self> {
        if !limit.is_finite() || limit <= 0.0 || window_hours == 0 {
            return None;
        }

        Some(Self {
            limit,
            window_hours: window_hours.clamp(1, 744),
        })
    }

    fn window(&self, now: &str) -> Option<LocalUsageWindow> {
        let end = OffsetDateTime::parse(now, &Rfc3339).ok()?;
        let start = end - Duration::hours(i64::try_from(self.window_hours).ok()?);

        Some(LocalUsageWindow {
            start_ms: unix_timestamp_ms(start),
            end_ms: unix_timestamp_ms(end),
        })
    }
}

impl LocalUsageWindow {
    fn contains(self, timestamp_ms: i128) -> bool {
        timestamp_ms >= self.start_ms && timestamp_ms <= self.end_ms
    }
}

fn calibration_snapshot_values(
    calibration: Option<&LocalQuotaCalibration>,
    window: Option<LocalUsageWindow>,
    window_tokens: u64,
    mapped_records: u64,
) -> CalibrationSnapshotValues {
    let Some(calibration) = calibration else {
        return CalibrationSnapshotValues {
            remaining_percent: None,
            used_percent: None,
            status: "disabled",
            reason: "uncalibrated_local_activity",
            window_hours: None,
            limit: None,
            usage_unit: None,
        };
    };

    if window.is_none() || mapped_records == 0 {
        return CalibrationSnapshotValues {
            remaining_percent: None,
            used_percent: None,
            status: "unmapped_window",
            reason: "calibration_window_unmapped",
            window_hours: Some(calibration.window_hours),
            limit: Some(calibration.limit),
            usage_unit: Some("tokens"),
        };
    }

    let used_percent = ((window_tokens as f64 / calibration.limit) * 100.0).clamp(0.0, 100.0);

    CalibrationSnapshotValues {
        remaining_percent: Some((100.0 - used_percent) as f32),
        used_percent: Some(used_percent as f32),
        status: "active",
        reason: "manual_quota_calibration",
        window_hours: Some(calibration.window_hours),
        limit: Some(calibration.limit),
        usage_unit: Some("tokens"),
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

fn parse_rfc3339_ms(value: &str) -> Option<i128> {
    OffsetDateTime::parse(value, &Rfc3339)
        .ok()
        .map(unix_timestamp_ms)
}

fn unix_timestamp_ms(value: OffsetDateTime) -> i128 {
    value.unix_timestamp_nanos() / 1_000_000
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

    fn quota(limit: f64, window_hours: u64) -> Option<LocalQuotaCalibration> {
        LocalQuotaCalibration::new(limit, window_hours)
    }

    fn ms(value: &str) -> i64 {
        i64::try_from(parse_rfc3339_ms(value).expect("timestamp parses")).expect("timestamp fits")
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
        assert_eq!(snapshot.details["calibrationStatus"], "disabled");
        assert!(snapshot.details.get("content").is_none());
        assert!(snapshot.details.get("sessionId").is_none());
        assert!(snapshot.details.get("cwd").is_none());
    }

    #[test]
    fn claude_local_provider_applies_manual_quota_window() {
        let provider = ClaudeLocalProvider::new(fixture_root()).with_calibration(quota(1000.0, 24));

        let snapshot = provider.refresh_snapshot("2026-06-03T12:00:00Z");

        assert_eq!(snapshot.confidence, UsageConfidence::Low);
        assert_eq!(snapshot.remaining_percent, Some(58.0));
        assert_eq!(snapshot.used_percent, Some(42.0));
        assert_eq!(snapshot.details["windowTokens"], 420);
        assert_eq!(snapshot.details["windowUsageRecords"], 2);
        assert_eq!(snapshot.details["recordsOutsideWindow"], 0);
        assert_eq!(snapshot.details["recordsWithoutTimestamp"], 0);
        assert_eq!(snapshot.details["calibrationStatus"], "active");
        assert_eq!(
            snapshot.details["remainingPercentReason"],
            "manual_quota_calibration"
        );
        assert_eq!(snapshot.details["quotaWindowHours"], 24);
        assert_eq!(snapshot.details["quotaLimit"], 1000.0);
        assert_eq!(snapshot.details["quotaUsageUnit"], "tokens");
    }

    #[test]
    fn claude_local_provider_reports_full_window_when_usage_is_older_than_calibration_window() {
        let provider = ClaudeLocalProvider::new(fixture_root()).with_calibration(quota(1000.0, 5));

        let snapshot = provider.refresh_snapshot("2026-06-04T12:00:00Z");

        assert_eq!(snapshot.remaining_percent, Some(100.0));
        assert_eq!(snapshot.used_percent, Some(0.0));
        assert_eq!(snapshot.details["windowTokens"], 0);
        assert_eq!(snapshot.details["windowUsageRecords"], 0);
        assert_eq!(snapshot.details["recordsOutsideWindow"], 2);
        assert_eq!(snapshot.details["calibrationStatus"], "active");
    }

    #[test]
    fn claude_local_provider_does_not_calibrate_records_without_timestamps() {
        let dir = TestDir::new();
        let projects_dir = dir.path.join(CLAUDE_PROJECTS_DIR).join("project-a");
        fs::create_dir_all(&projects_dir).expect("projects directory is created");
        fs::write(
            projects_dir.join("current.jsonl"),
            r#"{"type":"assistant","sessionId":"fixture-session","message":{"role":"assistant","model":"claude-fixture","usage":{"input_tokens":100,"output_tokens":50}}}"#,
        )
        .expect("fixture file is written");
        let provider = ClaudeLocalProvider::new(&dir.path).with_calibration(quota(1000.0, 5));

        let snapshot = provider.refresh_snapshot("2026-06-03T12:00:00Z");

        assert_eq!(snapshot.remaining_percent, None);
        assert_eq!(snapshot.used_percent, None);
        assert_eq!(snapshot.details["usageRecords"], 1);
        assert_eq!(snapshot.details["recordsWithoutTimestamp"], 1);
        assert_eq!(snapshot.details["windowTokens"], 0);
        assert_eq!(snapshot.details["calibrationStatus"], "unmapped_window");
        assert_eq!(
            snapshot.details["remainingPercentReason"],
            "calibration_window_unmapped"
        );
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
        assert_eq!(snapshot.details["calibrationStatus"], "disabled");
        assert!(snapshot.details.get("title").is_none());
        assert!(snapshot.details.get("cwd").is_none());
        assert!(snapshot.details.get("preview").is_none());
    }

    #[test]
    fn codex_local_provider_applies_manual_quota_window() {
        let dir = TestDir::new();
        create_codex_state_db(
            &dir.path,
            &[
                (1200, ms("2026-06-03T21:00:00Z"), Some("codex-fixture")),
                (800, ms("2026-06-03T10:00:00Z"), Some("codex-fixture")),
            ],
        );
        let provider = CodexLocalProvider::new(&dir.path).with_calibration(quota(2000.0, 5));

        let snapshot = provider.refresh_snapshot("2026-06-03T22:00:00Z");

        assert_eq!(snapshot.confidence, UsageConfidence::Low);
        assert_eq!(snapshot.remaining_percent, Some(40.0));
        assert_eq!(snapshot.used_percent, Some(60.0));
        assert_eq!(snapshot.details["totalTokens"], 2000);
        assert_eq!(snapshot.details["windowTokens"], 1200);
        assert_eq!(snapshot.details["windowUsageThreads"], 1);
        assert_eq!(snapshot.details["threadsOutsideWindow"], 1);
        assert_eq!(snapshot.details["threadsWithoutTimestamp"], 0);
        assert_eq!(snapshot.details["calibrationStatus"], "active");
        assert_eq!(
            snapshot.details["remainingPercentReason"],
            "manual_quota_calibration"
        );
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
