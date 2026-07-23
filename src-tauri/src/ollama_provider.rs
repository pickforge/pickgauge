//! Local Ollama daemon telemetry.
//!
//! Reads only the machine-local Ollama HTTP API (never browser cookies/profiles).
//! Ollama has no account-wide quota percentage; snapshots report availability,
//! installed/loaded model counts, and an optional Cloud plan label when the
//! daemon exposes one via `/api/me`. Percent gauges stay `null`.
//!
//! Endpoint base comes from `OLLAMA_HOST` (default `127.0.0.1:11434`). Non-loopback
//! hosts are rejected so the provider stays local-only (pickgauge#29).
use std::{
    env,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    time::Duration,
};

use serde::Deserialize;
use serde_json::json;

use crate::usage::{
    Service, UsageConfidence, UsageProvider, UsageProviderError, UsageProviderId, UsageSnapshot,
    UsageSource,
};

const DEFAULT_HOST: &str = "127.0.0.1:11434";
const HTTP_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone, Debug)]
pub(crate) struct OllamaLocalProvider {
    base_url: Result<String, UsageProviderError>,
    timeout: Duration,
}

#[derive(Debug, Deserialize)]
struct TagsResponse {
    #[serde(default)]
    models: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct PsResponse {
    #[serde(default)]
    models: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct MeResponse {
    plan: Option<String>,
}

impl Default for OllamaLocalProvider {
    fn default() -> Self {
        Self {
            base_url: resolve_ollama_base_url(env::var("OLLAMA_HOST").ok().as_deref()),
            timeout: HTTP_TIMEOUT,
        }
    }
}

impl OllamaLocalProvider {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            base_url: Ok(base_url.into()),
            timeout: HTTP_TIMEOUT,
        }
    }

    #[cfg(test)]
    fn with_timeout(base_url: impl Into<String>, timeout: Duration) -> Self {
        Self {
            base_url: Ok(base_url.into()),
            timeout,
        }
    }

    fn base_url(&self) -> Result<&str, UsageProviderError> {
        match &self.base_url {
            Ok(url) => Ok(url.as_str()),
            Err(error) => Err(*error),
        }
    }

    fn tags_endpoint(&self) -> Result<String, UsageProviderError> {
        Ok(format!("{}/api/tags", self.base_url()?.trim_end_matches('/')))
    }

    fn ps_endpoint(&self) -> Result<String, UsageProviderError> {
        Ok(format!("{}/api/ps", self.base_url()?.trim_end_matches('/')))
    }

    fn me_endpoint(&self) -> Result<String, UsageProviderError> {
        Ok(format!("{}/api/me", self.base_url()?.trim_end_matches('/')))
    }

    fn refresh_snapshot(&self, now: &str) -> Result<UsageSnapshot, UsageProviderError> {
        let tags_endpoint = self.tags_endpoint()?;
        let ps_endpoint = self.ps_endpoint()?;
        let me_endpoint = self.me_endpoint()?;
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .timeout(self.timeout)
            .build()
            .map_err(|_| UsageProviderError::Internal)?;

        let tags_response = client
            .get(tags_endpoint)
            .send()
            .map_err(map_request_error)?;
        let tags_status = tags_response.status();
        if !tags_status.is_success() {
            return Err(map_status_error(tags_status));
        }
        let tags_body = tags_response
            .text()
            .map_err(|_| UsageProviderError::ParseFailed)?;
        let model_count = parse_tags_model_count(&tags_body)?;

        let loaded_model_count = client
            .get(ps_endpoint)
            .send()
            .ok()
            .filter(|response| response.status().is_success())
            .and_then(|response| response.text().ok())
            .and_then(|body| parse_ps_model_count(&body).ok());

        let plan = client
            .post(me_endpoint)
            .send()
            .ok()
            .filter(|response| response.status().is_success())
            .and_then(|response| response.text().ok())
            .and_then(|body| parse_me_plan(&body));

        Ok(availability_snapshot(
            now,
            model_count,
            loaded_model_count,
            plan,
        ))
    }
}

