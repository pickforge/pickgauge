use crate::{
    browser_session::{
        configure_process_group, BrowserSessionManager, BrowserSessionMarker,
        PlaywrightLaunchRequest, PROFILE_STOP_TIMEOUT,
    },
    usage::Service,
};
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    io::{BufRead, BufReader, Read, Write},
    process::{ChildStdout, Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

pub const PLAYWRIGHT_SIDECAR_NAME: &str = "pickgauge-playwright-sidecar";
pub const PLAYWRIGHT_BACKEND_ID: &str = "playwright-headed-chromium-sidecar";
pub const PLAYWRIGHT_SIDECAR_ACTION_LAUNCH_LOGIN: &str = "launchLogin";
pub const PLAYWRIGHT_SIDECAR_ACTION_REFRESH_USAGE: &str = "refreshUsage";
pub const PLAYWRIGHT_SIDECAR_PROTOCOL_VERSION: u32 = 1;
pub const PLAYWRIGHT_SIDECAR_STATUS_CHECKED: &str = "checked";
pub const PLAYWRIGHT_SIDECAR_STATUS_LAUNCHED: &str = "launched";
// Covers the peer's sequential 30s browser-launch and navigation budgets,
// plus its final 5s network-idle wait.
const PLAYWRIGHT_SIDECAR_RESPONSE_TIMEOUT: Duration = Duration::from_secs(70);
const PLAYWRIGHT_SIDECAR_MAX_RESPONSE_BYTES: u64 = 64 * 1024;

#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightSidecarLaunchRequest {
    pub protocol_version: u32,
    pub action: &'static str,
    pub backend: &'static str,
    pub service: Service,
    pub url: String,
    pub profile_label: String,
    pub user_data_dir: String,
    pub headless: bool,
    pub args: Vec<String>,
    #[serde(skip)]
    pub diagnostics: PlaywrightSidecarLaunchDiagnostics,
}

impl fmt::Debug for PlaywrightSidecarLaunchRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let user_data_dir = format!("<{}>", self.profile_label);

        formatter
            .debug_struct("PlaywrightSidecarLaunchRequest")
            .field("protocol_version", &self.protocol_version)
            .field("action", &self.action)
            .field("backend", &self.backend)
            .field("service", &self.service)
            .field("url", &self.url)
            .field("profile_label", &self.profile_label)
            .field("user_data_dir", &user_data_dir)
            .field("headless", &self.headless)
            .field("arg_count", &self.diagnostics.arg_count)
            .field("diagnostics", &self.diagnostics)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaywrightSidecarLaunchDiagnostics {
    pub protocol_version: u32,
    pub action: &'static str,
    pub backend: &'static str,
    pub service: Service,
    pub profile_label: String,
    pub user_data_dir: String,
    pub headless: bool,
    pub arg_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaywrightSidecarLaunchResponse {
    pub protocol_version: u32,
    pub action: String,
    pub backend: String,
    pub service: Service,
    pub profile_label: String,
    pub headless: bool,
    pub arg_count: usize,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlaywrightSidecarUsageResponse {
    pub protocol_version: u32,
    pub action: String,
    pub backend: String,
    pub service: Service,
    pub profile_label: String,
    pub headless: bool,
    pub arg_count: usize,
    pub status: String,
    pub page_state: String,
    pub remaining_percent: Option<f32>,
    pub used_percent: Option<f32>,
    pub reset_at: Option<String>,
    pub visible_fields: Vec<String>,
    pub weekly: Option<PlaywrightSidecarUsageWindow>,
    pub fable: Option<PlaywrightSidecarUsageWindow>,
    pub products: Vec<PlaywrightSidecarUsageProduct>,
}

/// A secondary rate-limit window carried alongside the headline window.
/// Percentages and the reset timestamp are validated by the web adapter; this
/// protocol type only transports the raw values.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightSidecarUsageWindow {
    pub remaining_percent: Option<f32>,
    pub used_percent: Option<f32>,
    pub reset_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightSidecarUsageProduct {
    pub product: String,
    pub usage_percent: f32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPlaywrightSidecarResponse {
    ok: bool,
    status: String,
    protocol_version: u32,
    action: Option<String>,
    backend: Option<String>,
    service: Option<Service>,
    profile_label: Option<String>,
    headless: Option<bool>,
    arg_count: Option<usize>,
    page_state: Option<String>,
    remaining_percent: Option<f32>,
    used_percent: Option<f32>,
    reset_at: Option<String>,
    visible_fields: Option<Vec<String>>,
    weekly: Option<PlaywrightSidecarUsageWindow>,
    fable: Option<PlaywrightSidecarUsageWindow>,
    products: Option<Vec<PlaywrightSidecarUsageProduct>>,
}

struct ValidatedResponseEcho {
    protocol_version: u32,
    action: String,
    backend: String,
    service: Service,
    profile_label: String,
    headless: bool,
    arg_count: usize,
}

#[derive(Clone, Copy)]
enum SidecarOperation {
    Login,
    Usage,
}

impl SidecarOperation {
    fn unavailable(self) -> &'static str {
        match self {
            Self::Login => "Managed login sidecar is unavailable",
            Self::Usage => "Managed usage sidecar is unavailable",
        }
    }

    fn process_state_unavailable(self) -> &'static str {
        match self {
            Self::Login => "Managed login sidecar process state is unavailable",
            Self::Usage => "Managed usage sidecar process state is unavailable",
        }
    }

    fn serialize_failed(self) -> &'static str {
        match self {
            Self::Login => "Could not serialize managed login sidecar request",
            Self::Usage => "Could not serialize managed usage sidecar request",
        }
    }

    fn write_failed(self) -> &'static str {
        match self {
            Self::Login => "Could not write managed login sidecar request",
            Self::Usage => "Could not write managed usage sidecar request",
        }
    }

    fn read_failed(self) -> &'static str {
        match self {
            Self::Login => "Could not read managed login sidecar response",
            Self::Usage => "Could not read managed usage sidecar response",
        }
    }

    fn response_timeout(self) -> &'static str {
        match self {
            Self::Login => "Managed login sidecar did not acknowledge launch",
            Self::Usage => "Managed usage sidecar did not return refresh results",
        }
    }

    fn cleanup_failed(self) -> &'static str {
        match self {
            Self::Login => "Could not clean up managed login sidecar",
            Self::Usage => "Managed usage sidecar did not finish refresh",
        }
    }
}

