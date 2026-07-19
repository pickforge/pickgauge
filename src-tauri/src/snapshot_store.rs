use crate::{
    usage::{Service, UsageConfidence, UsageSnapshot, UsageSource},
    usage_model::{UsageModel, UsageWindow, UsageWindows},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const BUNDLE_IDENTIFIER: &str = "com.pickforge.pickgauge";
const SNAPSHOT_FILE_NAME: &str = "snapshots.json";
// Bumped from 1 to 2 when the cache moved from persisting each provider's
// unrestricted `details` bag to the sanitized, typed `UsageModel` projection
// (status/plan/windows only). Older files fail the version check below and
// are discarded rather than migrated: this is a self-healing local cache,
// repopulated by the next refresh.
const SNAPSHOT_VERSION: u32 = 2;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredSnapshots {
    version: u32,
    updated_at: String,
    snapshots: HashMap<String, PersistedUsageSnapshot>,
}

/// The sanitized, typed projection of a `UsageSnapshot` written to disk: only
/// the validated windows/status/plan model state (`usage_model`), never the
/// unrestricted provider `details` bag with its merge/backoff diagnostics.
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PersistedUsageSnapshot {
    service: Service,
    source: UsageSource,
    confidence: UsageConfidence,
    last_updated: String,
    remaining_percent: Option<f32>,
    used_percent: Option<f32>,
    reset_at: Option<String>,
    status: String,
    plan: Option<String>,
    windows: PersistedUsageWindows,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PersistedUsageWindows {
    five_hour: Option<PersistedUsageWindow>,
    week: Option<PersistedUsageWindow>,
    fable: Option<PersistedUsageWindow>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PersistedUsageWindow {
    remaining_percent: Option<f32>,
    used_percent: Option<f32>,
    reset_at: Option<String>,
}

impl From<UsageWindow> for PersistedUsageWindow {
    fn from(window: UsageWindow) -> Self {
        Self {
            remaining_percent: window.remaining_percent,
            used_percent: window.used_percent,
            reset_at: window.reset_at,
        }
    }
}

impl From<PersistedUsageWindow> for UsageWindow {
    fn from(window: PersistedUsageWindow) -> Self {
        Self {
            remaining_percent: window.remaining_percent,
            used_percent: window.used_percent,
            reset_at: window.reset_at,
        }
    }
}

impl From<UsageWindows> for PersistedUsageWindows {
    fn from(windows: UsageWindows) -> Self {
        Self {
            five_hour: windows.five_hour.map(Into::into),
            week: windows.week.map(Into::into),
            fable: windows.fable.map(Into::into),
        }
    }
}

impl From<&UsageSnapshot> for PersistedUsageSnapshot {
    fn from(snapshot: &UsageSnapshot) -> Self {
        let model = UsageModel::from_snapshot(snapshot);

        Self {
            service: snapshot.service,
            source: snapshot.source,
            confidence: snapshot.confidence,
            last_updated: snapshot.last_updated.clone(),
            remaining_percent: snapshot.remaining_percent,
            used_percent: snapshot.used_percent,
            reset_at: snapshot.reset_at.clone(),
            status: model.status,
            plan: model.plan,
            windows: model.windows.into(),
        }
    }
}

impl From<PersistedUsageSnapshot> for UsageSnapshot {
    fn from(persisted: PersistedUsageSnapshot) -> Self {
        let mut details = serde_json::json!({ "status": persisted.status });
        if let (Some(plan), Some(object)) = (&persisted.plan, details.as_object_mut()) {
            object.insert("plan".to_string(), serde_json::json!(plan));
        }
        if let Some(windows) = persisted_windows_json(&persisted.windows) {
            if let Some(object) = details.as_object_mut() {
                object.insert("windows".to_string(), windows);
            }
        }

        Self {
            service: persisted.service,
            remaining_percent: persisted.remaining_percent,
            used_percent: persisted.used_percent,
            reset_at: persisted.reset_at,
            source: persisted.source,
            confidence: persisted.confidence,
            last_updated: persisted.last_updated,
            details,
        }
    }
}

fn persisted_windows_json(windows: &PersistedUsageWindows) -> Option<serde_json::Value> {
    if windows.five_hour.is_none() && windows.week.is_none() && windows.fable.is_none() {
        return None;
    }

    Some(serde_json::json!({
        "fiveHour": persisted_window_json(&windows.five_hour),
        "week": persisted_window_json(&windows.week),
        "fable": persisted_window_json(&windows.fable),
    }))
}

fn persisted_window_json(window: &Option<PersistedUsageWindow>) -> serde_json::Value {
    match window {
        Some(window) => serde_json::json!({
            "remainingPercent": window.remaining_percent,
            "usedPercent": window.used_percent,
            "resetAt": window.reset_at,
        }),
        None => serde_json::Value::Null,
    }
}

pub fn app_data_dir() -> Result<PathBuf, String> {
    dirs::data_dir()
        .map(|data_dir| data_dir.join(BUNDLE_IDENTIFIER))
        .ok_or_else(|| "Could not resolve application data directory".to_string())
}

pub fn load_in(app_data_dir: &Path) -> Result<HashMap<String, UsageSnapshot>, String> {
    load_from_path(&app_data_dir.join(SNAPSHOT_FILE_NAME))
}

pub fn save_in(
    app_data_dir: &Path,
    snapshots: &HashMap<String, UsageSnapshot>,
    updated_at: &str,
) -> Result<(), String> {
    save_to_path(
        &app_data_dir.join(SNAPSHOT_FILE_NAME),
        snapshots,
        updated_at,
    )
}

pub fn clear_in(app_data_dir: &Path) -> Result<(), String> {
    let path = app_data_dir.join(SNAPSHOT_FILE_NAME);
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Could not clear usage snapshots: {error}")),
    }
}

fn load_from_path(path: &Path) -> Result<HashMap<String, UsageSnapshot>, String> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(error) => return Err(format!("Could not read usage snapshots: {error}")),
    };

    let stored = match serde_json::from_str::<StoredSnapshots>(&raw) {
        Ok(stored) if stored.version == SNAPSHOT_VERSION => stored,
        Ok(_) | Err(_) => return Ok(HashMap::new()),
    };

    Ok(stored
        .snapshots
        .into_iter()
        .map(|(provider_key, persisted)| (provider_key, persisted.into()))
        .collect())
}

