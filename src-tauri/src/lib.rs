mod browser_profile;
mod cli_provider;
mod kwin;
mod browser_session;
mod config;
mod official_reading;
mod snapshot_store;
mod usage_cli;
pub mod history;
pub mod local_provider;
pub mod sounds;
pub mod usage;
pub mod web_provider;

use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{ChildStdin, ChildStdout},
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
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
use web_provider::{VisiblePageState, VisibleProductInput, VisibleUsageInput, VisibleWindowInput};

use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_shell::ShellExt;

const SETTINGS_UPDATED_EVENT: &str = "settings://updated";
const LOGIN_REQUIRED_EVENT: &str = "login://required";
const SESSION_RESET_EVENT: &str = "session://reset";
const LOGIN_STATUS_REQUIRED: &str = "login_required";
const LOGIN_STATUS_LAUNCHED: &str = "launched";
const LOGIN_REASON_MANAGED_LOGIN_NOT_AVAILABLE: &str = "managed_login_not_available";
const LOGIN_REASON_SIDECAR_UNAVAILABLE: &str = "sidecar_unavailable";
const CODEX_USAGE_URL: &str = "https://chatgpt.com/codex/cloud/settings/analytics";
const CLAUDE_USAGE_URL: &str = "https://claude.ai/new#settings/usage";
const SENTRY_DSN: &str =
    "https://3a176d7b2fdccedfb2812e6a0b231f56@o4511699702317056.ingest.us.sentry.io/4511699813924864";
const PLAYWRIGHT_SIDECAR_NAME: &str = "pickgauge-playwright-sidecar";
const PLAYWRIGHT_SIDECAR_ACK_TIMEOUT: Duration = Duration::from_secs(15);
const LOG_FILE_NAME: &str = "pickgauge.log";
const LOG_REDACTION_POLICY_PATH: &str = "docs/security/log-redaction-policy.md";
const TRAY_ICON_SIZE: u32 = 64;
const TRAY_ICON_CENTER: f32 = 32.0;
const TRAY_ICON_OUTER_RADIUS: f32 = 30.0;
const TRAY_ICON_INNER_RADIUS: f32 = 21.0;
const TRAY_CODEX_ACCENT: [u8; 4] = [242, 242, 243, 255];
const TRAY_CLAUDE_ACCENT: [u8; 4] = [255, 122, 26, 255];
const TRAY_GROK_ACCENT: [u8; 4] = [156, 163, 175, 255];
const TRAY_OLLAMA_ACCENT: [u8; 4] = [37, 99, 235, 255];
const TRAY_LOW_ACCENT: [u8; 4] = [194, 65, 12, 255];
// Solid, not translucent: the icon must stay a self-contained dark coin so
// the gauge ring keeps contrast on light desktop panels too.
const TRAY_TRACK: [u8; 4] = [46, 46, 51, 255];
const TRAY_SURFACE: [u8; 4] = [15, 15, 17, 255];
const TRAY_TRANSPARENT: [u8; 4] = [0, 0, 0, 0];
const POPUP_ANCHOR_GAP: i32 = 10;
const FLOAT_WINDOW_LABEL: &str = "float";

fn sanitize_sentry_event(
    mut event: sentry::protocol::Event<'static>,
) -> sentry::protocol::Event<'static> {
    event.server_name = None;
    event.breadcrumbs = Default::default();
    strip_sentry_debug_image_paths(event.debug_meta.to_mut());
    event
}

fn strip_sentry_debug_image_paths(debug_meta: &mut sentry::protocol::DebugMeta) {
    for image in &mut debug_meta.images {
        match image {
            sentry::protocol::DebugImage::Apple(image) => {
                image.name = sentry_file_name(&image.name);
            }
            sentry::protocol::DebugImage::Symbolic(image) => {
                image.name = sentry_file_name(&image.name);
                if let Some(debug_file) = image.debug_file.as_mut() {
                    *debug_file = sentry_file_name(debug_file);
                }
            }
            sentry::protocol::DebugImage::Wasm(image) => {
                image.code_file = sentry_file_name(&image.code_file);
                if let Some(debug_file) = image.debug_file.as_mut() {
                    *debug_file = sentry_file_name(debug_file);
                }
            }
            _ => {}
        }
    }
}

fn sentry_file_name(path: &str) -> String {
    let trimmed = path.trim_end_matches(|ch| ch == '/' || ch == '\\');
    if trimmed.is_empty() {
        return path.to_string();
    }

    trimmed
        .rsplit(|ch| ch == '/' || ch == '\\')
        .next()
        .unwrap_or(trimmed)
        .to_string()
}

#[derive(Clone, Copy)]
enum StartupWarning {
    AutostartSync,
    BrowserSessionRecovery,
    InitialUsageRefresh,
    UsageHistoryStore,
}