/// Resolve `OLLAMA_HOST` into an `http(s)://loopback[:port]` base URL.
/// Non-loopback hosts are rejected so the provider never leaves the machine.
pub(crate) fn resolve_ollama_base_url(raw: Option<&str>) -> Result<String, UsageProviderError> {
    let input = raw
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_HOST);

    let (scheme, remainder) = match input.split_once("://") {
        Some((scheme, rest)) => {
            let scheme = scheme.to_ascii_lowercase();
            if scheme != "http" && scheme != "https" {
                return Err(UsageProviderError::UnsafePath);
            }
            (scheme, rest)
        }
        None => ("http".to_string(), input),
    };

    let host_port = remainder
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(remainder)
        .trim();
    if host_port.is_empty() {
        return Err(UsageProviderError::UnsafePath);
    }

    let host = host_from_host_port(host_port).ok_or(UsageProviderError::UnsafePath)?;
    let normalized_host = normalize_loopback_host(&host).ok_or(UsageProviderError::UnsafePath)?;

    let host_port = if host.eq_ignore_ascii_case(&normalized_host) {
        host_port.to_string()
    } else if let Some(port) = port_from_host_port(host_port) {
        format!("{normalized_host}:{port}")
    } else {
        normalized_host
    };

    Ok(format!("{scheme}://{host_port}"))
}

fn host_from_host_port(host_port: &str) -> Option<String> {
    if host_port.starts_with('[') {
        let end = host_port.find(']')?;
        return Some(host_port[1..end].to_string());
    }

    if let Some((host, port)) = host_port.rsplit_once(':') {
        if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) {
            return Some(host.to_string());
        }
    }

    Some(host_port.to_string())
}

fn port_from_host_port(host_port: &str) -> Option<&str> {
    if host_port.starts_with('[') {
        let end = host_port.find(']')?;
        let rest = host_port.get(end + 1..)?;
        return rest.strip_prefix(':').filter(|port| {
            !port.is_empty() && port.chars().all(|c| c.is_ascii_digit())
        });
    }

    host_port.rsplit_once(':').and_then(|(_, port)| {
        ( !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) ).then_some(port)
    })
}

fn normalize_loopback_host(host: &str) -> Option<String> {
    let host = host.trim();
    if host.eq_ignore_ascii_case("localhost") {
        return Some("localhost".to_string());
    }

    if let Ok(ip) = host.parse::<IpAddr>() {
        return match ip {
            IpAddr::V4(v4) if v4.is_loopback() || v4 == Ipv4Addr::UNSPECIFIED => {
                Some(if v4 == Ipv4Addr::UNSPECIFIED {
                    "127.0.0.1".to_string()
                } else {
                    v4.to_string()
                })
            }
            IpAddr::V6(v6) if v6.is_loopback() || v6 == Ipv6Addr::UNSPECIFIED => {
                Some(if v6 == Ipv6Addr::UNSPECIFIED {
                    "[::1]".to_string()
                } else {
                    format!("[{v6}]")
                })
            }
            _ => None,
        };
    }

    None
}

fn map_request_error(error: reqwest::Error) -> UsageProviderError {
    if error.is_timeout() {
        UsageProviderError::TimedOut
    } else if error.is_connect() {
        UsageProviderError::NotConfigured
    } else {
        UsageProviderError::NetworkUnavailable
    }
}

fn map_status_error(status: reqwest::StatusCode) -> UsageProviderError {
    match status {
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
            UsageProviderError::LoginRequired
        }
        status if status.is_server_error() => UsageProviderError::NetworkUnavailable,
        _ => UsageProviderError::ParseFailed,
    }
}

fn parse_tags_model_count(body: &str) -> Result<u64, UsageProviderError> {
    let parsed: TagsResponse =
        serde_json::from_str(body.trim()).map_err(|_| UsageProviderError::ParseFailed)?;
    Ok(parsed.models.len() as u64)
}

fn parse_ps_model_count(body: &str) -> Result<u64, UsageProviderError> {
    let parsed: PsResponse =
        serde_json::from_str(body.trim()).map_err(|_| UsageProviderError::ParseFailed)?;
    Ok(parsed.models.len() as u64)
}

fn parse_me_plan(body: &str) -> Option<String> {
    let body = body.trim();
    if body.is_empty() || body == "null" {
        return None;
    }
    let parsed: MeResponse = serde_json::from_str(body).ok()?;
    parsed
        .plan
        .map(|plan| plan.trim().to_string())
        .filter(|plan| !plan.is_empty())
}

