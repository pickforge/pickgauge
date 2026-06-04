mod browser_profile;
mod browser_session;
mod config;
pub mod local_provider;
pub mod usage;
pub mod web_provider;

use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdout},
    sync::{mpsc, Mutex},
    thread,
    time::Duration,
};
use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};
use usage::{
    Service, UsageDisplayState, UsageEngine, UsageProviderError, UsageProviderErrorEvent,
    UsageRefreshEvent, UsageRefreshStatus, UsageSnapshot, UsageSource,
};
use web_provider::{VisiblePageState, VisibleUsageInput};

use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_shell::ShellExt;

const SETTINGS_UPDATED_EVENT: &str = "settings://updated";
const LOGIN_REQUIRED_EVENT: &str = "login://required";
const SESSION_RESET_EVENT: &str = "session://reset";
const LOGIN_STATUS_REQUIRED: &str = "login_required";
const LOGIN_STATUS_LAUNCHED: &str = "launched";
const LOGIN_STATUS_ALREADY_AUTHENTICATED: &str = "already_authenticated";
const LOGIN_STATUS_PREFLIGHT_UNAVAILABLE: &str = "preflight_unavailable";
const LOGIN_REASON_MANAGED_LOGIN_NOT_AVAILABLE: &str = "managed_login_not_available";
const LOGIN_REASON_SIDECAR_UNAVAILABLE: &str = "sidecar_unavailable";
const CODEX_USAGE_URL: &str = "https://chatgpt.com/codex/cloud/settings/analytics";
const CLAUDE_USAGE_URL: &str = "https://claude.ai/new#settings/usage";
const PLAYWRIGHT_SIDECAR_NAME: &str = "forgegauge-playwright-sidecar";
const PLAYWRIGHT_SIDECAR_ACK_TIMEOUT: Duration = Duration::from_secs(15);
const LOG_FILE_NAME: &str = "forgegauge.log";
const LOG_REDACTION_POLICY_PATH: &str = "docs/security/log-redaction-policy.md";
const TRAY_UNKNOWN: &[u8] = include_bytes!("../../assets/branding/tray-unknown-64.png");
const TRAY_ICON_SIZE: u32 = 64;
const TRAY_ICON_CENTER: f32 = 32.0;
const TRAY_ICON_OUTER_RADIUS: f32 = 30.0;
const TRAY_ICON_INNER_RADIUS: f32 = 21.0;
const TRAY_CODEX_ACCENT: [u8; 4] = [37, 99, 235, 255];
const TRAY_CLAUDE_ACCENT: [u8; 4] = [199, 91, 18, 255];
const TRAY_LOW_ACCENT: [u8; 4] = [192, 38, 38, 255];
const TRAY_TRACK: [u8; 4] = [100, 112, 132, 112];
const TRAY_SURFACE: [u8; 4] = [246, 247, 251, 255];
const TRAY_TRANSPARENT: [u8; 4] = [0, 0, 0, 0];
const POPUP_ANCHOR_GAP: i32 = 10;

#[derive(Clone, Copy)]
enum StartupWarning {
    AutostartSync,
    BrowserSessionRecovery,
    InitialUsageRefresh,
}

