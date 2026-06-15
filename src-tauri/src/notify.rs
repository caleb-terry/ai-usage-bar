//! Quota notifications.
//!
//! After each poll the loop calls [`NotifyState::evaluate`] with the freshly
//! cached snapshots; it compares them against the last observed state and
//! returns the notifications that should fire *this* tick. Edge-triggered: a
//! notification is emitted only on the transition, never repeatedly while a
//! provider sits in the same band, so we never spam the user.

use crate::settings::{Settings, Thresholds};
use crate::usage::types::{DisplayMode, ProviderId, UsageSnapshot};
use std::collections::HashMap;

/// Severity band a utilization falls into, derived from the user's thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Band {
    Ok,
    Warn,
    Danger,
}

fn band(util: f32, t: &Thresholds) -> Band {
    if util >= t.danger {
        Band::Danger
    } else if util >= t.warn {
        Band::Warn
    } else {
        Band::Ok
    }
}

/// What we remember between ticks for a single provider.
#[derive(Debug, Clone, Copy, Default)]
struct ProviderState {
    /// Severity band of the peak window at last evaluation, if it had a
    /// meaningful percentage.
    band: Option<Band>,
    /// Whether the 5-hour session window was fully exhausted (100% used).
    session_exhausted: bool,
}

/// A notification the loop should display.
pub struct Notification {
    pub title: String,
    pub body: String,
}

/// Per-provider memory of the last evaluated state.
#[derive(Default)]
pub struct NotifyState {
    providers: HashMap<ProviderId, ProviderState>,
}

