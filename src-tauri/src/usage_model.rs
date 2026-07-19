//! First-class typed model state for a single service's usage reading.
//!
//! `UsageSnapshot` (see `usage.rs`) carries provider-specific state in an
//! unrestricted `details` JSON bag: independent quota windows (5h/weekly/
//! Fable), the sanitized official status, and the plan label are all
//! user-visible model state, but every consumer that needs them today
//! re-parses the bag by string key with its own null/freshness/headline
//! policy (the headless `usage --json` projection, the persisted snapshot
//! cache, and the frontend).
//!
//! `UsageModel::from_snapshot` is the seam: it accepts a sanitized adapter
//! reading (a merged, per-service `UsageSnapshot`) and produces validated,
//! typed windows/status/plan/headline state exactly once. Every projection
//! -- headless-v1 JSON (`usage_cli`), the persisted snapshot cache
//! (`snapshot_store`), and (in the future) a companion payload for #50 --
//! should build on this model instead of re-reading `details` directly.
//! Internal diagnostics (`mergeStatus`, `webStatus`, backoff bookkeeping,
//! etc.) are deliberately left out of the model: they stay in `details` for
//! desktop-only use and are never required by an external projection.
use crate::usage::UsageSnapshot;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

/// One validated rate-limit window: percentages are finite and within
/// `0..=100`, and a present `reset_at` is a parseable RFC 3339 timestamp.
#[derive(Clone, Debug, PartialEq)]
pub struct UsageWindow {
    pub remaining_percent: Option<f32>,
    pub used_percent: Option<f32>,
    pub reset_at: Option<String>,
}

/// The independent quota windows a service may report. Absence of one
/// window must never suppress another: a service can have a valid weekly
/// window with no five-hour window (or vice versa), and Claude's Fable
/// window is always independent of both.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UsageWindows {
    pub five_hour: Option<UsageWindow>,
    pub week: Option<UsageWindow>,
    pub fable: Option<UsageWindow>,
}

/// Validated, typed model state for one service: its official status, plan,
/// independent quota windows, and the single headline reading (if any) that
/// should drive a service's float/tray/summary gauge.
#[derive(Clone, Debug, PartialEq)]
pub struct UsageModel {
    pub status: String,
    pub plan: Option<String>,
    pub windows: UsageWindows,
    pub headline: Option<UsageWindow>,
}

impl UsageModel {
    /// Builds the typed model from a sanitized adapter/merge reading.
    ///
    /// `windows` holds only what the provider actually reported, validated;
    /// it is never synthesized from the headline (this is what keeps the
    /// headless-v1 JSON contract intact: a flat percentage with no window
    /// data reports `windows.fiveHour: null`, not a fabricated window).
    ///
    /// `headline` is always the snapshot's own top-level
    /// remaining/used/reset fields: upstream adapters and merge have
    /// already decided whether a five-hour, weekly, or no reading at all is
    /// the right headline (a single-window reading such as a Fable-only
    /// snapshot must never become it), and the model trusts that decision
    /// rather than re-deriving it from `windows`.
    pub fn from_snapshot(snapshot: &UsageSnapshot) -> Self {
        let details = &snapshot.details;
        let status = details
            .get("status")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let plan = details
            .get("plan")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);

        let windows_value = details.get("windows");
        let windows = UsageWindows {
            five_hour: windows_value
                .and_then(|windows| windows.get("fiveHour"))
                .and_then(validated_window),
            week: windows_value
                .and_then(|windows| windows.get("week"))
                .and_then(validated_window),
            fable: windows_value
                .and_then(|windows| windows.get("fable"))
                .and_then(validated_window),
        };

        Self {
            status,
            plan,
            windows,
            headline: headline_window(snapshot),
        }
    }
}

fn headline_window(snapshot: &UsageSnapshot) -> Option<UsageWindow> {
    if snapshot.remaining_percent.is_none() && snapshot.used_percent.is_none() {
        return None;
    }

    Some(UsageWindow {
        remaining_percent: snapshot.remaining_percent,
        used_percent: snapshot.used_percent,
        reset_at: snapshot.reset_at.clone(),
    })
}

fn validated_window(value: &serde_json::Value) -> Option<UsageWindow> {
    if value.is_null() {
        return None;
    }

    let remaining_percent = valid_percent(value.get("remainingPercent"));
    let used_percent = valid_percent(value.get("usedPercent"));

    if remaining_percent.is_none() && used_percent.is_none() {
        return None;
    }

    let reset_at = value
        .get("resetAt")
        .and_then(serde_json::Value::as_str)
        .filter(|reset_at| is_valid_rfc3339(reset_at))
        .map(str::to_string);

    Some(UsageWindow {
        remaining_percent,
        used_percent,
        reset_at,
    })
}

fn valid_percent(value: Option<&serde_json::Value>) -> Option<f32> {
    let value = value?.as_f64()? as f32;
    (value.is_finite() && (0.0..=100.0).contains(&value)).then_some(value)
}

