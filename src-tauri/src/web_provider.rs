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
const OLLAMA_VISIBLE_FIELDS: &[&str] = &[
    "remaining_percent",
    "used_percent",
    "reset_at",
    "quota_window",
    "plan_label",
];
const GROK_VISIBLE_FIELDS: &[&str] = &[
    "remaining_percent",
    "used_percent",
    "reset_at",
    "quota_window",
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
    #[serde(default)]
    pub second_window: Option<VisibleWindowInput>,
    #[serde(default)]
    pub fable_window: Option<VisibleWindowInput>,
    #[serde(default)]
    pub products: Vec<VisibleProductInput>,
}

/// A secondary rate-limit window. When present and valid it is rendered as the
/// second bar on the dashboard card, with the headline staying on the primary
/// window. Providers that expose a single window simply leave this `None`.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VisibleWindowInput {
    pub remaining_percent: Option<f32>,
    pub used_percent: Option<f32>,
    pub reset_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VisibleProductInput {
    pub product: String,
    pub usage_percent: f32,
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

    let windows = if input.service == Service::Grok {
        Some(serde_json::json!({
            "week": window_json(remaining_percent, used_percent, input.reset_at.as_deref()),
        }))
    } else {
        build_windows(
            remaining_percent,
            used_percent,
            input.reset_at.as_deref(),
            input.second_window.as_ref(),
            if input.service == Service::Claude {
                input.fable_window.as_ref()
            } else {
                None
            },
        )
    };

    let mut details = serde_json::json!({
        "status": "parsed",
        "providerId": provider_id.code(),
        "source": UsageSource::Web.code(),
        "lastOfficialCheckAt": observed_at,
        "visibleFields": visible_fields,
    });
    if let (Some(windows), Some(object)) = (windows, details.as_object_mut()) {
        object.insert("windows".into(), windows);
    }
    if input.service == Service::Grok {
        let products = match sanitized_grok_products(&input.products) {
            Some(products) => products,
            None => {
                return unknown_web_snapshot(
                    input.service,
                    provider_id,
                    UsageProviderError::ParseFailed,
                    observed_at,
                    serde_json::json!({ "reason": "invalid_products" }),
                );
            }
        };
        if let Some(object) = details.as_object_mut() {
            object.insert("products".into(), serde_json::Value::Array(products));
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
        details,
    }
}

/// Build the detail block when a valid optional window is present. The headline
/// is mirrored as `fiveHour`; the standard secondary becomes `week`, while
/// Claude's distinct Fable allowance becomes `fable`.
fn build_windows(
    primary_remaining: f32,
    primary_used: f32,
    primary_reset_at: Option<&str>,
    second: Option<&VisibleWindowInput>,
    fable: Option<&VisibleWindowInput>,
) -> Option<serde_json::Value> {
    let week = second.and_then(|window| {
        window_detail(
            window.remaining_percent,
            window.used_percent,
            window.reset_at.as_deref(),
        )
    });
    let fable = fable.and_then(|window| {
        window_detail(
            window.remaining_percent,
            window.used_percent,
            window.reset_at.as_deref(),
        )
    });

    if week.is_none() && fable.is_none() {
        return None;
    }

    let mut windows = serde_json::json!({
        "fiveHour": window_json(primary_remaining, primary_used, primary_reset_at),
    });
    let object = windows.as_object_mut().expect("window details are an object");
    if let Some(week) = week {
        object.insert("week".into(), week);
    }
    if let Some(fable) = fable {
        object.insert("fable".into(), fable);
    }
    Some(windows)
}

fn window_json(remaining: f32, used: f32, reset_at: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "remainingPercent": remaining,
        "usedPercent": used,
        "resetAt": reset_at,
    })
}