fn availability_snapshot(
    now: &str,
    model_count: u64,
    loaded_model_count: Option<u64>,
    plan: Option<String>,
) -> UsageSnapshot {
    let mut details = json!({
        "status": "parsed",
        "providerId": UsageProviderId::OllamaLocal.code(),
        "source": UsageSource::Local.code(),
        "via": "daemon",
        "modelCount": model_count,
        "quotaSupported": false,
        "remainingPercentReason": "ollama_has_no_account_quota",
    });

    if let Some(object) = details.as_object_mut() {
        if let Some(loaded) = loaded_model_count {
            object.insert("loadedModelCount".into(), json!(loaded));
        }
        if let Some(plan) = plan {
            object.insert("plan".into(), json!(plan));
        }
    }

    UsageSnapshot {
        service: Service::Ollama,
        remaining_percent: None,
        used_percent: None,
        reset_at: None,
        source: UsageSource::Local,
        confidence: UsageConfidence::Medium,
        last_updated: now.to_string(),
        details,
    }
}

impl UsageProvider for OllamaLocalProvider {
    fn provider_id(&self) -> UsageProviderId {
        UsageProviderId::OllamaLocal
    }

    fn service(&self) -> Service {
        Service::Ollama
    }

    fn source(&self) -> UsageSource {
        UsageSource::Local
    }

    fn refresh(&self, now: &str) -> Result<UsageSnapshot, UsageProviderError> {
        self.refresh_snapshot(now)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::Duration,
    };

    use super::*;