impl NotifyState {
    /// Compare the latest snapshots against remembered state and return the
    /// notifications to fire this tick. Updates the remembered state in place.
    ///
    /// Respects the user's `quota_warning_notifications` and
    /// `session_quota_notifications` toggles; when both are off the state is
    /// still tracked so re-enabling doesn't immediately re-fire on a band the
    /// user is already sitting in.
    pub fn evaluate(
        &mut self,
        settings: &Settings,
        snapshots: &HashMap<ProviderId, UsageSnapshot>,
    ) -> Vec<Notification> {
        let mut out = Vec::new();

        for (&id, snap) in snapshots {
            let prev = self.providers.get(&id).copied().unwrap_or_default();
            let mut next = ProviderState::default();

            // --- session-exhaustion (5h window hits 0% remaining) ---
            // `None` means no session window this tick (signed out, API-key mode,
            // etc.). In that case we carry the previous exhausted flag forward
            // rather than treating absent data as "recovered" — otherwise losing
            // the window would fire a spurious "quota available" notification.
            let session_util = match &snap.mode {
                DisplayMode::Session { primary, .. } => Some(primary.utilization),
                _ => None,
            };
            next.session_exhausted = match session_util {
                Some(u) => u >= 100.0,
                None => prev.session_exhausted,
            };

            if settings.session_quota_notifications {
                if next.session_exhausted && !prev.session_exhausted {
                    out.push(Notification {
                        title: format!("{} session quota reached", snap.provider.label()),
                        body: "The 5-hour session limit is at 0%. It'll reset soon.".to_string(),
                    });
                } else if prev.session_exhausted && session_util.map(|u| u < 100.0).unwrap_or(false)
                {
                    // Only a real, observed sub-100% reading counts as recovery.
                    out.push(Notification {
                        title: format!("{} session quota available", snap.provider.label()),
                        body: "Your 5-hour session limit has reset.".to_string(),
                    });
                }
            }

            // --- threshold warnings (peak window crosses warn / danger) ---
            next.band = snap
                .mode
                .peak_utilization()
                .map(|u| band(u, &settings.thresholds));

            if settings.quota_warning_notifications {
                if let (Some(now), prev_band) = (next.band, prev.band) {
                    let rose = match (prev_band, now) {
                        // Rising into a more severe band than before.
                        (Some(Band::Ok), Band::Warn | Band::Danger) => Some(now),
                        (Some(Band::Warn), Band::Danger) => Some(now),
                        // First observation that is already elevated.
                        (None, Band::Warn | Band::Danger) => Some(now),
                        _ => None,
                    };
                    if let Some(b) = rose {
                        let used = snap.mode.peak_utilization().unwrap_or(0.0).round() as i32;
                        let level = match b {
                            Band::Danger => "critically low",
                            _ => "running low",
                        };
                        out.push(Notification {
                            title: format!("{} quota {level}", snap.provider.label()),
                            body: format!("{used}% used."),
                        });
                    }
                }
            }

            self.providers.insert(id, next);
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::types::{DetailExtras, WindowUsage};
    use chrono::Utc;

    fn session_snap(util: f32) -> UsageSnapshot {
        UsageSnapshot {
            provider: ProviderId::Claude,
            plan_label: "max".into(),
            mode: DisplayMode::Session {
                primary: WindowUsage::new(util, "5h", None),
                secondary: None,
            },
            fetched_at: Utc::now(),
            stale: false,
            extras: DetailExtras::default(),
        }
    }

    fn map(snap: UsageSnapshot) -> HashMap<ProviderId, UsageSnapshot> {
        let mut m = HashMap::new();
        m.insert(ProviderId::Claude, snap);
        m
    }

    #[test]
    fn warns_once_on_rising_edge_only() {
        let settings = Settings::default(); // warn 50, danger 80
        let mut state = NotifyState::default();

        // Below warn: nothing.
        assert!(state
            .evaluate(&settings, &map(session_snap(20.0)))
            .is_empty());
        // Crosses warn: one notification.
        assert_eq!(state.evaluate(&settings, &map(session_snap(60.0))).len(), 1);
        // Still in warn band: nothing (edge-triggered).
        assert!(state
            .evaluate(&settings, &map(session_snap(65.0)))
            .is_empty());
        // Crosses into danger: one more.
        assert_eq!(state.evaluate(&settings, &map(session_snap(90.0))).len(), 1);
        // Still danger: nothing.
        assert!(state
            .evaluate(&settings, &map(session_snap(95.0)))
            .is_empty());
    }

    #[test]
    fn session_exhaustion_and_recovery_each_fire_once() {
        let mut settings = Settings::default();
        settings.quota_warning_notifications = false; // isolate session notifications
        let mut state = NotifyState::default();

        state.evaluate(&settings, &map(session_snap(50.0)));
        let exhausted = state.evaluate(&settings, &map(session_snap(100.0)));
        assert_eq!(exhausted.len(), 1);
        // Still exhausted: no repeat.
        assert!(state
            .evaluate(&settings, &map(session_snap(100.0)))
            .is_empty());
        // Recovers: one "available" notification.
        let recovered = state.evaluate(&settings, &map(session_snap(10.0)));
        assert_eq!(recovered.len(), 1);
    }

    #[test]
    fn losing_session_window_does_not_fire_recovery() {
        // Regression: an exhausted provider that stops returning a session
        // window (sign-out / API-key mode) must NOT emit a "quota available"
        // notification — absent data is not observed recovery.
        let mut settings = Settings::default();
        settings.quota_warning_notifications = false;
        let mut state = NotifyState::default();

        state.evaluate(&settings, &map(session_snap(100.0))); // exhausted
        let no_window = UsageSnapshot::unauthenticated(ProviderId::Claude, Utc::now());
        assert!(state.evaluate(&settings, &map(no_window)).is_empty());
        // And it stays exhausted: a later real sub-100% reading still recovers.
        let recovered = state.evaluate(&settings, &map(session_snap(10.0)));
        assert_eq!(recovered.len(), 1);
    }

    #[test]
    fn respects_disabled_toggles() {
        let mut settings = Settings::default();
        settings.quota_warning_notifications = false;
        settings.session_quota_notifications = false;
        let mut state = NotifyState::default();
        assert!(state
            .evaluate(&settings, &map(session_snap(95.0)))
            .is_empty());
        assert!(state
            .evaluate(&settings, &map(session_snap(100.0)))
            .is_empty());
    }
}
