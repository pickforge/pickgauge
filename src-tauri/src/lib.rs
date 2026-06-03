mod config;
mod usage;

use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, State, WindowEvent,
};
use usage::{Service, UsageDisplayState, UsageEngine, UsageSnapshot};

const TRAY_CODEX: &[u8] = include_bytes!("../../assets/branding/tray-codex-64.png");
const TRAY_CLAUDE: &[u8] = include_bytes!("../../assets/branding/tray-claude-64.png");
const TRAY_LOW: &[u8] = include_bytes!("../../assets/branding/tray-low-64.png");
const TRAY_UNKNOWN: &[u8] = include_bytes!("../../assets/branding/tray-unknown-64.png");

#[tauri::command]
fn get_app_config(engine: State<'_, UsageEngine>) -> Result<config::AppConfig, String> {
    engine.config()
}

#[tauri::command]
fn update_app_config(
    app: AppHandle,
    engine: State<'_, UsageEngine>,
    config: config::AppConfig,
) -> Result<config::AppConfig, String> {
    let config = config::save(&app, &config)?;
    engine.update_config(config.clone())?;
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
fn refresh_usage(
    app: AppHandle,
    engine: State<'_, UsageEngine>,
) -> Result<UsageDisplayState, String> {
    engine.refresh_all_and_emit(&app)
}

fn tray_icon_for(state: usage::TrayGaugeState, low_usage_threshold: f32) -> Image<'static> {
    let bytes = match state.remaining_percent {
        None => TRAY_UNKNOWN,
        Some(percent) if percent <= low_usage_threshold => TRAY_LOW,
        Some(_) => match state.service {
            Service::Codex => TRAY_CODEX,
            Service::Claude => TRAY_CLAUDE,
        },
    };

    Image::from_bytes(bytes)
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
            get_app_config,
            get_display_state,
            get_usage_snapshots,
            refresh_usage,
            update_app_config
        ])
        .setup(|app| {
            let app_handle = app.handle().clone();
            let config = config::load(&app_handle).unwrap_or_default();

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
