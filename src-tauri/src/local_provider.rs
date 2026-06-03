use crate::usage::{Service, UsageConfidence, UsageProviderId, UsageSnapshot, UsageSource};
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

#[derive(Clone, Debug)]
pub struct ClaudeLocalProvider {
    data_root: PathBuf,
}

#[derive(Debug, Default)]
struct ClaudeUsageSummary {
    files_scanned: u64,
    records_read: u64,
    usage_records: u64,
    invalid_records: u64,
    unreadable_files: u64,
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_input_tokens: u64,
    cache_read_input_tokens: u64,
    first_timestamp: Option<String>,
    last_timestamp: Option<String>,
    models: HashSet<String>,
    sessions: HashSet<String>,
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
            Ok(summary) => unknown_snapshot(
                now,
                "missing_data",
                serde_json::json!({
                    "filesScanned": summary.files_scanned,
                    "recordsRead": summary.records_read,
                    "usageRecords": summary.usage_records,
                    "invalidRecords": summary.invalid_records,
                    "unreadableFiles": summary.unreadable_files,
                }),
            ),
            Err(error) => unknown_snapshot(
                now,
                "missing_data",
                serde_json::json!({
                    "reason": error,
                    "filesScanned": 0,
                    "recordsRead": 0,
                    "usageRecords": 0,
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

        for file_path in files {
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

fn merge_json_objects(target: &mut serde_json::Value, source: serde_json::Value) {
    if let (Some(target), Some(source)) = (target.as_object_mut(), source.as_object()) {
        for (key, value) in source {
            target.insert(key.clone(), value.clone());
        }
    }
}

fn collect_jsonl_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(root)
        .map_err(|error| format!("Could not inspect Claude data root: {error}"))?;

    for entry in entries {
        let entry =
            entry.map_err(|error| format!("Could not inspect Claude data entry: {error}"))?;
        let path = entry.path();
        let metadata = entry
            .metadata()
            .map_err(|error| format!("Could not inspect Claude data metadata: {error}"))?;

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
}
