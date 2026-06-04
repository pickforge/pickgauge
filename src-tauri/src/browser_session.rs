use crate::usage::Service;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fmt, fs,
    path::{Path, PathBuf},
    process::Child,
    sync::Mutex,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const PROFILE_STOP_TIMEOUT: Duration = Duration::from_secs(3);
pub const SESSION_REGISTRY_FILE_NAME: &str = "managed-browser-sessions.json";
pub const PROCESS_MARKER_ENV: &str = "FORGEGAUGE_BROWSER_PROCESS_MARKER";
pub const CHROMIUM_DEFAULT_PROFILE_DIR: &str = "Default";
pub const CHROMIUM_PREFERENCES_FILE_NAME: &str = "Preferences";
pub const PROFILE_INSPECTION_ENTRY_LIMIT: usize = 2_048;
pub const PLAYWRIGHT_BACKEND_ID: &str = "playwright-headed-chromium-sidecar";
pub const PLAYWRIGHT_SIDECAR_ACTION_LAUNCH_LOGIN: &str = "launchLogin";
pub const PLAYWRIGHT_SIDECAR_ACTION_REFRESH_USAGE: &str = "refreshUsage";
pub const PLAYWRIGHT_SIDECAR_PROTOCOL_VERSION: u32 = 1;
pub const PLAYWRIGHT_SIDECAR_STATUS_CHECKED: &str = "checked";
pub const PLAYWRIGHT_SIDECAR_STATUS_LAUNCHED: &str = "launched";

const SESSION_REGISTRY_SCHEMA_VERSION: u32 = 1;
const CHROMIUM_PASSWORD_MANAGER_FLAGS: [&str; 4] = [
    "--disable-save-password-bubble",
    "--disable-password-manager-reauthentication",
    "--disable-features=AutofillServerCommunication",
    "--no-first-run",
];

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

#[derive(Clone, Eq, PartialEq)]
pub struct BrowserLaunchPlan {
    pub service: Service,
    pub profile_path: PathBuf,
    pub profile_label: String,
    pub args: Vec<String>,
    pub preferences: serde_json::Value,
    pub diagnostics: BrowserLaunchDiagnostics,
}

impl fmt::Debug for BrowserLaunchPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let profile_path = format!("<{}>", self.profile_label);

        formatter
            .debug_struct("BrowserLaunchPlan")
            .field("service", &self.service)
            .field("profile_path", &profile_path)
            .field("profile_label", &self.profile_label)
            .field("args", &self.diagnostics.args)
            .field("preferences", &self.preferences)
            .field("diagnostics", &self.diagnostics)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserLaunchDiagnostics {
    pub service: Service,
    pub profile_label: String,
    pub args: Vec<String>,
}

#[derive(Clone, Eq, PartialEq)]
pub struct PlaywrightLaunchRequest {
    pub service: Service,
    pub backend: &'static str,
    pub user_data_dir: PathBuf,
    pub profile_label: String,
    pub headless: bool,
    pub args: Vec<String>,
    pub diagnostics: PlaywrightLaunchDiagnostics,
}

impl fmt::Debug for PlaywrightLaunchRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let user_data_dir = format!("<{}>", self.profile_label);

        formatter
            .debug_struct("PlaywrightLaunchRequest")
            .field("service", &self.service)
            .field("backend", &self.backend)
            .field("user_data_dir", &user_data_dir)
            .field("profile_label", &self.profile_label)
            .field("headless", &self.headless)
            .field("args", &self.diagnostics.args)
            .field("diagnostics", &self.diagnostics)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaywrightLaunchDiagnostics {
    pub service: Service,
    pub backend: &'static str,
    pub profile_label: String,
    pub user_data_dir: String,
    pub headless: bool,
    pub args: Vec<String>,
}

#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightSidecarLaunchRequest {
    pub protocol_version: u32,
    pub action: &'static str,
    pub backend: &'static str,
    pub service: Service,
    pub url: String,
    pub profile_label: String,
    pub user_data_dir: String,
    pub headless: bool,
    pub args: Vec<String>,
    #[serde(skip)]
    pub diagnostics: PlaywrightSidecarLaunchDiagnostics,
}