/// Tracks which services were last seen below the low-usage threshold so we
/// only play a cue on the crossing, never on every refresh tick.
#[derive(Default)]
struct CueTracker {
    last_below: Mutex<HashMap<Service, bool>>,
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

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct UsageHistoryReport {
    codex: Vec<history::DailyGaugeStat>,
    claude: Vec<history::DailyGaugeStat>,
    days: u32,
    generated_at: String,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalDailyUsageReport {
    codex: Vec<local_provider::DailyTokenUsage>,
    claude: Vec<local_provider::DailyTokenUsage>,
    codex_status: Option<String>,
    claude_status: Option<String>,
    days: u32,
    generated_at: String,
}

struct ProviderLoginStartPlan {
    login: ProviderLoginStart,
    sidecar_request: Option<browser_session::PlaywrightSidecarLaunchRequest>,
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
        StartupWarning::AutostartSync => "PickGauge startup warning: autostart sync failed",
        StartupWarning::BrowserSessionRecovery => {
            "PickGauge startup warning: managed browser recovery failed"
        }
        StartupWarning::InitialUsageRefresh => {
            "PickGauge startup warning: initial usage refresh failed"
        }
        StartupWarning::UsageHistoryStore => {
            "PickGauge startup warning: usage history store is unavailable"
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
        Service::Grok | Service::Ollama => {
            unreachable!("deferred services have no official runtime URL")
        }
    }
}

fn browser_profile_service(service: Service) -> browser_profile::BrowserProfileService {
    match service {
        Service::Codex => browser_profile::BrowserProfileService::Codex,
        Service::Claude => browser_profile::BrowserProfileService::Claude,
        Service::Grok | Service::Ollama => {
            unreachable!("deferred services have no managed browser profile")
        }
    }
}

fn managed_browser_service(service: Service) -> bool {
    service.is_runtime()
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

#[derive(Default)]
struct ConfigMutationCoordinator {
    mutation: Mutex<()>,
}

impl ConfigMutationCoordinator {
    fn serialized<T>(&self, operation: impl FnOnce() -> CommandResult<T>) -> CommandResult<T> {
        let _guard = self.mutation.lock().map_err(|_| {
            command_error(
                "config_mutation_unavailable",
                "Settings updates are temporarily unavailable",
            )
        })?;
        operation()
    }

    fn update(
        &self,
        app: &AppHandle,
        mutation: impl FnOnce(config::AppConfig) -> config::AppConfig,
    ) -> CommandResult<config::AppConfig> {
        self.serialized(|| {
            let engine = app.state::<UsageEngine>();
            let config_load = app.state::<ConfigLoadState>();
            let previous_config = engine.config().map_err(map_usage_state_error)?;
            let config = mutation(previous_config.clone()).normalized();

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
                sync_autostart(app, config.autostart.enabled)?;
            }

            let config = match config::save(&config) {
                Ok(config) => config,
                Err(error) => {
                    if autostart_changed {
                        let _ = sync_autostart(app, previous_config.autostart.enabled);
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
            ensure_float_window(app, config.ui.float_button);
            Ok(config)
        })
    }
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
async fn update_app_config(
    app: AppHandle,
    config: config::AppConfig,
) -> CommandResult<config::AppConfig> {
    let update_app = app.clone();
    let config = tauri::async_runtime::spawn_blocking(move || {
        update_app_config_blocking(&update_app, config)
    })
    .await
    .map_err(|_| {
        command_error(
            "config_update_task_failed",
            "Settings update stopped unexpectedly",
        )
    })??;

    schedule_config_refresh(app);
    Ok(config)
}

fn update_app_config_blocking(
    app: &AppHandle,
    config: config::AppConfig,
) -> CommandResult<config::AppConfig> {
    app.state::<ConfigMutationCoordinator>()
        .update(app, |_| config)
}

fn schedule_config_refresh(app: AppHandle) {
    spawn_detached_blocking(move || {
        let engine = app.state::<UsageEngine>();
        if refresh_all_and_publish(&app, &engine).is_err() {
            log_startup_warning(StartupWarning::InitialUsageRefresh);
        }
    });
}

fn spawn_detached_blocking<F>(task: F)
where
    F: FnOnce() + Send + 'static,
{
    std::mem::drop(tauri::async_runtime::spawn_blocking(task));
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

/// Whether the desktop prefers a dark color scheme.
/// Asks the XDG settings portal: 0 = no preference, 1 = dark, 2 = light.
/// Defaults to dark on any failure, matching the canonical theme.
fn panel_prefers_dark() -> bool {
    let output = std::process::Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.portal.Desktop",
            "--object-path",
            "/org/freedesktop/portal/desktop",
            "--method",
            "org.freedesktop.portal.Settings.Read",
            "org.freedesktop.appearance",
            "color-scheme",
        ])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            !String::from_utf8_lossy(&out.stdout).contains("uint32 2")
        }
        _ => true,
    }
}

#[tauri::command]
fn get_system_theme() -> String {
    if panel_prefers_dark() {
        "dark".into()
    } else {
        "light".into()
    }
}

#[tauri::command]
fn get_display_state(engine: State<'_, UsageEngine>) -> CommandResult<UsageDisplayState> {
    engine.display_state().map_err(map_usage_state_error)
}

// Async so the SQLite reads stay off the main thread (sync commands block
// the whole UI in Tauri).
#[tauri::command]
async fn get_usage_history(
    history_store: State<'_, history::HistoryStore>,
    days: u32,
    utc_offset_seconds: i32,
) -> CommandResult<UsageHistoryReport> {
    let map_error =
        |_: String| command_error("usage_history_unavailable", "Usage history is unavailable");

    Ok(UsageHistoryReport {
        codex: history_store
            .daily_gauge(Service::Codex, days, utc_offset_seconds)
            .map_err(map_error)?,
        claude: history_store
            .daily_gauge(Service::Claude, days, utc_offset_seconds)
            .map_err(map_error)?,
        days,
        generated_at: usage::now_rfc3339(),
    })
}

/// Recent local-scan reports keyed by (days, utc_offset_seconds). The scan
/// parses every Codex/Claude session log on disk, so navigating between
/// Dashboard and History must not repeat it within a short window.
#[derive(Default)]
struct LocalUsageCache(Mutex<HashMap<(u32, i32), (Instant, LocalDailyUsageReport)>>);

const LOCAL_USAGE_CACHE_TTL: Duration = Duration::from_secs(60);

#[tauri::command]
async fn get_local_daily_usage(
    cache: State<'_, LocalUsageCache>,
    days: u32,
    utc_offset_seconds: i32,
) -> CommandResult<LocalDailyUsageReport> {
    let key = (days, utc_offset_seconds);
    if let Some((scanned_at, report)) = cache.0.lock().unwrap().get(&key) {
        if scanned_at.elapsed() < LOCAL_USAGE_CACHE_TTL {
            return Ok(report.clone());
        }
    }

    // The scan walks and parses every local session log; run it on a
    // blocking thread so the UI keeps rendering.
    let report =
        tauri::async_runtime::spawn_blocking(move || scan_local_daily_usage(days, utc_offset_seconds))
            .await
            .map_err(|_| {
                command_error("local_usage_scan_failed", "Local usage scan failed")
            })?;
    cache
        .0
        .lock()
        .unwrap()
        .insert(key, (Instant::now(), report.clone()));
    Ok(report)
}

fn scan_local_daily_usage(days: u32, utc_offset_seconds: i32) -> LocalDailyUsageReport {
    let now = usage::now_rfc3339();
    let (codex, codex_status) = match local_provider::CodexLocalProvider::from_default_root()
        .ok_or_else(|| "codex_home_unavailable".to_string())
        .and_then(|provider| provider.daily_token_usage(days, utc_offset_seconds, &now))
    {
        Ok(daily) => (daily, None),
        Err(status) => (Vec::new(), Some(status)),
    };
    let (claude, claude_status) = match local_provider::ClaudeLocalProvider::from_default_root()
        .ok_or_else(|| "claude_home_unavailable".to_string())
        .and_then(|provider| provider.daily_token_usage(days, utc_offset_seconds, &now))
    {
        Ok(daily) => (daily, None),
        Err(status) => (Vec::new(), Some(status)),
    };

    LocalDailyUsageReport {
        codex,
        claude,
        codex_status,
        claude_status,
        days,
        generated_at: now,
    }
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
    if !managed_browser_service(service) {
        return Err(command_error(
            "provider_action_unsupported",
            "Provider is deferred",
        ));
    }
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
async fn start_provider_login(
    app: AppHandle,
    service: Service,
) -> CommandResult<ProviderLoginStart> {
    tauri::async_runtime::spawn_blocking(move || start_provider_login_blocking(&app, service))
        .await
        .map_err(|_| command_error("login_task_failed", "Provider login task stopped unexpectedly"))?
}

fn start_provider_login_blocking(
    app: &AppHandle,
    service: Service,
) -> CommandResult<ProviderLoginStart> {
    if !managed_browser_service(service) {
        return Err(command_error(
            "managed_login_not_available",
            "Managed login is not available for this provider",
        ));
    }

    let engine = app.state::<UsageEngine>();
    let sessions = app.state::<browser_session::BrowserSessionManager>();
    let config = engine.config().map_err(map_usage_state_error)?;
    let app_data_dir = app.path().app_data_dir().map_err(map_app_data_dir_error)?;
    let now = usage::now_rfc3339();
    let plan = provider_login_start_plan(&config, &app_data_dir, service, now.clone())
        .map_err(map_browser_profile_error)?;
    let mut login = plan.login;
    let login_required_reason = if let Some(request) = plan.sidecar_request {
        match launch_playwright_sidecar_login(app, &sessions, &request) {
            Ok(_) => {
                login.status = LOGIN_STATUS_LAUNCHED.to_string();
                None
            }
            Err(_) => Some(LOGIN_REASON_SIDECAR_UNAVAILABLE),
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
    if !managed_browser_service(service) {
        return Err("Managed login is not available for this provider".to_string());
    }

    let paths = prepare_managed_browser_profiles(config, app_data_dir)?;
    let launch_request = paths.as_ref().map(|paths| {
        let profile_path = match service {
            Service::Codex => &paths.codex,
            Service::Claude => &paths.claude,
            Service::Grok | Service::Ollama => {
                unreachable!("unsupported managed browser service")
            }
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

    browser_session::configure_process_group(&mut command);
    let child = command
        .spawn()
        .map_err(|_| "Managed login sidecar is unavailable".to_string())?;
    let process_id = sessions.track_process(request.service, child, marker)?;
    let launch_result = (|| {
        let (mut stdin, stdout) = sessions.take_process_stdio(request.service)?;
        write_sidecar_launch_request(&mut stdin, request)?;
        let line = read_sidecar_stdout_line(stdout, PLAYWRIGHT_SIDECAR_ACK_TIMEOUT)?;
        browser_session::playwright_sidecar_launch_response(&line, request)?;
        Ok(())
    })();
    if let Err(error) = launch_result {
        return Err(stop_tracked_sidecar(sessions, request.service, error));
    }

    Ok(process_id)
}

fn write_sidecar_launch_request(
    stdin: &mut ChildStdin,
    request: &browser_session::PlaywrightSidecarLaunchRequest,
) -> Result<(), String> {
    let raw = serde_json::to_vec(request)
        .map_err(|_| "Could not serialize managed login sidecar request".to_string())?;
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

fn stop_tracked_sidecar(
    sessions: &browser_session::BrowserSessionManager,
    service: Service,
    error: String,
) -> String {
    let _ = sessions.stop_service(service, browser_session::PROFILE_STOP_TIMEOUT);
    error
}

#[tauri::command]
fn clear_cached_snapshots(
    app: AppHandle,
    engine: State<'_, UsageEngine>,
) -> CommandResult<UsageDisplayState> {
    let display_state = engine
        .clear_cached_snapshots()
        .map_err(map_snapshot_cache_error)?;
    if let Ok(app_data_dir) = app.path().app_data_dir() {
        let _ = snapshot_store::clear_in(&app_data_dir);
    }
    app.emit(usage::SNAPSHOTS_UPDATED_EVENT, &display_state)
        .map_err(map_event_emit_error)?;
    Ok(display_state)
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
    if !managed_browser_service(service) {
        return Err("Managed profile inspection is not available for this provider".to_string());
    }
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
        Service::Grok | Service::Ollama => {
            unreachable!("deferred services have no managed browser profile")
        }
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
        Service::Grok => "grok-profile",
        Service::Ollama => "ollama-profile",
    }
}

fn clear_provider_profile_for_service(
    app: &AppHandle,
    engine: &UsageEngine,
    sessions: &browser_session::BrowserSessionManager,
    service: Service,
) -> CommandResult<ClearedProviderProfile> {
    if !managed_browser_service(service) {
        return Err(command_error(
            "provider_action_unsupported",
            "Provider is deferred",
        ));
    }
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

/// Runs after every snapshots-updated emit: persists the usage trail and
/// plays threshold-crossing cues. Failures are deliberately non-fatal.
fn after_snapshots_updated(app: &AppHandle, display_state: &UsageDisplayState) {
    if let Some(store) = app.try_state::<history::HistoryStore>() {
        let _ = store.record(&display_state.snapshots, history::now_unix());
    }

    if let (Some(engine), Ok(app_data_dir)) = (
        app.try_state::<UsageEngine>(),
        app.path().app_data_dir(),
    ) {
        if let Ok(snapshots) = engine.raw_snapshots() {
            let _ = snapshot_store::save_in(&app_data_dir, &snapshots, &display_state.updated_at);
        }
    }

    play_threshold_cues(app, display_state);
}

fn play_threshold_cues(app: &AppHandle, display_state: &UsageDisplayState) {
    let Ok(config) = app.state::<UsageEngine>().config() else {
        return;
    };
    let Ok(app_data_dir) = app.path().app_data_dir() else {
        return;
    };
    let tracker = app.state::<CueTracker>();
    let Ok(mut last_below) = tracker.last_below.lock() else {
        return;
    };

    for snapshot in &display_state.snapshots {
        if snapshot.source == UsageSource::Fake {
            continue;
        }

        let Some(remaining) = snapshot.remaining_percent else {
            continue;
        };

        let below = remaining <= config.low_usage_threshold;
        let previous = last_below.insert(snapshot.service, below);

        if !config.ui.sounds {
            continue;
        }

        let crossed = match previous {
            Some(was_below) => was_below != below,
            None => below,
        };

        if crossed {
            let cue = if below {
                sounds::Cue::Warn
            } else {
                sounds::Cue::Recover
            };
            sounds::play(cue, &app_data_dir);
        }
    }
}

fn refresh_all_and_publish(
    app: &AppHandle,
    engine: &UsageEngine,
) -> Result<UsageDisplayState, String> {
    let display_state = engine.refresh_all_and_emit(app)?;
    after_snapshots_updated(app, &display_state);
    Ok(display_state)
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
    if !managed_browser_service(service) {
        return Err(command_error(
            "provider_refresh_unsupported",
            "Managed web refresh is not available for this provider",
        ));
    }

    engine
        .refresh_provider_source_with_snapshot(service, UsageSource::Web, |observed_at| {
            let response = headless_web_usage_response(app, engine, sessions, service)
                .map_err(|_| UsageProviderError::Internal)?;

            usage_snapshot_from_sidecar_usage_response(response, observed_at)
        })
        .map_err(map_provider_refresh_error)
}

/// Resolves the one official reading for a runtime service: a fresh CLI
/// check first, falling through to a managed-web attempt only when the CLI
/// reading is unavailable and the user has opted in to managed web.
fn refresh_official_reading(
    app: &AppHandle,
    engine: &UsageEngine,
    sessions: &browser_session::BrowserSessionManager,
    service: Service,
) -> CommandResult<UsageDisplayState> {
    let config = engine.config().map_err(map_usage_state_error)?;

    let cli_snapshot = if config.providers.cli_enabled {
        engine.refresh_cli_snapshot(service).ok().flatten()
    } else {
        None
    };

    if official_reading::managed_web_fallback_needed(
        config.providers.cli_enabled,
        config.providers.web_enabled,
        cli_snapshot.as_ref(),
    ) {
        return refresh_web_provider_headless(app, engine, sessions, service);
    }

    engine.display_state().map_err(map_usage_state_error)
}

fn refresh_due_web_provider_headless(
    app: &AppHandle,
    engine: &UsageEngine,
    sessions: &browser_session::BrowserSessionManager,
    service: Service,
) -> CommandResult<UsageDisplayState> {
    if !managed_browser_service(service) {
        return Err(command_error(
            "provider_refresh_unsupported",
            "Managed web refresh is not available for this provider",
        ));
    }

    engine
        .refresh_due_provider_source_with_snapshot(service, UsageSource::Web, |observed_at| {
            let response = headless_web_usage_response(app, engine, sessions, service)
                .map_err(|_| UsageProviderError::Internal)?;

            usage_snapshot_from_sidecar_usage_response(response, observed_at)
        })
        .map_err(map_provider_refresh_error)
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

    if !managed_browser_service(service) {
        return Err("Managed web refresh is not available for this provider".to_string());
    }

    let sidecar_request = web_usage_refresh_sidecar_request(&config, &app_data_dir, service)?;
    run_playwright_sidecar_usage_refresh(app, sessions, &sidecar_request)
}

fn web_usage_refresh_sidecar_request(
    config: &config::AppConfig,
    app_data_dir: &Path,
    service: Service,
) -> Result<browser_session::PlaywrightSidecarLaunchRequest, String> {
    if !managed_browser_service(service) {
        return Err("Managed web refresh is not available for this provider".to_string());
    }

    let paths = prepare_managed_browser_profiles(config, app_data_dir)?;
    let Some(paths) = paths else {
        return Err("Managed browser profile is not prepared".to_string());
    };
    let profile_path = match service {
        Service::Codex => &paths.codex,
        Service::Claude => &paths.claude,
        Service::Grok | Service::Ollama => unreachable!("unsupported managed browser service"),
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
    let marker = browser_session::BrowserSessionMarker::new(request.service);
    let (env_key, env_value) = marker.env_pair();
    command.env(env_key, env_value);
    browser_session::configure_process_group(&mut command);
    let child = command
        .spawn()
        .map_err(|_| "Managed usage sidecar is unavailable".to_string())?;
    sessions.track_process(request.service, child, marker)?;

    let response = (|| {
        let (mut stdin, stdout) = sessions.take_process_stdio(request.service)?;
        write_sidecar_launch_request(&mut stdin, request)?;
        let line = read_sidecar_stdout_line(stdout, PLAYWRIGHT_SIDECAR_ACK_TIMEOUT)?;
        browser_session::playwright_sidecar_usage_response(&line, request)
    })();
    let response = match response {
        Ok(response) => response,
        Err(error) => return Err(stop_tracked_sidecar(sessions, request.service, error)),
    };

    sessions
        .stop_service(request.service, Duration::ZERO)
        .map_err(|_| "Managed usage sidecar did not finish refresh".to_string())?;
    Ok(response)
}

fn usage_snapshot_from_sidecar_usage_response(
    response: browser_session::PlaywrightSidecarUsageResponse,
    observed_at: &str,
) -> Result<UsageSnapshot, UsageProviderError> {
    if !response.service.is_runtime() {
        return Err(UsageProviderError::Disabled);
    }
    let page_state = visible_page_state_from_sidecar(response.page_state.as_str())?;

    web_provider::parse_visible_usage(
        VisibleUsageInput {
            service: response.service,
            page_state,
            remaining_percent: response.remaining_percent,
            used_percent: response.used_percent,
            reset_at: response.reset_at,
            visible_fields: response.visible_fields,
            second_window: response.weekly.map(|window| VisibleWindowInput {
                remaining_percent: window.remaining_percent,
                used_percent: window.used_percent,
                reset_at: window.reset_at,
            }),
            fable_window: response.fable.map(|window| VisibleWindowInput {
                remaining_percent: window.remaining_percent,
                used_percent: window.used_percent,
                reset_at: window.reset_at,
            }),
            products: response
                .products
                .into_iter()
                .map(|product| VisibleProductInput {
                    product: product.product,
                    usage_percent: product.usage_percent,
                })
                .collect(),
        },
        observed_at,
    )
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

// Async so the blocking provider work runs off the main thread; a sync
// command would freeze the UI for the whole refresh.
#[tauri::command]
async fn refresh_usage(app: AppHandle) -> CommandResult<UsageDisplayState> {
    tauri::async_runtime::spawn_blocking(move || refresh_usage_blocking(&app))
        .await
        .map_err(|_| command_error("refresh_task_failed", "Usage refresh task stopped unexpectedly"))?
}

fn refresh_usage_blocking(app: &AppHandle) -> CommandResult<UsageDisplayState> {
    let engine = app.state::<UsageEngine>();
    let sessions = app.state::<browser_session::BrowserSessionManager>();

    emit_refresh_event(
        app,
        None,
        None,
        usage::REFRESH_STARTED_EVENT,
        UsageRefreshStatus::Started,
    )?;

    match refresh_all_with_headless_web(app, &engine, &sessions) {
        Ok(display_state) => {
            emit_provider_error_events(app, &display_state);
            emit_refresh_event(
                app,
                None,
                None,
                usage::REFRESH_FINISHED_EVENT,
                UsageRefreshStatus::Finished,
            )?;
            Ok(display_state)
        }
        Err(error) => {
            let _ = emit_refresh_event(
                app,
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

    // Official readings are resolved per service: a usable CLI reading
    // (just refreshed above) is always preferred, and managed browser
    // refresh only runs for a service whose CLI reading is unavailable, so
    // one service's failing CLI reading cannot suppress another's.
    for (service, enabled) in [
        (Service::Codex, config.enabled_services.codex),
        (Service::Claude, config.enabled_services.claude),
    ] {
        if !enabled {
            continue;
        }

        let cli_snapshot = engine.cli_snapshot(service).unwrap_or(None);
        if official_reading::managed_web_fallback_needed(
            config.providers.cli_enabled,
            config.providers.web_enabled,
            cli_snapshot.as_ref(),
        ) {
            if let Ok(updated) = refresh_web_provider_headless(app, engine, sessions, service) {
                display_state = updated;
            }
        }
    }

    app.emit(usage::SNAPSHOTS_UPDATED_EVENT, &display_state)
        .map_err(map_event_emit_error)?;
    after_snapshots_updated(app, &display_state);

    Ok(display_state)
}

fn refresh_due_with_headless_web(
    app: &AppHandle,
    engine: &UsageEngine,
    sessions: &browser_session::BrowserSessionManager,
) -> CommandResult<UsageDisplayState> {
    let config = engine.config().map_err(map_usage_state_error)?;

    for (service, enabled) in [
        (Service::Codex, config.enabled_services.codex),
        (Service::Claude, config.enabled_services.claude),
    ] {
        if !enabled {
            continue;
        }

        let cli_snapshot = engine.cli_snapshot(service).map_err(map_usage_state_error)?;
        if official_reading::managed_web_fallback_needed(
            config.providers.cli_enabled,
            config.providers.web_enabled,
            cli_snapshot.as_ref(),
        ) {
            refresh_due_web_provider_headless(app, engine, sessions, service)?;
        }
    }

    let display_state = engine.refresh_due().map_err(map_usage_refresh_error)?;
    app.emit(usage::SNAPSHOTS_UPDATED_EVENT, &display_state)
        .map_err(map_event_emit_error)?;
    after_snapshots_updated(app, &display_state);

    Ok(display_state)
}

// Async for the same reason as refresh_usage: the web/CLI provider work
// must not block the main thread.
#[tauri::command]
async fn refresh_provider(
    app: AppHandle,
    service: Service,
    source: UsageSource,
) -> CommandResult<UsageDisplayState> {
    tauri::async_runtime::spawn_blocking(move || refresh_provider_blocking(&app, service, source))
        .await
        .map_err(|_| command_error("refresh_task_failed", "Provider refresh task stopped unexpectedly"))?
}

fn refresh_provider_blocking(
    app: &AppHandle,
    service: Service,
    source: UsageSource,
) -> CommandResult<UsageDisplayState> {
    if !managed_browser_service(service) {
        return Err(command_error(
            "provider_refresh_unsupported",
            "Provider is deferred",
        ));
    }
    let engine = app.state::<UsageEngine>();
    let sessions = app.state::<browser_session::BrowserSessionManager>();

    emit_refresh_event(
        app,
        Some(service),
        Some(source),
        usage::REFRESH_STARTED_EVENT,
        UsageRefreshStatus::Started,
    )?;

    let refresh_result = if source == UsageSource::Web {
        refresh_official_reading(app, &engine, &sessions, service)
    } else {
        engine
            .refresh_provider_source(service, source)
            .map_err(map_provider_refresh_error)
    };

    match refresh_result {
        Ok(display_state) => {
            app.emit(usage::SNAPSHOTS_UPDATED_EVENT, &display_state)
                .map_err(map_event_emit_error)?;
            after_snapshots_updated(app, &display_state);
            emit_provider_error_events(app, &display_state);
            emit_refresh_event(
                app,
                Some(service),
                Some(source),
                usage::REFRESH_FINISHED_EVENT,
                UsageRefreshStatus::Finished,
            )?;
            Ok(display_state)
        }
        Err(error) => {
            let _ = emit_refresh_event(
                app,
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
        Service::Grok => TRAY_GROK_ACCENT,
        Service::Ollama => TRAY_OLLAMA_ACCENT,
    }
}

fn tray_icon_for(state: usage::TrayGaugeState, low_usage_threshold: f32) -> Image<'static> {
    let rgba = tray_icon_rgba_for(state, low_usage_threshold)
        // Unknown reading: the same dark coin with an empty track ring, so the
        // icon stays visible on both light and dark panels.
        .unwrap_or_else(|| dynamic_tray_icon_rgba(state.service, 0.0, -1.0));

    Image::new_owned(rgba, TRAY_ICON_SIZE, TRAY_ICON_SIZE)
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
            .title("PickGauge")
            .inner_size(1000.0, 700.0)
            .min_inner_size(820.0, 580.0)
            .resizable(true)
            .center()
            .visible(false)
            .decorations(false)
            .background_color(tauri::window::Color(10, 10, 11, 255))
            .build()
            .ok()
    })
}

/// The floating capsule lives above every other window, never takes focus,
/// and opens the main window on click. Created lazily so disabling it in
/// settings simply hides the window.
fn ensure_float_window(app: &AppHandle, visible: bool) {
    if let Some(window) = app.get_webview_window(FLOAT_WINDOW_LABEL) {
        if visible {
            let _ = window.show();
        } else {
            let _ = window.hide();
        }
        return;
    }

    if !visible {
        return;
    }

    kwin::ensure_float_rule();
    let window = WebviewWindowBuilder::new(
        app,
        FLOAT_WINDOW_LABEL,
        WebviewUrl::App("index.html".into()),
    )
    .title("PickGauge Float")
    .inner_size(
        f64::from(FLOAT_WINDOW_WIDTH),
        f64::from(FLOAT_WINDOW_HEIGHT),
    )
    .min_inner_size(
        f64::from(FLOAT_WINDOW_WIDTH),
        f64::from(FLOAT_WINDOW_HEIGHT),
    )
    .max_inner_size(
        f64::from(FLOAT_WINDOW_WIDTH),
        f64::from(FLOAT_WINDOW_HEIGHT),
    )
    // Resizable + exact min/max hints: with the decoration CSS reset in
    // clamp_float_window_size, GTK honors these as a fixed size on
    // Wayland (non-resizable windows ignore programmatic resizes there).
    .resizable(true)
    .maximizable(false)
    .minimizable(false)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .focusable(false)
    .skip_taskbar(true)
    .visible_on_all_workspaces(true)
    .position(64.0, 64.0)
    .build();

    // The WebKitGTK child requests a 200x200 minimum, which GTK promotes to
    // the window min/max hints — leaving an invisible dead zone that swallows
    // clicks meant for windows beneath. Override the child request directly.
    if let Ok(window) = window {
        clamp_float_window_size(&window);
    }
}

// Shared Pickforge float-capsule geometry (kept in sync with PickScribe).
// On Linux the glow margin gives the capsule's box-shadow room to fade out
// inside the window, and the GTK input shape below keeps that transparent
// ring click-through. Other platforms have no input-shape equivalent, so
// they keep a snug window (and Float.svelte drops the outer glow there).
// Float.svelte's capsule margin must match FLOAT_GLOW_MARGIN per platform.
// Border-box width: 2px border + 10px left padding + 26px mark + 10px gap
// + 89px two-ring slot + 10px gap + 7px status dot + 14px right padding.
const FLOAT_CAPSULE_WIDTH: i32 = 168;
const FLOAT_CAPSULE_HEIGHT: i32 = 56;
#[cfg(target_os = "linux")]
const FLOAT_GLOW_MARGIN: i32 = 24;
#[cfg(not(target_os = "linux"))]
const FLOAT_GLOW_MARGIN: i32 = 2;
const FLOAT_WINDOW_WIDTH: i32 = FLOAT_CAPSULE_WIDTH + 2 * FLOAT_GLOW_MARGIN;
const FLOAT_WINDOW_HEIGHT: i32 = FLOAT_CAPSULE_HEIGHT + 2 * FLOAT_GLOW_MARGIN;

/// GTK won't size the capsule correctly on its own: WebKitGTK requests a
/// 200x200 minimum on X11, and on Wayland resizes issued before the surface
/// is mapped are dropped, collapsing the window to the webview's tiny natural
/// height. Clamp immediately and again shortly after mapping.
#[cfg(target_os = "linux")]
fn clamp_float_window_size(window: &WebviewWindow) {
    fn clamp_now(window: &WebviewWindow) {
        let window_handle = window.clone();
        let _ = window.run_on_main_thread(move || {
            use gtk::prelude::*;

            if let Ok(gtk_window) = window_handle.gtk_window() {
                // Floor the toplevel itself: the webview sits in a container
                // whose natural size is 0, so a child request alone does not
                // propagate on Wayland.
                gtk_window.set_size_request(FLOAT_WINDOW_WIDTH, FLOAT_WINDOW_HEIGHT);
                if let Some(child) = gtk_window.child() {
                    child.set_size_request(FLOAT_WINDOW_WIDTH, FLOAT_WINDOW_HEIGHT);
                }

                gtk_window.resize(FLOAT_WINDOW_WIDTH, FLOAT_WINDOW_HEIGHT);
                if let Some(gdk_window) = gtk_window.window() {
                    // 2px slack so the capsule border stays clickable.
                    let rect = gtk::cairo::RectangleInt::new(
                        FLOAT_GLOW_MARGIN - 2,
                        FLOAT_GLOW_MARGIN - 2,
                        FLOAT_CAPSULE_WIDTH + 4,
                        FLOAT_CAPSULE_HEIGHT + 4,
                    );
                    let region = gtk::cairo::Region::create_rectangle(&rect);
                    gdk_window.input_shape_combine_region(&region, 0, 0);
                }
            }
        });
    }

    // GTK reserves invisible CSD shadow/resize margins (~26px per side) on
    // undecorated Wayland windows, shrinking the visible capsule by 52px in
    // each axis. Strip the decoration node — but only for this window: a
    // screen-wide reset also desyncs the main window's CSD hit-testing, so
    // its titlebar buttons stop responding until a maximize re-syncs them.
    {
        let window_handle = window.clone();
        let _ = window.run_on_main_thread(move || {
            use gtk::prelude::*;

            if let Ok(gtk_window) = window_handle.gtk_window() {
                gtk_window.set_widget_name("pickforge-float");
            }
            let provider = gtk::CssProvider::new();
            let _ = provider.load_from_data(
                b"window#pickforge-float decoration{box-shadow:none;margin:0;padding:0;border:none;border-radius:0;}",
            );
            if let Some(screen) = gtk::gdk::Screen::default() {
                gtk::StyleContext::add_provider_for_screen(
                    &screen,
                    &provider,
                    gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                );
            }
        });
    }

    clamp_now(window);
    // GTK3 CSD quirk: geometry hints are interpreted including the invisible
    // shadow margins while resize() works on content size, so a fixed hint
    // clamps the content too small (by the shadow size, theme-dependent).
    // Feedback loop: measure the content-size error and grow the hints until
    // the content settles at exactly the target.
    let window = window.clone();
    std::thread::spawn(move || {
        let compensation = std::sync::Arc::new(std::sync::Mutex::new((0i32, 0i32)));
        for _ in 0..30 {
            std::thread::sleep(Duration::from_millis(250));
            let handle = window.clone();
            let compensation = std::sync::Arc::clone(&compensation);
            let _ = window.run_on_main_thread(move || {
                use gtk::prelude::*;

                let Ok(gtk_window) = handle.gtk_window() else {
                    return;
                };
                let (content_w, content_h) = gtk_window.size();
                if content_w == FLOAT_WINDOW_WIDTH && content_h == FLOAT_WINDOW_HEIGHT {
                    return;
                }
                let mut comp = compensation.lock().unwrap();
                comp.0 = (comp.0 + FLOAT_WINDOW_WIDTH - content_w).clamp(0, 200);
                comp.1 = (comp.1 + FLOAT_WINDOW_HEIGHT - content_h).clamp(0, 200);
                let total_w = FLOAT_WINDOW_WIDTH + comp.0;
                let total_h = FLOAT_WINDOW_HEIGHT + comp.1;
                let geometry = gtk::gdk::Geometry::new(
                    total_w,
                    total_h,
                    total_w,
                    total_h,
                    0,
                    0,
                    0,
                    0,
                    0f64,
                    0f64,
                    gtk::gdk::Gravity::Center,
                );
                gtk_window.set_geometry_hints(
                    None::<&gtk::Window>,
                    Some(&geometry),
                    gtk::gdk::WindowHints::MIN_SIZE | gtk::gdk::WindowHints::MAX_SIZE,
                );
                gtk_window.resize(total_w, total_h);
            });
        }
    });
}

#[cfg(not(target_os = "linux"))]
fn clamp_float_window_size(window: &WebviewWindow) {
    let _ = window.set_size(dpi::LogicalSize::new(
        f64::from(FLOAT_WINDOW_WIDTH),
        f64::from(FLOAT_WINDOW_HEIGHT),
    ));
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

// tao's Wayland CSD wraps the header bar in a GtkEventBox with
// above-child input, which swallows clicks on the minimize/maximize/close
// buttons until a maximize/restore cycle re-stacks the input windows
// (tauri-apps/tao#1218). Lower the box below its child so the buttons get
// their events back.
#[cfg(target_os = "linux")]
fn fix_csd_titlebar_input(window: &WebviewWindow) {
    let handle = window.clone();
    let _ = window.run_on_main_thread(move || {
        use gtk::prelude::*;

        if let Ok(gtk_window) = handle.gtk_window() {
            if let Some(titlebar) = gtk_window.titlebar() {
                if let Some(event_box) = titlebar.downcast_ref::<gtk::EventBox>() {
                    event_box.set_above_child(false);
                }
            }
        }
    });
}

fn present_main_window(app: &tauri::AppHandle) {
    let window = main_window(app);

    if let Some(window) = window {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn toggle_main_window_near(app: &tauri::AppHandle, anchor: PopupAnchor) {
    if let Some(window) = main_window(app) {
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

#[tauri::command]
fn show_main_window(app: AppHandle) -> CommandResult<WindowVisibility> {
    present_main_window(&app);

    Ok(WindowVisibility {
        status: "visible".to_string(),
        updated_at: usage::now_rfc3339(),
    })
}

fn apply_float_button_toggle(app: &AppHandle) -> Option<bool> {
    app.state::<ConfigMutationCoordinator>()
        .update(app, |mut config| {
            config.ui.float_button = !config.ui.float_button;
            config
        })
        .ok()
        .map(|config| config.ui.float_button)
}

#[tauri::command]
fn toggle_float_button(app: AppHandle) -> CommandResult<bool> {
    apply_float_button_toggle(&app).ok_or_else(|| {
        command_error(
            "float_toggle_failed",
            "Could not toggle the floating button",
        )
    })
}

fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "show", "Show PickGauge", true, None::<&str>)?;
    let float_item = MenuItem::with_id(
        app,
        "toggle-float",
        "Show/hide floating button",
        true,
        None::<&str>,
    )?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &float_item, &quit_item])?;
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
        .tooltip("PickGauge")
        .icon(tray_icon_for(initial_state, config.low_usage_threshold))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => present_main_window(app),
            "toggle-float" => {
                let _ = apply_float_button_toggle(app);
            }
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
    if let Some(exit_code) = usage_cli::try_run_from_env() {
        std::process::exit(exit_code);
    }

    run_tray();
}

fn run_tray() {
    let context = tauri::generate_context!();
    let release = format!(
        "pickgauge@{}",
        context
            .config()
            .version
            .clone()
            .expect("version in tauri.conf.json")
    );
    let sentry_config = config::load_existing_or_default();
    let sentry_enabled = sentry_config.crash_reports
        && (!cfg!(debug_assertions)
            || std::env::var("PICKGAUGE_SENTRY_DEBUG").ok().as_deref() == Some("1"));
    let sentry_client = sentry::init((
        if sentry_enabled { SENTRY_DSN } else { "" },
        sentry::ClientOptions {
            release: Some(release.into()),
            before_send: Some(Arc::new(|event| Some(sanitize_sentry_event(event)))),
            ..Default::default()
        },
    ));
    let _minidump_guard = if sentry_enabled {
        match tauri_plugin_sentry::minidump::init(&sentry_client) {
            Ok(guard) => Some(guard),
            Err(error) => {
                eprintln!("PickGauge Sentry minidump init failed: {error}");
                None
            }
        }
    } else {
        None
    };
    let sentry_plugin = if sentry_enabled {
        tauri_plugin_sentry::init(&sentry_client)
    } else {
        tauri_plugin_sentry::init_with_no_injection(&sentry_client)
    };

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            clear_cached_snapshots,
            get_app_config,
            get_display_state,
            get_local_daily_usage,
            get_log_location,
            get_system_theme,
            get_usage_history,
            hide_main_window,
            inspect_provider_profile,
            open_official_usage_page,
            refresh_provider,
            refresh_usage,
            reset_provider_session,
            show_main_window,
            start_provider_login,
            toggle_float_button,
            update_app_config
        ])
        .plugin(sentry_plugin)
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let app_handle = app.handle().clone();
            let (config, config_error) = match config::load() {
                Ok(config) => (config, None),
                Err(error) => (config::AppConfig::default(), Some(error)),
            };

            app.manage(ConfigLoadState::new(config_error));
            app.manage(ConfigMutationCoordinator::default());
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
            let float_button_enabled = config.ui.float_button;
            app.manage(UsageEngine::new(config));
            app.manage(CueTracker::default());
            app.manage(LocalUsageCache::default());
            match app_handle.path().app_data_dir() {
                Ok(app_data_dir) => match history::HistoryStore::open_in(&app_data_dir) {
                    Ok(store) => {
                        app.manage(store);
                    }
                    Err(_) => log_startup_warning(StartupWarning::UsageHistoryStore),
                },
                Err(_) => log_startup_warning(StartupWarning::UsageHistoryStore),
            }
            if refresh_all_and_publish(&app_handle, &app.state::<UsageEngine>()).is_err() {
                log_startup_warning(StartupWarning::InitialUsageRefresh);
            }
            setup_tray(app)?;
            start_usage_scheduler(app_handle.clone());
            ensure_float_window(&app_handle, float_button_enabled);
            #[cfg(target_os = "linux")]
            if let Some(window) = app.get_webview_window("main") {
                fix_csd_titlebar_input(&window);
            }
            Ok(())
        })
        .build(context)
        .expect("error while building PickGauge")
        .run(|app_handle, event| match event {
            tauri::RunEvent::WindowEvent {
                label,
                event: WindowEvent::CloseRequested { api, .. },
                ..
            } if label == "main" => {
                api.prevent_close();
                hide_main_window_if_exists(app_handle);
            }
            tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } => {
                api.prevent_exit();
            }
            tauri::RunEvent::Exit => {
                let sessions = app_handle.state::<browser_session::BrowserSessionManager>();
                let _ = sessions.stop_all(browser_session::PROFILE_STOP_TIMEOUT);
            }
            _ => {}
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        borrow::Cow,
        path::PathBuf,
        sync::atomic::{AtomicU64, AtomicUsize, Ordering},
    };

    static NEXT_TEST_DIR_ID: AtomicU64 = AtomicU64::new(0);

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "pickgauge-lib-test-{}-{}",
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
    fn sentry_event_sanitizer_strips_debug_image_paths() {
        let apple_uuid = "2df005a8-67ab-4d33-98f2-52f9f6de4d15";
        let symbolic_id = "494f3aea-88fa-4296-9644-fa8ef5d139b6-1234";
        let wasm_id = "8c954262-f905-4992-8a61-f60825f4553b";
        let event = sentry::protocol::Event {
            server_name: Some("workstation".into()),
            breadcrumbs: vec![sentry::protocol::Breadcrumb::default()].into(),
            debug_meta: Cow::Owned(sentry::protocol::DebugMeta {
                images: vec![
                    sentry::protocol::AppleDebugImage {
                        name: "/Users/alice/Applications/PickGauge.app/Contents/MacOS/PickGauge"
                            .into(),
                        arch: Some("arm64".into()),
                        cpu_type: Some(16_777_228),
                        cpu_subtype: Some(0),
                        image_addr: 4096.into(),
                        image_size: 8192,
                        image_vmaddr: 12288.into(),
                        uuid: apple_uuid.parse().unwrap(),
                    }
                    .into(),
                    sentry::protocol::SymbolicDebugImage {
                        name: "/home/alice/Applications/PickGauge.AppImage".into(),
                        arch: Some("x86_64".into()),
                        image_addr: 0.into(),
                        image_size: 4096,
                        image_vmaddr: 0.into(),
                        id: symbolic_id.parse().unwrap(),
                        code_id: None,
                        debug_file: Some("C:\\Users\\alice\\pickgauge.debug".into()),
                    }
                    .into(),
                    sentry::protocol::WasmDebugImage {
                        name: "pickgauge_bg.wasm".into(),
                        debug_id: wasm_id.parse().unwrap(),
                        debug_file: Some("/home/alice/debug/pickgauge_bg.wasm.debug".into()),
                        code_id: Some("abc123".into()),
                        code_file: "C:\\Users\\alice\\pickgauge_bg.wasm".into(),
                    }
                    .into(),
                ],
                ..Default::default()
            }),
            ..Default::default()
        };

        let event = sanitize_sentry_event(event);

        assert_eq!(event.server_name, None);
        assert!(event.breadcrumbs.is_empty());
        match &event.debug_meta.images[0] {
            sentry::protocol::DebugImage::Apple(image) => {
                assert_eq!(image.name, "PickGauge");
                assert_eq!(image.arch.as_deref(), Some("arm64"));
                assert_eq!(image.cpu_type, Some(16_777_228));
                assert_eq!(image.cpu_subtype, Some(0));
                assert_eq!(image.image_addr, 4096.into());
                assert_eq!(image.image_size, 8192);
                assert_eq!(image.image_vmaddr, 12288.into());
                assert_eq!(image.uuid.to_string(), apple_uuid);
            }
            _ => panic!("expected apple debug image"),
        }
        match &event.debug_meta.images[1] {
            sentry::protocol::DebugImage::Symbolic(image) => {
                assert_eq!(image.name, "PickGauge.AppImage");
                assert_eq!(image.debug_file.as_deref(), Some("pickgauge.debug"));
                assert_eq!(image.id.to_string(), symbolic_id);
            }
            _ => panic!("expected symbolic debug image"),
        }
        match &event.debug_meta.images[2] {
            sentry::protocol::DebugImage::Wasm(image) => {
                assert_eq!(image.code_file, "pickgauge_bg.wasm");
                assert_eq!(
                    image.debug_file.as_deref(),
                    Some("pickgauge_bg.wasm.debug")
                );
                assert_eq!(image.debug_id.to_string(), wasm_id);
            }
            _ => panic!("expected wasm debug image"),
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
        assert!(!paths.root.join("grok").exists());
        assert!(!paths.root.join("ollama").exists());

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
    fn deferred_profile_inspection_does_not_prepare_legacy_paths() {
        let dir = TestDir::new();
        let mut config = config::AppConfig::default();
        config.providers.web_enabled = true;
        config.browser_profiles.grok_path = Some("relative-grok-profile".to_string());
        config.browser_profiles.ollama_path = Some("relative-ollama-profile".to_string());

        for service in [Service::Grok, Service::Ollama] {
            assert!(inspect_provider_profile_for_service(
                &config,
                &dir.path,
                service,
                "2026-07-12T12:00:00Z".to_string(),
            )
            .is_err());
        }

        assert!(!dir.path.join("browser-profiles").exists());
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
    fn deferred_sidecar_responses_are_rejected_without_snapshots() {
        for service in [Service::Grok, Service::Ollama] {
            let response = sidecar_usage_response(service, "usage");
            assert_eq!(
                usage_snapshot_from_sidecar_usage_response(
                    response,
                    "2026-07-12T12:00:00Z",
                ),
                Err(UsageProviderError::Disabled)
            );
        }
    }

    #[test]
    fn claude_sidecar_usage_response_carries_fable_window() {
        let response = browser_session::PlaywrightSidecarUsageResponse {
            remaining_percent: Some(82.0),
            used_percent: Some(18.0),
            visible_fields: vec![
                "remaining_percent".to_string(),
                "used_percent".to_string(),
                "quota_window".to_string(),
            ],
            weekly: Some(browser_session::PlaywrightSidecarUsageWindow {
                remaining_percent: Some(57.0),
                used_percent: Some(43.0),
                reset_at: None,
            }),
            fable: Some(browser_session::PlaywrightSidecarUsageWindow {
                remaining_percent: Some(88.0),
                used_percent: Some(12.0),
                reset_at: None,
            }),
            ..sidecar_usage_response(Service::Claude, "usage")
        };

        let snapshot = usage_snapshot_from_sidecar_usage_response(response, "2026-06-04T12:00:00Z")
            .expect("snapshot is built");
        let windows = &snapshot.details["windows"];

        assert_eq!(windows["fiveHour"]["usedPercent"], 18.0);
        assert_eq!(windows["week"]["remainingPercent"], 57.0);
        assert_eq!(windows["fable"]["usedPercent"], 12.0);
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
            weekly: None,
            fable: None,
            products: Vec::new(),
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
    fn config_mutation_coordinator_serializes_updates() {
        let coordinator = Arc::new(ConfigMutationCoordinator::default());
        let value = Arc::new(AtomicUsize::new(0));
        let mut workers = Vec::new();

        for _ in 0..4 {
            let coordinator = Arc::clone(&coordinator);
            let value = Arc::clone(&value);
            workers.push(thread::spawn(move || {
                coordinator
                    .serialized(|| {
                        let current = value.load(Ordering::SeqCst);
                        thread::sleep(Duration::from_millis(10));
                        value.store(current + 1, Ordering::SeqCst);
                        Ok(())
                    })
                    .expect("config mutation succeeds");
            }));
        }

        for worker in workers {
            worker.join().expect("config mutation worker completes");
        }

        assert_eq!(value.load(Ordering::SeqCst), 4);
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
    fn managed_login_and_web_refresh_allow_only_codex_and_claude() {
        let dir = TestDir::new();
        let mut config = config::AppConfig::default();
        config.providers.web_enabled = true;

        for service in [Service::Codex, Service::Claude] {
            assert!(managed_browser_service(service));
            assert!(provider_login_start_plan(
                &config,
                &dir.path,
                service,
                "2026-07-12T12:00:00Z".to_string(),
            )
            .is_ok());
            assert!(web_usage_refresh_sidecar_request(&config, &dir.path, service).is_ok());
        }

        for service in [Service::Grok, Service::Ollama] {
            assert!(!managed_browser_service(service));
            assert!(provider_login_start_plan(
                &config,
                &dir.path,
                service,
                "2026-07-12T12:00:00Z".to_string(),
            )
            .is_err());
            assert!(web_usage_refresh_sidecar_request(&config, &dir.path, service).is_err());
        }

        assert!(!dir.path.join("browser-profiles/grok").exists());
        assert!(!dir.path.join("browser-profiles/ollama").exists());
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
            path: "/tmp/pickgauge.log".to_string(),
            exists: false,
            redaction_policy: LOG_REDACTION_POLICY_PATH.to_string(),
            updated_at: "2026-06-03T00:00:00Z".to_string(),
        };
        let value = serde_json::to_value(location).expect("log location serializes");

        assert_eq!(
            value,
            serde_json::json!({
                "path": "/tmp/pickgauge.log",
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
        assert!(tray_icon_rgba_for(tray_state(Service::Grok, None), 20.0).is_none());
    }

    #[test]
    fn config_refresh_scheduler_returns_before_refresh_finishes() {
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();

        spawn_detached_blocking(move || {
            started_tx.send(()).expect("start signal sends");
            release_rx.recv().expect("refresh release arrives");
        });

        started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("detached refresh starts");
        release_tx
            .send(())
            .expect("caller remains responsive while refresh is running");
    }

    #[test]
    fn floating_window_geometry_fits_two_rings_in_the_168px_capsule() {
        const RING_DIAMETER: i32 = 34;
        const RING_GAP: i32 = 8;
        const RING_SLOT_WIDTH: i32 = 89;
        const FIXED_CONTENT_WIDTH: i32 = 2 + 10 + 26 + 10 + RING_SLOT_WIDTH + 10 + 7 + 14;

        assert_eq!(2 * RING_DIAMETER + RING_GAP, 76);
        assert_eq!(RING_SLOT_WIDTH - (2 * RING_DIAMETER + RING_GAP), 13);
        assert_eq!(FIXED_CONTENT_WIDTH, 168);
        assert_eq!(FLOAT_CAPSULE_WIDTH, FIXED_CONTENT_WIDTH);
        assert_eq!(FLOAT_CAPSULE_HEIGHT, 56);
        assert_eq!(
            FLOAT_WINDOW_WIDTH,
            FLOAT_CAPSULE_WIDTH + 2 * FLOAT_GLOW_MARGIN
        );
        assert_eq!(
            FLOAT_WINDOW_HEIGHT,
            FLOAT_CAPSULE_HEIGHT + 2 * FLOAT_GLOW_MARGIN
        );
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