fn save_to_path(
    path: &Path,
    snapshots: &HashMap<String, UsageSnapshot>,
    updated_at: &str,
) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Usage snapshot path must have a parent directory".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create usage snapshot directory: {error}"))?;

    let mut merged_snapshots = snapshots.clone();
    if let Ok(existing_snapshots) = load_from_path(path) {
        for (provider_key, existing) in existing_snapshots {
            let Some(next) = merged_snapshots.get(&provider_key) else {
                continue;
            };

            // A stale parsed gauge remains useful to headless consumers because
            // `lastUpdated` and `staleSeconds` make its age explicit.
            if snapshot_is_parsed(&existing) && !snapshot_is_parsed(next) {
                merged_snapshots.insert(provider_key, existing);
            }
        }
    }

    let stored = StoredSnapshots {
        version: SNAPSHOT_VERSION,
        updated_at: updated_at.to_string(),
        snapshots: merged_snapshots
            .iter()
            .map(|(provider_key, snapshot)| (provider_key.clone(), snapshot.into()))
            .collect(),
    };
    let raw = serde_json::to_string_pretty(&stored)
        .map_err(|error| format!("Could not serialize usage snapshots: {error}"))?;
    let temp_path = temp_snapshot_path(path);

    write_atomic_with_temp_path(path, parent, &temp_path, raw.as_bytes())
}

fn snapshot_is_parsed(snapshot: &UsageSnapshot) -> bool {
    snapshot
        .details
        .get("status")
        .and_then(serde_json::Value::as_str)
        == Some("parsed")
}

