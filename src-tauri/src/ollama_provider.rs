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
struct OllamaVersionResponse {
    version: String,
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
        format!("{}/api/version", self.base_url.trim_end_matches('/'))
    }

    fn refresh_snapshot(&self, now: &str) -> Result<UsageSnapshot, UsageProviderError> {
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .timeout(self.timeout)
            .build()
            .map_err(|_| UsageProviderError::Internal)?;
        let response = client
            .get(self.endpoint())
            .send()
            .map_err(map_request_error)?;
        let status = response.status();
        if !status.is_success() {
            return Err(map_status_error(status));
        }

        let body = response
            .text()
            .map_err(|_| UsageProviderError::ParseFailed)?;
        parse_version_body(&body, now)
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
        status if status.is_server_error() => UsageProviderError::NetworkUnavailable,
        _ => UsageProviderError::ParseFailed,
    }
}

fn parse_version_body(body: &str, now: &str) -> Result<UsageSnapshot, UsageProviderError> {
    let body: OllamaVersionResponse =
        serde_json::from_str(body.trim()).map_err(|_| UsageProviderError::ParseFailed)?;
    let version = body.version.trim();
    if version.is_empty() {
        return Err(UsageProviderError::ParseFailed);
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
            "version": version,
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

    const VERSION_FIXTURE: &str = r#"{"version":"0.12.1"}"#;

    #[test]
    fn parses_local_daemon_status_without_account_or_quota_fields() {
        let snapshot =
            parse_version_body(VERSION_FIXTURE, "2026-07-09T12:00:00Z").expect("version parses");

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
                "version": "0.12.1",
            })
        );
        assert!(snapshot.details.get("plan").is_none());
        assert!(snapshot.details.get("windows").is_none());
    }

    #[test]
    fn missing_or_invalid_version_is_parse_failed() {
        for body in ["", "null", "{}", r#"{"version":""}"#, "{"] {
            assert_eq!(
                parse_version_body(body, "2026-07-09T12:00:00Z"),
                Err(UsageProviderError::ParseFailed)
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
    fn status_errors_map_to_user_facing_states() {
        for status in [
            reqwest::StatusCode::UNAUTHORIZED,
            reqwest::StatusCode::FORBIDDEN,
        ] {
            assert_eq!(map_status_error(status), UsageProviderError::ParseFailed);
        }
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
    #[ignore = "requires a running local Ollama daemon"]
    fn live_daemon_response_parses() {
        let snapshot = OllamaLocalProvider::new()
            .refresh_snapshot("2026-07-09T12:00:00Z")
            .expect("local daemon response parses");

        assert!(snapshot.details["version"]
            .as_str()
            .is_some_and(|version| !version.is_empty()));
        assert!(snapshot.details.get("plan").is_none());
    }
}
