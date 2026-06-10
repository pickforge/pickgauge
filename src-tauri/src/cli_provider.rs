//! Official usage readings via the Codex/Claude CLIs' own OAuth credentials.
//!
//! Instead of scraping the dashboards in a headless browser, this reads the
//! tokens the installed CLIs already stored on disk, refreshes them in memory
//! when needed (never writing back to the CLI's files), and calls the same
//! usage endpoints the CLIs call. The result is the provider's real quota
//! number with no browser, captcha, or login flow.
//!
//! Endpoints/clients discovered from the shipped CLI binaries:
//! - Codex   refresh: POST https://auth.openai.com/oauth/token (client app_EMoamEEZ73f0CkXaXp7hrann)
//!           usage:   GET  https://chatgpt.com/backend-api/codex/usage
//! - Claude  refresh: POST https://platform.claude.com/v1/oauth/token (client 9d1c250a-…)
//!           usage:   GET  https://api.anthropic.com/api/oauth/usage

use std::path::PathBuf;
use std::time::Duration;

use serde_json::{json, Value};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::usage::{Service, UsageConfidence, UsageProviderError, UsageProviderId, UsageSnapshot, UsageSource};

const CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const CODEX_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const CODEX_USAGE_URL: &str = "https://chatgpt.com/backend-api/codex/usage";

const CLAUDE_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const CLAUDE_TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
const CLAUDE_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const CLAUDE_OAUTH_BETA: &str = "oauth-2025-04-20";

const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
// Refresh a Claude token this many ms before its stated expiry.
const EXPIRY_SKEW_MS: i64 = 60_000;

fn home() -> Result<PathBuf, UsageProviderError> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or(UsageProviderError::Internal)
}

fn client() -> Result<reqwest::blocking::Client, UsageProviderError> {
    reqwest::blocking::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|_| UsageProviderError::Internal)
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

/// Entry point used by the engine for the (codex|claude).cli providers.
pub fn refresh(service: Service, now: &str) -> Result<UsageSnapshot, UsageProviderError> {
    match service {
        Service::Codex => refresh_codex(now),
        Service::Claude => refresh_claude(now),
    }
}

fn snapshot(
    service: Service,
    used_percent: f32,
    reset_at: Option<String>,
    now: &str,
    details: Value,
) -> UsageSnapshot {
    let used = used_percent.clamp(0.0, 100.0);
    UsageSnapshot {
        service,
        remaining_percent: Some((100.0 - used).clamp(0.0, 100.0)),
        used_percent: Some(used),
        reset_at,
        source: UsageSource::Web,
        confidence: UsageConfidence::High,
        last_updated: now.to_string(),
        details,
    }
}

