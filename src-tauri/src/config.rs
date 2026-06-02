use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use tauri::{AppHandle, Manager};

const CONFIG_FILE_NAME: &str = "config.json";
const CONFIG_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub version: u32,
    pub enabled_services: ServiceToggles,
    pub providers: ProviderSettings,
    pub intervals: RefreshIntervals,
    pub low_usage_threshold: f32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceToggles {
    pub codex: bool,
    pub claude: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSettings {
    pub local_enabled: bool,
    pub web_enabled: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshIntervals {
    pub local_seconds: u64,
    pub web_minutes: u64,
    pub manual_web_refresh_cooldown_seconds: u64,
    pub gauge_switch_seconds: u64,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            enabled_services: ServiceToggles {
                codex: true,
                claude: true,
            },
            providers: ProviderSettings {
                local_enabled: true,
                web_enabled: false,
            },
            intervals: RefreshIntervals {
                local_seconds: 45,
                web_minutes: 30,
                manual_web_refresh_cooldown_seconds: 60,
                gauge_switch_seconds: 6,
            },
            low_usage_threshold: 20.0,
        }
    }
}

impl AppConfig {
    pub fn normalized(mut self) -> Self {
        self.version = CONFIG_VERSION;
        self.intervals.local_seconds = self.intervals.local_seconds.clamp(30, 60);
        self.intervals.web_minutes = self.intervals.web_minutes.clamp(15, 60);
        self.intervals.manual_web_refresh_cooldown_seconds =
            self.intervals.manual_web_refresh_cooldown_seconds.max(60);
        self.intervals.gauge_switch_seconds = self.intervals.gauge_switch_seconds.clamp(5, 10);
        self.low_usage_threshold = self.low_usage_threshold.clamp(1.0, 100.0);
        self
    }
}

fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join(CONFIG_FILE_NAME))
        .map_err(|error| format!("Could not resolve config path: {error}"))
}

pub fn load(app: &AppHandle) -> Result<AppConfig, String> {
    let path = config_path(app)?;

    if !path.exists() {
        let config = AppConfig::default();
        save(app, &config)?;
        return Ok(config);
    }

    let raw = fs::read_to_string(&path)
        .map_err(|error| format!("Could not read config file: {error}"))?;
    let config = serde_json::from_str::<AppConfig>(&raw)
        .map_err(|error| format!("Could not parse config file: {error}"))?
        .normalized();

    Ok(config)
}

pub fn save(app: &AppHandle, config: &AppConfig) -> Result<AppConfig, String> {
    let path = config_path(app)?;
    let config = config.clone().normalized();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create config directory: {error}"))?;
    }

    let raw = serde_json::to_string_pretty(&config)
        .map_err(|error| format!("Could not serialize config: {error}"))?;

    fs::write(&path, raw).map_err(|error| format!("Could not write config file: {error}"))?;

    Ok(config)
}
