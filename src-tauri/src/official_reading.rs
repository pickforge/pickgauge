//! Per-runtime-service official reading resolution.
//!
//! Codex and Claude each have two official-reading adapters: the CLI
//! (`cli_provider`), which reuses the installed CLI's own OAuth credentials,
//! and managed web (`web_provider` + the Playwright sidecar orchestration in
//! `lib.rs`), an opt-in headless browser. Both report `UsageSource::Web`
//! because a CLI reading *is* the official number, just fetched without a
//! browser.
//!
//! This module owns the policy that picks exactly one official baseline per
//! service from those two adapters: prefer a usable CLI reading, else fall
//! through to opt-in managed web, else surface a sanitized failing official
//! reading. It does not execute either adapter; `usage.rs` runs the CLI
//! adapter and `lib.rs` orchestrates the managed-web adapter, then both
//! consult this module to decide what to do with the result.
use crate::{usage::UsageSnapshot, usage_model::UsageModel};

/// True when a resolved official-reading snapshot represents a healthy,
/// parsed reading rather than a failing one (expired/absent credentials,
/// parse failure, login required, etc). Covers the "healthy CLI" state from
/// product issue #48; every other CLI state below is represented by `false`.
pub(crate) fn is_usable_official_snapshot(snapshot: &UsageSnapshot) -> bool {
    UsageModel::from_snapshot(snapshot).status == "parsed"
}

/// True when a Web-source snapshot came from the CLI adapter rather than the
/// managed-browser adapter. Both report `UsageSource::Web`, so merge needs
/// this to break ties deterministically instead of relying on snapshot
/// insertion order or timestamps that can coincide within one refresh cycle.
///
/// Checks `providerId` rather than the CLI adapter's own `via` detail
/// because `providerId` is set on both successful and failing snapshots
/// (the engine's generic error-snapshot path does not carry `via`), while
/// `via` is only present on a successfully parsed CLI reading.
pub(crate) fn is_cli_official_snapshot(snapshot: &UsageSnapshot) -> bool {
    matches!(
        snapshot
            .details
            .get("providerId")
            .and_then(|value| value.as_str()),
        Some("codex.cli") | Some("claude.cli")
    )
}

/// Whether a runtime service's official reading needs a managed-web attempt
/// this cycle.
///
/// - CLI disabled entirely: managed web is the only official source, so it
///   is needed whenever the user has opted in.
/// - CLI enabled and its latest reading is usable (the "healthy CLI" state):
///   never needed, so a healthy CLI reading is always preferred and never
///   incurs a browser launch.
/// - CLI enabled but its latest reading is missing or unusable (expired or
///   absent credentials, a parse failure, or no reading has been taken yet):
///   needed only when the user has opted in to managed web for that
///   fallback; otherwise the sanitized failing CLI reading is what is shown.
pub(crate) fn managed_web_fallback_needed(
    cli_enabled: bool,
    web_enabled: bool,
    cli_snapshot: Option<&UsageSnapshot>,
) -> bool {
    if !web_enabled {
        return false;
    }

    if !cli_enabled {
        return true;
    }

    !cli_snapshot.is_some_and(is_usable_official_snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::{Service, UsageConfidence, UsageProviderError, UsageSource};

    fn cli_snapshot(status: &str) -> UsageSnapshot {
        UsageSnapshot {
            service: Service::Codex,
            remaining_percent: if status == "parsed" { Some(0.0) } else { None },
            used_percent: if status == "parsed" {
                Some(100.0)
            } else {
                None
            },
            reset_at: None,
            source: UsageSource::Web,
            confidence: if status == "parsed" {
                UsageConfidence::High
            } else {
                UsageConfidence::Unknown
            },
            last_updated: "2026-07-09T12:00:00Z".to_string(),
            details: serde_json::json!({
                "status": status,
                "providerId": "codex.cli",
                "source": "web",
                "via": "cli",
            }),
        }
    }

    fn web_snapshot(status: &str) -> UsageSnapshot {
        UsageSnapshot {
            service: Service::Codex,
            remaining_percent: None,
            used_percent: None,
            reset_at: None,
            source: UsageSource::Web,
            confidence: UsageConfidence::Unknown,
            last_updated: "2026-07-09T12:00:00Z".to_string(),
            details: serde_json::json!({
                "status": status,
                "providerId": "codex.web",
                "source": "web",
            }),
        }
    }

    // Healthy CLI: never needs managed web, whether or not web is opted in.
    #[test]
    fn healthy_cli_never_needs_managed_web() {
        let healthy = cli_snapshot("parsed");
        assert!(is_usable_official_snapshot(&healthy));
        assert!(!managed_web_fallback_needed(true, true, Some(&healthy)));
        assert!(!managed_web_fallback_needed(true, false, Some(&healthy)));
    }

    // Expired/absent CLI creds (not_configured / login_required): falls
    // through to managed web only when opted in.
    #[test]
    fn expired_or_absent_cli_creds_fall_through_only_when_web_is_opted_in() {
        for status in [
            UsageProviderError::NotConfigured.code(),
            UsageProviderError::LoginRequired.code(),
        ] {
            let unusable = cli_snapshot(status);
            assert!(!is_usable_official_snapshot(&unusable));
            assert!(managed_web_fallback_needed(true, true, Some(&unusable)));
            assert!(!managed_web_fallback_needed(true, false, Some(&unusable)));
        }
    }

    // CLI parse failure: same fall-through policy as absent credentials.
    #[test]
    fn cli_parse_failure_falls_through_only_when_web_is_opted_in() {
        let parse_failed = cli_snapshot(UsageProviderError::ParseFailed.code());
        assert!(!is_usable_official_snapshot(&parse_failed));
        assert!(managed_web_fallback_needed(true, true, Some(&parse_failed)));
        assert!(!managed_web_fallback_needed(
            true,
            false,
            Some(&parse_failed)
        ));
    }

    // No CLI reading has been taken yet this cycle: treated the same as an
    // unusable one, still gated by the web opt-in.
    #[test]
    fn absent_cli_snapshot_falls_through_only_when_web_is_opted_in() {
        assert!(managed_web_fallback_needed(true, true, None));
        assert!(!managed_web_fallback_needed(true, false, None));
    }

    // CLI disabled entirely: managed web is the only source, gated by the
    // web opt-in alone.
    #[test]
    fn cli_disabled_uses_managed_web_only_when_opted_in() {
        assert!(managed_web_fallback_needed(false, true, None));
        assert!(!managed_web_fallback_needed(false, false, None));
    }

    // Web login absent is a managed-web *result*, not a resolution input:
    // the resolver still requested the attempt (CLI was unusable, web opted
    // in); the sanitized failing snapshot the attempt returns is what merge
    // then shows, unchanged by this module.
    #[test]
    fn web_login_absent_is_a_managed_web_result_not_a_resolver_input() {
        let unusable_cli = cli_snapshot(UsageProviderError::NotConfigured.code());
        assert!(managed_web_fallback_needed(true, true, Some(&unusable_cli)));

        let login_required_web = web_snapshot(UsageProviderError::LoginRequired.code());
        assert!(!is_usable_official_snapshot(&login_required_web));
        assert!(!is_cli_official_snapshot(&login_required_web));
    }

    #[test]
    fn cli_snapshot_outranks_web_snapshot_in_ties() {
        let cli = cli_snapshot(UsageProviderError::LoginRequired.code());
        let web = web_snapshot(UsageProviderError::LoginRequired.code());

        assert!(is_cli_official_snapshot(&cli));
        assert!(!is_cli_official_snapshot(&web));
    }
}
