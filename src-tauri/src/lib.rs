use serde::Serialize;
use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    Manager,
};

const TRAY_CODEX: &[u8] = include_bytes!("../../assets/branding/tray-codex-64.png");
const TRAY_CLAUDE: &[u8] = include_bytes!("../../assets/branding/tray-claude-64.png");
const TRAY_LOW: &[u8] = include_bytes!("../../assets/branding/tray-low-64.png");
const TRAY_UNKNOWN: &[u8] = include_bytes!("../../assets/branding/tray-unknown-64.png");

#[derive(Clone, Copy)]
enum Service {
    Codex,
    Claude,
}

#[derive(Clone, Copy)]
struct GaugeState {
    service: Service,
    remaining_percent: Option<f32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UsageSnapshot {
    service: &'static str,
    remaining_percent: Option<f32>,
    used_percent: Option<f32>,
    reset_at: Option<String>,
    source: &'static str,
    confidence: &'static str,
    last_updated: String,
    details: serde_json::Value,
}

#[tauri::command]
fn get_usage_snapshots() -> Vec<UsageSnapshot> {
    vec![
        UsageSnapshot {
            service: "codex",
            remaining_percent: Some(72.0),
            used_percent: Some(28.0),
            reset_at: None,
            source: "fake",
            confidence: "unknown",
            last_updated: "Waiting for local provider".to_string(),
            details: serde_json::json!({ "status": "placeholder" }),
        },
        UsageSnapshot {
            service: "claude",
            remaining_percent: Some(41.0),
            used_percent: Some(59.0),
            reset_at: None,
            source: "fake",
            confidence: "unknown",
            last_updated: "Waiting for local provider".to_string(),
            details: serde_json::json!({ "status": "placeholder" }),
        },
    ]
}

fn service_code(service: Service) -> &'static str {
    match service {
        Service::Codex => "Codex",
        Service::Claude => "Claude Code",
    }
}

fn gauge_states() -> [GaugeState; 2] {
    [
        GaugeState {
            service: Service::Codex,
            remaining_percent: Some(72.0),
        },
        GaugeState {
            service: Service::Claude,
            remaining_percent: Some(41.0),
        },
    ]
}

fn tray_icon_for(state: GaugeState) -> Image<'static> {
    let bytes = match state.remaining_percent {
        None => TRAY_UNKNOWN,
        Some(percent) if percent <= 20.0 => TRAY_LOW,
        Some(_) => match state.service {
            Service::Codex => TRAY_CODEX,
            Service::Claude => TRAY_CLAUDE,
        },
    };

    Image::from_bytes(bytes)
        .expect("valid bundled tray icon")
        .to_owned()
}

fn start_gauge_rotation(tray: TrayIcon) {
    std::thread::spawn(move || {
        let states = gauge_states();
        let mut index = 0usize;

        loop {
            let state = states[index % states.len()];
            let label = format!(
                "{}: {} remaining",
                service_code(state.service),
                state
                    .remaining_percent
                    .map(|percent| format!("{}%", percent.round()))
                    .unwrap_or_else(|| "unknown".to_string())
            );

            let _ = tray.set_icon(Some(tray_icon_for(state)));
            let _ = tray.set_tooltip(Some(label.as_str()));

            index += 1;
            std::thread::sleep(std::time::Duration::from_secs(6));
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
    let initial_state = gauge_states()[0];
    let app_handle = app.handle().clone();

    let tray = TrayIconBuilder::with_id("main")
        .tooltip("ForgeGauge")
        .icon(tray_icon_for(initial_state))
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
                show_main_window(&app_handle);
            }
        })
        .build(app)?;

    start_gauge_rotation(tray);

    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_usage_snapshots])
        .setup(|app| {
            setup_tray(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running ForgeGauge");
}