struct ConfigLoadState {
    error: Mutex<Option<String>>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CommandError {
    code: String,
    message: String,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct OfficialUsagePage {
    service: Service,
    url: String,
    opened_at: String,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderLoginStart {
    service: Service,
    url: String,
    status: String,
    backend: String,
    profile_label: String,
    profile_prepared: bool,
    started_at: String,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LoginRequiredEvent {
    service: Service,
    url: String,
    reason: String,
    emitted_at: String,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ClearedProviderProfile {
    service: Service,
    cleared: bool,
    cleared_at: String,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderProfileInspection {
    service: Service,
    profile_label: String,
    profile_prepared: bool,
    credential_store_files: usize,
    autofill_store_files: usize,
    cookie_store_files: usize,
    site_storage_entries: usize,
    symlink_entries: usize,
    password_saving_enabled: bool,
    autofill_enabled: bool,
    inspected_entries: usize,
    entry_limit_reached: bool,
    inspected_at: String,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LogLocation {
    path: String,
    exists: bool,
    redaction_policy: String,
    updated_at: String,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct WindowVisibility {
    status: String,
    updated_at: String,
}

struct ProviderLoginStartPlan {
    login: ProviderLoginStart,
    sidecar_request: Option<browser_session::PlaywrightSidecarLaunchRequest>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LoginPreflightDecision {
    AlreadyAuthenticated,
    LaunchBrowser,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LoginStartPreflightOutcome {
    status: &'static str,
    launch_headed_browser: bool,
}

type CommandResult<T> = Result<T, CommandError>;

impl CommandError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

fn command_error(code: &'static str, message: &'static str) -> CommandError {
    CommandError::new(code, message)
}

fn map_config_state_error(_: String) -> CommandError {
    command_error(
        "config_state_unavailable",
        "Configuration state is unavailable",
    )
}

fn map_recovered_config_error(_: String) -> CommandError {
    command_error(
        "config_recovered_with_defaults",
        "Recovered with default settings after config load failed",
    )
}

fn map_app_data_dir_error(_: tauri::Error) -> CommandError {
    command_error(
        "app_data_dir_unavailable",
        "App data directory is unavailable",
    )
}

fn map_browser_profile_error(error: String) -> CommandError {
    let message = if error.starts_with("Browser profile ") {
        error
    } else {
        "Could not update browser profiles".to_string()
    };

    CommandError::new("browser_profile_unavailable", message)
}

fn map_browser_session_error(_: String) -> CommandError {
    command_error(
        "browser_session_unavailable",
        "Could not stop managed browser session",
    )
}

fn map_browser_profile_inspection_error(_: String) -> CommandError {
    command_error(
        "browser_profile_inspection_unavailable",
        "Could not inspect browser profile",
    )
}

fn map_config_save_error(_: String) -> CommandError {
    command_error("config_save_failed", "Could not save app settings")
}

fn map_usage_state_error(_: String) -> CommandError {
    command_error("usage_state_unavailable", "Usage state is unavailable")
}

fn map_snapshot_cache_error(_: String) -> CommandError {
    command_error(
        "snapshot_cache_unavailable",
        "Could not clear cached usage snapshots",
    )
}

fn map_event_emit_error(_: tauri::Error) -> CommandError {
    command_error("event_emit_failed", "Could not emit app event")
}

fn map_usage_refresh_error(_: String) -> CommandError {
    command_error("usage_refresh_failed", "Could not refresh usage state")
}

fn map_provider_refresh_error(error: String) -> CommandError {
    let message = match error.as_str() {
        "Provider source cannot be refreshed directly"
        | "Provider is not configured"
        | "Web providers are disabled"
        | "Manual web refresh is cooling down" => error,
        _ => "Could not refresh provider".to_string(),
    };

    CommandError::new("provider_refresh_failed", message)
}

fn map_log_location_error() -> CommandError {
    command_error("log_location_unavailable", "Log location is unavailable")
}

fn map_open_usage_page_error() -> CommandError {
    command_error(
        "open_usage_page_failed",
        "Could not open official usage page",
    )
}

fn map_window_visibility_error() -> CommandError {
    command_error("window_visibility_failed", "Could not update app window")
}

fn map_autostart_error() -> CommandError {
    command_error("autostart_update_failed", "Could not update autostart")
}

fn startup_warning_message(warning: StartupWarning) -> &'static str {
    match warning {
        StartupWarning::AutostartSync => "ForgeGauge startup warning: autostart sync failed",
        StartupWarning::BrowserSessionRecovery => {
            "ForgeGauge startup warning: managed browser recovery failed"
        }
        StartupWarning::InitialUsageRefresh => {
            "ForgeGauge startup warning: initial usage refresh failed"
        }
    }
}

fn log_startup_warning(warning: StartupWarning) {
    eprintln!("{}", startup_warning_message(warning));
}

fn sync_autostart(app: &AppHandle, enabled: bool) -> CommandResult<()> {
    let manager = app.autolaunch();
    let current = manager.is_enabled().map_err(|_| map_autostart_error())?;

    match (enabled, current) {
        (true, false) => manager.enable().map_err(|_| map_autostart_error()),
        (false, true) => manager.disable().map_err(|_| map_autostart_error()),
        _ => Ok(()),
    }
}

fn official_usage_url(service: Service) -> &'static str {
    match service {
        Service::Codex => CODEX_USAGE_URL,
        Service::Claude => CLAUDE_USAGE_URL,
    }
}

fn browser_profile_service(service: Service) -> browser_profile::BrowserProfileService {
    match service {
        Service::Codex => browser_profile::BrowserProfileService::Codex,
        Service::Claude => browser_profile::BrowserProfileService::Claude,
    }
}

fn prepare_log_dir(path: &Path) -> CommandResult<()> {
    fs::create_dir_all(path).map_err(|_| map_log_location_error())?;
    set_restrictive_log_dir_permissions(path).map_err(|_| map_log_location_error())
}

#[cfg(unix)]
fn set_restrictive_log_dir_permissions(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn set_restrictive_log_dir_permissions(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

impl ConfigLoadState {
    fn new(error: Option<String>) -> Self {
        Self {
            error: Mutex::new(error),
        }
    }

    fn current_error(&self) -> Result<Option<String>, String> {
        self.error
            .lock()
            .map(|error| error.clone())
            .map_err(|_| "Config load state lock was poisoned".to_string())
    }

    fn clear_error(&self) -> Result<(), String> {
        self.error
            .lock()
            .map(|mut error| {
                *error = None;
            })
            .map_err(|_| "Config load state lock was poisoned".to_string())
    }
}

#[tauri::command]
fn get_app_config(
    engine: State<'_, UsageEngine>,
    config_load: State<'_, ConfigLoadState>,
) -> CommandResult<config::AppConfig> {
    if let Some(error) = config_load
        .current_error()
        .map_err(map_config_state_error)?
    {
        return Err(map_recovered_config_error(error));
    }

    engine.config().map_err(map_usage_state_error)
}

#[tauri::command]
fn update_app_config(
    app: AppHandle,
    engine: State<'_, UsageEngine>,
    config_load: State<'_, ConfigLoadState>,
    config: config::AppConfig,
) -> CommandResult<config::AppConfig> {
    let previous_config = engine.config().map_err(map_usage_state_error)?;
    let config = config.normalized();

    if browser_profile::should_prepare_browser_profiles(
        &config.browser_profiles,
        config.providers.web_enabled,
    ) {
        let app_data_dir = app.path().app_data_dir().map_err(map_app_data_dir_error)?;
        prepare_managed_browser_profiles(&config, &app_data_dir)
            .map_err(map_browser_profile_error)?;
    }

    let autostart_changed = previous_config.autostart.enabled != config.autostart.enabled;
    if autostart_changed {
        sync_autostart(&app, config.autostart.enabled)?;
    }

    let config = match config::save(&app, &config) {
        Ok(config) => config,
        Err(error) => {
            if autostart_changed {
                let _ = sync_autostart(&app, previous_config.autostart.enabled);
            }
            return Err(map_config_save_error(error));
        }
    };

    config_load.clear_error().map_err(map_config_state_error)?;
    engine
        .update_config(config.clone())
        .map_err(map_usage_state_error)?;
    app.emit(SETTINGS_UPDATED_EVENT, &config)
        .map_err(map_event_emit_error)?;
    engine
        .refresh_all_and_emit(&app)
        .map_err(map_usage_refresh_error)?;
    Ok(config)
}

fn prepare_managed_browser_profiles(
    config: &config::AppConfig,
    app_data_dir: &Path,
) -> Result<Option<browser_profile::BrowserProfilePaths>, String> {
    if !browser_profile::should_prepare_browser_profiles(
        &config.browser_profiles,
        config.providers.web_enabled,
    ) {
        return Ok(None);
    }

    let paths = browser_profile::prepare_browser_profiles(&config.browser_profiles, app_data_dir)?;
    browser_session::prepare_chromium_profile_preferences(&paths.codex)?;
    browser_session::prepare_chromium_profile_preferences(&paths.claude)?;

    Ok(Some(paths))
}

#[tauri::command]
fn get_usage_snapshots(engine: State<'_, UsageEngine>) -> CommandResult<Vec<UsageSnapshot>> {
    engine.snapshots().map_err(map_usage_state_error)
}

#[tauri::command]
fn get_display_state(engine: State<'_, UsageEngine>) -> CommandResult<UsageDisplayState> {
    engine.display_state().map_err(map_usage_state_error)
}

#[tauri::command]
fn get_log_location(app: AppHandle) -> CommandResult<LogLocation> {
    let log_dir = app
        .path()
        .app_log_dir()
        .map_err(|_| map_log_location_error())?;
    prepare_log_dir(&log_dir)?;
    let path = log_dir.join(LOG_FILE_NAME);

    Ok(LogLocation {
        exists: path.is_file(),
        path: path.to_string_lossy().to_string(),
        redaction_policy: LOG_REDACTION_POLICY_PATH.to_string(),
        updated_at: usage::now_rfc3339(),
    })
}

#[tauri::command]
fn open_official_usage_page(app: AppHandle, service: Service) -> CommandResult<OfficialUsagePage> {
    let url = official_usage_url(service);

    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|_| map_open_usage_page_error())?;

    Ok(OfficialUsagePage {
        service,
        url: url.to_string(),
        opened_at: usage::now_rfc3339(),
    })
}

#[tauri::command]
fn start_provider_login(
    app: AppHandle,
    engine: State<'_, UsageEngine>,
    sessions: State<'_, browser_session::BrowserSessionManager>,
    service: Service,
) -> CommandResult<ProviderLoginStart> {
    let config = engine.config().map_err(map_usage_state_error)?;
    let app_data_dir = app.path().app_data_dir().map_err(map_app_data_dir_error)?;
    let now = usage::now_rfc3339();
    let plan = provider_login_start_plan(&config, &app_data_dir, service, now.clone())
        .map_err(map_browser_profile_error)?;
    let mut login = plan.login;
    let login_required_reason = if let Some(request) = plan.sidecar_request {
        let preflight_outcome = match headless_web_usage_response(&app, &engine, &sessions, service)
        {
            Ok(response) => {
                let page_state = response.page_state.clone();
                refresh_web_provider_preflight_response(&app, &engine, service, response)?;
                login_start_preflight_outcome(Some(page_state.as_str()))
            }
            Err(_) => login_start_preflight_outcome(None),
        };

        login.status = preflight_outcome.status.to_string();

        if preflight_outcome.launch_headed_browser {
            match launch_playwright_sidecar_login(&app, &sessions, &request) {
                Ok(_) => {
                    login.status = LOGIN_STATUS_LAUNCHED.to_string();
                    None
                }
                Err(_) => Some(LOGIN_REASON_SIDECAR_UNAVAILABLE),
            }
        } else {
            None
        }
    } else {
        Some(LOGIN_REASON_MANAGED_LOGIN_NOT_AVAILABLE)
    };

    if let Some(reason) = login_required_reason {
        let event = LoginRequiredEvent {
            service,
            url: login.url.clone(),
            reason: reason.to_string(),
            emitted_at: now,
        };

        app.emit(LOGIN_REQUIRED_EVENT, &event)
            .map_err(map_event_emit_error)?;
    }

    Ok(login)
}

#[cfg(test)]
fn provider_login_start_report(
    config: &config::AppConfig,
    app_data_dir: &Path,
    service: Service,
    started_at: String,
) -> Result<ProviderLoginStart, String> {
    provider_login_start_plan(config, app_data_dir, service, started_at).map(|plan| plan.login)
}

fn provider_login_start_plan(
    config: &config::AppConfig,
    app_data_dir: &Path,
    service: Service,
    started_at: String,
) -> Result<ProviderLoginStartPlan, String> {
    let paths = prepare_managed_browser_profiles(config, app_data_dir)?;
    let launch_request = paths.as_ref().map(|paths| {
        let profile_path = match service {
            Service::Codex => &paths.codex,
            Service::Claude => &paths.claude,
        };
        let launch_plan = browser_session::chromium_launch_plan(service, profile_path);
        browser_session::playwright_launch_request(&launch_plan)
    });

    let sidecar_request = launch_request
        .as_ref()
        .map(|request| {
            browser_session::playwright_sidecar_launch_request(request, official_usage_url(service))
        })
        .transpose()?;
    let profile_prepared = launch_request.is_some();
    let profile_label = launch_request
        .as_ref()
        .map(|request| request.profile_label.clone())
        .unwrap_or_else(|| provider_profile_label(service).to_string());

    Ok(ProviderLoginStartPlan {
        login: ProviderLoginStart {
            service,
            url: official_usage_url(service).to_string(),
            status: LOGIN_STATUS_REQUIRED.to_string(),
            backend: browser_session::PLAYWRIGHT_BACKEND_ID.to_string(),
            profile_label,
            profile_prepared,
            started_at,
        },
        sidecar_request,
    })
}

fn should_launch_login_after_preflight(page_state: &str) -> bool {
    matches!(
        page_state,
        "logged_out" | "mfa_required" | "captcha_or_bot_check"
    )
}

fn login_preflight_decision(page_state: Option<&str>) -> LoginPreflightDecision {
    match page_state {
        Some("usage") => LoginPreflightDecision::AlreadyAuthenticated,
        Some(page_state) if should_launch_login_after_preflight(page_state) => {
            LoginPreflightDecision::LaunchBrowser
        }
        _ => LoginPreflightDecision::Unavailable,
    }
}

fn login_start_preflight_outcome(page_state: Option<&str>) -> LoginStartPreflightOutcome {
    match login_preflight_decision(page_state) {
        LoginPreflightDecision::AlreadyAuthenticated => LoginStartPreflightOutcome {
            status: LOGIN_STATUS_ALREADY_AUTHENTICATED,
            launch_headed_browser: false,
        },
        LoginPreflightDecision::LaunchBrowser => LoginStartPreflightOutcome {
            status: LOGIN_STATUS_REQUIRED,
            launch_headed_browser: true,
        },
        LoginPreflightDecision::Unavailable => LoginStartPreflightOutcome {
            status: LOGIN_STATUS_PREFLIGHT_UNAVAILABLE,
            launch_headed_browser: false,
        },
    }
}

fn launch_playwright_sidecar_login(
    app: &AppHandle,
    sessions: &browser_session::BrowserSessionManager,
    request: &browser_session::PlaywrightSidecarLaunchRequest,
) -> Result<u32, String> {
    sessions.stop_service(request.service, browser_session::PROFILE_STOP_TIMEOUT)?;

    let marker = browser_session::BrowserSessionMarker::new(request.service);
    let mut command: std::process::Command = app
        .shell()
        .sidecar(PLAYWRIGHT_SIDECAR_NAME)
        .map_err(|_| "Managed login sidecar is unavailable".to_string())?
        .into();
    let (env_key, env_value) = marker.env_pair();
    command.env(env_key, env_value);

    let mut child = command
        .spawn()
        .map_err(|_| "Managed login sidecar is unavailable".to_string())?;
    if let Err(error) = write_sidecar_launch_request(&mut child, request) {
        return Err(kill_untracked_child(child, &error));
    }
    let Some(stdout) = child.stdout.take() else {
        return Err(kill_untracked_child(
            child,
            "Managed login sidecar did not acknowledge launch",
        ));
    };
    let line = match read_sidecar_stdout_line(stdout, PLAYWRIGHT_SIDECAR_ACK_TIMEOUT) {
        Ok(line) => line,
        Err(error) => return Err(kill_untracked_child(child, &error)),
    };

    if let Err(error) = browser_session::playwright_sidecar_launch_response(&line, request) {
        return Err(kill_untracked_child(child, &error));
    }

    sessions.track_process(request.service, child, marker)
}

fn write_sidecar_launch_request(
    child: &mut Child,
    request: &browser_session::PlaywrightSidecarLaunchRequest,
) -> Result<(), String> {
    let raw = serde_json::to_vec(request)
        .map_err(|_| "Could not serialize managed login sidecar request".to_string())?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Managed login sidecar is unavailable".to_string())?;
    stdin
        .write_all(&raw)
        .and_then(|_| stdin.write_all(b"\n"))
        .map_err(|_| "Could not write managed login sidecar request".to_string())
}

fn read_sidecar_stdout_line(stdout: ChildStdout, timeout: Duration) -> Result<String, String> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let result = reader
            .read_line(&mut line)
            .map(|bytes| if bytes == 0 { String::new() } else { line })
            .map_err(|_| "Could not read managed login sidecar response".to_string());
        let _ = sender.send(result);
    });

    match receiver.recv_timeout(timeout) {
        Ok(Ok(line)) if !line.trim().is_empty() => Ok(line),
        Ok(Ok(_)) => Err("Managed login sidecar did not acknowledge launch".to_string()),
        Ok(Err(error)) => Err(error),
        Err(_) => Err("Managed login sidecar did not acknowledge launch".to_string()),
    }
}

fn kill_untracked_child(mut child: Child, error: &str) -> String {
    let _ = child.kill();
    let _ = child.wait();
    error.to_string()
}

#[tauri::command]
fn clear_cached_snapshots(
    app: AppHandle,
    engine: State<'_, UsageEngine>,
) -> CommandResult<UsageDisplayState> {
    let display_state = engine
        .clear_cached_snapshots()
        .map_err(map_snapshot_cache_error)?;
    app.emit(usage::SNAPSHOTS_UPDATED_EVENT, &display_state)
        .map_err(map_event_emit_error)?;
    Ok(display_state)
}

#[tauri::command]
fn clear_provider_profile(
    app: AppHandle,
    engine: State<'_, UsageEngine>,
    sessions: State<'_, browser_session::BrowserSessionManager>,
    service: Service,
) -> CommandResult<ClearedProviderProfile> {
    clear_provider_profile_for_service(&app, &engine, &sessions, service)
}

#[tauri::command]
fn reset_provider_session(
    app: AppHandle,
    engine: State<'_, UsageEngine>,
    sessions: State<'_, browser_session::BrowserSessionManager>,
    service: Service,
) -> CommandResult<ClearedProviderProfile> {
    let reset = clear_provider_profile_for_service(&app, &engine, &sessions, service)?;
    app.emit(SESSION_RESET_EVENT, &reset)
        .map_err(map_event_emit_error)?;
    Ok(reset)
}

#[tauri::command]
fn inspect_provider_profile(
    app: AppHandle,
    engine: State<'_, UsageEngine>,
    service: Service,
) -> CommandResult<ProviderProfileInspection> {
    let config = engine.config().map_err(map_usage_state_error)?;
    let app_data_dir = app.path().app_data_dir().map_err(map_app_data_dir_error)?;
    inspect_provider_profile_for_service(&config, &app_data_dir, service, usage::now_rfc3339())
        .map_err(map_browser_profile_inspection_error)
}

fn inspect_provider_profile_for_service(
    config: &config::AppConfig,
    app_data_dir: &Path,
    service: Service,
    inspected_at: String,
) -> Result<ProviderProfileInspection, String> {
    let Some(paths) = prepare_managed_browser_profiles(config, app_data_dir)? else {
        return Ok(provider_profile_inspection_report(
            service,
            false,
            None,
            inspected_at,
        ));
    };

    let profile_path = match service {
        Service::Codex => &paths.codex,
        Service::Claude => &paths.claude,
    };
    let inspection = browser_session::inspect_chromium_profile_storage(profile_path)?;

    Ok(provider_profile_inspection_report(
        service,
        true,
        Some(inspection),
        inspected_at,
    ))
}

fn provider_profile_inspection_report(
    service: Service,
    profile_prepared: bool,
    inspection: Option<browser_session::BrowserProfileStorageInspection>,
    inspected_at: String,
) -> ProviderProfileInspection {
    let inspection = inspection.unwrap_or(browser_session::BrowserProfileStorageInspection {
        credential_store_files: 0,
        autofill_store_files: 0,
        cookie_store_files: 0,
        site_storage_entries: 0,
        symlink_entries: 0,
        password_saving_enabled: false,
        autofill_enabled: false,
        inspected_entries: 0,
        entry_limit_reached: false,
    });

    ProviderProfileInspection {
        service,
        profile_label: provider_profile_label(service).to_string(),
        profile_prepared,
        credential_store_files: inspection.credential_store_files,
        autofill_store_files: inspection.autofill_store_files,
        cookie_store_files: inspection.cookie_store_files,
        site_storage_entries: inspection.site_storage_entries,
        symlink_entries: inspection.symlink_entries,
        password_saving_enabled: inspection.password_saving_enabled,
        autofill_enabled: inspection.autofill_enabled,
        inspected_entries: inspection.inspected_entries,
        entry_limit_reached: inspection.entry_limit_reached,
        inspected_at,
    }
}

fn provider_profile_label(service: Service) -> &'static str {
    match service {
        Service::Codex => "codex-profile",
        Service::Claude => "claude-profile",
    }
}

fn clear_provider_profile_for_service(
    app: &AppHandle,
    engine: &UsageEngine,
    sessions: &browser_session::BrowserSessionManager,
    service: Service,
) -> CommandResult<ClearedProviderProfile> {
    sessions
        .stop_service(service, browser_session::PROFILE_STOP_TIMEOUT)
        .map_err(map_browser_session_error)?;

    let config = engine.config().map_err(map_usage_state_error)?;
    let app_data_dir = app.path().app_data_dir().map_err(map_app_data_dir_error)?;
    let cleared = browser_profile::clear_browser_profile(
        &config.browser_profiles,
        &app_data_dir,
        browser_profile_service(service),
    )
    .map_err(map_browser_profile_error)?;

    Ok(ClearedProviderProfile {
        service,
        cleared,
        cleared_at: usage::now_rfc3339(),
    })
}

fn emit_refresh_event(
    app: &AppHandle,
    service: Option<Service>,
    source: Option<UsageSource>,
    event_name: &str,
    status: UsageRefreshStatus,
) -> CommandResult<()> {
    let event = UsageRefreshEvent::new(service, source, status, usage::now_rfc3339());
    app.emit(event_name, &event).map_err(map_event_emit_error)
}

fn emit_provider_error_events(app: &AppHandle, display_state: &UsageDisplayState) {
    for snapshot in &display_state.snapshots {
        let Some(status) = snapshot
            .details
            .get("status")
            .and_then(|value| value.as_str())
        else {
            continue;
        };

        if matches!(status, "parsed" | "placeholder") {
            continue;
        }

        let provider_id = snapshot
            .details
            .get("providerId")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        let event = UsageProviderErrorEvent::new(
            snapshot.service,
            snapshot.source,
            provider_id,
            status,
            usage::now_rfc3339(),
        );
        let _ = app.emit(usage::PROVIDER_ERROR_EVENT, event);
    }
}

fn refresh_web_provider_headless(
    app: &AppHandle,
    engine: &UsageEngine,
    sessions: &browser_session::BrowserSessionManager,
    service: Service,
) -> CommandResult<UsageDisplayState> {
    engine
        .refresh_provider_source_with_snapshot(service, UsageSource::Web, |observed_at| {
            let response = headless_web_usage_response(app, engine, sessions, service)
                .map_err(|_| UsageProviderError::Internal)?;

            usage_snapshot_from_sidecar_usage_response(response, observed_at)
        })
        .map_err(map_provider_refresh_error)
}

fn refresh_due_web_provider_headless(
    app: &AppHandle,
    engine: &UsageEngine,
    sessions: &browser_session::BrowserSessionManager,
    service: Service,
) -> CommandResult<UsageDisplayState> {
    engine
        .refresh_due_provider_source_with_snapshot(service, UsageSource::Web, |observed_at| {
            let response = headless_web_usage_response(app, engine, sessions, service)
                .map_err(|_| UsageProviderError::Internal)?;

            usage_snapshot_from_sidecar_usage_response(response, observed_at)
        })
        .map_err(map_provider_refresh_error)
}

fn refresh_web_provider_preflight_response(
    app: &AppHandle,
    engine: &UsageEngine,
    service: Service,
    response: browser_session::PlaywrightSidecarUsageResponse,
) -> CommandResult<UsageDisplayState> {
    let mut response = Some(response);
    let display_state = engine
        .refresh_preflight_provider_source_with_snapshot(service, UsageSource::Web, |observed_at| {
            let response = response.take().ok_or(UsageProviderError::Internal)?;

            usage_snapshot_from_sidecar_usage_response(response, observed_at)
        })
        .map_err(map_provider_refresh_error)?;

    app.emit(usage::SNAPSHOTS_UPDATED_EVENT, &display_state)
        .map_err(map_event_emit_error)?;
    emit_provider_error_events(app, &display_state);

    Ok(display_state)
}

fn headless_web_usage_response(
    app: &AppHandle,
    engine: &UsageEngine,
    sessions: &browser_session::BrowserSessionManager,
    service: Service,
) -> Result<browser_session::PlaywrightSidecarUsageResponse, String> {
    let config = engine
        .config()
        .map_err(|_| "Could not load usage configuration".to_string())?;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "Could not resolve app data directory".to_string())?;
    let sidecar_request = web_usage_refresh_sidecar_request(&config, &app_data_dir, service)?;

    run_playwright_sidecar_usage_refresh(app, sessions, &sidecar_request)
}

fn web_usage_refresh_sidecar_request(
    config: &config::AppConfig,
    app_data_dir: &Path,
    service: Service,
) -> Result<browser_session::PlaywrightSidecarLaunchRequest, String> {
    let paths = prepare_managed_browser_profiles(config, app_data_dir)?;
    let Some(paths) = paths else {
        return Err("Managed browser profile is not prepared".to_string());
    };
    let profile_path = match service {
        Service::Codex => &paths.codex,
        Service::Claude => &paths.claude,
    };
    let launch_plan = browser_session::chromium_launch_plan(service, profile_path);
    let launch_request = browser_session::playwright_launch_request(&launch_plan);

    browser_session::playwright_sidecar_refresh_request(
        &launch_request,
        official_usage_url(service),
    )
}

fn run_playwright_sidecar_usage_refresh(
    app: &AppHandle,
    sessions: &browser_session::BrowserSessionManager,
    request: &browser_session::PlaywrightSidecarLaunchRequest,
) -> Result<browser_session::PlaywrightSidecarUsageResponse, String> {
    sessions.stop_service(request.service, browser_session::PROFILE_STOP_TIMEOUT)?;

    let mut command: std::process::Command = app
        .shell()
        .sidecar(PLAYWRIGHT_SIDECAR_NAME)
        .map_err(|_| "Managed usage sidecar is unavailable".to_string())?
        .into();
    let mut child = command
        .spawn()
        .map_err(|_| "Managed usage sidecar is unavailable".to_string())?;

    if let Err(error) = write_sidecar_launch_request(&mut child, request) {
        return Err(kill_untracked_child(child, &error));
    }

    let Some(stdout) = child.stdout.take() else {
        return Err(kill_untracked_child(
            child,
            "Managed usage sidecar did not acknowledge refresh",
        ));
    };
    let line = match read_sidecar_stdout_line(stdout, PLAYWRIGHT_SIDECAR_ACK_TIMEOUT) {
        Ok(line) => line,
        Err(error) => return Err(kill_untracked_child(child, &error)),
    };
    let response = match browser_session::playwright_sidecar_usage_response(&line, request) {
        Ok(response) => response,
        Err(error) => return Err(kill_untracked_child(child, &error)),
    };

    child
        .wait()
        .map_err(|_| "Managed usage sidecar did not finish refresh".to_string())?;

    Ok(response)
}

fn usage_snapshot_from_sidecar_usage_response(
    response: browser_session::PlaywrightSidecarUsageResponse,
    observed_at: &str,
) -> Result<UsageSnapshot, UsageProviderError> {
    let page_state = visible_page_state_from_sidecar(response.page_state.as_str())?;

    Ok(web_provider::parse_visible_usage(
        VisibleUsageInput {
            service: response.service,
            page_state,
            remaining_percent: response.remaining_percent,
            used_percent: response.used_percent,
            reset_at: response.reset_at,
            visible_fields: response.visible_fields,
        },
        observed_at,
    ))
}

fn visible_page_state_from_sidecar(value: &str) -> Result<VisiblePageState, UsageProviderError> {
    match value {
        "usage" => Ok(VisiblePageState::Usage),
        "logged_out" => Ok(VisiblePageState::LoggedOut),
        "mfa_required" => Ok(VisiblePageState::MfaRequired),
        "captcha_or_bot_check" => Ok(VisiblePageState::CaptchaOrBotCheck),
        "network_unavailable" => Ok(VisiblePageState::NetworkUnavailable),
        "timed_out" => Ok(VisiblePageState::TimedOut),
        "unexpected_ui" => Ok(VisiblePageState::UnexpectedUi),
        _ => Err(UsageProviderError::UnexpectedUi),
    }
}

#[tauri::command]
fn refresh_usage(
    app: AppHandle,
    engine: State<'_, UsageEngine>,
    sessions: State<'_, browser_session::BrowserSessionManager>,
) -> CommandResult<UsageDisplayState> {
    emit_refresh_event(
        &app,
        None,
        None,
        usage::REFRESH_STARTED_EVENT,
        UsageRefreshStatus::Started,
    )?;

    match refresh_all_with_headless_web(&app, &engine, &sessions) {
        Ok(display_state) => {
            emit_provider_error_events(&app, &display_state);
            emit_refresh_event(
                &app,
                None,
                None,
                usage::REFRESH_FINISHED_EVENT,
                UsageRefreshStatus::Finished,
            )?;
            Ok(display_state)
        }
        Err(error) => {
            let _ = emit_refresh_event(
                &app,
                None,
                None,
                usage::REFRESH_FINISHED_EVENT,
                UsageRefreshStatus::Failed,
            );
            Err(error)
        }
    }
}

fn refresh_all_with_headless_web(
    app: &AppHandle,
    engine: &UsageEngine,
    sessions: &browser_session::BrowserSessionManager,
) -> CommandResult<UsageDisplayState> {
    let mut display_state = engine.refresh_all().map_err(map_usage_refresh_error)?;
    let config = engine.config().map_err(map_usage_state_error)?;

    if config.providers.web_enabled {
        for service in [Service::Codex, Service::Claude] {
            let service_enabled = match service {
                Service::Codex => config.enabled_services.codex,
                Service::Claude => config.enabled_services.claude,
            };

            if !service_enabled {
                continue;
            }

            if let Ok(updated) = refresh_web_provider_headless(app, engine, sessions, service) {
                display_state = updated;
            }
        }
    }

    app.emit(usage::SNAPSHOTS_UPDATED_EVENT, &display_state)
        .map_err(map_event_emit_error)?;

    Ok(display_state)
}

fn refresh_due_with_headless_web(
    app: &AppHandle,
    engine: &UsageEngine,
    sessions: &browser_session::BrowserSessionManager,
) -> CommandResult<UsageDisplayState> {
    let config = engine.config().map_err(map_usage_state_error)?;

    if config.providers.web_enabled {
        for service in [Service::Codex, Service::Claude] {
            let service_enabled = match service {
                Service::Codex => config.enabled_services.codex,
                Service::Claude => config.enabled_services.claude,
            };

            if service_enabled {
                refresh_due_web_provider_headless(app, engine, sessions, service)?;
            }
        }
    }

    let display_state = engine.refresh_due().map_err(map_usage_refresh_error)?;
    app.emit(usage::SNAPSHOTS_UPDATED_EVENT, &display_state)
        .map_err(map_event_emit_error)?;

    Ok(display_state)
}

#[tauri::command]
fn refresh_provider(
    app: AppHandle,
    engine: State<'_, UsageEngine>,
    sessions: State<'_, browser_session::BrowserSessionManager>,
    service: Service,
    source: UsageSource,
) -> CommandResult<UsageDisplayState> {
    emit_refresh_event(
        &app,
        Some(service),
        Some(source),
        usage::REFRESH_STARTED_EVENT,
        UsageRefreshStatus::Started,
    )?;

    let refresh_result = if source == UsageSource::Web {
        refresh_web_provider_headless(&app, &engine, &sessions, service)
    } else {
        engine
            .refresh_provider_source(service, source)
            .map_err(map_provider_refresh_error)
    };

    match refresh_result {
        Ok(display_state) => {
            app.emit(usage::SNAPSHOTS_UPDATED_EVENT, &display_state)
                .map_err(map_event_emit_error)?;
            emit_provider_error_events(&app, &display_state);
            emit_refresh_event(
                &app,
                Some(service),
                Some(source),
                usage::REFRESH_FINISHED_EVENT,
                UsageRefreshStatus::Finished,
            )?;
            Ok(display_state)
        }
        Err(error) => {
            let _ = emit_refresh_event(
                &app,
                Some(service),
                Some(source),
                usage::REFRESH_FINISHED_EVENT,
                UsageRefreshStatus::Failed,
            );
            Err(error)
        }
    }
}

fn tray_icon_rgba_for(state: usage::TrayGaugeState, low_usage_threshold: f32) -> Option<Vec<u8>> {
    state.remaining_percent.map(|percent| {
        dynamic_tray_icon_rgba(
            state.service,
            percent.clamp(0.0, 100.0),
            low_usage_threshold.clamp(0.0, 100.0),
        )
    })
}

fn dynamic_tray_icon_rgba(
    service: Service,
    remaining_percent: f32,
    low_usage_threshold: f32,
) -> Vec<u8> {
    let accent = tray_accent_for(service, remaining_percent, low_usage_threshold);
    let progress = remaining_percent / 100.0;
    let size = TRAY_ICON_SIZE as usize;
    let mut rgba = vec![0; size * size * 4];

    for y in 0..TRAY_ICON_SIZE {
        for x in 0..TRAY_ICON_SIZE {
            let dx = x as f32 + 0.5 - TRAY_ICON_CENTER;
            let dy = y as f32 + 0.5 - TRAY_ICON_CENTER;
            let distance = (dx.mul_add(dx, dy * dy)).sqrt();
            let color = if (TRAY_ICON_INNER_RADIUS..=TRAY_ICON_OUTER_RADIUS).contains(&distance) {
                let angle =
                    (dy.atan2(dx) + std::f32::consts::FRAC_PI_2).rem_euclid(std::f32::consts::TAU);

                if angle / std::f32::consts::TAU <= progress {
                    accent
                } else {
                    TRAY_TRACK
                }
            } else if distance < TRAY_ICON_INNER_RADIUS {
                TRAY_SURFACE
            } else {
                TRAY_TRANSPARENT
            };
            let index = ((y * TRAY_ICON_SIZE + x) * 4) as usize;
            rgba[index..index + 4].copy_from_slice(&color);
        }
    }

    rgba
}

fn tray_accent_for(service: Service, remaining_percent: f32, low_usage_threshold: f32) -> [u8; 4] {
    if remaining_percent <= low_usage_threshold {
        return TRAY_LOW_ACCENT;
    }

    match service {
        Service::Codex => TRAY_CODEX_ACCENT,
        Service::Claude => TRAY_CLAUDE_ACCENT,
    }
}

fn tray_icon_for(state: usage::TrayGaugeState, low_usage_threshold: f32) -> Image<'static> {
    if let Some(rgba) = tray_icon_rgba_for(state, low_usage_threshold) {
        return Image::new_owned(rgba, TRAY_ICON_SIZE, TRAY_ICON_SIZE);
    }

    Image::from_bytes(TRAY_UNKNOWN)
        .expect("valid bundled unknown tray icon")
        .to_owned()
}

fn start_usage_scheduler(app: AppHandle) {
    std::thread::spawn(move || loop {
        let sleep_duration = {
            let engine = app.state::<UsageEngine>();
            let sessions = app.state::<browser_session::BrowserSessionManager>();
            let _ = emit_refresh_event(
                &app,
                None,
                None,
                usage::REFRESH_STARTED_EVENT,
                UsageRefreshStatus::Started,
            );
            let refresh_result = refresh_due_with_headless_web(&app, &engine, &sessions);
            if let Ok(display_state) = &refresh_result {
                emit_provider_error_events(&app, display_state);
            }
            let _ = emit_refresh_event(
                &app,
                None,
                None,
                usage::REFRESH_FINISHED_EVENT,
                if refresh_result.is_ok() {
                    UsageRefreshStatus::Finished
                } else {
                    UsageRefreshStatus::Failed
                },
            );
            engine
                .scheduler_sleep_duration()
                .unwrap_or_else(|_| std::time::Duration::from_secs(45))
        };

        std::thread::sleep(sleep_duration);
    });
}

fn start_gauge_rotation(tray: TrayIcon, app: AppHandle) {
    std::thread::spawn(move || {
        let mut index = 0usize;

        loop {
            let engine = app.state::<UsageEngine>();
            let config = engine.config().unwrap_or_default();
            let display_state = engine
                .display_state()
                .unwrap_or_else(|_| UsageDisplayState {
                    snapshots: Vec::new(),
                    updated_at: String::new(),
                });
            let states = display_state.tray_states();
            let state = states[index % states.len()];
            let label = format!(
                "{}: {} remaining",
                state.service.label(),
                state
                    .remaining_percent
                    .map(|percent| format!("{}%", percent.round()))
                    .unwrap_or_else(|| "unknown".to_string())
            );

            let _ = tray.set_icon(Some(tray_icon_for(state, config.low_usage_threshold)));
            let _ = tray.set_tooltip(Some(label.as_str()));

            index += 1;
            std::thread::sleep(std::time::Duration::from_secs(
                config.intervals.gauge_switch_seconds,
            ));
        }
    });
}

fn main_window(app: &tauri::AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("main").or_else(|| {
        WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
            .title("ForgeGauge")
            .inner_size(420.0, 640.0)
            .min_inner_size(360.0, 520.0)
            .resizable(true)
            .center()
            .visible(false)
            .closable(false)
            .build()
            .ok()
    })
}

fn configure_popup_window(window: &WebviewWindow) {
    let _ = window.set_skip_taskbar(true);
    let _ = window.set_always_on_top(true);
}

#[derive(Clone, Copy, Debug)]
struct PopupAnchor {
    x: f64,
    y: f64,
}

#[derive(Clone, Copy, Debug)]
struct PopupSize {
    width: u32,
    height: u32,
}

#[derive(Clone, Copy, Debug)]
struct PopupWorkArea {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl PopupWorkArea {
    fn contains(self, anchor: PopupAnchor) -> bool {
        let min_x = self.x as f64;
        let min_y = self.y as f64;
        let max_x = min_x + self.width as f64;
        let max_y = min_y + self.height as f64;

        anchor.x >= min_x && anchor.x <= max_x && anchor.y >= min_y && anchor.y <= max_y
    }
}

fn popup_position_near_anchor(
    anchor: PopupAnchor,
    popup: PopupSize,
    work_area: PopupWorkArea,
) -> (i32, i32) {
    let anchor_x = anchor.x.round() as i32;
    let anchor_y = anchor.y.round() as i32;
    let popup_width = popup.width as i32;
    let popup_height = popup.height as i32;
    let min_x = work_area.x;
    let min_y = work_area.y;
    let max_x = work_area
        .x
        .saturating_add(work_area.width as i32)
        .saturating_sub(popup_width);
    let max_y = work_area
        .y
        .saturating_add(work_area.height as i32)
        .saturating_sub(popup_height);
    let preferred_x = anchor_x
        .saturating_sub(popup_width)
        .saturating_add(POPUP_ANCHOR_GAP);
    let fallback_x = anchor_x.saturating_add(POPUP_ANCHOR_GAP);
    let preferred_y = anchor_y
        .saturating_sub(popup_height)
        .saturating_sub(POPUP_ANCHOR_GAP);
    let fallback_y = anchor_y.saturating_add(POPUP_ANCHOR_GAP);
    let x = if preferred_x >= min_x {
        preferred_x
    } else {
        fallback_x
    };
    let y = if preferred_y >= min_y {
        preferred_y
    } else {
        fallback_y
    };

    (
        clamp_to_range(x, min_x, max_x),
        clamp_to_range(y, min_y, max_y),
    )
}

fn clamp_to_range(value: i32, min: i32, max: i32) -> i32 {
    if max < min {
        min
    } else {
        value.clamp(min, max)
    }
}

fn popup_work_area_for_anchor(
    app: &tauri::AppHandle,
    anchor: PopupAnchor,
) -> Option<PopupWorkArea> {
    let monitors = app.available_monitors().unwrap_or_default();

    monitors
        .iter()
        .map(|monitor| {
            let work_area = monitor.work_area();

            PopupWorkArea {
                x: work_area.position.x,
                y: work_area.position.y,
                width: work_area.size.width,
                height: work_area.size.height,
            }
        })
        .find(|work_area| work_area.contains(anchor))
        .or_else(|| {
            app.primary_monitor().ok().flatten().map(|monitor| {
                let work_area = monitor.work_area();

                PopupWorkArea {
                    x: work_area.position.x,
                    y: work_area.position.y,
                    width: work_area.size.width,
                    height: work_area.size.height,
                }
            })
        })
}

fn position_popup_window_near_anchor(
    app: &tauri::AppHandle,
    window: &WebviewWindow,
    anchor: PopupAnchor,
) {
    let Ok(size) = window.outer_size() else {
        return;
    };
    let Some(work_area) = popup_work_area_for_anchor(app, anchor) else {
        return;
    };
    let (x, y) = popup_position_near_anchor(
        anchor,
        PopupSize {
            width: size.width,
            height: size.height,
        },
        work_area,
    );

    let _ = window.set_position(dpi::PhysicalPosition::new(x, y));
}

fn show_main_window(app: &tauri::AppHandle) {
    let window = main_window(app);

    if let Some(window) = window {
        configure_popup_window(&window);
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn toggle_main_window_near(app: &tauri::AppHandle, anchor: PopupAnchor) {
    if let Some(window) = main_window(app) {
        configure_popup_window(&window);

        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            position_popup_window_near_anchor(app, &window, anchor);
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

fn hide_main_window_if_exists(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

#[tauri::command]
fn hide_main_window(app: AppHandle) -> CommandResult<WindowVisibility> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(map_window_visibility_error)?;
    window.hide().map_err(|_| map_window_visibility_error())?;

    Ok(WindowVisibility {
        status: "hidden".to_string(),
        updated_at: usage::now_rfc3339(),
    })
}

fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "show", "Show ForgeGauge", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &quit_item])?;
    let app_handle = app.handle().clone();
    let click_app_handle = app_handle.clone();
    let (config, initial_state) = {
        let engine = app.state::<UsageEngine>();
        let config = engine.config().unwrap_or_default();
        let display_state = engine
            .display_state()
            .unwrap_or_else(|_| UsageDisplayState {
                snapshots: Vec::new(),
                updated_at: String::new(),
            });
        let initial_state = display_state.tray_states()[0];

        (config, initial_state)
    };

    let tray = TrayIconBuilder::with_id("main")
        .tooltip("ForgeGauge")
        .icon(tray_icon_for(initial_state, config.low_usage_threshold))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(move |_tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                position,
                ..
            } = event
            {
                toggle_main_window_near(
                    &click_app_handle,
                    PopupAnchor {
                        x: position.x,
                        y: position.y,
                    },
                );
            }
        })
        .build(app)?;

    start_gauge_rotation(tray, app_handle);

    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            clear_cached_snapshots,
            clear_provider_profile,
            get_app_config,
            get_display_state,
            get_log_location,
            get_usage_snapshots,
            hide_main_window,
            inspect_provider_profile,
            open_official_usage_page,
            refresh_provider,
            refresh_usage,
            reset_provider_session,
            start_provider_login,
            update_app_config
        ])
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let app_handle = app.handle().clone();
            let (config, config_error) = match config::load(&app_handle) {
                Ok(config) => (config, None),
                Err(error) => (config::AppConfig::default(), Some(error)),
            };

            app.manage(ConfigLoadState::new(config_error));
            let sessions = match app_handle.path().app_data_dir() {
                Ok(app_data_dir) => browser_session::BrowserSessionManager::with_registry_path(
                    app_data_dir.join(browser_session::SESSION_REGISTRY_FILE_NAME),
                ),
                Err(_) => browser_session::BrowserSessionManager::default(),
            };
            if sessions.detect_orphans_on_startup().is_err() {
                log_startup_warning(StartupWarning::BrowserSessionRecovery);
            }
            app.manage(sessions);
            if sync_autostart(&app_handle, config.autostart.enabled).is_err() {
                log_startup_warning(StartupWarning::AutostartSync);
            }
            app.manage(UsageEngine::new(config));
            if app
                .state::<UsageEngine>()
                .refresh_all_and_emit(&app_handle)
                .is_err()
            {
                log_startup_warning(StartupWarning::InitialUsageRefresh);
            }
            if let Some(window) = app_handle.get_webview_window("main") {
                configure_popup_window(&window);
            }
            setup_tray(app)?;
            start_usage_scheduler(app_handle);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building ForgeGauge")
        .run(|app_handle, event| match event {
            tauri::RunEvent::WindowEvent { label, event, .. } if label == "main" => match event {
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    hide_main_window_if_exists(app_handle);
                }
                WindowEvent::Focused(false) => hide_main_window_if_exists(app_handle),
                _ => {}
            },
            tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } => {
                api.prevent_exit();
            }
            _ => {}
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT_TEST_DIR_ID: AtomicU64 = AtomicU64::new(0);

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "forgegauge-lib-test-{}-{}",
                std::process::id(),
                NEXT_TEST_DIR_ID.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).expect("test dir is created");
            Self { path }
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn tray_state(service: Service, remaining_percent: Option<f32>) -> usage::TrayGaugeState {
        usage::TrayGaugeState {
            service,
            remaining_percent,
        }
    }

