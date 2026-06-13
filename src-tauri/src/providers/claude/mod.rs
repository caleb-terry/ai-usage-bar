//! Claude Code provider.

mod auth;
mod client;
mod normalize;

use crate::providers::{Provider, ProviderError, ProviderResult};
use crate::usage::types::{ProviderId, UsageSnapshot};
use async_trait::async_trait;

pub struct ClaudeProvider {
    http: reqwest::Client,
}

impl ClaudeProvider {
    pub fn new(http: reqwest::Client) -> Self {
        Self { http }
    }
}

#[async_trait]
impl Provider for ClaudeProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Claude
    }

    fn has_credentials(&self) -> bool {
        auth::load_credentials().is_ok()
    }

    async fn fetch(&self) -> ProviderResult<UsageSnapshot> {
        // Credential loading may touch the OS keychain, which is a blocking
        // (and potentially prompt-gated) operation; keep it off async workers.
        let creds = tokio::task::spawn_blocking(auth::load_credentials)
            .await
            .map_err(|e| ProviderError::Other(e.to_string()))?
            .map_err(|_| ProviderError::Unauthenticated)?;
        let creds = auth::ensure_fresh(&self.http, creds).await?;
        let raw = client::fetch_usage(&self.http, &creds.access_token).await?;
        Ok(normalize::normalize(&raw, &creds.subscription_type))
    }
}
