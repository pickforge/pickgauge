use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager};

const CONFIG_FILE_NAME: &str = "config.json";
const CONFIG_VERSION: u32 = 3;
const MAX_PLAN_LABEL_LENGTH: usize = 80;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub version: u32,
    pub enabled_services: ServiceToggles,
    pub providers: ProviderSettings,
    pub intervals: RefreshIntervals,
    pub low_usage_threshold: f32,
    pub browser_profiles: BrowserProfileSettings,
    pub local_quotas: LocalQuotaSettings,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceToggles {
    pub codex: bool,
    pub claude: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSettings {
    pub local_enabled: bool,
    pub web_enabled: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshIntervals {
    pub local_seconds: u64,
    pub web_minutes: u64,
    pub manual_web_refresh_cooldown_seconds: u64,
    pub gauge_switch_seconds: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserProfileSettings {
    pub root_path: Option<String>,
    pub codex_path: Option<String>,
    pub claude_path: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalQuotaSettings {
    pub codex: LocalServiceQuotaSettings,
    pub claude: LocalServiceQuotaSettings,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalServiceQuotaSettings {
    pub enabled: bool,
    pub plan_label: String,
    pub limit_kind: LocalQuotaLimitKind,
    pub window_hours: u64,
    pub usage_unit: LocalQuotaUsageUnit,
    pub limit: f64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LocalQuotaLimitKind {
    RollingWindow,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LocalQuotaUsageUnit {
    Tokens,
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
            browser_profiles: BrowserProfileSettings {
                root_path: None,
                codex_path: None,
                claude_path: None,
            },
            local_quotas: LocalQuotaSettings::default(),
        }
    }
}

impl Default for LocalServiceQuotaSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            plan_label: String::new(),
            limit_kind: LocalQuotaLimitKind::RollingWindow,
            window_hours: 5,
            usage_unit: LocalQuotaUsageUnit::Tokens,
            limit: 0.0,
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
        self.local_quotas.normalize();
        self
    }
}

impl LocalQuotaSettings {
    fn normalize(&mut self) {
        self.codex.normalize();
        self.claude.normalize();
    }
}

impl LocalServiceQuotaSettings {
    fn normalize(&mut self) {
        self.plan_label = self
            .plan_label
            .trim()
            .chars()
            .take(MAX_PLAN_LABEL_LENGTH)
            .collect();
        self.window_hours = self.window_hours.clamp(1, 744);

        if !self.limit.is_finite() || self.limit < 0.0 {
            self.limit = 0.0;
        }
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
    load_from_path(&path)
}

pub fn save(app: &AppHandle, config: &AppConfig) -> Result<AppConfig, String> {
    let path = config_path(app)?;
    save_to_path(&path, config)
}

fn load_from_path(path: &Path) -> Result<AppConfig, String> {
    if !path.exists() {
        let config = AppConfig::default();
        save_to_path(path, &config)?;
        return Ok(config);
    }

    let raw =
        fs::read_to_string(path).map_err(|error| format!("Could not read config file: {error}"))?;
    let raw_value = serde_json::from_str::<Value>(&raw)
        .map_err(|error| format!("Could not parse config file: {error}"))?
        .try_into_config_value()?;
    let config = serde_json::from_value::<AppConfig>(raw_value)
        .map_err(|error| format!("Could not deserialize config file: {error}"))?
        .normalized();

    Ok(config)
}

fn save_to_path(path: &Path, config: &AppConfig) -> Result<AppConfig, String> {
    let config = config.clone().normalized();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create config directory: {error}"))?;
    }

    let raw = serde_json::to_string_pretty(&config)
        .map_err(|error| format!("Could not serialize config: {error}"))?;

    write_atomic(path, raw.as_bytes())?;

    Ok(config)
}

trait ConfigValueMigration {
    fn try_into_config_value(self) -> Result<Value, String>;
}

impl ConfigValueMigration for Value {
    fn try_into_config_value(mut self) -> Result<Value, String> {
        let object = self
            .as_object_mut()
            .ok_or_else(|| "Config root must be a JSON object".to_string())?;
        let version = match object.get("version") {
            Some(value) => value
                .as_u64()
                .ok_or_else(|| "Config version must be an integer".to_string())?,
            None => 1,
        };

        if version == 0 {
            return Err("Config version 0 is not supported".to_string());
        }

        if version > u64::from(CONFIG_VERSION) {
            return Err(format!(
                "Config version {version} is newer than supported version {CONFIG_VERSION}"
            ));
        }

        object.insert("version".to_string(), Value::from(version));
        migrate_config_value(&mut self)?;
        fill_missing_defaults(&mut self)?;

        Ok(self)
    }
}

fn migrate_config_value(value: &mut Value) -> Result<(), String> {
    loop {
        let version = config_value_version(value)?;

        if version == CONFIG_VERSION {
            return Ok(());
        }

        match version {
            1 => migrate_v1_to_v2(value)?,
            2 => migrate_v2_to_v3(value)?,
            _ => {
                return Err(format!(
                    "No migration is available for config version {version}"
                ));
            }
        }
    }
}

fn migrate_v1_to_v2(value: &mut Value) -> Result<(), String> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| "Config root must be a JSON object".to_string())?;
    object
        .entry("browserProfiles".to_string())
        .or_insert_with(|| {
            serde_json::json!({
                "rootPath": null,
                "codexPath": null,
                "claudePath": null,
            })
        });
    object.insert("version".to_string(), Value::from(2));
    Ok(())
}

fn migrate_v2_to_v3(value: &mut Value) -> Result<(), String> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| "Config root must be a JSON object".to_string())?;
    object
        .entry("localQuotas".to_string())
        .or_insert_with(default_local_quotas_value);
    object.insert("version".to_string(), Value::from(3));
    Ok(())
}

