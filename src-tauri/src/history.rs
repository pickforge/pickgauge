use crate::usage::{Service, UsageSnapshot, UsageSource};
use rusqlite::Connection;
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};
use time::OffsetDateTime;

const HISTORY_DB_FILE: &str = "history.db";
/// Re-record an unchanged reading at most this often, so the trail stays
/// dense enough for charts without writing every 45s refresh tick.
const MIN_UNCHANGED_SAMPLE_SPACING_SECONDS: i64 = 600;
const RETENTION_DAYS: i64 = 400;

pub struct HistoryStore {
    conn: Mutex<Connection>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyGaugeStat {
    pub day: String,
    pub avg_remaining_percent: Option<f64>,
    pub min_remaining_percent: Option<f64>,
    pub last_remaining_percent: Option<f64>,
    pub samples: i64,
}

pub fn now_unix() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

impl HistoryStore {
    pub fn open_in(dir: &Path) -> Result<Self, String> {
        fs::create_dir_all(dir)
            .map_err(|error| format!("Could not create history directory: {error}"))?;
        Self::open(&dir.join(HISTORY_DB_FILE))
    }

    fn open(path: &PathBuf) -> Result<Self, String> {
        let conn = Connection::open(path)
            .map_err(|error| format!("Could not open history database: {error}"))?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS usage_samples (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                recorded_at INTEGER NOT NULL,
                service TEXT NOT NULL,
                remaining_percent REAL,
                used_percent REAL,
                source TEXT NOT NULL,
                confidence TEXT NOT NULL,
                window_tokens INTEGER,
                total_tokens INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_usage_samples_service_time
                ON usage_samples(service, recorded_at DESC);",
        )
        .map_err(|error| format!("Could not prepare history schema: {error}"))?;

        set_restrictive_db_permissions(path);

        let store = Self {
            conn: Mutex::new(conn),
        };
        store.prune(now_unix())?;
        Ok(store)
    }

    pub fn record(&self, snapshots: &[UsageSnapshot], recorded_at: i64) -> Result<(), String> {
        let conn = self.lock()?;

        for snapshot in snapshots {
            if snapshot.source == UsageSource::Fake {
                continue;
            }

            let service = snapshot.service.code();
            let last: Option<(i64, Option<f64>)> = conn
                .query_row(
                    "SELECT recorded_at, remaining_percent FROM usage_samples
                     WHERE service = ?1 ORDER BY recorded_at DESC, id DESC LIMIT 1",
                    [service],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .ok();

            if let Some((last_at, last_remaining)) = last {
                let unchanged = match (last_remaining, snapshot.remaining_percent) {
                    (Some(previous), Some(current)) => (previous - f64::from(current)).abs() < 0.01,
                    (None, None) => true,
                    _ => false,
                };

                if unchanged && recorded_at - last_at < MIN_UNCHANGED_SAMPLE_SPACING_SECONDS {
                    continue;
                }
            }

            conn.execute(
                "INSERT INTO usage_samples
                    (recorded_at, service, remaining_percent, used_percent,
                     source, confidence, window_tokens, total_tokens)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    recorded_at,
                    service,
                    snapshot.remaining_percent.map(f64::from),
                    snapshot.used_percent.map(f64::from),
                    snapshot.source.code(),
                    confidence_code(snapshot),
                    detail_u64(snapshot, "windowTokens"),
                    detail_u64(snapshot, "totalTokens"),
                ],
            )
            .map_err(|error| format!("Could not record usage sample: {error}"))?;
        }

        Ok(())
    }

