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

/// Static, per-provider metadata. This table is the **single source of truth**
/// for everything downstream code needs to know about a provider that isn't
/// behavior: its wire id, display names, brand accent, and behavior class.
///
/// Adding a provider is one `ProviderId` variant plus one row here (and, for an
/// API-key provider, its endpoint/auth row in `providers::credits` and its env
/// var in `providers::api_key`). The TS side mirrors this exact table in
/// `src/api.ts`; `provider_metadata_matches_typescript` guards against drift.
pub struct ProviderMeta {
    pub id: ProviderId,
    /// Lowercase wire id used in JSON, config keys, and the CLI.
    pub str: &'static str,
    /// Human-facing display name.
    pub label: &'static str,
    /// Short label for the compact tab chips in the panel.
    pub tab_label: &'static str,
    /// Brand accent (hex) driving the tab underline and hero card fill.
    pub accent: &'static str,
    pub kind: ProviderKind,
    /// Environment variable consulted for this provider's API key, if it's an
    /// API-key provider. `None` for subscription providers (CLI-authenticated).
    pub env_var: Option<&'static str>,
    /// statuspage.io status endpoint, when the provider publishes one we poll.
    pub status_url: Option<&'static str>,
}

/// The provider table. Order defines `ProviderId::ALL` and the UI's default
/// provider ordering. Keep new providers grouped with their kind for clarity.
///
/// Columns: id, str, label, tab_label, accent, kind, env_var, status_url.
pub const PROVIDER_META: &[ProviderMeta] = &[
    meta(
        ProviderId::Claude,
        "claude",
        "Claude Code",
        "Claude",
        "#d97757",
        ProviderKind::Subscription,
        None,
        Some("https://status.claude.com/api/v2/status.json"),
    ),
    meta(
        ProviderId::Codex,
        "codex",
        "Codex",
        "Codex",
        "#10a37f",
        ProviderKind::Subscription,
        None,
        Some("https://status.openai.com/api/v2/status.json"),
    ),
    meta(
        ProviderId::OpenRouter,
        "openrouter",
        "OpenRouter",
        "OpenRouter",
        "#6566f1",
        ProviderKind::ApiKeyCredits,
        Some("OPENROUTER_API_KEY"),
        None,
    ),
    meta(
        ProviderId::ElevenLabs,
        "elevenlabs",
        "ElevenLabs",
        "11Labs",
        "#000000",
        ProviderKind::ApiKeyCredits,
        Some("ELEVENLABS_API_KEY"),
        None,
    ),
    meta(
        ProviderId::Groq,
        "groq",
        "Groq",
        "Groq",
        "#f55036",
        ProviderKind::ApiKeyCredits,
        Some("GROQ_API_KEY"),
        Some("https://groqstatus.com/api/v2/status.json"),
    ),
    meta(
        ProviderId::Deepgram,
        "deepgram",
        "Deepgram",
        "Deepgram",
        "#13ef93",
        ProviderKind::ApiKeyCredits,
        Some("DEEPGRAM_API_KEY"),
        None,
    ),
    meta(
        ProviderId::Zai,
        "zai",
        "z.ai",
        "z.ai",
        "#3b82f6",
        ProviderKind::ApiKeyCredits,
        Some("ZAI_API_KEY"),
        None,
    ),
    meta(
        ProviderId::MiniMax,
        "minimax",
        "MiniMax",
        "MiniMax",
        "#ff4f4f",
        ProviderKind::ApiKeyCredits,
        Some("MINIMAX_API_KEY"),
        None,
    ),
    meta(
        ProviderId::Gemini,
        "gemini",
        "Gemini",
        "Gemini",
        "#4285f4",
        ProviderKind::ApiKeyCredits,
        Some("GEMINI_API_KEY"),
        None,
    ),
    meta(
        ProviderId::Grok,
        "grok",
        "Grok",
        "Grok",
        "#1a1a1a",
        ProviderKind::ApiKeyCredits,
        Some("XAI_API_KEY"),
        None,
    ),
    meta(
        ProviderId::DeepSeek,
        "deepseek",
        "DeepSeek",
        "DeepSeek",
        "#4d6bfe",
        ProviderKind::ApiKeyCredits,
        Some("DEEPSEEK_API_KEY"),
        None,
    ),
    meta(
        ProviderId::Moonshot,
        "moonshot",
        "Moonshot",
        "Moonshot",
        "#16191e",
        ProviderKind::ApiKeyCredits,
        Some("MOONSHOT_API_KEY"),
        None,
    ),
    meta(
        ProviderId::Mistral,
        "mistral",
        "Mistral",
        "Mistral",
        "#fa520f",
        ProviderKind::ApiKeyCredits,
        Some("MISTRAL_API_KEY"),
        None,
    ),
    meta(
        ProviderId::Perplexity,
        "perplexity",
        "Perplexity",
        "Perplexity",
        "#20808d",
        ProviderKind::ApiKeyCredits,
        Some("PERPLEXITY_API_KEY"),
        None,
    ),
];