    const TAGS_FIXTURE: &str = r#"{"models":[{"name":"llama3.2:latest"},{"name":"nomic-embed-text:latest"}]}"#;
    const PS_FIXTURE: &str = r#"{"models":[{"name":"llama3.2:latest"}]}"#;
    const ME_FIXTURE: &str = r#"{
        "id": "<redacted>",
        "email": "<redacted>",
        "name": "<redacted>",
        "avatarurl": "<redacted>",
        "plan": "pro"
    }"#;

    #[test]
    // TODO(#69): split availability and quota assertions.
    #[allow(clippy::cognitive_complexity)]
    fn availability_snapshot_never_fabricates_quota_percentages() {
        let snapshot = availability_snapshot("2026-07-09T12:00:00Z", 2, Some(1), Some("pro".into()));

        assert_eq!(snapshot.service, Service::Ollama);
        assert_eq!(snapshot.remaining_percent, None);
        assert_eq!(snapshot.used_percent, None);
        assert_eq!(snapshot.reset_at, None);
        assert_eq!(snapshot.source, UsageSource::Local);
        assert_eq!(snapshot.confidence, UsageConfidence::Medium);
        assert_eq!(snapshot.details["status"], "parsed");
        assert_eq!(snapshot.details["providerId"], "ollama.local");
        assert_eq!(snapshot.details["via"], "daemon");
        assert_eq!(snapshot.details["modelCount"], 2);
        assert_eq!(snapshot.details["loadedModelCount"], 1);
        assert_eq!(snapshot.details["plan"], "pro");
        assert_eq!(snapshot.details["quotaSupported"], false);
        assert_eq!(
            snapshot.details["remainingPercentReason"],
            "ollama_has_no_account_quota"
        );
        assert!(snapshot.details.get("email").is_none());
        assert!(snapshot.details.get("id").is_none());
        assert!(snapshot.details.get("name").is_none());
    }

    #[test]
    fn parses_tags_and_ps_counts() {
        assert_eq!(parse_tags_model_count(TAGS_FIXTURE).unwrap(), 2);
        assert_eq!(parse_ps_model_count(PS_FIXTURE).unwrap(), 1);
        assert_eq!(parse_tags_model_count(r#"{"models":[]}"#).unwrap(), 0);
    }

    #[test]
    fn me_plan_is_optional_and_identity_free() {
        assert_eq!(parse_me_plan(ME_FIXTURE).as_deref(), Some("pro"));
        assert_eq!(parse_me_plan(r#"{"plan":null}"#), None);
        assert_eq!(parse_me_plan(""), None);
        assert_eq!(parse_me_plan("null"), None);
        assert_eq!(parse_me_plan(r#"{"plan":"  "}"#), None);
    }

    #[test]
    fn malformed_tags_body_is_parse_failed() {
        assert_eq!(
            parse_tags_model_count("{"),
            Err(UsageProviderError::ParseFailed)
        );
    }

    #[test]
    fn default_and_loopback_ollama_hosts_resolve() {
        assert_eq!(
            resolve_ollama_base_url(None).unwrap(),
            "http://127.0.0.1:11434"
        );
        assert_eq!(
            resolve_ollama_base_url(Some("localhost:11435")).unwrap(),
            "http://localhost:11435"
        );
        assert_eq!(
            resolve_ollama_base_url(Some("http://127.0.0.1:11434")).unwrap(),
            "http://127.0.0.1:11434"
        );
        assert_eq!(
            resolve_ollama_base_url(Some("https://[::1]:11434")).unwrap(),
            "https://[::1]:11434"
        );
        assert_eq!(
            resolve_ollama_base_url(Some("0.0.0.0:11434")).unwrap(),
            "http://127.0.0.1:11434"
        );
    }

    #[test]
    fn non_loopback_ollama_hosts_are_rejected() {
        for host in [
            "192.168.1.10:11434",
            "http://example.com:11434",
            "https://10.0.0.5",
            "ollama.internal",
            "file:///tmp/ollama",
        ] {
            assert_eq!(
                resolve_ollama_base_url(Some(host)),
                Err(UsageProviderError::UnsafePath),
                "host {host}"
            );
        }
    }

    #[test]
    fn closed_daemon_port_is_not_configured() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test port binds");
        let address = listener.local_addr().expect("test port address");
        drop(listener);
        let provider = OllamaLocalProvider::with_base_url(format!("http://{address}"));

        assert_eq!(
            provider.refresh_snapshot("2026-07-09T12:00:00Z"),
            Err(UsageProviderError::NotConfigured)
        );
    }

    #[test]
    fn hung_daemon_is_timed_out() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test port binds");
        let address = listener.local_addr().expect("test port address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("hung request accepted");
            let mut request = [0; 1024];
            let _ = stream.read(&mut request);
            thread::sleep(Duration::from_millis(250));
        });
        let provider = OllamaLocalProvider::with_timeout(
            format!("http://{address}"),
            Duration::from_millis(100),
        );

        assert_eq!(
            provider.refresh_snapshot("2026-07-09T12:00:00Z"),
            Err(UsageProviderError::TimedOut)
        );
        server.join().expect("hung server joins");
    }

    #[test]
    fn redirecting_daemon_is_parse_failed_without_following_location() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test port binds");
        let address = listener.local_addr().expect("test port address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("redirect request accepted");
            let mut request = [0; 1024];
            let _ = stream.read(&mut request);
            stream
                .write_all(
                    b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/outbound\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .expect("redirect response written");
        });
        let provider = OllamaLocalProvider::with_base_url(format!("http://{address}"));

        assert_eq!(
            provider.refresh_snapshot("2026-07-09T12:00:00Z"),
            Err(UsageProviderError::ParseFailed)
        );
        server.join().expect("redirect server joins");
    }

    #[test]
    fn live_loopback_daemon_reports_availability_without_quota() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test port binds");
        let address = listener.local_addr().expect("test port address");
        let server = thread::spawn(move || {
            for expected_path in ["/api/tags", "/api/ps", "/api/me"] {
                let (mut stream, _) = listener.accept().expect("request accepted");
                let mut request = [0; 2048];
                let n = stream.read(&mut request).unwrap_or(0);
                let request_text = String::from_utf8_lossy(&request[..n]);
                assert!(
                    request_text.contains(expected_path),
                    "expected path {expected_path} in {request_text}"
                );

                let body = match expected_path {
                    "/api/tags" => TAGS_FIXTURE,
                    "/api/ps" => PS_FIXTURE,
                    _ => ME_FIXTURE,
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .expect("response written");
            }
        });
        let provider = OllamaLocalProvider::with_base_url(format!("http://{address}"));
        let snapshot = provider
            .refresh_snapshot("2026-07-09T12:00:00Z")
            .expect("daemon response parses");

        assert_eq!(snapshot.remaining_percent, None);
        assert_eq!(snapshot.details["modelCount"], 2);
        assert_eq!(snapshot.details["loadedModelCount"], 1);
        assert_eq!(snapshot.details["plan"], "pro");
        assert_eq!(snapshot.details["quotaSupported"], false);
        server.join().expect("server joins");
    }

    #[test]
    fn status_errors_map_to_user_facing_states() {
        assert_eq!(
            map_status_error(reqwest::StatusCode::UNAUTHORIZED),
            UsageProviderError::LoginRequired
        );
        assert_eq!(
            map_status_error(reqwest::StatusCode::FORBIDDEN),
            UsageProviderError::LoginRequired
        );
        for status in [
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            reqwest::StatusCode::SERVICE_UNAVAILABLE,
            reqwest::StatusCode::from_u16(599).expect("valid 5xx status"),
        ] {
            assert_eq!(
                map_status_error(status),
                UsageProviderError::NetworkUnavailable
            );
        }
        assert_eq!(
            map_status_error(reqwest::StatusCode::FOUND),
            UsageProviderError::ParseFailed
        );
    }
}
