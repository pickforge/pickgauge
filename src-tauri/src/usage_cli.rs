use crate::{
    config, snapshot_store,
    usage::{self, UsageConfidence, UsageDisplayState, UsageEngine, UsageSnapshot},
    usage_model::{UsageModel, UsageWindow},
};
use serde::Serialize;
use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    io::{self, Write},
    path::Path,
};

const USAGE_JSON_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UsageCommand {
    Human,
    Json,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UsageJsonResponse {
    version: u32,
    generated_at: String,
    services: Vec<UsageJsonService>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UsageJsonService {
    service: String,
    label: String,
    status: String,
    plan: Option<String>,
    remaining_percent: Option<f32>,
    used_percent: Option<f32>,
    reset_at: Option<String>,
    windows: UsageJsonWindows,
    source: String,
    confidence: UsageConfidence,
    last_updated: String,
    stale_seconds: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UsageJsonWindows {
    five_hour: Option<UsageJsonWindow>,
    week: Option<UsageJsonWindow>,
    fable: Option<UsageJsonWindow>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UsageJsonWindow {
    remaining_percent: Option<f32>,
    used_percent: Option<f32>,
    reset_at: Option<String>,
}

pub fn try_run_from_env() -> Option<i32> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    #[cfg(windows)]
    attach_parent_console_if_usage(&args);
    let command = match parse_args(&args) {
        Ok(command) => command,
        Err(()) => {
            print_help();
            return Some(2);
        }
    }?;

    Some(run_usage(command))
}

#[cfg(windows)]
fn attach_parent_console_if_usage(args: &[OsString]) {
    if args.first().map(OsString::as_os_str) != Some(OsStr::new("usage")) {
        return;
    }

    use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};

    unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

fn parse_args(args: &[OsString]) -> Result<Option<UsageCommand>, ()> {
    if args.is_empty() {
        return Ok(None);
    }

    if args[0].as_os_str() != OsStr::new("usage") {
        return Ok(None);
    }

    match args {
        [_] => Ok(Some(UsageCommand::Human)),
        [_, flag] if flag.as_os_str() == OsStr::new("--json") => Ok(Some(UsageCommand::Json)),
        _ => Err(()),
    }
}

fn run_usage(command: UsageCommand) -> i32 {
    let result = (|| -> Result<(), String> {
        let config = config::load_read_only()?;
        let engine = UsageEngine::new_headless(config);
        engine.refresh_all()?;

        let app_data_dir = snapshot_store::app_data_dir()?;
        let persisted_snapshots = load_persisted_snapshots(&app_data_dir);
        let display_state = engine.overlay_persisted_snapshots(persisted_snapshots)?;
        let generated_at = usage::now_rfc3339();
        let response = usage_json_response(&display_state, &generated_at);

        match command {
            UsageCommand::Human => write_human_table(&response),
            UsageCommand::Json => write_json(&response),
        }
    })();

    match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("pickgauge usage: {error}");
            1
        }
    }
}

fn load_persisted_snapshots(app_data_dir: &Path) -> HashMap<String, UsageSnapshot> {
    match snapshot_store::load_in(app_data_dir) {
        Ok(snapshots) => snapshots,
        Err(error) => {
            eprintln!("pickgauge usage: ignoring snapshot cache: {error}");
            HashMap::new()
        }
    }
}

fn usage_json_response(display_state: &UsageDisplayState, generated_at: &str) -> UsageJsonResponse {
    UsageJsonResponse {
        version: USAGE_JSON_VERSION,
        generated_at: generated_at.to_string(),
        services: display_state
            .snapshots
            .iter()
            .map(|snapshot| usage_json_service(snapshot, generated_at))
            .collect(),
    }
}

fn usage_json_service(snapshot: &UsageSnapshot, generated_at: &str) -> UsageJsonService {
    let model = UsageModel::from_snapshot(snapshot);

    UsageJsonService {
        service: snapshot.service.code().to_string(),
        label: snapshot.service.label().to_string(),
        status: model.status,
        plan: model.plan,
        remaining_percent: snapshot.remaining_percent,
        used_percent: snapshot.used_percent,
        reset_at: snapshot.reset_at.clone(),
        windows: UsageJsonWindows {
            five_hour: json_window(model.windows.five_hour),
            week: json_window(model.windows.week),
            fable: json_window(model.windows.fable),
        },
        source: snapshot.source.code().to_string(),
        confidence: snapshot.confidence,
        last_updated: snapshot.last_updated.clone(),
        stale_seconds: stale_seconds(&snapshot.last_updated, generated_at),
    }
}

fn json_window(window: Option<UsageWindow>) -> Option<UsageJsonWindow> {
    window.map(|window| UsageJsonWindow {
        remaining_percent: window.remaining_percent,
        used_percent: window.used_percent,
        reset_at: window.reset_at,
    })
}

fn stale_seconds(last_updated: &str, generated_at: &str) -> Option<u64> {
    let last_updated = time::OffsetDateTime::parse(
        last_updated,
        &time::format_description::well_known::Rfc3339,
    )
    .ok()?;
    let generated_at = time::OffsetDateTime::parse(
        generated_at,
        &time::format_description::well_known::Rfc3339,
    )
    .ok()?;

    if generated_at < last_updated {
        return Some(0);
    }

    u64::try_from((generated_at - last_updated).whole_seconds()).ok()
}

fn write_json(response: &UsageJsonResponse) -> Result<(), String> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    serde_json::to_writer(&mut stdout, response)
        .map_err(|error| format!("Could not write JSON output: {error}"))?;
    writeln!(stdout).map_err(|error| format!("Could not finish JSON output: {error}"))
}