/// `const fn` row constructor so `PROVIDER_META` stays a compile-time constant
/// while reading as a flat table.
#[allow(clippy::too_many_arguments)]
const fn meta(
    id: ProviderId,
    str: &'static str,
    label: &'static str,
    tab_label: &'static str,
    accent: &'static str,
    kind: ProviderKind,
    env_var: Option<&'static str>,
    status_url: Option<&'static str>,
) -> ProviderMeta {
    ProviderMeta {
        id,
        str,
        label,
        tab_label,
        accent,
        kind,
        env_var,
        status_url,
    }
}

impl ProviderId {
    /// Every provider, in table order. Built from `PROVIDER_META` so the list
    /// can never fall out of sync with the metadata.
    pub const ALL: [ProviderId; PROVIDER_META.len()] = {
        let mut out = [ProviderId::Claude; PROVIDER_META.len()];
        let mut i = 0;
        while i < PROVIDER_META.len() {
            out[i] = PROVIDER_META[i].id;
            i += 1;
        }
        out
    };

    /// This provider's metadata row. Infallible: every variant has a row, and
    /// `provider_meta_covers_all_variants` proves it.
    pub fn meta(self) -> &'static ProviderMeta {
        let mut i = 0;
        while i < PROVIDER_META.len() {
            if PROVIDER_META[i].id as u8 == self as u8 {
                return &PROVIDER_META[i];
            }
            i += 1;
        }
        // A missing row is a programmer error (a new ProviderId variant added
        // without its PROVIDER_META entry). Fail loudly rather than silently
        // returning the wrong provider's label/accent/env_var/status_url — a
        // wrong-credentials/wrong-status bug is far nastier to trace than a
        // panic. `provider_meta_covers_all_variants` makes this unreachable in
        // a tested build; the table is ≤14 entries, so the scan isn't hot.
        unreachable!("ProviderId missing from PROVIDER_META")
    }

    pub fn as_str(self) -> &'static str {
        self.meta().str
    }

    /// Human-facing display name.
    pub fn label(self) -> &'static str {
        self.meta().label
    }

    /// Short label for the compact tab chips.
    pub fn tab_label(self) -> &'static str {
        self.meta().tab_label
    }

    /// Brand accent color (hex).
    pub fn accent(self) -> &'static str {
        self.meta().accent
    }

    pub fn kind(self) -> ProviderKind {
        self.meta().kind
    }

    /// Environment variable consulted for this provider's API key, if any.
    pub fn env_var(self) -> Option<&'static str> {
        self.meta().env_var
    }

    /// statuspage.io status endpoint for this provider, if one is published.
    pub fn status_url(self) -> Option<&'static str> {
        self.meta().status_url
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
    /// API key valid, but the provider reports a purchased dollar balance rather
    /// than a utilization percentage (OpenRouter, DeepSeek, …). The balance is
    /// the meaningful figure; there is no percentage for the icon.
    CreditBalance { balance_cents: u64 },
    /// No valid credentials found for this provider.
    Unauthenticated,
    /// API key valid, but the provider exposes no balance or usage figure — the
    /// only signal is that the key authenticates (Groq, Gemini, …).
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
            DisplayMode::CreditBalance { .. }
            | DisplayMode::Unauthenticated
            | DisplayMode::ApiKeyOnly => None,
        }
    }

    /// Short tray-title string shown next to the macOS menu-bar glyph: the
    /// 5-hour session (or spend-cap) percentage honoring the used/remaining
    /// preference, a dollar balance, or a terse placeholder. The single
    /// implementation consumed by the tray and (for parity) any other caller.
    pub fn tray_title(&self, show_remaining: bool) -> String {
        match self {
            DisplayMode::Session { primary, .. } => {
                format!(
                    "{}%",
                    display_pct(primary.utilization, show_remaining).round() as i32
                )
            }
            DisplayMode::SpendCap { utilization, .. } => {
                format!(
                    "{}%",
                    display_pct(*utilization, show_remaining).round() as i32
                )
            }
            DisplayMode::CreditBalance { balance_cents } => format_usd_cents(*balance_cents),
            DisplayMode::Unauthenticated => "—".to_string(),
            DisplayMode::ApiKeyOnly => "key".to_string(),
        }
    }

    /// One-line body describing the mode (no provider/plan prefix), used in the
    /// tray menu header, tooltip, and the CLI. `used`/`left` follows the
    /// used/remaining preference.
    pub fn status_summary(&self, show_remaining: bool) -> String {
        let label = if show_remaining { "left" } else { "used" };
        match self {
            DisplayMode::Session { primary, secondary } => {
                let p = display_pct(primary.utilization, show_remaining).round() as i32;
                match secondary {
                    Some(s) => format!(
                        "5h {p}% {label} · Wk {}% {label}",
                        display_pct(s.utilization, show_remaining).round() as i32
                    ),
                    None => format!("5h {p}% {label}"),
                }
            }
            DisplayMode::SpendCap { utilization, .. } => {
                format!(
                    "Spend cap {}% {label}",
                    display_pct(*utilization, show_remaining).round() as i32
                )
            }
            DisplayMode::CreditBalance { balance_cents } => {
                format!("{} credits", format_usd_cents(*balance_cents))
            }
            DisplayMode::Unauthenticated => "Sign in required".to_string(),
            DisplayMode::ApiKeyOnly => "API key — no usage limits reported".to_string(),
        }
    }
}