fn write_atomic_with_temp_path(
    path: &Path,
    parent: &Path,
    temp_path: &Path,
    bytes: &[u8],
) -> Result<(), String> {
    let write_result = write_temp_file(temp_path, bytes).and_then(|_| {
        fs::rename(temp_path, path)
            .map_err(|error| format!("Could not replace usage snapshots: {error}"))?;
        set_restrictive_file_permissions(path)?;
        sync_parent_dir(parent);
        Ok(())
    });

    if write_result.is_err() {
        let _ = fs::remove_file(temp_path);
    }

    write_result
}

fn write_temp_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("Could not create temporary usage snapshot file: {error}"))?;

    set_restrictive_file_permissions(path)?;
    file.write_all(bytes)
        .map_err(|error| format!("Could not write temporary usage snapshot file: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("Could not sync temporary usage snapshot file: {error}"))?;

    Ok(())
}

fn temp_snapshot_path(path: &Path) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let extension = format!("tmp.{}.{}", std::process::id(), timestamp);

    path.with_extension(extension)
}

#[cfg(unix)]
fn set_restrictive_file_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("Could not set usage snapshot permissions: {error}"))
}

#[cfg(not(unix))]
fn set_restrictive_file_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
fn sync_parent_dir(path: &Path) {
    if let Ok(directory) = File::open(path) {
        let _ = directory.sync_all();
    }
}