fn base_details(service: Service) -> Value {
    let provider_id = match service {
        Service::Codex => UsageProviderId::CodexCli,
        Service::Claude => UsageProviderId::ClaudeCli,
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
    let raw = std::fs::read_to_string(&path).map_err(|_| UsageProviderError::NotConfigured)?;
    let auth: Value = serde_json::from_str(&raw).map_err(|_| UsageProviderError::ParseFailed)?;

    let tokens = auth.get("tokens").ok_or(UsageProviderError::NotConfigured)?;
    let mut access = tokens
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or(UsageProviderError::NotConfigured)?;
    let refresh_token = tokens
        .get("refresh_token")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let account_id = tokens
        .get("account_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    // Try with the stored token; on auth failure refresh once and retry.
    match codex_usage(&access, &account_id, now) {
        Err(UsageProviderError::LoginRequired) if !refresh_token.is_empty() => {
            access = codex_refresh(&refresh_token)?;
            codex_usage(&access, &account_id, now)
        }
        other => other,
    }
}

fn codex_refresh(refresh_token: &str) -> Result<String, UsageProviderError> {
    let body = json!({
        "client_id": CODEX_CLIENT_ID,
        "grant_type": "refresh_token",
        "refresh_token": refresh_token,
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
    let parsed: Value = response.json().map_err(|_| UsageProviderError::ParseFailed)?;
    parsed
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or(UsageProviderError::ParseFailed)
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
    let body: Value = response.json().map_err(|_| UsageProviderError::ParseFailed)?;
    parse_codex_body(&body, now)
}

fn parse_codex_body(body: &Value, now: &str) -> Result<UsageSnapshot, UsageProviderError> {
    let rate = body.get("rate_limit").ok_or(UsageProviderError::ParseFailed)?;

    // Use whichever window is closest to its cap — that's the binding limit.
    let primary = codex_window(rate.get("primary_window"));
    let secondary = codex_window(rate.get("secondary_window"));
    let primary_pct = primary.as_ref().map(|w| w.0);
    let secondary_pct = secondary.as_ref().map(|w| w.0);
    let binding = [primary, secondary]
        .into_iter()
        .flatten()
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .ok_or(UsageProviderError::MissingData)?;

    let mut details = base_details(Service::Codex);
    if let Some(obj) = details.as_object_mut() {
        if let Some(plan) = body.get("plan_type").and_then(Value::as_str) {
            obj.insert("plan".into(), json!(plan));
        }
        obj.insert("primaryUsedPercent".into(), json!(primary_pct));
        obj.insert("secondaryUsedPercent".into(), json!(secondary_pct));
        obj.insert(
            "bindingWindow".into(),
            json!(if primary_pct.is_some_and(|p| (binding.0 - p).abs() < f32::EPSILON) {
                "primary"
            } else {
                "secondary"
            }),
        );
    }

    Ok(snapshot(Service::Codex, binding.0, binding.1, now, details))
}

/// (used_percent, reset_at_rfc3339) for a Codex rate-limit window.
fn codex_window(window: Option<&Value>) -> Option<(f32, Option<String>)> {
    let window = window?;
    let used = window.get("used_percent").and_then(Value::as_f64)? as f32;
    let reset = window
        .get("reset_at")
        .and_then(Value::as_i64)
        .and_then(unix_to_rfc3339);
    Some((used, reset))
}

// ---------------------------------------------------------------- Claude

fn refresh_claude(now: &str) -> Result<UsageSnapshot, UsageProviderError> {
    let path = home()?.join(".claude/.credentials.json");
    let raw = std::fs::read_to_string(&path).map_err(|_| UsageProviderError::NotConfigured)?;
    let creds: Value = serde_json::from_str(&raw).map_err(|_| UsageProviderError::ParseFailed)?;
    let oauth = creds
        .get("claudeAiOauth")
        .ok_or(UsageProviderError::NotConfigured)?;

    let mut access = oauth
        .get("accessToken")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or(UsageProviderError::NotConfigured)?;
    let refresh_token = oauth
        .get("refreshToken")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let expires_at = oauth.get("expiresAt").and_then(Value::as_i64).unwrap_or(0);

    // Claude states an explicit expiry; refresh proactively when it's near.
    let now_ms = OffsetDateTime::now_utc().unix_timestamp() * 1000;
    if expires_at > 0 && now_ms >= expires_at - EXPIRY_SKEW_MS && !refresh_token.is_empty() {
        access = claude_refresh(&refresh_token)?;
    }

    match claude_usage(&access, now) {
        Err(UsageProviderError::LoginRequired) if !refresh_token.is_empty() => {
            access = claude_refresh(&refresh_token)?;
            claude_usage(&access, now)
        }
        other => other,
    }
}

fn claude_refresh(refresh_token: &str) -> Result<String, UsageProviderError> {
    let body = json!({
        "client_id": CLAUDE_CLIENT_ID,
        "grant_type": "refresh_token",
        "refresh_token": refresh_token,
    });
    let response = client()?
        .post(CLAUDE_TOKEN_URL)
        .json(&body)
        .send()
        .map_err(|_| UsageProviderError::NetworkUnavailable)?;
    if !response.status().is_success() {
        return Err(UsageProviderError::LoginRequired);
    }
    let parsed: Value = response.json().map_err(|_| UsageProviderError::ParseFailed)?;
    parsed
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or(UsageProviderError::ParseFailed)
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
    let body: Value = response.json().map_err(|_| UsageProviderError::ParseFailed)?;
    parse_claude_body(&body, now)
}

fn parse_claude_body(body: &Value, now: &str) -> Result<UsageSnapshot, UsageProviderError> {
    let five_hour = claude_window(body.get("five_hour"));
    let seven_day = claude_window(body.get("seven_day"));
    let five_hour_pct = five_hour.as_ref().map(|w| w.0);
    let seven_day_pct = seven_day.as_ref().map(|w| w.0);
    let binding = [five_hour, seven_day]
        .into_iter()
        .flatten()
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .ok_or(UsageProviderError::MissingData)?;

    let mut details = base_details(Service::Claude);
    if let Some(obj) = details.as_object_mut() {
        obj.insert("fiveHourUtilization".into(), json!(five_hour_pct));
        obj.insert("sevenDayUtilization".into(), json!(seven_day_pct));
    }

    Ok(snapshot(Service::Claude, binding.0, binding.1, now, details))
}

/// (utilization_percent, resets_at) for a Claude usage window.
fn claude_window(window: Option<&Value>) -> Option<(f32, Option<String>)> {
    let window = window?;
    if window.is_null() {
        return None;
    }
    let util = window.get("utilization").and_then(Value::as_f64)? as f32;
    let reset = window
        .get("resets_at")
        .and_then(Value::as_str)
        .map(str::to_string);
    Some((util, reset))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real response shape from https://chatgpt.com/backend-api/codex/usage.
    #[test]
    fn parses_codex_binding_window() {
        let body = json!({
            "plan_type": "pro",
            "rate_limit": {
                "primary_window": { "used_percent": 0, "reset_at": 1781145921 },
                "secondary_window": { "used_percent": 77, "reset_at": 1781137520 }
            }
        });
        let snap = parse_codex_body(&body, "2026-06-10T00:00:00Z").unwrap();
        assert_eq!(snap.used_percent, Some(77.0));
        assert_eq!(snap.remaining_percent, Some(23.0));
        assert_eq!(snap.source, UsageSource::Web);
        assert_eq!(snap.confidence, UsageConfidence::High);
        assert_eq!(snap.details["plan"], "pro");
        assert_eq!(snap.details["bindingWindow"], "secondary");
        assert!(snap.reset_at.is_some());
    }

    // Real response shape from https://api.anthropic.com/api/oauth/usage.
    #[test]
    fn parses_claude_binding_window() {
        let body = json!({
            "five_hour": { "utilization": 33.0, "resets_at": "2026-06-11T02:00:00Z" },
            "seven_day": { "utilization": 29.0, "resets_at": "2026-06-17T11:59:59Z" },
            "seven_day_opus": null
        });
        let snap = parse_claude_body(&body, "2026-06-10T00:00:00Z").unwrap();
        assert_eq!(snap.used_percent, Some(33.0));
        assert_eq!(snap.remaining_percent, Some(67.0));
        assert_eq!(snap.reset_at.as_deref(), Some("2026-06-11T02:00:00Z"));
    }

    #[test]
    fn codex_missing_rate_limit_is_parse_failed() {
        let body = json!({ "plan_type": "pro" });
        assert_eq!(
            parse_codex_body(&body, "2026-06-10T00:00:00Z"),
            Err(UsageProviderError::ParseFailed)
        );
    }

    #[test]
    fn claude_skips_null_windows() {
        let body = json!({
            "five_hour": { "utilization": 12.0, "resets_at": "2026-06-11T02:00:00Z" },
            "seven_day": null
        });
        let snap = parse_claude_body(&body, "2026-06-10T00:00:00Z").unwrap();
        assert_eq!(snap.used_percent, Some(12.0));
    }
}
