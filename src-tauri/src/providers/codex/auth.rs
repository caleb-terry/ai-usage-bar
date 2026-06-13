//! Codex credential loading and OAuth refresh.
//!
//! Lookup order: `$CODEX_HOME/auth.json` → `~/.config/codex/auth.json` →
//! `~/.codex/auth.json` → OS keyring service `Codex Auth`.
//!
//! Refresh tokens rotate and are single-use, so we refresh conservatively
//! (only when older than 8 days or after a 401) and write the new token back to
//! the same store Codex reads from, atomically, to avoid invalidating
//! concurrent CLI sessions.

use crate::providers::{ProviderError, ProviderResult};
use serde::Deserialize;
use std::path::PathBuf;

const KEYRING_SERVICE: &str = "Codex Auth";
const REFRESH_URL: &str = "https://auth.openai.com/oauth/token";
const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
/// Refresh when the stored token is older than this many days.
const REFRESH_AFTER_DAYS: i64 = 8;

#[derive(Debug, Clone)]
pub struct CodexCredentials {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub account_id: Option<String>,
    pub api_key: Option<String>,
    /// RFC3339 timestamp of the last refresh, if recorded.
    pub last_refresh: Option<String>,
    pub source: CredentialSource,
}

impl CodexCredentials {
    /// True when the only credential is a raw API key (no ChatGPT OAuth tokens).
    pub fn is_api_key_only(&self) -> bool {
        self.access_token.is_none() && self.api_key.is_some()
    }
}

#[derive(Debug, Clone)]
pub enum CredentialSource {
    Keyring,
    File(PathBuf),
}

#[derive(Debug, Deserialize)]
struct AuthFile {
    #[serde(default)]
    tokens: Option<Tokens>,
    #[serde(rename = "OPENAI_API_KEY", default)]
    openai_api_key: Option<String>,
    #[serde(default)]
    last_refresh: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Tokens {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
}

fn candidate_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(dir) = std::env::var("CODEX_HOME") {
        paths.push(PathBuf::from(dir).join("auth.json"));
    }
    if let Some(base) = directories::BaseDirs::new() {
        let home = base.home_dir();
        paths.push(home.join(".config").join("codex").join("auth.json"));
        paths.push(home.join(".codex").join("auth.json"));
    }
    paths
}

fn parse(json: &str, source: CredentialSource) -> ProviderResult<CodexCredentials> {
    let parsed: AuthFile =
        serde_json::from_str(json).map_err(|e| ProviderError::Parse(e.to_string()))?;
    let tokens = parsed.tokens.unwrap_or(Tokens {
        access_token: None,
        refresh_token: None,
        account_id: None,
        id_token: None,
    });
    // An account_id can also be derived from the id_token JWT, but the explicit
    // field is authoritative when present.
    let _ = tokens.id_token;
    Ok(CodexCredentials {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        account_id: tokens.account_id,
        api_key: parsed.openai_api_key,
        last_refresh: parsed.last_refresh,
        source,
    })
}

pub fn load_credentials() -> ProviderResult<CodexCredentials> {
    for path in candidate_paths() {
        if let Ok(json) = std::fs::read_to_string(&path) {
            if let Ok(creds) = parse(&json, CredentialSource::File(path.clone())) {
                if creds.access_token.is_some() || creds.api_key.is_some() {
                    return Ok(creds);
                }
            }
        }
    }

    // Keyring fallback (macOS). Skipped when AIUSAGEBAR_NO_KEYCHAIN is set so
    // the unsigned CLI doesn't block on a SecurityAgent prompt.
    if !keychain_disabled() {
        if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, &whoami_account()) {
            if let Ok(secret) = entry.get_password() {
                if let Ok(creds) = parse(&secret, CredentialSource::Keyring) {
                    if creds.access_token.is_some() || creds.api_key.is_some() {
                        return Ok(creds);
                    }
                }
            }
        }
    }

    Err(ProviderError::Unauthenticated)
}

/// Whether Keychain access is disabled via the `AIUSAGEBAR_NO_KEYCHAIN` env var.
fn keychain_disabled() -> bool {
    std::env::var("AIUSAGEBAR_NO_KEYCHAIN")
        .map(|v| v != "0" && !v.is_empty())
        .unwrap_or(false)
}

fn whoami_account() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "user".to_string())
}

#[derive(Debug, Deserialize)]
struct RefreshResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
}

fn needs_refresh(creds: &CodexCredentials) -> bool {
    if creds.access_token.is_none() {
        return false; // nothing to refresh (api-key mode)
    }
    match &creds.last_refresh {
        Some(ts) => match chrono::DateTime::parse_from_rfc3339(ts) {
            Ok(dt) => {
                let age = chrono::Utc::now().signed_duration_since(dt.with_timezone(&chrono::Utc));
                age.num_days() >= REFRESH_AFTER_DAYS
            }
            Err(_) => true,
        },
        None => true,
    }
}

