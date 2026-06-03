use crate::{
    config::{AppConfig, LocalQuotaLimitKind, LocalQuotaUsageUnit, LocalServiceQuotaSettings},
    local_provider::{ClaudeLocalProvider, CodexLocalProvider, LocalQuotaCalibration},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Mutex,
    time::Duration,
};
use tauri::{AppHandle, Emitter};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

pub const SNAPSHOTS_UPDATED_EVENT: &str = "usage://snapshots-updated";

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Service {
    Codex,
    Claude,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UsageSource {
    Local,
    Web,
    Merged,
    Fake,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UsageConfidence {
    High,
    Medium,
    Low,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub service: Service,
    pub remaining_percent: Option<f32>,
    pub used_percent: Option<f32>,
    pub reset_at: Option<String>,
    pub source: UsageSource,
    pub confidence: UsageConfidence,
    pub last_updated: String,
    pub details: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageDisplayState {
    pub snapshots: Vec<UsageSnapshot>,
    pub updated_at: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrayGaugeState {
    pub service: Service,
    pub remaining_percent: Option<f32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub enum UsageProviderError {
    NotConfigured,
    Disabled,
    MissingData,
    PermissionDenied,
    ParseFailed,
    LoginRequired,
    MfaRequired,
    CaptchaOrBotCheck,
    NetworkUnavailable,
    TimedOut,
    UnexpectedUi,
    UnsafePath,
    Internal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsageProviderId {
    CodexLocal,
    CodexWeb,
    ClaudeLocal,
    ClaudeWeb,
    Fake,
}

trait UsageProvider: Send + Sync {
    fn provider_id(&self) -> UsageProviderId;
    fn service(&self) -> Service;
    fn source(&self) -> UsageSource;
    fn refresh(&self, now: &str) -> Result<UsageSnapshot, UsageProviderError>;
    fn local_data_root(&self) -> Option<PathBuf> {
        None
    }
    fn local_calibration(&self) -> Option<LocalQuotaCalibration> {
        None
    }

    fn provider_key(&self) -> String {
        self.provider_id().refresh_key(self.service())
    }
}

trait Clock: Send + Sync {
    fn now_rfc3339(&self) -> String;
}

pub struct UsageEngine {
    inner: Mutex<UsageEngineState>,
    clock: Box<dyn Clock>,
    local_roots: LocalProviderRoots,
}

struct UsageEngineState {
    config: AppConfig,
    providers: Vec<Box<dyn UsageProvider>>,
    snapshots: HashMap<Service, UsageSnapshot>,
    active_provider_keys: HashSet<String>,
    last_updated: String,
}

#[derive(Clone, Copy)]
struct FakeUsageProvider {
    service: Service,
    remaining_percent: f32,
}

struct SystemClock;

#[derive(Clone, Debug, Default)]
struct LocalProviderRoots {
    codex: Option<PathBuf>,
    claude: Option<PathBuf>,
}

impl Service {
    pub fn code(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Claude => "Claude Code",
        }
    }
}

impl UsageSource {
    pub fn code(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Web => "web",
            Self::Merged => "merged",
            Self::Fake => "fake",
        }
    }
}

impl UsageProviderError {
    fn code(self) -> &'static str {
        match self {
            Self::NotConfigured => "not_configured",
            Self::Disabled => "disabled",
            Self::MissingData => "missing_data",
            Self::PermissionDenied => "permission_denied",
            Self::ParseFailed => "parse_failed",
            Self::LoginRequired => "login_required",
            Self::MfaRequired => "mfa_required",
            Self::CaptchaOrBotCheck => "captcha_or_bot_check",
            Self::NetworkUnavailable => "network_unavailable",
            Self::TimedOut => "timed_out",
            Self::UnexpectedUi => "unexpected_ui",
            Self::UnsafePath => "unsafe_path",
            Self::Internal => "internal",
        }
    }
}

impl UsageProviderId {
    pub fn for_service_source(service: Service, source: UsageSource) -> Option<Self> {
        match (service, source) {
            (Service::Codex, UsageSource::Local) => Some(Self::CodexLocal),
            (Service::Codex, UsageSource::Web) => Some(Self::CodexWeb),
            (Service::Claude, UsageSource::Local) => Some(Self::ClaudeLocal),
            (Service::Claude, UsageSource::Web) => Some(Self::ClaudeWeb),
            (_, UsageSource::Fake) => Some(Self::Fake),
            (_, UsageSource::Merged) => None,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::CodexLocal => "codex.local",
            Self::CodexWeb => "codex.web",
            Self::ClaudeLocal => "claude.local",
            Self::ClaudeWeb => "claude.web",
            Self::Fake => "fake",
        }
    }

    fn refresh_key(self, service: Service) -> String {
        match self {
            Self::Fake => format!("{}.{}", service.code(), self.code()),
            _ => self.code().to_string(),
        }
    }
}

impl Clock for SystemClock {
    fn now_rfc3339(&self) -> String {
        now_rfc3339()
    }
}

impl UsageDisplayState {
    pub fn tray_states(&self) -> Vec<TrayGaugeState> {
        let mut states: Vec<_> = self
            .snapshots
            .iter()
            .map(|snapshot| TrayGaugeState {
                service: snapshot.service,
                remaining_percent: snapshot.remaining_percent,
            })
            .collect();

        states.sort_by_key(|state| match state.service {
            Service::Codex => 0,
            Service::Claude => 1,
        });

        if states.is_empty() {
            states.push(TrayGaugeState {
                service: Service::Codex,
                remaining_percent: None,
            });
        }

        states
    }
}

impl UsageEngine {
    pub fn new(config: AppConfig) -> Self {
        Self::with_clock(config, Box::new(SystemClock))
    }

    fn with_clock(config: AppConfig, clock: Box<dyn Clock>) -> Self {
        Self::with_clock_and_local_roots(config, clock, LocalProviderRoots::default())
    }

    fn with_clock_and_local_roots(
        config: AppConfig,
        clock: Box<dyn Clock>,
        local_roots: LocalProviderRoots,
    ) -> Self {
        let config = config.normalized();
        let last_updated = clock.now_rfc3339();

        Self {
            inner: Mutex::new(UsageEngineState {
                providers: providers_for_config(&config, &local_roots),
                config,
                snapshots: HashMap::new(),
                active_provider_keys: HashSet::new(),
                last_updated,
            }),
            clock,
            local_roots,
        }
    }

    pub fn config(&self) -> Result<AppConfig, String> {
        let state = self.lock()?;
        Ok(state.config.clone())
    }

    pub fn update_config(&self, config: AppConfig) -> Result<AppConfig, String> {
        let config = config.normalized();
        let providers = providers_for_config(&config, &self.local_roots);
        let provider_keys: HashSet<_> = providers
            .iter()
            .map(|provider| provider.provider_key())
            .collect();
        let mut state = self.lock()?;

        state.config = config.clone();
        state.providers = providers;
        state
            .snapshots
            .retain(|service, _| config.service_enabled(*service));
        state
            .active_provider_keys
            .retain(|key| provider_keys.contains(key));

        Ok(config)
    }

    pub fn display_state(&self) -> Result<UsageDisplayState, String> {
        let state = self.lock()?;
        Ok(state.display_state())
    }

    pub fn snapshots(&self) -> Result<Vec<UsageSnapshot>, String> {
        Ok(self.display_state()?.snapshots)
    }

    pub fn refresh_all(&self) -> Result<UsageDisplayState, String> {
        let (providers, provider_services) = {
            let state = self.lock()?;
            let providers = state
                .providers
                .iter()
                .map(|provider| provider_descriptor(provider.as_ref()))
                .collect::<Vec<_>>();
            let provider_services = providers
                .iter()
                .map(|provider| provider.service)
                .collect::<HashSet<_>>();

            (providers, provider_services)
        };

        let mut refreshed = Vec::new();
        let now = self.clock.now_rfc3339();

        for provider in providers {
            if !self.try_begin_refresh(provider.provider_key.clone())? {
                continue;
            }

            let snapshot = self
                .refresh_provider(&provider, &now)
                .unwrap_or_else(|error| error_snapshot(&provider, error, &now));
            refreshed.push(snapshot);
            self.finish_refresh(&provider.provider_key)?;
        }

        let mut state = self.lock()?;
        state
            .snapshots
            .retain(|service, _| provider_services.contains(service));

        for snapshot in refreshed {
            state.snapshots.insert(snapshot.service, snapshot);
        }

        state.last_updated = now;
        Ok(state.display_state())
    }

    pub fn refresh_provider_source(
        &self,
        service: Service,
        source: UsageSource,
    ) -> Result<UsageDisplayState, String> {
        let provider_id = UsageProviderId::for_service_source(service, source)
            .ok_or_else(|| "Provider source cannot be refreshed directly".to_string())?;
        let provider = {
            let state = self.lock()?;
            state
                .providers
                .iter()
                .map(|provider| provider_descriptor(provider.as_ref()))
                .find(|provider| provider.service == service && provider.provider_id == provider_id)
        }
        .ok_or_else(|| "Provider is not configured".to_string())?;
        let now = self.clock.now_rfc3339();

        if !self.try_begin_refresh(provider.provider_key.clone())? {
            return self.display_state();
        }

        let snapshot = self
            .refresh_provider(&provider, &now)
            .unwrap_or_else(|error| error_snapshot(&provider, error, &now));
        self.finish_refresh(&provider.provider_key)?;

        let mut state = self.lock()?;
        state.snapshots.insert(snapshot.service, snapshot);
        state.last_updated = now;
        Ok(state.display_state())
    }

    pub fn refresh_all_and_emit(&self, app: &AppHandle) -> Result<UsageDisplayState, String> {
        let display_state = self.refresh_all()?;
        app.emit(SNAPSHOTS_UPDATED_EVENT, &display_state)
            .map_err(|error| format!("Could not emit usage update: {error}"))?;
        Ok(display_state)
    }

    pub fn scheduler_sleep_duration(&self) -> Result<Duration, String> {
        let state = self.lock()?;
        Ok(Duration::from_secs(state.config.intervals.local_seconds))
    }

    fn refresh_provider(
        &self,
        provider: &ProviderDescriptor,
        now: &str,
    ) -> Result<UsageSnapshot, UsageProviderError> {
        match (provider.provider_id, provider.service) {
            (UsageProviderId::Fake, Service::Codex) => FakeUsageProvider {
                service: Service::Codex,
                remaining_percent: 72.0,
            }
            .refresh(now),
            (UsageProviderId::Fake, Service::Claude) => FakeUsageProvider {
                service: Service::Claude,
                remaining_percent: 41.0,
            }
            .refresh(now),
            (UsageProviderId::ClaudeLocal, Service::Claude) => provider
                .local_data_root
                .clone()
                .map(ClaudeLocalProvider::new)
                .map(|local_provider| {
                    local_provider.with_calibration(provider.local_calibration.clone())
                })
                .ok_or(UsageProviderError::Internal)?
                .refresh(now),
            (UsageProviderId::CodexLocal, Service::Codex) => provider
                .local_data_root
                .clone()
                .map(CodexLocalProvider::new)
                .map(|local_provider| {
                    local_provider.with_calibration(provider.local_calibration.clone())
                })
                .ok_or(UsageProviderError::Internal)?
                .refresh(now),
            _ => Err(UsageProviderError::Internal),
        }
    }

    fn try_begin_refresh(&self, provider_key: String) -> Result<bool, String> {
        let mut state = self.lock()?;

        if state.active_provider_keys.contains(&provider_key) {
            return Ok(false);
        }

        state.active_provider_keys.insert(provider_key);
        Ok(true)
    }

    fn finish_refresh(&self, provider_key: &str) -> Result<(), String> {
        let mut state = self.lock()?;
        state.active_provider_keys.remove(provider_key);
        Ok(())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, UsageEngineState>, String> {
        self.inner
            .lock()
            .map_err(|_| "Usage engine state lock was poisoned".to_string())
    }
}

impl UsageEngineState {
    fn display_state(&self) -> UsageDisplayState {
        let mut snapshots = self.snapshots.values().cloned().collect::<Vec<_>>();
        snapshots.sort_by_key(|snapshot| match snapshot.service {
            Service::Codex => 0,
            Service::Claude => 1,
        });

        UsageDisplayState {
            snapshots,
            updated_at: self.last_updated.clone(),
        }
    }
}

impl AppConfig {
    fn service_enabled(&self, service: Service) -> bool {
        match service {
            Service::Codex => self.enabled_services.codex,
            Service::Claude => self.enabled_services.claude,
        }
    }
}

impl UsageProvider for FakeUsageProvider {
    fn provider_id(&self) -> UsageProviderId {
        UsageProviderId::for_service_source(self.service(), self.source())
            .expect("fake providers have a provider id")
    }

    fn service(&self) -> Service {
        self.service
    }

    fn source(&self) -> UsageSource {
        UsageSource::Fake
    }

    fn refresh(&self, now: &str) -> Result<UsageSnapshot, UsageProviderError> {
        Ok(UsageSnapshot {
            service: self.service,
            remaining_percent: Some(self.remaining_percent),
            used_percent: Some(100.0 - self.remaining_percent),
            reset_at: None,
            source: UsageSource::Fake,
            confidence: UsageConfidence::Unknown,
            last_updated: now.to_string(),
            details: serde_json::json!({
                "status": "placeholder",
                "providerId": self.provider_id().code(),
                "source": self.source().code(),
            }),
        })
    }
}

impl UsageProvider for ClaudeLocalProvider {
    fn provider_id(&self) -> UsageProviderId {
        UsageProviderId::ClaudeLocal
    }

    fn service(&self) -> Service {
        Service::Claude
    }

    fn source(&self) -> UsageSource {
        UsageSource::Local
    }

    fn refresh(&self, now: &str) -> Result<UsageSnapshot, UsageProviderError> {
        Ok(self.refresh_snapshot(now))
    }

    fn local_data_root(&self) -> Option<PathBuf> {
        Some(self.data_root().to_path_buf())
    }

    fn local_calibration(&self) -> Option<LocalQuotaCalibration> {
        self.calibration()
    }
}

impl UsageProvider for CodexLocalProvider {
    fn provider_id(&self) -> UsageProviderId {
        UsageProviderId::CodexLocal
    }

    fn service(&self) -> Service {
        Service::Codex
    }

    fn source(&self) -> UsageSource {
        UsageSource::Local
    }

    fn refresh(&self, now: &str) -> Result<UsageSnapshot, UsageProviderError> {
        Ok(self.refresh_snapshot(now))
    }

    fn local_data_root(&self) -> Option<PathBuf> {
        Some(self.data_root().to_path_buf())
    }

    fn local_calibration(&self) -> Option<LocalQuotaCalibration> {
        self.calibration()
    }
}

#[derive(Clone, Debug)]
struct ProviderDescriptor {
    provider_key: String,
    provider_id: UsageProviderId,
    service: Service,
    source: UsageSource,
    local_data_root: Option<PathBuf>,
    local_calibration: Option<LocalQuotaCalibration>,
}

fn provider_descriptor(provider: &dyn UsageProvider) -> ProviderDescriptor {
    ProviderDescriptor {
        provider_key: provider.provider_key(),
        provider_id: provider.provider_id(),
        service: provider.service(),
        source: provider.source(),
        local_data_root: provider.local_data_root(),
        local_calibration: provider.local_calibration(),
    }
}

fn providers_for_config(
    config: &AppConfig,
    local_roots: &LocalProviderRoots,
) -> Vec<Box<dyn UsageProvider>> {
    let mut providers: Vec<Box<dyn UsageProvider>> = Vec::new();

    if config.enabled_services.codex {
        if config.providers.local_enabled {
            let calibration = quota_calibration(&config.local_quotas.codex);
            let provider = local_roots
                .codex
                .clone()
                .map(CodexLocalProvider::new)
                .or_else(CodexLocalProvider::from_default_root);

            if let Some(provider) = provider {
                providers.push(Box::new(provider.with_calibration(calibration)));
            }
        } else {
            providers.push(Box::new(FakeUsageProvider {
                service: Service::Codex,
                remaining_percent: 72.0,
            }));
        }
    }

    if config.enabled_services.claude {
        if config.providers.local_enabled {
            let calibration = quota_calibration(&config.local_quotas.claude);
            let provider = local_roots
                .claude
                .clone()
                .map(ClaudeLocalProvider::new)
                .or_else(ClaudeLocalProvider::from_default_root);

            if let Some(provider) = provider {
                providers.push(Box::new(provider.with_calibration(calibration)));
            }
        } else {
            providers.push(Box::new(FakeUsageProvider {
                service: Service::Claude,
                remaining_percent: 41.0,
            }));
        }
    }

    providers
}

fn quota_calibration(settings: &LocalServiceQuotaSettings) -> Option<LocalQuotaCalibration> {
    if !settings.enabled
        || settings.limit_kind != LocalQuotaLimitKind::RollingWindow
        || settings.usage_unit != LocalQuotaUsageUnit::Tokens
    {
        return None;
    }

    LocalQuotaCalibration::new(settings.limit, settings.window_hours)
}

fn error_snapshot(
    provider: &ProviderDescriptor,
    error: UsageProviderError,
    now: &str,
) -> UsageSnapshot {
    UsageSnapshot {
        service: provider.service,
        remaining_percent: None,
        used_percent: None,
        reset_at: None,
        source: provider.source,
        confidence: UsageConfidence::Unknown,
        last_updated: now.to_string(),
        details: serde_json::json!({
            "status": error.code(),
            "providerId": provider.provider_id.code(),
            "source": provider.source.code(),
        }),
    }
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    struct FixedClock {
        now: String,
    }

    static TEST_ID: AtomicU64 = AtomicU64::new(1);

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new() -> Self {
            let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("forgegauge-usage-test-{}-{id}", std::process::id()));

            fs::create_dir_all(&path).expect("test directory is created");
            Self { path }
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    impl Clock for FixedClock {
        fn now_rfc3339(&self) -> String {
            self.now.clone()
        }
    }

    fn config_with_services(codex: bool, claude: bool) -> AppConfig {
        AppConfig {
            enabled_services: crate::config::ServiceToggles { codex, claude },
            providers: crate::config::ProviderSettings {
                local_enabled: false,
                web_enabled: false,
            },
            ..AppConfig::default()
        }
    }

    fn local_claude_config() -> AppConfig {
        AppConfig {
            enabled_services: crate::config::ServiceToggles {
                codex: false,
                claude: true,
            },
            providers: crate::config::ProviderSettings {
                local_enabled: true,
                web_enabled: false,
            },
            ..AppConfig::default()
        }
    }

    fn local_codex_config() -> AppConfig {
        AppConfig {
            enabled_services: crate::config::ServiceToggles {
                codex: true,
                claude: false,
            },
            providers: crate::config::ProviderSettings {
                local_enabled: true,
                web_enabled: false,
            },
            ..AppConfig::default()
        }
    }

    fn configured_quota(limit: f64, window_hours: u64) -> crate::config::LocalServiceQuotaSettings {
        crate::config::LocalServiceQuotaSettings {
            enabled: true,
            plan_label: String::new(),
            limit_kind: crate::config::LocalQuotaLimitKind::RollingWindow,
            window_hours,
            usage_unit: crate::config::LocalQuotaUsageUnit::Tokens,
            limit,
        }
    }

    fn claude_fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/claude-local")
    }

    fn create_codex_state_db(root: &std::path::Path) {
        create_codex_state_db_with_updated_at(root, rfc3339_ms("2026-06-03T22:00:00Z"));
    }

    fn create_codex_state_db_with_updated_at(root: &std::path::Path, updated_at_ms: i64) {
        let connection =
            rusqlite::Connection::open(root.join("state_5.sqlite")).expect("state db is created");
        connection
            .execute(
                "CREATE TABLE threads (
                    tokens_used INTEGER NOT NULL DEFAULT 0,
                    updated_at INTEGER NOT NULL,
                    updated_at_ms INTEGER,
                    model TEXT
                )",
                [],
            )
            .expect("threads table is created");
        connection
            .execute(
                "INSERT INTO threads (tokens_used, updated_at, updated_at_ms, model)
                 VALUES (900, ?1, ?2, 'codex-fixture')",
                (updated_at_ms / 1000, updated_at_ms),
            )
            .expect("thread row is inserted");
    }

    fn rfc3339_ms(value: &str) -> i64 {
        let timestamp = OffsetDateTime::parse(value, &Rfc3339).expect("timestamp parses");
        i64::try_from(timestamp.unix_timestamp_nanos() / 1_000_000).expect("timestamp fits")
    }

    #[test]
    fn fake_provider_refreshes_enabled_services() {
        let engine = UsageEngine::new(config_with_services(true, true));
        let display_state = engine.refresh_all().expect("refresh succeeds");

        assert_eq!(display_state.snapshots.len(), 2);
        assert_eq!(display_state.snapshots[0].service, Service::Codex);
        assert_eq!(display_state.snapshots[0].remaining_percent, Some(72.0));
        assert_eq!(display_state.snapshots[0].source, UsageSource::Fake);
        assert_eq!(display_state.snapshots[1].service, Service::Claude);
        assert_eq!(display_state.snapshots[1].remaining_percent, Some(41.0));
    }

    #[test]
    fn targeted_provider_refresh_updates_one_configured_provider() {
        let engine = UsageEngine::new(config_with_services(true, true));

        let display_state = engine
            .refresh_provider_source(Service::Codex, UsageSource::Fake)
            .expect("targeted refresh succeeds");

        assert_eq!(display_state.snapshots.len(), 1);
        assert_eq!(display_state.snapshots[0].service, Service::Codex);
        assert_eq!(display_state.snapshots[0].remaining_percent, Some(72.0));

        let display_state = engine
            .refresh_provider_source(Service::Claude, UsageSource::Fake)
            .expect("second targeted refresh succeeds");

        assert_eq!(display_state.snapshots.len(), 2);
        assert_eq!(display_state.snapshots[0].service, Service::Codex);
        assert_eq!(display_state.snapshots[1].service, Service::Claude);
    }

    #[test]
    fn targeted_provider_refresh_rejects_unconfigured_or_unrefreshable_sources() {
        let engine = UsageEngine::new(config_with_services(true, false));

        let unconfigured = engine
            .refresh_provider_source(Service::Claude, UsageSource::Fake)
            .expect_err("disabled provider is rejected");
        let merged = engine
            .refresh_provider_source(Service::Codex, UsageSource::Merged)
            .expect_err("merged source is rejected");

        assert_eq!(unconfigured, "Provider is not configured");
        assert_eq!(merged, "Provider source cannot be refreshed directly");
    }

    #[test]
    fn refresh_uses_injected_clock_for_snapshots_and_display_state() {
        let now = "2026-06-03T21:30:00Z";
        let engine = UsageEngine::with_clock(
            config_with_services(true, true),
            Box::new(FixedClock {
                now: now.to_string(),
            }),
        );

        let display_state = engine.refresh_all().expect("refresh succeeds");

        assert_eq!(display_state.updated_at, now);
        assert!(display_state
            .snapshots
            .iter()
            .all(|snapshot| snapshot.last_updated == now));
    }

    #[test]
    fn disabled_services_clear_display_cache_and_tray_falls_back_to_unknown() {
        let engine = UsageEngine::new(config_with_services(true, true));
        engine.refresh_all().expect("initial refresh succeeds");
        engine
            .update_config(config_with_services(false, false))
            .expect("config update succeeds");
        let display_state = engine.refresh_all().expect("refresh succeeds");

        assert!(display_state.snapshots.is_empty());
        assert_eq!(
            display_state.tray_states(),
            vec![TrayGaugeState {
                service: Service::Codex,
                remaining_percent: None,
            }]
        );
    }

    #[test]
    fn disabling_one_service_removes_only_that_service_from_display_cache() {
        let engine = UsageEngine::new(config_with_services(true, true));
        engine.refresh_all().expect("initial refresh succeeds");
        engine
            .update_config(config_with_services(false, true))
            .expect("config update succeeds");
        let display_state = engine.refresh_all().expect("refresh succeeds");

        assert_eq!(display_state.snapshots.len(), 1);
        assert_eq!(display_state.snapshots[0].service, Service::Claude);
        assert_eq!(display_state.snapshots[0].remaining_percent, Some(41.0));
    }

    #[test]
    fn tray_states_are_sorted_and_reflect_snapshot_percentages() {
        let engine = UsageEngine::new(config_with_services(true, true));
        let display_state = engine.refresh_all().expect("refresh succeeds");

        assert_eq!(
            display_state.tray_states(),
            vec![
                TrayGaugeState {
                    service: Service::Codex,
                    remaining_percent: Some(72.0),
                },
                TrayGaugeState {
                    service: Service::Claude,
                    remaining_percent: Some(41.0),
                }
            ]
        );
    }

    #[test]
    fn provider_refresh_overlap_is_skipped_until_finished() {
        let engine = UsageEngine::new(config_with_services(true, true));

        assert!(engine
            .try_begin_refresh("codex.fake".to_string())
            .expect("begin succeeds"));
        assert!(!engine
            .try_begin_refresh("codex.fake".to_string())
            .expect("second begin is skipped"));

        engine
            .finish_refresh("codex.fake")
            .expect("finish succeeds");
        assert!(engine
            .try_begin_refresh("codex.fake".to_string())
            .expect("begin after finish succeeds"));
    }

    #[test]
    fn skipped_provider_refresh_keeps_existing_cached_snapshot() {
        let engine = UsageEngine::new(config_with_services(true, true));
        engine.refresh_all().expect("initial refresh succeeds");
        assert!(engine
            .try_begin_refresh("codex.fake".to_string())
            .expect("begin succeeds"));

        let display_state = engine.refresh_all().expect("refresh succeeds");

        assert_eq!(display_state.snapshots.len(), 2);
        assert_eq!(display_state.snapshots[0].service, Service::Codex);
        assert_eq!(display_state.snapshots[0].remaining_percent, Some(72.0));
        engine
            .finish_refresh("codex.fake")
            .expect("finish succeeds");
    }

    #[test]
    fn disabling_a_provider_clears_pending_refresh_tracking() {
        let engine = UsageEngine::new(config_with_services(true, true));
        assert!(engine
            .try_begin_refresh("codex.fake".to_string())
            .expect("begin succeeds"));

        engine
            .update_config(config_with_services(false, true))
            .expect("config update succeeds");

        assert!(engine
            .try_begin_refresh("codex.fake".to_string())
            .expect("disabled provider key was cleared"));
    }

    #[test]
    fn display_state_serializes_to_expected_ipc_shape() {
        let engine = UsageEngine::new(config_with_services(true, true));
        let display_state = engine.refresh_all().expect("refresh succeeds");
        let value = serde_json::to_value(display_state).expect("serializes");
        let first_snapshot = &value["snapshots"][0];

        assert_eq!(first_snapshot["service"], "codex");
        assert_eq!(first_snapshot["remainingPercent"], 72.0);
        assert_eq!(first_snapshot["source"], "fake");
        assert_eq!(first_snapshot["confidence"], "unknown");
        assert!(first_snapshot["lastUpdated"]
            .as_str()
            .expect("lastUpdated is a string")
            .contains('T'));
        assert!(value["updatedAt"]
            .as_str()
            .expect("updatedAt is a string")
            .contains('T'));
    }

    #[test]
    fn local_claude_provider_is_registered_when_local_providers_are_enabled() {
        let engine = UsageEngine::with_clock_and_local_roots(
            local_claude_config(),
            Box::new(FixedClock {
                now: "2026-06-03T22:30:00Z".to_string(),
            }),
            LocalProviderRoots {
                codex: None,
                claude: Some(claude_fixture_root()),
            },
        );

        let display_state = engine.refresh_all().expect("refresh succeeds");

        assert_eq!(display_state.snapshots.len(), 1);
        assert_eq!(display_state.snapshots[0].service, Service::Claude);
        assert_eq!(display_state.snapshots[0].source, UsageSource::Local);
        assert_eq!(display_state.snapshots[0].confidence, UsageConfidence::Low);
        assert_eq!(display_state.snapshots[0].remaining_percent, None);
        assert_eq!(
            display_state.snapshots[0].details["providerId"],
            "claude.local"
        );
        assert_eq!(display_state.snapshots[0].details["usageRecords"], 2);
    }

    #[test]
    fn local_codex_provider_is_registered_when_local_providers_are_enabled() {
        let dir = TestDir::new();
        create_codex_state_db(&dir.path);
        let engine = UsageEngine::with_clock_and_local_roots(
            local_codex_config(),
            Box::new(FixedClock {
                now: "2026-06-03T22:45:00Z".to_string(),
            }),
            LocalProviderRoots {
                codex: Some(dir.path.clone()),
                claude: None,
            },
        );

        let display_state = engine.refresh_all().expect("refresh succeeds");

        assert_eq!(display_state.snapshots.len(), 1);
        assert_eq!(display_state.snapshots[0].service, Service::Codex);
        assert_eq!(display_state.snapshots[0].source, UsageSource::Local);
        assert_eq!(display_state.snapshots[0].confidence, UsageConfidence::Low);
        assert_eq!(display_state.snapshots[0].remaining_percent, None);
        assert_eq!(
            display_state.snapshots[0].details["providerId"],
            "codex.local"
        );
        assert_eq!(display_state.snapshots[0].details["threadsRead"], 1);
        assert_eq!(display_state.snapshots[0].details["totalTokens"], 900);
    }

    #[test]
    fn local_codex_provider_receives_quota_calibration_from_config() {
        let dir = TestDir::new();
        create_codex_state_db_with_updated_at(&dir.path, rfc3339_ms("2026-06-03T21:00:00Z"));
        let config = AppConfig {
            local_quotas: crate::config::LocalQuotaSettings {
                codex: configured_quota(1000.0, 5),
                claude: crate::config::LocalServiceQuotaSettings::default(),
            },
            ..local_codex_config()
        };
        let engine = UsageEngine::with_clock_and_local_roots(
            config,
            Box::new(FixedClock {
                now: "2026-06-03T22:00:00Z".to_string(),
            }),
            LocalProviderRoots {
                codex: Some(dir.path.clone()),
                claude: None,
            },
        );

        let display_state = engine.refresh_all().expect("refresh succeeds");

        assert_eq!(display_state.snapshots[0].remaining_percent, Some(10.0));
        assert_eq!(display_state.snapshots[0].used_percent, Some(90.0));
        assert_eq!(
            display_state.snapshots[0].details["calibrationStatus"],
            "active"
        );
        assert_eq!(display_state.snapshots[0].details["windowTokens"], 900);
    }

    #[test]
    fn provider_errors_map_to_sanitized_unknown_snapshots() {
        let provider = ProviderDescriptor {
            provider_key: "codex.fake".to_string(),
            provider_id: UsageProviderId::Fake,
            service: Service::Codex,
            source: UsageSource::Fake,
            local_data_root: None,
            local_calibration: None,
        };
        let snapshot = error_snapshot(
            &provider,
            UsageProviderError::ParseFailed,
            "2026-06-03T20:00:00Z",
        );

        assert_eq!(snapshot.remaining_percent, None);
        assert_eq!(snapshot.used_percent, None);
        assert_eq!(snapshot.confidence, UsageConfidence::Unknown);
        assert_eq!(snapshot.details["status"], "parse_failed");
        assert_eq!(snapshot.details["providerId"], UsageProviderId::Fake.code());
        assert!(snapshot.details.get("raw").is_none());
    }

    #[test]
    fn provider_ids_are_stable() {
        assert_eq!(
            UsageProviderId::for_service_source(Service::Codex, UsageSource::Local)
                .expect("codex local id")
                .code(),
            "codex.local"
        );
        assert_eq!(
            UsageProviderId::for_service_source(Service::Codex, UsageSource::Web)
                .expect("codex web id")
                .code(),
            "codex.web"
        );
        assert_eq!(
            UsageProviderId::for_service_source(Service::Claude, UsageSource::Local)
                .expect("claude local id")
                .code(),
            "claude.local"
        );
        assert_eq!(
            UsageProviderId::for_service_source(Service::Claude, UsageSource::Web)
                .expect("claude web id")
                .code(),
            "claude.web"
        );
        assert_eq!(
            UsageProviderId::for_service_source(Service::Codex, UsageSource::Fake)
                .expect("fake id")
                .code(),
            "fake"
        );
    }

    #[test]
    fn fake_provider_refresh_keys_remain_per_service() {
        assert_eq!(
            UsageProviderId::Fake.refresh_key(Service::Codex),
            "codex.fake"
        );
        assert_eq!(
            UsageProviderId::Fake.refresh_key(Service::Claude),
            "claude.fake"
        );
    }
}
