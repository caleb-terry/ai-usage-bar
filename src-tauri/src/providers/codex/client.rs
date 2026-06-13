//! Codex usage API client.

use crate::providers::{ProviderError, ProviderResult};
use serde::{Deserialize, Deserializer};

const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const USER_AGENT: &str = concat!("ai-usage-bar/", env!("CARGO_PKG_VERSION"));

/// `credits.balance` is returned as a JSON string (e.g. "0", "12.50").
/// Accept string, number, or null.
fn de_balance<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrNumber {
        S(String),
        N(f64),
        Null,
    }
    Ok(match Option::<StringOrNumber>::deserialize(d)? {
        Some(StringOrNumber::S(s)) => s.trim().parse::<f64>().ok(),
        Some(StringOrNumber::N(n)) => Some(n),
        Some(StringOrNumber::Null) | None => None,
    })
}

/// Raw Codex usage response.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RawUsage {
    #[serde(default)]
    pub plan_type: Option<String>,
    #[serde(default)]
    pub rate_limit: Option<RateLimit>,
    #[serde(default)]
    pub code_review_rate_limit: Option<RateWindow>,
    #[serde(default)]
    pub spend_control: Option<SpendControl>,
    #[serde(default)]
    pub credits: Option<Credits>,
    #[serde(default)]
    pub rate_limit_reset_credits: Option<ResetCredits>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RateLimit {
    #[serde(default)]
    pub primary_window: Option<RateWindow>,
    #[serde(default)]
    pub secondary_window: Option<RateWindow>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RateWindow {
    #[serde(default)]
    pub used_percent: f32,
    /// Unix seconds.
    #[serde(default)]
    pub reset_at: Option<i64>,
    /// Window length in seconds; parsed for completeness, not yet displayed.
    #[allow(dead_code)]
    #[serde(default)]
    pub limit_window_seconds: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SpendControl {
    #[serde(default)]
    pub individual_limit: Option<IndividualLimit>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct IndividualLimit {
    #[serde(default)]
    pub used_percent: f32,
    #[serde(default)]
    pub used_cents: Option<u64>,
    #[serde(default)]
    pub limit_cents: Option<u64>,
    #[serde(default)]
    pub reset_at: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Credits {
    #[serde(default, deserialize_with = "de_balance")]
    pub balance: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ResetCredits {
    #[serde(default)]
    pub available_count: Option<u32>,
}

pub async fn fetch_usage(
    http: &reqwest::Client,
    access_token: &str,
    account_id: Option<&str>,
) -> ProviderResult<RawUsage> {
    let mut req = http
        .get(USAGE_URL)
        .header("Authorization", format!("Bearer {access_token}"))
        .header("Accept", "application/json")
        .header("User-Agent", USER_AGENT);

    // Required for workspace accounts; harmless for personal accounts.
    if let Some(id) = account_id {
        req = req.header("ChatGPT-Account-Id", id);
    }

    let resp = req.send().await?;

    match resp.status() {
        s if s.is_success() => resp
            .json::<RawUsage>()
            .await
            .map_err(|e| ProviderError::Parse(e.to_string())),
        reqwest::StatusCode::TOO_MANY_REQUESTS => Err(ProviderError::RateLimited),
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
            Err(ProviderError::AuthFailed(format!("HTTP {}", resp.status())))
        }
        s => Err(ProviderError::Network(format!("HTTP {s}"))),
    }
}
