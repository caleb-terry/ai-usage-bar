//! Normalize Codex's raw usage response into a unified [`UsageSnapshot`].

use super::client::RawUsage;
use crate::usage::types::{DetailExtras, DisplayMode, ProviderId, UsageSnapshot, WindowUsage};
use chrono::{DateTime, Utc};

fn from_unix(secs: Option<i64>) -> Option<DateTime<Utc>> {
    secs.and_then(|s| DateTime::<Utc>::from_timestamp(s, 0))
}

pub fn normalize(raw: &RawUsage) -> UsageSnapshot {
    let now = Utc::now();
    let plan_label = raw.plan_type.clone().unwrap_or_else(|| "codex".to_string());

    let extras = DetailExtras {
        credit_balance_cents: raw
            .credits
            .as_ref()
            .and_then(|c| c.balance)
            .map(|b| b.floor() as u64),
        code_review_utilization: raw.code_review_rate_limit.as_ref().map(|w| w.used_percent),
        on_demand_resets: raw
            .rate_limit_reset_credits
            .as_ref()
            .and_then(|r| r.available_count),
    };

    // Prefer session rate windows. If absent but a workspace spend cap exists,
    // fall to spend-cap mode.
    let primary_window = raw
        .rate_limit
        .as_ref()
        .and_then(|rl| rl.primary_window.as_ref());

    let mode = if let Some(primary) = primary_window {
        let secondary = raw
            .rate_limit
            .as_ref()
            .and_then(|rl| rl.secondary_window.as_ref())
            .map(|w| WindowUsage::new(w.used_percent, "Week", from_unix(w.reset_at)));
        DisplayMode::Session {
            primary: WindowUsage::new(primary.used_percent, "5h", from_unix(primary.reset_at)),
            secondary,
        }
    } else if let Some(limit) = raw
        .spend_control
        .as_ref()
        .and_then(|s| s.individual_limit.as_ref())
    {
        DisplayMode::SpendCap {
            utilization: limit.used_percent,
            used_cents: limit.used_cents,
            limit_cents: limit.limit_cents,
            reset_at: from_unix(limit.reset_at),
        }
    } else {
        DisplayMode::Session {
            primary: WindowUsage::new(0.0, "5h", None),
            secondary: None,
        }
    };

    UsageSnapshot {
        provider: ProviderId::Codex,
        plan_label,
        mode,
        fetched_at: now,
        stale: false,
        extras,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::codex::client::{
        Credits, IndividualLimit, RateLimit, RateWindow, RawUsage, ResetCredits, SpendControl,
    };

    #[test]
    fn session_mode_with_both_windows_and_extras() {
        let raw = RawUsage {
            plan_type: Some("plus".to_string()),
            rate_limit: Some(RateLimit {
                primary_window: Some(RateWindow {
                    used_percent: 55.0,
                    reset_at: Some(1_780_000_000),
                    limit_window_seconds: Some(18_000),
                }),
                secondary_window: Some(RateWindow {
                    used_percent: 22.0,
                    reset_at: Some(1_781_000_000),
                    limit_window_seconds: Some(604_800),
                }),
            }),
            credits: Some(Credits {
                balance: Some(12.9),
            }),
            rate_limit_reset_credits: Some(ResetCredits {
                available_count: Some(3),
            }),
            ..Default::default()
        };
        let snap = normalize(&raw);
        assert_eq!(snap.plan_label, "plus");
        assert_eq!(snap.extras.credit_balance_cents, Some(12));
        assert_eq!(snap.extras.on_demand_resets, Some(3));
        match snap.mode {
            DisplayMode::Session { primary, secondary } => {
                assert_eq!(primary.utilization, 55.0);
                assert!(primary.reset_at.is_some());
                assert_eq!(secondary.unwrap().utilization, 22.0);
            }
            other => panic!("expected session, got {other:?}"),
        }
    }

    #[test]
    fn spend_cap_when_no_session_windows() {
        let raw = RawUsage {
            plan_type: Some("business".to_string()),
            spend_control: Some(SpendControl {
                individual_limit: Some(IndividualLimit {
                    used_percent: 73.0,
                    used_cents: Some(7300),
                    limit_cents: Some(10000),
                    reset_at: None,
                }),
            }),
            ..Default::default()
        };
        let snap = normalize(&raw);
        match snap.mode {
            DisplayMode::SpendCap {
                utilization,
                used_cents,
                limit_cents,
                ..
            } => {
                assert_eq!(utilization, 73.0);
                assert_eq!(used_cents, Some(7300));
                assert_eq!(limit_cents, Some(10000));
            }
            other => panic!("expected spend cap, got {other:?}"),
        }
    }

    #[test]
    fn empty_response_degrades_to_zero_session() {
        let snap = normalize(&RawUsage::default());
        assert_eq!(snap.plan_label, "codex");
        match snap.mode {
            DisplayMode::Session { primary, secondary } => {
                assert_eq!(primary.utilization, 0.0);
                assert!(secondary.is_none());
            }
            other => panic!("expected session, got {other:?}"),
        }
    }
}
