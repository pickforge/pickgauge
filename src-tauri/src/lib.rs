mod browser_profile;
mod config;
pub mod local_provider;
pub mod usage;
pub mod web_provider;

use std::{fs, path::Path, sync::Mutex};
use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, WindowEvent,
};
use usage::{
    Service, UsageDisplayState, UsageEngine, UsageProviderErrorEvent, UsageRefreshEvent,
    UsageRefreshStatus, UsageSnapshot, UsageSource,
};

use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_opener::OpenerExt;

const SETTINGS_UPDATED_EVENT: &str = "settings://updated";
const LOGIN_REQUIRED_EVENT: &str = "login://required";
const SESSION_RESET_EVENT: &str = "session://reset";
const CODEX_USAGE_URL: &str = "https://chatgpt.com/codex/cloud/settings/analytics";
const CLAUDE_USAGE_URL: &str = "https://claude.ai/new#settings/usage";
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

#[derive(Clone, Copy)]
enum StartupWarning {
    AutostartSyncFailed,
    InitialUsageRefreshFailed,
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
struct LogLocation {
    path: String,
    exists: bool,
    redaction_policy: String,
    updated_at: String,
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

fn map_autostart_error() -> CommandError {
    command_error("autostart_update_failed", "Could not update autostart")
}

fn startup_warning_message(warning: StartupWarning) -> &'static str {
    match warning {
        StartupWarning::AutostartSyncFailed => "ForgeGauge startup warning: autostart sync failed",
        StartupWarning::InitialUsageRefreshFailed => {
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
        browser_profile::prepare_browser_profiles(&config.browser_profiles, &app_data_dir)
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
fn start_provider_login(app: AppHandle, service: Service) -> CommandResult<ProviderLoginStart> {
    let now = usage::now_rfc3339();
    let url = official_usage_url(service).to_string();
    let login = ProviderLoginStart {
        service,
        url: url.clone(),
        status: "login_required".to_string(),
        started_at: now.clone(),
    };
    let event = LoginRequiredEvent {
        service,
        url,
        reason: "managed_login_not_available".to_string(),
        emitted_at: now,
    };

    app.emit(LOGIN_REQUIRED_EVENT, &event)
        .map_err(map_event_emit_error)?;

    Ok(login)
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
    service: Service,
) -> CommandResult<ClearedProviderProfile> {
    clear_provider_profile_for_service(&app, &engine, service)
}

#[tauri::command]
fn reset_provider_session(
    app: AppHandle,
    engine: State<'_, UsageEngine>,
    service: Service,
) -> CommandResult<ClearedProviderProfile> {
    let reset = clear_provider_profile_for_service(&app, &engine, service)?;
    app.emit(SESSION_RESET_EVENT, &reset)
        .map_err(map_event_emit_error)?;
    Ok(reset)
}

fn clear_provider_profile_for_service(
    app: &AppHandle,
    engine: &UsageEngine,
    service: Service,
) -> CommandResult<ClearedProviderProfile> {
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

#[tauri::command]
fn refresh_usage(
    app: AppHandle,
    engine: State<'_, UsageEngine>,
) -> CommandResult<UsageDisplayState> {
    emit_refresh_event(
        &app,
        None,
        None,
        usage::REFRESH_STARTED_EVENT,
        UsageRefreshStatus::Started,
    )?;

    match engine.refresh_all_and_emit(&app) {
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
            Err(map_usage_refresh_error(error))
        }
    }
}

#[tauri::command]
fn refresh_provider(
    app: AppHandle,
    engine: State<'_, UsageEngine>,
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

    match engine.refresh_provider_source(service, source) {
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
            Err(map_provider_refresh_error(error))
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
            let _ = emit_refresh_event(
                &app,
                None,
                None,
                usage::REFRESH_STARTED_EVENT,
                UsageRefreshStatus::Started,
            );
            let refresh_result = engine.refresh_due_and_emit(&app);
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

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn setup_window_lifecycle(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let window_on_close = window.clone();

        window.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window_on_close.hide();
            }
        });
    }
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
                ..
            } = event
            {
                show_main_window(&click_app_handle);
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
        .setup(|app| {
            let app_handle = app.handle().clone();
            let (config, config_error) = match config::load(&app_handle) {
                Ok(config) => (config, None),
                Err(error) => (config::AppConfig::default(), Some(error)),
            };

            app.manage(ConfigLoadState::new(config_error));
            if sync_autostart(&app_handle, config.autostart.enabled).is_err() {
                log_startup_warning(StartupWarning::AutostartSyncFailed);
            }
            app.manage(UsageEngine::new(config));
            if app
                .state::<UsageEngine>()
                .refresh_all_and_emit(&app_handle)
                .is_err()
            {
                log_startup_warning(StartupWarning::InitialUsageRefreshFailed);
            }
            setup_window_lifecycle(&app_handle);
            setup_tray(app)?;
            start_usage_scheduler(app_handle);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running ForgeGauge");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tray_state(service: Service, remaining_percent: Option<f32>) -> usage::TrayGaugeState {
        usage::TrayGaugeState {
            service,
            remaining_percent,
        }
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
            StartupWarning::AutostartSyncFailed,
            StartupWarning::InitialUsageRefreshFailed,
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
            started_at: "2026-06-03T00:00:00Z".to_string(),
        };
        let value = serde_json::to_value(login).expect("provider login start serializes");

        assert_eq!(
            value,
            serde_json::json!({
                "service": "codex",
                "url": "https://chatgpt.com/codex/cloud/settings/analytics",
                "status": "login_required",
                "startedAt": "2026-06-03T00:00:00Z"
            })
        );
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