impl fmt::Debug for PlaywrightSidecarLaunchRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let user_data_dir = format!("<{}>", self.profile_label);

        formatter
            .debug_struct("PlaywrightSidecarLaunchRequest")
            .field("protocol_version", &self.protocol_version)
            .field("action", &self.action)
            .field("backend", &self.backend)
            .field("service", &self.service)
            .field("url", &self.url)
            .field("profile_label", &self.profile_label)
            .field("user_data_dir", &user_data_dir)
            .field("headless", &self.headless)
            .field("arg_count", &self.diagnostics.arg_count)
            .field("diagnostics", &self.diagnostics)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaywrightSidecarLaunchDiagnostics {
    pub protocol_version: u32,
    pub action: &'static str,
    pub backend: &'static str,
    pub service: Service,
    pub profile_label: String,
    pub user_data_dir: String,
    pub headless: bool,
    pub arg_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaywrightSidecarLaunchResponse {
    pub protocol_version: u32,
    pub action: String,
    pub backend: String,
    pub service: Service,
    pub profile_label: String,
    pub headless: bool,
    pub arg_count: usize,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlaywrightSidecarUsageResponse {
    pub protocol_version: u32,
    pub action: String,
    pub backend: String,
    pub service: Service,
    pub profile_label: String,
    pub headless: bool,
    pub arg_count: usize,
    pub status: String,
    pub page_state: String,
    pub remaining_percent: Option<f32>,
    pub used_percent: Option<f32>,
    pub reset_at: Option<String>,
    pub visible_fields: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPlaywrightSidecarLaunchResponse {
    ok: bool,
    status: String,
    protocol_version: u32,
    action: Option<String>,
    backend: Option<String>,
    service: Option<Service>,
    profile_label: Option<String>,
    headless: Option<bool>,
    arg_count: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPlaywrightSidecarUsageResponse {
    ok: bool,
    status: String,
    protocol_version: u32,
    action: Option<String>,
    backend: Option<String>,
    service: Option<Service>,
    profile_label: Option<String>,
    headless: Option<bool>,
    arg_count: Option<usize>,
    page_state: Option<String>,
    remaining_percent: Option<f32>,
    used_percent: Option<f32>,
    reset_at: Option<String>,
    visible_fields: Option<Vec<String>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserProfileStorageInspection {
    pub credential_store_files: usize,
    pub autofill_store_files: usize,
    pub cookie_store_files: usize,
    pub site_storage_entries: usize,
    pub symlink_entries: usize,
    pub password_saving_enabled: bool,
    pub autofill_enabled: bool,
    pub inspected_entries: usize,
    pub entry_limit_reached: bool,
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

#[allow(dead_code)]
pub fn chromium_launch_plan(
    service: Service,
    profile_path: impl Into<PathBuf>,
) -> BrowserLaunchPlan {
    let profile_path = profile_path.into();
    let profile_label = profile_label(service);
    let mut args = Vec::with_capacity(CHROMIUM_PASSWORD_MANAGER_FLAGS.len() + 1);
    args.push(format!("--user-data-dir={}", profile_path.display()));
    args.extend(
        CHROMIUM_PASSWORD_MANAGER_FLAGS
            .iter()
            .map(|flag| flag.to_string()),
    );

    let diagnostics = BrowserLaunchDiagnostics {
        service,
        profile_label: profile_label.clone(),
        args: sanitized_launch_args(&args, profile_label.as_str()),
    };
    let preferences = chromium_disabled_storage_preferences();

    BrowserLaunchPlan {
        service,
        profile_path,
        profile_label,
        args,
        preferences,
        diagnostics,
    }
}

#[allow(dead_code)]
pub fn playwright_launch_request(plan: &BrowserLaunchPlan) -> PlaywrightLaunchRequest {
    let args = playwright_launch_args(&plan.args);
    let user_data_dir = format!("<{}>", plan.profile_label);
    let diagnostics = PlaywrightLaunchDiagnostics {
        service: plan.service,
        backend: PLAYWRIGHT_BACKEND_ID,
        profile_label: plan.profile_label.clone(),
        user_data_dir,
        headless: false,
        args: sanitized_launch_args(&args, plan.profile_label.as_str()),
    };

    PlaywrightLaunchRequest {
        service: plan.service,
        backend: PLAYWRIGHT_BACKEND_ID,
        user_data_dir: plan.profile_path.clone(),
        profile_label: plan.profile_label.clone(),
        headless: false,
        args,
        diagnostics,
    }
}

#[allow(dead_code)]
pub fn playwright_sidecar_launch_request(
    request: &PlaywrightLaunchRequest,
    url: impl Into<String>,
) -> Result<PlaywrightSidecarLaunchRequest, String> {
    playwright_sidecar_request(
        request,
        url,
        PLAYWRIGHT_SIDECAR_ACTION_LAUNCH_LOGIN,
        false,
        "Managed browser login URL must use HTTPS",
    )
}

#[allow(dead_code)]
pub fn playwright_sidecar_refresh_request(
    request: &PlaywrightLaunchRequest,
    url: impl Into<String>,
) -> Result<PlaywrightSidecarLaunchRequest, String> {
    playwright_sidecar_request(
        request,
        url,
        PLAYWRIGHT_SIDECAR_ACTION_REFRESH_USAGE,
        true,
        "Managed browser refresh URL must use HTTPS",
    )
}

fn playwright_sidecar_request(
    request: &PlaywrightLaunchRequest,
    url: impl Into<String>,
    action: &'static str,
    headless: bool,
    https_error: &str,
) -> Result<PlaywrightSidecarLaunchRequest, String> {
    let url = url.into();
    if !url.starts_with("https://") {
        return Err(https_error.to_string());
    }

    let user_data_dir = request.user_data_dir.to_string_lossy().to_string();
    let redacted_user_data_dir = format!("<{}>", request.profile_label);
    let diagnostics = PlaywrightSidecarLaunchDiagnostics {
        protocol_version: PLAYWRIGHT_SIDECAR_PROTOCOL_VERSION,
        action,
        backend: request.backend,
        service: request.service,
        profile_label: request.profile_label.clone(),
        user_data_dir: redacted_user_data_dir,
        headless,
        arg_count: request.args.len(),
    };

    Ok(PlaywrightSidecarLaunchRequest {
        protocol_version: PLAYWRIGHT_SIDECAR_PROTOCOL_VERSION,
        action,
        backend: request.backend,
        service: request.service,
        url,
        profile_label: request.profile_label.clone(),
        user_data_dir,
        headless,
        args: request.args.clone(),
        diagnostics,
    })
}

#[allow(dead_code)]
pub fn playwright_sidecar_launch_response(
    raw: &str,
    request: &PlaywrightSidecarLaunchRequest,
) -> Result<PlaywrightSidecarLaunchResponse, String> {
    let response = serde_json::from_str::<RawPlaywrightSidecarLaunchResponse>(raw)
        .map_err(|_| "Managed login sidecar returned invalid response".to_string())?;

    if !response.ok {
        return Err("Managed login sidecar rejected launch".to_string());
    }

    if response.status != PLAYWRIGHT_SIDECAR_STATUS_LAUNCHED {
        return Err("Managed login sidecar did not launch browser".to_string());
    }

    let Some(action) = response.action else {
        return Err("Managed login sidecar returned mismatched response".to_string());
    };
    let Some(backend) = response.backend else {
        return Err("Managed login sidecar returned mismatched response".to_string());
    };
    let Some(service) = response.service else {
        return Err("Managed login sidecar returned mismatched response".to_string());
    };
    let Some(profile_label) = response.profile_label else {
        return Err("Managed login sidecar returned mismatched response".to_string());
    };
    let Some(headless) = response.headless else {
        return Err("Managed login sidecar returned mismatched response".to_string());
    };
    let Some(arg_count) = response.arg_count else {
        return Err("Managed login sidecar returned mismatched response".to_string());
    };

    if response.protocol_version != request.protocol_version
        || action != request.action
        || backend != request.backend
        || service != request.service
        || profile_label != request.profile_label
        || headless != request.headless
        || arg_count != request.args.len()
    {
        return Err("Managed login sidecar returned mismatched response".to_string());
    }

    Ok(PlaywrightSidecarLaunchResponse {
        protocol_version: response.protocol_version,
        action,
        backend,
        service,
        profile_label,
        headless,
        arg_count,
        status: response.status,
    })
}

#[allow(dead_code)]
pub fn playwright_sidecar_usage_response(
    raw: &str,
    request: &PlaywrightSidecarLaunchRequest,
) -> Result<PlaywrightSidecarUsageResponse, String> {
    let response = serde_json::from_str::<RawPlaywrightSidecarUsageResponse>(raw)
        .map_err(|_| "Managed usage sidecar returned invalid response".to_string())?;

    if !response.ok {
        return Err("Managed usage sidecar rejected refresh".to_string());
    }

    if response.status != PLAYWRIGHT_SIDECAR_STATUS_CHECKED {
        return Err("Managed usage sidecar did not check usage".to_string());
    }

    let Some(action) = response.action else {
        return Err("Managed usage sidecar returned mismatched response".to_string());
    };
    let Some(backend) = response.backend else {
        return Err("Managed usage sidecar returned mismatched response".to_string());
    };
    let Some(service) = response.service else {
        return Err("Managed usage sidecar returned mismatched response".to_string());
    };
    let Some(profile_label) = response.profile_label else {
        return Err("Managed usage sidecar returned mismatched response".to_string());
    };
    let Some(headless) = response.headless else {
        return Err("Managed usage sidecar returned mismatched response".to_string());
    };
    let Some(arg_count) = response.arg_count else {
        return Err("Managed usage sidecar returned mismatched response".to_string());
    };
    let Some(page_state) = response.page_state else {
        return Err("Managed usage sidecar returned invalid page state".to_string());
    };
    let visible_fields = response.visible_fields.unwrap_or_default();

    if response.protocol_version != request.protocol_version
        || action != request.action
        || backend != request.backend
        || service != request.service
        || profile_label != request.profile_label
        || headless != request.headless
        || arg_count != request.args.len()
    {
        return Err("Managed usage sidecar returned mismatched response".to_string());
    }

    if page_state.len() > 32
        || !page_state
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
    {
        return Err("Managed usage sidecar returned invalid page state".to_string());
    }

    if visible_fields.iter().any(|field| {
        field.is_empty()
            || field.len() > 64
            || !field
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
    }) {
        return Err("Managed usage sidecar returned invalid visible fields".to_string());
    }

    Ok(PlaywrightSidecarUsageResponse {
        protocol_version: response.protocol_version,
        action,
        backend,
        service,
        profile_label,
        headless,
        arg_count,
        status: response.status,
        page_state,
        remaining_percent: response.remaining_percent,
        used_percent: response.used_percent,
        reset_at: response.reset_at,
        visible_fields,
    })
}

fn chromium_disabled_storage_preferences() -> serde_json::Value {
    json!({
        "autofill": {
            "credit_card_enabled": false,
            "enabled": false,
            "profile_enabled": false
        },
        "credentials_enable_autosignin": false,
        "credentials_enable_service": false,
        "profile": {
            "password_manager_allow_show_passwords": false,
            "password_manager_enabled": false
        }
    })
}

#[allow(dead_code)]
pub fn prepare_chromium_profile_preferences(profile_path: &Path) -> Result<PathBuf, String> {
    reject_symlink_path(profile_path)?;
    fs::create_dir_all(profile_path)
        .map_err(|_| "Could not prepare managed browser preferences".to_string())?;
    set_restrictive_directory_permissions(profile_path)
        .map_err(|_| "Could not prepare managed browser preferences".to_string())?;

    let default_profile_dir = profile_path.join(CHROMIUM_DEFAULT_PROFILE_DIR);
    reject_symlink_path(&default_profile_dir)?;
    fs::create_dir_all(&default_profile_dir)
        .map_err(|_| "Could not prepare managed browser preferences".to_string())?;
    set_restrictive_directory_permissions(&default_profile_dir)
        .map_err(|_| "Could not prepare managed browser preferences".to_string())?;

    let preferences_path = default_profile_dir.join(CHROMIUM_PREFERENCES_FILE_NAME);
    reject_symlink_path(&preferences_path)?;
    let mut preferences = read_chromium_preferences(&preferences_path)?;
    merge_chromium_preferences(&mut preferences, &chromium_disabled_storage_preferences())?;

    let raw = serde_json::to_string_pretty(&preferences)
        .map_err(|_| "Could not serialize managed browser preferences".to_string())?;
    fs::write(&preferences_path, raw)
        .map_err(|_| "Could not write managed browser preferences".to_string())?;
    set_restrictive_file_permissions(&preferences_path)
        .map_err(|_| "Could not write managed browser preferences".to_string())?;

    Ok(preferences_path)
}

#[allow(dead_code)]
pub fn inspect_chromium_profile_storage(
    profile_path: &Path,
) -> Result<BrowserProfileStorageInspection, String> {
    let mut inspection = BrowserProfileStorageInspection {
        credential_store_files: 0,
        autofill_store_files: 0,
        cookie_store_files: 0,
        site_storage_entries: 0,
        symlink_entries: 0,
        password_saving_enabled: false,
        autofill_enabled: false,
        inspected_entries: 0,
        entry_limit_reached: false,
    };

    match fs::symlink_metadata(profile_path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            inspection.symlink_entries = 1;
            return Ok(inspection);
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err("Managed browser profile must be a directory".to_string());
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(inspection),
        Err(_) => return Err("Could not inspect managed browser profile".to_string()),
    }

    inspect_profile_entries(profile_path, &mut inspection)?;
    let preferences_path = profile_path
        .join(CHROMIUM_DEFAULT_PROFILE_DIR)
        .join(CHROMIUM_PREFERENCES_FILE_NAME);
    apply_preference_inspection(&preferences_path, &mut inspection)?;

    Ok(inspection)
}

fn read_chromium_preferences(path: &Path) -> Result<Value, String> {
    if !path.exists() {
        return Ok(json!({}));
    }

    let raw = fs::read_to_string(path)
        .map_err(|_| "Could not read managed browser preferences".to_string())?;
    let preferences = serde_json::from_str::<Value>(&raw)
        .map_err(|_| "Could not parse managed browser preferences".to_string())?;

    if !preferences.is_object() {
        return Err("Managed browser preferences must be a JSON object".to_string());
    }

    Ok(preferences)
}

fn inspect_profile_entries(
    profile_path: &Path,
    inspection: &mut BrowserProfileStorageInspection,
) -> Result<(), String> {
    let mut pending = vec![profile_path.to_path_buf()];

    while let Some(path) = pending.pop() {
        if inspection.inspected_entries >= PROFILE_INSPECTION_ENTRY_LIMIT {
            inspection.entry_limit_reached = true;
            return Ok(());
        }

        let entries = match fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(_) => return Err("Could not inspect managed browser profile".to_string()),
        };

        for entry in entries {
            if inspection.inspected_entries >= PROFILE_INSPECTION_ENTRY_LIMIT {
                inspection.entry_limit_reached = true;
                return Ok(());
            }

            let entry =
                entry.map_err(|_| "Could not inspect managed browser profile".to_string())?;
            inspection.inspected_entries += 1;
            let metadata = entry
                .path()
                .symlink_metadata()
                .map_err(|_| "Could not inspect managed browser profile".to_string())?;

            if metadata.file_type().is_symlink() {
                inspection.symlink_entries += 1;
                continue;
            }

            if is_chromium_login_data_file(&entry.file_name()) {
                inspection.credential_store_files += 1;
            }

            if is_chromium_autofill_data_file(&entry.file_name()) {
                inspection.autofill_store_files += 1;
            }

            if is_chromium_cookie_data_file(&entry.file_name()) {
                inspection.cookie_store_files += 1;
            }

            if is_chromium_site_storage_entry(&entry.file_name()) {
                inspection.site_storage_entries += 1;
            }

            if metadata.is_dir() {
                pending.push(entry.path());
            }
        }
    }

    Ok(())
}

fn apply_preference_inspection(
    preferences_path: &Path,
    inspection: &mut BrowserProfileStorageInspection,
) -> Result<(), String> {
    match fs::symlink_metadata(preferences_path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            inspection.symlink_entries += 1;
            return Ok(());
        }
        Ok(metadata) if !metadata.is_file() => return Ok(()),
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("Could not inspect managed browser preferences".to_string()),
    }

    let preferences = read_chromium_preferences(preferences_path)?;
    inspection.password_saving_enabled =
        preference_bool(&preferences, &["credentials_enable_service"])
            || preference_bool(&preferences, &["credentials_enable_autosignin"])
            || preference_bool(&preferences, &["profile", "password_manager_enabled"])
            || preference_bool(
                &preferences,
                &["profile", "password_manager_allow_show_passwords"],
            );
    inspection.autofill_enabled = preference_bool(&preferences, &["autofill", "enabled"])
        || preference_bool(&preferences, &["autofill", "profile_enabled"])
        || preference_bool(&preferences, &["autofill", "credit_card_enabled"]);

    Ok(())
}

fn preference_bool(preferences: &Value, path: &[&str]) -> bool {
    path.iter()
        .try_fold(preferences, |value, segment| value.get(*segment))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn is_chromium_login_data_file(name: &std::ffi::OsStr) -> bool {
    name.to_str()
        .map(|name| name == "Login Data" || name.starts_with("Login Data-"))
        .unwrap_or(false)
}

fn is_chromium_autofill_data_file(name: &std::ffi::OsStr) -> bool {
    name.to_str()
        .map(|name| name == "Web Data" || name.starts_with("Web Data-"))
        .unwrap_or(false)
}

fn is_chromium_cookie_data_file(name: &std::ffi::OsStr) -> bool {
    name.to_str()
        .map(|name| name == "Cookies" || name.starts_with("Cookies-"))
        .unwrap_or(false)
}

fn is_chromium_site_storage_entry(name: &std::ffi::OsStr) -> bool {
    matches!(
        name.to_str(),
        Some("IndexedDB" | "Local Storage" | "Session Storage" | "Service Worker")
    )
}

fn merge_chromium_preferences(target: &mut Value, patch: &Value) -> Result<(), String> {
    let target = target
        .as_object_mut()
        .ok_or_else(|| "Managed browser preferences must be a JSON object".to_string())?;
    let patch = patch
        .as_object()
        .ok_or_else(|| "Managed browser preferences patch must be a JSON object".to_string())?;

    for (key, value) in patch {
        match (target.get_mut(key), value) {
            (Some(existing), Value::Object(_)) if existing.is_object() => {
                merge_chromium_preferences(existing, value)?;
            }
            _ => {
                target.insert(key.clone(), value.clone());
            }
        }
    }

    Ok(())
}

fn sanitized_launch_args(args: &[String], profile_label: &str) -> Vec<String> {
    args.iter()
        .map(|arg| {
            if arg.starts_with("--user-data-dir=") {
                format!("--user-data-dir=<{profile_label}>")
            } else {
                arg.clone()
            }
        })
        .collect()
}

fn playwright_launch_args(args: &[String]) -> Vec<String> {
    args.iter()
        .filter(|arg| !arg.starts_with("--user-data-dir="))
        .cloned()
        .collect()
}

fn profile_label(service: Service) -> String {
    match service {
        Service::Codex => "codex-profile".to_string(),
        Service::Claude => "claude-profile".to_string(),
    }
}

fn reject_symlink_path(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err("Managed browser path must not be a symlink".to_string())
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("Could not inspect managed browser path".to_string()),
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

    #[test]
    fn chromium_launch_plan_uses_service_profile_path() {
        let profile_path = PathBuf::from("/tmp/forgegauge/browser-profiles/codex");
        let plan = chromium_launch_plan(Service::Codex, profile_path.clone());

        assert_eq!(plan.service, Service::Codex);
        assert_eq!(plan.profile_path, profile_path);
        assert_eq!(plan.profile_label, "codex-profile");
        assert!(plan
            .args
            .contains(&"--user-data-dir=/tmp/forgegauge/browser-profiles/codex".to_string()));
    }

    #[test]
    fn chromium_launch_plan_disables_password_and_autofill_prompts() {
        let plan = chromium_launch_plan(Service::Claude, "/tmp/forgegauge/browser-profiles/claude");

        assert!(plan
            .args
            .contains(&"--disable-save-password-bubble".to_string()));
        assert!(plan
            .args
            .contains(&"--disable-password-manager-reauthentication".to_string()));
        assert!(plan
            .args
            .contains(&"--disable-features=AutofillServerCommunication".to_string()));
        assert!(plan.args.contains(&"--no-first-run".to_string()));
        assert_eq!(plan.preferences["credentials_enable_service"], false);
        assert_eq!(plan.preferences["credentials_enable_autosignin"], false);
        assert_eq!(
            plan.preferences["profile"]["password_manager_enabled"],
            false
        );
        assert_eq!(
            plan.preferences["profile"]["password_manager_allow_show_passwords"],
            false
        );
        assert_eq!(plan.preferences["autofill"]["enabled"], false);
        assert_eq!(plan.preferences["autofill"]["profile_enabled"], false);
        assert_eq!(plan.preferences["autofill"]["credit_card_enabled"], false);
    }

    #[test]
    fn launch_diagnostics_redact_raw_profile_paths() {
        let profile_path =
            "/home/dev/.local/share/com.pickforge.forgegauge/browser-profiles/claude";
        let plan = chromium_launch_plan(Service::Claude, profile_path);
        let diagnostics = format!("{:?}", plan.diagnostics);

        assert_eq!(plan.diagnostics.service, Service::Claude);
        assert_eq!(plan.diagnostics.profile_label, "claude-profile");
        assert!(plan
            .diagnostics
            .args
            .contains(&"--user-data-dir=<claude-profile>".to_string()));
        assert!(!diagnostics.contains(profile_path));
        assert!(!diagnostics.contains("/home/dev"));
    }

    #[test]
    fn launch_plan_debug_redacts_raw_profile_paths_and_args() {
        let profile_path = "/home/dev/.local/share/com.pickforge.forgegauge/browser-profiles/codex";
        let plan = chromium_launch_plan(Service::Codex, profile_path);
        let debug = format!("{plan:?}");

        assert!(debug.contains("codex-profile"));
        assert!(debug.contains("--user-data-dir=<codex-profile>"));
        assert!(!debug.contains(profile_path));
        assert!(!debug.contains("/home/dev"));
        assert!(!debug.contains("--user-data-dir=/"));
    }

    #[test]
    fn service_launch_plans_have_distinct_profile_labels() {
        let codex = chromium_launch_plan(Service::Codex, "/tmp/profiles/codex");
        let claude = chromium_launch_plan(Service::Claude, "/tmp/profiles/claude");

        assert_ne!(codex.profile_label, claude.profile_label);
        assert_ne!(
            codex.diagnostics.profile_label,
            claude.diagnostics.profile_label
        );
        assert_eq!(codex.diagnostics.args[0], "--user-data-dir=<codex-profile>");
        assert_eq!(
            claude.diagnostics.args[0],
            "--user-data-dir=<claude-profile>"
        );
    }

    #[test]
    fn playwright_launch_request_uses_persistent_context_contract() {
        let profile_path = PathBuf::from("/tmp/forgegauge/browser-profiles/codex");
        let plan = chromium_launch_plan(Service::Codex, profile_path.clone());
        let request = playwright_launch_request(&plan);

        assert_eq!(request.service, Service::Codex);
        assert_eq!(request.backend, PLAYWRIGHT_BACKEND_ID);
        assert_eq!(request.user_data_dir, profile_path);
        assert_eq!(request.profile_label, "codex-profile");
        assert!(!request.headless);
        assert!(!request
            .args
            .iter()
            .any(|arg| arg.starts_with("--user-data-dir=")));
        assert!(request
            .args
            .contains(&"--disable-save-password-bubble".to_string()));
        assert!(request.args.contains(&"--no-first-run".to_string()));
    }

    #[test]
    fn playwright_launch_request_diagnostics_redact_user_data_dir() {
        let profile_path =
            "/home/dev/.local/share/com.pickforge.forgegauge/browser-profiles/claude";
        let plan = chromium_launch_plan(Service::Claude, profile_path);
        let request = playwright_launch_request(&plan);
        let diagnostics = format!("{:?}", request.diagnostics);
        let debug = format!("{request:?}");

        assert_eq!(request.diagnostics.backend, PLAYWRIGHT_BACKEND_ID);
        assert_eq!(request.diagnostics.user_data_dir, "<claude-profile>");
        assert!(request
            .diagnostics
            .args
            .contains(&"--disable-save-password-bubble".to_string()));
        assert!(!request
            .diagnostics
            .args
            .iter()
            .any(|arg| arg.starts_with("--user-data-dir=")));
        assert!(!diagnostics.contains(profile_path));
        assert!(!debug.contains(profile_path));
        assert!(!debug.contains("/home/dev"));
    }

    #[test]
    fn playwright_sidecar_launch_request_serializes_to_protocol_shape() {
        let profile_path = "/tmp/forgegauge/browser-profiles/codex";
        let plan = chromium_launch_plan(Service::Codex, profile_path);
        let launch_request = playwright_launch_request(&plan);
        let sidecar_request = playwright_sidecar_launch_request(
            &launch_request,
            "https://chatgpt.com/codex/cloud/settings/analytics",
        )
        .expect("sidecar request is created");
        let value = serde_json::to_value(&sidecar_request).expect("sidecar request serializes");

        assert_eq!(sidecar_request.protocol_version, 1);
        assert_eq!(sidecar_request.action, "launchLogin");
        assert_eq!(sidecar_request.backend, PLAYWRIGHT_BACKEND_ID);
        assert_eq!(sidecar_request.service, Service::Codex);
        assert_eq!(sidecar_request.profile_label, "codex-profile");
        assert_eq!(sidecar_request.user_data_dir, profile_path);
        assert!(!sidecar_request.headless);
        assert_eq!(value["protocolVersion"], 1);
        assert_eq!(value["action"], "launchLogin");
        assert_eq!(value["backend"], "playwright-headed-chromium-sidecar");
        assert_eq!(value["service"], "codex");
        assert_eq!(value["profileLabel"], "codex-profile");
        assert_eq!(value["userDataDir"], profile_path);
        assert_eq!(value["headless"], false);
        assert!(value["args"]
            .as_array()
            .expect("args are an array")
            .iter()
            .all(|arg| !arg
                .as_str()
                .expect("arg is a string")
                .starts_with("--user-data-dir=")));
        assert!(value.get("diagnostics").is_none());
    }

    #[test]
    fn playwright_sidecar_launch_request_debug_redacts_sensitive_launch_input() {
        let profile_path =
            "/home/dev/.local/share/com.pickforge.forgegauge/browser-profiles/claude";
        let plan = chromium_launch_plan(Service::Claude, profile_path);
        let launch_request = playwright_launch_request(&plan);
        let sidecar_request =
            playwright_sidecar_launch_request(&launch_request, "https://claude.ai/usage")
                .expect("sidecar request is created");
        let diagnostics = format!("{:?}", sidecar_request.diagnostics);
        let debug = format!("{sidecar_request:?}");

        assert_eq!(
            sidecar_request.diagnostics.user_data_dir,
            "<claude-profile>"
        );
        assert_eq!(sidecar_request.diagnostics.arg_count, 4);
        assert!(!diagnostics.contains(profile_path));
        assert!(!debug.contains(profile_path));
        assert!(!debug.contains("/home/dev"));
        assert!(!debug.contains("--disable-save-password-bubble"));
    }

    #[test]
    fn playwright_sidecar_refresh_request_serializes_to_headless_protocol_shape() {
        let profile_path = "/tmp/forgegauge/browser-profiles/claude";
        let plan = chromium_launch_plan(Service::Claude, profile_path);
        let launch_request = playwright_launch_request(&plan);
        let sidecar_request =
            playwright_sidecar_refresh_request(&launch_request, "https://claude.ai/new")
                .expect("sidecar request is created");
        let value = serde_json::to_value(&sidecar_request).expect("sidecar request serializes");

        assert_eq!(sidecar_request.protocol_version, 1);
        assert_eq!(sidecar_request.action, "refreshUsage");
        assert_eq!(sidecar_request.backend, PLAYWRIGHT_BACKEND_ID);
        assert_eq!(sidecar_request.service, Service::Claude);
        assert_eq!(sidecar_request.profile_label, "claude-profile");
        assert_eq!(sidecar_request.user_data_dir, profile_path);
        assert!(sidecar_request.headless);
        assert_eq!(value["protocolVersion"], 1);
        assert_eq!(value["action"], "refreshUsage");
        assert_eq!(value["backend"], "playwright-headed-chromium-sidecar");
        assert_eq!(value["service"], "claude");
        assert_eq!(value["profileLabel"], "claude-profile");
        assert_eq!(value["userDataDir"], profile_path);
        assert_eq!(value["headless"], true);
        assert!(value.get("diagnostics").is_none());
    }

    #[test]
    fn playwright_sidecar_refresh_request_debug_redacts_sensitive_input() {
        let profile_path = "/home/dev/.local/share/com.pickforge.forgegauge/browser-profiles/codex";
        let plan = chromium_launch_plan(Service::Codex, profile_path);
        let launch_request = playwright_launch_request(&plan);
        let sidecar_request = playwright_sidecar_refresh_request(
            &launch_request,
            "https://chatgpt.com/codex/cloud/settings/analytics",
        )
        .expect("sidecar request is created");
        let diagnostics = format!("{:?}", sidecar_request.diagnostics);
        let debug = format!("{sidecar_request:?}");

        assert_eq!(sidecar_request.diagnostics.user_data_dir, "<codex-profile>");
        assert!(sidecar_request.diagnostics.headless);
        assert!(!diagnostics.contains(profile_path));
        assert!(!debug.contains(profile_path));
        assert!(!debug.contains("/home/dev"));
        assert!(!debug.contains("--disable-save-password-bubble"));
    }

    #[test]
    fn playwright_sidecar_launch_request_rejects_non_https_urls() {
        let plan = chromium_launch_plan(Service::Codex, "/tmp/forgegauge/codex");
        let launch_request = playwright_launch_request(&plan);
        let error = playwright_sidecar_launch_request(&launch_request, "http://example.test")
            .expect_err("non-https urls are rejected");

        assert_eq!(error, "Managed browser login URL must use HTTPS");
        assert!(!error.contains("example.test"));
    }

    #[test]
    fn playwright_sidecar_launch_response_accepts_launched_status() {
        let plan = chromium_launch_plan(Service::Codex, "/tmp/forgegauge/codex");
        let launch_request = playwright_launch_request(&plan);
        let sidecar_request = playwright_sidecar_launch_request(
            &launch_request,
            "https://chatgpt.com/codex/cloud/settings/analytics",
        )
        .expect("sidecar request is created");
        let raw = serde_json::json!({
            "ok": true,
            "status": "launched",
            "protocolVersion": 1,
            "action": "launchLogin",
            "backend": "playwright-headed-chromium-sidecar",
            "service": "codex",
            "profileLabel": "codex-profile",
            "headless": false,
            "argCount": sidecar_request.args.len()
        })
        .to_string();
        let response = playwright_sidecar_launch_response(&raw, &sidecar_request)
            .expect("launched response is accepted");

        assert_eq!(response.status, PLAYWRIGHT_SIDECAR_STATUS_LAUNCHED);
        assert_eq!(response.service, Service::Codex);
        assert_eq!(response.profile_label, "codex-profile");
        assert_eq!(response.arg_count, sidecar_request.args.len());
    }

    #[test]
    fn playwright_sidecar_usage_response_accepts_checked_status() {
        let plan = chromium_launch_plan(Service::Codex, "/tmp/forgegauge/codex");
        let launch_request = playwright_launch_request(&plan);
        let sidecar_request = playwright_sidecar_refresh_request(
            &launch_request,
            "https://chatgpt.com/codex/cloud/settings/analytics",
        )
        .expect("sidecar request is created");
        let raw = serde_json::json!({
            "ok": true,
            "status": "checked",
            "protocolVersion": 1,
            "action": "refreshUsage",
            "backend": "playwright-headed-chromium-sidecar",
            "service": "codex",
            "profileLabel": "codex-profile",
            "headless": true,
            "argCount": sidecar_request.args.len(),
            "pageState": "usage",
            "remainingPercent": 62.5,
            "usedPercent": 37.5,
            "resetAt": "2026-06-05T00:00:00Z",
            "visibleFields": ["remaining_percent", "used_percent", "reset_at"]
        })
        .to_string();
        let response = playwright_sidecar_usage_response(&raw, &sidecar_request)
            .expect("checked response is accepted");

        assert_eq!(response.status, PLAYWRIGHT_SIDECAR_STATUS_CHECKED);
        assert_eq!(response.action, PLAYWRIGHT_SIDECAR_ACTION_REFRESH_USAGE);
        assert_eq!(response.page_state, "usage");
        assert_eq!(response.remaining_percent, Some(62.5));
        assert_eq!(response.used_percent, Some(37.5));
        assert_eq!(
            response.visible_fields,
            vec!["remaining_percent", "used_percent", "reset_at"]
        );
    }

    #[test]
    fn playwright_sidecar_usage_response_rejects_unsanitized_fields() {
        let profile_path = "/home/dev/.local/share/com.pickforge.forgegauge/browser-profiles/codex";
        let plan = chromium_launch_plan(Service::Codex, profile_path);
        let launch_request = playwright_launch_request(&plan);
        let sidecar_request = playwright_sidecar_refresh_request(
            &launch_request,
            "https://chatgpt.com/codex/cloud/settings/analytics",
        )
        .expect("sidecar request is created");
        let raw = serde_json::json!({
            "ok": true,
            "status": "checked",
            "protocolVersion": 1,
            "action": "refreshUsage",
            "backend": "playwright-headed-chromium-sidecar",
            "service": "codex",
            "profileLabel": "codex-profile",
            "headless": true,
            "argCount": sidecar_request.args.len(),
            "pageState": "usage",
            "visibleFields": ["raw account email"]
        })
        .to_string();
        let error = playwright_sidecar_usage_response(&raw, &sidecar_request)
            .expect_err("unsanitized visible fields are rejected");

        assert_eq!(
            error,
            "Managed usage sidecar returned invalid visible fields"
        );
        assert!(!error.contains(profile_path));
        assert!(!error.contains("/home/dev"));
        assert!(!error.contains("email"));
    }

    #[test]
    fn playwright_sidecar_launch_response_rejects_mismatch_without_echoing_input() {
        let profile_path = "/home/dev/.local/share/com.pickforge.forgegauge/browser-profiles/codex";
        let plan = chromium_launch_plan(Service::Codex, profile_path);
        let launch_request = playwright_launch_request(&plan);
        let sidecar_request = playwright_sidecar_launch_request(
            &launch_request,
            "https://chatgpt.com/codex/cloud/settings/analytics",
        )
        .expect("sidecar request is created");
        let raw = serde_json::json!({
            "ok": true,
            "status": "launched",
            "protocolVersion": 1,
            "action": "launchLogin",
            "backend": "playwright-headed-chromium-sidecar",
            "service": "claude",
            "profileLabel": "claude-profile",
            "headless": false,
            "argCount": sidecar_request.args.len(),
            "userDataDir": profile_path
        })
        .to_string();
        let error = playwright_sidecar_launch_response(&raw, &sidecar_request)
            .expect_err("mismatched response is rejected");

        assert_eq!(error, "Managed login sidecar returned mismatched response");
        assert!(!error.contains(profile_path));
        assert!(!error.contains("/home/dev"));
    }

    #[test]
    fn playwright_sidecar_launch_response_rejects_failed_status_without_raw_code_details() {
        let plan = chromium_launch_plan(Service::Claude, "/tmp/forgegauge/claude");
        let launch_request = playwright_launch_request(&plan);
        let sidecar_request =
            playwright_sidecar_launch_request(&launch_request, "https://claude.ai/usage")
                .expect("sidecar request is created");
        let raw = serde_json::json!({
            "ok": false,
            "status": "rejected",
            "protocolVersion": 1,
            "code": "/tmp/forgegauge/claude"
        })
        .to_string();
        let error = playwright_sidecar_launch_response(&raw, &sidecar_request)
            .expect_err("rejected response is rejected");

        assert_eq!(error, "Managed login sidecar rejected launch");
        assert!(!error.contains("/tmp/forgegauge"));
    }

    #[test]
    fn prepare_chromium_profile_preferences_creates_disabled_storage_preferences() {
        let dir = TestDir::new();
        let profile_path = dir.path.join("codex");
        fs::create_dir_all(&profile_path).expect("profile dir is created");

        let preferences_path =
            prepare_chromium_profile_preferences(&profile_path).expect("preferences are prepared");
        let preferences = read_test_preferences(&preferences_path);

        assert_eq!(
            preferences_path,
            profile_path
                .join(CHROMIUM_DEFAULT_PROFILE_DIR)
                .join(CHROMIUM_PREFERENCES_FILE_NAME)
        );
        assert_preference_false(&preferences, &["credentials_enable_service"]);
        assert_preference_false(&preferences, &["credentials_enable_autosignin"]);
        assert_preference_false(&preferences, &["profile", "password_manager_enabled"]);
        assert_preference_false(
            &preferences,
            &["profile", "password_manager_allow_show_passwords"],
        );
        assert_preference_false(&preferences, &["autofill", "enabled"]);
        assert_preference_false(&preferences, &["autofill", "profile_enabled"]);
        assert_preference_false(&preferences, &["autofill", "credit_card_enabled"]);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let service_profile_mode = fs::metadata(&profile_path)
                .expect("service profile metadata")
                .permissions()
                .mode()
                & 0o777;
            let profile_mode = fs::metadata(preferences_path.parent().expect("parent exists"))
                .expect("default profile metadata")
                .permissions()
                .mode()
                & 0o777;
            let preferences_mode = fs::metadata(&preferences_path)
                .expect("preferences metadata")
                .permissions()
                .mode()
                & 0o777;

            assert_eq!(service_profile_mode, 0o700);
            assert_eq!(profile_mode, 0o700);
            assert_eq!(preferences_mode, 0o600);
        }
    }

    #[test]
    fn prepare_chromium_profile_preferences_merges_without_removing_existing_values() {
        let dir = TestDir::new();
        let profile_path = dir.path.join("claude");
        let default_profile_dir = profile_path.join(CHROMIUM_DEFAULT_PROFILE_DIR);
        fs::create_dir_all(&default_profile_dir).expect("default profile dir is created");
        let preferences_path = default_profile_dir.join(CHROMIUM_PREFERENCES_FILE_NAME);
        fs::write(
            &preferences_path,
            serde_json::json!({
                "browser": {
                    "window_placement": {
                        "left": 10
                    }
                },
                "autofill": {
                    "enabled": true,
                    "custom_key": "preserved"
                },
                "credentials_enable_service": true
            })
            .to_string(),
        )
        .expect("preferences are written");

        prepare_chromium_profile_preferences(&profile_path).expect("preferences are prepared");
        let preferences = read_test_preferences(&preferences_path);

        assert_eq!(preferences["browser"]["window_placement"]["left"], 10);
        assert_eq!(preferences["autofill"]["custom_key"], "preserved");
        assert_preference_false(&preferences, &["autofill", "enabled"]);
        assert_preference_false(&preferences, &["autofill", "profile_enabled"]);
        assert_preference_false(&preferences, &["credentials_enable_service"]);
    }

    #[test]
    fn prepare_chromium_profile_preferences_rejects_malformed_preferences() {
        let dir = TestDir::new();
        let profile_path = dir.path.join("codex");
        let default_profile_dir = profile_path.join(CHROMIUM_DEFAULT_PROFILE_DIR);
        fs::create_dir_all(&default_profile_dir).expect("default profile dir is created");
        let preferences_path = default_profile_dir.join(CHROMIUM_PREFERENCES_FILE_NAME);
        fs::write(&preferences_path, "{not json").expect("malformed preferences are written");

        let error = prepare_chromium_profile_preferences(&profile_path)
            .expect_err("malformed preferences are rejected");

        assert_eq!(error, "Could not parse managed browser preferences");
        assert!(!error.contains(profile_path.to_string_lossy().as_ref()));
        assert!(!error.contains("{not json"));
    }

    #[cfg(unix)]
    #[test]
    fn prepare_chromium_profile_preferences_rejects_symlinked_preferences() {
        use std::os::unix::fs::symlink;

        let dir = TestDir::new();
        let profile_path = dir.path.join("claude");
        let default_profile_dir = profile_path.join(CHROMIUM_DEFAULT_PROFILE_DIR);
        fs::create_dir_all(&default_profile_dir).expect("default profile dir is created");
        let target = dir.path.join("target-preferences");
        fs::write(&target, "{}").expect("target preferences are written");
        symlink(
            &target,
            default_profile_dir.join(CHROMIUM_PREFERENCES_FILE_NAME),
        )
        .expect("preferences symlink is created");

        let error = prepare_chromium_profile_preferences(&profile_path)
            .expect_err("symlinked preferences are rejected");

        assert_eq!(error, "Managed browser path must not be a symlink");
    }

    #[test]
    fn inspect_chromium_profile_storage_returns_empty_for_missing_profile() {
        let dir = TestDir::new();

        let inspection = inspect_chromium_profile_storage(&dir.path.join("missing"))
            .expect("missing profile is inspectable");

        assert_eq!(
            inspection,
            BrowserProfileStorageInspection {
                credential_store_files: 0,
                autofill_store_files: 0,
                cookie_store_files: 0,
                site_storage_entries: 0,
                symlink_entries: 0,
                password_saving_enabled: false,
                autofill_enabled: false,
                inspected_entries: 0,
                entry_limit_reached: false,
            }
        );
    }

    #[test]
    fn inspect_chromium_profile_storage_reports_disabled_prepared_profile() {
        let dir = TestDir::new();
        let profile_path = dir.path.join("codex");
        prepare_chromium_profile_preferences(&profile_path).expect("preferences are prepared");

        let inspection =
            inspect_chromium_profile_storage(&profile_path).expect("profile is inspected");
        let debug = format!("{inspection:?}");

        assert_eq!(inspection.credential_store_files, 0);
        assert_eq!(inspection.autofill_store_files, 0);
        assert_eq!(inspection.cookie_store_files, 0);
        assert_eq!(inspection.site_storage_entries, 0);
        assert_eq!(inspection.symlink_entries, 0);
        assert!(!inspection.password_saving_enabled);
        assert!(!inspection.autofill_enabled);
        assert!(!inspection.entry_limit_reached);
        assert!(!debug.contains(profile_path.to_string_lossy().as_ref()));
    }

    #[test]
    fn inspect_chromium_profile_storage_counts_credential_store_files() {
        let dir = TestDir::new();
        let profile_path = dir.path.join("claude");
        let default_profile_dir = profile_path.join(CHROMIUM_DEFAULT_PROFILE_DIR);
        fs::create_dir_all(&default_profile_dir).expect("default profile dir is created");
        fs::write(
            default_profile_dir.join("Login Data"),
            "database placeholder",
        )
        .expect("login data marker is written");
        fs::write(
            default_profile_dir.join("Login Data-journal"),
            "journal placeholder",
        )
        .expect("login data journal marker is written");

        let inspection =
            inspect_chromium_profile_storage(&profile_path).expect("profile is inspected");

        assert_eq!(inspection.credential_store_files, 2);
        assert_eq!(inspection.autofill_store_files, 0);
        assert_eq!(inspection.cookie_store_files, 0);
        assert_eq!(inspection.site_storage_entries, 0);
    }

    #[test]
    fn inspect_chromium_profile_storage_counts_autofill_store_files() {
        let dir = TestDir::new();
        let profile_path = dir.path.join("codex");
        let default_profile_dir = profile_path.join(CHROMIUM_DEFAULT_PROFILE_DIR);
        fs::create_dir_all(&default_profile_dir).expect("default profile dir is created");
        fs::write(default_profile_dir.join("Web Data"), "database placeholder")
            .expect("web data marker is written");
        fs::write(
            default_profile_dir.join("Web Data-journal"),
            "journal placeholder",
        )
        .expect("web data journal marker is written");
        fs::write(
            default_profile_dir.join("Web Database"),
            "non-matching placeholder",
        )
        .expect("non-matching marker is written");

        let inspection =
            inspect_chromium_profile_storage(&profile_path).expect("profile is inspected");

        assert_eq!(inspection.autofill_store_files, 2);
        assert_eq!(inspection.credential_store_files, 0);
        assert_eq!(inspection.cookie_store_files, 0);
        assert_eq!(inspection.site_storage_entries, 0);
    }

    #[test]
    fn inspect_chromium_profile_storage_counts_cookie_store_files() {
        let dir = TestDir::new();
        let profile_path = dir.path.join("codex");
        let network_dir = profile_path
            .join(CHROMIUM_DEFAULT_PROFILE_DIR)
            .join("Network");
        fs::create_dir_all(&network_dir).expect("network dir is created");
        fs::write(network_dir.join("Cookies"), "database placeholder")
            .expect("cookies marker is written");
        fs::write(network_dir.join("Cookies-wal"), "wal placeholder")
            .expect("cookies wal marker is written");
        fs::write(
            network_dir.join("Cookie Controls"),
            "non-matching placeholder",
        )
        .expect("non-matching marker is written");

        let inspection =
            inspect_chromium_profile_storage(&profile_path).expect("profile is inspected");

        assert_eq!(inspection.cookie_store_files, 2);
        assert_eq!(inspection.credential_store_files, 0);
        assert_eq!(inspection.autofill_store_files, 0);
    }

    #[test]
    fn inspect_chromium_profile_storage_counts_site_storage_entries() {
        let dir = TestDir::new();
        let profile_path = dir.path.join("claude");
        let default_profile_dir = profile_path.join(CHROMIUM_DEFAULT_PROFILE_DIR);
        fs::create_dir_all(default_profile_dir.join("Local Storage"))
            .expect("local storage dir is created");
        fs::create_dir_all(default_profile_dir.join("Session Storage"))
            .expect("session storage dir is created");
        fs::create_dir_all(default_profile_dir.join("IndexedDB"))
            .expect("indexeddb dir is created");
        fs::create_dir_all(default_profile_dir.join("Service Worker"))
            .expect("service worker dir is created");
        fs::write(
            default_profile_dir.join("Storage Notes"),
            "non-matching placeholder",
        )
        .expect("non-matching marker is written");

        let inspection =
            inspect_chromium_profile_storage(&profile_path).expect("profile is inspected");

        assert_eq!(inspection.site_storage_entries, 4);
        assert_eq!(inspection.cookie_store_files, 0);
        assert_eq!(inspection.credential_store_files, 0);
        assert_eq!(inspection.autofill_store_files, 0);
    }

    #[test]
    fn inspect_chromium_profile_storage_reports_enabled_preferences() {
        let dir = TestDir::new();
        let profile_path = dir.path.join("codex");
        let default_profile_dir = profile_path.join(CHROMIUM_DEFAULT_PROFILE_DIR);
        fs::create_dir_all(&default_profile_dir).expect("default profile dir is created");
        fs::write(
            default_profile_dir.join(CHROMIUM_PREFERENCES_FILE_NAME),
            serde_json::json!({
                "autofill": {
                    "enabled": true
                },
                "credentials_enable_service": true
            })
            .to_string(),
        )
        .expect("preferences are written");

        let inspection =
            inspect_chromium_profile_storage(&profile_path).expect("profile is inspected");

        assert!(inspection.password_saving_enabled);
        assert!(inspection.autofill_enabled);
    }

    #[test]
    fn inspect_chromium_profile_storage_rejects_malformed_preferences_without_leaking_content() {
        let dir = TestDir::new();
        let profile_path = dir.path.join("claude");
        let default_profile_dir = profile_path.join(CHROMIUM_DEFAULT_PROFILE_DIR);
        fs::create_dir_all(&default_profile_dir).expect("default profile dir is created");
        fs::write(
            default_profile_dir.join(CHROMIUM_PREFERENCES_FILE_NAME),
            "{secret token}",
        )
        .expect("malformed preferences are written");

        let error = inspect_chromium_profile_storage(&profile_path)
            .expect_err("malformed preferences are rejected");

        assert_eq!(error, "Could not parse managed browser preferences");
        assert!(!error.contains(profile_path.to_string_lossy().as_ref()));
        assert!(!error.contains("secret"));
        assert!(!error.contains("token"));
    }

    #[cfg(unix)]
    #[test]
    fn inspect_chromium_profile_storage_counts_symlinks_without_following_them() {
        use std::os::unix::fs::symlink;

        let dir = TestDir::new();
        let profile_path = dir.path.join("codex");
        fs::create_dir_all(&profile_path).expect("profile dir is created");
        let target = dir.path.join("target");
        fs::write(&target, "target").expect("target is written");
        symlink(&target, profile_path.join("Login Data")).expect("symlink is created");

        let inspection =
            inspect_chromium_profile_storage(&profile_path).expect("profile is inspected");

        assert_eq!(inspection.symlink_entries, 1);
        assert_eq!(inspection.credential_store_files, 0);
        assert_eq!(inspection.autofill_store_files, 0);
        assert_eq!(inspection.cookie_store_files, 0);
        assert_eq!(inspection.site_storage_entries, 0);
    }

    fn read_test_preferences(path: &Path) -> Value {
        let raw = fs::read_to_string(path).expect("preferences are readable");
        serde_json::from_str(&raw).expect("preferences parse")
    }

    fn assert_preference_false(preferences: &Value, path: &[&str]) {
        let value = path
            .iter()
            .fold(preferences, |value, segment| &value[*segment]);

        assert_eq!(value.as_bool(), Some(false), "{path:?} should be false");
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