pub async fn ensure_fresh(
    http: &reqwest::Client,
    creds: CodexCredentials,
) -> ProviderResult<CodexCredentials> {
    if !needs_refresh(&creds) {
        return Ok(creds);
    }
    let Some(refresh_token) = creds.refresh_token.clone() else {
        return Ok(creds);
    };

    let body = serde_json::json!({
        "grant_type": "refresh_token",
        "client_id": CLIENT_ID,
        "refresh_token": refresh_token,
    });

    let resp = http
        .post(REFRESH_URL)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        // Keep using the existing (possibly still-valid) token rather than
        // discarding it; the fetch will surface a 401 if truly dead.
        log::warn!("Codex token refresh failed: HTTP {}", resp.status());
        return Ok(creds);
    }

    let refreshed: RefreshResponse = resp
        .json()
        .await
        .map_err(|e| ProviderError::Parse(e.to_string()))?;

    let updated = CodexCredentials {
        access_token: Some(refreshed.access_token),
        refresh_token: refreshed.refresh_token.or(creds.refresh_token),
        account_id: creds.account_id,
        api_key: creds.api_key,
        last_refresh: Some(chrono::Utc::now().to_rfc3339()),
        source: creds.source,
    };

    if let Err(e) = persist(&updated, refreshed.id_token.as_deref()) {
        log::warn!("failed to persist refreshed Codex token: {e}");
    }

    Ok(updated)
}

/// Write refreshed tokens back, preserving the original file's other fields.
fn persist(creds: &CodexCredentials, id_token: Option<&str>) -> ProviderResult<()> {
    match &creds.source {
        CredentialSource::File(path) => {
            // Read-modify-write to preserve unknown fields.
            let existing = std::fs::read_to_string(path).unwrap_or_else(|_| "{}".to_string());
            let mut root: serde_json::Value =
                serde_json::from_str(&existing).unwrap_or(serde_json::json!({}));

            let tokens = root
                .as_object_mut()
                .and_then(|o| {
                    o.entry("tokens")
                        .or_insert_with(|| serde_json::json!({}))
                        .as_object_mut()
                })
                .ok_or_else(|| ProviderError::Other("auth.json shape".to_string()))?;

            if let Some(at) = &creds.access_token {
                tokens.insert("access_token".into(), serde_json::json!(at));
            }
            if let Some(rt) = &creds.refresh_token {
                tokens.insert("refresh_token".into(), serde_json::json!(rt));
            }
            if let Some(it) = id_token {
                tokens.insert("id_token".into(), serde_json::json!(it));
            }
            if let Some(obj) = root.as_object_mut() {
                obj.insert("last_refresh".into(), serde_json::json!(creds.last_refresh));
            }

            let serialized = serde_json::to_string_pretty(&root)
                .map_err(|e| ProviderError::Other(e.to_string()))?;
            let tmp = path.with_extension("json.tmp");
            std::fs::write(&tmp, &serialized).map_err(|e| ProviderError::Other(e.to_string()))?;
            std::fs::rename(&tmp, path).map_err(|e| ProviderError::Other(e.to_string()))?;
        }
        CredentialSource::Keyring => {
            let envelope = serde_json::json!({
                "tokens": {
                    "access_token": creds.access_token,
                    "refresh_token": creds.refresh_token,
                    "account_id": creds.account_id,
                    "id_token": id_token,
                },
                "last_refresh": creds.last_refresh,
            });
            let entry = keyring::Entry::new(KEYRING_SERVICE, &whoami_account())
                .map_err(|e| ProviderError::Other(e.to_string()))?;
            entry
                .set_password(&envelope.to_string())
                .map_err(|e| ProviderError::Other(e.to_string()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_oauth_tokens() {
        let json = r#"{
            "tokens": {
                "access_token": "at-1",
                "refresh_token": "rt-1",
                "account_id": "acct-9",
                "id_token": "jwt"
            },
            "last_refresh": "2026-06-01T00:00:00Z"
        }"#;
        let c = parse(json, CredentialSource::Keyring).unwrap();
        assert_eq!(c.access_token.as_deref(), Some("at-1"));
        assert_eq!(c.account_id.as_deref(), Some("acct-9"));
        assert!(!c.is_api_key_only());
    }

    #[test]
    fn detects_api_key_only() {
        let json = r#"{"OPENAI_API_KEY": "sk-abc"}"#;
        let c = parse(json, CredentialSource::Keyring).unwrap();
        assert!(c.is_api_key_only());
    }

    #[test]
    fn needs_refresh_when_no_timestamp() {
        let c = CodexCredentials {
            access_token: Some("a".into()),
            refresh_token: Some("r".into()),
            account_id: None,
            api_key: None,
            last_refresh: None,
            source: CredentialSource::Keyring,
        };
        assert!(needs_refresh(&c));
    }

    #[test]
    fn no_refresh_when_recent() {
        let recent = chrono::Utc::now().to_rfc3339();
        let c = CodexCredentials {
            access_token: Some("a".into()),
            refresh_token: Some("r".into()),
            account_id: None,
            api_key: None,
            last_refresh: Some(recent),
            source: CredentialSource::Keyring,
        };
        assert!(!needs_refresh(&c));
    }

    #[test]
    fn api_key_only_never_refreshes() {
        let c = CodexCredentials {
            access_token: None,
            refresh_token: None,
            account_id: None,
            api_key: Some("sk".into()),
            last_refresh: None,
            source: CredentialSource::Keyring,
        };
        assert!(!needs_refresh(&c));
    }
}
