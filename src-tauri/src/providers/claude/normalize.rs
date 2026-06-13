//! Normalize Claude's raw usage response into a unified [`UsageSnapshot`].

use super::client::RawUsage;
use crate::usage::types::{DisplayMode, ProviderId, UsageSnapshot, WindowUsage};
use chrono::{DateTime, Utc};

fn parse_iso(s: &Option<String>) -> Option<DateTime<Utc>> {
    s.as_ref()
        .and_then(|v| DateTime::parse_from_rfc3339(v).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

pub fn normalize(raw: &RawUsage, subscription_type: &str) -> UsageSnapshot {
    let now = Utc::now();
    let plan_label = if subscription_type.is_empty() {
        "claude".to_string()
    } else {
        subscription_type.to_string()
    };

    // Prefer session windows. Classification: if a 5-hour window is present, we
    // are in Session mode. Otherwise, if spend (extra_usage) data is present,
    // fall to SpendCap. Otherwise treat as unauthenticated-shaped (shouldn't
    // happen on a valid fetch, but degrade gracefully).
    let mode = if let Some(five) = &raw.five_hour {
        let primary = WindowUsage::new(five.utilization, "5h", parse_iso(&five.resets_at));
        let secondary = raw
            .seven_day
            .as_ref()
            .map(|w| WindowUsage::new(w.utilization, "Week", parse_iso(&w.resets_at)));
        DisplayMode::Session { primary, secondary }
    } else if let Some(extra) = &raw.extra_usage {
        DisplayMode::SpendCap {
            utilization: extra.utilization.unwrap_or_else(|| {
                match (extra.used_credits, extra.monthly_limit) {
                    (Some(used), Some(limit)) if limit > 0.0 => {
                        (used as f32 / limit as f32) * 100.0
                    }
                    _ => 0.0,
                }
            }),
            used_cents: extra.used_credits.map(|v| v.round() as u64),
            limit_cents: extra.monthly_limit.map(|v| v.round() as u64),
            reset_at: parse_iso(&extra.resets_at),
        }
    } else {
        // No windows and no spend data — present an empty session.
        DisplayMode::Session {
            primary: WindowUsage::new(0.0, "5h", None),
            secondary: None,
        }
    };

    UsageSnapshot {
        provider: ProviderId::Claude,
        plan_label,
        mode,
        fetched_at: now,
        stale: false,
        extras: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::claude::client::{RawExtraUsage, RawUsage, RawWindow};

    #[test]
    fn session_mode_from_both_windows() {
        let raw = RawUsage {
            five_hour: Some(RawWindow {
                utilization: 42.0,
                resets_at: Some("2026-06-12T20:00:00Z".to_string()),
            }),
            seven_day: Some(RawWindow {
                utilization: 18.0,
                resets_at: Some("2026-06-19T00:00:00Z".to_string()),
            }),
            extra_usage: None,
        };
        let snap = normalize(&raw, "max");
        assert_eq!(snap.plan_label, "max");
        match snap.mode {
            DisplayMode::Session { primary, secondary } => {
                assert_eq!(primary.utilization, 42.0);
                assert!(primary.reset_at.is_some());
                assert_eq!(secondary.unwrap().utilization, 18.0);
            }
            other => panic!("expected session, got {other:?}"),
        }
    }

    #[test]
    fn spend_cap_mode_when_no_session_windows() {
        let raw = RawUsage {
            five_hour: None,
            seven_day: None,
            extra_usage: Some(RawExtraUsage {
                utilization: None,
                used_credits: Some(400.0),
                monthly_limit: Some(1000.0),
                resets_at: None,
            }),
        };
        let snap = normalize(&raw, "");
        assert_eq!(snap.plan_label, "claude");
        match snap.mode {
            DisplayMode::SpendCap {
                utilization,
                used_cents,
                limit_cents,
                ..
            } => {
                assert_eq!(utilization, 40.0);
                assert_eq!(used_cents, Some(400));
                assert_eq!(limit_cents, Some(1000));
            }
            other => panic!("expected spend cap, got {other:?}"),
        }
    }

    #[test]
    fn five_hour_only_still_session() {
        let raw = RawUsage {
            five_hour: Some(RawWindow {
                utilization: 5.0,
                resets_at: None,
            }),
            seven_day: None,
            extra_usage: None,
        };
        let snap = normalize(&raw, "pro");
        match snap.mode {
            DisplayMode::Session { secondary, .. } => assert!(secondary.is_none()),
            other => panic!("expected session, got {other:?}"),
        }
    }
}
