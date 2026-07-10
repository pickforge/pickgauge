use crate::usage::UsageSnapshot;
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
const SNAPSHOT_VERSION: u32 = 1;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredSnapshots {
    version: u32,
    updated_at: String,
    snapshots: HashMap<String, UsageSnapshot>,
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

    Ok(stored.snapshots)
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

    let stored = StoredSnapshots {
        version: SNAPSHOT_VERSION,
        updated_at: updated_at.to_string(),
        snapshots: snapshots.clone(),
    };
    let raw = serde_json::to_string_pretty(&stored)
        .map_err(|error| format!("Could not serialize usage snapshots: {error}"))?;
    let temp_path = temp_snapshot_path(path);

    write_atomic_with_temp_path(path, parent, &temp_path, raw.as_bytes())
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

    #[test]
    fn snapshots_round_trip_through_store() {
        let dir = TestDir::new();
        let snapshots = HashMap::from([("codex.cli".to_string(), snapshot())]);

        save_in(&dir.path, &snapshots, "2026-07-09T12:00:00Z").expect("snapshots save");

        assert_eq!(
            load_in(&dir.path).expect("snapshots load"),
            snapshots
        );
        let raw = fs::read_to_string(dir.snapshot_path()).expect("snapshot file reads");
        assert!(raw.contains("\"version\": 1"));
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
            r#"{"version":2,"updatedAt":"2026-07-09T12:00:00Z","snapshots":{}}"#,
        )
        .expect("unsupported snapshot file is written");
        assert!(load_in(&dir.path)
            .expect("unsupported snapshots are ignored")
            .is_empty());
    }
}
