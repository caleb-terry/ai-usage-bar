//! Local session-log scanner that reconstructs token spend per day.
//!
//! Both Claude Code and Codex append newline-delimited JSON to per-session log
//! files. Each provider records token usage differently, so we have one parser
//! per provider, both emitting the same `DayBucket` stream keyed by calendar day
//! (UTC). The aggregator in `mod.rs` sums those into the summary the UI shows.
//!
//! This is deliberately resilient: a single malformed line never aborts a scan,
//! and unknown models are skipped (counted as tokens, billed as $0) rather than
//! guessed at. The goal is a useful *local* estimate, matching ccusage's intent,
//! not an authoritative invoice.

use super::pricing::{self, TokenCounts};
use crate::usage::types::ProviderId;
use chrono::{DateTime, NaiveDate, Utc};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One calendar day's accumulated spend for a provider.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DayBucket {
    pub cost_usd: f64,
    pub tokens: u64,
}

impl DayBucket {
    fn add(&mut self, tokens: &TokenCounts, model: &str) {
        self.tokens += tokens.total_tokens();
        if let Some(p) = pricing::lookup(model) {
            self.cost_usd += tokens.cost(&p);
        }
    }
}

/// Scan one provider's logs, returning per-day buckets keyed by UTC date.
/// Only days on or after `since` are retained.
pub fn scan(provider: ProviderId, since: NaiveDate) -> HashMap<NaiveDate, DayBucket> {
    let mut out: HashMap<NaiveDate, DayBucket> = HashMap::new();
    // Only Claude/Codex write local session logs we can price; API-key
    // providers report a credit balance instead, so they have no buckets.
    for file in log_files(provider) {
        let Ok(contents) = std::fs::read_to_string(&file) else {
            continue;
        };
        match provider {
            ProviderId::Claude => scan_claude(&contents, since, &mut out),
            ProviderId::Codex => scan_codex(&contents, since, &mut out),
            _ => {}
        }
    }
    out
}

/// Enumerate candidate log files for a provider.
fn log_files(provider: ProviderId) -> Vec<PathBuf> {
    match provider {
        ProviderId::Claude => {
            let dir = claude_config_dir().join("projects");
            collect_jsonl(&dir)
        }
        ProviderId::Codex => {
            let dir = codex_home().join("sessions");
            collect_jsonl(&dir)
        }
        // No local logs for API-key providers.
        _ => Vec::new(),
    }
}

fn claude_config_dir() -> PathBuf {
    if let Ok(d) = std::env::var("CLAUDE_CONFIG_DIR") {
        return PathBuf::from(d);
    }
    home().join(".claude")
}

fn codex_home() -> PathBuf {
    if let Ok(d) = std::env::var("CODEX_HOME") {
        return PathBuf::from(d);
    }
    home().join(".codex")
}

fn home() -> PathBuf {
    directories::BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .unwrap_or_default()
}

/// Recursively collect every `*.jsonl` file under `root` (best-effort).
fn collect_jsonl(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                out.push(path);
            }
        }
    }
    out
}

fn day_of(rfc3339: &str) -> Option<NaiveDate> {
    DateTime::parse_from_rfc3339(rfc3339)
        .ok()
        .map(|dt| dt.with_timezone(&Utc).date_naive())
}

// ---------------------------------------------------------------------------
// Claude
// ---------------------------------------------------------------------------

/// Parse Claude Code logs. Each assistant turn is one line carrying
/// `message.usage` (input/output + cache create/read) and `message.model`,
/// timestamped at the top level.
fn scan_claude(contents: &str, since: NaiveDate, out: &mut HashMap<NaiveDate, DayBucket>) {
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(day) = v.get("timestamp").and_then(|t| t.as_str()).and_then(day_of) else {
            continue;
        };
        if day < since {
            continue;
        }
        let Some(msg) = v.get("message") else {
            continue;
        };
        let Some(usage) = msg.get("usage") else {
            continue;
        };
        let model = msg.get("model").and_then(|m| m.as_str()).unwrap_or("");
        if model.is_empty() {
            continue;
        }
        let tc = TokenCounts {
            input: usage_u64(usage, "input_tokens"),
            output: usage_u64(usage, "output_tokens"),
            cache_write: usage_u64(usage, "cache_creation_input_tokens"),
            cache_read: usage_u64(usage, "cache_read_input_tokens"),
        };
        if tc.total_tokens() == 0 {
            continue;
        }
        out.entry(day).or_default().add(&tc, model);
    }
}

