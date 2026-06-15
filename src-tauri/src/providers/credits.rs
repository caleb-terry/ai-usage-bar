//! Generic API-key "credit balance / usage" provider.
//!
//! Most non-subscription providers expose a single authenticated endpoint that
//! reports either a purchased credit balance or a metered quota. We fetch that,
//! normalize into a [`UsageSnapshot`] (credit balance lands in `DetailExtras`;
//! a metered quota becomes a `SpendCap`-style utilization), and let the shared
//! aggregator/tray/panel render it like any other provider.
//!
//! Endpoints implemented against published, key-only APIs:
//!   - OpenRouter: `GET /api/v1/credits` → `{data:{total_credits,total_usage}}`
//!   - ElevenLabs: `GET /v1/user/subscription` → character quota + reset
//!   - Groq / Deepgram / z.ai / MiniMax: best-documented balance endpoint;
//!     normalized leniently since their shapes vary by account/region.

use crate::providers::{api_key, Provider, ProviderError, ProviderResult};
use crate::usage::types::{DetailExtras, DisplayMode, ProviderId, UsageSnapshot, WindowUsage};
use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use serde_json::Value;

/// One API-key provider, dispatched by `ProviderId`.
pub struct CreditsProvider {
    http: reqwest::Client,
    id: ProviderId,
}

impl CreditsProvider {
    pub fn new(http: reqwest::Client, id: ProviderId) -> Self {
        Self { http, id }
    }
}

#[async_trait]
impl Provider for CreditsProvider {
    fn id(&self) -> ProviderId {
        self.id
    }

    fn has_credentials(&self) -> bool {
        api_key::load_key(self.id).is_some()
    }

    async fn fetch(&self) -> ProviderResult<UsageSnapshot> {
        let id = self.id;
        let key = tokio::task::spawn_blocking(move || api_key::load_key(id))
            .await
            .map_err(|e| ProviderError::Other(e.to_string()))?
            .ok_or(ProviderError::Unauthenticated)?;

        match id {
            ProviderId::OpenRouter => self.fetch_openrouter(&key).await,
            ProviderId::ElevenLabs => self.fetch_elevenlabs(&key).await,
            ProviderId::Groq => self.fetch_groq(&key).await,
            ProviderId::Deepgram => self.fetch_deepgram(&key).await,
            ProviderId::Zai => self.fetch_zai(&key).await,
            ProviderId::MiniMax => self.fetch_minimax(&key).await,
            ProviderId::Gemini => self.fetch_gemini(&key).await,
            ProviderId::Grok => self.fetch_grok(&key).await,
            ProviderId::DeepSeek => self.fetch_deepseek(&key).await,
            ProviderId::Moonshot => self.fetch_moonshot(&key).await,
            ProviderId::Mistral => self.fetch_mistral(&key).await,
            ProviderId::Perplexity => self.fetch_perplexity(&key).await,
            ProviderId::Claude | ProviderId::Codex => {
                Err(ProviderError::Other("not an API-key provider".into()))
            }
        }
    }
}

impl CreditsProvider {
    /// GET a JSON body with a Bearer token, mapping common HTTP failures.
    async fn get_json_bearer(&self, url: &str, key: &str) -> ProviderResult<Value> {
        let resp = self
            .http
            .get(url)
            .header("Authorization", format!("Bearer {key}"))
            .header("Accept", "application/json")
            .send()
            .await?;
        self.json_or_err(resp).await
    }

