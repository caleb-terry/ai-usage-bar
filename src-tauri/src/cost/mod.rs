//! Local cost summary: ccusage-style spend reconstruction from session logs.
//!
//! [`scanner`] turns each provider's JSONL session logs into per-day token/cost
//! buckets; this module aggregates them into a [`CostSummary`] (today + the
//! configured history window) and caches the result for a short TTL so the scan
//! — which reads many files — doesn't run on every poll tick.

pub mod pricing;
pub mod scanner;

use crate::usage::types::ProviderId;
use chrono::{Duration, Local};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

/// Re-scan logs at most this often; cheaper than re-reading every poll.
const SCAN_TTL_SECS: u64 = 600;

/// One provider's spend over today and the history window.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ProviderCost {
    pub today_usd: f64,
    pub today_tokens: u64,
    pub window_usd: f64,
    pub window_tokens: u64,
}

/// The full cost summary handed to the UI.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct CostSummary {
    /// Per-provider breakdown, keyed by provider id.
    pub providers: HashMap<ProviderId, ProviderCost>,
    /// Sum across providers, for the headline figure.
    pub total_today_usd: f64,
    pub total_window_usd: f64,
    /// Inclusive number of days the window covers.
    pub window_days: u32,
}

/// Compute a fresh summary for the given providers over a `history_days` window.
pub fn compute(providers: &[ProviderId], history_days: u32) -> CostSummary {
    // Local time so "today" and the window match the user's wall clock; the
    // scanner buckets each log line in local time too (see `scanner::day_of`).
    let today = Local::now().date_naive();
    let since = today - Duration::days(history_days.saturating_sub(1) as i64);

    let mut summary = CostSummary {
        window_days: history_days,
        ..Default::default()
    };

    for &id in providers {
        let buckets = scanner::scan(id, since);
        let mut pc = ProviderCost::default();
        for (day, b) in &buckets {
            pc.window_usd += b.cost_usd;
            pc.window_tokens += b.tokens;
            if *day == today {
                pc.today_usd += b.cost_usd;
                pc.today_tokens += b.tokens;
            }
        }
        summary.total_today_usd += pc.today_usd;
        summary.total_window_usd += pc.window_usd;
        summary.providers.insert(id, pc);
    }

    summary
}

/// Process-wide cache of the last scan, so repeated polls reuse a recent result.
struct CacheEntry {
    summary: CostSummary,
    computed_at: Instant,
    history_days: u32,
    /// The exact provider set the cached summary was computed for. Toggling a
    /// provider changes which logs are scanned, so a different set must miss the
    /// cache — otherwise a newly-enabled provider would show $0 and a disabled
    /// one would keep contributing for up to the TTL.
    providers: Vec<ProviderId>,
}

static CACHE: Mutex<Option<CacheEntry>> = Mutex::new(None);

/// Return a cached summary if fresh and computed for the same window *and* the
/// same provider set; otherwise recompute, cache, and return. A history-window
/// or provider-set change busts the cache so the user sees an immediate effect
/// when they adjust either.
pub fn summary_cached(providers: &[ProviderId], history_days: u32) -> CostSummary {
    {
        let guard = CACHE.lock().unwrap();
        if let Some(entry) = guard.as_ref() {
            let fresh = entry.computed_at.elapsed().as_secs() < SCAN_TTL_SECS;
            if fresh && entry.history_days == history_days && entry.providers == providers {
                return entry.summary.clone();
            }
        }
    }

    let summary = compute(providers, history_days);
    let mut guard = CACHE.lock().unwrap();
    *guard = Some(CacheEntry {
        summary: summary.clone(),
        computed_at: Instant::now(),
        history_days,
        providers: providers.to_vec(),
    });
    summary
}

/// Drop the cache (used after a manual refresh so the next read rescans).
pub fn invalidate() {
    *CACHE.lock().unwrap() = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_records_window_days() {
        // No logs guaranteed in CI; just assert the window metadata round-trips
        // and totals are non-negative.
        let s = compute(&[ProviderId::Claude, ProviderId::Codex], 30);
        assert_eq!(s.window_days, 30);
        assert!(s.total_today_usd >= 0.0);
        assert!(s.total_window_usd >= 0.0);
    }

    #[test]
    fn cache_misses_on_provider_set_change() {
        // A different provider set must not be served the previous set's cached
        // summary, even within the TTL. The summary objects are equal here (no
        // logs in CI), so we assert the cache entry's recorded provider set
        // tracks the most recent call rather than the value comparison.
        invalidate();
        summary_cached(&[ProviderId::Claude], 30);
        summary_cached(&[ProviderId::Claude, ProviderId::Codex], 30);
        let guard = CACHE.lock().unwrap();
        let entry = guard.as_ref().expect("cache populated");
        assert_eq!(
            entry.providers,
            vec![ProviderId::Claude, ProviderId::Codex],
            "cache must recompute (and re-key) when the provider set changes"
        );
    }
}