fn usage_u64(usage: &serde_json::Value, key: &str) -> u64 {
    usage.get(key).and_then(|n| n.as_u64()).unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Codex
// ---------------------------------------------------------------------------

/// Parse Codex rollout logs. The active model is announced in `session_meta` /
/// `turn_context` events; subsequent `token_count` events carry per-turn deltas
/// in `last_token_usage`. We attribute each delta to the most recently seen
/// model. (`total_token_usage` is cumulative and would double-count, so it is
/// intentionally ignored.)
fn scan_codex(contents: &str, since: NaiveDate, out: &mut HashMap<NaiveDate, DayBucket>) {
    let mut current_model = String::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };

        // Track the active model from meta/context events.
        if let Some(m) = extract_codex_model(&v) {
            current_model = m;
        }

        let payload = v.get("payload");
        let is_token_count =
            payload.and_then(|p| p.get("type")).and_then(|t| t.as_str()) == Some("token_count");
        if !is_token_count {
            continue;
        }

        let Some(day) = v.get("timestamp").and_then(|t| t.as_str()).and_then(day_of) else {
            continue;
        };
        if day < since || current_model.is_empty() {
            continue;
        }

        let Some(last) = payload
            .and_then(|p| p.get("info"))
            .and_then(|i| i.get("last_token_usage"))
        else {
            continue;
        };

        // Codex's `input_tokens` already excludes the cached portion; cached is
        // reported separately, so map it to the cache-read lane.
        let tc = TokenCounts {
            input: usage_u64(last, "input_tokens"),
            output: usage_u64(last, "output_tokens") + usage_u64(last, "reasoning_output_tokens"),
            cache_write: 0,
            cache_read: usage_u64(last, "cached_input_tokens"),
        };
        if tc.total_tokens() == 0 {
            continue;
        }
        out.entry(day).or_default().add(&tc, &current_model);
    }
}

/// Pull a model id out of a `session_meta` or `turn_context` line.
fn extract_codex_model(v: &serde_json::Value) -> Option<String> {
    let ty = v.get("type").and_then(|t| t.as_str())?;
    if ty != "session_meta" && ty != "turn_context" {
        return None;
    }
    // The model can sit at payload.model or payload.*.model depending on the
    // event; search the payload object for the first "model" string.
    let payload = v.get("payload")?;
    find_model(payload)
}

fn find_model(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::Object(map) => {
            if let Some(m) = map.get("model").and_then(|m| m.as_str()) {
                if !m.is_empty() {
                    return Some(m.to_string());
                }
            }
            map.values().find_map(find_model)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn parses_claude_usage_line() {
        let line = r#"{"timestamp":"2026-06-10T12:00:00.000Z","message":{"model":"claude-opus-4-8","usage":{"input_tokens":100,"output_tokens":200,"cache_creation_input_tokens":50,"cache_read_input_tokens":10}}}"#;
        let mut out = HashMap::new();
        scan_claude(line, d("2026-06-01"), &mut out);
        let bucket = out.get(&d("2026-06-10")).unwrap();
        assert_eq!(bucket.tokens, 360);
        assert!(bucket.cost_usd > 0.0);
    }

    #[test]
    fn claude_respects_since_cutoff() {
        let line = r#"{"timestamp":"2026-05-01T12:00:00.000Z","message":{"model":"claude-opus-4-8","usage":{"input_tokens":100,"output_tokens":200}}}"#;
        let mut out = HashMap::new();
        scan_claude(line, d("2026-06-01"), &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn parses_codex_token_count_attributed_to_model() {
        let meta = r#"{"type":"turn_context","timestamp":"2026-06-08T14:37:40.000Z","payload":{"model":"gpt-5.5"}}"#;
        let tc = r#"{"type":"event_msg","timestamp":"2026-06-08T14:38:00.000Z","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":1000,"output_tokens":500,"reasoning_output_tokens":200,"cached_input_tokens":300}}}}"#;
        let mut out = HashMap::new();
        scan_codex(&format!("{meta}\n{tc}"), d("2026-06-01"), &mut out);
        let bucket = out.get(&d("2026-06-08")).unwrap();
        // 1000 in + (500+200) out + 300 cached = 2000 tokens
        assert_eq!(bucket.tokens, 2000);
        assert!(bucket.cost_usd > 0.0);
    }

    #[test]
    fn codex_token_count_without_model_is_skipped() {
        let tc = r#"{"type":"event_msg","timestamp":"2026-06-08T14:38:00.000Z","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":1000,"output_tokens":500}}}}"#;
        let mut out = HashMap::new();
        scan_codex(tc, d("2026-06-01"), &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn malformed_lines_are_skipped_not_fatal() {
        let input = "not json\n{}\n";
        let mut out = HashMap::new();
        scan_claude(input, d("2020-01-01"), &mut out);
        scan_codex(input, d("2020-01-01"), &mut out);
        assert!(out.is_empty());
    }
}
