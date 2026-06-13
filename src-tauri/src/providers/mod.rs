//! Provider abstraction. Each provider knows how to load its own local
//! credentials, refresh tokens when needed, fetch usage, and normalize the
//! response into a [`UsageSnapshot`].

pub mod claude;
pub mod codex;

use crate::usage::types::{ProviderId, UsageSnapshot};
use async_trait::async_trait;

/// Errors a provider can surface while fetching usage.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    /// No usable credentials were found on disk or in the OS keystore.
    #[error("not authenticated")]
    Unauthenticated,
    /// Credentials exist but the token is invalid/expired and refresh failed.
    #[error("authentication failed: {0}")]
    AuthFailed(String),
    /// The provider returned a rate-limit response (HTTP 429).
    #[error("rate limited")]
    RateLimited,
    /// Any transport/HTTP error.
    #[error("network error: {0}")]
    Network(String),
    /// The response could not be parsed into the expected shape.
    #[error("unexpected response: {0}")]
    Parse(String),
    /// Provider-specific catch-all.
    #[error("{0}")]
    Other(String),
}

impl From<reqwest::Error> for ProviderError {
    fn from(e: reqwest::Error) -> Self {
        if e.status() == Some(reqwest::StatusCode::TOO_MANY_REQUESTS) {
            ProviderError::RateLimited
        } else {
            ProviderError::Network(e.to_string())
        }
    }
}

pub type ProviderResult<T> = Result<T, ProviderError>;

/// A usage provider (Claude Code, OpenAI Codex, ...).
#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> ProviderId;

    /// Returns `true` if credentials for this provider appear to be present.
    /// Cheap, synchronous-ish check used to auto-enable providers.
    fn has_credentials(&self) -> bool;

    /// Fetch and normalize a fresh usage snapshot. Implementations should
    /// refresh tokens transparently when needed.
    async fn fetch(&self) -> ProviderResult<UsageSnapshot>;
}

/// Perform a single live fetch for one provider using local credentials.
/// Exposed for the `live_fetch` example / manual smoke testing.
pub async fn fetch_once(id: ProviderId) -> ProviderResult<UsageSnapshot> {
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| ProviderError::Network(e.to_string()))?;
    let provider: Box<dyn Provider> = match id {
        ProviderId::Claude => Box::new(claude::ClaudeProvider::new(http)),
        ProviderId::Codex => Box::new(codex::CodexProvider::new(http)),
    };
    provider.fetch().await
}