    #[test]
    fn prepare_managed_browser_profiles_skips_when_web_profiles_are_not_needed() {
        let dir = TestDir::new();
        let config = config::AppConfig::default();

        let paths = prepare_managed_browser_profiles(&config, &dir.path)
            .expect("profile preparation skips");

        assert_eq!(paths, None);
        assert!(!dir.path.join("browser-profiles").exists());
    }

    #[test]
    fn prepare_managed_browser_profiles_initializes_chromium_preferences_for_enabled_web_profiles()
    {
        let dir = TestDir::new();
        let mut config = config::AppConfig::default();
        config.providers.web_enabled = true;

        let paths = prepare_managed_browser_profiles(&config, &dir.path)
            .expect("profile preparation succeeds")
            .expect("profile paths are returned");
        let codex_preferences = read_preferences(
            paths
                .codex
                .join(browser_session::CHROMIUM_DEFAULT_PROFILE_DIR)
                .join(browser_session::CHROMIUM_PREFERENCES_FILE_NAME),
        );
        let claude_preferences = read_preferences(
            paths
                .claude
                .join(browser_session::CHROMIUM_DEFAULT_PROFILE_DIR)
                .join(browser_session::CHROMIUM_PREFERENCES_FILE_NAME),
        );

        assert_preference_false(&codex_preferences, &["credentials_enable_service"]);
        assert_preference_false(&codex_preferences, &["credentials_enable_autosignin"]);
        assert_preference_false(&codex_preferences, &["profile", "password_manager_enabled"]);
        assert_preference_false(&codex_preferences, &["autofill", "enabled"]);
        assert_preference_false(&codex_preferences, &["autofill", "profile_enabled"]);
        assert_preference_false(&codex_preferences, &["autofill", "credit_card_enabled"]);
        assert_preference_false(&claude_preferences, &["credentials_enable_service"]);
        assert_preference_false(&claude_preferences, &["autofill", "enabled"]);
    }

