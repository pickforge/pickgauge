//! Official usage readings via installed CLIs' own OAuth credentials.
//!
//! Instead of scraping dashboards in a headless browser, this reads the tokens
//! those CLIs already stored on disk and calls the same usage endpoints they
//! call.
//!
//! Endpoints/clients discovered from the shipped CLI binaries:
//! - Codex refresh: POST https://auth.openai.com/oauth/token (client app_EMoamEEZ73f0CkXaXp7hrann)
//!   usage: GET https://chatgpt.com/backend-api/codex/usage
//! - Claude refresh: POST https://platform.claude.com/v1/oauth/token (client 9d1c250a-…)
//!   usage: GET https://api.anthropic.com/api/oauth/usage
//! - Grok usage: GET https://grok.com/rest/subscriptions
//!   (bearer from ~/.grok/auth.json; never refreshed or written back)
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, SystemTime},
};

use serde_json::{json, Value};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::usage::{
    Service, UsageConfidence, UsageProviderError, UsageProviderId, UsageSnapshot, UsageSource,
};

const CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const CODEX_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const CODEX_USAGE_URL: &str = "https://chatgpt.com/backend-api/codex/usage";

const CLAUDE_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const CLAUDE_TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
const CLAUDE_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const CLAUDE_OAUTH_BETA: &str = "oauth-2025-04-20";

const GROK_SUBSCRIPTIONS_URL: &str = "https://grok.com/rest/subscriptions";

const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
// Refresh a token this many ms before its stated expiry.
const EXPIRY_SKEW_MS: i64 = 60_000;

#[derive(Clone, Debug, Eq, PartialEq)]
struct CredentialFileStamp {
    modified: Option<SystemTime>,
    len: u64,
}

#[derive(Clone)]
struct CachedOAuthState {
    access_token: String,
    refresh_token: String,
    expires_at_ms: Option<i64>,
    account_id: String,
    source_stamp: CredentialFileStamp,
}

static HTTP_CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
static OAUTH_CACHE: OnceLock<Mutex<HashMap<Service, CachedOAuthState>>> = OnceLock::new();

fn home() -> Result<PathBuf, UsageProviderError> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or(UsageProviderError::Internal)
}

fn client() -> Result<&'static reqwest::blocking::Client, UsageProviderError> {
    if let Some(client) = HTTP_CLIENT.get() {
        return Ok(client);
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|_| UsageProviderError::Internal)?;
    let _ = HTTP_CLIENT.set(client);
    HTTP_CLIENT.get().ok_or(UsageProviderError::Internal)
}

fn oauth_cache() -> &'static Mutex<HashMap<Service, CachedOAuthState>> {
    OAUTH_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn credential_file_stamp(path: &Path) -> Result<CredentialFileStamp, UsageProviderError> {
    let metadata = std::fs::metadata(path).map_err(|_| UsageProviderError::NotConfigured)?;
    Ok(CredentialFileStamp {
        modified: metadata.modified().ok(),
        len: metadata.len(),
    })
}

fn cached_or_load_oauth(
    service: Service,
    path: &Path,
    load: impl FnOnce(&Path, CredentialFileStamp) -> Result<CachedOAuthState, UsageProviderError>,
) -> Result<CachedOAuthState, UsageProviderError> {
    let source_stamp = match credential_file_stamp(path) {
        Ok(stamp) => stamp,
        Err(error) => {
            discard_oauth(service)?;
            return Err(error);
        }
    };
    {
        let mut cache = oauth_cache()
            .lock()
            .map_err(|_| UsageProviderError::Internal)?;
        if let Some(cached) = cache.get(&service) {
            if cached.source_stamp == source_stamp {
                return Ok(cached.clone());
            }
        }
        cache.remove(&service);
    }

    let loaded = load(path, source_stamp)?;
    oauth_cache()
        .lock()
        .map_err(|_| UsageProviderError::Internal)?
        .insert(service, loaded.clone());
    Ok(loaded)
}

fn retain_oauth(service: Service, oauth: CachedOAuthState) -> Result<(), UsageProviderError> {
    oauth_cache()
        .lock()
        .map_err(|_| UsageProviderError::Internal)?
        .insert(service, oauth);
    Ok(())
}

fn discard_oauth(service: Service) -> Result<(), UsageProviderError> {
    oauth_cache()
        .lock()
        .map_err(|_| UsageProviderError::Internal)?
        .remove(&service);
    Ok(())
}

fn refreshed_oauth_state(
    previous: &CachedOAuthState,
    response: &Value,
    now_ms: i64,
) -> Result<CachedOAuthState, UsageProviderError> {
    let access_token = response
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .ok_or(UsageProviderError::ParseFailed)?;
    let refresh_token = response
        .get("refresh_token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .unwrap_or(&previous.refresh_token)
        .to_string();
    let expires_at_ms = response
        .get("expires_in")
        .and_then(Value::as_i64)
        .filter(|seconds| *seconds > 0)
        .and_then(|seconds| seconds.checked_mul(1_000))
        .and_then(|duration| now_ms.checked_add(duration));

    Ok(CachedOAuthState {
        access_token,
        refresh_token,
        expires_at_ms,
        account_id: previous.account_id.clone(),
        source_stamp: previous.source_stamp.clone(),
    })
}

fn now_unix_ms() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp() * 1_000
}

