use crate::usage::Service;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Child,
    sync::Mutex,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const PROFILE_STOP_TIMEOUT: Duration = Duration::from_secs(3);
pub const SESSION_REGISTRY_FILE_NAME: &str = "managed-browser-sessions.json";
pub const PROCESS_MARKER_ENV: &str = "FORGEGAUGE_BROWSER_PROCESS_MARKER";

const SESSION_REGISTRY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug)]
pub struct BrowserSessionManager {
    processes: Mutex<HashMap<Service, ManagedBrowserProcess>>,
    orphans: Mutex<HashMap<Service, OrphanedBrowserProcess>>,
    registry_path: Option<PathBuf>,
}

#[derive(Debug)]
struct ManagedBrowserProcess {
    process_id: u32,
    process_marker: String,
    started_at: String,
    child: Child,
}

#[derive(Clone, Debug)]
struct OrphanedBrowserProcess {
    process_id: u32,
    process_marker: String,
    started_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserSessionMarker {
    service: Service,
    process_marker: String,
    started_at: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrowserSessionStopStatus {
    NoManagedProcess,
    AlreadyExited,
    Stopped,
    Killed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrowserSessionStopResult {
    pub service: Service,
    pub status: BrowserSessionStopStatus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrowserSessionStartupRecovery {
    pub orphaned_processes: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserSessionRegistry {
    schema_version: u32,
    processes: Vec<BrowserSessionRecord>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserSessionRecord {
    service: Service,
    process_id: u32,
    process_marker: String,
    started_at: String,
}

impl Default for BrowserSessionManager {
    fn default() -> Self {
        Self {
            processes: Mutex::new(HashMap::new()),
            orphans: Mutex::new(HashMap::new()),
            registry_path: None,
        }
    }
}

#[allow(dead_code)]
impl BrowserSessionMarker {
    pub fn new(service: Service) -> Self {
        Self {
            service,
            process_marker: new_process_marker(service),
            started_at: now_unix_millis().to_string(),
        }
    }

    pub fn env_pair(&self) -> (&'static str, &str) {
        (PROCESS_MARKER_ENV, &self.process_marker)
    }
}

impl BrowserSessionRecord {
    fn from_managed(service: Service, process: &ManagedBrowserProcess) -> Self {
        Self {
            service,
            process_id: process.process_id,
            process_marker: process.process_marker.clone(),
            started_at: process.started_at.clone(),
        }
    }

    fn from_orphan(service: Service, process: &OrphanedBrowserProcess) -> Self {
        Self {
            service,
            process_id: process.process_id,
            process_marker: process.process_marker.clone(),
            started_at: process.started_at.clone(),
        }
    }
}

impl BrowserSessionManager {
    pub fn with_registry_path(path: impl Into<PathBuf>) -> Self {
        Self {
            processes: Mutex::new(HashMap::new()),
            orphans: Mutex::new(HashMap::new()),
            registry_path: Some(path.into()),
        }
    }

    #[allow(dead_code)]
    pub fn track_process(
        &self,
        service: Service,
        mut child: Child,
        marker: BrowserSessionMarker,
    ) -> Result<u32, String> {
        if marker.service != service {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Managed browser marker service does not match".to_string());
        }

        if self.has_managed_process(service)? {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Managed browser process already exists".to_string());
        }

        let mut processes = self
            .processes
            .lock()
            .map_err(|_| "Browser session state is unavailable".to_string())?;

        let process_id = child.id();
        processes.insert(
            service,
            ManagedBrowserProcess {
                process_id,
                process_marker: marker.process_marker,
                started_at: marker.started_at,
                child,
            },
        );
        drop(processes);

        if let Err(error) = self.write_registry_snapshot() {
            let _ = self.stop_service(service, Duration::from_secs(1));
            return Err(error);
        }

        Ok(process_id)
    }

    pub fn stop_service(
        &self,
        service: Service,
        timeout: Duration,
    ) -> Result<BrowserSessionStopResult, String> {
        let mut processes = self
            .processes
            .lock()
            .map_err(|_| "Browser session state is unavailable".to_string())?;

        if !processes.contains_key(&service) {
            drop(processes);
            return self.stop_orphaned_service(service, timeout);
        }

        let status = {
            let process = processes
                .get_mut(&service)
                .ok_or_else(|| "Browser session state is unavailable".to_string())?;
            stop_process(process, timeout)?
        };

        processes.remove(&service);
        drop(processes);

        self.write_registry_snapshot()?;
        Ok(BrowserSessionStopResult { service, status })
    }

    pub fn detect_orphans_on_startup(&self) -> Result<BrowserSessionStartupRecovery, String> {
        let records = self.read_registry_records()?;
        let mut orphans = self
            .orphans
            .lock()
            .map_err(|_| "Browser session state is unavailable".to_string())?;
        orphans.clear();

        for record in records {
            if process_matches_marker(record.process_id, &record.process_marker) {
                orphans.insert(
                    record.service,
                    OrphanedBrowserProcess {
                        process_id: record.process_id,
                        process_marker: record.process_marker,
                        started_at: record.started_at,
                    },
                );
            }
        }

        let orphaned_processes = orphans.len();
        drop(orphans);

        self.write_registry_snapshot()?;
        Ok(BrowserSessionStartupRecovery { orphaned_processes })
    }

    fn has_managed_process(&self, service: Service) -> Result<bool, String> {
        let tracked = self
            .processes
            .lock()
            .map_err(|_| "Browser session state is unavailable".to_string())?
            .contains_key(&service);

        if tracked {
            return Ok(true);
        }

        self.orphans
            .lock()
            .map_err(|_| "Browser session state is unavailable".to_string())
            .map(|orphans| orphans.contains_key(&service))
    }

    fn stop_orphaned_service(
        &self,
        service: Service,
        timeout: Duration,
    ) -> Result<BrowserSessionStopResult, String> {
        let mut orphans = self
            .orphans
            .lock()
            .map_err(|_| "Browser session state is unavailable".to_string())?;

        if !orphans.contains_key(&service) {
            return Ok(BrowserSessionStopResult {
                service,
                status: BrowserSessionStopStatus::NoManagedProcess,
            });
        }

        let status = {
            let process = orphans
                .get(&service)
                .ok_or_else(|| "Browser session state is unavailable".to_string())?;
            stop_orphan_process(process, timeout)?
        };

        orphans.remove(&service);
        drop(orphans);

        self.write_registry_snapshot()?;
        Ok(BrowserSessionStopResult { service, status })
    }

    fn read_registry_records(&self) -> Result<Vec<BrowserSessionRecord>, String> {
        let Some(path) = &self.registry_path else {
            return Ok(Vec::new());
        };

        if !path.exists() {
            return Ok(Vec::new());
        }

        let raw = fs::read_to_string(path)
            .map_err(|_| "Could not read browser session registry".to_string())?;
        let registry = serde_json::from_str::<BrowserSessionRegistry>(&raw)
            .map_err(|_| "Could not parse browser session registry".to_string())?;

        if registry.schema_version != SESSION_REGISTRY_SCHEMA_VERSION {
            return Err("Unsupported browser session registry version".to_string());
        }

        Ok(registry.processes)
    }

    fn write_registry_snapshot(&self) -> Result<(), String> {
        let Some(path) = &self.registry_path else {
            return Ok(());
        };

        let records = self.registry_records()?;
        if records.is_empty() {
            remove_registry_file(path)?;
            return Ok(());
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|_| "Could not create browser session registry directory".to_string())?;
            set_restrictive_directory_permissions(parent)?;
        }

        let registry = BrowserSessionRegistry {
            schema_version: SESSION_REGISTRY_SCHEMA_VERSION,
            processes: records,
        };
        let raw = serde_json::to_string_pretty(&registry)
            .map_err(|_| "Could not serialize browser session registry".to_string())?;
        fs::write(path, raw).map_err(|_| "Could not write browser session registry".to_string())?;
        set_restrictive_file_permissions(path)
    }

    fn registry_records(&self) -> Result<Vec<BrowserSessionRecord>, String> {
        let processes = self
            .processes
            .lock()
            .map_err(|_| "Browser session state is unavailable".to_string())?;
        let orphans = self
            .orphans
            .lock()
            .map_err(|_| "Browser session state is unavailable".to_string())?;
        let mut records = Vec::with_capacity(processes.len() + orphans.len());

        for (service, process) in processes.iter() {
            records.push(BrowserSessionRecord::from_managed(*service, process));
        }

        for (service, process) in orphans.iter() {
            records.push(BrowserSessionRecord::from_orphan(*service, process));
        }

        Ok(records)
    }
}

fn stop_process(
    process: &mut ManagedBrowserProcess,
    timeout: Duration,
) -> Result<BrowserSessionStopStatus, String> {
    if process
        .child
        .try_wait()
        .map_err(|_| "Could not inspect managed browser process".to_string())?
        .is_some()
    {
        return Ok(BrowserSessionStopStatus::AlreadyExited);
    }

    request_graceful_shutdown(process)?;
    if wait_for_exit(&mut process.child, timeout)? {
        return Ok(BrowserSessionStopStatus::Stopped);
    }

    process
        .child
        .kill()
        .map_err(|_| "Could not stop managed browser process".to_string())?;
    process
        .child
        .wait()
        .map_err(|_| "Could not reap managed browser process".to_string())?;
    Ok(BrowserSessionStopStatus::Killed)
}

fn stop_orphan_process(
    process: &OrphanedBrowserProcess,
    timeout: Duration,
) -> Result<BrowserSessionStopStatus, String> {
    if !process_matches_marker(process.process_id, &process.process_marker) {
        return Ok(BrowserSessionStopStatus::AlreadyExited);
    }

    request_graceful_shutdown_pid(process.process_id)?;
    if wait_for_pid_exit(process.process_id, &process.process_marker, timeout) {
        return Ok(BrowserSessionStopStatus::Stopped);
    }

    kill_process_pid(process.process_id)?;
    if wait_for_pid_exit(
        process.process_id,
        &process.process_marker,
        Duration::from_secs(1),
    ) {
        return Ok(BrowserSessionStopStatus::Killed);
    }

    Err("Could not stop orphaned managed browser process".to_string())
}

fn wait_for_exit(child: &mut Child, timeout: Duration) -> Result<bool, String> {
    let deadline = Instant::now() + timeout;

    loop {
        if child
            .try_wait()
            .map_err(|_| "Could not inspect managed browser process".to_string())?
            .is_some()
        {
            return Ok(true);
        }

        if Instant::now() >= deadline {
            return Ok(false);
        }

        thread::sleep(Duration::from_millis(20));
    }
}

fn wait_for_pid_exit(process_id: u32, process_marker: &str, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;

    loop {
        if !process_matches_marker(process_id, process_marker) {
            return true;
        }

        if Instant::now() >= deadline {
            return false;
        }

        thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(unix)]
fn request_graceful_shutdown(process: &ManagedBrowserProcess) -> Result<(), String> {
    request_graceful_shutdown_pid(process.process_id)
}

#[cfg(unix)]
fn request_graceful_shutdown_pid(process_id: u32) -> Result<(), String> {
    let result = unsafe { libc::kill(process_id as libc::pid_t, libc::SIGTERM) };

    if result == 0 {
        return Ok(());
    }

    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }

    Err("Could not request managed browser shutdown".to_string())
}

#[cfg(not(unix))]
fn request_graceful_shutdown(_process: &ManagedBrowserProcess) -> Result<(), String> {
    Ok(())
}

#[cfg(not(unix))]
fn request_graceful_shutdown_pid(_process_id: u32) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
fn kill_process_pid(process_id: u32) -> Result<(), String> {
    let result = unsafe { libc::kill(process_id as libc::pid_t, libc::SIGKILL) };

    if result == 0 {
        return Ok(());
    }

    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }

    Err("Could not stop managed browser process".to_string())
}

#[cfg(not(unix))]
fn kill_process_pid(_process_id: u32) -> Result<(), String> {
    Err("Could not stop managed browser process".to_string())
}

#[cfg(unix)]
fn process_matches_marker(process_id: u32, process_marker: &str) -> bool {
    let environ_path = PathBuf::from("/proc")
        .join(process_id.to_string())
        .join("environ");
    let Ok(environ) = fs::read(environ_path) else {
        return false;
    };
    let expected = format!("{PROCESS_MARKER_ENV}={process_marker}");

    environ
        .split(|byte| *byte == 0)
        .filter_map(|entry| std::str::from_utf8(entry).ok())
        .any(|entry| entry == expected)
}

#[cfg(not(unix))]
fn process_matches_marker(_process_id: u32, _process_marker: &str) -> bool {
    false
}

fn remove_registry_file(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("Could not remove browser session registry".to_string()),
    }
}

#[allow(dead_code)]
fn new_process_marker(service: Service) -> String {
    format!("{service:?}-{}-{}", std::process::id(), now_unix_millis())
}

#[allow(dead_code)]
fn now_unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[cfg(unix)]
fn set_restrictive_file_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|_| "Could not set browser session registry permissions".to_string())
}

#[cfg(not(unix))]
fn set_restrictive_file_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
fn set_restrictive_directory_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|_| "Could not set browser session registry directory permissions".to_string())
}