    #[test]
    fn inspect_provider_profile_for_service_reports_unprepared_default_profiles_without_paths() {
        let dir = TestDir::new();
        let config = config::AppConfig::default();

        let report = inspect_provider_profile_for_service(
            &config,
            &dir.path,
            Service::Claude,
            "2026-06-04T00:00:00Z".to_string(),
        )
        .expect("profile inspection succeeds");
        let value = serde_json::to_value(&report).expect("report serializes");

        assert_eq!(report.service, Service::Claude);
        assert_eq!(report.profile_label, "claude-profile");
        assert!(!report.profile_prepared);
        assert_eq!(report.credential_store_files, 0);
        assert_eq!(report.autofill_store_files, 0);
        assert_eq!(report.cookie_store_files, 0);
        assert_eq!(report.site_storage_entries, 0);
        assert_eq!(report.symlink_entries, 0);
        assert!(!report.password_saving_enabled);
        assert!(!report.autofill_enabled);
        assert_eq!(value["profileLabel"], "claude-profile");
        assert!(value.get("path").is_none());
        assert!(value.get("profilePath").is_none());
        assert!(!format!("{value:?}").contains(dir.path.to_string_lossy().as_ref()));
    }

    #[test]
    fn inspect_provider_profile_for_service_reports_enabled_web_profile_state() {
        let dir = TestDir::new();
        let mut config = config::AppConfig::default();
        config.providers.web_enabled = true;

        let report = inspect_provider_profile_for_service(
            &config,
            &dir.path,
            Service::Codex,
            "2026-06-04T00:05:00Z".to_string(),
        )
        .expect("profile inspection succeeds");

        assert_eq!(report.service, Service::Codex);
        assert_eq!(report.profile_label, "codex-profile");
        assert!(report.profile_prepared);
        assert_eq!(report.credential_store_files, 0);
        assert_eq!(report.autofill_store_files, 0);
        assert_eq!(report.cookie_store_files, 0);
        assert_eq!(report.site_storage_entries, 0);
        assert_eq!(report.symlink_entries, 0);
        assert!(!report.password_saving_enabled);
        assert!(!report.autofill_enabled);
        assert!(report.inspected_entries > 0);
        assert!(!report.entry_limit_reached);
    }