fn is_valid_rfc3339(value: &str) -> bool {
    OffsetDateTime::parse(value, &Rfc3339).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::{Service, UsageConfidence, UsageSource};

    fn snapshot(details: serde_json::Value) -> UsageSnapshot {
        UsageSnapshot {
            service: Service::Codex,
            remaining_percent: details
                .get("__remainingPercent")
                .and_then(serde_json::Value::as_f64)
                .map(|value| value as f32),
            used_percent: details
                .get("__usedPercent")
                .and_then(serde_json::Value::as_f64)
                .map(|value| value as f32),
            reset_at: details
                .get("__resetAt")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            source: UsageSource::Web,
            confidence: UsageConfidence::High,
            last_updated: "2026-07-09T12:00:00Z".to_string(),
            details,
        }
    }

    #[test]
    fn primary_window_absence_does_not_suppress_weekly() {
        let snap = snapshot(serde_json::json!({
            "status": "parsed",
            "__remainingPercent": 60.0,
            "__usedPercent": 40.0,
            "windows": {
                "fiveHour": null,
                "week": { "remainingPercent": 60.0, "usedPercent": 40.0, "resetAt": null }
            }
        }));

        let model = UsageModel::from_snapshot(&snap);

        assert!(model.windows.five_hour.is_none());
        assert_eq!(model.windows.week.as_ref().unwrap().remaining_percent, Some(60.0));
        assert_eq!(model.headline.as_ref().unwrap().remaining_percent, Some(60.0));
    }

    #[test]
    fn fable_only_reading_does_not_become_the_headline() {
        let snap = snapshot(serde_json::json!({
            "status": "parsed",
            "windows": {
                "fable": { "remainingPercent": 88.0, "usedPercent": 12.0, "resetAt": null }
            }
        }));

        let model = UsageModel::from_snapshot(&snap);

        assert!(model.headline.is_none());
        assert!(model.windows.five_hour.is_none());
        assert_eq!(model.windows.fable.as_ref().unwrap().remaining_percent, Some(88.0));
    }

    #[test]
    fn zero_percent_windows_are_valid() {
        let snap = snapshot(serde_json::json!({
            "status": "parsed",
            "__remainingPercent": 0.0,
            "__usedPercent": 100.0,
            "windows": {
                "fiveHour": { "remainingPercent": 0.0, "usedPercent": 100.0, "resetAt": null }
            }
        }));

        let model = UsageModel::from_snapshot(&snap);

        assert_eq!(model.windows.five_hour.as_ref().unwrap().remaining_percent, Some(0.0));
        assert_eq!(model.headline.as_ref().unwrap().remaining_percent, Some(0.0));
    }

    #[test]
    fn null_windows_are_unavailable_not_zero() {
        let snap = snapshot(serde_json::json!({ "status": "not_configured" }));

        let model = UsageModel::from_snapshot(&snap);

        assert!(model.windows.five_hour.is_none());
        assert!(model.windows.week.is_none());
        assert!(model.windows.fable.is_none());
        assert!(model.headline.is_none());
    }

    #[test]
    fn plan_only_reading_is_valid_without_a_headline() {
        let snap = snapshot(serde_json::json!({ "status": "parsed", "plan": "Pro" }));

        let model = UsageModel::from_snapshot(&snap);

        assert_eq!(model.plan.as_deref(), Some("Pro"));
        assert!(model.headline.is_none());
    }

    #[test]
    fn invalid_reset_at_is_dropped_but_percentages_remain() {
        let snap = snapshot(serde_json::json!({
            "status": "parsed",
            "windows": {
                "week": { "remainingPercent": 40.0, "usedPercent": 60.0, "resetAt": "not-a-timestamp" }
            }
        }));

        let model = UsageModel::from_snapshot(&snap);
        let week = model.windows.week.expect("week window remains valid");

        assert_eq!(week.remaining_percent, Some(40.0));
        assert!(week.reset_at.is_none());
    }

    #[test]
    fn out_of_range_percent_is_dropped() {
        let snap = snapshot(serde_json::json!({
            "status": "parsed",
            "windows": {
                "week": { "remainingPercent": 140.0, "usedPercent": -5.0, "resetAt": null }
            }
        }));

        let model = UsageModel::from_snapshot(&snap);

        assert!(model.windows.week.is_none());
    }

    #[test]
    fn flat_percent_without_windows_is_the_headline_but_not_a_synthesized_window() {
        let snap = snapshot(serde_json::json!({
            "status": "parsed",
            "__remainingPercent": 72.0,
            "__usedPercent": 28.0
        }));

        let model = UsageModel::from_snapshot(&snap);

        assert_eq!(model.headline.as_ref().unwrap().remaining_percent, Some(72.0));
        assert!(model.windows.five_hour.is_none());
    }
}
