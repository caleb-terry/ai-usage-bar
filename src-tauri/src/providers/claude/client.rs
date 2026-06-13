//! Claude usage API client.

use crate::providers::{ProviderError, ProviderResult};
use serde::Deserialize;

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const OAUTH_BETA: &str = "oauth-2025-04-20";
/// Versioned UA so we can spot breakage upstream and keep parity with the CLI.
const USER_AGENT: &str = concat!("ai-usage-bar/", env!("CARGO_PKG_VERSION"));

/// Raw deserialized usage response. All windows are optional because enterprise
/// / spend-cap accounts may omit session windows entirely.
#[derive(Debug, Clone, Deserialize)]
pub struct RawUsage {
    #[serde(default)]
    pub five_hour: Option<RawWindow>,
    #[serde(default)]
    pub seven_day: Option<RawWindow>,
    #[serde(default)]
    pub extra_usage: Option<RawExtraUsage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawWindow {
    #[serde(default)]
    pub utilization: f32,
    #[serde(default)]
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawExtraUsage {
    #[serde(default)]
    pub utilization: Option<f32>,
    // The API returns these as JSON numbers that may be fractional (e.g. 0.0),
    // so they must be floats, not integers.
    #[serde(default)]
    pub used_credits: Option<f64>,
    #[serde(default)]
    pub monthly_limit: Option<f64>,
    #[serde(default)]
    pub resets_at: Option<String>,
}

pub async fn fetch_usage(http: &reqwest::Client, access_token: &str) -> ProviderResult<RawUsage> {
    let resp = http
        .get(USAGE_URL)
        .header("Authorization", format!("Bearer {access_token}"))
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("anthropic-beta", OAUTH_BETA)
        .header("User-Agent", USER_AGENT)
        .send()
        .await?;

    match resp.status() {
        s if s.is_success() => resp
            .json::<RawUsage>()
            .await
            .map_err(|e| ProviderError::Parse(e.to_string())),
        reqwest::StatusCode::TOO_MANY_REQUESTS => Err(ProviderError::RateLimited),
        reqwest::StatusCode::UNAUTHORIZED => Err(ProviderError::AuthFailed("HTTP 401".to_string())),
        s => Err(ProviderError::Network(format!("HTTP {s}"))),
    }
}
