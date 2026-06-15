//! Unified usage model shared across all providers.
//!
//! Every provider normalizes its raw API response into a [`UsageSnapshot`]. The
//! tray renderer, settings UI, and detail panel all consume this single shape so
//! they never need provider-specific branching.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Identifies a usage provider.
///
/// Claude and Codex are subscription providers with OAuth session windows and
/// (for those two) local cost logs + status pages. The remaining variants are
/// API-key providers that report a purchased *credit balance* or spend, fetched
/// from a single usage endpoint with a stored API key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderId {
    Claude,
    Codex,
    OpenRouter,
    ElevenLabs,
    Groq,
    Deepgram,
    #[serde(rename = "zai")]
    Zai,
    MiniMax,
    Gemini,
    Grok,
    DeepSeek,
    Moonshot,
    Mistral,
    Perplexity,
}

/// Broad behavior class for a provider, so generic code (settings, cost, status)
/// can branch on capability rather than enumerating every variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    /// OAuth subscription provider with session/weekly windows (Claude, Codex).
    Subscription,
    /// API-key provider reporting a credit balance / spend.
    ApiKeyCredits,
}

impl ProviderId {
    pub const ALL: [ProviderId; 14] = [
        ProviderId::Claude,
        ProviderId::Codex,
        ProviderId::OpenRouter,
        ProviderId::ElevenLabs,
        ProviderId::Groq,
        ProviderId::Deepgram,
        ProviderId::Zai,
        ProviderId::MiniMax,
        ProviderId::Gemini,
        ProviderId::Grok,
        ProviderId::DeepSeek,
        ProviderId::Moonshot,
        ProviderId::Mistral,
        ProviderId::Perplexity,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ProviderId::Claude => "claude",
            ProviderId::Codex => "codex",
            ProviderId::OpenRouter => "openrouter",
            ProviderId::ElevenLabs => "elevenlabs",
            ProviderId::Groq => "groq",
            ProviderId::Deepgram => "deepgram",
            ProviderId::Zai => "zai",
            ProviderId::MiniMax => "minimax",
            ProviderId::Gemini => "gemini",
            ProviderId::Grok => "grok",
            ProviderId::DeepSeek => "deepseek",
            ProviderId::Moonshot => "moonshot",
            ProviderId::Mistral => "mistral",
            ProviderId::Perplexity => "perplexity",
        }
    }

    /// Human-facing display name.
    pub fn label(self) -> &'static str {
        match self {
            ProviderId::Claude => "Claude Code",
            ProviderId::Codex => "Codex",
            ProviderId::OpenRouter => "OpenRouter",
            ProviderId::ElevenLabs => "ElevenLabs",
            ProviderId::Groq => "Groq",
            ProviderId::Deepgram => "Deepgram",
            ProviderId::Zai => "z.ai",
            ProviderId::MiniMax => "MiniMax",
            ProviderId::Gemini => "Gemini",
            ProviderId::Grok => "Grok",
            ProviderId::DeepSeek => "DeepSeek",
            ProviderId::Moonshot => "Moonshot",
            ProviderId::Mistral => "Mistral",
            ProviderId::Perplexity => "Perplexity",
        }
    }

    pub fn kind(self) -> ProviderKind {
        match self {
            ProviderId::Claude | ProviderId::Codex => ProviderKind::Subscription,
            _ => ProviderKind::ApiKeyCredits,
        }
    }
}

/// A single rate-limit window (e.g. the rolling 5-hour or 7-day window).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowUsage {
    /// Percentage of the window consumed, 0.0..=100.0.
    pub utilization: f32,
    /// When this window resets, if the provider reports it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_at: Option<DateTime<Utc>>,
    /// Short human label for the window, e.g. "5h" or "Week".
    pub label: String,
}

impl WindowUsage {
    pub fn new(
        utilization: f32,
        label: impl Into<String>,
        reset_at: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            utilization: utilization.clamp(0.0, 100.0),
            reset_at,
            label: label.into(),
        }
    }
}