#[cfg(not(unix))]
fn sync_parent_dir(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::{Service, UsageConfidence, UsageSource};
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_ID: AtomicU64 = AtomicU64::new(1);

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new() -> Self {
            let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "pickgauge-snapshot-store-test-{}-{id}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("test directory is created");
            Self { path }
        }

        fn snapshot_path(&self) -> PathBuf {
            self.path.join(SNAPSHOT_FILE_NAME)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn snapshot() -> UsageSnapshot {
        UsageSnapshot {
            service: Service::Codex,
            remaining_percent: Some(64.0),
            used_percent: Some(36.0),
            reset_at: Some("2026-07-10T12:00:00Z".to_string()),
            source: UsageSource::Web,
            confidence: UsageConfidence::High,
            last_updated: "2026-07-09T12:00:00Z".to_string(),
            details: serde_json::json!({
                "status": "parsed",
                "providerId": "codex.cli",
                "windows": {
                    "fiveHour": {
                        "remainingPercent": 64.0,
                        "usedPercent": 36.0,
                        "resetAt": "2026-07-10T12:00:00Z"
                    },
                    "week": null
                }
            }),
        }
    }

    fn snapshot_with_status(status: &str, remaining_percent: Option<f32>) -> UsageSnapshot {
        let mut snapshot = snapshot();
        snapshot.remaining_percent = remaining_percent;
        snapshot.used_percent = remaining_percent.map(|remaining| 100.0 - remaining);
        snapshot.details["status"] = serde_json::Value::String(status.to_string());
        snapshot
    }

    #[test]
    fn snapshots_round_trip_through_store() {
        let dir = TestDir::new();
        let snapshots = HashMap::from([("codex.cli".to_string(), snapshot())]);

        save_in(&dir.path, &snapshots, "2026-07-09T12:00:00Z").expect("snapshots save");

        // The store persists the sanitized model projection (status/plan/
        // windows), not the provider's unrestricted `details` bag, so
        // diagnostic-only keys like `providerId` do not round-trip.
        let loaded = load_in(&dir.path).expect("snapshots load");
        let loaded_snapshot = &loaded["codex.cli"];
        assert_eq!(loaded_snapshot.service, Service::Codex);
        assert_eq!(loaded_snapshot.remaining_percent, Some(64.0));
        assert_eq!(loaded_snapshot.used_percent, Some(36.0));
        assert_eq!(loaded_snapshot.details["status"], "parsed");
        assert_eq!(
            loaded_snapshot.details["windows"]["fiveHour"]["remainingPercent"],
            64.0
        );
        assert!(loaded_snapshot.details.get("providerId").is_none());

        let raw = fs::read_to_string(dir.snapshot_path()).expect("snapshot file reads");
        assert!(raw.contains("\"version\": 2"));
        assert!(raw.contains("\"updatedAt\": \"2026-07-09T12:00:00Z\""));
    }

    #[test]
    fn failed_temp_write_preserves_existing_snapshot_file() {
        let dir = TestDir::new();
        let snapshots = HashMap::from([("codex.cli".to_string(), snapshot())]);
        save_in(&dir.path, &snapshots, "2026-07-09T12:00:00Z").expect("snapshots save");
        let before = fs::read_to_string(dir.snapshot_path()).expect("existing file reads");

        let temp_path = dir.path.join("missing").join("snapshots.tmp");
        let result = write_atomic_with_temp_path(
            &dir.snapshot_path(),
            &dir.path,
            &temp_path,
            b"replacement",
        );

        assert!(result.is_err());
        assert_eq!(
            fs::read_to_string(dir.snapshot_path()).expect("existing file remains"),
            before
        );
    }

    #[test]
    fn corrupt_or_unsupported_snapshot_files_are_ignored() {
        let dir = TestDir::new();
        let path = dir.snapshot_path();
        fs::write(&path, "{ not json").expect("corrupt snapshot file is written");
        assert!(load_in(&dir.path)
            .expect("corrupt snapshots are ignored")
            .is_empty());

        fs::write(
            &path,
            r#"{"version":3,"updatedAt":"2026-07-09T12:00:00Z","snapshots":{}}"#,
        )
        .expect("unsupported snapshot file is written");
        assert!(load_in(&dir.path)
            .expect("unsupported snapshots are ignored")
            .is_empty());
    }

    #[test]
    fn parsed_snapshot_survives_placeholder_overwrite() {
        let dir = TestDir::new();
        let key = "codex.web".to_string();
        save_in(
            &dir.path,
            &HashMap::from([(key.clone(), snapshot_with_status("parsed", Some(64.0)))]),
            "2026-07-09T12:00:00Z",
        )
        .expect("parsed snapshot saves");

        save_in(
            &dir.path,
            &HashMap::from([(key.clone(), snapshot_with_status("login_required", None))]),
            "2026-07-09T12:01:00Z",
        )
        .expect("placeholder snapshot saves");

        let saved = load_in(&dir.path).expect("snapshots load");
        assert_eq!(saved[&key].details["status"], "parsed");
        assert_eq!(saved[&key].remaining_percent, Some(64.0));
    }

    #[test]
    fn newer_parsed_snapshot_replaces_previous_parsed_snapshot() {
        let dir = TestDir::new();
        let key = "codex.web".to_string();
        save_in(
            &dir.path,
            &HashMap::from([(key.clone(), snapshot_with_status("parsed", Some(64.0)))]),
            "2026-07-09T12:00:00Z",
        )
        .expect("initial parsed snapshot saves");

        save_in(
            &dir.path,
            &HashMap::from([(key.clone(), snapshot_with_status("parsed", Some(42.0)))]),
            "2026-07-09T12:01:00Z",
        )
        .expect("new parsed snapshot saves");

        assert_eq!(
            load_in(&dir.path).expect("snapshots load")[&key].remaining_percent,
            Some(42.0)
        );
    }

    #[test]
    fn non_parsed_snapshot_persists_when_no_parsed_snapshot_exists() {
        let dir = TestDir::new();
        let key = "codex.web".to_string();
        save_in(
            &dir.path,
            &HashMap::from([(key.clone(), snapshot_with_status("login_required", None))]),
            "2026-07-09T12:00:00Z",
        )
        .expect("placeholder snapshot saves");

        assert_eq!(
            load_in(&dir.path).expect("snapshots load")[&key].details["status"],
            "login_required"
        );
    }

    #[test]
    fn clear_removes_snapshot_file() {
        let dir = TestDir::new();
        save_in(
            &dir.path,
            &HashMap::from([("codex.cli".to_string(), snapshot())]),
            "2026-07-09T12:00:00Z",
        )
        .expect("snapshots save");

        clear_in(&dir.path).expect("snapshots clear");

        assert!(!dir.snapshot_path().exists());
        clear_in(&dir.path).expect("missing snapshots clear");
    }
}