fn map_status_error(status: reqwest::StatusCode) -> UsageProviderError {
    match status.as_u16() {
        401 | 403 => UsageProviderError::LoginRequired,
        429 => UsageProviderError::TimedOut,
        _ => UsageProviderError::ParseFailed,
    }
}

fn unix_to_rfc3339(secs: i64) -> Option<String> {
    OffsetDateTime::from_unix_timestamp(secs)
        .ok()
        .and_then(|dt| dt.format(&Rfc3339).ok())
}

/// Entry point used by the engine for CLI-backed providers.
pub fn refresh(service: Service, now: &str) -> Result<UsageSnapshot, UsageProviderError> {
    match service {
        Service::Codex => refresh_codex(now),
        Service::Claude => refresh_claude(now),
        Service::Grok => refresh_grok(now),
        Service::Ollama => Err(UsageProviderError::Internal),
    }
}

/// One rate-limit window: percent used and when it resets.
struct Window {
    used: f32,
    reset: Option<String>,
}

fn window_json(window: &Option<Window>) -> Value {
    match window {
        Some(w) => {
            let used = w.used.clamp(0.0, 100.0);
            json!({
                "usedPercent": used,
                "remainingPercent": (100.0 - used).clamp(0.0, 100.0),
                "resetAt": w.reset,
            })
        }
        None => Value::Null,
    }
}

/// Build a snapshot carrying both windows. The headline number (drives the
/// float capsule and OS tray) uses the primary window only when the payload
/// identifies its duration as the expected service window.
fn build_snapshot(
    service: Service,
    five_hour: Option<Window>,
    week: Option<Window>,
    extra: Value,
    now: &str,
) -> Result<UsageSnapshot, UsageProviderError> {
    let headline = five_hour
        .as_ref()
        .or(week.as_ref())
        .ok_or(UsageProviderError::MissingData)?;
    let used = headline.used.clamp(0.0, 100.0);
    let reset_at = headline.reset.clone();

    let mut details = base_details(service);
    if let Some(obj) = details.as_object_mut() {
        obj.insert(
            "windows".into(),
            json!({
                "fiveHour": window_json(&five_hour),
                "week": window_json(&week),
            }),
        );
        if let Some(extra_obj) = extra.as_object() {
            for (key, value) in extra_obj {
                obj.insert(key.clone(), value.clone());
            }
        }
    }

    Ok(UsageSnapshot {
        service,
        remaining_percent: Some((100.0 - used).clamp(0.0, 100.0)),
        used_percent: Some(used),
        reset_at,
        source: UsageSource::Web,
        confidence: UsageConfidence::High,
        last_updated: now.to_string(),
        details,
    })
}

fn base_details(service: Service) -> Value {
    let provider_id = match service {
        Service::Codex => UsageProviderId::CodexCli,
        Service::Claude => UsageProviderId::ClaudeCli,
        Service::Grok => UsageProviderId::GrokCli,
        Service::Ollama => unreachable!("Ollama has no CLI provider"),
    };
    json!({
        "status": "parsed",
        "providerId": provider_id.code(),
        "source": UsageSource::Web.code(),
        "via": "cli",
    })
}

// ---------------------------------------------------------------- Codex

fn refresh_codex(now: &str) -> Result<UsageSnapshot, UsageProviderError> {
    let path = home()?.join(".codex/auth.json");
    let mut oauth = cached_or_load_oauth(Service::Codex, &path, load_codex_oauth)?;

    // Try with the retained token; on auth failure refresh once and retry.
    match codex_usage(&oauth.access_token, &oauth.account_id, now) {
        Err(UsageProviderError::LoginRequired) if !oauth.refresh_token.is_empty() => {
            oauth = match codex_refresh(&oauth) {
                Ok(oauth) => oauth,
                Err(error) => {
                    discard_oauth(Service::Codex)?;
                    return Err(error);
                }
            };
            retain_oauth(Service::Codex, oauth.clone())?;
            let result = codex_usage(&oauth.access_token, &oauth.account_id, now);
            if result == Err(UsageProviderError::LoginRequired) {
                discard_oauth(Service::Codex)?;
            }
            result
        }
        Err(UsageProviderError::LoginRequired) => {
            discard_oauth(Service::Codex)?;
            Err(UsageProviderError::LoginRequired)
        }
        other => other,
    }
}

