//! Shared API-key loading for credit/usage providers.
//!
//! Mirrors CodexBar's model: a key may come from an environment variable or
//! from a small JSON config file at `~/.aiusagebar/config.json`:
//!
//! ```json
//! { "providers": { "openrouter": { "api_key": "sk-or-..." } } }
//! ```
//!
//! Env var wins over the file so one-off shell scripts and CI work without
//! touching the stored config. Keys are never logged.

use crate::usage::types::ProviderId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// The environment variable consulted for each API-key provider.
pub fn env_var(provider: ProviderId) -> Option<&'static str> {
    match provider {
        ProviderId::OpenRouter => Some("OPENROUTER_API_KEY"),
        ProviderId::ElevenLabs => Some("ELEVENLABS_API_KEY"),
        ProviderId::Groq => Some("GROQ_API_KEY"),
        ProviderId::Deepgram => Some("DEEPGRAM_API_KEY"),
        ProviderId::Zai => Some("ZAI_API_KEY"),
        ProviderId::MiniMax => Some("MINIMAX_API_KEY"),
        ProviderId::Gemini => Some("GEMINI_API_KEY"),
        ProviderId::Grok => Some("XAI_API_KEY"),
        ProviderId::DeepSeek => Some("DEEPSEEK_API_KEY"),
        ProviderId::Moonshot => Some("MOONSHOT_API_KEY"),
        ProviderId::Mistral => Some("MISTRAL_API_KEY"),
        ProviderId::Perplexity => Some("PERPLEXITY_API_KEY"),
        // Subscription providers don't use a stored API key here.
        ProviderId::Claude | ProviderId::Codex => None,
    }
}

/// On-disk config holding per-provider API keys.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ApiKeyConfig {
    #[serde(default)]
    pub providers: HashMap<String, ProviderEntry>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProviderEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

fn config_path() -> PathBuf {
    let home = directories::BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .unwrap_or_default();
    home.join(".aiusagebar").join("config.json")
}

fn load_config() -> ApiKeyConfig {
    match std::fs::read_to_string(config_path()) {
        Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
        Err(_) => ApiKeyConfig::default(),
    }
}

/// Resolve the API key for a provider: env var first, then the config file.
/// Blank/whitespace-only values are treated as absent.
pub fn load_key(provider: ProviderId) -> Option<String> {
    if let Some(var) = env_var(provider) {
        if let Ok(val) = std::env::var(var) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    let cfg = load_config();
    cfg.providers
        .get(provider.as_str())
        .and_then(|e| e.api_key.as_ref())
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
}

/// Persist an API key for a provider into the config file, creating it (with
/// owner-only permissions on Unix) if needed. Passing `None` clears the key.
pub fn store_key(provider: ProviderId, key: Option<&str>) -> std::io::Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut cfg = load_config();
    let entry = cfg
        .providers
        .entry(provider.as_str().to_string())
        .or_default();
    entry.api_key = key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty());

    let json = serde_json::to_string_pretty(&cfg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    restrict_permissions(&tmp);
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

#[cfg(unix)]
fn restrict_permissions(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &std::path::Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_var_only_for_api_key_providers() {
        assert!(env_var(ProviderId::OpenRouter).is_some());
        assert!(env_var(ProviderId::Claude).is_none());
        assert!(env_var(ProviderId::Codex).is_none());
    }

    #[test]
    fn config_roundtrips() {
        let mut cfg = ApiKeyConfig::default();
        cfg.providers.insert(
            "openrouter".into(),
            ProviderEntry {
                api_key: Some("sk-or-test".into()),
            },
        );
        let json = serde_json::to_string(&cfg).unwrap();
        let back: ApiKeyConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.providers.get("openrouter").unwrap().api_key.as_deref(),
            Some("sk-or-test")
        );
    }
}
