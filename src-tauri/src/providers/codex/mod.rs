//! OpenAI Codex provider.

mod app_server;
mod auth;
mod client;
mod normalize;

use crate::providers::{Provider, ProviderError, ProviderResult};
use crate::usage::types::{DisplayMode, ProviderId, UsageSnapshot};
use async_trait::async_trait;

pub struct CodexProvider {
    http: reqwest::Client,
}

impl CodexProvider {
    pub fn new(http: reqwest::Client) -> Self {
        Self { http }
    }
}

#[async_trait]
impl Provider for CodexProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Codex
    }

    fn has_credentials(&self) -> bool {
        auth::load_credentials().is_ok()
    }

    async fn fetch(&self) -> ProviderResult<UsageSnapshot> {
        // File reads are quick, but the keyring fallback can block; offload.
        let creds = tokio::task::spawn_blocking(auth::load_credentials)
            .await
            .map_err(|e| ProviderError::Other(e.to_string()))?
            .map_err(|_| ProviderError::Unauthenticated)?;

        // API-key-only accounts have no subscription rate windows to report.
        if creds.is_api_key_only() {
            return Ok(UsageSnapshot {
                provider: ProviderId::Codex,
                plan_label: "api".to_string(),
                mode: DisplayMode::ApiKeyOnly,
                fetched_at: chrono::Utc::now(),
                stale: false,
                extras: Default::default(),
            });
        }

        let creds = auth::ensure_fresh(&self.http, creds).await?;
        let access = creds
            .access_token
            .as_deref()
            .ok_or(ProviderError::Unauthenticated)?;

        // Primary: HTTP usage API. Fallback: app-server JSON-RPC.
        match client::fetch_usage(&self.http, access, creds.account_id.as_deref()).await {
            Ok(raw) => Ok(normalize::normalize(&raw)),
            Err(http_err) => match app_server::fetch_via_app_server().await {
                Ok(raw) => Ok(normalize::normalize(&raw)),
                Err(_) => Err(http_err),
            },
        }
    }
}
