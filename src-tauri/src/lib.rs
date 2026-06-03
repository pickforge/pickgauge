mod browser_profile;
mod config;
pub mod local_provider;
pub mod usage;

use std::sync::Mutex;
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

use tauri_plugin_opener::OpenerExt;

const SETTINGS_UPDATED_EVENT: &str = "settings://updated";
const CODEX_USAGE_URL: &str = "https://chatgpt.com/codex/cloud/settings/analytics";
const CLAUDE_USAGE_URL: &str = "https://claude.ai/new#settings/usage";
const TRAY_CODEX: &[u8] = include_bytes!("../../assets/branding/tray-codex-64.png");
const TRAY_CLAUDE: &[u8] = include_bytes!("../../assets/branding/tray-claude-64.png");
const TRAY_LOW: &[u8] = include_bytes!("../../assets/branding/tray-low-64.png");
const TRAY_UNKNOWN: &[u8] = include_bytes!("../../assets/branding/tray-unknown-64.png");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrayIconAsset {
    Codex,
    Claude,
    Low,
    Unknown,
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
        "Could not prepare browser profiles".to_string()
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
        "Provider source cannot be refreshed directly" | "Provider is not configured" => error,
        _ => "Could not refresh provider".to_string(),
    };

    CommandError::new("provider_refresh_failed", message)
}

fn map_open_usage_page_error() -> CommandError {
    command_error(
        "open_usage_page_failed",
        "Could not open official usage page",
    )
}

fn official_usage_url(service: Service) -> &'static str {
    match service {
        Service::Codex => CODEX_USAGE_URL,
        Service::Claude => CLAUDE_USAGE_URL,
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
fn update_app_config(
    app: AppHandle,
    engine: State<'_, UsageEngine>,
    config_load: State<'_, ConfigLoadState>,
    config: config::AppConfig,
) -> CommandResult<config::AppConfig> {
    if browser_profile::should_prepare_browser_profiles(
        &config.browser_profiles,
        config.providers.web_enabled,
    ) {
        let app_data_dir = app.path().app_data_dir().map_err(map_app_data_dir_error)?;
        browser_profile::prepare_browser_profiles(&config.browser_profiles, &app_data_dir)
            .map_err(map_browser_profile_error)?;
    }

    let config = config::save(&app, &config).map_err(map_config_save_error)?;
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

impl TrayIconAsset {
    fn bytes(self) -> &'static [u8] {
        match self {
            Self::Codex => TRAY_CODEX,
            Self::Claude => TRAY_CLAUDE,
            Self::Low => TRAY_LOW,
            Self::Unknown => TRAY_UNKNOWN,
        }
    }
}

fn tray_icon_asset_for(state: usage::TrayGaugeState, low_usage_threshold: f32) -> TrayIconAsset {
    match state.remaining_percent {
        None => TrayIconAsset::Unknown,
        Some(percent) if percent <= low_usage_threshold => TrayIconAsset::Low,
        Some(_) => match state.service {
            Service::Codex => TrayIconAsset::Codex,
            Service::Claude => TrayIconAsset::Claude,
        },
    }
}

fn tray_icon_for(state: usage::TrayGaugeState, low_usage_threshold: f32) -> Image<'static> {
    Image::from_bytes(tray_icon_asset_for(state, low_usage_threshold).bytes())
        .expect("valid bundled tray icon")
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
            let refresh_result = engine.refresh_all_and_emit(&app);
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
            get_app_config,
            get_display_state,
            get_usage_snapshots,
            open_official_usage_page,
            refresh_provider,
            refresh_usage,
            update_app_config
        ])
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_handle = app.handle().clone();
            let (config, config_error) = match config::load(&app_handle) {
                Ok(config) => (config, None),
                Err(error) => (config::AppConfig::default(), Some(error)),
            };

            app.manage(ConfigLoadState::new(config_error));
            app.manage(UsageEngine::new(config));
            if let Err(error) = app.state::<UsageEngine>().refresh_all_and_emit(&app_handle) {
                eprintln!("Could not refresh initial usage state: {error}");
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
    fn unknown_tray_state_uses_unknown_icon_asset() {
        assert_eq!(
            tray_icon_asset_for(tray_state(Service::Codex, None), 20.0),
            TrayIconAsset::Unknown
        );
    }

    #[test]
    fn low_tray_state_uses_low_icon_asset_at_threshold() {
        assert_eq!(
            tray_icon_asset_for(tray_state(Service::Claude, Some(20.0)), 20.0),
            TrayIconAsset::Low
        );
    }

    #[test]
    fn codex_tray_state_uses_codex_icon_asset_above_threshold() {
        assert_eq!(
            tray_icon_asset_for(tray_state(Service::Codex, Some(21.0)), 20.0),
            TrayIconAsset::Codex
        );
    }

    #[test]
    fn claude_tray_state_uses_claude_icon_asset_above_threshold() {
        assert_eq!(
            tray_icon_asset_for(tray_state(Service::Claude, Some(21.0)), 20.0),
            TrayIconAsset::Claude
        );
    }
}