    #[test]
    fn provider_profile_inspection_serializes_to_sanitized_ipc_shape() {
        let report = provider_profile_inspection_report(
            Service::Codex,
            true,
            Some(browser_session::BrowserProfileStorageInspection {
                credential_store_files: 2,
                autofill_store_files: 3,
                cookie_store_files: 4,
                site_storage_entries: 5,
                symlink_entries: 1,
                password_saving_enabled: true,
                autofill_enabled: false,
                inspected_entries: 7,
                entry_limit_reached: false,
            }),
            "2026-06-04T00:10:00Z".to_string(),
        );
        let value = serde_json::to_value(report).expect("report serializes");

        assert_eq!(
            value,
            serde_json::json!({
                "service": "codex",
                "profileLabel": "codex-profile",
                "profilePrepared": true,
                "credentialStoreFiles": 2,
                "autofillStoreFiles": 3,
                "cookieStoreFiles": 4,
                "siteStorageEntries": 5,
                "symlinkEntries": 1,
                "passwordSavingEnabled": true,
                "autofillEnabled": false,
                "inspectedEntries": 7,
                "entryLimitReached": false,
                "inspectedAt": "2026-06-04T00:10:00Z"
            })
        );
        assert!(value.get("path").is_none());
        assert!(value.get("raw").is_none());
    }