#[cfg(not(unix))]
fn set_restrictive_directory_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
        thread,
    };

    static NEXT_TEST_DIR_ID: AtomicU64 = AtomicU64::new(0);

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new() -> Self {
            let mut path = std::env::temp_dir();
            path.push(format!(
                "forgegauge-browser-session-test-{}-{}",
                std::process::id(),
                NEXT_TEST_DIR_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).expect("test dir is created");
            Self { path }
        }

        fn registry_path(&self) -> PathBuf {
            self.path.join(SESSION_REGISTRY_FILE_NAME)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn stop_service_without_tracked_process_is_noop() {
        let manager = BrowserSessionManager::default();

        let result = manager
            .stop_service(Service::Codex, Duration::from_millis(1))
            .expect("stop succeeds");

        assert_eq!(
            result,
            BrowserSessionStopResult {
                service: Service::Codex,
                status: BrowserSessionStopStatus::NoManagedProcess,
            }
        );
    }

    #[test]
    fn track_process_refuses_duplicate_service_owner() {
        let manager = BrowserSessionManager::default();
        let first_marker = BrowserSessionMarker::new(Service::Codex);
        let second_marker = BrowserSessionMarker::new(Service::Codex);
        let first = sleeping_child(&first_marker);
        let second = sleeping_child(&second_marker);

        manager
            .track_process(Service::Codex, first, first_marker)
            .expect("first process is tracked");
        let error = manager
            .track_process(Service::Codex, second, second_marker)
            .expect_err("duplicate process is rejected");

        assert_eq!(error, "Managed browser process already exists");
        let result = manager
            .stop_service(Service::Codex, Duration::from_secs(1))
            .expect("tracked process stops");
        assert_ne!(result.status, BrowserSessionStopStatus::NoManagedProcess);
    }

    #[test]
    fn stop_service_reaps_exited_process() {
        let manager = BrowserSessionManager::default();
        let marker = BrowserSessionMarker::new(Service::Claude);
        let child = exited_child(&marker);

        manager
            .track_process(Service::Claude, child, marker)
            .expect("process is tracked");
        thread::sleep(Duration::from_millis(50));
        let result = manager
            .stop_service(Service::Claude, Duration::from_millis(1))
            .expect("tracked process stops");

        assert_eq!(result.status, BrowserSessionStopStatus::AlreadyExited);
    }

    #[test]
    fn stop_service_terminates_running_process() {
        let manager = BrowserSessionManager::default();
        let marker = BrowserSessionMarker::new(Service::Codex);
        let child = sleeping_child(&marker);

        manager
            .track_process(Service::Codex, child, marker)
            .expect("process is tracked");
        let result = manager
            .stop_service(Service::Codex, Duration::from_secs(1))
            .expect("tracked process stops");

        assert!(matches!(
            result.status,
            BrowserSessionStopStatus::Stopped | BrowserSessionStopStatus::Killed
        ));
    }

    #[test]
    fn tracked_process_registry_is_removed_after_stop() {
        let dir = TestDir::new();
        let manager = BrowserSessionManager::with_registry_path(dir.registry_path());
        let marker = BrowserSessionMarker::new(Service::Codex);
        let child = sleeping_child(&marker);

        manager
            .track_process(Service::Codex, child, marker)
            .expect("process is tracked");
        assert!(dir.registry_path().exists());

        manager
            .stop_service(Service::Codex, Duration::from_secs(1))
            .expect("tracked process stops");

        assert!(!dir.registry_path().exists());
    }

    #[cfg(unix)]
    #[test]
    fn startup_detects_and_stops_orphaned_process_from_registry() {
        let dir = TestDir::new();
        let registry_path = dir.registry_path();
        let marker = BrowserSessionMarker::new(Service::Claude);
        let mut child = sleeping_child(&marker);
        let process_id = child.id();
        wait_for_test_process_marker(process_id, &marker.process_marker);
        let registry = BrowserSessionRegistry {
            schema_version: SESSION_REGISTRY_SCHEMA_VERSION,
            processes: vec![BrowserSessionRecord {
                service: Service::Claude,
                process_id,
                process_marker: marker.process_marker.clone(),
                started_at: marker.started_at.clone(),
            }],
        };
        fs::write(
            &registry_path,
            serde_json::to_string_pretty(&registry).expect("registry serializes"),
        )
        .expect("registry is written");

        let second_manager = BrowserSessionManager::with_registry_path(registry_path);
        let recovery = second_manager
            .detect_orphans_on_startup()
            .expect("orphan detection succeeds");
        assert_eq!(
            recovery,
            BrowserSessionStartupRecovery {
                orphaned_processes: 1,
            }
        );
        assert!(process_matches_marker(
            process_id,
            marker.process_marker.as_str()
        ));

        let result = second_manager
            .stop_service(Service::Claude, Duration::from_secs(1))
            .expect("orphaned process stops");

        assert!(matches!(
            result.status,
            BrowserSessionStopStatus::Stopped | BrowserSessionStopStatus::Killed
        ));
        let _ = child.wait();
        assert!(!dir.registry_path().exists());
    }

    #[test]
    fn startup_discards_stale_or_unverified_registry_entries() {
        let dir = TestDir::new();
        let registry = BrowserSessionRegistry {
            schema_version: SESSION_REGISTRY_SCHEMA_VERSION,
            processes: vec![BrowserSessionRecord {
                service: Service::Codex,
                process_id: std::process::id(),
                process_marker: "not-this-process".to_string(),
                started_at: "0".to_string(),
            }],
        };
        fs::write(
            dir.registry_path(),
            serde_json::to_string_pretty(&registry).expect("registry serializes"),
        )
        .expect("registry is written");

        let manager = BrowserSessionManager::with_registry_path(dir.registry_path());
        let recovery = manager
            .detect_orphans_on_startup()
            .expect("orphan detection succeeds");

        assert_eq!(
            recovery,
            BrowserSessionStartupRecovery {
                orphaned_processes: 0,
            }
        );
        assert!(!dir.registry_path().exists());
    }

    #[cfg(unix)]
    fn sleeping_child(marker: &BrowserSessionMarker) -> Child {
        let (key, value) = marker.env_pair();
        Command::new("sleep")
            .arg("30")
            .env(key, value)
            .spawn()
            .expect("sleep process starts")
    }

    #[cfg(not(unix))]
    fn sleeping_child(marker: &BrowserSessionMarker) -> Child {
        let (key, value) = marker.env_pair();
        Command::new("cmd")
            .args(["/C", "ping -n 30 127.0.0.1 >NUL"])
            .env(key, value)
            .spawn()
            .expect("sleep process starts")
    }

    #[cfg(unix)]
    fn exited_child(marker: &BrowserSessionMarker) -> Child {
        let (key, value) = marker.env_pair();
        Command::new("true")
            .env(key, value)
            .spawn()
            .expect("short process starts")
    }

    #[cfg(not(unix))]
    fn exited_child(marker: &BrowserSessionMarker) -> Child {
        let (key, value) = marker.env_pair();
        Command::new("cmd")
            .args(["/C", "exit 0"])
            .env(key, value)
            .spawn()
            .expect("short process starts")
    }

    #[cfg(unix)]
    fn wait_for_test_process_marker(process_id: u32, process_marker: &str) {
        let deadline = Instant::now() + Duration::from_secs(1);

        while Instant::now() < deadline {
            if process_matches_marker(process_id, process_marker) {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }

        panic!("test process marker was not visible");
    }
}