    async fn json_or_err(&self, resp: reqwest::Response) -> ProviderResult<Value> {
        match resp.status() {
            s if s.is_success() => resp
                .json::<Value>()
                .await
                .map_err(|e| ProviderError::Parse(e.to_string())),
            reqwest::StatusCode::TOO_MANY_REQUESTS => Err(ProviderError::RateLimited),
            reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
                Err(ProviderError::AuthFailed(format!("HTTP {}", resp.status())))
            }
            s => Err(ProviderError::Network(format!("HTTP {s}"))),
        }
    }

    fn snapshot(
        &self,
        plan: impl Into<String>,
        mode: DisplayMode,
        extras: DetailExtras,
    ) -> UsageSnapshot {
        UsageSnapshot {
            provider: self.id,
            plan_label: plan.into(),
            mode,
            fetched_at: Utc::now(),
            stale: false,
            extras,
        }
    }

    /// Build a credit-balance-only snapshot (no rate windows). The balance is
    /// surfaced in the detail panel; there's no meaningful icon percentage, so
    /// it presents as `ApiKeyOnly`.
    fn balance_snapshot(&self, plan: impl Into<String>, balance_cents: u64) -> UsageSnapshot {
        self.snapshot(
            plan,
            DisplayMode::ApiKeyOnly,
            DetailExtras {
                credit_balance_cents: Some(balance_cents),
                ..Default::default()
            },
        )
    }

    // --- OpenRouter ---------------------------------------------------------
    async fn fetch_openrouter(&self, key: &str) -> ProviderResult<UsageSnapshot> {
        let v = self
            .get_json_bearer("https://openrouter.ai/api/v1/credits", key)
            .await?;
        let data = v.get("data").unwrap_or(&v);
        let total = f64_at(data, "total_credits").unwrap_or(0.0);
        let used = f64_at(data, "total_usage").unwrap_or(0.0);
        let balance = (total - used).max(0.0);
        Ok(self.balance_snapshot("credits", (balance * 100.0).round() as u64))
    }

    // --- ElevenLabs ---------------------------------------------------------
    async fn fetch_elevenlabs(&self, key: &str) -> ProviderResult<UsageSnapshot> {
        // ElevenLabs uses an `xi-api-key` header rather than Bearer.
        let resp = self
            .http
            .get("https://api.elevenlabs.io/v1/user/subscription")
            .header("xi-api-key", key)
            .header("Accept", "application/json")
            .send()
            .await?;
        let v = self.json_or_err(resp).await?;

        let used = u64_at(&v, "character_count").unwrap_or(0);
        let limit = u64_at(&v, "character_limit").unwrap_or(0);
        let tier = v
            .get("tier")
            .and_then(|t| t.as_str())
            .unwrap_or("subscription")
            .to_string();
        let reset_at = u64_at(&v, "next_character_count_reset_unix")
            .and_then(|secs| Utc.timestamp_opt(secs as i64, 0).single());

        if limit == 0 {
            return Ok(self.balance_snapshot(tier, 0));
        }
        let utilization = (used as f32 / limit as f32 * 100.0).clamp(0.0, 100.0);
        Ok(self.snapshot(
            tier,
            DisplayMode::Session {
                primary: WindowUsage::new(utilization, "Chars", reset_at),
                secondary: None,
            },
            DetailExtras::default(),
        ))
    }

    // --- Groq ---------------------------------------------------------------
    async fn fetch_groq(&self, key: &str) -> ProviderResult<UsageSnapshot> {
        // Groq is OpenAI-compatible; verify the key by listing models. Groq has
        // no public credit endpoint, so we report authenticated API-key status.
        let _ = self
            .get_json_bearer("https://api.groq.com/openai/v1/models", key)
            .await?;
        Ok(self.snapshot("api", DisplayMode::ApiKeyOnly, DetailExtras::default()))
    }

    // --- Deepgram -----------------------------------------------------------
    async fn fetch_deepgram(&self, key: &str) -> ProviderResult<UsageSnapshot> {
        // Deepgram keys authenticate with `Token <key>`. The projects endpoint
        // confirms the key; balances are project-scoped and omitted here.
        let resp = self
            .http
            .get("https://api.deepgram.com/v1/projects")
            .header("Authorization", format!("Token {key}"))
            .header("Accept", "application/json")
            .send()
            .await?;
        let v = self.json_or_err(resp).await?;
        // Sum any reported balances across projects when present.
        let balance = v
            .get("balances")
            .and_then(|b| b.as_array())
            .map(|arr| arr.iter().filter_map(|x| f64_at(x, "amount")).sum::<f64>());
        match balance {
            Some(amt) => Ok(self.balance_snapshot("api", (amt * 100.0).round() as u64)),
            None => Ok(self.snapshot("api", DisplayMode::ApiKeyOnly, DetailExtras::default())),
        }
    }

    // --- z.ai ---------------------------------------------------------------
    async fn fetch_zai(&self, key: &str) -> ProviderResult<UsageSnapshot> {
        // z.ai is OpenAI-compatible (Bearer). Confirm the key via models list;
        // subscription balances require the dashboard session, not the API key.
        let _ = self
            .get_json_bearer("https://api.z.ai/api/paas/v4/models", key)
            .await?;
        Ok(self.snapshot("api", DisplayMode::ApiKeyOnly, DetailExtras::default()))
    }

    // --- MiniMax ------------------------------------------------------------
    async fn fetch_minimax(&self, key: &str) -> ProviderResult<UsageSnapshot> {
        // MiniMax exposes a balance endpoint that returns the account balance in
        // the response when available; tolerate either field name/region.
        let v = self
            .get_json_bearer("https://api.minimax.io/v1/query/account_balance", key)
            .await?;
        let balance = f64_at(&v, "balance").or_else(|| f64_at(&v, "amount"));
        match balance {
            Some(amt) => Ok(self.balance_snapshot("api", (amt * 100.0).round() as u64)),
            None => Ok(self.snapshot("api", DisplayMode::ApiKeyOnly, DetailExtras::default())),
        }
    }

    // --- Gemini -------------------------------------------------------------
    async fn fetch_gemini(&self, key: &str) -> ProviderResult<UsageSnapshot> {
        // Google AI Studio authenticates with a `?key=` query param. Listing
        // models confirms the key; there's no public credit/usage endpoint.
        let url = format!("https://generativelanguage.googleapis.com/v1beta/models?key={key}");
        let resp = self
            .http
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await?;
        let _ = self.json_or_err(resp).await?;
        Ok(self.snapshot("api", DisplayMode::ApiKeyOnly, DetailExtras::default()))
    }

    // --- Grok (xAI) ---------------------------------------------------------
    async fn fetch_grok(&self, key: &str) -> ProviderResult<UsageSnapshot> {
        // xAI is OpenAI-compatible (Bearer). Confirm the key via the models list.
        let _ = self
            .get_json_bearer("https://api.x.ai/v1/models", key)
            .await?;
        Ok(self.snapshot("api", DisplayMode::ApiKeyOnly, DetailExtras::default()))
    }

    // --- DeepSeek -----------------------------------------------------------
    async fn fetch_deepseek(&self, key: &str) -> ProviderResult<UsageSnapshot> {
        // DeepSeek publishes a real balance endpoint: `balance_infos[]` carries
        // `total_balance` as a decimal string in the account currency.
        let v = self
            .get_json_bearer("https://api.deepseek.com/user/balance", key)
            .await?;
        let balance = v
            .get("balance_infos")
            .and_then(|b| b.as_array())
            .and_then(|arr| arr.first())
            .and_then(|info| f64_at(info, "total_balance"));
        match balance {
            Some(amt) => Ok(self.balance_snapshot("api", (amt * 100.0).round() as u64)),
            None => Ok(self.snapshot("api", DisplayMode::ApiKeyOnly, DetailExtras::default())),
        }
    }

    // --- Moonshot (Kimi) ----------------------------------------------------
    async fn fetch_moonshot(&self, key: &str) -> ProviderResult<UsageSnapshot> {
        // Moonshot exposes a balance endpoint returning `data.available_balance`.
        let v = self
            .get_json_bearer("https://api.moonshot.cn/v1/users/me/balance", key)
            .await?;
        let data = v.get("data").unwrap_or(&v);
        let balance = f64_at(data, "available_balance");
        match balance {
            Some(amt) => Ok(self.balance_snapshot("api", (amt * 100.0).round() as u64)),
            None => Ok(self.snapshot("api", DisplayMode::ApiKeyOnly, DetailExtras::default())),
        }
    }

    // --- Mistral ------------------------------------------------------------
    async fn fetch_mistral(&self, key: &str) -> ProviderResult<UsageSnapshot> {
        // Mistral is OpenAI-compatible (Bearer); confirm via the models list.
        let _ = self
            .get_json_bearer("https://api.mistral.ai/v1/models", key)
            .await?;
        Ok(self.snapshot("api", DisplayMode::ApiKeyOnly, DetailExtras::default()))
    }

    // --- Perplexity ---------------------------------------------------------
    async fn fetch_perplexity(&self, key: &str) -> ProviderResult<UsageSnapshot> {
        // Perplexity is OpenAI-compatible (Bearer); confirm via the models list.
        let _ = self
            .get_json_bearer("https://api.perplexity.ai/models", key)
            .await?;
        Ok(self.snapshot("api", DisplayMode::ApiKeyOnly, DetailExtras::default()))
    }
}