pub fn playwright_sidecar_launch_request(
    request: &PlaywrightLaunchRequest,
    url: impl Into<String>,
) -> Result<PlaywrightSidecarLaunchRequest, String> {
    playwright_sidecar_request(
        request,
        url,
        PLAYWRIGHT_SIDECAR_ACTION_LAUNCH_LOGIN,
        false,
        "Managed browser login URL must use HTTPS",
    )
}

pub fn playwright_sidecar_refresh_request(
    request: &PlaywrightLaunchRequest,
    url: impl Into<String>,
) -> Result<PlaywrightSidecarLaunchRequest, String> {
    playwright_sidecar_request(
        request,
        url,
        PLAYWRIGHT_SIDECAR_ACTION_REFRESH_USAGE,
        true,
        "Managed browser refresh URL must use HTTPS",
    )
}

fn playwright_sidecar_request(
    request: &PlaywrightLaunchRequest,
    url: impl Into<String>,
    action: &'static str,
    headless: bool,
    https_error: &str,
) -> Result<PlaywrightSidecarLaunchRequest, String> {
    let url = url.into();
    if !url.starts_with("https://") {
        return Err(https_error.to_string());
    }

    let user_data_dir = request.user_data_dir.to_string_lossy().to_string();
    let diagnostics = PlaywrightSidecarLaunchDiagnostics {
        protocol_version: PLAYWRIGHT_SIDECAR_PROTOCOL_VERSION,
        action,
        backend: request.backend,
        service: request.service,
        profile_label: request.profile_label.clone(),
        user_data_dir: format!("<{}>", request.profile_label),
        headless,
        arg_count: request.args.len(),
    };

    Ok(PlaywrightSidecarLaunchRequest {
        protocol_version: PLAYWRIGHT_SIDECAR_PROTOCOL_VERSION,
        action,
        backend: request.backend,
        service: request.service,
        url,
        profile_label: request.profile_label.clone(),
        user_data_dir,
        headless,
        args: request.args.clone(),
        diagnostics,
    })
}

pub fn launch_login(
    command: Command,
    sessions: &BrowserSessionManager,
    request: &PlaywrightSidecarLaunchRequest,
) -> Result<u32, String> {
    launch_login_with_timeout(
        command,
        sessions,
        request,
        PLAYWRIGHT_SIDECAR_RESPONSE_TIMEOUT,
    )
}