    #[test]
    fn sidecar_logged_out_usage_response_maps_to_login_required_snapshot() {
        let response = sidecar_usage_response(Service::Claude, "logged_out");

        let snapshot = usage_snapshot_from_sidecar_usage_response(response, "2026-06-04T12:00:00Z")
            .expect("snapshot is built");

        assert_eq!(snapshot.service, Service::Claude);
        assert_eq!(snapshot.source, UsageSource::Web);
        assert_eq!(snapshot.remaining_percent, None);
        assert_eq!(snapshot.details["status"], "login_required");
        assert_eq!(snapshot.details["reason"], "logged_out");
        assert_eq!(snapshot.details["providerId"], "claude.web");
    }

    #[test]
    fn sidecar_interruption_usage_responses_map_to_fail_closed_web_snapshots() {
        for (page_state, status, reason) in [
            ("mfa_required", "mfa_required", "mfa_required"),
            (
                "captcha_or_bot_check",
                "captcha_or_bot_check",
                "captcha_or_bot_check",
            ),
            (
                "network_unavailable",
                "network_unavailable",
                "network_unavailable",
            ),
            ("timed_out", "timed_out", "timed_out"),
            ("unexpected_ui", "unexpected_ui", "unexpected_ui"),
        ] {
            let response = sidecar_usage_response(Service::Codex, page_state);
            let snapshot =
                usage_snapshot_from_sidecar_usage_response(response, "2026-06-04T12:00:00Z")
                    .expect("snapshot is built");

            assert_eq!(snapshot.service, Service::Codex);
            assert_eq!(snapshot.source, UsageSource::Web);
            assert_eq!(snapshot.confidence, usage::UsageConfidence::Unknown);
            assert_eq!(snapshot.remaining_percent, None);
            assert_eq!(snapshot.used_percent, None);
            assert_eq!(snapshot.details["status"], status);
            assert_eq!(snapshot.details["reason"], reason);
            assert_eq!(snapshot.details["providerId"], "codex.web");
        }
    }

    #[test]
    fn unsupported_sidecar_page_state_is_rejected_without_echoing_state() {
        let response = sidecar_usage_response(Service::Claude, "raw_authenticated_html");
        let error = usage_snapshot_from_sidecar_usage_response(response, "2026-06-04T12:00:00Z")
            .expect_err("unsupported sidecar state is rejected");

        assert_eq!(error, UsageProviderError::UnexpectedUi);
        assert!(!format!("{error:?}").contains("raw_authenticated_html"));
    }

    #[test]
    fn sidecar_usage_response_maps_visible_fields_to_web_snapshot() {
        let response = browser_session::PlaywrightSidecarUsageResponse {
            remaining_percent: Some(63.0),
            used_percent: Some(37.0),
            reset_at: Some("2026-06-05T00:00:00Z".to_string()),
            visible_fields: vec![
                "remaining_percent".to_string(),
                "used_percent".to_string(),
                "reset_at".to_string(),
            ],
            ..sidecar_usage_response(Service::Codex, "usage")
        };

        let snapshot = usage_snapshot_from_sidecar_usage_response(response, "2026-06-04T12:00:00Z")
            .expect("snapshot is built");

        assert_eq!(snapshot.service, Service::Codex);
        assert_eq!(snapshot.source, UsageSource::Web);
        assert_eq!(snapshot.remaining_percent, Some(63.0));
        assert_eq!(snapshot.used_percent, Some(37.0));
        assert_eq!(snapshot.details["status"], "parsed");
        assert_eq!(
            snapshot.details["lastOfficialCheckAt"],
            "2026-06-04T12:00:00Z"
        );
    }

    #[test]
    fn sidecar_usage_response_with_missing_visible_data_maps_to_unknown_snapshot() {
        let response = sidecar_usage_response(Service::Claude, "usage");
        let snapshot = usage_snapshot_from_sidecar_usage_response(response, "2026-06-04T12:00:00Z")
            .expect("snapshot is built");

        assert_eq!(snapshot.service, Service::Claude);
        assert_eq!(snapshot.source, UsageSource::Web);
        assert_eq!(snapshot.confidence, usage::UsageConfidence::Unknown);
        assert_eq!(snapshot.remaining_percent, None);
        assert_eq!(snapshot.details["status"], "missing_data");
        assert_eq!(snapshot.details["reason"], "missing_visible_percentage");
        assert_eq!(snapshot.details["providerId"], "claude.web");
    }

    #[test]
    fn sidecar_usage_response_parse_failures_are_sanitized() {
        for (response, reason) in [
            (
                browser_session::PlaywrightSidecarUsageResponse {
                    remaining_percent: Some(80.0),
                    used_percent: Some(80.0),
                    visible_fields: vec![
                        "remaining_percent".to_string(),
                        "used_percent".to_string(),
                    ],
                    ..sidecar_usage_response(Service::Codex, "usage")
                },
                "invalid_visible_percentage",
            ),
            (
                browser_session::PlaywrightSidecarUsageResponse {
                    remaining_percent: Some(80.0),
                    reset_at: Some("not-a-timestamp".to_string()),
                    visible_fields: vec!["remaining_percent".to_string(), "reset_at".to_string()],
                    ..sidecar_usage_response(Service::Codex, "usage")
                },
                "invalid_reset_at",
            ),
            (
                browser_session::PlaywrightSidecarUsageResponse {
                    remaining_percent: Some(80.0),
                    visible_fields: vec![
                        "remaining_percent".to_string(),
                        "raw_authenticated_html".to_string(),
                    ],
                    ..sidecar_usage_response(Service::Codex, "usage")
                },
                "unsupported_visible_field",
            ),
        ] {
            let snapshot =
                usage_snapshot_from_sidecar_usage_response(response, "2026-06-04T12:00:00Z")
                    .expect("snapshot is built");
            let serialized_details = snapshot.details.to_string();

            assert_eq!(snapshot.service, Service::Codex);
            assert_eq!(snapshot.source, UsageSource::Web);
            assert_eq!(snapshot.confidence, usage::UsageConfidence::Unknown);
            assert_eq!(snapshot.remaining_percent, None);
            assert_eq!(snapshot.used_percent, None);
            assert_eq!(snapshot.details["status"], "parse_failed");
            assert_eq!(snapshot.details["reason"], reason);
            assert_eq!(snapshot.details["providerId"], "codex.web");
            assert!(!serialized_details.contains("not-a-timestamp"));
            assert!(!serialized_details.contains("raw_authenticated_html"));
        }
    }

