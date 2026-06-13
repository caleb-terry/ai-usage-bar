//! Per-model token pricing, in USD per single token.
//!
//! Mirrors the ccusage / CodexBar approach: published prices are USD per 1M
//! tokens, converted here to per-token. We match on a normalized model id with
//! substring rules so that dated snapshots (`claude-opus-4-8-20260101`) and
//! aliases resolve to the right lane without an exact-match table.
//!
//! Pricing is intentionally a small built-in table rather than a live
//! models.dev fetch: cost scanning must work fully offline, and the local
//! session logs only ever reference the handful of first-party Claude/OpenAI
//! models a user actually runs. Unknown models fall back to a conservative
//! zero so we never *over*-report spend.

/// Cost lanes for one model, USD per single token.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelPricing {
    pub input: f64,
    pub output: f64,
    /// Cost to *write* a cache entry (5m/1h ephemeral). Falls back to input.
    pub cache_write: f64,
    /// Cost to *read* from cache. Much cheaper than fresh input.
    pub cache_read: f64,
}

impl ModelPricing {
    /// Build from USD-per-1M figures.
    const fn per_million(input: f64, output: f64, cache_write: f64, cache_read: f64) -> Self {
        Self {
            input: input / 1_000_000.0,
            output: output / 1_000_000.0,
            cache_write: cache_write / 1_000_000.0,
            cache_read: cache_read / 1_000_000.0,
        }
    }
}

/// Token counts pulled from a single log entry's `usage` block.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TokenCounts {
    pub input: u64,
    pub output: u64,
    pub cache_write: u64,
    pub cache_read: u64,
}

impl TokenCounts {
    pub fn cost(&self, p: &ModelPricing) -> f64 {
        self.input as f64 * p.input
            + self.output as f64 * p.output
            + self.cache_write as f64 * p.cache_write
            + self.cache_read as f64 * p.cache_read
    }

    pub fn total_tokens(&self) -> u64 {
        self.input + self.output + self.cache_write + self.cache_read
    }
}

/// Resolve pricing for a model id via normalized substring matching.
///
/// Returns `None` for models we don't recognize so the caller can skip them
/// rather than invent a price.
pub fn lookup(model: &str) -> Option<ModelPricing> {
    let m = model.to_ascii_lowercase();

    // --- Anthropic Claude ---
    // Order matters: check the most specific tier tokens first.
    if m.contains("opus") {
        // Opus 4.x family.
        return Some(ModelPricing::per_million(15.0, 75.0, 18.75, 1.50));
    }
    if m.contains("sonnet") {
        // Sonnet 4.x / 3.7 family.
        return Some(ModelPricing::per_million(3.0, 15.0, 3.75, 0.30));
    }
    if m.contains("haiku") {
        // Haiku 4.x / 3.5 family.
        return Some(ModelPricing::per_million(0.80, 4.0, 1.0, 0.08));
    }

    // --- OpenAI (Codex) ---
    // gpt-5 / codex models. cache_read is OpenAI's discounted cached-input rate;
    // OpenAI has no separate cache-write charge, so cache_write == input.
    if m.contains("gpt-5") || m.contains("codex") || m.contains("o4") || m.contains("o3") {
        return Some(ModelPricing::per_million(1.25, 10.0, 1.25, 0.125));
    }
    if m.contains("gpt-4.1") {
        return Some(ModelPricing::per_million(2.0, 8.0, 2.0, 0.50));
    }
    if m.contains("gpt-4o") {
        return Some(ModelPricing::per_million(2.50, 10.0, 2.50, 1.25));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_dated_and_aliased_models() {
        assert!(lookup("claude-opus-4-8-20260101").is_some());
        assert!(lookup("claude-sonnet-4-6").is_some());
        assert!(lookup("gpt-5-codex").is_some());
        assert!(lookup("o3-mini").is_some());
        assert!(lookup("totally-unknown-model").is_none());
    }

    #[test]
    fn opus_costs_more_than_haiku() {
        let opus = lookup("claude-opus-4-8").unwrap();
        let haiku = lookup("claude-haiku-4-5").unwrap();
        assert!(opus.output > haiku.output);
    }

    #[test]
    fn cost_sums_all_lanes() {
        let p = ModelPricing::per_million(1_000_000.0, 1_000_000.0, 1_000_000.0, 1_000_000.0);
        let tc = TokenCounts {
            input: 1,
            output: 2,
            cache_write: 3,
            cache_read: 4,
        };
        // Every lane is $1/token here, so cost == total tokens.
        assert_eq!(tc.cost(&p), 10.0);
        assert_eq!(tc.total_tokens(), 10);
    }
}