fn launch_login_with_timeout(
    command: Command,
    sessions: &BrowserSessionManager,
    request: &PlaywrightSidecarLaunchRequest,
    timeout: Duration,
) -> Result<u32, String> {
    let (process_id, _) = execute(
        command,
        sessions,
        request,
        SidecarOperation::Login,
        timeout,
        true,
        playwright_sidecar_launch_response,
    )?;
    Ok(process_id)
}

pub fn refresh_usage(
    command: Command,
    sessions: &BrowserSessionManager,
    request: &PlaywrightSidecarLaunchRequest,
) -> Result<PlaywrightSidecarUsageResponse, String> {
    refresh_usage_with_timeout(
        command,
        sessions,
        request,
        PLAYWRIGHT_SIDECAR_RESPONSE_TIMEOUT,
    )
}

fn refresh_usage_with_timeout(
    command: Command,
    sessions: &BrowserSessionManager,
    request: &PlaywrightSidecarLaunchRequest,
    timeout: Duration,
) -> Result<PlaywrightSidecarUsageResponse, String> {
    let (_, response) = execute(
        command,
        sessions,
        request,
        SidecarOperation::Usage,
        timeout,
        false,
        playwright_sidecar_usage_response,
    )?;
    Ok(response)
}

fn execute<T>(
    mut command: Command,
    sessions: &BrowserSessionManager,
    request: &PlaywrightSidecarLaunchRequest,
    operation: SidecarOperation,
    timeout: Duration,
    keep_tracked: bool,
    decode: fn(&str, &PlaywrightSidecarLaunchRequest) -> Result<T, String>,
) -> Result<(u32, T), String> {
    sessions
        .stop_service(request.service, PROFILE_STOP_TIMEOUT)
        .map_err(|_| operation.cleanup_failed().to_string())?;

    let marker = BrowserSessionMarker::new(request.service);
    let (env_key, env_value) = marker.env_pair();
    command.env(env_key, env_value);
    configure_process_group(&mut command);
    command.stderr(Stdio::null());
    let child = command
        .spawn()
        .map_err(|_| operation.unavailable().to_string())?;
    let process_id = sessions
        .track_process(request.service, child, marker.clone())
        .map_err(|_| operation.process_state_unavailable().to_string())?;

    let response = (|| {
        let (mut stdin, stdout) = sessions
            .take_process_stdio(&marker)
            .map_err(|_| operation.process_state_unavailable().to_string())?;
        write_request(&mut stdin, request, operation)?;
        drop(stdin);
        let line = read_response_line(stdout, timeout, operation)?;
        decode(&line, request)
    })();

    let response = match response {
        Ok(response) => response,
        Err(error) => {
            return Err(clean_up_after_failure(sessions, &marker, operation, error));
        }
    };

    if !keep_tracked {
        sessions
            .stop_marked_process(&marker, Duration::ZERO)
            .map_err(|_| operation.cleanup_failed().to_string())?;
    }

    Ok((process_id, response))
}

fn write_request(
    stdin: &mut impl Write,
    request: &PlaywrightSidecarLaunchRequest,
    operation: SidecarOperation,
) -> Result<(), String> {
    let raw = serde_json::to_vec(request).map_err(|_| operation.serialize_failed().to_string())?;
    stdin
        .write_all(&raw)
        .and_then(|_| stdin.write_all(b"\n"))
        .map_err(|_| operation.write_failed().to_string())
}

fn read_response_line(
    stdout: ChildStdout,
    timeout: Duration,
    operation: SidecarOperation,
) -> Result<String, String> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        let mut bytes = Vec::new();
        let result = reader
            .take(PLAYWRIGHT_SIDECAR_MAX_RESPONSE_BYTES + 1)
            .read_until(b'\n', &mut bytes)
            .and_then(|read| {
                if bytes.len() as u64 > PLAYWRIGHT_SIDECAR_MAX_RESPONSE_BYTES {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "sidecar response exceeds limit",
                    ));
                }
                String::from_utf8(bytes)
                    .map(|line| if read == 0 { String::new() } else { line })
                    .map_err(|_| {
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "sidecar response is not UTF-8",
                        )
                    })
            });
        let _ = sender.send(result);
    });

    match receiver.recv_timeout(timeout) {
        Ok(Ok(line)) if !line.trim().is_empty() => Ok(line),
        Ok(Ok(_)) | Err(mpsc::RecvTimeoutError::Timeout) => {
            Err(operation.response_timeout().to_string())
        }
        Ok(Err(_)) | Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err(operation.read_failed().to_string())
        }
    }
}

