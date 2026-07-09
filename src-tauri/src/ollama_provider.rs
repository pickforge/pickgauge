use std::time::Duration;

use serde::Deserialize;
use serde_json::json;

use crate::usage::{
    Service, UsageConfidence, UsageProvider, UsageProviderError, UsageProviderId, UsageSnapshot,
    UsageSource,
};

const DEFAULT_BASE_URL: &str = "http://localhost:11434";
const HTTP_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone, Debug)]
pub(crate) struct OllamaLocalProvider {
    base_url: String,
    timeout: Duration,
}

#[derive(Deserialize)]
struct OllamaMeResponse {
    plan: Option<String>,
}

impl Default for OllamaLocalProvider {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
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
            base_url: base_url.into(),
            timeout: HTTP_TIMEOUT,
        }
    }

    #[cfg(test)]
    fn with_timeout(base_url: impl Into<String>, timeout: Duration) -> Self {
        Self {
            base_url: base_url.into(),
            timeout,
        }
    }

    fn endpoint(&self) -> String {
        format!("{}/api/me", self.base_url.trim_end_matches('/'))
    }

    fn refresh_snapshot(&self, now: &str) -> Result<UsageSnapshot, UsageProviderError> {
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .timeout(self.timeout)
            .build()
            .map_err(|_| UsageProviderError::Internal)?;
        let response = client
            .post(self.endpoint())
            .send()
            .map_err(map_request_error)?;
        let status = response.status();
        if !status.is_success() {
            return Err(map_status_error(status));
        }

        let body = response
            .text()
            .map_err(|_| UsageProviderError::ParseFailed)?;
        parse_me_body(&body, now)
    }
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

fn parse_me_body(body: &str, now: &str) -> Result<UsageSnapshot, UsageProviderError> {
    let body = body.trim();
    if body.is_empty() {
        return Err(UsageProviderError::LoginRequired);
    }

    let body: Option<OllamaMeResponse> =
        serde_json::from_str(body).map_err(|_| UsageProviderError::ParseFailed)?;
    let plan = body
        .and_then(|body| body.plan)
        .ok_or(UsageProviderError::LoginRequired)?;
    snapshot_from_plan(plan, now)
}

fn snapshot_from_plan(plan: String, now: &str) -> Result<UsageSnapshot, UsageProviderError> {
    let plan = plan.trim();
    if plan.is_empty() {
        return Err(UsageProviderError::LoginRequired);
    }

    Ok(UsageSnapshot {
        service: Service::Ollama,
        remaining_percent: None,
        used_percent: None,
        reset_at: None,
        source: UsageSource::Local,
        confidence: UsageConfidence::Medium,
        last_updated: now.to_string(),
        details: json!({
            "status": "parsed",
            "providerId": UsageProviderId::OllamaLocal.code(),
            "source": UsageSource::Local.code(),
            "via": "daemon",
            "plan": plan,
        }),
    })
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

    const ME_FIXTURE: &str = r#"{
        "id": "<redacted>",
        "email": "<redacted>",
        "name": "<redacted>",
        "avatarurl": "<redacted>",
        "plan": "pro"
    }"#;

    #[test]
    fn parses_plan_only_snapshot_without_identity_fields() {
        let snapshot = parse_me_body(ME_FIXTURE, "2026-07-09T12:00:00Z").expect("plan parses");

        assert_eq!(snapshot.service, Service::Ollama);
        assert_eq!(snapshot.remaining_percent, None);
        assert_eq!(snapshot.used_percent, None);
        assert_eq!(snapshot.reset_at, None);
        assert_eq!(snapshot.source, UsageSource::Local);
        assert_eq!(snapshot.confidence, UsageConfidence::Medium);
        assert_eq!(
            snapshot.details,
            json!({
                "status": "parsed",
                "providerId": "ollama.local",
                "source": "local",
                "via": "daemon",
                "plan": "pro",
            })
        );
    }

    #[test]
    fn missing_plan_requires_login() {
        assert_eq!(
            parse_me_body(r#"{"id":"<redacted>"}"#, "2026-07-09T12:00:00Z"),
            Err(UsageProviderError::LoginRequired)
        );
    }

    #[test]
    fn empty_identity_bodies_require_login() {
        for body in ["", "null", r#"{"plan":null}"#] {
            assert_eq!(
                parse_me_body(body, "2026-07-09T12:00:00Z"),
                Err(UsageProviderError::LoginRequired)
            );
        }
    }

    #[test]
    fn empty_plan_requires_login() {
        assert_eq!(
            snapshot_from_plan("  ".to_string(), "2026-07-09T12:00:00Z"),
            Err(UsageProviderError::LoginRequired)
        );
    }

    #[test]
    fn malformed_identity_body_is_parse_failed() {
        assert_eq!(
            parse_me_body("{", "2026-07-09T12:00:00Z"),
            Err(UsageProviderError::ParseFailed)
        );
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
        assert_eq!(
            map_status_error(reqwest::StatusCode::BAD_REQUEST),
            UsageProviderError::ParseFailed
        );
    }

    #[test]
    #[ignore = "requires a running, signed-in local Ollama daemon"]
    fn live_daemon_response_parses() {
        let snapshot = OllamaLocalProvider::new()
            .refresh_snapshot("2026-07-09T12:00:00Z")
            .expect("local daemon response parses");

        assert!(snapshot.details["plan"].as_str().is_some_and(|plan| !plan.is_empty()));
    }
}
