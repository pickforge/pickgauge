use crate::config::AppConfig;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
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

    fn provider_key(&self) -> String {
        self.provider_id().refresh_key(self.service())
    }
}

pub struct UsageEngine {
    inner: Mutex<UsageEngineState>,
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
    fn code(self) -> &'static str {
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
        let config = config.normalized();

        Self {
            inner: Mutex::new(UsageEngineState {
                providers: providers_for_config(&config),
                config,
                snapshots: HashMap::new(),
                active_provider_keys: HashSet::new(),
                last_updated: now_rfc3339(),
            }),
        }
    }

    pub fn config(&self) -> Result<AppConfig, String> {
        let state = self.lock()?;
        Ok(state.config.clone())
    }

    pub fn update_config(&self, config: AppConfig) -> Result<AppConfig, String> {
        let config = config.normalized();
        let providers = providers_for_config(&config);
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
                .map(|provider| ProviderDescriptor {
                    provider_key: provider.provider_key(),
                    provider_id: provider.provider_id(),
                    service: provider.service(),
                    source: provider.source(),
                })
                .collect::<Vec<_>>();
            let provider_services = providers
                .iter()
                .map(|provider| provider.service)
                .collect::<HashSet<_>>();

            (providers, provider_services)
        };

        let mut refreshed = Vec::new();
        let now = now_rfc3339();

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

struct ProviderDescriptor {
    provider_key: String,
    provider_id: UsageProviderId,
    service: Service,
    source: UsageSource,
}

fn providers_for_config(config: &AppConfig) -> Vec<Box<dyn UsageProvider>> {
    let mut providers: Vec<Box<dyn UsageProvider>> = Vec::new();

    if config.enabled_services.codex {
        providers.push(Box::new(FakeUsageProvider {
            service: Service::Codex,
            remaining_percent: 72.0,
        }));
    }

    if config.enabled_services.claude {
        providers.push(Box::new(FakeUsageProvider {
            service: Service::Claude,
            remaining_percent: 41.0,
        }));
    }

    providers
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

    fn config_with_services(codex: bool, claude: bool) -> AppConfig {
        AppConfig {
            enabled_services: crate::config::ServiceToggles { codex, claude },
            ..AppConfig::default()
        }
    }

    #[test]
    fn fake_provider_refreshes_enabled_services() {
        let engine = UsageEngine::new(AppConfig::default());
        let display_state = engine.refresh_all().expect("refresh succeeds");

        assert_eq!(display_state.snapshots.len(), 2);
        assert_eq!(display_state.snapshots[0].service, Service::Codex);
        assert_eq!(display_state.snapshots[0].remaining_percent, Some(72.0));
        assert_eq!(display_state.snapshots[0].source, UsageSource::Fake);
        assert_eq!(display_state.snapshots[1].service, Service::Claude);
        assert_eq!(display_state.snapshots[1].remaining_percent, Some(41.0));
    }

    #[test]
    fn disabled_services_clear_display_cache_and_tray_falls_back_to_unknown() {
        let engine = UsageEngine::new(AppConfig::default());
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
        let engine = UsageEngine::new(AppConfig::default());
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
        let engine = UsageEngine::new(AppConfig::default());

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
        let engine = UsageEngine::new(AppConfig::default());
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
        let engine = UsageEngine::new(AppConfig::default());
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
        let engine = UsageEngine::new(AppConfig::default());
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
    fn provider_errors_map_to_sanitized_unknown_snapshots() {
        let provider = ProviderDescriptor {
            provider_key: "codex.fake".to_string(),
            provider_id: UsageProviderId::Fake,
            service: Service::Codex,
            source: UsageSource::Fake,
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