fn clean_up_after_failure(
    sessions: &BrowserSessionManager,
    marker: &BrowserSessionMarker,
    operation: SidecarOperation,
    error: String,
) -> String {
    match sessions.stop_marked_process(marker, PROFILE_STOP_TIMEOUT) {
        Ok(_) => error,
        Err(_) => operation.cleanup_failed().to_string(),
    }
}

pub fn playwright_sidecar_launch_response(
    raw: &str,
    request: &PlaywrightSidecarLaunchRequest,
) -> Result<PlaywrightSidecarLaunchResponse, String> {
    let response = parse_response(raw, "Managed login sidecar returned invalid response")?;

    if !response.ok {
        return Err("Managed login sidecar rejected launch".to_string());
    }
    if response.status != PLAYWRIGHT_SIDECAR_STATUS_LAUNCHED {
        return Err("Managed login sidecar did not launch browser".to_string());
    }

    let echo = validate_response_echo(
        &response,
        request,
        "Managed login sidecar returned mismatched response",
    )?;

    Ok(PlaywrightSidecarLaunchResponse {
        protocol_version: echo.protocol_version,
        action: echo.action,
        backend: echo.backend,
        service: echo.service,
        profile_label: echo.profile_label,
        headless: echo.headless,
        arg_count: echo.arg_count,
        status: response.status,
    })
}

pub fn playwright_sidecar_usage_response(
    raw: &str,
    request: &PlaywrightSidecarLaunchRequest,
) -> Result<PlaywrightSidecarUsageResponse, String> {
    let response = parse_response(raw, "Managed usage sidecar returned invalid response")?;

    if !response.ok {
        return Err("Managed usage sidecar rejected refresh".to_string());
    }
    if response.status != PLAYWRIGHT_SIDECAR_STATUS_CHECKED {
        return Err("Managed usage sidecar did not check usage".to_string());
    }

    let echo = validate_response_echo(
        &response,
        request,
        "Managed usage sidecar returned mismatched response",
    )?;
    let Some(page_state) = response.page_state else {
        return Err("Managed usage sidecar returned invalid page state".to_string());
    };
    let visible_fields = response.visible_fields.unwrap_or_default();
    let products = response.products.unwrap_or_default();

    if page_state.len() > 32
        || !page_state
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
    {
        return Err("Managed usage sidecar returned invalid page state".to_string());
    }

    if visible_fields.iter().any(|field| {
        field.is_empty()
            || field.len() > 64
            || !field
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
    }) {
        return Err("Managed usage sidecar returned invalid visible fields".to_string());
    }

    if products.len() > 6
        || products.iter().any(|product| {
            !matches!(
                product.product.as_str(),
                "PRODUCT_GROK_CHAT"
                    | "PRODUCT_GROK_BUILD"
                    | "PRODUCT_API"
                    | "PRODUCT_GROK_IMAGINE"
                    | "PRODUCT_GROK_VOICE"
                    | "PRODUCT_GROK_PLUGINS"
            ) || !product.usage_percent.is_finite()
                || !(0.0..=100.0).contains(&product.usage_percent)
        })
    {
        return Err("Managed usage sidecar returned invalid products".to_string());
    }

    Ok(PlaywrightSidecarUsageResponse {
        protocol_version: echo.protocol_version,
        action: echo.action,
        backend: echo.backend,
        service: echo.service,
        profile_label: echo.profile_label,
        headless: echo.headless,
        arg_count: echo.arg_count,
        status: response.status,
        page_state,
        remaining_percent: response.remaining_percent,
        used_percent: response.used_percent,
        reset_at: response.reset_at,
        visible_fields,
        weekly: response.weekly,
        fable: response.fable,
        products,
    })
}

fn parse_response(
    raw: &str,
    invalid_response: &'static str,
) -> Result<RawPlaywrightSidecarResponse, String> {
    serde_json::from_str(raw).map_err(|_| invalid_response.to_string())
}

