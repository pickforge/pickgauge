mod config;
mod usage;

use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};
use usage::{Service, UsageSnapshot};

const TRAY_CODEX: &[u8] = include_bytes!("../../assets/branding/tray-codex-64.png");
const TRAY_CLAUDE: &[u8] = include_bytes!("../../assets/branding/tray-claude-64.png");
const TRAY_LOW: &[u8] = include_bytes!("../../assets/branding/tray-low-64.png");
const TRAY_UNKNOWN: &[u8] = include_bytes!("../../assets/branding/tray-unknown-64.png");

#[derive(Clone, Copy)]
struct GaugeState {
    service: Service,
    remaining_percent: Option<f32>,
}

#[tauri::command]
fn get_app_config(app: AppHandle) -> Result<config::AppConfig, String> {
    config::load(&app)
}

#[tauri::command]
fn update_app_config(
    app: AppHandle,
    config: config::AppConfig,
) -> Result<config::AppConfig, String> {
    config::save(&app, &config)
}

#[tauri::command]
fn get_usage_snapshots(app: AppHandle) -> Result<Vec<UsageSnapshot>, String> {
    let config = config::load(&app)?;
    Ok(usage::current_snapshots(&config))
}

fn gauge_states(config: &config::AppConfig) -> Vec<GaugeState> {
    let mut states = Vec::new();

    if config.enabled_services.codex {
        states.push(GaugeState {
            service: Service::Codex,
            remaining_percent: Some(72.0),
        });
    }

    if config.enabled_services.claude {
        states.push(GaugeState {
            service: Service::Claude,
            remaining_percent: Some(41.0),
        });
    }

    if states.is_empty() {
        states.push(GaugeState {
            service: Service::Codex,
            remaining_percent: None,
        });
    }

    states
}

fn tray_icon_for(state: GaugeState, low_usage_threshold: f32) -> Image<'static> {
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

fn start_gauge_rotation(tray: TrayIcon, app: AppHandle) {
    std::thread::spawn(move || {
        let mut index = 0usize;

        loop {
            let config = config::load(&app).unwrap_or_default();
            let states = gauge_states(&config);
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
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "show", "Show ForgeGauge", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &quit_item])?;
    let app_handle = app.handle().clone();
    let click_app_handle = app_handle.clone();
    let config = config::load(&app_handle).unwrap_or_default();
    let initial_state = gauge_states(&config)[0];

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
            get_usage_snapshots,
            update_app_config
        ])
        .setup(|app| {
            setup_tray(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running ForgeGauge");
}
