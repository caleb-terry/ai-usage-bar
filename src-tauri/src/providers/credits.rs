//! Generic API-key "credit balance / usage" provider.
//!
//! Most non-subscription providers expose a single authenticated endpoint that
//! reports either a purchased credit balance or a metered quota. We fetch that,
//! normalize into a [`UsageSnapshot`] (credit balance lands in `DetailExtras`;
//! a metered quota becomes a session window), and let the shared
//! aggregator/tray/panel render it like any other provider.
//!
//! Rather than a near-identical `fetch_*` method per provider, every provider
//! is one row in [`ENDPOINTS`]: a URL, an [`Auth`] scheme, and a [`Parse`]
//! strategy. The strategies capture the only things that actually differ —
//! where the number lives in the JSON, and whether it's a balance, a metered
//! quota, or just a key-validity probe. Adding a provider is one row (plus its
//! metadata + env-var entries in `usage::types` / `api_key`).

use crate::providers::{api_key, Provider, ProviderError, ProviderResult};
use crate::usage::types::{DetailExtras, DisplayMode, ProviderId, UsageSnapshot, WindowUsage};
use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use serde_json::Value;

/// How a provider authenticates its credit/usage request.
#[derive(Clone, Copy)]
enum Auth {
    /// `Authorization: Bearer <key>` (the OpenAI-compatible majority).
    Bearer,
    /// `Authorization: Token <key>` (Deepgram).
    Token,
    /// `xi-api-key: <key>` (ElevenLabs).
    XiApiKey,
    /// `?key=<key>` query parameter (Google AI Studio / Gemini).
    QueryKey,
}

/// How to turn a provider's JSON response into a snapshot. Each variant names a
/// JSON shape; the field paths are the only thing that varies between providers
/// sharing a shape.
#[derive(Clone, Copy)]
enum Parse {
    /// Key-validity probe only (no public balance/usage endpoint). The request
    /// succeeding is the whole signal → `ApiKeyOnly`.
    Probe,
    /// OpenRouter: `data.total_credits - data.total_usage` → remaining balance.
    OpenRouterCredits,
    /// ElevenLabs: `character_count` / `character_limit` → metered session.
    ElevenLabsChars,
    /// A single balance number at a JSON path, in the account currency
    /// (multiplied to cents). The path is a slash-separated drill-down; `[0]`
    /// selects the first array element. e.g. "data/available_balance" or
    /// "balance_infos/[0]/total_balance". If the path misses, parsing falls
    /// back to a top-level `amount` field (this fallback applies to *every*
    /// `Balance` provider, not just MiniMax — see `normalize`).
    Balance(&'static str),
    /// Sum of `amount` across an array at `path` (Deepgram project balances).
    BalanceSum(&'static str),
}

/// One API-key provider's endpoint descriptor.
struct CreditEndpoint {
    url: &'static str,
    auth: Auth,
    parse: Parse,
    /// Plan label applied to the resulting snapshot.
    plan: &'static str,
}

const fn endpoint(
    url: &'static str,
    auth: Auth,
    parse: Parse,
    plan: &'static str,
) -> CreditEndpoint {
    CreditEndpoint {
        url,
        auth,
        parse,
        plan,
    }
}

/// The per-provider endpoint table. `None` for providers that aren't API-key
/// credit providers (the two subscription providers).
fn endpoint_for(id: ProviderId) -> Option<CreditEndpoint> {
    use Auth::*;
    use Parse::*;
    Some(match id {
        ProviderId::OpenRouter => endpoint(
            "https://openrouter.ai/api/v1/credits",
            Bearer,
            OpenRouterCredits,
            "credits",
        ),
        ProviderId::ElevenLabs => endpoint(
            "https://api.elevenlabs.io/v1/user/subscription",
            XiApiKey,
            ElevenLabsChars,
            "subscription",
        ),
        // DeepSeek publishes `balance_infos[0].total_balance` as a decimal string.
        ProviderId::DeepSeek => endpoint(
            "https://api.deepseek.com/user/balance",
            Bearer,
            Balance("balance_infos/[0]/total_balance"),
            "api",
        ),
        // Moonshot (Kimi): `data.available_balance`.
        ProviderId::Moonshot => endpoint(
            "https://api.moonshot.cn/v1/users/me/balance",
            Bearer,
            Balance("data/available_balance"),
            "api",
        ),
        // MiniMax: a top-level `balance`.
        ProviderId::MiniMax => endpoint(
            "https://api.minimax.io/v1/query/account_balance",
            Bearer,
            Balance("balance"),
            "api",
        ),
        // Deepgram: `Token` auth; sum `amount` across `balances[]`.
        ProviderId::Deepgram => endpoint(
            "https://api.deepgram.com/v1/projects",
            Token,
            BalanceSum("balances"),
            "api",
        ),
        // Key-validity probes: OpenAI-compatible model lists / no public balance.
        ProviderId::Groq => endpoint(
            "https://api.groq.com/openai/v1/models",
            Bearer,
            Probe,
            "api",
        ),
        ProviderId::Zai => endpoint("https://api.z.ai/api/paas/v4/models", Bearer, Probe, "api"),
        ProviderId::Gemini => endpoint(
            "https://generativelanguage.googleapis.com/v1beta/models",
            QueryKey,
            Probe,
            "api",
        ),
        ProviderId::Grok => endpoint("https://api.x.ai/v1/models", Bearer, Probe, "api"),
        ProviderId::Mistral => endpoint("https://api.mistral.ai/v1/models", Bearer, Probe, "api"),
        ProviderId::Perplexity => {
            endpoint("https://api.perplexity.ai/models", Bearer, Probe, "api")
        }
        ProviderId::Claude | ProviderId::Codex => return None,
    })
}

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
        let endpoint = endpoint_for(id)
            .ok_or_else(|| ProviderError::Other("not an API-key provider".into()))?;

        let key = tokio::task::spawn_blocking(move || api_key::load_key(id))
            .await
            .map_err(|e| ProviderError::Other(e.to_string()))?
            .ok_or(ProviderError::Unauthenticated)?;

        let v = self.request(&endpoint, &key).await?;
        Ok(self.normalize(&endpoint, v))
    }
}