fn validate_response_echo(
    response: &RawPlaywrightSidecarResponse,
    request: &PlaywrightSidecarLaunchRequest,
    mismatch: &'static str,
) -> Result<ValidatedResponseEcho, String> {
    let (
        Some(action),
        Some(backend),
        Some(service),
        Some(profile_label),
        Some(headless),
        Some(arg_count),
    ) = (
        response.action.clone(),
        response.backend.clone(),
        response.service,
        response.profile_label.clone(),
        response.headless,
        response.arg_count,
    )
    else {
        return Err(mismatch.to_string());
    };

    if response.protocol_version != request.protocol_version
        || action != request.action
        || backend != request.backend
        || service != request.service
        || profile_label != request.profile_label
        || headless != request.headless
        || arg_count != request.args.len()
    {
        return Err(mismatch.to_string());
    }

    Ok(ValidatedResponseEcho {
        protocol_version: response.protocol_version,
        action,
        backend,
        service,
        profile_label,
        headless,
        arg_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser_session::{
        chromium_launch_plan, playwright_launch_request, BrowserSessionStopStatus,
    };
    use std::{io, process::Stdio};

    #[test]
    fn request_write_failures_are_action_specific_and_redacted() {
        let request = test_request(Service::Codex, false);
        let mut writer = FailingWriter;

        let error = write_request(&mut writer, &request, SidecarOperation::Login)
            .expect_err("write failure is mapped");

        assert_eq!(error, "Could not write managed login sidecar request");
        assert!(!error.contains(&request.user_data_dir));
        assert!(!error.contains(&request.url));
    }

    #[cfg(unix)]
    #[test]
    fn tracked_login_keeps_process_owned_after_valid_exchange() {
        let sessions = BrowserSessionManager::default();
        let request = test_request(Service::Codex, false);
        let response = response_line(&request, "launched", None);
        let command = shell_command(&format!(
            "read request; printf '%s\\n' '{}'; sleep 30",
            shell_quote(&response)
        ));

        let process_id =
            launch_login_with_timeout(command, &sessions, &request, Duration::from_secs(1))
                .expect("login exchange succeeds");

        assert!(process_id > 0);
        let stopped = sessions
            .stop_service(Service::Codex, Duration::from_secs(1))
            .expect("tracked login stops");
        assert_ne!(stopped.status, BrowserSessionStopStatus::NoManagedProcess);
    }

    #[cfg(unix)]
    #[test]
    fn one_shot_refresh_closes_stdin_and_cleans_up_after_valid_exchange() {
        let sessions = BrowserSessionManager::default();
        let request = test_request(Service::Claude, true);
        let response = response_line(&request, "checked", Some("usage"));
        let command = shell_command(&format!(
            "request=$(cat); printf '%s\\n' '{}'; sleep 30",
            shell_quote(&response)
        ));

        let result =
            refresh_usage_with_timeout(command, &sessions, &request, Duration::from_secs(1))
                .expect("usage exchange succeeds");

        assert_eq!(result.page_state, "usage");
        assert_eq!(
            sessions
                .stop_service(Service::Claude, Duration::from_millis(1))
                .expect("refresh has no remaining process")
                .status,
            BrowserSessionStopStatus::NoManagedProcess
        );
    }

    #[cfg(unix)]
    #[test]
    fn stderr_backpressure_does_not_block_a_valid_response() {
        let sessions = BrowserSessionManager::default();
        let request = test_request(Service::Claude, true);
        let response = response_line(&request, "checked", Some("usage"));
        let mut command = shell_command(&format!(
            "request=$(cat); head -c 1048576 /dev/zero >&2; printf '%s\\n' '{}'; sleep 30",
            shell_quote(&response)
        ));
        command.stderr(Stdio::piped());

        let result =
            refresh_usage_with_timeout(command, &sessions, &request, Duration::from_secs(1))
                .expect("stderr cannot block the protocol response");

        assert_eq!(result.page_state, "usage");
        assert_no_process(&sessions, Service::Claude);
    }

    #[cfg(unix)]
    #[test]
    fn invalid_response_cleans_up_without_echoing_sidecar_output() {
        let sessions = BrowserSessionManager::default();
        let request = test_request(Service::Codex, true);
        let command =
            shell_command("read request; printf '%s\\n' '{not-json-with-/home/private}'; sleep 30");

        let error =
            refresh_usage_with_timeout(command, &sessions, &request, Duration::from_secs(1))
                .expect_err("invalid response is rejected");

        assert_eq!(error, "Managed usage sidecar returned invalid response");
        assert!(!error.contains("/home/private"));
        assert_no_process(&sessions, Service::Codex);
    }

    #[cfg(unix)]
    #[test]
    fn oversized_response_cleans_up_without_echoing_output() {
        let sessions = BrowserSessionManager::default();
        let request = test_request(Service::Codex, true);
        let command = shell_command(
            "read request; head -c 65537 /dev/zero | tr '\\0' x; printf '\\n'; sleep 30",
        );

        let error =
            refresh_usage_with_timeout(command, &sessions, &request, Duration::from_secs(1))
                .expect_err("oversized response is rejected");

        assert_eq!(error, "Could not read managed usage sidecar response");
        assert_no_process(&sessions, Service::Codex);
    }

    #[cfg(unix)]
    #[test]
    fn response_timeout_cleans_up_tracked_process() {
        let sessions = BrowserSessionManager::default();
        let request = test_request(Service::Claude, false);
        let command = shell_command("read request; sleep 30");

        let error =
            launch_login_with_timeout(command, &sessions, &request, Duration::from_millis(20))
                .expect_err("missing acknowledgement times out");

        assert_eq!(error, "Managed login sidecar did not acknowledge launch");
        assert_no_process(&sessions, Service::Claude);
    }

    #[cfg(unix)]
    #[test]
    fn response_eof_cleans_up_tracked_process() {
        let sessions = BrowserSessionManager::default();
        let request = test_request(Service::Codex, true);
        let command = shell_command("read request; exit 0");

        let error =
            refresh_usage_with_timeout(command, &sessions, &request, Duration::from_secs(1))
                .expect_err("empty response is rejected");

        assert_eq!(
            error,
            "Managed usage sidecar did not return refresh results"
        );
        assert_no_process(&sessions, Service::Codex);
    }

    #[test]
    fn spawn_failure_is_redacted_and_leaves_no_process_owner() {
        let sessions = BrowserSessionManager::default();
        let request = test_request(Service::Claude, true);
        let command = Command::new("/pickgauge/missing/sidecar-with-secret-name");

        let error =
            refresh_usage_with_timeout(command, &sessions, &request, Duration::from_millis(10))
                .expect_err("missing sidecar fails");

        assert_eq!(error, "Managed usage sidecar is unavailable");
        assert!(!error.contains("secret"));
        assert_no_process(&sessions, Service::Claude);
    }

    fn test_request(service: Service, headless: bool) -> PlaywrightSidecarLaunchRequest {
        let profile = match service {
            Service::Codex => "/tmp/pickgauge-private/codex",
            Service::Claude => "/tmp/pickgauge-private/claude",
            Service::Grok | Service::Ollama => unreachable!("test service is managed"),
        };
        let plan = chromium_launch_plan(service, profile);
        let launch = playwright_launch_request(&plan);
        if headless {
            playwright_sidecar_refresh_request(&launch, "https://example.test/usage")
                .expect("refresh request")
        } else {
            playwright_sidecar_launch_request(&launch, "https://example.test/login")
                .expect("login request")
        }
    }

    fn response_line(
        request: &PlaywrightSidecarLaunchRequest,
        status: &str,
        page_state: Option<&str>,
    ) -> String {
        let mut response = serde_json::json!({
            "ok": true,
            "status": status,
            "protocolVersion": request.protocol_version,
            "action": request.action,
            "backend": request.backend,
            "service": request.service,
            "profileLabel": request.profile_label,
            "headless": request.headless,
            "argCount": request.args.len(),
        });
        if let Some(page_state) = page_state {
            response["pageState"] = serde_json::Value::String(page_state.to_string());
        }
        response.to_string()
    }

    #[cfg(unix)]
    fn shell_command(script: &str) -> Command {
        let mut command = Command::new("sh");
        command
            .args(["-c", script])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        command
    }

    #[cfg(unix)]
    fn shell_quote(value: &str) -> String {
        value.replace('\'', "'\\''")
    }

    fn assert_no_process(sessions: &BrowserSessionManager, service: Service) {
        assert_eq!(
            sessions
                .stop_service(service, Duration::from_millis(1))
                .expect("process state is inspectable")
                .status,
            BrowserSessionStopStatus::NoManagedProcess
        );
    }

    struct FailingWriter;

    impl Write for FailingWriter {
        fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "secret"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
}