fn write_human_table(response: &UsageJsonResponse) -> Result<(), String> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    writeln!(
        stdout,
        "{:<12} {:<16} {:<8} {:<8} {:<28} {:<8} {}",
        "Service", "Plan", "5h", "Week", "Resets", "Source", "Staleness"
    )
    .map_err(|error| format!("Could not write usage table: {error}"))?;

    for service in &response.services {
        let five_hour = service
            .windows
            .five_hour
            .as_ref()
            .and_then(|window| window.remaining_percent)
            .or_else(|| {
                service
                    .windows
                    .week
                    .is_none()
                    .then_some(service.remaining_percent)
                    .flatten()
            });
        writeln!(
            stdout,
            "{:<12} {:<16} {:<8} {:<8} {:<28} {:<8} {}",
            service.label,
            service.plan.as_deref().unwrap_or("—"),
            format_percent(five_hour),
            format_percent(service.windows.week.as_ref().and_then(|window| window.remaining_percent)),
            format_resets(&service.windows, service.reset_at.as_deref()),
            service.source,
            format_staleness(service.stale_seconds),
        )
        .map_err(|error| format!("Could not write usage table: {error}"))?;
    }

    Ok(())
}

fn format_percent(value: Option<f32>) -> String {
    value
        .map(|value| format!("{value:.0}%"))
        .unwrap_or_else(|| "—".to_string())
}

fn format_resets(windows: &UsageJsonWindows, reset_at: Option<&str>) -> String {
    let five_hour = windows
        .five_hour
        .as_ref()
        .and_then(|window| window.reset_at.as_deref())
        .map(|reset_at| format!("5h {}", reset_time(reset_at)));
    let week = windows
        .week
        .as_ref()
        .and_then(|window| window.reset_at.as_deref())
        .map(|reset_at| format!("wk {}", reset_date_time(reset_at)));

    let resets = [five_hour, week]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");

    if resets.is_empty() {
        return reset_at.unwrap_or("—").chars().take(28).collect();
    }

    resets
        .chars()
        .take(28)
        .collect::<String>()
}

fn reset_time(reset_at: &str) -> String {
    reset_at
        .get(11..16)
        .unwrap_or(reset_at)
        .to_string()
}

fn reset_date_time(reset_at: &str) -> String {
    reset_at
        .get(5..16)
        .unwrap_or(reset_at)
        .to_string()
}