fn default_local_quotas_value() -> Value {
    serde_json::to_value(LocalQuotaSettings::default()).unwrap_or(Value::Null)
}

fn config_value_version(value: &Value) -> Result<u32, String> {
    let version = value
        .as_object()
        .and_then(|object| object.get("version"))
        .and_then(Value::as_u64)
        .ok_or_else(|| "Config version must be an integer".to_string())?;

    u32::try_from(version).map_err(|_| format!("Config version {version} is too large"))
}

fn fill_missing_defaults(value: &mut Value) -> Result<(), String> {
    let defaults = serde_json::to_value(AppConfig::default())
        .map_err(|error| format!("Could not serialize default config: {error}"))?;

    merge_missing_fields(value, &defaults);
    Ok(())
}

fn merge_missing_fields(value: &mut Value, defaults: &Value) {
    if let (Value::Object(value_map), Value::Object(default_map)) = (value, defaults) {
        for (key, default_value) in default_map {
            match value_map.get_mut(key) {
                Some(existing_value) => merge_missing_fields(existing_value, default_value),
                None => {
                    value_map.insert(key.clone(), default_value.clone());
                }
            }
        }
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Config path must have a parent directory".to_string())?;
    let temp_path = temp_config_path(path);
    write_atomic_with_temp_path(path, parent, &temp_path, bytes)
}

fn write_atomic_with_temp_path(
    path: &Path,
    parent: &Path,
    temp_path: &Path,
    bytes: &[u8],
) -> Result<(), String> {
    let write_result = write_temp_file(temp_path, bytes).and_then(|_| {
        fs::rename(temp_path, path)
            .map_err(|error| format!("Could not replace config file: {error}"))?;
        set_restrictive_file_permissions(path)?;
        sync_parent_dir(parent);
        Ok(())
    });

    if write_result.is_err() {
        let _ = fs::remove_file(temp_path);
    }

    write_result
}

fn write_temp_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("Could not create temporary config file: {error}"))?;

    set_restrictive_file_permissions(path)?;
    file.write_all(bytes)
        .map_err(|error| format!("Could not write temporary config file: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("Could not sync temporary config file: {error}"))?;

    Ok(())
}

fn temp_config_path(path: &Path) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let extension = format!("tmp.{}.{}", std::process::id(), timestamp);

    path.with_extension(extension)
}

#[cfg(unix)]
fn set_restrictive_file_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("Could not set config file permissions: {error}"))
}