fn load_codex_oauth(
    path: &Path,
    source_stamp: CredentialFileStamp,
) -> Result<CachedOAuthState, UsageProviderError> {
    let raw = std::fs::read_to_string(path).map_err(|_| UsageProviderError::NotConfigured)?;
    let auth: Value = serde_json::from_str(&raw).map_err(|_| UsageProviderError::ParseFailed)?;
    let tokens = auth
        .get("tokens")
        .ok_or(UsageProviderError::NotConfigured)?;

    Ok(CachedOAuthState {
        access_token: tokens
            .get("access_token")
            .and_then(Value::as_str)
            .filter(|token| !token.is_empty())
            .map(str::to_string)
            .ok_or(UsageProviderError::NotConfigured)?,
        refresh_token: tokens
            .get("refresh_token")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        expires_at_ms: None,
        account_id: tokens
            .get("account_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        source_stamp,
    })
}

fn codex_refresh(previous: &CachedOAuthState) -> Result<CachedOAuthState, UsageProviderError> {
    let body = json!({
        "client_id": CODEX_CLIENT_ID,
        "grant_type": "refresh_token",
        "refresh_token": previous.refresh_token,
        "scope": "openid profile email",
    });
    let response = client()?
        .post(CODEX_TOKEN_URL)
        .json(&body)
        .send()
        .map_err(|_| UsageProviderError::NetworkUnavailable)?;
    if !response.status().is_success() {
        return Err(UsageProviderError::LoginRequired);
    }
    let parsed: Value = response
        .json()
        .map_err(|_| UsageProviderError::ParseFailed)?;
    refreshed_oauth_state(previous, &parsed, now_unix_ms())
}

fn codex_usage(
    access: &str,
    account_id: &str,
    now: &str,
) -> Result<UsageSnapshot, UsageProviderError> {
    let response = client()?
        .get(CODEX_USAGE_URL)
        .bearer_auth(access)
        .header("ChatGPT-Account-Id", account_id)
        .header("originator", "codex_cli")
        .send()
        .map_err(|_| UsageProviderError::NetworkUnavailable)?;
    let status = response.status();
    if !status.is_success() {
        return Err(map_status_error(status));
    }
    let body: Value = response
        .json()
        .map_err(|_| UsageProviderError::ParseFailed)?;
    parse_codex_body(&body, now)
}

fn parse_codex_body(body: &Value, now: &str) -> Result<UsageSnapshot, UsageProviderError> {
    let rate = body
        .get("rate_limit")
        .ok_or(UsageProviderError::ParseFailed)?;

    // The payload declares each duration; only label the actual five-hour and
    // seven-day windows as such.
    let five_hour = codex_window(rate.get("primary_window"), 5 * 60 * 60);
    let week = codex_window(rate.get("secondary_window"), 7 * 24 * 60 * 60);

    let extra = match body.get("plan_type").and_then(Value::as_str) {
        Some(plan) => json!({ "plan": plan }),
        None => Value::Null,
    };

    build_snapshot(Service::Codex, five_hour, week, extra, now)
}

/// A Codex rate-limit window with its payload-declared duration.
fn codex_window(window: Option<&Value>, expected_seconds: u64) -> Option<Window> {
    let window = window?.as_object()?;
    let duration = window.get("limit_window_seconds")?.as_u64()?;
    if duration != expected_seconds {
        return None;
    }

    let used = window.get("used_percent")?.as_f64()? as f32;
    if !used.is_finite() || !(0.0..=100.0).contains(&used) {
        return None;
    }
    let reset = window
        .get("reset_at")
        .and_then(Value::as_i64)
        .and_then(unix_to_rfc3339);
    Some(Window { used, reset })
}

// ---------------------------------------------------------------- Claude

fn refresh_claude(now: &str) -> Result<UsageSnapshot, UsageProviderError> {
    let path = home()?.join(".claude/.credentials.json");
    let mut oauth = cached_or_load_oauth(Service::Claude, &path, load_claude_oauth)?;

    // Claude states an explicit expiry; refresh proactively when it's near.
    let now_ms = now_unix_ms();
    if oauth
        .expires_at_ms
        .is_some_and(|expires_at| now_ms >= expires_at - EXPIRY_SKEW_MS)
        && !oauth.refresh_token.is_empty()
    {
        oauth = match claude_refresh(&oauth) {
            Ok(oauth) => oauth,
            Err(error) => {
                discard_oauth(Service::Claude)?;
                return Err(error);
            }
        };
        retain_oauth(Service::Claude, oauth.clone())?;
    }

    match claude_usage(&oauth.access_token, now) {
        Err(UsageProviderError::LoginRequired) if !oauth.refresh_token.is_empty() => {
            oauth = match claude_refresh(&oauth) {
                Ok(oauth) => oauth,
                Err(error) => {
                    discard_oauth(Service::Claude)?;
                    return Err(error);
                }
            };
            retain_oauth(Service::Claude, oauth.clone())?;
            let result = claude_usage(&oauth.access_token, now);
            if result == Err(UsageProviderError::LoginRequired) {
                discard_oauth(Service::Claude)?;
            }
            result
        }
        Err(UsageProviderError::LoginRequired) => {
            discard_oauth(Service::Claude)?;
            Err(UsageProviderError::LoginRequired)
        }
        other => other,
    }
}

fn load_claude_oauth(
    path: &Path,
    source_stamp: CredentialFileStamp,
) -> Result<CachedOAuthState, UsageProviderError> {
    let raw = std::fs::read_to_string(path).map_err(|_| UsageProviderError::NotConfigured)?;
    let creds: Value = serde_json::from_str(&raw).map_err(|_| UsageProviderError::ParseFailed)?;
    let oauth = creds
        .get("claudeAiOauth")
        .ok_or(UsageProviderError::NotConfigured)?;

    Ok(CachedOAuthState {
        access_token: oauth
            .get("accessToken")
            .and_then(Value::as_str)
            .filter(|token| !token.is_empty())
            .map(str::to_string)
            .ok_or(UsageProviderError::NotConfigured)?,
        refresh_token: oauth
            .get("refreshToken")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        expires_at_ms: oauth.get("expiresAt").and_then(Value::as_i64),
        account_id: String::new(),
        source_stamp,
    })
}

fn claude_refresh(previous: &CachedOAuthState) -> Result<CachedOAuthState, UsageProviderError> {
    let body = json!({
        "client_id": CLAUDE_CLIENT_ID,
        "grant_type": "refresh_token",
        "refresh_token": previous.refresh_token,
    });
    let response = client()?
        .post(CLAUDE_TOKEN_URL)
        .json(&body)
        .send()
        .map_err(|_| UsageProviderError::NetworkUnavailable)?;
    if !response.status().is_success() {
        return Err(UsageProviderError::LoginRequired);
    }
    let parsed: Value = response
        .json()
        .map_err(|_| UsageProviderError::ParseFailed)?;
    refreshed_oauth_state(previous, &parsed, now_unix_ms())
}

fn claude_usage(access: &str, now: &str) -> Result<UsageSnapshot, UsageProviderError> {
    let response = client()?
        .get(CLAUDE_USAGE_URL)
        .bearer_auth(access)
        .header("anthropic-beta", CLAUDE_OAUTH_BETA)
        .send()
        .map_err(|_| UsageProviderError::NetworkUnavailable)?;
    let status = response.status();
    if !status.is_success() {
        return Err(map_status_error(status));
    }
    let body: Value = response
        .json()
        .map_err(|_| UsageProviderError::ParseFailed)?;
    parse_claude_body(&body, now)
}

fn parse_claude_body(body: &Value, now: &str) -> Result<UsageSnapshot, UsageProviderError> {
    let five_hour = claude_window(body.get("five_hour"));
    let week = claude_window(body.get("seven_day"));
    build_snapshot(Service::Claude, five_hour, week, Value::Null, now)
}

/// A Claude usage window (`utilization` percent, ISO `resets_at`).
fn claude_window(window: Option<&Value>) -> Option<Window> {
    let window = window?;
    if window.is_null() {
        return None;
    }
    let used = window.get("utilization").and_then(Value::as_f64)? as f32;
    let reset = window
        .get("resets_at")
        .and_then(Value::as_str)
        .map(str::to_string);
    Some(Window { used, reset })
}

// ---------------------------------------------------------------- Grok

fn refresh_grok(now: &str) -> Result<UsageSnapshot, UsageProviderError> {
    refresh_grok_from(
        &home()?.join(".grok/auth.json"),
        GROK_SUBSCRIPTIONS_URL,
        client()?,
        now,
    )
}

fn refresh_grok_from(
    auth_path: &Path,
    subscriptions_url: &str,
    http: &reqwest::blocking::Client,
    now: &str,
) -> Result<UsageSnapshot, UsageProviderError> {
    let access = read_grok_access_token_from(auth_path)?;
    grok_subscriptions_from(http, subscriptions_url, &access, now)
}

fn read_grok_access_token_from(path: &Path) -> Result<String, UsageProviderError> {
    let raw = std::fs::read_to_string(path).map_err(|_| UsageProviderError::NotConfigured)?;
    let auth: Value = serde_json::from_str(&raw).map_err(|_| UsageProviderError::ParseFailed)?;
    grok_access_token_from_auth(&auth).ok_or(UsageProviderError::NotConfigured)
}

fn grok_access_token_from_auth(auth: &Value) -> Option<String> {
    let entries = auth.as_object()?;
    if let Some((_, entry)) = entries
        .iter()
        .find(|(entry_key, _)| entry_key.starts_with("https://auth.x.ai"))
    {
        return grok_entry_access_token(entry);
    }

    entries.values().find_map(grok_entry_access_token)
}

fn grok_entry_access_token(entry: &Value) -> Option<String> {
    entry
        .get("key")
        .and_then(Value::as_str)
        .filter(|access| !access.is_empty())
        .map(str::to_string)
}

fn grok_subscriptions_from(
    http: &reqwest::blocking::Client,
    subscriptions_url: &str,
    access: &str,
    now: &str,
) -> Result<UsageSnapshot, UsageProviderError> {
    let response = http
        .get(subscriptions_url)
        .bearer_auth(access)
        .send()
        .map_err(|_| UsageProviderError::NetworkUnavailable)?;
    let status = response.status();
    if !status.is_success() {
        return Err(map_status_error(status));
    }
    let body: Value = response
        .json()
        .map_err(|_| UsageProviderError::ParseFailed)?;
    parse_grok_body(&body, now)
}

fn parse_grok_body(body: &Value, now: &str) -> Result<UsageSnapshot, UsageProviderError> {
    let subscriptions = body
        .get("subscriptions")
        .and_then(Value::as_array)
        .ok_or(UsageProviderError::ParseFailed)?;
    let subscription = subscriptions
        .iter()
        .find(|subscription| {
            active_grok_subscription_tier(subscription)
                .is_some_and(|tier| tier.starts_with("SUBSCRIPTION_TIER_GROK_"))
        })
        .or_else(|| {
            subscriptions
                .iter()
                .find(|subscription| active_grok_subscription_tier(subscription).is_some())
        })
        .ok_or(UsageProviderError::MissingData)?;
    let tier =
        active_grok_subscription_tier(subscription).ok_or(UsageProviderError::ParseFailed)?;
    let billing_period_end = subscription
        .get("billingPeriodEnd")
        .and_then(Value::as_str)
        .filter(|value| OffsetDateTime::parse(value, &Rfc3339).is_ok())
        .map(str::to_string);
    let mut details = base_details(Service::Grok);
    let details_object = details
        .as_object_mut()
        .ok_or(UsageProviderError::Internal)?;
    details_object.insert("plan".to_string(), Value::String(grok_plan_name(tier)));
    details_object.insert("quotaSupported".to_string(), Value::Bool(false));
    details_object.insert(
        "remainingPercentReason".to_string(),
        Value::String("grok_cli_plan_only".to_string()),
    );
    if let Some(billing_period_end) = billing_period_end {
        details_object.insert(
            "billingPeriodEnd".to_string(),
            Value::String(billing_period_end),
        );
    }

    // The subscriptions endpoint truthfully exposes plan/billing metadata only.
    // Never invent remaining/used percentages from it.
    Ok(UsageSnapshot {
        service: Service::Grok,
        remaining_percent: None,
        used_percent: None,
        reset_at: None,
        source: UsageSource::Web,
        confidence: UsageConfidence::Medium,
        last_updated: now.to_string(),
        details,
    })
}

fn active_grok_subscription_tier(subscription: &Value) -> Option<&str> {
    let tier = subscription.get("tier").and_then(Value::as_str)?;
    (subscription.get("status").and_then(Value::as_str) == Some("SUBSCRIPTION_STATUS_ACTIVE")
        && (tier.starts_with("SUBSCRIPTION_TIER_GROK_")
            || tier.starts_with("SUBSCRIPTION_TIER_X_")))
    .then_some(tier)
}

fn grok_plan_name(tier: &str) -> String {
    let tier = tier.strip_prefix("SUBSCRIPTION_TIER_").unwrap_or(tier);
    tier.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!(
                    "{}{}",
                    first.to_ascii_uppercase(),
                    chars.as_str().to_ascii_lowercase()
                ),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::atomic::{AtomicU64, Ordering},
        thread,
    };

    static NEXT_CACHE_TEST_ID: AtomicU64 = AtomicU64::new(0);

    const GROK_SUBSCRIPTIONS_FIXTURE: &str =
        include_str!("../tests/fixtures/grok-cli/subscriptions-success.json");

    fn test_stamp() -> CredentialFileStamp {
        CredentialFileStamp {
            modified: None,
            len: 0,
        }
    }

    // Real response shape from https://chatgpt.com/backend-api/codex/usage.
    #[test]
    fn parses_codex_both_windows_headline_is_five_hour() {
        let body = json!({
            "plan_type": "pro",
            "rate_limit": {
                "primary_window": {
                    "used_percent": 1,
                    "limit_window_seconds": 18000,
                    "reset_at": 1781145921
                },
                "secondary_window": {
                    "used_percent": 77,
                    "limit_window_seconds": 604800,
                    "reset_at": 1781137520
                }
            }
        });
        let snap = parse_codex_body(&body, "2026-06-10T00:00:00Z").unwrap();
        // Headline = 5-hour window (drives float + tray).
        assert_eq!(snap.remaining_percent, Some(99.0));
        assert_eq!(snap.source, UsageSource::Web);
        assert_eq!(snap.confidence, UsageConfidence::High);
        assert_eq!(snap.details["plan"], "pro");
        // Both windows are carried for the card.
        let windows = &snap.details["windows"];
        assert_eq!(windows["fiveHour"]["remainingPercent"], 99.0);
        assert_eq!(windows["week"]["remainingPercent"], 23.0);
        assert!(windows["fiveHour"]["resetAt"].is_string());
    }

    // Real response shape from https://api.anthropic.com/api/oauth/usage.
    #[test]
    fn parses_claude_both_windows() {
        let body = json!({
            "five_hour": { "utilization": 43.0, "resets_at": "2026-06-11T02:00:00Z" },
            "seven_day": { "utilization": 30.0, "resets_at": "2026-06-17T11:59:59Z" },
            "seven_day_opus": null
        });
        let snap = parse_claude_body(&body, "2026-06-10T00:00:00Z").unwrap();
        assert_eq!(snap.remaining_percent, Some(57.0));
        let windows = &snap.details["windows"];
        assert_eq!(windows["fiveHour"]["remainingPercent"], 57.0);
        assert_eq!(windows["week"]["remainingPercent"], 70.0);
        assert_eq!(snap.reset_at.as_deref(), Some("2026-06-11T02:00:00Z"));
    }

    #[test]
    fn codex_absent_disabled_or_invalid_primary_does_not_become_five_hour_window() {
        for primary in [
            Value::Null,
            json!({
                "used_percent": 0,
                "limit_window_seconds": 0,
                "reset_at": 1781145921
            }),
            json!({
                "used_percent": 12,
                "limit_window_seconds": 3600,
                "reset_at": 1781145921
            }),
            json!({
                "used_percent": 101,
                "limit_window_seconds": 18000,
                "reset_at": 1781145921
            }),
        ] {
            let body = json!({
                "plan_type": "pro",
                "rate_limit": {
                    "primary_window": primary,
                    "secondary_window": {
                        "used_percent": 40,
                        "limit_window_seconds": 604800,
                        "reset_at": 1781137520
                    }
                }
            });

            let snapshot = parse_codex_body(&body, "2026-06-10T00:00:00Z")
                .expect("valid weekly window remains usable");
            assert!(snapshot.details["windows"]["fiveHour"].is_null());
            assert_eq!(
                snapshot.details["windows"]["week"]["remainingPercent"],
                60.0
            );
            assert_eq!(snapshot.remaining_percent, Some(60.0));
        }
    }

    #[test]
    fn claude_null_week_keeps_five_hour_headline() {
        let body = json!({
            "five_hour": { "utilization": 12.0, "resets_at": "2026-06-11T02:00:00Z" },
            "seven_day": null
        });
        let snap = parse_claude_body(&body, "2026-06-10T00:00:00Z").unwrap();
        assert_eq!(snap.remaining_percent, Some(88.0));
        assert!(snap.details["windows"]["week"].is_null());
    }

    #[test]
    fn refreshed_oauth_retains_rotated_refresh_token_and_expiry() {
        let previous = CachedOAuthState {
            access_token: "old-access".to_string(),
            refresh_token: "old-refresh".to_string(),
            expires_at_ms: Some(1),
            account_id: "account".to_string(),
            source_stamp: test_stamp(),
        };

        let refreshed = refreshed_oauth_state(
            &previous,
            &json!({
                "access_token": "new-access",
                "refresh_token": "new-refresh",
                "expires_in": 3_600
            }),
            10_000,
        )
        .expect("refresh response parses");

        assert_eq!(refreshed.access_token, "new-access");
        assert_eq!(refreshed.refresh_token, "new-refresh");
        assert_eq!(refreshed.expires_at_ms, Some(3_610_000));
        assert_eq!(refreshed.account_id, "account");
    }

    #[test]
    fn refreshed_oauth_keeps_refresh_token_when_response_omits_rotation() {
        let previous = CachedOAuthState {
            access_token: "old-access".to_string(),
            refresh_token: "old-refresh".to_string(),
            expires_at_ms: None,
            account_id: String::new(),
            source_stamp: test_stamp(),
        };

        let refreshed =
            refreshed_oauth_state(&previous, &json!({ "access_token": "new-access" }), 10_000)
                .expect("refresh response parses");

        assert_eq!(refreshed.refresh_token, "old-refresh");
        assert_eq!(refreshed.expires_at_ms, None);
    }

    #[test]
    fn oauth_cache_reloads_when_the_cli_credentials_file_changes() {
        let path = std::env::temp_dir().join(format!(
            "pickgauge-oauth-cache-test-{}-{}",
            std::process::id(),
            NEXT_CACHE_TEST_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, "first").expect("first credentials write succeeds");

        let first = cached_or_load_oauth(Service::Grok, &path, |path, source_stamp| {
            Ok(CachedOAuthState {
                access_token: std::fs::read_to_string(path).expect("credentials read succeeds"),
                refresh_token: String::new(),
                expires_at_ms: None,
                account_id: String::new(),
                source_stamp,
            })
        })
        .expect("first credentials load succeeds");
        assert_eq!(first.access_token, "first");

        let cached = cached_or_load_oauth(Service::Grok, &path, |_, _| {
            panic!("unchanged credentials should use the cache")
        })
        .expect("cached credentials load succeeds");
        assert_eq!(cached.access_token, "first");

        std::fs::write(&path, "second-longer").expect("replacement credentials write succeeds");
        let reloaded = cached_or_load_oauth(Service::Grok, &path, |path, source_stamp| {
            Ok(CachedOAuthState {
                access_token: std::fs::read_to_string(path).expect("credentials read succeeds"),
                refresh_token: String::new(),
                expires_at_ms: None,
                account_id: String::new(),
                source_stamp,
            })
        })
        .expect("replacement credentials load succeeds");

        assert_eq!(reloaded.access_token, "second-longer");
        discard_oauth(Service::Grok).expect("test cache clears");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    // TODO(#69): split subscription selection assertions by source.
    #[allow(clippy::cognitive_complexity)]
    fn prefers_active_grok_subscription_over_active_x_tier() {
        let body: Value = serde_json::from_str(GROK_SUBSCRIPTIONS_FIXTURE).unwrap();
        let snapshot = parse_grok_body(&body, "2026-07-09T20:00:00Z").unwrap();

        assert_eq!(snapshot.service, Service::Grok);
        assert_eq!(snapshot.remaining_percent, None);
        assert_eq!(snapshot.used_percent, None);
        assert_eq!(snapshot.reset_at, None);
        assert_eq!(snapshot.source, UsageSource::Web);
        assert_eq!(snapshot.confidence, UsageConfidence::Medium);
        assert_eq!(snapshot.details["status"], "parsed");
        assert_eq!(snapshot.details["providerId"], "grok.cli");
        assert_eq!(snapshot.details["source"], "web");
        assert_eq!(snapshot.details["via"], "cli");
        assert_eq!(snapshot.details["plan"], "Grok Pro");
        assert_eq!(snapshot.details["billingPeriodEnd"], "2026-08-24T20:20:59Z");
        assert_eq!(snapshot.details["quotaSupported"], false);
        assert_eq!(snapshot.details["remainingPercentReason"], "grok_cli_plan_only");
        assert!(snapshot.details.get("windows").is_none());
    }

    #[test]
    fn parses_an_active_x_tier_subscription() {
        let body = json!({
            "subscriptions": [{
                "tier": "SUBSCRIPTION_TIER_X_PREMIUM",
                "status": "SUBSCRIPTION_STATUS_ACTIVE"
            }]
        });

        let snapshot = parse_grok_body(&body, "2026-07-09T20:00:00Z").unwrap();

        assert_eq!(snapshot.details["plan"], "X Premium");
        assert!(snapshot.details.get("billingPeriodEnd").is_none());
        assert_eq!(snapshot.remaining_percent, None);
    }

    #[test]
    fn grok_missing_subscriptions_is_parse_failed() {
        assert_eq!(
            parse_grok_body(&json!({}), "2026-07-09T20:00:00Z"),
            Err(UsageProviderError::ParseFailed)
        );
    }

    #[test]
    fn grok_without_an_active_grok_subscription_is_missing_data() {
        let body = json!({
            "subscriptions": [{
                "tier": "SUBSCRIPTION_TIER_GROK_PRO",
                "status": "SUBSCRIPTION_STATUS_INACTIVE"
            }]
        });

        assert_eq!(
            parse_grok_body(&body, "2026-07-09T20:00:00Z"),
            Err(UsageProviderError::MissingData)
        );
    }

    #[test]
    fn prefers_current_grok_auth_entry_over_legacy_entries() {
        let auth = json!({
            "https://accounts.x.ai/sign-in": { "key": "legacy" },
            "https://auth.x.ai::current-client": { "key": "current" }
        });

        assert_eq!(grok_access_token_from_auth(&auth).as_deref(), Some("current"));
    }

    #[test]
    fn uses_legacy_grok_auth_entry_when_current_entry_is_absent() {
        let auth = json!({
            "https://accounts.x.ai/sign-in": { "key": "legacy" }
        });

        assert_eq!(grok_access_token_from_auth(&auth).as_deref(), Some("legacy"));
    }

    #[test]
    fn ollama_has_no_cli_collection_path() {
        assert_eq!(
            refresh(Service::Ollama, "2026-07-09T20:00:00Z"),
            Err(UsageProviderError::Internal)
        );
    }

    fn write_temp_auth(contents: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "pickgauge-grok-auth-{}-{}",
            std::process::id(),
            NEXT_CACHE_TEST_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, contents).expect("temp auth write succeeds");
        path
    }

    fn test_http_client(timeout: Duration) -> reqwest::blocking::Client {
        reqwest::blocking::Client::builder()
            .timeout(timeout)
            .no_proxy()
            .build()
            .expect("test http client builds")
    }

    fn serve_one_http_response(status_line: &str, body: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test port binds");
        let address = listener.local_addr().expect("test port address");
        let status_line = status_line.to_string();
        let body = body.to_string();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("request accepted");
            let mut request = [0; 4096];
            let _ = stream.read(&mut request);
            let response = format!(
                "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("response written");
        });
        format!("http://{address}/rest/subscriptions")
    }

    fn serve_hung_http() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test port binds");
        let address = listener.local_addr().expect("test port address");
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("hung request accepted");
            let mut request = [0; 1024];
            let _ = stream.read(&mut request);
            thread::sleep(Duration::from_millis(250));
        });
        format!("http://{address}/rest/subscriptions")
    }

    #[test]
    fn grok_missing_auth_file_is_not_configured() {
        let missing = std::env::temp_dir().join(format!(
            "pickgauge-grok-auth-missing-{}-{}",
            std::process::id(),
            NEXT_CACHE_TEST_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&missing);

        assert_eq!(
            refresh_grok_from(
                &missing,
                GROK_SUBSCRIPTIONS_URL,
                &test_http_client(Duration::from_millis(100)),
                "2026-07-09T20:00:00Z",
            ),
            Err(UsageProviderError::NotConfigured)
        );
    }

    #[test]
    fn grok_malformed_auth_json_is_parse_failed() {
        let path = write_temp_auth("{not-json");

        assert_eq!(
            refresh_grok_from(
                &path,
                GROK_SUBSCRIPTIONS_URL,
                &test_http_client(Duration::from_millis(100)),
                "2026-07-09T20:00:00Z",
            ),
            Err(UsageProviderError::ParseFailed)
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn grok_auth_without_access_token_is_not_configured() {
        let path = write_temp_auth(r#"{"https://auth.x.ai::client":{}}"#);

        assert_eq!(
            refresh_grok_from(
                &path,
                GROK_SUBSCRIPTIONS_URL,
                &test_http_client(Duration::from_millis(100)),
                "2026-07-09T20:00:00Z",
            ),
            Err(UsageProviderError::NotConfigured)
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn grok_expired_token_response_is_login_required() {
        let path = write_temp_auth(r#"{"https://auth.x.ai::client":{"key":"expired-token"}}"#);
        let url = serve_one_http_response("HTTP/1.1 401 Unauthorized", r#"{"error":"expired"}"#);

        assert_eq!(
            refresh_grok_from(
                &path,
                &url,
                &test_http_client(Duration::from_secs(2)),
                "2026-07-09T20:00:00Z",
            ),
            Err(UsageProviderError::LoginRequired)
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn grok_other_non_success_response_is_parse_failed() {
        let path = write_temp_auth(r#"{"https://auth.x.ai::client":{"key":"token"}}"#);
        let url = serve_one_http_response("HTTP/1.1 500 Internal Server Error", r#"{"error":"boom"}"#);

        assert_eq!(
            refresh_grok_from(
                &path,
                &url,
                &test_http_client(Duration::from_secs(2)),
                "2026-07-09T20:00:00Z",
            ),
            Err(UsageProviderError::ParseFailed)
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn grok_malformed_body_json_is_parse_failed() {
        let path = write_temp_auth(r#"{"https://auth.x.ai::client":{"key":"token"}}"#);
        let url = serve_one_http_response("HTTP/1.1 200 OK", "{not-json");

        assert_eq!(
            refresh_grok_from(
                &path,
                &url,
                &test_http_client(Duration::from_secs(2)),
                "2026-07-09T20:00:00Z",
            ),
            Err(UsageProviderError::ParseFailed)
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn grok_request_timeout_is_network_unavailable() {
        let path = write_temp_auth(r#"{"https://auth.x.ai::client":{"key":"token"}}"#);
        let url = serve_hung_http();

        assert_eq!(
            refresh_grok_from(
                &path,
                &url,
                &test_http_client(Duration::from_millis(100)),
                "2026-07-09T20:00:00Z",
            ),
            Err(UsageProviderError::NetworkUnavailable)
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn grok_live_loopback_subscriptions_parse_plan_only() {
        let path = write_temp_auth(r#"{"https://auth.x.ai::client":{"key":"token"}}"#);
        let url = serve_one_http_response("HTTP/1.1 200 OK", GROK_SUBSCRIPTIONS_FIXTURE);

        let snapshot = refresh_grok_from(
            &path,
            &url,
            &test_http_client(Duration::from_secs(2)),
            "2026-07-09T20:00:00Z",
        )
        .expect("loopback subscriptions parse");

        assert_eq!(snapshot.details["plan"], "Grok Pro");
        assert_eq!(snapshot.remaining_percent, None);
        let _ = std::fs::remove_file(path);
    }
}