/// Apply the show-remaining preference to a raw utilization. The single
/// implementation of this rule: `DisplayMode`'s presentation methods call it
/// directly, and `Settings::display_pct` delegates to it (settings already
/// depends on this module, so there's no cycle and no second copy to drift).
pub(crate) fn display_pct(utilization: f32, show_remaining: bool) -> f32 {
    if show_remaining {
        100.0 - utilization
    } else {
        utilization
    }
}

/// Format a cent amount as `$12.34` / `$7,293` for menu-bar and menu display.
/// `pub(crate)` so the icon renderer (baked-glyph path) formats balances
/// identically to the tray title, rather than truncating to whole dollars.
/// Locale: en-only (`,`/`.`); revisit if i18n grows beyond English.
pub(crate) fn format_usd_cents(cents: u64) -> String {
    let dollars = cents as f64 / 100.0;
    if dollars >= 1000.0 {
        // Thousands separator, no decimals, for compact menu-bar fit.
        let whole = dollars.round() as u64;
        let mut s = whole.to_string();
        let mut i = s.len();
        while i > 3 {
            i -= 3;
            s.insert(i, ',');
        }
        format!("${s}")
    } else {
        format!("${dollars:.2}")
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

    #[test]
    fn tray_title_formats_each_mode() {
        let session = DisplayMode::Session {
            primary: WindowUsage::new(45.0, "5h", None),
            secondary: None,
        };
        assert_eq!(session.tray_title(false), "45%");
        assert_eq!(session.tray_title(true), "55%"); // remaining

        let bal = DisplayMode::CreditBalance {
            balance_cents: 1850,
        };
        assert_eq!(bal.tray_title(false), "$18.50");
        let big = DisplayMode::CreditBalance {
            balance_cents: 729_300,
        };
        assert_eq!(big.tray_title(false), "$7,293");

        assert_eq!(DisplayMode::Unauthenticated.tray_title(false), "—");
        assert_eq!(DisplayMode::ApiKeyOnly.tray_title(false), "key");
    }

    #[test]
    fn status_summary_formats_each_mode() {
        let session = DisplayMode::Session {
            primary: WindowUsage::new(42.0, "5h", None),
            secondary: Some(WindowUsage::new(18.0, "Week", None)),
        };
        assert_eq!(session.status_summary(false), "5h 42% used · Wk 18% used");
        assert_eq!(session.status_summary(true), "5h 58% left · Wk 82% left");

        let bal = DisplayMode::CreditBalance {
            balance_cents: 1850,
        };
        assert_eq!(bal.status_summary(false), "$18.50 credits");
        assert_eq!(
            DisplayMode::Unauthenticated.status_summary(false),
            "Sign in required"
        );
    }

    #[test]
    fn credit_balance_has_no_peak() {
        assert_eq!(
            DisplayMode::CreditBalance { balance_cents: 500 }.peak_utilization(),
            None
        );
    }

    #[test]
    fn provider_meta_covers_all_variants() {
        // Every ProviderId resolves to its own row (not the fallback). This
        // makes ProviderId::meta() infallible by construction.
        for id in ProviderId::ALL {
            assert_eq!(
                id.meta().id,
                id,
                "{} resolved to the wrong row",
                id.as_str()
            );
        }
        assert_eq!(PROVIDER_META.len(), ProviderId::ALL.len());
    }

    /// The TS side (`src/api.ts`) hand-mirrors this table. If a provider is
    /// added/edited in Rust but the TS literal drifts, this snapshot catches it:
    /// update `EXPECTED_TS_TABLE` and the TS file together. Each tuple is
    /// (str, label, tab_label, accent, is_api_key).
    #[test]
    fn provider_metadata_matches_typescript() {
        const EXPECTED_TS_TABLE: &[(&str, &str, &str, &str, bool)] = &[
            ("claude", "Claude Code", "Claude", "#d97757", false),
            ("codex", "Codex", "Codex", "#10a37f", false),
            ("openrouter", "OpenRouter", "OpenRouter", "#6566f1", true),
            ("elevenlabs", "ElevenLabs", "11Labs", "#000000", true),
            ("groq", "Groq", "Groq", "#f55036", true),
            ("deepgram", "Deepgram", "Deepgram", "#13ef93", true),
            ("zai", "z.ai", "z.ai", "#3b82f6", true),
            ("minimax", "MiniMax", "MiniMax", "#ff4f4f", true),
            ("gemini", "Gemini", "Gemini", "#4285f4", true),
            ("grok", "Grok", "Grok", "#1a1a1a", true),
            ("deepseek", "DeepSeek", "DeepSeek", "#4d6bfe", true),
            ("moonshot", "Moonshot", "Moonshot", "#16191e", true),
            ("mistral", "Mistral", "Mistral", "#fa520f", true),
            ("perplexity", "Perplexity", "Perplexity", "#20808d", true),
        ];
        let actual: Vec<_> = PROVIDER_META
            .iter()
            .map(|m| {
                (
                    m.str,
                    m.label,
                    m.tab_label,
                    m.accent,
                    m.kind == ProviderKind::ApiKeyCredits,
                )
            })
            .collect();
        assert_eq!(actual, EXPECTED_TS_TABLE);
    }

    /// `env_var` and `status_url` aren't carried by the TS mirror, so the
    /// snapshot above can't guard them. Pin them here: a typo in an API-key env
    /// var (e.g. `XAI_API_KEY`) or a status endpoint would otherwise silently
    /// break credential loading / status polling with no failing test.
    #[test]
    fn provider_env_var_and_status_url_pinned() {
        const EXPECTED: &[(&str, Option<&str>, Option<&str>)] = &[
            // (str, env_var, status_url)
            (
                "claude",
                None,
                Some("https://status.claude.com/api/v2/status.json"),
            ),
            (
                "codex",
                None,
                Some("https://status.openai.com/api/v2/status.json"),
            ),
            ("openrouter", Some("OPENROUTER_API_KEY"), None),
            ("elevenlabs", Some("ELEVENLABS_API_KEY"), None),
            (
                "groq",
                Some("GROQ_API_KEY"),
                Some("https://groqstatus.com/api/v2/status.json"),
            ),
            ("deepgram", Some("DEEPGRAM_API_KEY"), None),
            ("zai", Some("ZAI_API_KEY"), None),
            ("minimax", Some("MINIMAX_API_KEY"), None),
            ("gemini", Some("GEMINI_API_KEY"), None),
            ("grok", Some("XAI_API_KEY"), None),
            ("deepseek", Some("DEEPSEEK_API_KEY"), None),
            ("moonshot", Some("MOONSHOT_API_KEY"), None),
            ("mistral", Some("MISTRAL_API_KEY"), None),
            ("perplexity", Some("PERPLEXITY_API_KEY"), None),
        ];
        let actual: Vec<_> = PROVIDER_META
            .iter()
            .map(|m| (m.str, m.env_var, m.status_url))
            .collect();
        assert_eq!(actual, EXPECTED);
        // Every API-key provider must declare an env var; subscription providers
        // must not. This catches a kind/env_var mismatch independent of the
        // pinned list above.
        for m in PROVIDER_META {
            match m.kind {
                ProviderKind::ApiKeyCredits => {
                    assert!(m.env_var.is_some(), "{} missing env_var", m.str)
                }
                ProviderKind::Subscription => {
                    assert!(m.env_var.is_none(), "{} should not have env_var", m.str)
                }
            }
        }
    }
}
