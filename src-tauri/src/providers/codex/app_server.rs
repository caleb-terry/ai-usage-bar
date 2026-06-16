//! Fallback path: query the local `codex` CLI's app-server over JSON-RPC when
//! the HTTP usage endpoint is unreachable.
//!
//! We spawn `codex -s read-only -a untrusted app-server`, send an
//! `account/rateLimits/read` request over stdin, and parse the first matching
//! response from stdout. This is best-effort: any failure (binary missing, RPC
//! error, timeout) returns an error so the caller can surface the original HTTP
//! failure instead.

use super::client::{RateLimit, RateWindow, RawUsage};
use crate::providers::{ProviderError, ProviderResult};
use serde::Deserialize;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

const RPC_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Deserialize)]
struct RpcEnvelope {
    #[serde(default)]
    id: Option<u64>,
    #[serde(default)]
    result: Option<RateLimitsResult>,
}

#[derive(Debug, Deserialize)]
struct RateLimitsResult {
    #[serde(default)]
    primary: Option<RpcWindow>,
    #[serde(default)]
    secondary: Option<RpcWindow>,
}

#[derive(Debug, Deserialize)]
struct RpcWindow {
    #[serde(default)]
    used_percent: f32,
    #[serde(default)]
    resets_at: Option<i64>,
}

impl From<RpcWindow> for RateWindow {
    fn from(w: RpcWindow) -> Self {
        RateWindow {
            used_percent: w.used_percent,
            reset_at: w.resets_at,
            limit_window_seconds: None,
        }
    }
}

pub async fn fetch_via_app_server() -> ProviderResult<RawUsage> {
    tokio::time::timeout(RPC_TIMEOUT, run_rpc())
        .await
        .map_err(|_| ProviderError::Network("app-server timeout".to_string()))?
}

async fn run_rpc() -> ProviderResult<RawUsage> {
    let mut child = Command::new("codex")
        .args(["-s", "read-only", "-a", "untrusted", "app-server"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| ProviderError::Other(format!("spawn codex: {e}")))?;

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "account/rateLimits/read",
        "params": {}
    });

    if let Some(mut stdin) = child.stdin.take() {
        let line = format!("{request}\n");
        stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| ProviderError::Other(e.to_string()))?;
        stdin
            .flush()
            .await
            .map_err(|e| ProviderError::Other(e.to_string()))?;
    }

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ProviderError::Other("no stdout".to_string()))?;
    let mut reader = BufReader::new(stdout).lines();

    let mut found: Option<RateLimitsResult> = None;
    while let Ok(Some(line)) = reader.next_line().await {
        if let Ok(env) = serde_json::from_str::<RpcEnvelope>(&line) {
            if env.id == Some(1) {
                found = env.result;
                break;
            }
        }
    }

    // Best-effort cleanup.
    let _ = child.kill().await;

    let result = found.ok_or_else(|| ProviderError::Parse("no rateLimits result".to_string()))?;
    Ok(RawUsage {
        rate_limit: Some(RateLimit {
            primary_window: result.primary.map(Into::into),
            secondary_window: result.secondary.map(Into::into),
        }),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rate_limits_envelope() {
        // A well-formed JSON-RPC reply maps both windows through to RateWindow.
        let line = r#"{"jsonrpc":"2.0","id":1,"result":{"primary":{"used_percent":42.5,"resets_at":1893456000},"secondary":{"used_percent":12.0,"resets_at":1893460000}}}"#;
        let env: RpcEnvelope = serde_json::from_str(line).unwrap();
        assert_eq!(env.id, Some(1));
        let result = env.result.expect("result present");
        let primary: RateWindow = result.primary.unwrap().into();
        assert_eq!(primary.used_percent, 42.5);
        assert_eq!(primary.reset_at, Some(1893456000));
        assert!(primary.limit_window_seconds.is_none());
        let secondary: RateWindow = result.secondary.unwrap().into();
        assert_eq!(secondary.used_percent, 12.0);
    }

    #[test]
    fn missing_fields_default_rather_than_fail() {
        // The bespoke `#[serde(default)]` structs must tolerate a sparse reply:
        // absent windows/fields deserialize to None/0, never an error. This is
        // the silent-zeros risk the audit flagged — pin it so a wire change that
        // *renames* a field is caught here (deserializes to zeros) rather than in
        // production.
        let line = r#"{"jsonrpc":"2.0","id":1,"result":{"primary":{}}}"#;
        let env: RpcEnvelope = serde_json::from_str(line).unwrap();
        let result = env.result.unwrap();
        let primary: RateWindow = result.primary.unwrap().into();
        assert_eq!(primary.used_percent, 0.0);
        assert_eq!(primary.reset_at, None);
        assert!(result.secondary.is_none());
    }

    #[test]
    fn non_matching_id_has_no_result() {
        // A reply for a different request id is ignored by the reader loop; here
        // we just confirm an unrelated notification has no result to latch onto.
        let line = r#"{"jsonrpc":"2.0","method":"someNotification","params":{}}"#;
        let env: RpcEnvelope = serde_json::from_str(line).unwrap();
        assert_eq!(env.id, None);
        assert!(env.result.is_none());
    }
}
