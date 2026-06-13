//! Claude Code credential loading and OAuth refresh.
//!
//! Lookup order:
//!   1. macOS Keychain / Windows Credential Manager service `Claude Code-credentials`
//!      (config-scoped variant when `CLAUDE_CONFIG_DIR` is set)
//!   2. `~/.claude/.credentials.json` (or `$CLAUDE_CONFIG_DIR/.credentials.json`)
//!
//! We treat all credentials as **read-only by default**: we only write back when
//! we successfully refresh a token, and we write back to the same store we read
//! from so concurrent CLI sessions stay valid.

use crate::providers::{ProviderError, ProviderResult};
use serde::Deserialize;
use std::path::PathBuf;

const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";
const REFRESH_URL: &str = "https://platform.claude.com/v1/oauth/token";
const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
/// Refresh this many milliseconds before the token actually expires.
const REFRESH_SKEW_MS: i64 = 5 * 60 * 1000;

/// Parsed Claude OAuth credentials.
#[derive(Debug, Clone)]
pub struct ClaudeCredentials {
    pub access_token: String,
    pub refresh_token: String,
    /// Expiry in unix milliseconds.
    pub expires_at: i64,
    pub subscription_type: String,
    /// Where these creds came from, so refreshes are written back in place.
    pub source: CredentialSource,
}

#[derive(Debug, Clone)]
pub enum CredentialSource {
    Keychain(String),
    File(PathBuf),
}

/// The on-disk / keychain JSON envelope.
#[derive(Debug, Deserialize)]
struct CredentialFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: OAuthBlock,
}

#[derive(Debug, Deserialize)]
struct OAuthBlock {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "refreshToken")]
    refresh_token: String,
    #[serde(rename = "expiresAt")]
    expires_at: i64,
    #[serde(rename = "subscriptionType", default)]
    subscription_type: Option<String>,
}

fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    let home = directories::BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .unwrap_or_default();
    home.join(".claude")
}

/// Keychain service name, accounting for a hashed `CLAUDE_CONFIG_DIR` suffix.
fn keychain_service() -> String {
    if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        // Claude uses the first 8 chars of the sha256 of the config dir.
        let digest = sha256_hex(dir.as_bytes());
        format!("{KEYCHAIN_SERVICE}-{}", &digest[..8])
    } else {
        KEYCHAIN_SERVICE.to_string()
    }
}

/// Minimal SHA-256 (avoids pulling a crypto dependency for an 8-char prefix).
fn sha256_hex(data: &[u8]) -> String {
    // FIPS-180-4 SHA-256.
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for (i, word) in w.iter_mut().take(16).enumerate() {
            *word = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ ((!v[4]) & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v[7] = v[6];
            v[6] = v[5];
            v[5] = v[4];
            v[4] = v[3].wrapping_add(t1);
            v[3] = v[2];
            v[2] = v[1];
            v[1] = v[0];
            v[0] = t1.wrapping_add(t2);
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(v[i]);
        }
    }
    h.iter().map(|b| format!("{b:08x}")).collect()
}

fn parse(json: &str, source: CredentialSource) -> ProviderResult<ClaudeCredentials> {
    let parsed: CredentialFile =
        serde_json::from_str(json).map_err(|e| ProviderError::Parse(e.to_string()))?;
    Ok(ClaudeCredentials {
        access_token: parsed.claude_ai_oauth.access_token,
        refresh_token: parsed.claude_ai_oauth.refresh_token,
        expires_at: parsed.claude_ai_oauth.expires_at,
        subscription_type: parsed.claude_ai_oauth.subscription_type.unwrap_or_default(),
        source,
    })
}

/// Load credentials from the keychain, falling back to the JSON file.
pub fn load_credentials() -> ProviderResult<ClaudeCredentials> {
    let service = keychain_service();
    // Keychain entries are stored under the user's account name. We try the
    // generic "user"/empty account first; `keyring` resolves the active user.
    if let Ok(entry) = keyring::Entry::new(&service, &whoami_account()) {
        if let Ok(secret) = entry.get_password() {
            if let Ok(creds) = parse(&secret, CredentialSource::Keychain(service.clone())) {
                return Ok(creds);
            }
        }
    }

    let path = config_dir().join(".credentials.json");
    let json = std::fs::read_to_string(&path).map_err(|_| ProviderError::Unauthenticated)?;
    parse(&json, CredentialSource::File(path))
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
    /// Lifetime in seconds.
    #[serde(default)]
    expires_in: Option<i64>,
}