impl CreditsProvider {
    /// Issue the authenticated GET described by `endpoint` and return the parsed
    /// JSON body (mapping common HTTP failures to `ProviderError`).
    async fn request(&self, endpoint: &CreditEndpoint, key: &str) -> ProviderResult<Value> {
        let req = match endpoint.auth {
            Auth::Bearer => self
                .http
                .get(endpoint.url)
                .header("Authorization", format!("Bearer {key}")),
            Auth::Token => self
                .http
                .get(endpoint.url)
                .header("Authorization", format!("Token {key}")),
            Auth::XiApiKey => self.http.get(endpoint.url).header("xi-api-key", key),
            // Google AI Studio takes the key as a query param, not a header.
            // Use `.query()` rather than formatting it into the URL so the key
            // isn't baked into the request URL string (defense in depth against
            // it surfacing in error/log output; see ProviderError's From impl).
            Auth::QueryKey => self.http.get(endpoint.url).query(&[("key", key)]),
        };
        let resp = req.header("Accept", "application/json").send().await?;
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

    /// Apply the endpoint's parse strategy to a response body.
    fn normalize(&self, endpoint: &CreditEndpoint, v: Value) -> UsageSnapshot {
        match endpoint.parse {
            Parse::Probe => self.snapshot(
                endpoint.plan,
                DisplayMode::ApiKeyOnly,
                DetailExtras::default(),
            ),
            Parse::OpenRouterCredits => {
                let data = v.get("data").unwrap_or(&v);
                let total = f64_at(data, "total_credits").unwrap_or(0.0);
                let used = f64_at(data, "total_usage").unwrap_or(0.0);
                let balance = (total - used).max(0.0);
                self.balance_snapshot(endpoint.plan, (balance * 100.0).round() as u64)
            }
            Parse::ElevenLabsChars => {
                let used = u64_at(&v, "character_count").unwrap_or(0);
                let limit = u64_at(&v, "character_limit").unwrap_or(0);
                let tier = v
                    .get("tier")
                    .and_then(|t| t.as_str())
                    .unwrap_or(endpoint.plan)
                    .to_string();
                let reset_at = u64_at(&v, "next_character_count_reset_unix")
                    .and_then(|secs| Utc.timestamp_opt(secs as i64, 0).single());
                if limit == 0 {
                    return self.balance_snapshot(tier, 0);
                }
                let utilization = (used as f32 / limit as f32 * 100.0).clamp(0.0, 100.0);
                self.snapshot(
                    tier,
                    DisplayMode::Session {
                        primary: WindowUsage::new(utilization, "Chars", reset_at),
                        secondary: None,
                    },
                    DetailExtras::default(),
                )
            }
            Parse::Balance(path) => match dig_f64(&v, path).or_else(|| f64_at(&v, "amount")) {
                Some(amt) => self.balance_snapshot(endpoint.plan, (amt * 100.0).round() as u64),
                None => self.snapshot(
                    endpoint.plan,
                    DisplayMode::ApiKeyOnly,
                    DetailExtras::default(),
                ),
            },
            Parse::BalanceSum(path) => {
                let sum = v
                    .get(path)
                    .and_then(|b| b.as_array())
                    .map(|arr| arr.iter().filter_map(|x| f64_at(x, "amount")).sum::<f64>());
                match sum {
                    Some(amt) => self.balance_snapshot(endpoint.plan, (amt * 100.0).round() as u64),
                    None => self.snapshot(
                        endpoint.plan,
                        DisplayMode::ApiKeyOnly,
                        DetailExtras::default(),
                    ),
                }
            }
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

    /// Build a credit-balance snapshot (no rate windows). The balance drives the
    /// `CreditBalance` display mode (shown in the tray title and menu) and is
    /// also mirrored into `DetailExtras` for the panel's footer.
    fn balance_snapshot(&self, plan: impl Into<String>, balance_cents: u64) -> UsageSnapshot {
        self.snapshot(
            plan,
            DisplayMode::CreditBalance { balance_cents },
            DetailExtras {
                credit_balance_cents: Some(balance_cents),
                ..Default::default()
            },
        )
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

/// Drill into a JSON value along a slash-separated path, returning a number at
/// the leaf (parsed from a string if needed). `[n]` selects an array index.
/// e.g. `dig_f64(v, "balance_infos/[0]/total_balance")`.
fn dig_f64(v: &Value, path: &str) -> Option<f64> {
    let mut cur = v;
    let mut segments = path.split('/').peekable();
    while let Some(seg) = segments.next() {
        let next = if let Some(idx) = seg.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            cur.get(idx.parse::<usize>().ok()?)?
        } else {
            cur.get(seg)?
        };
        // On the final segment, coerce to f64 (number or numeric string).
        if segments.peek().is_none() {
            return next
                .as_f64()
                .or_else(|| next.as_str().and_then(|s| s.parse::<f64>().ok()));
        }
        cur = next;
    }
    None
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
    fn dig_f64_walks_objects_and_arrays() {
        let v = serde_json::json!({
            "balance_infos": [{ "total_balance": "42.50" }],
            "data": { "available_balance": 9.0 },
        });
        assert_eq!(dig_f64(&v, "balance_infos/[0]/total_balance"), Some(42.5));
        assert_eq!(dig_f64(&v, "data/available_balance"), Some(9.0));
        assert_eq!(dig_f64(&v, "data/missing"), None);
        assert_eq!(dig_f64(&v, "balance_infos/[3]/total_balance"), None);
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

    #[test]
    fn every_api_key_provider_has_an_endpoint() {
        use crate::usage::types::ProviderKind;
        for id in ProviderId::ALL {
            let has = endpoint_for(id).is_some();
            assert_eq!(
                has,
                id.kind() == ProviderKind::ApiKeyCredits,
                "endpoint coverage mismatch for {}",
                id.as_str()
            );
        }
    }

    fn provider(id: ProviderId) -> CreditsProvider {
        CreditsProvider::new(reqwest::Client::new(), id)
    }

    /// Drive each real-balance provider's documented response shape through the
    /// full `normalize` path so a typo'd JSON path (which `dig_f64` would turn
    /// into a silent `None` → `ApiKeyOnly`) fails here instead of in the field.
    #[test]
    fn balance_paths_resolve_for_each_provider() {
        // DeepSeek: balance_infos[0].total_balance as a decimal *string*.
        let p = provider(ProviderId::DeepSeek);
        let ep = endpoint_for(ProviderId::DeepSeek).unwrap();
        let body = serde_json::json!({ "balance_infos": [{ "total_balance": "42.50" }] });
        assert!(matches!(
            p.normalize(&ep, body).mode,
            DisplayMode::CreditBalance {
                balance_cents: 4250
            }
        ));

        // Moonshot: data.available_balance as a number.
        let p = provider(ProviderId::Moonshot);
        let ep = endpoint_for(ProviderId::Moonshot).unwrap();
        let body = serde_json::json!({ "data": { "available_balance": 9.0 } });
        assert!(matches!(
            p.normalize(&ep, body).mode,
            DisplayMode::CreditBalance { balance_cents: 900 }
        ));

        // MiniMax: top-level `balance`.
        let p = provider(ProviderId::MiniMax);
        let ep = endpoint_for(ProviderId::MiniMax).unwrap();
        let body = serde_json::json!({ "balance": 12.0 });
        assert!(matches!(
            p.normalize(&ep, body).mode,
            DisplayMode::CreditBalance {
                balance_cents: 1200
            }
        ));

        // MiniMax `amount` fallback when the primary path is absent.
        let body = serde_json::json!({ "amount": 3.5 });
        assert!(matches!(
            p.normalize(&ep, body).mode,
            DisplayMode::CreditBalance { balance_cents: 350 }
        ));

        // Deepgram: sum `amount` across `balances[]`.
        let p = provider(ProviderId::Deepgram);
        let ep = endpoint_for(ProviderId::Deepgram).unwrap();
        let body = serde_json::json!({ "balances": [{ "amount": 1.0 }, { "amount": 2.5 }] });
        assert!(matches!(
            p.normalize(&ep, body).mode,
            DisplayMode::CreditBalance { balance_cents: 350 }
        ));

        // A balance provider with no recognizable figure degrades to ApiKeyOnly,
        // not a wrong number.
        let body = serde_json::json!({ "unexpected": true });
        assert!(matches!(
            p.normalize(&ep, body).mode,
            DisplayMode::ApiKeyOnly
        ));
    }
}