#[cfg(not(unix))]
fn set_restrictive_file_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
fn sync_parent_dir(path: &Path) {
    if let Ok(directory) = File::open(path) {
        let _ = directory.sync_all();
    }
}

#[cfg(not(unix))]
fn sync_parent_dir(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_ID: AtomicU64 = AtomicU64::new(1);

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new() -> Self {
            let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default();
            let path = std::env::temp_dir().join(format!(
                "forgegauge-config-test-{}-{timestamp}-{id}",
                std::process::id()
            ));

            fs::create_dir_all(&path).expect("test directory is created");
            Self { path }
        }

        fn config_path(&self) -> PathBuf {
            self.path.join(CONFIG_FILE_NAME)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn config_with_invalid_intervals() -> AppConfig {
        AppConfig {
            providers: ProviderSettings {
                local_enabled: true,
                web_enabled: true,
            },
            intervals: RefreshIntervals {
                local_seconds: 1,
                web_minutes: 1,
                manual_web_refresh_cooldown_seconds: 1,
                gauge_switch_seconds: 99,
            },
            low_usage_threshold: 200.0,
            ..AppConfig::default()
        }
    }

    fn configured_quota(label: &str, limit: f64, window_hours: u64) -> LocalServiceQuotaSettings {
        LocalServiceQuotaSettings {
            enabled: true,
            plan_label: label.to_string(),
            limit_kind: LocalQuotaLimitKind::RollingWindow,
            window_hours,
            usage_unit: LocalQuotaUsageUnit::Tokens,
            limit,
        }
    }

    #[test]
    fn missing_config_file_creates_default_config() {
        let dir = TestDir::new();
        let path = dir.config_path();

        let config = load_from_path(&path).expect("missing config loads");

        assert_eq!(config, AppConfig::default());
        assert!(path.exists());
    }

    #[test]
    fn current_config_round_trips_through_atomic_store() {
        let dir = TestDir::new();
        let path = dir.config_path();
        let config = AppConfig {
            enabled_services: ServiceToggles {
                codex: false,
                claude: true,
            },
            providers: ProviderSettings {
                local_enabled: false,
                web_enabled: true,
            },
            intervals: RefreshIntervals {
                local_seconds: 60,
                web_minutes: 45,
                manual_web_refresh_cooldown_seconds: 90,
                gauge_switch_seconds: 10,
            },
            low_usage_threshold: 12.0,
            local_quotas: LocalQuotaSettings {
                codex: configured_quota("Codex Pro", 250_000.0, 5),
                claude: configured_quota("Claude Max", 500_000.0, 24),
            },
            ..AppConfig::default()
        };

        let saved = save_to_path(&path, &config).expect("config saves");
        let loaded = load_from_path(&path).expect("config loads");

        assert_eq!(saved, config);
        assert_eq!(loaded, config);
        assert!(fs::read_dir(&dir.path)
            .expect("read test dir")
            .all(|entry| !entry
                .expect("dir entry")
                .file_name()
                .to_string_lossy()
                .contains(".tmp.")));
    }

    #[test]
    fn partial_old_config_is_default_filled_before_deserialization() {
        let dir = TestDir::new();
        let path = dir.config_path();
        fs::write(
            &path,
            r#"{
  "version": 1,
  "enabledServices": {
    "codex": false
  },
  "intervals": {
    "localSeconds": 30
  }
}"#,
        )
        .expect("test config is written");

        let config = load_from_path(&path).expect("partial config migrates");

        assert!(!config.enabled_services.codex);
        assert!(config.enabled_services.claude);
        assert!(config.providers.local_enabled);
        assert!(!config.providers.web_enabled);
        assert_eq!(config.intervals.local_seconds, 30);
        assert_eq!(config.intervals.web_minutes, 30);
        assert_eq!(config.intervals.manual_web_refresh_cooldown_seconds, 60);
        assert_eq!(config.intervals.gauge_switch_seconds, 6);
        assert_eq!(config.version, CONFIG_VERSION);
        assert_eq!(
            config.browser_profiles,
            BrowserProfileSettings {
                root_path: None,
                codex_path: None,
                claude_path: None,
            }
        );
        assert_eq!(config.local_quotas, LocalQuotaSettings::default());
    }

    #[test]
    fn v1_config_migrates_to_current_with_browser_profile_and_quota_defaults() {
        let dir = TestDir::new();
        let path = dir.config_path();
        fs::write(
            &path,
            r#"{
  "version": 1,
  "enabledServices": {
    "codex": true,
    "claude": false
  },
  "providers": {
    "localEnabled": true,
    "webEnabled": true
  },
  "intervals": {
    "localSeconds": 45,
    "webMinutes": 30,
    "manualWebRefreshCooldownSeconds": 90,
    "gaugeSwitchSeconds": 6
  },
  "lowUsageThreshold": 15
}"#,
        )
        .expect("test config is written");

        let config = load_from_path(&path).expect("v1 config migrates");

        assert_eq!(config.version, CONFIG_VERSION);
        assert!(config.enabled_services.codex);
        assert!(!config.enabled_services.claude);
        assert!(config.providers.web_enabled);
        assert_eq!(
            config.browser_profiles,
            BrowserProfileSettings {
                root_path: None,
                codex_path: None,
                claude_path: None,
            }
        );
        assert_eq!(config.local_quotas, LocalQuotaSettings::default());
    }

    #[test]
    fn v2_config_migrates_to_v3_with_quota_defaults() {
        let dir = TestDir::new();
        let path = dir.config_path();
        fs::write(
            &path,
            r#"{
  "version": 2,
  "enabledServices": {
    "codex": true,
    "claude": true
  },
  "providers": {
    "localEnabled": true,
    "webEnabled": false
  },
  "intervals": {
    "localSeconds": 45,
    "webMinutes": 30,
    "manualWebRefreshCooldownSeconds": 60,
    "gaugeSwitchSeconds": 6
  },
  "lowUsageThreshold": 20,
  "browserProfiles": {
    "rootPath": null,
    "codexPath": null,
    "claudePath": null
  }
}"#,
        )
        .expect("test config is written");

        let config = load_from_path(&path).expect("v2 config migrates");

        assert_eq!(config.version, CONFIG_VERSION);
        assert_eq!(config.local_quotas, LocalQuotaSettings::default());
    }

    #[test]
    fn failed_migration_preserves_previous_config_file() {
        let dir = TestDir::new();
        let path = dir.config_path();
        let raw = r#"{
  "version": 1,
  "enabledServices": {
    "codex": "yes"
  }
}"#;
        fs::write(&path, raw).expect("test config is written");

        let error = load_from_path(&path).expect_err("migrated invalid config fails");

        assert!(error.contains("Could not deserialize config file"));
        assert_eq!(fs::read_to_string(&path).expect("config remains"), raw);
    }

    #[test]
    fn current_config_preserves_browser_profile_settings() {
        let dir = TestDir::new();
        let path = dir.config_path();
        let config = AppConfig {
            browser_profiles: BrowserProfileSettings {
                root_path: Some("/tmp/forgegauge/browser".to_string()),
                codex_path: Some("/tmp/forgegauge/codex".to_string()),
                claude_path: Some("/tmp/forgegauge/claude".to_string()),
            },
            ..AppConfig::default()
        };

        save_to_path(&path, &config).expect("config saves");
        let loaded = load_from_path(&path).expect("config loads");

        assert_eq!(loaded.browser_profiles, config.browser_profiles);
    }

    #[test]
    fn current_config_preserves_local_quota_settings() {
        let dir = TestDir::new();
        let path = dir.config_path();
        let config = AppConfig {
            local_quotas: LocalQuotaSettings {
                codex: configured_quota("Codex Team", 125_000.0, 12),
                claude: configured_quota("Claude Max", 300_000.0, 5),
            },
            ..AppConfig::default()
        };

        save_to_path(&path, &config).expect("config saves");
        let loaded = load_from_path(&path).expect("config loads");

        assert_eq!(loaded.local_quotas, config.local_quotas);
    }

    #[test]
    fn malformed_config_is_rejected_without_overwriting_file() {
        let dir = TestDir::new();
        let path = dir.config_path();
        let raw = "{ invalid";
        fs::write(&path, raw).expect("test config is written");

        let error = load_from_path(&path).expect_err("malformed config fails");

        assert!(error.contains("Could not parse config file"));
        assert_eq!(fs::read_to_string(&path).expect("config remains"), raw);
    }

    #[test]
    fn future_config_version_is_rejected_without_overwriting_file() {
        let dir = TestDir::new();
        let path = dir.config_path();
        let raw = r#"{"version":999}"#;
        fs::write(&path, raw).expect("test config is written");

        let error = load_from_path(&path).expect_err("future config fails");

        assert!(error.contains("newer than supported"));
        assert_eq!(fs::read_to_string(&path).expect("config remains"), raw);
    }

    #[test]
    fn non_integer_config_version_is_rejected_without_overwriting_file() {
        let dir = TestDir::new();
        let path = dir.config_path();
        let raw = r#"{"version":"1"}"#;
        fs::write(&path, raw).expect("test config is written");

        let error = load_from_path(&path).expect_err("invalid version fails");

        assert!(error.contains("Config version must be an integer"));
        assert_eq!(fs::read_to_string(&path).expect("config remains"), raw);
    }

    #[test]
    fn failed_atomic_write_preserves_previous_config_file() {
        let dir = TestDir::new();
        let path = dir.config_path();
        let temp_path = dir.path.join("blocked-temp-path");
        save_to_path(&path, &AppConfig::default()).expect("config saves");
        let previous = fs::read_to_string(&path).expect("previous config exists");
        fs::create_dir(&temp_path).expect("blocking temp directory is created");

        let error = write_atomic_with_temp_path(
            &path,
            &dir.path,
            &temp_path,
            br#"{"version":1,"enabledServices":{"codex":false,"claude":false}}"#,
        )
        .expect_err("temp path directory makes write fail");

        assert!(error.contains("Could not create temporary config file"));
        assert_eq!(fs::read_to_string(&path).expect("config remains"), previous);
    }

    #[test]
    fn web_providers_are_disabled_by_default() {
        let dir = TestDir::new();
        let config = load_from_path(&dir.config_path()).expect("default config loads");

        assert!(!config.providers.web_enabled);
    }

    #[test]
    fn save_normalizes_intervals_cooldown_and_threshold() {
        let dir = TestDir::new();
        let path = dir.config_path();

        let saved = save_to_path(&path, &config_with_invalid_intervals()).expect("config saves");
        let loaded = load_from_path(&path).expect("config loads");

        assert_eq!(saved.intervals.local_seconds, 30);
        assert_eq!(saved.intervals.web_minutes, 15);
        assert_eq!(saved.intervals.manual_web_refresh_cooldown_seconds, 60);
        assert_eq!(saved.intervals.gauge_switch_seconds, 10);
        assert_eq!(saved.low_usage_threshold, 100.0);
        assert_eq!(loaded, saved);
    }

    #[test]
    fn save_normalizes_local_quota_fields() {
        let dir = TestDir::new();
        let path = dir.config_path();
        let config = AppConfig {
            local_quotas: LocalQuotaSettings {
                codex: configured_quota(
                    "  This label is intentionally long enough to exceed the stored label limit by a wide margin  ",
                    -1.0,
                    0,
                ),
                claude: configured_quota("  Claude Max  ", f64::INFINITY, 1_000),
            },
            ..AppConfig::default()
        };

        let saved = save_to_path(&path, &config).expect("config saves");

        assert_eq!(saved.local_quotas.codex.limit, 0.0);
        assert_eq!(saved.local_quotas.codex.window_hours, 1);
        assert_eq!(
            saved.local_quotas.codex.plan_label.chars().count(),
            MAX_PLAN_LABEL_LENGTH
        );
        assert_eq!(saved.local_quotas.claude.limit, 0.0);
        assert_eq!(saved.local_quotas.claude.window_hours, 744);
        assert_eq!(saved.local_quotas.claude.plan_label, "Claude Max");
    }

    #[cfg(unix)]
    #[test]
    fn saved_config_uses_restrictive_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TestDir::new();
        let path = dir.config_path();
        save_to_path(&path, &AppConfig::default()).expect("config saves");

        let mode = fs::metadata(&path)
            .expect("config metadata")
            .permissions()
            .mode()
            & 0o777;

        assert_eq!(mode, 0o600);
    }
}