/// Refresh the token if it is expired or within the skew window. Returns the
/// (possibly refreshed) credentials, writing successful refreshes back to the
/// originating store.
pub async fn ensure_fresh(
    http: &reqwest::Client,
    creds: ClaudeCredentials,
) -> ProviderResult<ClaudeCredentials> {
    let now_ms = chrono::Utc::now().timestamp_millis();
    if creds.expires_at - now_ms > REFRESH_SKEW_MS {
        return Ok(creds);
    }

    let body = serde_json::json!({
        "grant_type": "refresh_token",
        "refresh_token": creds.refresh_token,
        "client_id": CLIENT_ID,
    });

    let resp = http
        .post(REFRESH_URL)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(ProviderError::AuthFailed(format!(
            "refresh returned HTTP {}",
            resp.status()
        )));
    }

    let refreshed: RefreshResponse = resp
        .json()
        .await
        .map_err(|e| ProviderError::Parse(e.to_string()))?;

    let new_expires =
        chrono::Utc::now().timestamp_millis() + refreshed.expires_in.unwrap_or(3600) * 1000;

    let updated = ClaudeCredentials {
        access_token: refreshed.access_token,
        refresh_token: refreshed.refresh_token.unwrap_or(creds.refresh_token),
        expires_at: new_expires,
        subscription_type: creds.subscription_type,
        source: creds.source,
    };

    if let Err(e) = persist(&updated) {
        // A failed write-back is non-fatal for *this* fetch, but log it: the
        // CLI may re-refresh independently.
        log::warn!("failed to persist refreshed Claude token: {e}");
    }

    Ok(updated)
}

/// Write refreshed credentials back to wherever they came from.
fn persist(creds: &ClaudeCredentials) -> ProviderResult<()> {
    let envelope = serde_json::json!({
        "claudeAiOauth": {
            "accessToken": creds.access_token,
            "refreshToken": creds.refresh_token,
            "expiresAt": creds.expires_at,
            "subscriptionType": creds.subscription_type,
        }
    });
    let serialized =
        serde_json::to_string(&envelope).map_err(|e| ProviderError::Other(e.to_string()))?;

    match &creds.source {
        CredentialSource::Keychain(service) => {
            let entry = keyring::Entry::new(service, &whoami_account())
                .map_err(|e| ProviderError::Other(e.to_string()))?;
            // Preserve any other fields by merging would require reading first;
            // the CLI envelope only contains claudeAiOauth, so a full write is safe.
            entry
                .set_password(&serialized)
                .map_err(|e| ProviderError::Other(e.to_string()))?;
        }
        CredentialSource::File(path) => {
            // Atomic-ish write: temp file then rename.
            let tmp = path.with_extension("json.tmp");
            std::fs::write(&tmp, &serialized).map_err(|e| ProviderError::Other(e.to_string()))?;
            std::fs::rename(&tmp, path).map_err(|e| ProviderError::Other(e.to_string()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_known_vector() {
        // SHA-256("abc")
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        // SHA-256("")
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn parses_credential_envelope() {
        let json = r#"{
            "claudeAiOauth": {
                "accessToken": "tok-abc",
                "refreshToken": "ref-xyz",
                "expiresAt": 1893456000000,
                "subscriptionType": "max"
            }
        }"#;
        let creds = parse(json, CredentialSource::File(PathBuf::from("/tmp/x"))).unwrap();
        assert_eq!(creds.access_token, "tok-abc");
        assert_eq!(creds.refresh_token, "ref-xyz");
        assert_eq!(creds.expires_at, 1893456000000);
        assert_eq!(creds.subscription_type, "max");
    }

    #[test]
    fn missing_subscription_type_defaults_empty() {
        let json = r#"{"claudeAiOauth":{"accessToken":"a","refreshToken":"r","expiresAt":1}}"#;
        let creds = parse(json, CredentialSource::File(PathBuf::from("/tmp/x"))).unwrap();
        assert_eq!(creds.subscription_type, "");
    }
}
