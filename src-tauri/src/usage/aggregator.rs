//! Polls all enabled providers and caches their latest snapshots.

use crate::providers::{Provider, ProviderError};
use crate::settings::Settings;
use crate::usage::types::{ProviderId, UsageSnapshot};
use std::collections::HashMap;
use std::sync::Arc;

/// Holds the provider implementations and the most recent snapshot per provider.
pub struct Aggregator {
    providers: HashMap<ProviderId, Arc<dyn Provider>>,
    /// Last good (or unauthenticated) snapshot per provider, for stale-serving.
    cache: HashMap<ProviderId, UsageSnapshot>,
}

impl Aggregator {
    pub fn new(providers: Vec<Arc<dyn Provider>>) -> Self {
        let providers = providers.into_iter().map(|p| (p.id(), p)).collect();
        Self {
            providers,
            cache: HashMap::new(),
        }
    }

    pub fn cached(&self, id: ProviderId) -> Option<&UsageSnapshot> {
        self.cache.get(&id)
    }

    pub fn all_cached(&self) -> &HashMap<ProviderId, UsageSnapshot> {
        &self.cache
    }

    /// Poll every enabled provider once, updating the cache. On error, the
    /// previous snapshot is retained and marked `stale`. Returns the set of
    /// providers whose displayed snapshot changed.
    pub async fn poll_enabled(&mut self, settings: &Settings) -> Vec<ProviderId> {
        let mut changed = Vec::new();

        for id in ProviderId::ALL {
            if !settings.provider_enabled(id) {
                self.cache.remove(&id);
                continue;
            }
            let Some(provider) = self.providers.get(&id).cloned() else {
                continue;
            };

            let next = match provider.fetch().await {
                Ok(snap) => snap,
                Err(ProviderError::Unauthenticated) => {
                    UsageSnapshot::unauthenticated(id, chrono::Utc::now())
                }
                Err(e) => {
                    log::warn!("provider {} fetch failed: {e}", id.as_str());
                    // Serve cached, marked stale.
                    match self.cache.get(&id).cloned() {
                        Some(mut prev) => {
                            prev.stale = true;
                            prev
                        }
                        None => UsageSnapshot::unauthenticated(id, chrono::Utc::now()),
                    }
                }
            };

            let differs = self
                .cache
                .get(&id)
                .map(|prev| !snapshots_visually_equal(prev, &next))
                .unwrap_or(true);
            if differs {
                changed.push(id);
            }
            self.cache.insert(id, next);
        }

        changed
    }
}

/// Compare only the fields that affect what the tray icon renders, so we avoid
/// redundant redraws when only `fetched_at` ticks.
fn snapshots_visually_equal(a: &UsageSnapshot, b: &UsageSnapshot) -> bool {
    a.provider == b.provider
        && a.plan_label == b.plan_label
        && a.stale == b.stale
        && a.mode == b.mode
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::types::{DisplayMode, WindowUsage};

    fn snap(util: f32) -> UsageSnapshot {
        UsageSnapshot {
            provider: ProviderId::Claude,
            plan_label: "max".into(),
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
    fn visual_equality_ignores_timestamp() {
        let a = snap(40.0);
        let mut b = snap(40.0);
        b.fetched_at = a.fetched_at + chrono::Duration::seconds(180);
        assert!(snapshots_visually_equal(&a, &b));
    }

    #[test]
    fn visual_equality_detects_value_change() {
        assert!(!snapshots_visually_equal(&snap(40.0), &snap(41.0)));
    }
}
