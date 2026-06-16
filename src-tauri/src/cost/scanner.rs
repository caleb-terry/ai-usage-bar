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
use chrono::{DateTime, Local, NaiveDate};
use std::collections::{HashMap, HashSet};
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

/// Scan one provider's logs, returning per-day buckets keyed by local date.
/// Only days on or after `since` are retained.
pub fn scan(provider: ProviderId, since: NaiveDate) -> HashMap<NaiveDate, DayBucket> {
    let mut out: HashMap<NaiveDate, DayBucket> = HashMap::new();
    // Claude Code writes the same assistant turn into multiple JSONL files
    // (resumed sessions, sub-agents). ccusage dedupes by (message.id, requestId)
    // across all files so a turn is counted once; we mirror that with a set
    // shared across every file in this provider's scan.
    let mut seen: HashSet<String> = HashSet::new();
    // Only Claude/Codex write local session logs we can price; API-key
    // providers report a credit balance instead, so they have no buckets.
    for file in log_files(provider) {
        let Ok(contents) = std::fs::read_to_string(&file) else {
            continue;
        };
        match provider {
            ProviderId::Claude => scan_claude(&contents, since, &mut out, &mut seen),
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
///
/// Uses the dir entry's own file type (which does *not* follow symlinks) rather
/// than `Path::is_dir` (which does): a symlink pointing back up the tree would
/// otherwise create a cycle and spin the scan forever. Symlinked directories are
/// simply not descended into; symlinked `.jsonl` files are still read.
fn collect_jsonl(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            let path = entry.path();
            if ft.is_dir() {
                stack.push(path);
            } else if (ft.is_file() || ft.is_symlink())
                && path.extension().and_then(|e| e.to_str()) == Some("jsonl")
            {
                out.push(path);
            }
        }
    }
    out
}

/// The calendar day of an RFC3339 timestamp in the user's *local* time zone.
/// Buckets must match the user's wall clock — bucketing in UTC would roll
/// afternoon/evening spend for anyone west of UTC into "tomorrow", so
/// `today_usd` wouldn't match what they see on the clock. The caller's `today`
/// / `since` cutoffs are likewise local (see `cost::compute`).
fn day_of(rfc3339: &str) -> Option<NaiveDate> {
    DateTime::parse_from_rfc3339(rfc3339)
        .ok()
        .map(|dt| dt.with_timezone(&Local).date_naive())
}

// ---------------------------------------------------------------------------
// Claude
// ---------------------------------------------------------------------------

/// Parse Claude Code logs. Each assistant turn is one line carrying
/// `message.usage` (input/output + cache create/read) and `message.model`,
/// timestamped at the top level.
///
/// `seen` dedupes turns across files by their `(message.id, requestId)` pair:
/// Claude Code writes the same assistant turn into multiple JSONL files (resumed
/// sessions, sub-agents), so without this the same spend is counted once per
/// copy. This mirrors ccusage's dedup. Lines missing either key fall through and
/// are counted (they carry no duplicate identity to key on).
fn scan_claude(
    contents: &str,
    since: NaiveDate,
    out: &mut HashMap<NaiveDate, DayBucket>,
    seen: &mut HashSet<String>,
) {
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

        // Skip a turn we've already counted in another file. Only dedupe when
        // both identifying fields are present; otherwise there's no stable
        // identity to key on and we count the line as before.
        if let (Some(mid), Some(rid)) = (
            msg.get("id").and_then(|m| m.as_str()),
            v.get("requestId").and_then(|r| r.as_str()),
        ) {
            if !seen.insert(format!("{mid}\u{1}{rid}")) {
                continue;
            }
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
        // Use a midday-UTC timestamp so the local-time bucket lands on the same
        // date regardless of the test machine's zone (any offset within ±12h of
        // noon stays on the 10th).
        let line = r#"{"timestamp":"2026-06-10T12:00:00.000Z","message":{"model":"claude-opus-4-8","usage":{"input_tokens":100,"output_tokens":200,"cache_creation_input_tokens":50,"cache_read_input_tokens":10}}}"#;
        let mut out = HashMap::new();
        let mut seen = HashSet::new();
        scan_claude(line, d("2026-06-01"), &mut out, &mut seen);
        let bucket = out.get(&d("2026-06-10")).unwrap();
        assert_eq!(bucket.tokens, 360);
        assert!(bucket.cost_usd > 0.0);
    }

    #[test]
    fn claude_respects_since_cutoff() {
        let line = r#"{"timestamp":"2026-05-01T12:00:00.000Z","message":{"model":"claude-opus-4-8","usage":{"input_tokens":100,"output_tokens":200}}}"#;
        let mut out = HashMap::new();
        let mut seen = HashSet::new();
        scan_claude(line, d("2026-06-01"), &mut out, &mut seen);
        assert!(out.is_empty());
    }

    #[test]
    fn claude_dedupes_repeated_turns_across_files() {
        // Same (message.id, requestId) appearing in two files (e.g. a resumed
        // session) must be counted once, not twice.
        let line = r#"{"timestamp":"2026-06-10T12:00:00.000Z","requestId":"req_1","message":{"id":"msg_1","model":"claude-opus-4-8","usage":{"input_tokens":100,"output_tokens":200}}}"#;
        let mut out = HashMap::new();
        let mut seen = HashSet::new();
        scan_claude(line, d("2026-06-01"), &mut out, &mut seen); // file A
        scan_claude(line, d("2026-06-01"), &mut out, &mut seen); // file B (duplicate)
        let bucket = out.get(&d("2026-06-10")).unwrap();
        assert_eq!(bucket.tokens, 300, "duplicate turn must be counted once");
    }

    #[test]
    fn claude_counts_lines_without_dedup_keys() {
        // Lines lacking message.id / requestId have no duplicate identity, so
        // they're counted each time rather than dropped.
        let line = r#"{"timestamp":"2026-06-10T12:00:00.000Z","message":{"model":"claude-opus-4-8","usage":{"input_tokens":100,"output_tokens":200}}}"#;
        let mut out = HashMap::new();
        let mut seen = HashSet::new();
        scan_claude(line, d("2026-06-01"), &mut out, &mut seen);
        scan_claude(line, d("2026-06-01"), &mut out, &mut seen);
        let bucket = out.get(&d("2026-06-10")).unwrap();
        assert_eq!(bucket.tokens, 600);
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
        let mut seen = HashSet::new();
        scan_claude(input, d("2020-01-01"), &mut out, &mut seen);
        scan_codex(input, d("2020-01-01"), &mut out);
        assert!(out.is_empty());
    }
}