/// How a provider's current state should be displayed.
///
/// Classification happens during normalization based on which fields the
/// provider's API actually returned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DisplayMode {
    /// Subscription session windows present (5h + weekly).
    Session {
        primary: WindowUsage,
        secondary: Option<WindowUsage>,
    },
    /// Session windows unavailable but a spend cap is reported.
    SpendCap {
        utilization: f32,
        #[serde(skip_serializing_if = "Option::is_none")]
        used_cents: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        limit_cents: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reset_at: Option<DateTime<Utc>>,
    },
    /// No valid credentials found for this provider.
    Unauthenticated,
    /// Codex authenticated via raw API key — no subscription windows exist.
    ApiKeyOnly,
}

impl DisplayMode {
    /// The single utilization figure used for auto provider-selection and
    /// threshold coloring: the most-constrained window/cap currently active.
    /// Returns `None` for states with no meaningful percentage.
    pub fn peak_utilization(&self) -> Option<f32> {
        match self {
            DisplayMode::Session { primary, secondary } => {
                let mut peak = primary.utilization;
                if let Some(s) = secondary {
                    peak = peak.max(s.utilization);
                }
                Some(peak)
            }
            DisplayMode::SpendCap { utilization, .. } => Some(*utilization),
            DisplayMode::Unauthenticated | DisplayMode::ApiKeyOnly => None,
        }
    }
}

/// Provider-specific extras surfaced only in the detail panel (never the icon).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DetailExtras {
    /// Purchased credit balance, in the provider's smallest currency unit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credit_balance_cents: Option<u64>,
    /// Code-review weekly limit utilization, if reported (Codex).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_review_utilization: Option<f32>,
    /// Number of available on-demand resets (Codex).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_demand_resets: Option<u32>,
    /// Extra-usage spend against the monthly cap (Claude), in cents. Surfaced
    /// alongside the session bars so spend is visible without losing the
    /// windows; the icon never shows this.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_usage_used_cents: Option<u64>,
    /// Monthly extra-usage spend cap (Claude), in cents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_usage_cap_cents: Option<u64>,
}

/// A fully normalized snapshot of one provider's usage at a point in time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageSnapshot {
    pub provider: ProviderId,
    /// e.g. "pro", "max", "plus", "business" — never hard-coded; from the API.
    pub plan_label: String,
    pub mode: DisplayMode,
    pub fetched_at: DateTime<Utc>,
    /// True when serving cached data after a fetch error.
    pub stale: bool,
    #[serde(default)]
    pub extras: DetailExtras,
}

impl UsageSnapshot {
    /// Construct an unauthenticated placeholder snapshot for a provider.
    pub fn unauthenticated(provider: ProviderId, fetched_at: DateTime<Utc>) -> Self {
        Self {
            provider,
            plan_label: String::new(),
            mode: DisplayMode::Unauthenticated,
            fetched_at,
            stale: false,
            extras: DetailExtras::default(),
        }
    }

    pub fn peak_utilization(&self) -> Option<f32> {
        self.mode.peak_utilization()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_clamps_utilization() {
        let w = WindowUsage::new(140.0, "5h", None);
        assert_eq!(w.utilization, 100.0);
        let w = WindowUsage::new(-5.0, "5h", None);
        assert_eq!(w.utilization, 0.0);
    }

    #[test]
    fn peak_picks_max_window() {
        let mode = DisplayMode::Session {
            primary: WindowUsage::new(42.0, "5h", None),
            secondary: Some(WindowUsage::new(81.0, "Week", None)),
        };
        assert_eq!(mode.peak_utilization(), Some(81.0));
    }

    #[test]
    fn peak_none_for_unauthenticated() {
        assert_eq!(DisplayMode::Unauthenticated.peak_utilization(), None);
        assert_eq!(DisplayMode::ApiKeyOnly.peak_utilization(), None);
    }
}