/// Validate and shape a secondary window with the same rules as the headline.
/// Returns `None` (dropping the window) when the percentages are missing,
/// inconsistent, or the reset timestamp is not RFC 3339, so a malformed weekly
/// meter degrades to a single bar rather than failing the whole snapshot.
fn window_detail(
    remaining_percent: Option<f32>,
    used_percent: Option<f32>,
    reset_at: Option<&str>,
) -> Option<serde_json::Value> {
    if has_invalid_percent(remaining_percent)
        || has_invalid_percent(used_percent)
        || has_inconsistent_percentages(remaining_percent, used_percent)
    {
        return None;
    }

    let (remaining, used) = visible_percentages(remaining_percent, used_percent)?;

    if let Some(reset) = reset_at {
        if time::OffsetDateTime::parse(reset, &Rfc3339).is_err() {
            return None;
        }
    }

    Some(window_json(remaining, used, reset_at))
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
        Service::Grok => GROK_VISIBLE_FIELDS,
        Service::Ollama => OLLAMA_VISIBLE_FIELDS,
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

fn sanitized_grok_products(products: &[VisibleProductInput]) -> Option<Vec<serde_json::Value>> {
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
        return None;
    }

    Some(
        products
            .iter()
            .map(|product| {
                serde_json::json!({
                    "product": product.product,
                    "usagePercent": product.usage_percent,
                })
            })
            .collect(),
    )
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
            "grok-success" => include_str!("../tests/fixtures/web-visible/grok-success.json"),
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
    fn grok_visible_usage_fixture_is_weekly_only_with_sanitized_products() {
        let snapshot = parse_visible_usage(fixture("grok-success"), OBSERVED_AT);

        assert_eq!(snapshot.service, Service::Grok);
        assert_eq!(snapshot.source, UsageSource::Web);
        assert_eq!(snapshot.confidence, UsageConfidence::High);
        assert_eq!(snapshot.remaining_percent, Some(71.5));
        assert_eq!(snapshot.used_percent, Some(28.5));
        assert_eq!(snapshot.reset_at.as_deref(), Some("2026-07-16T00:00:00Z"));
        assert_eq!(snapshot.details["providerId"], "grok.web");
        assert!(snapshot.details["windows"].get("fiveHour").is_none());
        assert_eq!(snapshot.details["windows"]["week"]["remainingPercent"], 71.5);
        assert_eq!(snapshot.details["products"][0]["product"], "PRODUCT_GROK_BUILD");
        assert_eq!(snapshot.details["products"][0]["usagePercent"], 42.0);
    }

    #[test]
    fn secondary_window_is_carried_as_dual_windows_with_session_headline() {
        let input = VisibleUsageInput {
            service: Service::Ollama,
            page_state: VisiblePageState::Usage,
            remaining_percent: Some(83.0),
            used_percent: Some(17.0),
            reset_at: Some("2026-06-20T19:00:00Z".to_string()),
            visible_fields: vec![
                "used_percent".to_string(),
                "remaining_percent".to_string(),
                "reset_at".to_string(),
                "quota_window".to_string(),
            ],
            second_window: Some(VisibleWindowInput {
                remaining_percent: Some(43.0),
                used_percent: Some(57.0),
                reset_at: Some("2026-06-22T00:00:00Z".to_string()),
            }),
            fable_window: None,
            products: Vec::new(),
        };

        let snapshot = parse_visible_usage(input, OBSERVED_AT);

        assert_eq!(snapshot.confidence, UsageConfidence::High);
        // The headline stays on the session window so the float and tray are unchanged.
        assert_eq!(snapshot.remaining_percent, Some(83.0));
        assert_eq!(snapshot.reset_at.as_deref(), Some("2026-06-20T19:00:00Z"));

        let windows = &snapshot.details["windows"];
        assert_eq!(windows["fiveHour"]["remainingPercent"], 83.0);
        assert_eq!(windows["fiveHour"]["resetAt"], "2026-06-20T19:00:00Z");
        assert_eq!(windows["week"]["remainingPercent"], 43.0);
        assert_eq!(windows["week"]["usedPercent"], 57.0);
        assert_eq!(windows["week"]["resetAt"], "2026-06-22T00:00:00Z");
    }

    #[test]
    fn invalid_secondary_window_degrades_to_single_window() {
        let input = VisibleUsageInput {
            service: Service::Ollama,
            page_state: VisiblePageState::Usage,
            remaining_percent: Some(83.0),
            used_percent: Some(17.0),
            reset_at: Some("2026-06-20T19:00:00Z".to_string()),
            visible_fields: vec!["used_percent".to_string(), "remaining_percent".to_string()],
            second_window: Some(VisibleWindowInput {
                remaining_percent: Some(10.0),
                used_percent: Some(80.0),
                reset_at: None,
            }),
            fable_window: None,
            products: Vec::new(),
        };

        let snapshot = parse_visible_usage(input, OBSERVED_AT);

        // The malformed weekly window is dropped rather than failing the snapshot.
        assert_eq!(snapshot.confidence, UsageConfidence::High);
        assert_eq!(snapshot.remaining_percent, Some(83.0));
        assert!(snapshot.details.get("windows").is_none());
    }

    #[test]
    fn claude_fable_window_is_carried_without_replacing_the_weekly_window() {
        let input = VisibleUsageInput {
            service: Service::Claude,
            page_state: VisiblePageState::Usage,
            remaining_percent: Some(82.0),
            used_percent: Some(18.0),
            reset_at: None,
            visible_fields: vec!["remaining_percent".to_string(), "used_percent".to_string()],
            second_window: Some(VisibleWindowInput {
                remaining_percent: Some(57.0),
                used_percent: Some(43.0),
                reset_at: None,
            }),
            fable_window: Some(VisibleWindowInput {
                remaining_percent: Some(88.0),
                used_percent: Some(12.0),
                reset_at: None,
            }),
            products: Vec::new(),
        };

        let snapshot = parse_visible_usage(input, OBSERVED_AT);
        let windows = &snapshot.details["windows"];

        assert_eq!(windows["fiveHour"]["remainingPercent"], 82.0);
        assert_eq!(windows["week"]["usedPercent"], 43.0);
        assert_eq!(windows["fable"]["remainingPercent"], 88.0);
    }

    #[test]
    fn invalid_claude_fable_window_is_dropped_without_losing_other_windows() {
        let input = VisibleUsageInput {
            service: Service::Claude,
            page_state: VisiblePageState::Usage,
            remaining_percent: Some(82.0),
            used_percent: Some(18.0),
            reset_at: None,
            visible_fields: vec!["remaining_percent".to_string(), "used_percent".to_string()],
            second_window: Some(VisibleWindowInput {
                remaining_percent: Some(57.0),
                used_percent: Some(43.0),
                reset_at: None,
            }),
            fable_window: Some(VisibleWindowInput {
                remaining_percent: Some(88.0),
                used_percent: Some(80.0),
                reset_at: None,
            }),
            products: Vec::new(),
        };

        let snapshot = parse_visible_usage(input, OBSERVED_AT);
        let windows = &snapshot.details["windows"];

        assert_eq!(windows["week"]["remainingPercent"], 57.0);
        assert!(windows.get("fable").is_none());
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
