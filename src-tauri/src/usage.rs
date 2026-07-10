use crate::{
    config::{AppConfig, LocalQuotaLimitKind, LocalQuotaUsageUnit, LocalServiceQuotaSettings},
    local_provider::{ClaudeLocalProvider, CodexLocalProvider, LocalQuotaCalibration},
    ollama_provider::OllamaLocalProvider,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Mutex,
    time::Duration,
};
use tauri::{AppHandle, Emitter};
use time::{format_description::well_known::Rfc3339, Duration as TimeDuration, OffsetDateTime};

pub const SNAPSHOTS_UPDATED_EVENT: &str = "usage://snapshots-updated";
pub const REFRESH_STARTED_EVENT: &str = "usage://refresh-started";
pub const REFRESH_FINISHED_EVENT: &str = "usage://refresh-finished";
pub const PROVIDER_ERROR_EVENT: &str = "usage://provider-error";
const PROVIDER_BACKOFF_BASE_SECONDS: u64 = 30;
const PROVIDER_BACKOFF_MAX_SECONDS: u64 = 15 * 60;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Service {
    Codex,
    Claude,
    Grok,
    Ollama,
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

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRefreshEvent {
    pub service: Option<Service>,
    pub source: Option<UsageSource>,
    pub status: UsageRefreshStatus,
    pub emitted_at: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageProviderErrorEvent {
    pub service: Service,
    pub source: UsageSource,
    pub provider_id: String,
    pub status: String,
    pub emitted_at: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UsageRefreshStatus {
    Started,
    Finished,
    Failed,
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
    CodexCli,
    ClaudeLocal,
    ClaudeWeb,
    ClaudeCli,
    GrokCli,
    GrokWeb,
    OllamaLocal,
    OllamaWeb,
    Fake,
}

pub(crate) trait UsageProvider: Send + Sync {
    fn provider_id(&self) -> UsageProviderId;
    fn service(&self) -> Service;
    fn source(&self) -> UsageSource;
    fn refresh(&self, now: &str) -> Result<UsageSnapshot, UsageProviderError>;
    fn is_placeholder(&self) -> bool {
        false
    }
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
    snapshots: HashMap<String, UsageSnapshot>,
    active_provider_keys: HashSet<String>,
    provider_failures: HashMap<String, ProviderFailureState>,
    scheduled_provider_refreshes: HashMap<String, String>,
    manual_web_refreshes: HashMap<Service, String>,
    last_updated: String,
}

#[derive(Clone, Debug, PartialEq)]
struct ProviderFailureState {
    consecutive_failures: u32,
    backoff_seconds: u64,
    retry_after: String,
}

#[derive(Clone, Copy)]
struct FakeUsageProvider {
    service: Service,
    remaining_percent: f32,
}

#[derive(Clone, Copy)]
struct FailClosedWebProvider {
    service: Service,
}

#[derive(Clone, Copy)]
struct CliUsageProvider {
    service: Service,
}

struct SystemClock;

#[derive(Clone, Debug, Default)]
struct LocalProviderRoots {
    codex: Option<PathBuf>,
    claude: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RefreshPolicy {
    Manual,
    Preflight,
    Scheduled,
}

impl Service {
    pub fn code(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Grok => "grok",
            Self::Ollama => "ollama",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Claude => "Claude Code",
            Self::Grok => "Grok",
            Self::Ollama => "Ollama",
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
    pub fn code(self) -> &'static str {
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
            (Service::Grok, UsageSource::Local) => None,
            (Service::Grok, UsageSource::Web) => Some(Self::GrokWeb),
            (Service::Ollama, UsageSource::Local) => Some(Self::OllamaLocal),
            (Service::Ollama, UsageSource::Web) => Some(Self::OllamaWeb),
            (_, UsageSource::Fake) => Some(Self::Fake),
            (_, UsageSource::Merged) => None,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::CodexLocal => "codex.local",
            Self::CodexWeb => "codex.web",
            Self::CodexCli => "codex.cli",
            Self::ClaudeLocal => "claude.local",
            Self::ClaudeWeb => "claude.web",
            Self::ClaudeCli => "claude.cli",
            Self::GrokCli => "grok.cli",
            Self::GrokWeb => "grok.web",
            Self::OllamaLocal => "ollama.local",
            Self::OllamaWeb => "ollama.web",
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
            .filter(|snapshot| {
                snapshot.remaining_percent.is_some()
                    || snapshot
                        .details
                        .get("plan")
                        .and_then(serde_json::Value::as_str)
                        .is_none()
            })
            .map(|snapshot| TrayGaugeState {
                service: snapshot.service,
                remaining_percent: snapshot.remaining_percent,
            })
            .collect();

        states.sort_by_key(|state| match state.service {
            Service::Codex => 0,
            Service::Claude => 1,
            Service::Grok => 2,
            Service::Ollama => 3,
        });

        if states.is_empty() {
            let service = self
                .snapshots
                .iter()
                .map(|snapshot| snapshot.service)
                .min_by_key(|service| match service {
                    Service::Codex => 0,
                    Service::Claude => 1,
                    Service::Grok => 2,
                    Service::Ollama => 3,
                })
                .unwrap_or(Service::Codex);
            states.push(TrayGaugeState {
                service,
                remaining_percent: None,
            });
        }

        states
    }
}

impl UsageRefreshEvent {
    pub fn new(
        service: Option<Service>,
        source: Option<UsageSource>,
        status: UsageRefreshStatus,
        emitted_at: String,
    ) -> Self {
        Self {
            service,
            source,
            status,
            emitted_at,
        }
    }
}

impl UsageProviderErrorEvent {
    pub fn new(
        service: Service,
        source: UsageSource,
        provider_id: impl Into<String>,
        status: impl Into<String>,
        emitted_at: String,
    ) -> Self {
        Self {
            service,
            source,
            provider_id: provider_id.into(),
            status: status.into(),
            emitted_at,
        }
    }
}

impl UsageEngine {
    pub fn new(config: AppConfig) -> Self {
        Self::with_clock(config, Box::new(SystemClock))
    }

    pub(crate) fn new_headless(config: AppConfig) -> Self {
        let mut engine = Self::new(config);
        let state = engine
            .inner
            .get_mut()
            .expect("new usage engine state is not poisoned");
        state
            .providers
            .retain(|provider| provider.source() != UsageSource::Fake);
        engine
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
                provider_failures: HashMap::new(),
                scheduled_provider_refreshes: HashMap::new(),
                manual_web_refreshes: HashMap::new(),
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
            .retain(|provider_key, _| provider_keys.contains(provider_key));
        state
            .active_provider_keys
            .retain(|key| provider_keys.contains(key));
        state
            .provider_failures
            .retain(|key, _| provider_keys.contains(key));
        state
            .scheduled_provider_refreshes
            .retain(|key, _| provider_keys.contains(key));
        if config.providers.web_enabled {
            state
                .manual_web_refreshes
                .retain(|service, _| config.service_enabled(*service));
        } else {
            state.manual_web_refreshes.clear();
        }

        Ok(config)
    }

    pub fn display_state(&self) -> Result<UsageDisplayState, String> {
        let now = self.clock.now_rfc3339();
        let state = self.lock()?;
        Ok(state.display_state(&now))
    }

    pub fn snapshots(&self) -> Result<Vec<UsageSnapshot>, String> {
        Ok(self.display_state()?.snapshots)
    }

    pub(crate) fn raw_snapshots(&self) -> Result<HashMap<String, UsageSnapshot>, String> {
        let state = self.lock()?;
        Ok(state.snapshots.clone())
    }

    pub(crate) fn overlay_persisted_snapshots(
        &self,
        persisted_snapshots: HashMap<String, UsageSnapshot>,
    ) -> Result<UsageDisplayState, String> {
        let now = self.clock.now_rfc3339();
        let mut state = self.lock()?;
        let placeholder_keys = state
            .providers
            .iter()
            .filter(|provider| {
                provider.is_placeholder() && provider.source() == UsageSource::Web
            })
            .map(|provider| (provider.provider_key(), provider.service()))
            .collect::<Vec<_>>();

        for (provider_key, service) in placeholder_keys {
            let Some(live_snapshot) = state.snapshots.get(&provider_key) else {
                continue;
            };
            let live_login_required = live_snapshot
                .details
                .get("status")
                .and_then(serde_json::Value::as_str)
                == Some(UsageProviderError::LoginRequired.code());
            let Some(persisted_snapshot) = persisted_snapshots.get(&provider_key) else {
                continue;
            };

            if live_login_required
                && persisted_snapshot.service == service
                && persisted_snapshot.source == UsageSource::Web
            {
                state
                    .snapshots
                    .insert(provider_key, persisted_snapshot.clone());
            }
        }

        Ok(headless_display_state(&state, &now))
    }

    pub fn clear_cached_snapshots(&self) -> Result<UsageDisplayState, String> {
        let now = self.clock.now_rfc3339();
        let mut state = self.lock()?;

        state.snapshots.clear();
        state.last_updated = now;
        Ok(state.display_state(&state.last_updated))
    }

    pub fn refresh_all(&self) -> Result<UsageDisplayState, String> {
        self.refresh_all_with_policy(RefreshPolicy::Manual)
    }

    pub fn refresh_due(&self) -> Result<UsageDisplayState, String> {
        self.refresh_all_with_policy(RefreshPolicy::Scheduled)
    }

    fn refresh_all_with_policy(&self, policy: RefreshPolicy) -> Result<UsageDisplayState, String> {
        let (providers, provider_keys, config, scheduled_provider_refreshes) = {
            let state = self.lock()?;
            let providers = state
                .providers
                .iter()
                .map(|provider| provider_descriptor(provider.as_ref()))
                .collect::<Vec<_>>();
            let provider_keys = providers
                .iter()
                .map(|provider| provider.provider_key.clone())
                .collect::<HashSet<_>>();

            (
                providers,
                provider_keys,
                state.config.clone(),
                state.scheduled_provider_refreshes.clone(),
            )
        };

        let mut refreshed = Vec::new();
        let now = self.clock.now_rfc3339();

        for provider in providers {
            if policy == RefreshPolicy::Scheduled
                && !provider_refresh_due(
                    scheduled_provider_refreshes.get(&provider.provider_key),
                    &now,
                    provider_refresh_interval(&config, provider.source),
                )
            {
                continue;
            }

            if !self.try_begin_refresh_with_backoff(
                provider.provider_key.clone(),
                provider.source,
                !provider.is_placeholder,
                &now,
            )? {
                continue;
            }

            let snapshot = match self.refresh_provider(&provider, &now) {
                Ok(snapshot) => {
                    self.record_provider_success(&provider.provider_key)?;
                    snapshot
                }
                Err(error) => {
                    let failure = if provider.is_placeholder {
                        None
                    } else {
                        self.record_provider_failure(&provider.provider_key, provider.source, &now)?
                    };
                    let mut snapshot = error_snapshot(&provider, error, &now);
                    if let Some(failure) = failure {
                        add_failure_details(&mut snapshot, &failure);
                    }
                    snapshot
                }
            };
            self.record_scheduled_provider_refresh(&provider.provider_key, &now)?;
            refreshed.push((provider.provider_key.clone(), snapshot));
            self.finish_refresh(&provider.provider_key)?;
        }

        let mut state = self.lock()?;
        state
            .snapshots
            .retain(|provider_key, _| provider_keys.contains(provider_key));

        let refreshed_any = !refreshed.is_empty();

        for (provider_key, snapshot) in refreshed {
            state.snapshots.insert(provider_key, snapshot);
        }

        if policy == RefreshPolicy::Manual || refreshed_any {
            state.last_updated = now.clone();
        }
        Ok(state.display_state(&now))
    }

    pub fn refresh_provider_source(
        &self,
        service: Service,
        source: UsageSource,
    ) -> Result<UsageDisplayState, String> {
        self.refresh_provider_source_with_snapshot_policy(
            service,
            source,
            RefreshPolicy::Manual,
            false,
            |now| {
                let provider_id = UsageProviderId::for_service_source(service, source)
                    .ok_or(UsageProviderError::Internal)?;
                let provider = {
                    let state = self
                        .inner
                        .lock()
                        .map_err(|_| UsageProviderError::Internal)?;
                    state
                        .providers
                        .iter()
                        .map(|provider| provider_descriptor(provider.as_ref()))
                        .find(|provider| {
                            provider.service == service && provider.provider_id == provider_id
                        })
                }
                .ok_or(UsageProviderError::Internal)?;

                self.refresh_provider(&provider, now)
            },
        )
    }

    pub fn refresh_provider_source_with_snapshot<F>(
        &self,
        service: Service,
        source: UsageSource,
        refresh: F,
    ) -> Result<UsageDisplayState, String>
    where
        F: FnOnce(&str) -> Result<UsageSnapshot, UsageProviderError>,
    {
        self.refresh_provider_source_with_snapshot_policy(
            service,
            source,
            RefreshPolicy::Manual,
            true,
            refresh,
        )
    }

    pub fn refresh_due_provider_source_with_snapshot<F>(
        &self,
        service: Service,
        source: UsageSource,
        refresh: F,
    ) -> Result<UsageDisplayState, String>
    where
        F: FnOnce(&str) -> Result<UsageSnapshot, UsageProviderError>,
    {
        self.refresh_provider_source_with_snapshot_policy(
            service,
            source,
            RefreshPolicy::Scheduled,
            true,
            refresh,
        )
    }

    pub fn refresh_preflight_provider_source_with_snapshot<F>(
        &self,
        service: Service,
        source: UsageSource,
        refresh: F,
    ) -> Result<UsageDisplayState, String>
    where
        F: FnOnce(&str) -> Result<UsageSnapshot, UsageProviderError>,
    {
        self.refresh_provider_source_with_snapshot_policy(
            service,
            source,
            RefreshPolicy::Preflight,
            true,
            refresh,
        )
    }

    fn refresh_provider_source_with_snapshot_policy<F>(
        &self,
        service: Service,
        source: UsageSource,
        policy: RefreshPolicy,
        external_refresh: bool,
        refresh: F,
    ) -> Result<UsageDisplayState, String>
    where
        F: FnOnce(&str) -> Result<UsageSnapshot, UsageProviderError>,
    {
        let provider_id = UsageProviderId::for_service_source(service, source)
            .ok_or_else(|| "Provider source cannot be refreshed directly".to_string())?;
        let now = self.clock.now_rfc3339();

        if policy == RefreshPolicy::Manual && source == UsageSource::Web {
            self.ensure_manual_web_refresh_allowed(service, &now)?;
        }

        let (provider, last_scheduled_refresh, config) = {
            let state = self.lock()?;
            let provider = state
                .providers
                .iter()
                .map(|provider| provider_descriptor(provider.as_ref()))
                .find(|provider| provider.service == service && provider.provider_id == provider_id)
                .ok_or_else(|| "Provider is not configured".to_string())?;
            let last_scheduled_refresh = state
                .scheduled_provider_refreshes
                .get(&provider.provider_key)
                .cloned();

            (provider, last_scheduled_refresh, state.config.clone())
        };

        if policy == RefreshPolicy::Scheduled
            && !provider_refresh_due(
                last_scheduled_refresh.as_ref(),
                &now,
                provider_refresh_interval(&config, source),
            )
        {
            return self.display_state();
        }

        let apply_backoff = external_refresh || !provider.is_placeholder;
        if !self.try_begin_refresh_with_backoff(
            provider.provider_key.clone(),
            provider.source,
            apply_backoff,
            &now,
        )? {
            return self.display_state();
        }

        let snapshot = match refresh(&now) {
            Ok(snapshot) if snapshot.service == service && snapshot.source == source => {
                self.record_provider_success(&provider.provider_key)?;
                snapshot
            }
            Ok(_) => {
                let failure = if apply_backoff {
                    self.record_provider_failure(&provider.provider_key, provider.source, &now)?
                } else {
                    None
                };
                let mut snapshot = error_snapshot(&provider, UsageProviderError::Internal, &now);
                if let Some(failure) = failure {
                    add_failure_details(&mut snapshot, &failure);
                }
                snapshot
            }
            Err(error) => {
                let failure = if apply_backoff {
                    self.record_provider_failure(&provider.provider_key, provider.source, &now)?
                } else {
                    None
                };
                let mut snapshot = error_snapshot(&provider, error, &now);
                if let Some(failure) = failure {
                    add_failure_details(&mut snapshot, &failure);
                }
                snapshot
            }
        };
        self.record_scheduled_provider_refresh(&provider.provider_key, &now)?;
        if policy == RefreshPolicy::Manual && source == UsageSource::Web {
            self.record_manual_web_refresh(service, &now)?;
        }
        self.finish_refresh(&provider.provider_key)?;

        let mut state = self.lock()?;
        state.snapshots.insert(provider.provider_key, snapshot);
        state.last_updated = now.clone();
        Ok(state.display_state(&now))
    }

    pub fn refresh_all_and_emit(&self, app: &AppHandle) -> Result<UsageDisplayState, String> {
        let display_state = self.refresh_all()?;
        app.emit(SNAPSHOTS_UPDATED_EVENT, &display_state)
            .map_err(|error| format!("Could not emit usage update: {error}"))?;
        Ok(display_state)
    }

    pub fn refresh_due_and_emit(&self, app: &AppHandle) -> Result<UsageDisplayState, String> {
        let display_state = self.refresh_due()?;
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
            (UsageProviderId::OllamaLocal, Service::Ollama) => {
                OllamaLocalProvider::new().refresh(now)
            }
            (UsageProviderId::CodexWeb, Service::Codex)
            | (UsageProviderId::ClaudeWeb, Service::Claude)
            | (UsageProviderId::GrokWeb, Service::Grok)
            | (UsageProviderId::OllamaWeb, Service::Ollama) => {
                Err(UsageProviderError::LoginRequired)
            }
            (UsageProviderId::CodexCli, Service::Codex) => {
                crate::cli_provider::refresh(Service::Codex, now)
            }
            (UsageProviderId::ClaudeCli, Service::Claude) => {
                crate::cli_provider::refresh(Service::Claude, now)
            }
            (UsageProviderId::GrokCli, Service::Grok) => {
                crate::cli_provider::refresh(Service::Grok, now)
            }
            _ => Err(UsageProviderError::Internal),
        }
    }

    #[cfg(test)]
    fn try_begin_refresh(
        &self,
        provider_key: String,
        source: UsageSource,
        now: &str,
    ) -> Result<bool, String> {
        self.try_begin_refresh_with_backoff(provider_key, source, true, now)
    }

    fn try_begin_refresh_with_backoff(
        &self,
        provider_key: String,
        source: UsageSource,
        apply_backoff: bool,
        now: &str,
    ) -> Result<bool, String> {
        let mut state = self.lock()?;

        if state.active_provider_keys.contains(&provider_key) {
            return Ok(false);
        }

        if apply_backoff
            && source != UsageSource::Local
            && provider_backoff_active(state.provider_failures.get(&provider_key), now)
        {
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

    fn record_provider_failure(
        &self,
        provider_key: &str,
        source: UsageSource,
        now: &str,
    ) -> Result<Option<ProviderFailureState>, String> {
        if source == UsageSource::Local {
            return Ok(None);
        }

        let mut state = self.lock()?;
        let entry = state
            .provider_failures
            .entry(provider_key.to_string())
            .or_insert_with(|| ProviderFailureState {
                consecutive_failures: 0,
                backoff_seconds: 0,
                retry_after: now.to_string(),
            });
        let consecutive_failures = entry.consecutive_failures.saturating_add(1);
        let backoff_seconds = provider_backoff_seconds(consecutive_failures);
        let retry_after = retry_after_rfc3339(now, backoff_seconds);

        *entry = ProviderFailureState {
            consecutive_failures,
            backoff_seconds,
            retry_after,
        };

        Ok(Some(entry.clone()))
    }

    fn record_provider_success(&self, provider_key: &str) -> Result<(), String> {
        let mut state = self.lock()?;
        state.provider_failures.remove(provider_key);
        Ok(())
    }

    fn record_scheduled_provider_refresh(
        &self,
        provider_key: &str,
        now: &str,
    ) -> Result<(), String> {
        let mut state = self.lock()?;
        state
            .scheduled_provider_refreshes
            .insert(provider_key.to_string(), now.to_string());
        Ok(())
    }

    fn ensure_manual_web_refresh_allowed(&self, service: Service, now: &str) -> Result<(), String> {
        let state = self.lock()?;

        if !state.config.providers.web_enabled {
            return Err("Web providers are disabled".to_string());
        }

        let cooldown =
            Duration::from_secs(state.config.intervals.manual_web_refresh_cooldown_seconds);
        if manual_web_refresh_cooldown_active(
            state.manual_web_refreshes.get(&service),
            now,
            cooldown,
        ) {
            return Err("Manual web refresh is cooling down".to_string());
        }

        Ok(())
    }

    fn record_manual_web_refresh(&self, service: Service, now: &str) -> Result<(), String> {
        let mut state = self.lock()?;
        state.manual_web_refreshes.insert(service, now.to_string());
        Ok(())
    }

    #[cfg(test)]
    fn provider_failure_state(
        &self,
        provider_key: &str,
    ) -> Result<Option<ProviderFailureState>, String> {
        let state = self.lock()?;
        Ok(state.provider_failures.get(provider_key).cloned())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, UsageEngineState>, String> {
        self.inner
            .lock()
            .map_err(|_| "Usage engine state lock was poisoned".to_string())
    }
}

impl UsageEngineState {
    fn display_state(&self, now: &str) -> UsageDisplayState {
        let mut snapshots = merged_display_snapshots(&self.snapshots, &self.config, now);
        for snapshot in &mut snapshots {
            add_stale_details(snapshot, &self.config, now);
        }
        snapshots.sort_by_key(|snapshot| match snapshot.service {
            Service::Codex => 0,
            Service::Claude => 1,
            Service::Grok => 2,
            Service::Ollama => 3,
        });

        UsageDisplayState {
            snapshots,
            updated_at: self.last_updated.clone(),
        }
    }
}

fn merged_display_snapshots(
    snapshots: &HashMap<String, UsageSnapshot>,
    config: &AppConfig,
    now: &str,
) -> Vec<UsageSnapshot> {
    [Service::Codex, Service::Claude, Service::Grok, Service::Ollama]
        .into_iter()
        .filter(|service| config.service_enabled(*service))
        .filter_map(|service| {
            let service_snapshots = snapshots
                .values()
                .filter(|snapshot| snapshot.service == service)
                .collect::<Vec<_>>();
            merge_service_snapshots(service_snapshots, config, now)
        })
        .collect()
}

fn headless_display_state(state: &UsageEngineState, now: &str) -> UsageDisplayState {
    let mut display_state = state.display_state(now);
    let present_services = display_state
        .snapshots
        .iter()
        .map(|snapshot| snapshot.service)
        .collect::<HashSet<_>>();

    for service in [Service::Codex, Service::Claude, Service::Grok, Service::Ollama] {
        if state.config.service_enabled(service) && !present_services.contains(&service) {
            display_state
                .snapshots
                .push(unavailable_service_snapshot(service, now));
        }
    }

    display_state.snapshots.sort_by_key(|snapshot| match snapshot.service {
        Service::Codex => 0,
        Service::Claude => 1,
        Service::Grok => 2,
        Service::Ollama => 3,
    });
    display_state
}

fn unavailable_service_snapshot(service: Service, now: &str) -> UsageSnapshot {
    UsageSnapshot {
        service,
        remaining_percent: None,
        used_percent: None,
        reset_at: None,
        source: UsageSource::Merged,
        confidence: UsageConfidence::Unknown,
        last_updated: now.to_string(),
        details: serde_json::json!({
            "status": UsageProviderError::NotConfigured.code(),
            "providerId": "merged",
            "source": UsageSource::Merged.code(),
        }),
    }
}

fn merge_service_snapshots(
    snapshots: Vec<&UsageSnapshot>,
    config: &AppConfig,
    now: &str,
) -> Option<UsageSnapshot> {
    let web = preferred_web_snapshot(&snapshots)
        .map(|snapshot| web_snapshot_with_carried_plan(snapshot, &snapshots));
    let local = latest_snapshot_with_source(&snapshots, UsageSource::Local);
    let fake = latest_snapshot_with_source(&snapshots, UsageSource::Fake);

    if let Some(web) = web.as_ref() {
        if !web_has_usage_percent(web) {
            if let Some(local) = local.filter(|snapshot| is_plan_only_local_snapshot(snapshot)) {
                return Some(local.clone());
            }

            if let Some(fallback) = local.or(fake) {
                return Some(web_unavailable_fallback_snapshot(web, fallback));
            }

            let mut snapshot = web.clone();
            set_detail(&mut snapshot, "mergeStatus", "web_unavailable");
            set_detail(
                &mut snapshot,
                "lastOfficialCheckAt",
                web.last_updated.clone(),
            );
            return Some(snapshot);
        }

        if web_baseline_stale(web, config, now) {
            let mut snapshot = web.clone();
            snapshot.confidence = lower_confidence(snapshot.confidence);
            set_detail(&mut snapshot, "mergeStatus", "stale_web_baseline");
            set_detail(&mut snapshot, "baselineAt", web.last_updated.clone());
            set_detail(
                &mut snapshot,
                "lastOfficialCheckAt",
                web.last_updated.clone(),
            );
            return Some(snapshot);
        }

        if let Some(local) = local.filter(|snapshot| !is_plan_only_local_snapshot(snapshot)) {
            if let Some(delta_percent) = local_delta_percent(local, &web.last_updated) {
                return Some(merged_web_and_local_snapshot(web, local, delta_percent));
            }

            let mut snapshot = web.clone();
            snapshot.confidence = lower_confidence(snapshot.confidence);
            set_detail(&mut snapshot, "mergeStatus", "local_delta_unavailable");
            set_detail(&mut snapshot, "baselineAt", web.last_updated.clone());
            set_detail(
                &mut snapshot,
                "lastOfficialCheckAt",
                web.last_updated.clone(),
            );
            return Some(snapshot);
        }

        let mut snapshot = web.clone();
        set_detail(&mut snapshot, "mergeStatus", "web_only");
        set_detail(&mut snapshot, "baselineAt", web.last_updated.clone());
        set_detail(
            &mut snapshot,
            "lastOfficialCheckAt",
            web.last_updated.clone(),
        );
        return Some(snapshot);
    }

    local.cloned().or_else(|| fake.cloned())
}

fn web_has_usage_percent(snapshot: &UsageSnapshot) -> bool {
    snapshot.remaining_percent.is_some() || snapshot.used_percent.is_some()
}

fn is_plan_only_local_snapshot(snapshot: &UsageSnapshot) -> bool {
    snapshot.remaining_percent.is_none()
        && snapshot.used_percent.is_none()
        && snapshot
            .details
            .get("plan")
            .and_then(|plan| plan.as_str())
            .is_some()
}

fn web_unavailable_fallback_snapshot(
    web: &UsageSnapshot,
    fallback: &UsageSnapshot,
) -> UsageSnapshot {
    let mut snapshot = fallback.clone();
    snapshot.confidence = lower_confidence(snapshot.confidence);
    set_detail(&mut snapshot, "mergeStatus", "web_unavailable_fallback");
    set_detail(
        &mut snapshot,
        "lastOfficialCheckAt",
        web.last_updated.clone(),
    );

    if let Some(status) = web.details.get("status").cloned() {
        set_detail(&mut snapshot, "webStatus", status);
    }

    if let Some(reason) = web.details.get("reason").and_then(sanitized_web_reason) {
        set_detail(&mut snapshot, "webReason", reason);
    }

    if let Some(provider_id) = web.details.get("providerId").cloned() {
        set_detail(&mut snapshot, "webProviderId", provider_id);
    }

    snapshot
}

fn sanitized_web_reason(value: &serde_json::Value) -> Option<String> {
    let reason = value.as_str()?;

    if reason.is_empty() || reason.len() > 64 {
        return None;
    }

    if reason
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Some(reason.to_string());
    }

    None
}

fn latest_snapshot_with_source<'a>(
    snapshots: &'a [&'a UsageSnapshot],
    source: UsageSource,
) -> Option<&'a UsageSnapshot> {
    snapshots
        .iter()
        .copied()
        .filter(|snapshot| snapshot.source == source)
        .max_by_key(|snapshot| parse_rfc3339(&snapshot.last_updated))
}

fn preferred_web_snapshot<'a>(snapshots: &'a [&'a UsageSnapshot]) -> Option<&'a UsageSnapshot> {
    snapshots
        .iter()
        .copied()
        .filter(|snapshot| snapshot.source == UsageSource::Web)
        .max_by(|left, right| {
            web_has_usage_percent(left)
                .cmp(&web_has_usage_percent(right))
                .then_with(|| web_snapshot_is_parsed(left).cmp(&web_snapshot_is_parsed(right)))
                .then_with(|| {
                    parse_rfc3339(&left.last_updated).cmp(&parse_rfc3339(&right.last_updated))
                })
        })
}

fn web_snapshot_is_parsed(snapshot: &UsageSnapshot) -> bool {
    snapshot.details.get("status").and_then(|value| value.as_str()) == Some("parsed")
}

fn web_snapshot_with_carried_plan(
    web: &UsageSnapshot,
    siblings: &[&UsageSnapshot],
) -> UsageSnapshot {
    let mut snapshot = web.clone();
    if snapshot
        .details
        .get("plan")
        .and_then(|value| value.as_str())
        .is_some()
    {
        return snapshot;
    }

    let plan_sibling = siblings
        .iter()
        .copied()
        .filter(|candidate| {
            candidate
                .details
                .get("plan")
                .and_then(|value| value.as_str())
                .is_some()
        })
        .max_by_key(|candidate| parse_rfc3339(&candidate.last_updated));

    let Some(plan_sibling) = plan_sibling else {
        return snapshot;
    };

    if let Some(plan) = plan_sibling.details.get("plan").cloned() {
        set_detail(&mut snapshot, "plan", plan);
    }
    if snapshot
        .details
        .get("billingPeriodEnd")
        .and_then(|value| value.as_str())
        .is_none()
    {
        if let Some(billing_period_end) = plan_sibling.details.get("billingPeriodEnd").cloned() {
            set_detail(&mut snapshot, "billingPeriodEnd", billing_period_end);
        }
    }

    snapshot
}

fn web_baseline_stale(snapshot: &UsageSnapshot, config: &AppConfig, now: &str) -> bool {
    snapshot_age_seconds(&snapshot.last_updated, now)
        .map(|age| age > provider_refresh_interval(config, UsageSource::Web).as_secs())
        .unwrap_or(false)
}

fn local_delta_percent(local: &UsageSnapshot, baseline_at: &str) -> Option<f32> {
    let details = local.details.as_object()?;
    let delta_baseline_at = details.get("deltaBaselineAt")?.as_str()?;
    let delta_unit = details.get("deltaUnit")?.as_str()?;
    let calibration_status = details.get("calibrationStatus")?.as_str()?;

    if delta_baseline_at != baseline_at
        || delta_unit != "percent"
        || calibration_status != "active"
        || local.confidence == UsageConfidence::Unknown
    {
        return None;
    }

    if parse_rfc3339(&local.last_updated)? < parse_rfc3339(baseline_at)? {
        return None;
    }

    local.used_percent
}

fn merged_web_and_local_snapshot(
    web: &UsageSnapshot,
    local: &UsageSnapshot,
    delta_percent: f32,
) -> UsageSnapshot {
    let web_remaining = web.remaining_percent.unwrap_or(0.0);
    let web_used = web.used_percent.unwrap_or(100.0 - web_remaining);
    let used_percent = (web_used + delta_percent).clamp(0.0, 100.0);
    let remaining_percent = (100.0 - used_percent).clamp(0.0, 100.0);
    let mut details = serde_json::json!({
        "status": "parsed",
        "providerId": "merged",
        "source": UsageSource::Merged.code(),
        "mergeStatus": "web_plus_local_delta",
        "baselineAt": web.last_updated.clone(),
        "lastOfficialCheckAt": web.last_updated.clone(),
        "localDeltaAt": local.last_updated.clone(),
        "localDeltaPercent": delta_percent,
        "webProviderId": web.details.get("providerId").cloned(),
        "localProviderId": local.details.get("providerId").cloned(),
    });

    remove_null_details(&mut details);

    UsageSnapshot {
        service: web.service,
        remaining_percent: Some(remaining_percent),
        used_percent: Some(used_percent),
        reset_at: web.reset_at.clone(),
        source: UsageSource::Merged,
        confidence: lower_confidence(web.confidence),
        last_updated: latest_rfc3339(&web.last_updated, &local.last_updated),
        details,
    }
}

fn latest_rfc3339(first: &str, second: &str) -> String {
    match (parse_rfc3339(first), parse_rfc3339(second)) {
        (Some(first_time), Some(second_time)) if second_time > first_time => second.to_string(),
        _ => first.to_string(),
    }
}

fn lower_confidence(confidence: UsageConfidence) -> UsageConfidence {
    match confidence {
        UsageConfidence::High => UsageConfidence::Medium,
        UsageConfidence::Medium => UsageConfidence::Low,
        UsageConfidence::Low | UsageConfidence::Unknown => confidence,
    }
}

fn set_detail(
    snapshot: &mut UsageSnapshot,
    key: impl Into<String>,
    value: impl Into<serde_json::Value>,
) {
    if let Some(details) = snapshot.details.as_object_mut() {
        details.insert(key.into(), value.into());
    }
}

fn remove_null_details(details: &mut serde_json::Value) {
    if let Some(object) = details.as_object_mut() {
        object.retain(|_, value| !value.is_null());
    }
}

impl AppConfig {
    fn service_enabled(&self, service: Service) -> bool {
        match service {
            Service::Codex => self.enabled_services.codex,
            Service::Claude => self.enabled_services.claude,
            Service::Grok => self.enabled_services.grok,
            Service::Ollama => self.enabled_services.ollama,
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

impl UsageProvider for FailClosedWebProvider {
    fn provider_id(&self) -> UsageProviderId {
        UsageProviderId::for_service_source(self.service(), self.source())
            .expect("web providers have a provider id")
    }

    fn service(&self) -> Service {
        self.service
    }

    fn source(&self) -> UsageSource {
        UsageSource::Web
    }

    fn refresh(&self, _now: &str) -> Result<UsageSnapshot, UsageProviderError> {
        Err(UsageProviderError::LoginRequired)
    }

    fn is_placeholder(&self) -> bool {
        true
    }
}

impl UsageProvider for CliUsageProvider {
    fn provider_id(&self) -> UsageProviderId {
        match self.service {
            Service::Codex => UsageProviderId::CodexCli,
            Service::Claude => UsageProviderId::ClaudeCli,
            Service::Grok => UsageProviderId::GrokCli,
            Service::Ollama => unreachable!("Ollama has no CLI provider"),
        }
    }

    fn service(&self) -> Service {
        self.service
    }

    // CLI readings ARE the official number, just fetched via the API rather
    // than the browser, so they flow through the same Web merge/priority path.
    fn source(&self) -> UsageSource {
        UsageSource::Web
    }

    fn refresh(&self, now: &str) -> Result<UsageSnapshot, UsageProviderError> {
        crate::cli_provider::refresh(self.service, now)
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
    is_placeholder: bool,
    local_data_root: Option<PathBuf>,
    local_calibration: Option<LocalQuotaCalibration>,
}

fn provider_descriptor(provider: &dyn UsageProvider) -> ProviderDescriptor {
    ProviderDescriptor {
        provider_key: provider.provider_key(),
        provider_id: provider.provider_id(),
        service: provider.service(),
        source: provider.source(),
        is_placeholder: provider.is_placeholder(),
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

        // CLI credentials take precedence over browser scraping for the
        // official (Web-source) reading.
        if config.providers.cli_enabled {
            providers.push(Box::new(CliUsageProvider {
                service: Service::Codex,
            }));
        } else if config.providers.web_enabled {
            providers.push(Box::new(FailClosedWebProvider {
                service: Service::Codex,
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

        if config.providers.cli_enabled {
            providers.push(Box::new(CliUsageProvider {
                service: Service::Claude,
            }));
        } else if config.providers.web_enabled {
            providers.push(Box::new(FailClosedWebProvider {
                service: Service::Claude,
            }));
        }
    }

    if config.enabled_services.grok {
        if config.providers.cli_enabled {
            providers.push(Box::new(CliUsageProvider {
                service: Service::Grok,
            }));
        }
        if config.providers.web_enabled {
            providers.push(Box::new(FailClosedWebProvider {
                service: Service::Grok,
            }));
        }
    }

    if config.enabled_services.ollama {
        if config.providers.local_enabled {
            providers.push(Box::new(OllamaLocalProvider::new()));
        }
        if config.providers.web_enabled {
            providers.push(Box::new(FailClosedWebProvider {
                service: Service::Ollama,
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

fn add_failure_details(snapshot: &mut UsageSnapshot, failure: &ProviderFailureState) {
    if let Some(details) = snapshot.details.as_object_mut() {
        details.insert(
            "consecutiveFailures".to_string(),
            serde_json::json!(failure.consecutive_failures),
        );
        details.insert(
            "backoffSeconds".to_string(),
            serde_json::json!(failure.backoff_seconds),
        );
        details.insert(
            "retryAfter".to_string(),
            serde_json::json!(failure.retry_after),
        );
    }
}

fn add_stale_details(snapshot: &mut UsageSnapshot, config: &AppConfig, now: &str) {
    let Some(stale_seconds) = snapshot_age_seconds(&snapshot.last_updated, now) else {
        return;
    };
    let stale = stale_seconds > provider_refresh_interval(config, snapshot.source).as_secs();

    if let Some(details) = snapshot.details.as_object_mut() {
        details.insert("stale".to_string(), serde_json::json!(stale));
        details.insert("staleSeconds".to_string(), serde_json::json!(stale_seconds));
    }
}

fn snapshot_age_seconds(last_updated: &str, now: &str) -> Option<u64> {
    let last_updated = parse_rfc3339(last_updated)?;
    let now = parse_rfc3339(now)?;

    if now < last_updated {
        return Some(0);
    }

    u64::try_from((now - last_updated).whole_seconds()).ok()
}

fn provider_backoff_seconds(consecutive_failures: u32) -> u64 {
    if consecutive_failures == 0 {
        return 0;
    }

    let exponent = consecutive_failures.saturating_sub(1).min(5);
    PROVIDER_BACKOFF_BASE_SECONDS
        .saturating_mul(1_u64 << exponent)
        .min(PROVIDER_BACKOFF_MAX_SECONDS)
}

fn provider_backoff_active(failure: Option<&ProviderFailureState>, now: &str) -> bool {
    let Some(failure) = failure else {
        return false;
    };
    let Some(now) = parse_rfc3339(now) else {
        return false;
    };
    let Some(retry_after) = parse_rfc3339(&failure.retry_after) else {
        return false;
    };

    now < retry_after
}

fn provider_refresh_interval(config: &AppConfig, source: UsageSource) -> Duration {
    match source {
        UsageSource::Web => Duration::from_secs(config.intervals.web_minutes.saturating_mul(60)),
        UsageSource::Local | UsageSource::Fake | UsageSource::Merged => {
            Duration::from_secs(config.intervals.local_seconds)
        }
    }
}

fn provider_refresh_due(last_refreshed_at: Option<&String>, now: &str, interval: Duration) -> bool {
    let Some(last_refreshed_at) = last_refreshed_at else {
        return true;
    };
    let Some(last_refreshed_at) = parse_rfc3339(last_refreshed_at) else {
        return true;
    };
    let Some(now) = parse_rfc3339(now) else {
        return true;
    };
    let seconds = i64::try_from(interval.as_secs()).unwrap_or(i64::MAX);

    now >= last_refreshed_at + TimeDuration::seconds(seconds)
}

fn manual_web_refresh_cooldown_active(
    last_refreshed_at: Option<&String>,
    now: &str,
    cooldown: Duration,
) -> bool {
    last_refreshed_at.is_some() && !provider_refresh_due(last_refreshed_at, now, cooldown)
}

fn retry_after_rfc3339(now: &str, backoff_seconds: u64) -> String {
    let Some(now) = parse_rfc3339(now) else {
        return now.to_string();
    };
    let seconds = i64::try_from(backoff_seconds).unwrap_or(i64::MAX);

    (now + TimeDuration::seconds(seconds))
        .format(&Rfc3339)
        .unwrap_or_else(|_| now_rfc3339())
}

fn parse_rfc3339(value: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(value, &Rfc3339).ok()
}

pub(crate) fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::VecDeque,
        fs,
        sync::{
            atomic::{AtomicU64, AtomicUsize, Ordering},
            Mutex as TestMutex,
        },
    };

    struct FixedClock {
        now: String,
    }

    struct SequenceClock {
        values: TestMutex<VecDeque<String>>,
    }

    static TEST_ID: AtomicU64 = AtomicU64::new(1);

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new() -> Self {
            let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("pickgauge-usage-test-{}-{id}", std::process::id()));

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

    impl SequenceClock {
        fn new(values: &[&str]) -> Self {
            Self {
                values: TestMutex::new(values.iter().map(|value| value.to_string()).collect()),
            }
        }
    }

    impl Clock for SequenceClock {
        fn now_rfc3339(&self) -> String {
            let mut values = self.values.lock().expect("sequence clock lock succeeds");

            if values.len() > 1 {
                return values.pop_front().expect("sequence clock has a value");
            }

            values
                .front()
                .cloned()
                .expect("sequence clock has a final value")
        }
    }

    fn config_with_services(codex: bool, claude: bool) -> AppConfig {
        AppConfig {
            enabled_services: crate::config::ServiceToggles {
                codex,
                claude,
                grok: false,
                ollama: false,
            },
            providers: crate::config::ProviderSettings {
                local_enabled: false,
                web_enabled: false,
                cli_enabled: false,
            },
            ..AppConfig::default()
        }
    }

    fn local_claude_config() -> AppConfig {
        AppConfig {
            enabled_services: crate::config::ServiceToggles {
                codex: false,
                claude: true,
                grok: false,
                ollama: false,
            },
            providers: crate::config::ProviderSettings {
                local_enabled: true,
                web_enabled: false,
                cli_enabled: false,
            },
            ..AppConfig::default()
        }
    }

    fn local_codex_config() -> AppConfig {
        AppConfig {
            enabled_services: crate::config::ServiceToggles {
                codex: true,
                claude: false,
                grok: false,
                ollama: false,
            },
            providers: crate::config::ProviderSettings {
                local_enabled: true,
                web_enabled: false,
                cli_enabled: false,
            },
            ..AppConfig::default()
        }
    }

    fn web_enabled_config() -> AppConfig {
        AppConfig {
            enabled_services: crate::config::ServiceToggles {
                codex: true,
                claude: true,
                grok: false,
                ollama: false,
            },
            providers: crate::config::ProviderSettings {
                local_enabled: false,
                web_enabled: true,
                cli_enabled: false,
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

    fn display_state_from_provider_snapshots(
        config: AppConfig,
        snapshots: Vec<(&str, UsageSnapshot)>,
        now: &str,
    ) -> UsageDisplayState {
        UsageEngineState {
            config: config.normalized(),
            providers: Vec::new(),
            snapshots: snapshots
                .into_iter()
                .map(|(key, snapshot)| (key.to_string(), snapshot))
                .collect(),
            active_provider_keys: HashSet::new(),
            provider_failures: HashMap::new(),
            scheduled_provider_refreshes: HashMap::new(),
            manual_web_refreshes: HashMap::new(),
            last_updated: now.to_string(),
        }
        .display_state(now)
    }

    fn web_snapshot(service: Service, remaining_percent: f32, last_updated: &str) -> UsageSnapshot {
        UsageSnapshot {
            service,
            remaining_percent: Some(remaining_percent),
            used_percent: Some(100.0 - remaining_percent),
            reset_at: Some("2026-06-04T00:00:00Z".to_string()),
            source: UsageSource::Web,
            confidence: UsageConfidence::High,
            last_updated: last_updated.to_string(),
            details: serde_json::json!({
                "status": "parsed",
                "providerId": UsageProviderId::for_service_source(service, UsageSource::Web)
                    .expect("web provider id")
                    .code(),
                "source": UsageSource::Web.code(),
            }),
        }
    }

    fn web_error_snapshot(
        service: Service,
        error: UsageProviderError,
        last_updated: &str,
    ) -> UsageSnapshot {
        let provider_id = UsageProviderId::for_service_source(service, UsageSource::Web)
            .expect("web provider id");
        let provider = ProviderDescriptor {
            provider_key: provider_id.refresh_key(service),
            provider_id,
            service,
            source: UsageSource::Web,
            is_placeholder: false,
            local_data_root: None,
            local_calibration: None,
        };

        error_snapshot(&provider, error, last_updated)
    }

    fn local_delta_snapshot(
        service: Service,
        used_delta_percent: f32,
        baseline_at: &str,
        last_updated: &str,
    ) -> UsageSnapshot {
        UsageSnapshot {
            service,
            remaining_percent: Some(100.0 - used_delta_percent),
            used_percent: Some(used_delta_percent),
            reset_at: None,
            source: UsageSource::Local,
            confidence: UsageConfidence::Low,
            last_updated: last_updated.to_string(),
            details: serde_json::json!({
                "status": "parsed",
                "providerId": UsageProviderId::for_service_source(service, UsageSource::Local)
                    .expect("local provider id")
                    .code(),
                "source": UsageSource::Local.code(),
                "calibrationStatus": "active",
                "deltaBaselineAt": baseline_at,
                "deltaUnit": "percent",
            }),
        }
    }

    fn uncalibrated_local_snapshot(service: Service, last_updated: &str) -> UsageSnapshot {
        UsageSnapshot {
            service,
            remaining_percent: None,
            used_percent: None,
            reset_at: None,
            source: UsageSource::Local,
            confidence: UsageConfidence::Low,
            last_updated: last_updated.to_string(),
            details: serde_json::json!({
                "status": "parsed",
                "providerId": UsageProviderId::for_service_source(service, UsageSource::Local)
                    .expect("local provider id")
                    .code(),
                "source": UsageSource::Local.code(),
                "calibrationStatus": "disabled",
            }),
        }
    }

    fn ollama_plan_snapshot(last_updated: &str) -> UsageSnapshot {
        UsageSnapshot {
            service: Service::Ollama,
            remaining_percent: None,
            used_percent: None,
            reset_at: None,
            source: UsageSource::Local,
            confidence: UsageConfidence::Medium,
            last_updated: last_updated.to_string(),
            details: serde_json::json!({
                "status": "parsed",
                "providerId": "ollama.local",
                "source": "local",
                "via": "daemon",
                "plan": "pro",
            }),
        }
    }

    fn grok_cli_plan_snapshot(last_updated: &str) -> UsageSnapshot {
        UsageSnapshot {
            service: Service::Grok,
            remaining_percent: None,
            used_percent: None,
            reset_at: None,
            source: UsageSource::Web,
            confidence: UsageConfidence::Medium,
            last_updated: last_updated.to_string(),
            details: serde_json::json!({
                "status": "parsed",
                "providerId": "grok.cli",
                "source": "web",
                "plan": "Grok Pro",
                "billingPeriodEnd": "2026-07-16T00:00:00Z",
            }),
        }
    }

    fn grok_web_config() -> AppConfig {
        AppConfig {
            enabled_services: crate::config::ServiceToggles {
                codex: false,
                claude: false,
                grok: true,
                ollama: false,
            },
            providers: crate::config::ProviderSettings {
                local_enabled: false,
                web_enabled: true,
                cli_enabled: true,
            },
            ..AppConfig::default()
        }
    }

    fn ollama_web_config() -> AppConfig {
        AppConfig {
            enabled_services: crate::config::ServiceToggles {
                codex: false,
                claude: false,
                grok: false,
                ollama: true,
            },
            providers: crate::config::ProviderSettings {
                local_enabled: true,
                web_enabled: true,
                cli_enabled: false,
            },
            ..AppConfig::default()
        }
    }

    fn assert_sanitized_details(label: &str, details: &serde_json::Value) {
        let forbidden_keys = [
            "account",
            "auth",
            "content",
            "cookie",
            "cwd",
            "email",
            "gitBranch",
            "html",
            "path",
            "preview",
            "raw",
            "requestId",
            "sessionId",
            "title",
            "token",
            "uuid",
        ];
        let forbidden_text = [
            "bearer ",
            "cookie=",
            "set-cookie",
            "auth.json",
            "access_token",
            "refresh_token",
            "sk-",
            "/home/",
            "/users/",
            "c:\\users\\",
            "<html",
            "<!doctype",
        ];

        fn visit(
            label: &str,
            value: &serde_json::Value,
            forbidden_keys: &[&str],
            forbidden_text: &[&str],
        ) {
            match value {
                serde_json::Value::Object(object) => {
                    for (key, value) in object {
                        let lower_key = key.to_ascii_lowercase();
                        assert!(
                            !forbidden_keys
                                .iter()
                                .any(|forbidden| lower_key == forbidden.to_ascii_lowercase()),
                            "{label} contains forbidden details key {key}"
                        );
                        visit(label, value, forbidden_keys, forbidden_text);
                    }
                }
                serde_json::Value::Array(values) => {
                    for value in values {
                        visit(label, value, forbidden_keys, forbidden_text);
                    }
                }
                serde_json::Value::String(text) => {
                    let lower_text = text.to_ascii_lowercase();
                    assert!(
                        !forbidden_text
                            .iter()
                            .any(|forbidden| lower_text.contains(forbidden)),
                        "{label} contains sensitive-looking details text"
                    );
                }
                _ => {}
            }
        }

        visit(label, details, &forbidden_keys, &forbidden_text);
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
    fn headless_display_includes_enabled_service_without_registered_provider() {
        let config = AppConfig {
            enabled_services: crate::config::ServiceToggles {
                codex: false,
                claude: false,
                grok: true,
                ollama: false,
            },
            providers: crate::config::ProviderSettings {
                local_enabled: false,
                web_enabled: false,
                cli_enabled: false,
            },
            ..AppConfig::default()
        };
        let engine = UsageEngine::with_clock(
            config,
            Box::new(FixedClock {
                now: "2026-07-09T12:00:00Z".to_string(),
            }),
        );

        let display_state = engine
            .overlay_persisted_snapshots(HashMap::new())
            .expect("headless display state loads");

        assert_eq!(display_state.snapshots.len(), 1);
        assert_eq!(display_state.snapshots[0].service, Service::Grok);
        assert_eq!(display_state.snapshots[0].source, UsageSource::Merged);
        assert_eq!(display_state.snapshots[0].confidence, UsageConfidence::Unknown);
        assert_eq!(display_state.snapshots[0].details["status"], "not_configured");
    }

    #[test]
    fn persisted_snapshot_replaces_live_fail_closed_login_placeholder() {
        let config = AppConfig {
            enabled_services: crate::config::ServiceToggles {
                codex: true,
                claude: false,
                grok: false,
                ollama: false,
            },
            providers: crate::config::ProviderSettings {
                local_enabled: false,
                web_enabled: true,
                cli_enabled: false,
            },
            ..AppConfig::default()
        };
        let engine = UsageEngine::with_clock(
            config,
            Box::new(FixedClock {
                now: "2026-07-09T12:00:00Z".to_string(),
            }),
        );
        engine.refresh_all().expect("placeholder refresh succeeds");
        let persisted = HashMap::from([(
            "codex.web".to_string(),
            web_snapshot(Service::Codex, 64.0, "2026-07-09T11:00:00Z"),
        )]);

        let display_state = engine
            .overlay_persisted_snapshots(persisted)
            .expect("persisted snapshot overlays placeholder");

        assert_eq!(display_state.snapshots[0].remaining_percent, Some(64.0));
        assert_eq!(display_state.snapshots[0].details["status"], "parsed");
    }

    #[test]
    fn persisted_snapshot_does_not_replace_live_cli_login_error() {
        let config = AppConfig {
            enabled_services: crate::config::ServiceToggles {
                codex: true,
                claude: false,
                grok: false,
                ollama: false,
            },
            providers: crate::config::ProviderSettings {
                local_enabled: false,
                web_enabled: true,
                cli_enabled: true,
            },
            ..AppConfig::default()
        };
        let engine = UsageEngine::with_clock(
            config,
            Box::new(FixedClock {
                now: "2026-07-09T12:00:00Z".to_string(),
            }),
        );
        let live_cli_error = UsageSnapshot {
            service: Service::Codex,
            remaining_percent: None,
            used_percent: None,
            reset_at: None,
            source: UsageSource::Web,
            confidence: UsageConfidence::Unknown,
            last_updated: "2026-07-09T12:00:00Z".to_string(),
            details: serde_json::json!({
                "status": "login_required",
                "providerId": "codex.cli",
                "source": "web",
            }),
        };
        engine
            .lock()
            .expect("engine state locks")
            .snapshots
            .insert("codex.cli".to_string(), live_cli_error);
        let persisted = HashMap::from([(
            "codex.cli".to_string(),
            web_snapshot(Service::Codex, 64.0, "2026-07-09T11:00:00Z"),
        )]);

        engine
            .overlay_persisted_snapshots(persisted)
            .expect("overlay completes");

        let raw_snapshots = engine.raw_snapshots().expect("raw snapshots load");
        assert_eq!(raw_snapshots["codex.cli"].details["status"], "login_required");
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
    fn clear_cached_snapshots_removes_display_state_without_changing_config() {
        let engine = UsageEngine::with_clock(
            config_with_services(true, true),
            Box::new(FixedClock {
                now: "2026-06-03T23:00:00Z".to_string(),
            }),
        );
        engine.refresh_all().expect("refresh succeeds");

        let display_state = engine
            .clear_cached_snapshots()
            .expect("cached snapshots clear");

        assert!(display_state.snapshots.is_empty());
        assert_eq!(display_state.updated_at, "2026-06-03T23:00:00Z");
        assert!(
            engine
                .config()
                .expect("config loads")
                .enabled_services
                .codex
        );
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
    fn tray_states_exclude_successful_plan_only_grok_snapshots() {
        let display_state = UsageDisplayState {
            snapshots: vec![
                UsageSnapshot {
                    service: Service::Grok,
                    remaining_percent: None,
                    used_percent: None,
                    reset_at: None,
                    source: UsageSource::Web,
                    confidence: UsageConfidence::Medium,
                    last_updated: "2026-07-09T20:00:00Z".to_string(),
                    details: serde_json::json!({
                        "status": "parsed",
                        "providerId": "grok.cli",
                        "plan": "Grok Pro",
                    }),
                },
                web_snapshot(Service::Codex, 72.0, "2026-07-09T20:00:00Z"),
            ],
            updated_at: "2026-07-09T20:00:00Z".to_string(),
        };

        assert_eq!(
            display_state.tray_states(),
            vec![TrayGaugeState {
                service: Service::Codex,
                remaining_percent: Some(72.0),
            }]
        );
    }

    #[test]
    fn tray_states_keep_grok_errors_without_a_plan() {
        let display_state = UsageDisplayState {
            snapshots: vec![UsageSnapshot {
                service: Service::Grok,
                remaining_percent: None,
                used_percent: None,
                reset_at: None,
                source: UsageSource::Web,
                confidence: UsageConfidence::Unknown,
                last_updated: "2026-07-09T20:00:00Z".to_string(),
                details: serde_json::json!({
                    "status": "login_required",
                    "providerId": "grok.cli",
                }),
            }],
            updated_at: "2026-07-09T20:00:00Z".to_string(),
        };

        assert_eq!(
            display_state.tray_states(),
            vec![TrayGaugeState {
                service: Service::Grok,
                remaining_percent: None,
            }]
        );
    }

    #[test]
    fn tray_states_use_grok_placeholder_for_a_grok_only_plan_snapshot() {
        let display_state = UsageDisplayState {
            snapshots: vec![UsageSnapshot {
                service: Service::Grok,
                remaining_percent: None,
                used_percent: None,
                reset_at: None,
                source: UsageSource::Web,
                confidence: UsageConfidence::Medium,
                last_updated: "2026-07-09T20:00:00Z".to_string(),
                details: serde_json::json!({
                    "status": "parsed",
                    "providerId": "grok.cli",
                    "plan": "Grok Pro",
                }),
            }],
            updated_at: "2026-07-09T20:00:00Z".to_string(),
        };

        assert_eq!(
            display_state.tray_states(),
            vec![TrayGaugeState {
                service: Service::Grok,
                remaining_percent: None,
            }]
        );
    }

    #[test]
    fn tray_states_use_codex_placeholder_when_snapshots_are_empty() {
        let display_state = UsageDisplayState {
            snapshots: Vec::new(),
            updated_at: "2026-07-09T20:00:00Z".to_string(),
        };

        assert_eq!(
            display_state.tray_states(),
            vec![TrayGaugeState {
                service: Service::Codex,
                remaining_percent: None,
            }]
        );
    }

    #[test]
    fn provider_refresh_overlap_is_skipped_until_finished() {
        let engine = UsageEngine::new(config_with_services(true, true));
        let now = "2026-06-03T23:00:00Z";

        assert!(engine
            .try_begin_refresh("codex.fake".to_string(), UsageSource::Fake, now)
            .expect("begin succeeds"));
        assert!(!engine
            .try_begin_refresh("codex.fake".to_string(), UsageSource::Fake, now)
            .expect("second begin is skipped"));

        engine
            .finish_refresh("codex.fake")
            .expect("finish succeeds");
        assert!(engine
            .try_begin_refresh("codex.fake".to_string(), UsageSource::Fake, now)
            .expect("begin after finish succeeds"));
    }

    #[test]
    fn skipped_provider_refresh_keeps_existing_cached_snapshot() {
        let engine = UsageEngine::new(config_with_services(true, true));
        engine.refresh_all().expect("initial refresh succeeds");
        assert!(engine
            .try_begin_refresh(
                "codex.fake".to_string(),
                UsageSource::Fake,
                "2026-06-03T23:00:00Z",
            )
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
            .try_begin_refresh(
                "codex.fake".to_string(),
                UsageSource::Fake,
                "2026-06-03T23:00:00Z",
            )
            .expect("begin succeeds"));

        engine
            .update_config(config_with_services(false, true))
            .expect("config update succeeds");

        assert!(engine
            .try_begin_refresh(
                "codex.fake".to_string(),
                UsageSource::Fake,
                "2026-06-03T23:00:00Z",
            )
            .expect("disabled provider key was cleared"));
    }

    #[test]
    fn web_provider_failure_backoff_is_bounded_and_blocks_retry_until_elapsed() {
        let engine = UsageEngine::new(config_with_services(true, true));
        let provider_key = "codex.web";

        let first = engine
            .record_provider_failure(provider_key, UsageSource::Web, "2026-06-03T23:00:00Z")
            .expect("failure records")
            .expect("web failure is tracked");
        let second = engine
            .record_provider_failure(provider_key, UsageSource::Web, "2026-06-03T23:00:30Z")
            .expect("second failure records")
            .expect("web failure is tracked");

        assert_eq!(first.consecutive_failures, 1);
        assert_eq!(first.backoff_seconds, 30);
        assert_eq!(first.retry_after, "2026-06-03T23:00:30Z");
        assert_eq!(second.consecutive_failures, 2);
        assert_eq!(second.backoff_seconds, 60);
        assert_eq!(second.retry_after, "2026-06-03T23:01:30Z");
        assert!(!engine
            .try_begin_refresh(
                provider_key.to_string(),
                UsageSource::Web,
                "2026-06-03T23:01:29Z",
            )
            .expect("backoff check succeeds"));
        assert!(engine
            .try_begin_refresh(
                provider_key.to_string(),
                UsageSource::Web,
                "2026-06-03T23:01:30Z",
            )
            .expect("retry after backoff succeeds"));

        for _ in 0..8 {
            engine
                .record_provider_failure(provider_key, UsageSource::Web, "2026-06-03T23:02:00Z")
                .expect("failure records")
                .expect("web failure is tracked");
        }
        let bounded = engine
            .provider_failure_state(provider_key)
            .expect("failure state reads")
            .expect("failure state exists");

        assert_eq!(bounded.backoff_seconds, PROVIDER_BACKOFF_MAX_SECONDS);
    }

    #[test]
    fn local_provider_failures_do_not_back_off_later_refreshes() {
        let config = AppConfig {
            enabled_services: crate::config::ServiceToggles {
                codex: false,
                claude: false,
                grok: false,
                ollama: true,
            },
            providers: crate::config::ProviderSettings {
                local_enabled: true,
                web_enabled: false,
                cli_enabled: false,
            },
            ..AppConfig::default()
        };
        let engine = UsageEngine::new(config);
        let refresh_calls = AtomicUsize::new(0);

        for _ in 0..2 {
            engine
                .refresh_provider_source_with_snapshot(Service::Ollama, UsageSource::Local, |_| {
                    refresh_calls.fetch_add(1, Ordering::Relaxed);
                    Err(UsageProviderError::NotConfigured)
                })
                .expect("local refresh records an unavailable snapshot");
        }

        assert_eq!(refresh_calls.load(Ordering::Relaxed), 2);
        assert_eq!(
            engine
                .provider_failure_state("ollama.local")
                .expect("failure state reads"),
            None
        );
    }

    #[test]
    fn provider_success_resets_failure_backoff_state() {
        let engine = UsageEngine::new(config_with_services(true, true));

        engine
            .record_provider_failure("codex.fake", UsageSource::Fake, "2026-06-03T23:00:00Z")
            .expect("failure records")
            .expect("fake failure is tracked");
        assert!(engine
            .provider_failure_state("codex.fake")
            .expect("failure state reads")
            .is_some());

        engine
            .record_provider_success("codex.fake")
            .expect("success records");

        assert_eq!(
            engine
                .provider_failure_state("codex.fake")
                .expect("failure state reads"),
            None
        );
    }

    #[test]
    fn disabling_a_provider_clears_failure_backoff_state() {
        let engine = UsageEngine::new(config_with_services(true, true));

        engine
            .record_provider_failure("codex.fake", UsageSource::Fake, "2026-06-03T23:00:00Z")
            .expect("failure records")
            .expect("fake failure is tracked");
        engine
            .update_config(config_with_services(false, true))
            .expect("config update succeeds");

        assert_eq!(
            engine
                .provider_failure_state("codex.fake")
                .expect("failure state reads"),
            None
        );
    }

    #[test]
    fn manual_web_refresh_requires_provider_opt_in() {
        let engine = UsageEngine::new(config_with_services(true, true));

        let error = engine
            .refresh_provider_source(Service::Codex, UsageSource::Web)
            .expect_err("web refresh requires opt-in");

        assert_eq!(error, "Web providers are disabled");
    }

    #[test]
    fn disabled_web_providers_are_not_registered_for_all_refreshes() {
        let engine = UsageEngine::new(config_with_services(true, true));
        let providers = {
            let state = engine.lock().expect("usage engine locks");
            state
                .providers
                .iter()
                .map(|provider| provider_descriptor(provider.as_ref()))
                .collect::<Vec<_>>()
        };

        assert!(providers
            .iter()
            .all(|provider| provider.source != UsageSource::Web));
        assert!(providers.iter().all(|provider| !matches!(
            provider.provider_id,
            UsageProviderId::CodexWeb | UsageProviderId::ClaudeWeb
        )));

        let display_state = engine.refresh_all().expect("refresh succeeds");

        assert!(display_state
            .snapshots
            .iter()
            .all(|snapshot| snapshot.source != UsageSource::Web));
    }

    #[test]
    fn ollama_local_provider_registers_with_local_readings() {
        let config = AppConfig {
            enabled_services: crate::config::ServiceToggles {
                codex: false,
                claude: false,
                grok: false,
                ollama: true,
            },
            providers: crate::config::ProviderSettings {
                local_enabled: true,
                web_enabled: true,
                cli_enabled: false,
            },
            ..AppConfig::default()
        };
        let providers = providers_for_config(&config, &LocalProviderRoots::default());

        assert!(providers.iter().any(|provider| {
            provider.provider_id() == UsageProviderId::OllamaLocal
                && provider.service() == Service::Ollama
                && provider.source() == UsageSource::Local
        }));
        assert!(providers.iter().any(|provider| {
            provider.provider_id() == UsageProviderId::OllamaWeb
                && provider.service() == Service::Ollama
                && provider.source() == UsageSource::Web
        }));
    }

    #[test]
    fn manual_web_refresh_fails_closed_when_backend_is_not_selected() {
        let engine = UsageEngine::new(web_enabled_config());

        let display_state = engine
            .refresh_provider_source(Service::Codex, UsageSource::Web)
            .expect("web provider fails closed");
        let snapshot = display_state
            .snapshots
            .iter()
            .find(|snapshot| snapshot.service == Service::Codex)
            .expect("codex snapshot exists");

        assert_eq!(snapshot.source, UsageSource::Web);
        assert_eq!(snapshot.remaining_percent, None);
        assert_eq!(snapshot.details["status"], "login_required");
        assert_eq!(snapshot.details["providerId"], "codex.web");
        assert_eq!(snapshot.details["mergeStatus"], "web_unavailable");
    }

    #[test]
    fn grok_placeholder_failure_does_not_block_the_following_headless_refresh() {
        let now = "2026-07-09T12:00:00Z";
        let engine = UsageEngine::with_clock(
            grok_web_config(),
            Box::new(FixedClock {
                now: now.to_string(),
            }),
        );
        let refresh_calls = AtomicUsize::new(0);

        engine.refresh_all().expect("placeholder refresh succeeds");

        assert_eq!(
            engine
                .provider_failure_state("grok.web")
                .expect("failure state reads"),
            None
        );
        engine
            .refresh_provider_source_with_snapshot(Service::Grok, UsageSource::Web, |_| {
                refresh_calls.fetch_add(1, Ordering::Relaxed);
                Ok(web_snapshot(Service::Grok, 71.5, now))
            })
            .expect("headless refresh is not blocked");

        assert_eq!(refresh_calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn ollama_placeholder_failure_does_not_block_the_following_headless_refresh() {
        let now = "2026-07-09T12:00:00Z";
        let mut config = ollama_web_config();
        config.providers.local_enabled = false;
        let engine = UsageEngine::with_clock(
            config,
            Box::new(FixedClock {
                now: now.to_string(),
            }),
        );
        let refresh_calls = AtomicUsize::new(0);

        engine.refresh_all().expect("placeholder refresh succeeds");

        assert_eq!(
            engine
                .provider_failure_state("ollama.web")
                .expect("failure state reads"),
            None
        );
        engine
            .refresh_provider_source_with_snapshot(Service::Ollama, UsageSource::Web, |_| {
                refresh_calls.fetch_add(1, Ordering::Relaxed);
                Ok(web_snapshot(Service::Ollama, 82.0, now))
            })
            .expect("headless refresh is not blocked");

        assert_eq!(refresh_calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn headless_web_failure_still_records_backoff_for_a_placeholder_provider_key() {
        let now = "2026-07-09T12:00:00Z";
        let engine = UsageEngine::with_clock(
            grok_web_config(),
            Box::new(FixedClock {
                now: now.to_string(),
            }),
        );

        engine.refresh_all().expect("placeholder refresh succeeds");
        engine
            .refresh_provider_source_with_snapshot(Service::Grok, UsageSource::Web, |_| {
                Err(UsageProviderError::NetworkUnavailable)
            })
            .expect("headless failure records a snapshot");

        assert!(engine
            .provider_failure_state("grok.web")
            .expect("failure state reads")
            .is_some());
        assert!(!engine
            .try_begin_refresh("grok.web".to_string(), UsageSource::Web, now)
            .expect("real web failure remains backoff gated"));
    }

    #[test]
    fn manual_web_refresh_accepts_external_headless_snapshot() {
        let engine = UsageEngine::with_clock(
            web_enabled_config(),
            Box::new(FixedClock {
                now: "2026-06-04T12:00:00Z".to_string(),
            }),
        );

        let display_state = engine
            .refresh_provider_source_with_snapshot(Service::Claude, UsageSource::Web, |now| {
                Ok(UsageSnapshot {
                    service: Service::Claude,
                    remaining_percent: None,
                    used_percent: None,
                    reset_at: None,
                    source: UsageSource::Web,
                    confidence: UsageConfidence::Unknown,
                    last_updated: now.to_string(),
                    details: serde_json::json!({
                        "status": "login_required",
                        "providerId": "claude.web",
                        "source": "web",
                        "reason": "logged_out",
                        "lastOfficialCheckAt": now
                    }),
                })
            })
            .expect("external web snapshot refresh succeeds");
        let snapshot = display_state
            .snapshots
            .iter()
            .find(|snapshot| snapshot.service == Service::Claude)
            .expect("claude snapshot exists");

        assert_eq!(snapshot.source, UsageSource::Web);
        assert_eq!(snapshot.details["status"], "login_required");
        assert_eq!(snapshot.details["reason"], "logged_out");
        assert_eq!(
            engine
                .ensure_manual_web_refresh_allowed(Service::Claude, "2026-06-04T12:00:01Z")
                .expect_err("external web refresh records cooldown"),
            "Manual web refresh is cooling down"
        );
    }

    #[test]
    fn preflight_web_refresh_accepts_external_headless_snapshot_without_manual_cooldown() {
        let engine = UsageEngine::with_clock(
            web_enabled_config(),
            Box::new(FixedClock {
                now: "2026-06-04T12:00:00Z".to_string(),
            }),
        );
        let refresh_calls = AtomicUsize::new(0);

        engine
            .record_manual_web_refresh(Service::Claude, "2026-06-04T11:59:30Z")
            .expect("manual cooldown is recorded");
        assert_eq!(
            engine
                .ensure_manual_web_refresh_allowed(Service::Claude, "2026-06-04T12:00:00Z")
                .expect_err("manual cooldown starts active"),
            "Manual web refresh is cooling down"
        );

        let display_state = engine
            .refresh_preflight_provider_source_with_snapshot(
                Service::Claude,
                UsageSource::Web,
                |now| {
                    refresh_calls.fetch_add(1, Ordering::Relaxed);
                    Ok(UsageSnapshot {
                        service: Service::Claude,
                        remaining_percent: Some(63.0),
                        used_percent: Some(37.0),
                        reset_at: Some("2026-06-04T17:00:00Z".to_string()),
                        source: UsageSource::Web,
                        confidence: UsageConfidence::High,
                        last_updated: now.to_string(),
                        details: serde_json::json!({
                            "status": "parsed",
                            "providerId": "claude.web",
                            "source": "web",
                            "lastOfficialCheckAt": now
                        }),
                    })
                },
            )
            .expect("preflight web snapshot refresh succeeds");
        let snapshot = display_state
            .snapshots
            .iter()
            .find(|snapshot| snapshot.service == Service::Claude)
            .expect("claude snapshot exists");

        assert_eq!(refresh_calls.load(Ordering::Relaxed), 1);
        assert_eq!(snapshot.source, UsageSource::Web);
        assert_eq!(snapshot.remaining_percent, Some(63.0));
        assert_eq!(snapshot.details["status"], "parsed");
        assert_eq!(
            engine
                .ensure_manual_web_refresh_allowed(Service::Claude, "2026-06-04T12:00:29Z")
                .expect_err("preflight refresh does not replace manual cooldown"),
            "Manual web refresh is cooling down"
        );
        engine
            .ensure_manual_web_refresh_allowed(Service::Claude, "2026-06-04T12:00:30Z")
            .expect("preflight refresh does not extend manual cooldown");
    }

    #[test]
    fn scheduled_web_refresh_accepts_external_headless_snapshot_without_manual_cooldown() {
        let engine = UsageEngine::with_clock(
            web_enabled_config(),
            Box::new(FixedClock {
                now: "2026-06-04T12:00:00Z".to_string(),
            }),
        );
        let refresh_calls = AtomicUsize::new(0);

        let display_state = engine
            .refresh_due_provider_source_with_snapshot(Service::Codex, UsageSource::Web, |now| {
                refresh_calls.fetch_add(1, Ordering::Relaxed);
                Ok(UsageSnapshot {
                    service: Service::Codex,
                    remaining_percent: None,
                    used_percent: None,
                    reset_at: None,
                    source: UsageSource::Web,
                    confidence: UsageConfidence::Unknown,
                    last_updated: now.to_string(),
                    details: serde_json::json!({
                        "status": "login_required",
                        "providerId": "codex.web",
                        "source": "web",
                        "reason": "logged_out",
                        "lastOfficialCheckAt": now
                    }),
                })
            })
            .expect("scheduled external web snapshot refresh succeeds");
        let snapshot = display_state
            .snapshots
            .iter()
            .find(|snapshot| snapshot.service == Service::Codex)
            .expect("codex snapshot exists");

        assert_eq!(refresh_calls.load(Ordering::Relaxed), 1);
        assert_eq!(snapshot.source, UsageSource::Web);
        assert_eq!(snapshot.details["status"], "login_required");
        engine
            .ensure_manual_web_refresh_allowed(Service::Codex, "2026-06-04T12:00:01Z")
            .expect("scheduled refresh does not record manual cooldown");

        engine
            .refresh_due_provider_source_with_snapshot(Service::Codex, UsageSource::Web, |_| {
                refresh_calls.fetch_add(1, Ordering::Relaxed);
                Err(UsageProviderError::Internal)
            })
            .expect("non-due scheduled refresh is skipped");

        assert_eq!(refresh_calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn manual_web_refresh_cooldown_uses_configured_boundary() {
        let engine = UsageEngine::new(web_enabled_config());

        engine
            .record_manual_web_refresh(Service::Codex, "2026-06-03T23:00:00Z")
            .expect("manual web refresh records");

        let early = engine
            .ensure_manual_web_refresh_allowed(Service::Codex, "2026-06-03T23:00:59Z")
            .expect_err("refresh is cooling down");
        assert_eq!(early, "Manual web refresh is cooling down");

        engine
            .ensure_manual_web_refresh_allowed(Service::Codex, "2026-06-03T23:01:00Z")
            .expect("cooldown boundary allows refresh");
    }

    #[test]
    fn disabling_web_providers_clears_manual_web_refresh_cooldown() {
        let engine = UsageEngine::new(web_enabled_config());

        engine
            .record_manual_web_refresh(Service::Codex, "2026-06-03T23:00:00Z")
            .expect("manual web refresh records");
        engine
            .update_config(config_with_services(true, true))
            .expect("web providers disable");
        engine
            .update_config(web_enabled_config())
            .expect("web providers re-enable");

        engine
            .ensure_manual_web_refresh_allowed(Service::Codex, "2026-06-03T23:00:01Z")
            .expect("cooldown state was cleared when web providers disabled");
    }

    #[test]
    fn provider_refresh_interval_uses_source_specific_config() {
        let config = AppConfig {
            intervals: crate::config::RefreshIntervals {
                local_seconds: 30,
                web_minutes: 15,
                manual_web_refresh_cooldown_seconds: 60,
                gauge_switch_seconds: 6,
            },
            ..AppConfig::default()
        }
        .normalized();

        assert_eq!(
            provider_refresh_interval(&config, UsageSource::Local),
            Duration::from_secs(30)
        );
        assert_eq!(
            provider_refresh_interval(&config, UsageSource::Fake),
            Duration::from_secs(30)
        );
        assert_eq!(
            provider_refresh_interval(&config, UsageSource::Web),
            Duration::from_secs(15 * 60)
        );
    }

    #[test]
    fn scheduled_refresh_respects_local_interval_boundary() {
        let config = AppConfig {
            intervals: crate::config::RefreshIntervals {
                local_seconds: 30,
                web_minutes: 15,
                manual_web_refresh_cooldown_seconds: 60,
                gauge_switch_seconds: 6,
            },
            ..config_with_services(true, false)
        };
        let engine = UsageEngine::with_clock(
            config,
            Box::new(SequenceClock::new(&[
                "2026-06-03T22:59:00Z",
                "2026-06-03T23:00:00Z",
                "2026-06-03T23:00:29Z",
                "2026-06-03T23:00:30Z",
            ])),
        );

        let first = engine.refresh_due().expect("first scheduled refresh runs");
        let skipped = engine
            .refresh_due()
            .expect("second scheduled refresh is skipped");
        let refreshed = engine.refresh_due().expect("third scheduled refresh runs");

        assert_eq!(first.updated_at, "2026-06-03T23:00:00Z");
        assert_eq!(skipped.updated_at, "2026-06-03T23:00:00Z");
        assert_eq!(refreshed.updated_at, "2026-06-03T23:00:30Z");
        assert_eq!(refreshed.snapshots.len(), 1);
    }

    #[test]
    fn manual_refresh_bypasses_scheduled_interval() {
        let config = AppConfig {
            intervals: crate::config::RefreshIntervals {
                local_seconds: 30,
                web_minutes: 15,
                manual_web_refresh_cooldown_seconds: 60,
                gauge_switch_seconds: 6,
            },
            ..config_with_services(true, false)
        };
        let engine = UsageEngine::with_clock(
            config,
            Box::new(SequenceClock::new(&[
                "2026-06-03T22:59:00Z",
                "2026-06-03T23:00:00Z",
                "2026-06-03T23:00:01Z",
            ])),
        );

        engine
            .refresh_due()
            .expect("scheduled refresh seeds last refresh");
        let refreshed = engine.refresh_all().expect("manual refresh runs");

        assert_eq!(refreshed.updated_at, "2026-06-03T23:00:01Z");
        assert_eq!(refreshed.snapshots.len(), 1);
    }

    #[test]
    fn display_state_marks_cached_snapshots_stale_after_source_interval() {
        let config = AppConfig {
            intervals: crate::config::RefreshIntervals {
                local_seconds: 30,
                web_minutes: 15,
                manual_web_refresh_cooldown_seconds: 60,
                gauge_switch_seconds: 6,
            },
            ..config_with_services(true, false)
        };
        let engine = UsageEngine::with_clock(
            config,
            Box::new(SequenceClock::new(&[
                "2026-06-03T22:59:00Z",
                "2026-06-03T23:00:00Z",
                "2026-06-03T23:00:31Z",
            ])),
        );

        engine.refresh_all().expect("manual refresh succeeds");
        let display_state = engine.display_state().expect("display state loads");
        let details = &display_state.snapshots[0].details;

        assert_eq!(details["stale"], true);
        assert_eq!(details["staleSeconds"], 31);
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
    fn display_state_uses_web_only_snapshot_when_no_local_delta_exists() {
        let display_state = display_state_from_provider_snapshots(
            web_enabled_config(),
            vec![(
                "codex.web",
                web_snapshot(Service::Codex, 82.0, "2026-06-03T23:00:00Z"),
            )],
            "2026-06-03T23:05:00Z",
        );
        let snapshot = &display_state.snapshots[0];

        assert_eq!(snapshot.source, UsageSource::Web);
        assert_eq!(snapshot.remaining_percent, Some(82.0));
        assert_eq!(snapshot.confidence, UsageConfidence::High);
        assert_eq!(snapshot.details["mergeStatus"], "web_only");
        assert_eq!(
            snapshot.details["lastOfficialCheckAt"],
            "2026-06-03T23:00:00Z"
        );
    }

    #[test]
    fn plan_only_ollama_snapshot_does_not_downgrade_web_usage() {
        let display_state = display_state_from_provider_snapshots(
            ollama_web_config(),
            vec![
                (
                    "ollama.web",
                    web_snapshot(Service::Ollama, 82.0, "2026-07-09T12:00:00Z"),
                ),
                (
                    "ollama.local",
                    ollama_plan_snapshot("2026-07-09T12:00:01Z"),
                ),
            ],
            "2026-07-09T12:00:01Z",
        );
        let snapshot = &display_state.snapshots[0];

        assert_eq!(snapshot.source, UsageSource::Web);
        assert_eq!(snapshot.remaining_percent, Some(82.0));
        assert_eq!(snapshot.confidence, UsageConfidence::High);
        assert_eq!(snapshot.details["mergeStatus"], "web_only");
        assert_eq!(snapshot.details["plan"], "pro");
    }

    #[test]
    fn grok_web_usage_beats_cli_plan_and_carries_its_plan_details() {
        let display_state = display_state_from_provider_snapshots(
            grok_web_config(),
            vec![
                (
                    "grok.web",
                    web_snapshot(Service::Grok, 71.5, "2026-07-09T12:00:00Z"),
                ),
                ("grok.cli", grok_cli_plan_snapshot("2026-07-09T12:10:00Z")),
            ],
            "2026-07-09T12:10:00Z",
        );
        let snapshot = &display_state.snapshots[0];

        assert_eq!(snapshot.remaining_percent, Some(71.5));
        assert_eq!(snapshot.details["providerId"], "grok.web");
        assert_eq!(snapshot.details["plan"], "Grok Pro");
        assert_eq!(snapshot.details["billingPeriodEnd"], "2026-07-16T00:00:00Z");
    }

    #[test]
    fn grok_cli_plan_beats_a_grok_web_login_failure() {
        let display_state = display_state_from_provider_snapshots(
            grok_web_config(),
            vec![
                (
                    "grok.web",
                    web_error_snapshot(
                        Service::Grok,
                        UsageProviderError::LoginRequired,
                        "2026-07-09T12:10:00Z",
                    ),
                ),
                ("grok.cli", grok_cli_plan_snapshot("2026-07-09T12:00:00Z")),
            ],
            "2026-07-09T12:10:00Z",
        );
        let snapshot = &display_state.snapshots[0];

        assert_eq!(snapshot.details["providerId"], "grok.cli");
        assert_eq!(snapshot.details["plan"], "Grok Pro");
        assert_eq!(snapshot.remaining_percent, None);
    }

    #[test]
    fn single_web_snapshots_keep_existing_codex_and_claude_behavior() {
        let display_state = display_state_from_provider_snapshots(
            web_enabled_config(),
            vec![
                (
                    "codex.web",
                    web_snapshot(Service::Codex, 82.0, "2026-07-09T12:00:00Z"),
                ),
                (
                    "claude.web",
                    web_snapshot(Service::Claude, 63.0, "2026-07-09T12:00:00Z"),
                ),
            ],
            "2026-07-09T12:01:00Z",
        );

        assert_eq!(display_state.snapshots.len(), 2);
        assert_eq!(display_state.snapshots[0].details["providerId"], "codex.web");
        assert_eq!(display_state.snapshots[1].details["providerId"], "claude.web");
        assert!(display_state
            .snapshots
            .iter()
            .all(|snapshot| snapshot.details["mergeStatus"] == "web_only"));
    }

    #[test]
    fn plan_only_ollama_snapshot_stays_clean_when_web_login_fails() {
        let display_state = display_state_from_provider_snapshots(
            ollama_web_config(),
            vec![
                (
                    "ollama.web",
                    web_error_snapshot(
                        Service::Ollama,
                        UsageProviderError::LoginRequired,
                        "2026-07-09T12:00:00Z",
                    ),
                ),
                (
                    "ollama.local",
                    ollama_plan_snapshot("2026-07-09T12:00:01Z"),
                ),
            ],
            "2026-07-09T12:00:01Z",
        );
        let snapshot = &display_state.snapshots[0];

        assert_eq!(snapshot.source, UsageSource::Local);
        assert_eq!(snapshot.confidence, UsageConfidence::Medium);
        assert_eq!(snapshot.details["plan"], "pro");
        assert!(snapshot.details.get("webStatus").is_none());
        assert!(snapshot.details.get("mergeStatus").is_none());
    }

    #[test]
    fn display_state_uses_local_only_snapshot_without_web_baseline() {
        let display_state = display_state_from_provider_snapshots(
            local_codex_config(),
            vec![(
                "codex.local",
                uncalibrated_local_snapshot(Service::Codex, "2026-06-03T23:00:00Z"),
            )],
            "2026-06-03T23:05:00Z",
        );
        let snapshot = &display_state.snapshots[0];

        assert_eq!(snapshot.source, UsageSource::Local);
        assert_eq!(snapshot.remaining_percent, None);
        assert_eq!(snapshot.details["providerId"], "codex.local");
    }

    #[test]
    fn display_state_merges_web_baseline_with_matching_local_delta() {
        let baseline_at = "2026-06-03T23:00:00Z";
        let display_state = display_state_from_provider_snapshots(
            web_enabled_config(),
            vec![
                ("codex.web", web_snapshot(Service::Codex, 80.0, baseline_at)),
                (
                    "codex.local",
                    local_delta_snapshot(Service::Codex, 15.0, baseline_at, "2026-06-03T23:05:00Z"),
                ),
            ],
            "2026-06-03T23:05:00Z",
        );
        let snapshot = &display_state.snapshots[0];

        assert_eq!(snapshot.source, UsageSource::Merged);
        assert_eq!(snapshot.remaining_percent, Some(65.0));
        assert_eq!(snapshot.used_percent, Some(35.0));
        assert_eq!(snapshot.confidence, UsageConfidence::Medium);
        assert_eq!(snapshot.details["mergeStatus"], "web_plus_local_delta");
        assert_eq!(snapshot.details["baselineAt"], baseline_at);
        assert_eq!(snapshot.details["localDeltaPercent"], 15.0);
    }

    #[test]
    fn display_state_does_not_double_count_delta_after_web_baseline_refresh() {
        let display_state = display_state_from_provider_snapshots(
            web_enabled_config(),
            vec![
                (
                    "codex.web",
                    web_snapshot(Service::Codex, 75.0, "2026-06-03T23:10:00Z"),
                ),
                (
                    "codex.local",
                    local_delta_snapshot(
                        Service::Codex,
                        15.0,
                        "2026-06-03T23:00:00Z",
                        "2026-06-03T23:11:00Z",
                    ),
                ),
            ],
            "2026-06-03T23:11:00Z",
        );
        let snapshot = &display_state.snapshots[0];

        assert_eq!(snapshot.source, UsageSource::Web);
        assert_eq!(snapshot.remaining_percent, Some(75.0));
        assert_eq!(snapshot.confidence, UsageConfidence::Medium);
        assert_eq!(snapshot.details["mergeStatus"], "local_delta_unavailable");
    }

    #[test]
    fn display_state_marks_stale_web_baseline_without_applying_local_delta() {
        let baseline_at = "2026-06-03T23:00:00Z";
        let display_state = display_state_from_provider_snapshots(
            web_enabled_config(),
            vec![
                ("codex.web", web_snapshot(Service::Codex, 80.0, baseline_at)),
                (
                    "codex.local",
                    local_delta_snapshot(Service::Codex, 15.0, baseline_at, "2026-06-03T23:20:00Z"),
                ),
            ],
            "2026-06-03T23:31:00Z",
        );
        let snapshot = &display_state.snapshots[0];

        assert_eq!(snapshot.source, UsageSource::Web);
        assert_eq!(snapshot.remaining_percent, Some(80.0));
        assert_eq!(snapshot.confidence, UsageConfidence::Medium);
        assert_eq!(snapshot.details["mergeStatus"], "stale_web_baseline");
        assert_eq!(snapshot.details["stale"], true);
    }

    #[test]
    fn display_state_clamps_merged_percentages() {
        let baseline_at = "2026-06-03T23:00:00Z";
        let display_state = display_state_from_provider_snapshots(
            web_enabled_config(),
            vec![
                ("codex.web", web_snapshot(Service::Codex, 20.0, baseline_at)),
                (
                    "codex.local",
                    local_delta_snapshot(Service::Codex, 50.0, baseline_at, "2026-06-03T23:05:00Z"),
                ),
            ],
            "2026-06-03T23:05:00Z",
        );
        let snapshot = &display_state.snapshots[0];

        assert_eq!(snapshot.source, UsageSource::Merged);
        assert_eq!(snapshot.remaining_percent, Some(0.0));
        assert_eq!(snapshot.used_percent, Some(100.0));
    }

    #[test]
    fn display_state_keeps_web_baseline_when_local_delta_is_unavailable() {
        let display_state = display_state_from_provider_snapshots(
            web_enabled_config(),
            vec![
                (
                    "codex.web",
                    web_snapshot(Service::Codex, 80.0, "2026-06-03T23:00:00Z"),
                ),
                (
                    "codex.local",
                    uncalibrated_local_snapshot(Service::Codex, "2026-06-03T23:05:00Z"),
                ),
            ],
            "2026-06-03T23:05:00Z",
        );
        let snapshot = &display_state.snapshots[0];

        assert_eq!(snapshot.source, UsageSource::Web);
        assert_eq!(snapshot.remaining_percent, Some(80.0));
        assert_eq!(snapshot.confidence, UsageConfidence::Medium);
        assert_eq!(snapshot.details["mergeStatus"], "local_delta_unavailable");
    }

    #[test]
    fn display_state_keeps_local_snapshot_when_web_provider_fails_closed() {
        let mut web_snapshot = web_error_snapshot(
            Service::Codex,
            UsageProviderError::LoginRequired,
            "2026-06-03T23:10:00Z",
        );
        web_snapshot.details["reason"] = serde_json::json!("logged_out");
        let display_state = display_state_from_provider_snapshots(
            web_enabled_config(),
            vec![
                ("codex.web", web_snapshot),
                (
                    "codex.local",
                    uncalibrated_local_snapshot(Service::Codex, "2026-06-03T23:05:00Z"),
                ),
            ],
            "2026-06-03T23:10:00Z",
        );
        let snapshot = &display_state.snapshots[0];

        assert_eq!(snapshot.source, UsageSource::Local);
        assert_eq!(snapshot.remaining_percent, None);
        assert_eq!(snapshot.details["status"], "parsed");
        assert_eq!(snapshot.details["webStatus"], "login_required");
        assert_eq!(snapshot.details["webReason"], "logged_out");
        assert_eq!(snapshot.details["webProviderId"], "codex.web");
        assert_eq!(snapshot.details["mergeStatus"], "web_unavailable_fallback");
    }

    #[test]
    fn display_state_clears_web_status_when_later_web_snapshot_succeeds() {
        let local_snapshot = uncalibrated_local_snapshot(Service::Codex, "2026-06-03T23:05:00Z");
        let mut web_failure = web_error_snapshot(
            Service::Codex,
            UsageProviderError::LoginRequired,
            "2026-06-03T23:10:00Z",
        );
        web_failure.details["reason"] = serde_json::json!("logged_out");

        let failed_display_state = display_state_from_provider_snapshots(
            web_enabled_config(),
            vec![
                ("codex.web", web_failure),
                ("codex.local", local_snapshot.clone()),
            ],
            "2026-06-03T23:10:00Z",
        );
        let failed_snapshot = &failed_display_state.snapshots[0];

        assert_eq!(failed_snapshot.source, UsageSource::Local);
        assert_eq!(failed_snapshot.details["webStatus"], "login_required");
        assert_eq!(
            failed_snapshot.details["mergeStatus"],
            "web_unavailable_fallback"
        );

        let successful_display_state = display_state_from_provider_snapshots(
            web_enabled_config(),
            vec![
                (
                    "codex.web",
                    web_snapshot(Service::Codex, 64.0, "2026-06-03T23:11:00Z"),
                ),
                ("codex.local", local_snapshot),
            ],
            "2026-06-03T23:11:00Z",
        );
        let successful_snapshot = &successful_display_state.snapshots[0];

        assert_eq!(successful_snapshot.source, UsageSource::Web);
        assert_eq!(successful_snapshot.remaining_percent, Some(64.0));
        assert_eq!(
            successful_snapshot.details.get("webStatus"),
            None,
            "Successful web preflight must clear stale login-required fallback status",
        );
        assert_eq!(
            successful_snapshot.details.get("webReason"),
            None,
            "Successful web preflight must clear stale fallback reason",
        );
        assert_eq!(
            successful_snapshot.details["mergeStatus"],
            "local_delta_unavailable"
        );
    }

    #[test]
    fn display_state_keeps_local_snapshot_for_web_interruption_failures() {
        for (error, status) in [
            (UsageProviderError::MfaRequired, "mfa_required"),
            (
                UsageProviderError::CaptchaOrBotCheck,
                "captcha_or_bot_check",
            ),
            (UsageProviderError::UnexpectedUi, "unexpected_ui"),
            (UsageProviderError::ParseFailed, "parse_failed"),
            (
                UsageProviderError::NetworkUnavailable,
                "network_unavailable",
            ),
            (UsageProviderError::TimedOut, "timed_out"),
        ] {
            let display_state = display_state_from_provider_snapshots(
                web_enabled_config(),
                vec![
                    (
                        "codex.web",
                        web_error_snapshot(Service::Codex, error, "2026-06-03T23:10:00Z"),
                    ),
                    (
                        "codex.local",
                        uncalibrated_local_snapshot(Service::Codex, "2026-06-03T23:05:00Z"),
                    ),
                ],
                "2026-06-03T23:10:00Z",
            );
            let snapshot = &display_state.snapshots[0];

            assert_eq!(snapshot.source, UsageSource::Local);
            assert_eq!(snapshot.details["status"], "parsed");
            assert_eq!(snapshot.details["webStatus"], status);
            assert_eq!(snapshot.details["webProviderId"], "codex.web");
            assert_eq!(snapshot.details["mergeStatus"], "web_unavailable_fallback");
        }
    }

    #[test]
    fn display_state_drops_unsanitized_web_failure_reason() {
        let mut web_snapshot = web_error_snapshot(
            Service::Codex,
            UsageProviderError::UnexpectedUi,
            "2026-06-03T23:10:00Z",
        );
        web_snapshot.details["reason"] = serde_json::json!("/home/dev/page <html>");
        let display_state = display_state_from_provider_snapshots(
            web_enabled_config(),
            vec![
                ("codex.web", web_snapshot),
                (
                    "codex.local",
                    uncalibrated_local_snapshot(Service::Codex, "2026-06-03T23:05:00Z"),
                ),
            ],
            "2026-06-03T23:10:00Z",
        );
        let snapshot = &display_state.snapshots[0];

        assert_eq!(snapshot.details["webStatus"], "unexpected_ui");
        assert!(snapshot.details.get("webReason").is_none());
    }

    #[test]
    fn display_state_preserves_unknown_local_snapshot_without_web_baseline() {
        let mut snapshot = uncalibrated_local_snapshot(Service::Codex, "2026-06-03T23:00:00Z");
        snapshot.confidence = UsageConfidence::Unknown;
        snapshot.details["status"] = serde_json::json!("missing_data");
        let display_state = display_state_from_provider_snapshots(
            local_codex_config(),
            vec![("codex.local", snapshot)],
            "2026-06-03T23:05:00Z",
        );
        let snapshot = &display_state.snapshots[0];

        assert_eq!(snapshot.source, UsageSource::Local);
        assert_eq!(snapshot.confidence, UsageConfidence::Unknown);
        assert_eq!(snapshot.details["status"], "missing_data");
    }

    #[test]
    fn refresh_event_serializes_to_expected_payload_shape() {
        let value = serde_json::to_value(UsageRefreshEvent::new(
            Some(Service::Codex),
            Some(UsageSource::Local),
            UsageRefreshStatus::Started,
            "2026-06-03T23:15:00Z".to_string(),
        ))
        .expect("serializes");

        assert_eq!(value["service"], "codex");
        assert_eq!(value["source"], "local");
        assert_eq!(value["status"], "started");
        assert_eq!(value["emittedAt"], "2026-06-03T23:15:00Z");
    }

    #[test]
    fn provider_error_event_serializes_to_sanitized_payload_shape() {
        let value = serde_json::to_value(UsageProviderErrorEvent::new(
            Service::Claude,
            UsageSource::Local,
            "claude.local",
            "missing_data",
            "2026-06-03T23:20:00Z".to_string(),
        ))
        .expect("serializes");

        assert_eq!(value["service"], "claude");
        assert_eq!(value["source"], "local");
        assert_eq!(value["providerId"], "claude.local");
        assert_eq!(value["status"], "missing_data");
        assert_eq!(value["emittedAt"], "2026-06-03T23:20:00Z");
        assert!(value.get("raw").is_none());
        assert!(value.get("path").is_none());
    }

    #[test]
    fn display_and_error_snapshot_details_are_sanitized() {
        let dir = TestDir::new();
        create_codex_state_db(&dir.path);
        let local_state = UsageEngine::with_clock_and_local_roots(
            local_codex_config(),
            Box::new(FixedClock {
                now: "2026-06-03T23:00:00Z".to_string(),
            }),
            LocalProviderRoots {
                codex: Some(dir.path.clone()),
                claude: None,
            },
        )
        .refresh_all()
        .expect("local refresh succeeds");
        let merge_state = display_state_from_provider_snapshots(
            web_enabled_config(),
            vec![
                (
                    "codex.web",
                    web_snapshot(Service::Codex, 80.0, "2026-06-03T23:00:00Z"),
                ),
                (
                    "codex.local",
                    local_delta_snapshot(
                        Service::Codex,
                        10.0,
                        "2026-06-03T23:00:00Z",
                        "2026-06-03T23:05:00Z",
                    ),
                ),
            ],
            "2026-06-03T23:05:00Z",
        );
        let provider = ProviderDescriptor {
            provider_key: "codex.web".to_string(),
            provider_id: UsageProviderId::CodexWeb,
            service: Service::Codex,
            source: UsageSource::Web,
            is_placeholder: false,
            local_data_root: None,
            local_calibration: None,
        };

        for snapshot in local_state
            .snapshots
            .iter()
            .chain(merge_state.snapshots.iter())
        {
            assert_sanitized_details("display snapshot", &snapshot.details);
        }

        for error in [
            UsageProviderError::NotConfigured,
            UsageProviderError::Disabled,
            UsageProviderError::MissingData,
            UsageProviderError::PermissionDenied,
            UsageProviderError::ParseFailed,
            UsageProviderError::LoginRequired,
            UsageProviderError::MfaRequired,
            UsageProviderError::CaptchaOrBotCheck,
            UsageProviderError::NetworkUnavailable,
            UsageProviderError::TimedOut,
            UsageProviderError::UnexpectedUi,
            UsageProviderError::UnsafePath,
            UsageProviderError::Internal,
        ] {
            let snapshot = error_snapshot(&provider, error, "2026-06-03T23:00:00Z");
            assert_sanitized_details("provider error snapshot", &snapshot.details);
        }
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
            is_placeholder: false,
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
        assert_eq!(UsageProviderId::GrokCli.code(), "grok.cli");
        assert_eq!(
            UsageProviderId::for_service_source(Service::Grok, UsageSource::Web)
                .expect("grok web id")
                .code(),
            "grok.web"
        );
        assert_eq!(
            UsageProviderId::for_service_source(Service::Ollama, UsageSource::Local)
                .expect("ollama local id")
                .code(),
            "ollama.local"
        );
        assert_eq!(
            UsageProviderId::for_service_source(Service::Codex, UsageSource::Fake)
                .expect("fake id")
                .code(),
            "fake"
        );
    }

    #[test]
    fn grok_cli_provider_is_registered_when_enabled() {
        let config = AppConfig {
            enabled_services: crate::config::ServiceToggles {
                codex: false,
                claude: false,
                grok: true,
                ollama: false,
            },
            providers: crate::config::ProviderSettings {
                local_enabled: false,
                web_enabled: false,
                cli_enabled: true,
            },
            ..AppConfig::default()
        };

        let providers = providers_for_config(&config, &LocalProviderRoots::default());

        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].provider_id(), UsageProviderId::GrokCli);
        assert_eq!(providers[0].service(), Service::Grok);
        assert_eq!(providers[0].source(), UsageSource::Web);
    }

    #[test]
    fn grok_cli_and_web_providers_register_together_when_enabled() {
        let providers = providers_for_config(&grok_web_config(), &LocalProviderRoots::default());

        assert!(providers.iter().any(|provider| {
            provider.provider_id() == UsageProviderId::GrokCli
                && provider.service() == Service::Grok
                && provider.source() == UsageSource::Web
        }));
        assert!(providers.iter().any(|provider| {
            provider.provider_id() == UsageProviderId::GrokWeb
                && provider.service() == Service::Grok
                && provider.source() == UsageSource::Web
        }));
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
