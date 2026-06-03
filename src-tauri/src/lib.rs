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
use usage::{Service, UsageDisplayState, UsageEngine, UsageSnapshot, UsageSource};

const SETTINGS_UPDATED_EVENT: &str = "settings://updated";
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
) -> Result<config::AppConfig, String> {
    if let Some(error) = config_load.current_error()? {
        return Err(format!(
            "Recovered with default settings after config load failed: {error}"
        ));
    }

    engine.config()
}

#[tauri::command]
fn update_app_config(
    app: AppHandle,
    engine: State<'_, UsageEngine>,
    config_load: State<'_, ConfigLoadState>,
    config: config::AppConfig,
) -> Result<config::AppConfig, String> {
    if browser_profile::should_prepare_browser_profiles(
        &config.browser_profiles,
        config.providers.web_enabled,
    ) {
        let app_data_dir = app
            .path()
            .app_data_dir()
            .map_err(|error| format!("Could not resolve app data directory: {error}"))?;
        browser_profile::prepare_browser_profiles(&config.browser_profiles, &app_data_dir)?;
    }

    let config = config::save(&app, &config)?;
    config_load.clear_error()?;
    engine.update_config(config.clone())?;
    app.emit(SETTINGS_UPDATED_EVENT, &config)
        .map_err(|error| format!("Could not emit settings update: {error}"))?;
    engine.refresh_all_and_emit(&app)?;
    Ok(config)
}

#[tauri::command]
fn get_usage_snapshots(engine: State<'_, UsageEngine>) -> Result<Vec<UsageSnapshot>, String> {
    engine.snapshots()
}

#[tauri::command]
fn get_display_state(engine: State<'_, UsageEngine>) -> Result<UsageDisplayState, String> {
    engine.display_state()
}

#[tauri::command]
fn clear_cached_snapshots(
    app: AppHandle,
    engine: State<'_, UsageEngine>,
) -> Result<UsageDisplayState, String> {
    let display_state = engine.clear_cached_snapshots()?;
    app.emit(usage::SNAPSHOTS_UPDATED_EVENT, &display_state)
        .map_err(|error| format!("Could not emit usage update: {error}"))?;
    Ok(display_state)
}

#[tauri::command]
fn refresh_usage(
    app: AppHandle,
    engine: State<'_, UsageEngine>,
) -> Result<UsageDisplayState, String> {
    engine.refresh_all_and_emit(&app)
}

#[tauri::command]
fn refresh_provider(
    app: AppHandle,
    engine: State<'_, UsageEngine>,
    service: Service,
    source: UsageSource,
) -> Result<UsageDisplayState, String> {
    let display_state = engine.refresh_provider_source(service, source)?;
    app.emit(usage::SNAPSHOTS_UPDATED_EVENT, &display_state)
        .map_err(|error| format!("Could not emit usage update: {error}"))?;
    Ok(display_state)
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
            let _ = engine.refresh_all_and_emit(&app);
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
            refresh_provider,
            refresh_usage,
            update_app_config
        ])
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