    fn sidecar_usage_response(
        service: Service,
        page_state: &str,
    ) -> browser_session::PlaywrightSidecarUsageResponse {
        browser_session::PlaywrightSidecarUsageResponse {
            protocol_version: 1,
            action: browser_session::PLAYWRIGHT_SIDECAR_ACTION_REFRESH_USAGE.to_string(),
            backend: browser_session::PLAYWRIGHT_BACKEND_ID.to_string(),
            service,
            profile_label: provider_profile_label(service).to_string(),
            headless: true,
            arg_count: 4,
            status: browser_session::PLAYWRIGHT_SIDECAR_STATUS_CHECKED.to_string(),
            page_state: page_state.to_string(),
            remaining_percent: None,
            used_percent: None,
            reset_at: None,
            visible_fields: Vec::new(),
        }
    }

    fn read_preferences(path: impl Into<PathBuf>) -> serde_json::Value {
        let raw = std::fs::read_to_string(path.into()).expect("preferences are readable");
        serde_json::from_str(&raw).expect("preferences parse")
    }

    fn assert_preference_false(preferences: &serde_json::Value, path: &[&str]) {
        let value = path
            .iter()
            .fold(preferences, |value, segment| &value[*segment]);

        assert_eq!(value.as_bool(), Some(false), "{path:?} should be false");
    }

    #[test]
    fn config_load_state_reports_and_clears_startup_error() {
        let state = ConfigLoadState::new(Some("bad config".to_string()));

        assert_eq!(
            state.current_error().expect("state lock succeeds"),
            Some("bad config".to_string())
        );

        state.clear_error().expect("state clear succeeds");

        assert_eq!(state.current_error().expect("state lock succeeds"), None);
    }

    #[test]
    fn command_error_serializes_to_stable_shape() {
        let error = CommandError::new("usage_state_unavailable", "Usage state is unavailable");
        let value = serde_json::to_value(error).expect("command error serializes");

        assert_eq!(
            value,
            serde_json::json!({
                "code": "usage_state_unavailable",
                "message": "Usage state is unavailable"
            })
        );
    }

    #[test]
    fn command_error_mapping_hides_internal_config_details() {
        let error = map_config_save_error(
            "Could not save /home/dev/.config/private/config.json".to_string(),
        );

        assert_eq!(
            error,
            CommandError::new("config_save_failed", "Could not save app settings")
        );
    }

    #[test]
    fn startup_warning_messages_are_sanitized() {
        let forbidden = [
            "cookie",
            "token",
            "authorization",
            "bearer",
            "password",
            "session",
            "account",
            "<html",
            "/home/",
            "/users/",
            "c:\\users\\",
        ];

        for warning in [
            StartupWarning::AutostartSync,
            StartupWarning::BrowserSessionRecovery,
            StartupWarning::InitialUsageRefresh,
        ] {
            let message = startup_warning_message(warning).to_ascii_lowercase();

            for marker in forbidden {
                assert!(
                    !message.contains(marker),
                    "startup warning contains forbidden marker {marker}"
                );
            }
        }
    }

    #[test]
    fn official_usage_page_serializes_to_ipc_shape() {
        let page = OfficialUsagePage {
            service: Service::Claude,
            url: official_usage_url(Service::Claude).to_string(),
            opened_at: "2026-06-03T00:00:00Z".to_string(),
        };
        let value = serde_json::to_value(page).expect("official page serializes");

        assert_eq!(
            value,
            serde_json::json!({
                "service": "claude",
                "url": "https://claude.ai/new#settings/usage",
                "openedAt": "2026-06-03T00:00:00Z"
            })
        );
    }

    #[test]
    fn official_usage_urls_match_services() {
        assert_eq!(
            official_usage_url(Service::Codex),
            "https://chatgpt.com/codex/cloud/settings/analytics"
        );
        assert_eq!(
            official_usage_url(Service::Claude),
            "https://claude.ai/new#settings/usage"
        );
    }

    #[test]
    fn provider_login_start_serializes_to_ipc_shape() {
        let login = ProviderLoginStart {
            service: Service::Codex,
            url: official_usage_url(Service::Codex).to_string(),
            status: "login_required".to_string(),
            backend: browser_session::PLAYWRIGHT_BACKEND_ID.to_string(),
            profile_label: "codex-profile".to_string(),
            profile_prepared: true,
            started_at: "2026-06-03T00:00:00Z".to_string(),
        };
        let value = serde_json::to_value(login).expect("provider login start serializes");

        assert_eq!(
            value,
            serde_json::json!({
                "service": "codex",
                "url": "https://chatgpt.com/codex/cloud/settings/analytics",
                "status": "login_required",
                "backend": "playwright-headed-chromium-sidecar",
                "profileLabel": "codex-profile",
                "profilePrepared": true,
                "startedAt": "2026-06-03T00:00:00Z"
            })
        );
    }

    #[test]
    fn provider_login_start_serializes_already_authenticated_status() {
        let login = ProviderLoginStart {
            service: Service::Claude,
            url: official_usage_url(Service::Claude).to_string(),
            status: LOGIN_STATUS_ALREADY_AUTHENTICATED.to_string(),
            backend: browser_session::PLAYWRIGHT_BACKEND_ID.to_string(),
            profile_label: "claude-profile".to_string(),
            profile_prepared: true,
            started_at: "2026-06-04T12:20:00Z".to_string(),
        };
        let value = serde_json::to_value(login).expect("provider login start serializes");

        assert_eq!(value["status"], "already_authenticated");
        assert_eq!(value["service"], "claude");
        assert!(value.get("profilePath").is_none());
        assert!(value.get("userDataDir").is_none());
    }

    #[test]
    fn provider_login_start_serializes_preflight_unavailable_status() {
        let login = ProviderLoginStart {
            service: Service::Codex,
            url: official_usage_url(Service::Codex).to_string(),
            status: LOGIN_STATUS_PREFLIGHT_UNAVAILABLE.to_string(),
            backend: browser_session::PLAYWRIGHT_BACKEND_ID.to_string(),
            profile_label: "codex-profile".to_string(),
            profile_prepared: true,
            started_at: "2026-06-04T12:25:00Z".to_string(),
        };
        let value = serde_json::to_value(login).expect("provider login start serializes");

        assert_eq!(value["status"], "preflight_unavailable");
        assert_eq!(value["service"], "codex");
        assert!(value.get("profilePath").is_none());
        assert!(value.get("userDataDir").is_none());
    }

    #[test]
    fn login_preflight_launches_headed_browser_only_for_user_action_states() {
        assert!(!should_launch_login_after_preflight("usage"));

        for page_state in ["logged_out", "mfa_required", "captcha_or_bot_check"] {
            assert!(should_launch_login_after_preflight(page_state));
        }

        for page_state in ["network_unavailable", "timed_out", "unexpected_ui"] {
            assert!(!should_launch_login_after_preflight(page_state));
        }
    }

    #[test]
    fn login_preflight_decision_skips_headed_browser_for_authenticated_or_unavailable_states() {
        assert_eq!(
            login_preflight_decision(Some("usage")),
            LoginPreflightDecision::AlreadyAuthenticated
        );

        for page_state in [
            "network_unavailable",
            "timed_out",
            "unexpected_ui",
            "parse_failed",
            "unsupported_state",
        ] {
            assert_eq!(
                login_preflight_decision(Some(page_state)),
                LoginPreflightDecision::Unavailable
            );
        }

        assert_eq!(
            login_preflight_decision(None),
            LoginPreflightDecision::Unavailable
        );
    }

    #[test]
    fn login_preflight_decision_launches_headed_browser_only_for_user_action_states() {
        for page_state in ["logged_out", "mfa_required", "captcha_or_bot_check"] {
            assert_eq!(
                login_preflight_decision(Some(page_state)),
                LoginPreflightDecision::LaunchBrowser
            );
        }
    }

    #[test]
    fn login_start_preflight_outcome_skips_headed_browser_when_already_authenticated() {
        assert_eq!(
            login_start_preflight_outcome(Some("usage")),
            LoginStartPreflightOutcome {
                status: LOGIN_STATUS_ALREADY_AUTHENTICATED,
                launch_headed_browser: false,
            }
        );
    }

    #[test]
    fn login_start_preflight_outcome_launches_headed_browser_only_for_user_action_states() {
        for page_state in ["logged_out", "mfa_required", "captcha_or_bot_check"] {
            assert_eq!(
                login_start_preflight_outcome(Some(page_state)),
                LoginStartPreflightOutcome {
                    status: LOGIN_STATUS_REQUIRED,
                    launch_headed_browser: true,
                }
            );
        }
    }

    #[test]
    fn login_start_preflight_outcome_skips_headed_browser_for_unavailable_states() {
        for page_state in [
            None,
            Some("network_unavailable"),
            Some("timed_out"),
            Some("unexpected_ui"),
            Some("parse_failed"),
            Some("unsupported_state"),
        ] {
            assert_eq!(
                login_start_preflight_outcome(page_state),
                LoginStartPreflightOutcome {
                    status: LOGIN_STATUS_PREFLIGHT_UNAVAILABLE,
                    launch_headed_browser: false,
                }
            );
        }
    }

    #[test]
    fn provider_login_start_report_includes_sanitized_playwright_profile_metadata() {
        let dir = TestDir::new();
        let mut config = config::AppConfig::default();
        config.providers.web_enabled = true;

        let login = provider_login_start_report(
            &config,
            &dir.path,
            Service::Claude,
            "2026-06-04T10:00:00Z".to_string(),
        )
        .expect("login start report succeeds");
        let value = serde_json::to_value(&login).expect("login start report serializes");

        assert_eq!(login.backend, browser_session::PLAYWRIGHT_BACKEND_ID);
        assert_eq!(login.profile_label, "claude-profile");
        assert!(login.profile_prepared);
        assert_eq!(value["backend"], "playwright-headed-chromium-sidecar");
        assert_eq!(value["profileLabel"], "claude-profile");
        assert_eq!(value["profilePrepared"], true);
        assert!(value.get("profilePath").is_none());
        assert!(value.get("userDataDir").is_none());
        assert!(!format!("{value:?}").contains(dir.path.to_string_lossy().as_ref()));
    }

