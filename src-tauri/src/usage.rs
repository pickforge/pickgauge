use crate::config::AppConfig;
use serde::Serialize;

#[derive(Clone, Copy)]
pub enum Service {
    Codex,
    Claude,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub service: &'static str,
    pub remaining_percent: Option<f32>,
    pub used_percent: Option<f32>,
    pub reset_at: Option<String>,
    pub source: &'static str,
    pub confidence: &'static str,
    pub last_updated: String,
    pub details: serde_json::Value,
}

impl Service {
    pub fn code(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Claude => "Claude Code",
        }
    }
}

pub fn current_snapshots(config: &AppConfig) -> Vec<UsageSnapshot> {
    let mut snapshots = Vec::new();

    if config.enabled_services.codex {
        snapshots.push(fake_snapshot(Service::Codex, 72.0));
    }

    if config.enabled_services.claude {
        snapshots.push(fake_snapshot(Service::Claude, 41.0));
    }

    snapshots
}

fn fake_snapshot(service: Service, remaining_percent: f32) -> UsageSnapshot {
    UsageSnapshot {
        service: service.code(),
        remaining_percent: Some(remaining_percent),
        used_percent: Some(100.0 - remaining_percent),
        reset_at: None,
        source: "fake",
        confidence: "unknown",
        last_updated: "Waiting for local provider".to_string(),
        details: serde_json::json!({
            "status": "placeholder",
            "provider": "fake",
        }),
    }
}
