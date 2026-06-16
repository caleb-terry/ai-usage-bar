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
    ///
    /// Providers are fetched **concurrently** — one task per enabled provider —
    /// so a poll tick takes as long as the slowest provider, not the sum of all
    /// of them. This matters with the dozen API-key providers; serial fetches
    /// would stack their HTTP round-trips (and hold the aggregator lock the whole
    /// time, since the caller polls under a mutex). The cache merge below runs
    /// after the join, so it stays single-threaded and order-stable.
    pub async fn poll_enabled(&mut self, settings: &Settings) -> Vec<ProviderId> {
        // Drop cache entries for providers that are no longer enabled.
        let enabled: Vec<ProviderId> = ProviderId::ALL
            .into_iter()
            .filter(|id| settings.provider_enabled(*id))
            .collect();
        self.cache.retain(|id, _| enabled.contains(id));

        // Spawn a fetch task per enabled provider that has an implementation.
        let mut handles = Vec::new();
        for id in enabled {
            let Some(provider) = self.providers.get(&id).cloned() else {
                continue;
            };
            handles.push((id, tokio::spawn(async move { provider.fetch().await })));
        }

        // Merge results back into the cache in `ProviderId::ALL` order so the
        // returned `changed` set is deterministic.
        let mut results: HashMap<ProviderId, UsageSnapshot> = HashMap::new();
        for (id, handle) in handles {
            let fetched = handle
                .await
                .unwrap_or_else(|e| Err(ProviderError::Other(e.to_string())));
            results.insert(id, self.resolve(id, fetched));
        }

        let mut changed = Vec::new();
        for id in ProviderId::ALL {
            let Some(next) = results.remove(&id) else {
                continue;
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

    /// Turn one provider's fetch result into the snapshot to cache: pass through
    /// success, synthesize an unauthenticated placeholder, or (on a transient
    /// error) serve the previous snapshot marked stale.
    fn resolve(
        &self,
        id: ProviderId,
        fetched: Result<UsageSnapshot, ProviderError>,
    ) -> UsageSnapshot {
        match fetched {
            Ok(snap) => snap,
            Err(ProviderError::Unauthenticated) => {
                UsageSnapshot::unauthenticated(id, chrono::Utc::now())
            }
            Err(e) => {
                log::warn!("provider {} fetch failed: {e}", id.as_str());
                match self.cache.get(&id).cloned() {
                    Some(mut prev) => {
                        prev.stale = true;
                        prev
                    }
                    None => UsageSnapshot::unauthenticated(id, chrono::Utc::now()),
                }
            }
        }
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

    // A trivial provider that returns a fixed snapshot, to exercise the
    // concurrent poll/merge path without real network calls.
    struct FakeProvider {
        id: ProviderId,
        util: f32,
    }

    #[async_trait::async_trait]
    impl Provider for FakeProvider {
        fn id(&self) -> ProviderId {
            self.id
        }
        fn has_credentials(&self) -> bool {
            true
        }
        async fn fetch(&self) -> crate::providers::ProviderResult<UsageSnapshot> {
            let mut s = snap(self.util);
            s.provider = self.id;
            Ok(s)
        }
    }

    fn settings_with(enabled: &[ProviderId]) -> Settings {
        Settings {
            enabled_providers: enabled.to_vec(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn poll_caches_enabled_and_reports_changes() {
        let mut agg = Aggregator::new(vec![
            Arc::new(FakeProvider {
                id: ProviderId::Claude,
                util: 30.0,
            }),
            Arc::new(FakeProvider {
                id: ProviderId::Codex,
                util: 70.0,
            }),
        ]);
        let settings = settings_with(&[ProviderId::Claude, ProviderId::Codex]);

        // First poll: both providers are new, so both report changed.
        let changed = agg.poll_enabled(&settings).await;
        assert_eq!(changed, vec![ProviderId::Claude, ProviderId::Codex]);
        assert_eq!(agg.all_cached().len(), 2);

        // Second poll with identical data: nothing changed (timestamps ignored).
        let changed = agg.poll_enabled(&settings).await;
        assert!(changed.is_empty());
    }

    #[tokio::test]
    async fn poll_drops_disabled_from_cache() {
        let mut agg = Aggregator::new(vec![
            Arc::new(FakeProvider {
                id: ProviderId::Claude,
                util: 30.0,
            }),
            Arc::new(FakeProvider {
                id: ProviderId::Codex,
                util: 70.0,
            }),
        ]);

        agg.poll_enabled(&settings_with(&[ProviderId::Claude, ProviderId::Codex]))
            .await;
        assert_eq!(agg.all_cached().len(), 2);

        // Disabling Codex evicts it from the cache.
        agg.poll_enabled(&settings_with(&[ProviderId::Claude]))
            .await;
        assert!(agg.cached(ProviderId::Claude).is_some());
        assert!(agg.cached(ProviderId::Codex).is_none());
    }
}