    #[test]
    fn provider_login_start_plan_builds_sidecar_request_without_exposing_paths_to_ipc() {
        let dir = TestDir::new();
        let mut config = config::AppConfig::default();
        config.providers.web_enabled = true;

        let plan = provider_login_start_plan(
            &config,
            &dir.path,
            Service::Codex,
            "2026-06-04T10:10:00Z".to_string(),
        )
        .expect("login start plan succeeds");
        let sidecar_request = plan
            .sidecar_request
            .as_ref()
            .expect("sidecar request is prepared");
        let value = serde_json::to_value(&plan.login).expect("login start serializes");

        assert_eq!(plan.login.status, LOGIN_STATUS_REQUIRED);
        assert_eq!(sidecar_request.service, Service::Codex);
        assert_eq!(sidecar_request.url, official_usage_url(Service::Codex));
        assert_eq!(sidecar_request.profile_label, "codex-profile");
        assert!(sidecar_request
            .user_data_dir
            .contains("browser-profiles/codex"));
        assert_eq!(value["status"], "login_required");
        assert!(value.get("profilePath").is_none());
        assert!(value.get("userDataDir").is_none());
        assert!(!format!("{value:?}").contains(dir.path.to_string_lossy().as_ref()));
    }

    #[test]
    fn provider_login_start_plan_skips_sidecar_request_when_web_is_disabled() {
        let dir = TestDir::new();
        let config = config::AppConfig::default();

        let plan = provider_login_start_plan(
            &config,
            &dir.path,
            Service::Claude,
            "2026-06-04T10:15:00Z".to_string(),
        )
        .expect("login start plan succeeds");

        assert_eq!(plan.login.status, LOGIN_STATUS_REQUIRED);
        assert_eq!(plan.login.profile_label, "claude-profile");
        assert!(!plan.login.profile_prepared);
        assert!(plan.sidecar_request.is_none());
    }

    #[test]
    fn web_usage_refresh_sidecar_request_uses_headless_managed_profile() {
        let dir = TestDir::new();
        let mut config = config::AppConfig::default();
        config.providers.web_enabled = true;

        let request = web_usage_refresh_sidecar_request(&config, &dir.path, Service::Claude)
            .expect("refresh request is built");
        let debug = format!("{request:?}");

        assert_eq!(
            request.action,
            browser_session::PLAYWRIGHT_SIDECAR_ACTION_REFRESH_USAGE
        );
        assert_eq!(request.service, Service::Claude);
        assert_eq!(request.url, official_usage_url(Service::Claude));
        assert_eq!(request.profile_label, "claude-profile");
        assert!(request.headless);
        assert!(request.user_data_dir.contains("browser-profiles/claude"));
        assert!(request
            .args
            .iter()
            .all(|arg| !arg.starts_with("--user-data-dir=")));
        assert_eq!(request.diagnostics.action, "refreshUsage");
        assert_eq!(request.diagnostics.user_data_dir, "<claude-profile>");
        assert!(request.diagnostics.headless);
        assert!(!debug.contains(dir.path.to_string_lossy().as_ref()));
    }

    #[test]
    fn provider_login_start_report_marks_unprepared_profiles_when_web_is_disabled() {
        let dir = TestDir::new();
        let config = config::AppConfig::default();

        let login = provider_login_start_report(
            &config,
            &dir.path,
            Service::Codex,
            "2026-06-04T10:05:00Z".to_string(),
        )
        .expect("login start report succeeds");

        assert_eq!(login.backend, browser_session::PLAYWRIGHT_BACKEND_ID);
        assert_eq!(login.profile_label, "codex-profile");
        assert!(!login.profile_prepared);
        assert!(!dir.path.join("browser-profiles").exists());
    }

    #[test]
    fn login_required_event_serializes_to_ipc_shape() {
        let event = LoginRequiredEvent {
            service: Service::Claude,
            url: official_usage_url(Service::Claude).to_string(),
            reason: "managed_login_not_available".to_string(),
            emitted_at: "2026-06-03T00:00:00Z".to_string(),
        };
        let value = serde_json::to_value(event).expect("login required event serializes");

        assert_eq!(
            value,
            serde_json::json!({
                "service": "claude",
                "url": "https://claude.ai/new#settings/usage",
                "reason": "managed_login_not_available",
                "emittedAt": "2026-06-03T00:00:00Z"
            })
        );
    }

    #[test]
    fn login_required_event_serializes_sidecar_unavailable_reason() {
        let event = LoginRequiredEvent {
            service: Service::Codex,
            url: official_usage_url(Service::Codex).to_string(),
            reason: "sidecar_unavailable".to_string(),
            emitted_at: "2026-06-04T10:20:00Z".to_string(),
        };
        let value = serde_json::to_value(event).expect("login required event serializes");

        assert_eq!(value["reason"], "sidecar_unavailable");
        assert!(value.get("details").is_none());
    }

    #[test]
    fn login_required_event_name_is_stable() {
        assert_eq!(LOGIN_REQUIRED_EVENT, "login://required");
    }

    #[test]
    fn session_reset_event_name_is_stable() {
        assert_eq!(SESSION_RESET_EVENT, "session://reset");
    }

    #[test]
    fn cleared_provider_profile_serializes_to_ipc_shape() {
        let cleared = ClearedProviderProfile {
            service: Service::Codex,
            cleared: true,
            cleared_at: "2026-06-03T00:00:00Z".to_string(),
        };
        let value = serde_json::to_value(cleared).expect("cleared profile serializes");

        assert_eq!(
            value,
            serde_json::json!({
                "service": "codex",
                "cleared": true,
                "clearedAt": "2026-06-03T00:00:00Z"
            })
        );
    }

    #[test]
    fn log_location_serializes_to_ipc_shape() {
        let location = LogLocation {
            path: "/tmp/forgegauge.log".to_string(),
            exists: false,
            redaction_policy: LOG_REDACTION_POLICY_PATH.to_string(),
            updated_at: "2026-06-03T00:00:00Z".to_string(),
        };
        let value = serde_json::to_value(location).expect("log location serializes");

        assert_eq!(
            value,
            serde_json::json!({
                "path": "/tmp/forgegauge.log",
                "exists": false,
                "redactionPolicy": "docs/security/log-redaction-policy.md",
                "updatedAt": "2026-06-03T00:00:00Z"
            })
        );
    }

    #[test]
    fn window_visibility_serializes_to_ipc_shape() {
        let visibility = WindowVisibility {
            status: "hidden".to_string(),
            updated_at: "2026-06-03T00:00:00Z".to_string(),
        };
        let value = serde_json::to_value(visibility).expect("window visibility serializes");

        assert_eq!(
            value,
            serde_json::json!({
                "status": "hidden",
                "updatedAt": "2026-06-03T00:00:00Z"
            })
        );
    }

    #[test]
    fn popup_position_prefers_above_anchor_and_clamps_to_work_area() {
        let position = popup_position_near_anchor(
            PopupAnchor {
                x: 1900.0,
                y: 1040.0,
            },
            PopupSize {
                width: 420,
                height: 640,
            },
            PopupWorkArea {
                x: 0,
                y: 0,
                width: 1920,
                height: 1040,
            },
        );

        assert_eq!(position, (1490, 390));
    }

    #[test]
    fn popup_position_falls_below_when_anchor_is_near_top_edge() {
        let position = popup_position_near_anchor(
            PopupAnchor { x: 20.0, y: 20.0 },
            PopupSize {
                width: 420,
                height: 640,
            },
            PopupWorkArea {
                x: 0,
                y: 0,
                width: 1920,
                height: 1040,
            },
        );

        assert_eq!(position, (30, 30));
    }

    #[test]
    fn popup_position_supports_negative_origin_monitors() {
        let position = popup_position_near_anchor(
            PopupAnchor {
                x: -1000.0,
                y: 1040.0,
            },
            PopupSize {
                width: 420,
                height: 640,
            },
            PopupWorkArea {
                x: -1280,
                y: 0,
                width: 1280,
                height: 1040,
            },
        );

        assert_eq!(position, (-990, 390));
    }

    #[test]
    fn popup_position_handles_work_area_smaller_than_popup() {
        let position = popup_position_near_anchor(
            PopupAnchor { x: 120.0, y: 120.0 },
            PopupSize {
                width: 420,
                height: 640,
            },
            PopupWorkArea {
                x: 10,
                y: 20,
                width: 300,
                height: 300,
            },
        );

        assert_eq!(position, (10, 20));
    }

    #[test]
    fn unknown_tray_state_does_not_generate_dynamic_icon() {
        assert!(tray_icon_rgba_for(tray_state(Service::Codex, None), 20.0).is_none());
    }

    #[test]
    fn percentage_tray_icon_has_expected_rgba_size() {
        let rgba = tray_icon_rgba_for(tray_state(Service::Codex, Some(72.0)), 20.0)
            .expect("known percentage renders dynamic icon");

        assert_eq!(rgba.len(), (TRAY_ICON_SIZE * TRAY_ICON_SIZE * 4) as usize);
    }

    #[test]
    fn percentage_tray_icon_changes_with_remaining_percent() {
        let high = tray_icon_rgba_for(tray_state(Service::Codex, Some(80.0)), 20.0)
            .expect("high percentage renders dynamic icon");
        let medium = tray_icon_rgba_for(tray_state(Service::Codex, Some(40.0)), 20.0)
            .expect("medium percentage renders dynamic icon");

        assert_ne!(high, medium);
    }

    #[test]
    fn percentage_tray_icon_uses_service_accent_above_threshold() {
        let codex = tray_icon_rgba_for(tray_state(Service::Codex, Some(72.0)), 20.0)
            .expect("codex percentage renders dynamic icon");
        let claude = tray_icon_rgba_for(tray_state(Service::Claude, Some(72.0)), 20.0)
            .expect("claude percentage renders dynamic icon");

        assert!(codex
            .chunks_exact(4)
            .any(|pixel| pixel == TRAY_CODEX_ACCENT));
        assert!(claude
            .chunks_exact(4)
            .any(|pixel| pixel == TRAY_CLAUDE_ACCENT));
    }

    #[test]
    fn low_percentage_tray_icon_uses_low_accent_at_threshold() {
        let rgba = tray_icon_rgba_for(tray_state(Service::Claude, Some(20.0)), 20.0)
            .expect("low percentage renders dynamic icon");

        assert!(rgba.chunks_exact(4).any(|pixel| pixel == TRAY_LOW_ACCENT));
    }
}
