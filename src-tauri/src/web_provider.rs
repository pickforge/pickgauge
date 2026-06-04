use crate::usage::{
    Service, UsageConfidence, UsageProviderError, UsageProviderId, UsageSnapshot, UsageSource,
};
use serde::Deserialize;
use time::format_description::well_known::Rfc3339;

const CODEX_VISIBLE_FIELDS: &[&str] = &[
    "remaining_percent",
    "used_percent",
    "reset_at",
    "quota_window",
    "plan_label",
];
const CLAUDE_VISIBLE_FIELDS: &[&str] = &[
    "remaining_percent",
    "used_percent",
    "reset_at",
    "quota_window",
    "plan_label",
];

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VisibleUsageInput {
    pub service: Service,
    pub page_state: VisiblePageState,
    pub remaining_percent: Option<f32>,
    pub used_percent: Option<f32>,
    pub reset_at: Option<String>,
    pub visible_fields: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VisiblePageState {
    Usage,
    LoggedOut,
    MfaRequired,
    CaptchaOrBotCheck,
    NetworkUnavailable,
    TimedOut,
    UnexpectedUi,
}

pub fn parse_visible_usage(input: VisibleUsageInput, observed_at: &str) -> UsageSnapshot {
    let provider_id = UsageProviderId::for_service_source(input.service, UsageSource::Web)
        .expect("web provider id exists for service");

    let visible_fields = match sanitized_visible_fields(input.service, &input.visible_fields) {
        Ok(fields) => fields,
        Err(rejected_field_count) => {
            return unknown_web_snapshot(
                input.service,
                provider_id,
                UsageProviderError::ParseFailed,
                observed_at,
                serde_json::json!({
                    "reason": "unsupported_visible_field",
                    "rejectedFieldCount": rejected_field_count,
                }),
            );
        }
    };

    match input.page_state {
        VisiblePageState::Usage => {
            parse_usage_state(input, provider_id, observed_at, visible_fields)
        }
        VisiblePageState::LoggedOut => unknown_web_snapshot(
            input.service,
            provider_id,
            UsageProviderError::LoginRequired,
            observed_at,
            serde_json::json!({ "reason": "logged_out" }),
        ),
        VisiblePageState::MfaRequired => unknown_web_snapshot(
            input.service,
            provider_id,
            UsageProviderError::MfaRequired,
            observed_at,
            serde_json::json!({ "reason": "mfa_required" }),
        ),
        VisiblePageState::CaptchaOrBotCheck => unknown_web_snapshot(
            input.service,
            provider_id,
            UsageProviderError::CaptchaOrBotCheck,
            observed_at,
            serde_json::json!({ "reason": "captcha_or_bot_check" }),
        ),
        VisiblePageState::NetworkUnavailable => unknown_web_snapshot(
            input.service,
            provider_id,
            UsageProviderError::NetworkUnavailable,
            observed_at,
            serde_json::json!({ "reason": "network_unavailable" }),
        ),
        VisiblePageState::TimedOut => unknown_web_snapshot(
            input.service,
            provider_id,
            UsageProviderError::TimedOut,
            observed_at,
            serde_json::json!({ "reason": "timed_out" }),
        ),
        VisiblePageState::UnexpectedUi => unknown_web_snapshot(
            input.service,
            provider_id,
            UsageProviderError::UnexpectedUi,
            observed_at,
            serde_json::json!({ "reason": "unexpected_ui" }),
        ),
    }
}

fn parse_usage_state(
    input: VisibleUsageInput,
    provider_id: UsageProviderId,
    observed_at: &str,
    visible_fields: Vec<String>,
) -> UsageSnapshot {
    if has_invalid_percent(input.remaining_percent)
        || has_invalid_percent(input.used_percent)
        || has_inconsistent_percentages(input.remaining_percent, input.used_percent)
    {
        return unknown_web_snapshot(
            input.service,
            provider_id,
            UsageProviderError::ParseFailed,
            observed_at,
            serde_json::json!({
                "reason": "invalid_visible_percentage",
                "visibleFields": visible_fields,
            }),
        );
    }

    let Some((remaining_percent, used_percent)) =
        visible_percentages(input.remaining_percent, input.used_percent)
    else {
        return unknown_web_snapshot(
            input.service,
            provider_id,
            UsageProviderError::MissingData,
            observed_at,
            serde_json::json!({
                "reason": "missing_visible_percentage",
                "visibleFields": visible_fields,
            }),
        );
    };

    if let Some(reset_at) = &input.reset_at {
        if time::OffsetDateTime::parse(reset_at, &Rfc3339).is_err() {
            return unknown_web_snapshot(
                input.service,
                provider_id,
                UsageProviderError::ParseFailed,
                observed_at,
                serde_json::json!({
                    "reason": "invalid_reset_at",
                    "visibleFields": visible_fields,
                }),
            );
        }
    }

    UsageSnapshot {
        service: input.service,
        remaining_percent: Some(remaining_percent),
        used_percent: Some(used_percent),
        reset_at: input.reset_at,
        source: UsageSource::Web,
        confidence: UsageConfidence::High,
        last_updated: observed_at.to_string(),
        details: serde_json::json!({
            "status": "parsed",
            "providerId": provider_id.code(),
            "source": UsageSource::Web.code(),
            "lastOfficialCheckAt": observed_at,
            "visibleFields": visible_fields,
        }),
    }
}

fn visible_percentages(
    remaining_percent: Option<f32>,
    used_percent: Option<f32>,
) -> Option<(f32, f32)> {
    let remaining_percent = remaining_percent.filter(|value| valid_percent(*value));
    let used_percent = used_percent.filter(|value| valid_percent(*value));

    match (remaining_percent, used_percent) {
        (Some(remaining), Some(used)) if ((100.0 - used) - remaining).abs() <= 1.0 => {
            Some((remaining, used))
        }
        (Some(_), Some(_)) => None,
        (Some(remaining), None) => Some((remaining, 100.0 - remaining)),
        (None, Some(used)) => Some((100.0 - used, used)),
        (None, None) => None,
    }
}

fn valid_percent(value: f32) -> bool {
    value.is_finite() && (0.0..=100.0).contains(&value)
}

fn has_invalid_percent(value: Option<f32>) -> bool {
    matches!(value, Some(value) if !valid_percent(value))
}

fn has_inconsistent_percentages(remaining_percent: Option<f32>, used_percent: Option<f32>) -> bool {
    matches!(
        (remaining_percent, used_percent),
        (Some(remaining), Some(used))
            if valid_percent(remaining)
                && valid_percent(used)
                && ((100.0 - used) - remaining).abs() > 1.0
    )
}

fn sanitized_visible_fields(
    service: Service,
    visible_fields: &[String],
) -> Result<Vec<String>, usize> {
    let allowed = match service {
        Service::Codex => CODEX_VISIBLE_FIELDS,
        Service::Claude => CLAUDE_VISIBLE_FIELDS,
    };
    let rejected_field_count = visible_fields
        .iter()
        .filter(|field| !allowed.contains(&field.as_str()))
        .count();

    if rejected_field_count > 0 {
        return Err(rejected_field_count);
    }

    Ok(visible_fields.to_vec())
}

fn unknown_web_snapshot(
    service: Service,
    provider_id: UsageProviderId,
    error: UsageProviderError,
    observed_at: &str,
    extra_details: serde_json::Value,
) -> UsageSnapshot {
    let mut details = serde_json::json!({
        "status": error.code(),
        "providerId": provider_id.code(),
        "source": UsageSource::Web.code(),
        "lastOfficialCheckAt": observed_at,
    });

    if let (Some(details), Some(extra_details)) =
        (details.as_object_mut(), extra_details.as_object())
    {
        for (key, value) in extra_details {
            details.insert(key.clone(), value.clone());
        }
    }

    UsageSnapshot {
        service,
        remaining_percent: None,
        used_percent: None,
        reset_at: None,
        source: UsageSource::Web,
        confidence: UsageConfidence::Unknown,
        last_updated: observed_at.to_string(),
        details,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::Path};

    const OBSERVED_AT: &str = "2026-06-04T00:00:00Z";

    fn fixture(name: &str) -> VisibleUsageInput {
        let raw = match name {
            "codex-success" => include_str!("../tests/fixtures/web-visible/codex-success.json"),
            "claude-success" => include_str!("../tests/fixtures/web-visible/claude-success.json"),
            "partial-visible" => {
                include_str!("../tests/fixtures/web-visible/partial-visible.json")
            }
            "logged-out" => include_str!("../tests/fixtures/web-visible/logged-out.json"),
            "mfa-required" => include_str!("../tests/fixtures/web-visible/mfa-required.json"),
            "captcha" => include_str!("../tests/fixtures/web-visible/captcha.json"),
            "network-unavailable" => {
                include_str!("../tests/fixtures/web-visible/network-unavailable.json")
            }
            "timed-out" => include_str!("../tests/fixtures/web-visible/timed-out.json"),
            "unexpected-ui" => include_str!("../tests/fixtures/web-visible/unexpected-ui.json"),
            "parse-failure" => include_str!("../tests/fixtures/web-visible/parse-failure.json"),
            "unsanitized-field" => {
                include_str!("../tests/fixtures/web-visible/unsanitized-field.json")
            }
            _ => panic!("unknown fixture"),
        };

        serde_json::from_str(raw).expect("fixture parses")
    }

    #[test]
    fn codex_visible_usage_fixture_parses_successfully() {
        let snapshot = parse_visible_usage(fixture("codex-success"), OBSERVED_AT);

        assert_eq!(snapshot.service, Service::Codex);
        assert_eq!(snapshot.source, UsageSource::Web);
        assert_eq!(snapshot.confidence, UsageConfidence::High);
        assert_eq!(snapshot.remaining_percent, Some(82.0));
        assert_eq!(snapshot.used_percent, Some(18.0));
        assert_eq!(snapshot.details["status"], "parsed");
        assert_eq!(snapshot.details["providerId"], "codex.web");
        assert_eq!(snapshot.details["lastOfficialCheckAt"], OBSERVED_AT);
    }

    #[test]
    fn claude_visible_usage_fixture_parses_successfully() {
        let snapshot = parse_visible_usage(fixture("claude-success"), OBSERVED_AT);

        assert_eq!(snapshot.service, Service::Claude);
        assert_eq!(snapshot.source, UsageSource::Web);
        assert_eq!(snapshot.confidence, UsageConfidence::High);
        assert_eq!(snapshot.remaining_percent, Some(63.0));
        assert_eq!(snapshot.used_percent, Some(37.0));
        assert_eq!(snapshot.details["status"], "parsed");
        assert_eq!(snapshot.details["providerId"], "claude.web");
    }

    #[test]
    fn partial_visible_data_returns_unknown_without_inventing_percentages() {
        let snapshot = parse_visible_usage(fixture("partial-visible"), OBSERVED_AT);

        assert_eq!(snapshot.confidence, UsageConfidence::Unknown);
        assert_eq!(snapshot.remaining_percent, None);
        assert_eq!(snapshot.used_percent, None);
        assert_eq!(snapshot.details["status"], "missing_data");
        assert_eq!(snapshot.details["reason"], "missing_visible_percentage");
    }

    #[test]
    fn logged_out_page_returns_login_required_snapshot() {
        let snapshot = parse_visible_usage(fixture("logged-out"), OBSERVED_AT);

        assert_eq!(snapshot.confidence, UsageConfidence::Unknown);
        assert_eq!(snapshot.details["status"], "login_required");
        assert_eq!(snapshot.details["reason"], "logged_out");
    }

    #[test]
    fn mfa_required_page_returns_mfa_required_snapshot() {
        let snapshot = parse_visible_usage(fixture("mfa-required"), OBSERVED_AT);

        assert_eq!(snapshot.confidence, UsageConfidence::Unknown);
        assert_eq!(snapshot.details["status"], "mfa_required");
    }

    #[test]
    fn captcha_page_returns_captcha_snapshot() {
        let snapshot = parse_visible_usage(fixture("captcha"), OBSERVED_AT);

        assert_eq!(snapshot.confidence, UsageConfidence::Unknown);
        assert_eq!(snapshot.details["status"], "captcha_or_bot_check");
    }

    #[test]
    fn network_unavailable_returns_network_unavailable_snapshot() {
        let snapshot = parse_visible_usage(fixture("network-unavailable"), OBSERVED_AT);

        assert_eq!(snapshot.confidence, UsageConfidence::Unknown);
        assert_eq!(snapshot.details["status"], "network_unavailable");
        assert_eq!(snapshot.details["reason"], "network_unavailable");
    }

    #[test]
    fn timed_out_returns_timed_out_snapshot() {
        let snapshot = parse_visible_usage(fixture("timed-out"), OBSERVED_AT);

        assert_eq!(snapshot.confidence, UsageConfidence::Unknown);
        assert_eq!(snapshot.details["status"], "timed_out");
        assert_eq!(snapshot.details["reason"], "timed_out");
    }

    #[test]
    fn unexpected_ui_returns_unexpected_ui_snapshot() {
        let snapshot = parse_visible_usage(fixture("unexpected-ui"), OBSERVED_AT);

        assert_eq!(snapshot.confidence, UsageConfidence::Unknown);
        assert_eq!(snapshot.details["status"], "unexpected_ui");
    }

    #[test]
    fn inconsistent_visible_percentages_return_parse_failed_snapshot() {
        let snapshot = parse_visible_usage(fixture("parse-failure"), OBSERVED_AT);

        assert_eq!(snapshot.confidence, UsageConfidence::Unknown);
        assert_eq!(snapshot.remaining_percent, None);
        assert_eq!(snapshot.details["status"], "parse_failed");
        assert_eq!(snapshot.details["reason"], "invalid_visible_percentage");
    }

    #[test]
    fn unsupported_visible_field_names_are_rejected_without_echoing_them() {
        let snapshot = parse_visible_usage(fixture("unsanitized-field"), OBSERVED_AT);

        assert_eq!(snapshot.confidence, UsageConfidence::Unknown);
        assert_eq!(snapshot.details["status"], "parse_failed");
        assert_eq!(snapshot.details["reason"], "unsupported_visible_field");
        assert_eq!(snapshot.details["rejectedFieldCount"], 1);
        assert!(snapshot.details.get("visibleFields").is_none());
    }

    #[test]
    fn committed_web_visible_fixtures_are_sanitized() {
        let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/web-visible");
        let forbidden = [
            "<html",
            "<body",
            "cookie",
            "token",
            "authorization",
            "bearer",
            "email",
            "@example",
            "account",
            "session_id",
            "password",
            "/home/",
            "/users/",
            "localstorage",
            "indexeddb",
        ];

        for entry in fs::read_dir(fixture_dir).expect("fixture dir exists") {
            let entry = entry.expect("fixture entry is readable");
            let path = entry.path();

            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }

            let raw = fs::read_to_string(&path).expect("fixture reads");
            let lowercase = raw.to_lowercase();
            for marker in forbidden {
                assert!(
                    !lowercase.contains(marker),
                    "fixture {} contains forbidden marker {marker}",
                    path.display()
                );
            }
        }
    }
}