fn format_staleness(value: Option<u64>) -> String {
    value
        .map(|value| format!("{value}s"))
        .unwrap_or_else(|| "—".to_string())
}

fn print_help() {
    eprintln!("Usage: pickgauge usage [--json]");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::{Service, UsageConfidence, UsageSource};

    #[test]
    fn parses_usage_command_variants_and_rejects_unknown_arguments() {
        assert_eq!(parse_args(&[]), Ok(None));
        assert_eq!(parse_args(&[OsString::from("--hidden")]), Ok(None));
        assert_eq!(
            parse_args(&[OsString::from("usage")]),
            Ok(Some(UsageCommand::Human))
        );
        assert_eq!(
            parse_args(&[OsString::from("usage"), OsString::from("--json")]),
            Ok(Some(UsageCommand::Json))
        );
        assert_eq!(
            parse_args(&[OsString::from("usage"), OsString::from("--yaml")]),
            Err(())
        );
        assert_eq!(
            parse_args(&[
                OsString::from("usage"),
                OsString::from("--json"),
                OsString::from("extra"),
            ]),
            Err(())
        );
    }

    #[test]
    fn cache_read_failure_falls_back_to_live_only_output() {
        let path = std::env::temp_dir().join(format!(
            "pickgauge-usage-cli-test-{}",
            std::process::id()
        ));
        let cache_path = path.join("snapshots.json");
        std::fs::create_dir_all(&cache_path).expect("blocking cache directory is created");

        assert!(load_persisted_snapshots(&path).is_empty());

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn serializes_usage_json_v1_golden_response() {
        let display_state = UsageDisplayState {
            snapshots: vec![
                UsageSnapshot {
                    service: Service::Codex,
                    remaining_percent: Some(79.0),
                    used_percent: Some(21.0),
                    reset_at: Some("2026-07-09T14:30:00Z".to_string()),
                    source: UsageSource::Web,
                    confidence: UsageConfidence::High,
                    last_updated: "2026-07-09T12:00:00Z".to_string(),
                    details: serde_json::json!({
                        "status": "parsed",
                        "plan": "Plus",
                        "windows": {
                            "fiveHour": {
                                "remainingPercent": 79.0,
                                "usedPercent": 21.0,
                                "resetAt": "2026-07-09T14:30:00Z"
                            },
                            "week": {
                                "remainingPercent": 91.0,
                                "usedPercent": 9.0,
                                "resetAt": null
                            }
                        }
                    }),
                },
                UsageSnapshot {
                    service: Service::Claude,
                    remaining_percent: Some(64.0),
                    used_percent: Some(36.0),
                    reset_at: Some("2026-07-09T14:00:00Z".to_string()),
                    source: UsageSource::Web,
                    confidence: UsageConfidence::High,
                    last_updated: "2026-07-09T12:00:00Z".to_string(),
                    details: serde_json::json!({
                        "status": "parsed",
                        "plan": "Pro",
                        "windows": {
                            "fiveHour": {
                                "remainingPercent": 64.0,
                                "usedPercent": 36.0,
                                "resetAt": "2026-07-09T14:00:00Z"
                            },
                            "week": {
                                "remainingPercent": 88.0,
                                "usedPercent": 12.0,
                                "resetAt": null
                            },
                            "fable": {
                                "remainingPercent": 52.0,
                                "usedPercent": 48.0,
                                "resetAt": "2026-07-11T12:00:00Z"
                            }
                        }
                    }),
                },
            ],
            updated_at: "2026-07-09T12:00:00Z".to_string(),
        };

        let response = usage_json_response(&display_state, "2026-07-09T12:00:05Z");
        let expected = serde_json::from_str::<serde_json::Value>(include_str!(
            "../tests/fixtures/usage-cli-v1.json"
        ))
        .expect("golden fixture parses");

        assert_eq!(
            serde_json::to_value(response).expect("usage JSON serializes"),
            expected
        );
    }
}
