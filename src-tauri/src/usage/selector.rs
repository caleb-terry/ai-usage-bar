//! Chooses which provider's snapshot the single tray icon should display.

use crate::settings::{ActiveProvider, Settings};
use crate::usage::types::{ProviderId, UsageSnapshot};
use std::collections::HashMap;

/// Resolve the active provider id given the user's preference and current data.
///
/// - Explicit Claude/Codex: that provider if enabled, else the other enabled one.
/// - Auto: the enabled provider with the highest peak utilization. Providers
///   with no meaningful percentage (unauthenticated / api-key) lose to any
///   provider that has one; ties and all-empty fall back to provider order.
pub fn resolve_active(
    settings: &Settings,
    cache: &HashMap<ProviderId, UsageSnapshot>,
) -> Option<ProviderId> {
    let enabled: Vec<ProviderId> = ProviderId::ALL
        .into_iter()
        .filter(|id| settings.provider_enabled(*id))
        .collect();
    if enabled.is_empty() {
        return None;
    }

    match settings.active_provider {
        ActiveProvider::Claude => pick_explicit(ProviderId::Claude, &enabled),
        ActiveProvider::Codex => pick_explicit(ProviderId::Codex, &enabled),
        ActiveProvider::Auto => Some(pick_auto(&enabled, cache)),
    }
}

fn pick_explicit(preferred: ProviderId, enabled: &[ProviderId]) -> Option<ProviderId> {
    if enabled.contains(&preferred) {
        Some(preferred)
    } else {
        enabled.first().copied()
    }
}

fn pick_auto(enabled: &[ProviderId], cache: &HashMap<ProviderId, UsageSnapshot>) -> ProviderId {
    let mut best = enabled[0];
    let mut best_util: Option<f32> = cache.get(&best).and_then(|s| s.peak_utilization());

    for &id in &enabled[1..] {
        let util = cache.get(&id).and_then(|s| s.peak_utilization());
        match (best_util, util) {
            (None, Some(_)) => {
                best = id;
                best_util = util;
            }
            (Some(b), Some(u)) if u > b => {
                best = id;
                best_util = util;
            }
            _ => {}
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::types::{DisplayMode, WindowUsage};

    fn session_snap(provider: ProviderId, util: f32) -> UsageSnapshot {
        UsageSnapshot {
            provider,
            plan_label: "x".into(),
            mode: DisplayMode::Session {
                primary: WindowUsage::new(util, "5h", None),
                secondary: None,
            },
            fetched_at: chrono::Utc::now(),
            stale: false,
            extras: Default::default(),
        }
    }

    #[test]
    fn auto_picks_higher_utilization() {
        let mut cache = HashMap::new();
        cache.insert(ProviderId::Claude, session_snap(ProviderId::Claude, 30.0));
        cache.insert(ProviderId::Codex, session_snap(ProviderId::Codex, 70.0));
        let settings = Settings::default();
        assert_eq!(resolve_active(&settings, &cache), Some(ProviderId::Codex));
    }

    #[test]
    fn auto_prefers_provider_with_data_over_unauthenticated() {
        let mut cache = HashMap::new();
        cache.insert(
            ProviderId::Claude,
            UsageSnapshot::unauthenticated(ProviderId::Claude, chrono::Utc::now()),
        );
        cache.insert(ProviderId::Codex, session_snap(ProviderId::Codex, 5.0));
        let settings = Settings::default();
        assert_eq!(resolve_active(&settings, &cache), Some(ProviderId::Codex));
    }

    #[test]
    fn explicit_falls_back_when_disabled() {
        let settings = Settings {
            active_provider: ActiveProvider::Codex,
            enabled_providers: vec![ProviderId::Claude],
            ..Default::default()
        };
        assert_eq!(
            resolve_active(&settings, &HashMap::new()),
            Some(ProviderId::Claude)
        );
    }

    #[test]
    fn none_when_nothing_enabled() {
        let settings = Settings {
            enabled_providers: vec![],
            ..Default::default()
        };
        assert_eq!(resolve_active(&settings, &HashMap::new()), None);
    }
}