    pub fn daily_gauge(
        &self,
        service: Service,
        days: u32,
        utc_offset_seconds: i32,
    ) -> Result<Vec<DailyGaugeStat>, String> {
        let conn = self.lock()?;
        let since = now_unix() - i64::from(days.clamp(1, 730)) * 86_400;
        let offset = i64::from(utc_offset_seconds.clamp(-64_800, 64_800));
        let mut statement = conn
            .prepare(
                "SELECT date(recorded_at + ?1, 'unixepoch'), remaining_percent
                 FROM usage_samples
                 WHERE service = ?2 AND recorded_at >= ?3
                 ORDER BY recorded_at ASC, id ASC",
            )
            .map_err(|error| format!("Could not prepare history query: {error}"))?;

        let rows = statement
            .query_map(rusqlite::params![offset, service.code(), since], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<f64>>(1)?))
            })
            .map_err(|error| format!("Could not query usage history: {error}"))?;

        let mut days: Vec<DailyGaugeStat> = Vec::new();
        let mut sums: Vec<(f64, i64)> = Vec::new();

        for row in rows {
            let (day, remaining) =
                row.map_err(|error| format!("Could not read usage history: {error}"))?;

            if days.last().map(|stat| stat.day.as_str()) != Some(day.as_str()) {
                days.push(DailyGaugeStat {
                    day,
                    avg_remaining_percent: None,
                    min_remaining_percent: None,
                    last_remaining_percent: None,
                    samples: 0,
                });
                sums.push((0.0, 0));
            }

            let stat = days.last_mut().expect("day bucket exists");
            let sum = sums.last_mut().expect("day sums exist");
            stat.samples += 1;

            if let Some(remaining) = remaining {
                sum.0 += remaining;
                sum.1 += 1;
                stat.avg_remaining_percent = Some(sum.0 / sum.1 as f64);
                stat.min_remaining_percent = Some(
                    stat.min_remaining_percent
                        .map_or(remaining, |current| current.min(remaining)),
                );
                stat.last_remaining_percent = Some(remaining);
            }
        }

        Ok(days)
    }

    fn prune(&self, now: i64) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute(
            "DELETE FROM usage_samples WHERE recorded_at < ?1",
            [now - RETENTION_DAYS * 86_400],
        )
        .map_err(|error| format!("Could not prune usage history: {error}"))?;
        Ok(())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, String> {
        self.conn
            .lock()
            .map_err(|_| "History store lock was poisoned".to_string())
    }
}

fn confidence_code(snapshot: &UsageSnapshot) -> &'static str {
    match snapshot.confidence {
        crate::usage::UsageConfidence::High => "high",
        crate::usage::UsageConfidence::Medium => "medium",
        crate::usage::UsageConfidence::Low => "low",
        crate::usage::UsageConfidence::Unknown => "unknown",
    }
}

fn detail_u64(snapshot: &UsageSnapshot, key: &str) -> Option<i64> {
    snapshot
        .details
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| i64::try_from(value).ok())
}

#[cfg(unix)]
fn set_restrictive_db_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn set_restrictive_db_permissions(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::UsageConfidence;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_ID: AtomicU64 = AtomicU64::new(1);

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new() -> Self {
            let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "pickgauge-history-test-{}-{id}",
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

    fn snapshot(service: Service, remaining: Option<f32>, source: UsageSource) -> UsageSnapshot {
        UsageSnapshot {
            service,
            remaining_percent: remaining,
            used_percent: remaining.map(|value| 100.0 - value),
            reset_at: None,
            source,
            confidence: UsageConfidence::Low,
            last_updated: "2026-06-09T12:00:00Z".to_string(),
            details: serde_json::json!({ "windowTokens": 1200, "totalTokens": 9000 }),
        }
    }

    #[test]
    fn records_and_aggregates_daily_samples() {
        let dir = TestDir::new();
        let store = HistoryStore::open_in(&dir.path).expect("store opens");
        let base = now_unix() - 86_400;

        store
            .record(
                &[snapshot(Service::Codex, Some(80.0), UsageSource::Local)],
                base,
            )
            .expect("first sample records");
        store
            .record(
                &[snapshot(Service::Codex, Some(60.0), UsageSource::Local)],
                base + 3_600,
            )
            .expect("second sample records");
        store
            .record(
                &[snapshot(Service::Claude, Some(50.0), UsageSource::Local)],
                base + 3_600,
            )
            .expect("other service records");

        let stats = store
            .daily_gauge(Service::Codex, 7, 0)
            .expect("daily stats load");

        assert_eq!(stats.iter().map(|stat| stat.samples).sum::<i64>(), 2);
        let last_day = stats.last().expect("at least one day");
        assert_eq!(last_day.min_remaining_percent, Some(60.0));
        assert_eq!(last_day.last_remaining_percent, Some(60.0));
    }

    #[test]
    fn skips_unchanged_samples_within_spacing_window() {
        let dir = TestDir::new();
        let store = HistoryStore::open_in(&dir.path).expect("store opens");
        let base = now_unix();

        for offset in [0, 60, 120] {
            store
                .record(
                    &[snapshot(Service::Claude, Some(42.0), UsageSource::Local)],
                    base + offset,
                )
                .expect("sample records");
        }

        let stats = store
            .daily_gauge(Service::Claude, 2, 0)
            .expect("daily stats load");

        assert_eq!(stats.iter().map(|stat| stat.samples).sum::<i64>(), 1);
    }

    #[test]
    fn fake_snapshots_are_not_recorded() {
        let dir = TestDir::new();
        let store = HistoryStore::open_in(&dir.path).expect("store opens");

        store
            .record(
                &[snapshot(Service::Codex, Some(72.0), UsageSource::Fake)],
                now_unix(),
            )
            .expect("record call succeeds");

        let stats = store
            .daily_gauge(Service::Codex, 2, 0)
            .expect("daily stats load");

        assert!(stats.is_empty());
    }
}
