//! Provider service-status polling.
//!
//! Both Anthropic (Claude) and OpenAI publish a statuspage.io page exposing a
//! stable `/api/v2/status.json` with a single `status.indicator` field
//! (`none` | `minor` | `major` | `critical`) plus a human description. We poll
//! those, normalize them into an [`Incident`] per provider, and surface the
//! worst active one as a badge on the tray icon and in the menu — mirroring
//! CodexBar's incident overlay.
//!
//! Polling is best-effort and fully decoupled from usage: a status fetch
//! failure never affects usage display, and an unreachable status page is
//! simply treated as "unknown" (no badge), never as an outage.

use crate::usage::types::ProviderId;
use serde::{Deserialize, Serialize};

/// Severity of a provider incident, ordered so the max() is the worst.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// All systems operational.
    None,
    Minor,
    Major,
    Critical,
}

impl Severity {
    /// Map a statuspage.io `indicator` string onto our severity.
    fn from_indicator(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "minor" => Severity::Minor,
            "major" => Severity::Major,
            "critical" => Severity::Critical,
            // "none", "maintenance", and anything unrecognized are non-incidents.
            _ => Severity::None,
        }
    }

    /// True when this severity warrants a visible badge.
    pub fn is_incident(self) -> bool {
        self != Severity::None
    }
}

/// A provider's current service status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Incident {
    pub provider: ProviderId,
    pub severity: Severity,
    /// statuspage.io's human description, e.g. "Partial Outage".
    pub description: String,
}

#[derive(Debug, Deserialize)]
struct StatusResponse {
    status: StatusBlock,
}

#[derive(Debug, Deserialize)]
struct StatusBlock {
    #[serde(default)]
    indicator: String,
    #[serde(default)]
    description: String,
}

/// The statuspage.io status endpoint for each provider, when one exists.
/// Anthropic's `status.anthropic.com` redirects to `status.claude.com`; we
/// point at the canonical host directly to avoid a redirect hop. Providers
/// without a known statuspage return `None` and are simply not polled.
fn status_url(provider: ProviderId) -> Option<&'static str> {
    match provider {
        ProviderId::Claude => Some("https://status.claude.com/api/v2/status.json"),
        ProviderId::Codex => Some("https://status.openai.com/api/v2/status.json"),
        ProviderId::Groq => Some("https://groqstatus.com/api/v2/status.json"),
        // ElevenLabs/Deepgram/z.ai/MiniMax: no public statuspage.io we poll.
        _ => None,
    }
}

/// Fetch and normalize one provider's status. Returns `None` on any failure (or
/// when the provider has no status page) so the caller can leave the previous
/// (or empty) state untouched rather than flashing a spurious badge.
pub async fn fetch_one(http: &reqwest::Client, provider: ProviderId) -> Option<Incident> {
    let url = status_url(provider)?;
    let resp = http.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: StatusResponse = resp.json().await.ok()?;
    Some(Incident {
        provider,
        severity: Severity::from_indicator(&body.status.indicator),
        description: if body.status.description.is_empty() {
            "Unknown".to_string()
        } else {
            body.status.description
        },
    })
}

/// Fetch status for several providers concurrently. Failed fetches are dropped.
/// Spawns one task per provider so all status pages are polled in parallel
/// without pulling in a futures-combinator dependency.
pub async fn fetch_many(http: &reqwest::Client, providers: &[ProviderId]) -> Vec<Incident> {
    let handles: Vec<_> = providers
        .iter()
        .map(|&p| {
            let http = http.clone();
            tokio::spawn(async move { fetch_one(&http, p).await })
        })
        .collect();

    let mut out = Vec::new();
    for h in handles {
        if let Ok(Some(incident)) = h.await {
            out.push(incident);
        }
    }
    out
}

/// The worst active incident across a set, if any. Used to decide the tray badge.
pub fn worst(incidents: &[Incident]) -> Option<&Incident> {
    incidents
        .iter()
        .filter(|i| i.severity.is_incident())
        .max_by_key(|i| i.severity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indicator_mapping() {
        assert_eq!(Severity::from_indicator("none"), Severity::None);
        assert_eq!(Severity::from_indicator("MINOR"), Severity::Minor);
        assert_eq!(Severity::from_indicator("major"), Severity::Major);
        assert_eq!(Severity::from_indicator("critical"), Severity::Critical);
        assert_eq!(Severity::from_indicator("maintenance"), Severity::None);
        assert_eq!(Severity::from_indicator("garbage"), Severity::None);
    }

    #[test]
    fn severity_orders_by_seriousness() {
        assert!(Severity::Critical > Severity::Major);
        assert!(Severity::Major > Severity::Minor);
        assert!(Severity::Minor > Severity::None);
        assert!(!Severity::None.is_incident());
        assert!(Severity::Minor.is_incident());
    }

    #[test]
    fn worst_picks_highest_active() {
        let incidents = vec![
            Incident {
                provider: ProviderId::Claude,
                severity: Severity::Minor,
                description: "Degraded".into(),
            },
            Incident {
                provider: ProviderId::Codex,
                severity: Severity::Major,
                description: "Outage".into(),
            },
        ];
        assert_eq!(worst(&incidents).unwrap().provider, ProviderId::Codex);
    }

    #[test]
    fn worst_ignores_operational() {
        let incidents = vec![Incident {
            provider: ProviderId::Claude,
            severity: Severity::None,
            description: "All Systems Operational".into(),
        }];
        assert!(worst(&incidents).is_none());
    }

    #[test]
    fn parses_statuspage_shape() {
        let json = r#"{"page":{"name":"OpenAI"},"status":{"indicator":"minor","description":"Partial Outage"}}"#;
        let parsed: StatusResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            Severity::from_indicator(&parsed.status.indicator),
            Severity::Minor
        );
        assert_eq!(parsed.status.description, "Partial Outage");
    }
}
