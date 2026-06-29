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

/// Build a snapshot carrying BOTH windows. The headline number (drives the
/// float capsule and the OS tray) is the 5-hour session window, falling back
/// to the weekly window when the session window is absent.
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

    // primary_window = 5-hour session, secondary_window = weekly.
    let five_hour = codex_window(rate.get("primary_window"));
    let week = codex_window(rate.get("secondary_window"));

    let extra = match body.get("plan_type").and_then(Value::as_str) {
        Some(plan) => json!({ "plan": plan }),
        None => Value::Null,
    };

    build_snapshot(Service::Codex, five_hour, week, extra, now)
}

/// A Codex rate-limit window (`used_percent`, unix `reset_at`).
fn codex_window(window: Option<&Value>) -> Option<Window> {
    let window = window?;
    let used = window.get("used_percent").and_then(Value::as_f64)? as f32;
    let reset = window
        .get("reset_at")
        .and_then(Value::as_i64)
        .and_then(unix_to_rfc3339);
    Some(Window { used, reset })
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

#[cfg(test)]
mod tests {
    use super::*;

    // Real response shape from https://chatgpt.com/backend-api/codex/usage.
    #[test]
    fn parses_codex_both_windows_headline_is_five_hour() {
        let body = json!({
            "plan_type": "pro",
            "rate_limit": {
                "primary_window": { "used_percent": 1, "reset_at": 1781145921 },
                "secondary_window": { "used_percent": 77, "reset_at": 1781137520 }
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
    fn codex_missing_rate_limit_is_parse_failed() {
        let body = json!({ "plan_type": "pro" });
        assert_eq!(
            parse_codex_body(&body, "2026-06-10T00:00:00Z"),
            Err(UsageProviderError::ParseFailed)
        );
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
}