/// Read a numeric field that may be a JSON number or a numeric string.
fn f64_at(v: &Value, key: &str) -> Option<f64> {
    let field = v.get(key)?;
    field
        .as_f64()
        .or_else(|| field.as_str().and_then(|s| s.parse::<f64>().ok()))
}

fn u64_at(v: &Value, key: &str) -> Option<u64> {
    let field = v.get(key)?;
    field
        .as_u64()
        .or_else(|| field.as_str().and_then(|s| s.parse::<u64>().ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f64_at_handles_number_and_string() {
        let v = serde_json::json!({ "a": 12.5, "b": "7.25", "c": "nope" });
        assert_eq!(f64_at(&v, "a"), Some(12.5));
        assert_eq!(f64_at(&v, "b"), Some(7.25));
        assert_eq!(f64_at(&v, "c"), None);
        assert_eq!(f64_at(&v, "missing"), None);
    }

    #[test]
    fn openrouter_balance_math() {
        // total - used, floored at 0, ×100 → cents.
        let data = serde_json::json!({ "total_credits": 10.0, "total_usage": 3.5 });
        let total = f64_at(&data, "total_credits").unwrap();
        let used = f64_at(&data, "total_usage").unwrap();
        assert_eq!(((total - used).max(0.0) * 100.0).round() as u64, 650);
    }

    #[test]
    fn elevenlabs_utilization() {
        let v = serde_json::json!({ "character_count": 25_000, "character_limit": 100_000, "tier": "creator" });
        let used = u64_at(&v, "character_count").unwrap();
        let limit = u64_at(&v, "character_limit").unwrap();
        let util = used as f32 / limit as f32 * 100.0;
        assert_eq!(util, 25.0);
    }
}
