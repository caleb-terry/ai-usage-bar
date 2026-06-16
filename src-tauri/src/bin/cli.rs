//! `aiusagebar` — standalone CLI that mirrors the menu bar app's data without
//! any UI. It reuses the same provider fetchers, local cost scanner, status
//! poller, and API-key config the app uses, so scripts and CI get identical
//! numbers.
//!
//! Commands:
//!   aiusagebar usage  [--provider <id|all>] [--format text|json] [--pretty] [--status]
//!   aiusagebar cost   [--format text|json] [--pretty] [--refresh]
//!   aiusagebar config providers
//!   aiusagebar config enable  --provider <id>
//!   aiusagebar config disable --provider <id>
//!   aiusagebar config set-api-key --provider <id> [--stdin | --api-key <key>]
//!   aiusagebar config dump
//!   aiusagebar --version | --help
//!
//! Arg parsing is hand-rolled (the surface is small) to avoid pulling a CLI
//! framework into the app's dependency tree.

use ai_usage_bar_lib::cost;
use ai_usage_bar_lib::providers::{self, api_key};
use ai_usage_bar_lib::settings;
use ai_usage_bar_lib::status;
use ai_usage_bar_lib::usage::types::{ProviderId, ProviderKind};
use std::collections::BTreeMap;
use std::io::Read;
use std::process::ExitCode;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<ExitCode, String> {
    if args.iter().any(|a| a == "-V" || a == "--version") {
        println!("aiusagebar {VERSION}");
        return Ok(ExitCode::SUCCESS);
    }
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return Ok(ExitCode::SUCCESS);
    }

    let cmd = args[0].as_str();
    let rest = &args[1..];
    match cmd {
        "usage" => cmd_usage(rest),
        "cost" => cmd_cost(rest),
        "config" => cmd_config(rest),
        other => Err(format!("unknown command '{other}' (try --help)")),
    }
}

fn print_help() {
    println!(
        "aiusagebar {VERSION} — AI provider usage from the command line\n\n\
USAGE:\n  \
aiusagebar <command> [options]\n\n\
COMMANDS:\n  \
usage    Fetch live usage for enabled providers\n  \
cost     Local token cost summary (Claude + Codex logs)\n  \
config   Inspect/modify provider config and API keys\n\n\
usage OPTIONS:\n  \
--provider <id|all>   Limit to one provider, or 'all' (default: enabled)\n  \
--format text|json    Output format (default: text)\n  \
--pretty              Pretty-print JSON\n  \
--status              Include provider service status\n\n\
cost OPTIONS:\n  \
--format text|json    Output format (default: text)\n  \
--pretty              Pretty-print JSON\n  \
--refresh             Ignore the cached scan\n\n\
config SUBCOMMANDS:\n  \
providers                              List providers and enabled state\n  \
enable  --provider <id>                Enable a provider\n  \
disable --provider <id>                Disable a provider\n  \
set-api-key --provider <id> [--stdin|--api-key <key>]   Store an API key\n  \
dump                                   Print normalized settings JSON\n\n\
GLOBAL:\n  \
-h, --help     Show help\n  \
-V, --version  Show version"
    );
}

// --- arg helpers -----------------------------------------------------------

fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn opt(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn parse_provider(s: &str) -> Result<ProviderId, String> {
    ProviderId::ALL
        .into_iter()
        .find(|p| p.as_str() == s)
        .ok_or_else(|| {
            let known: Vec<&str> = ProviderId::ALL.iter().map(|p| p.as_str()).collect();
            format!("unknown provider '{s}' (known: {})", known.join(", "))
        })
}

/// Resolve the provider set for `usage`: explicit `--provider`, `all`, or the
/// settings-enabled set.
fn resolve_providers(args: &[String]) -> Result<Vec<ProviderId>, String> {
    match opt(args, "--provider").as_deref() {
        Some("all") => Ok(ProviderId::ALL.to_vec()),
        Some(s) => Ok(vec![parse_provider(s)?]),
        None => {
            let s = settings::load();
            let enabled: Vec<ProviderId> = ProviderId::ALL
                .into_iter()
                .filter(|p| s.provider_enabled(*p))
                .collect();
            Ok(if enabled.is_empty() {
                ProviderId::ALL.to_vec()
            } else {
                enabled
            })
        }
    }
}

// --- usage -----------------------------------------------------------------

fn cmd_usage(args: &[String]) -> Result<ExitCode, String> {
    let providers = resolve_providers(args)?;
    let as_json = opt(args, "--format").as_deref() == Some("json");
    let pretty = flag(args, "--pretty");
    let want_status = flag(args, "--status");

    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    rt.block_on(async move {
        // Fetch all requested providers concurrently rather than serially, so a
        // multi-provider `usage` call is bounded by the slowest provider.
        let handles: Vec<_> = providers
            .iter()
            .map(|&id| (id, tokio::spawn(async move { providers::fetch_once(id).await })))
            .collect();
        let mut entries = Vec::new();
        for (id, handle) in handles {
            let result = handle
                .await
                .unwrap_or_else(|e| Err(providers::ProviderError::Other(e.to_string())));
            entries.push((id, result));
        }

        let incidents = if want_status {
            let http = reqwest::Client::new();
            status::fetch_many(&http, &providers).await
        } else {
            Vec::new()
        };

        if as_json {
            print_usage_json(&entries, &incidents, pretty);
        } else {
            print_usage_text(&entries, &incidents);
        }
    });

    Ok(ExitCode::SUCCESS)
}

fn print_usage_text(
    entries: &[(
        ProviderId,
        providers::ProviderResult<ai_usage_bar_lib::usage::types::UsageSnapshot>,
    )],
    incidents: &[status::Incident],
) {
    for (id, result) in entries {
        match result {
            Ok(snap) => {
                let plan = if snap.plan_label.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", snap.plan_label)
                };
                // Reuse the model's one-line presentation so the CLI never drifts
                // from the tray/menu wording. `show_remaining = false` → "used".
                let body = snap.mode.status_summary(false);
                let stale = if snap.stale { " [stale]" } else { "" };
                println!("{}{plan}: {body}{stale}", id.label());
            }
            Err(providers::ProviderError::Unauthenticated) => {
                println!("{}: not authenticated", id.label());
            }
            Err(e) => println!("{}: error — {e}", id.label()),
        }
    }
    for inc in incidents {
        if inc.severity.is_incident() {
            println!("⚠ {}: {}", inc.provider.label(), inc.description);
        }
    }
}

fn print_usage_json(
    entries: &[(
        ProviderId,
        providers::ProviderResult<ai_usage_bar_lib::usage::types::UsageSnapshot>,
    )],
    incidents: &[status::Incident],
    pretty: bool,
) {
    let snapshots: BTreeMap<&str, serde_json::Value> = entries
        .iter()
        .map(|(id, result)| {
            let val = match result {
                Ok(snap) => serde_json::to_value(snap).unwrap_or(serde_json::Value::Null),
                Err(e) => serde_json::json!({ "error": e.to_string() }),
            };
            (id.as_str(), val)
        })
        .collect();

    let out = serde_json::json!({
        "providers": snapshots,
        "incidents": incidents,
    });
    print_json(&out, pretty);
}

// --- cost ------------------------------------------------------------------

fn cmd_cost(args: &[String]) -> Result<ExitCode, String> {
    if flag(args, "--refresh") {
        cost::invalidate();
    }
    let as_json = opt(args, "--format").as_deref() == Some("json");
    let pretty = flag(args, "--pretty");

    let s = settings::load();
    let providers: Vec<ProviderId> = ProviderId::ALL
        .into_iter()
        .filter(|p| p.kind() == ProviderKind::Subscription && s.provider_enabled(*p))
        .collect();
    // Subscription providers always cost-scannable even if not "enabled" for the
    // tray; if none enabled, scan both.
    let providers = if providers.is_empty() {
        vec![ProviderId::Claude, ProviderId::Codex]
    } else {
        providers
    };

    let summary = cost::compute(&providers, s.cost_history_days());

    if as_json {
        print_json(
            &serde_json::to_value(&summary).unwrap_or(serde_json::Value::Null),
            pretty,
        );
    } else {
        println!(
            "Cost today: ${:.2}   ({}-day: ${:.2})",
            summary.total_today_usd, summary.window_days, summary.total_window_usd
        );
        for (id, pc) in &summary.providers {
            println!(
                "  {:<12} today ${:>8.2}  window ${:>9.2}  ({} tokens)",
                id.label(),
                pc.today_usd,
                pc.window_usd,
                pc.window_tokens
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

// --- config ----------------------------------------------------------------

fn cmd_config(args: &[String]) -> Result<ExitCode, String> {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("providers");
    let rest = if args.is_empty() { args } else { &args[1..] };
    match sub {
        "providers" => config_providers(),
        "enable" => config_set_enabled(rest, true),
        "disable" => config_set_enabled(rest, false),
        "set-api-key" => config_set_api_key(rest),
        "dump" => {
            let s = settings::load();
            print_json(
                &serde_json::to_value(&s).unwrap_or(serde_json::Value::Null),
                true,
            );
            Ok(ExitCode::SUCCESS)
        }
        other => Err(format!("unknown config subcommand '{other}'")),
    }
}

fn config_providers() -> Result<ExitCode, String> {
    let s = settings::load();
    for id in ProviderId::ALL {
        let enabled = if s.provider_enabled(id) { "on " } else { "off" };
        let kind = match id.kind() {
            ProviderKind::Subscription => "subscription",
            ProviderKind::ApiKeyCredits => "api-key",
        };
        let key = if id.kind() == ProviderKind::ApiKeyCredits {
            if api_key::load_key(id).is_some() {
                " [key stored]"
            } else {
                " [no key]"
            }
        } else {
            ""
        };
        println!("{enabled}  {:<11} {:<12}{key}", id.as_str(), kind);
    }
    Ok(ExitCode::SUCCESS)
}

fn config_set_enabled(args: &[String], enable: bool) -> Result<ExitCode, String> {
    let id = parse_provider(&opt(args, "--provider").ok_or("--provider <id> required")?)?;
    let mut s = settings::load();
    let mut set: Vec<ProviderId> = s.enabled_providers.clone();
    set.retain(|p| *p != id);
    if enable {
        set.push(id);
    }
    s.enabled_providers = set;
    settings::save(&s).map_err(|e| e.to_string())?;
    println!(
        "{} {}",
        id.as_str(),
        if enable { "enabled" } else { "disabled" }
    );
    Ok(ExitCode::SUCCESS)
}

fn config_set_api_key(args: &[String]) -> Result<ExitCode, String> {
    let id = parse_provider(&opt(args, "--provider").ok_or("--provider <id> required")?)?;
    if api_key::env_var(id).is_none() {
        return Err(format!("{} is not an API-key provider", id.as_str()));
    }
    let key = if flag(args, "--stdin") {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| e.to_string())?;
        buf.trim().to_string()
    } else if let Some(k) = opt(args, "--api-key") {
        k
    } else {
        return Err("provide the key via --stdin or --api-key <key>".to_string());
    };

    if key.is_empty() {
        return Err("empty API key".to_string());
    }
    api_key::store_key(id, Some(&key)).map_err(|e| e.to_string())?;

    // Match CodexBar: storing a key enables the provider by default.
    let mut s = settings::load();
    if !s.provider_enabled(id) {
        s.enabled_providers.push(id);
        settings::save(&s).map_err(|e| e.to_string())?;
    }
    println!("stored API key for {} and enabled it", id.as_str());
    Ok(ExitCode::SUCCESS)
}

// --- output ----------------------------------------------------------------

fn print_json(value: &serde_json::Value, pretty: bool) {
    let s = if pretty {
        serde_json::to_string_pretty(value)
    } else {
        serde_json::to_string(value)
    }
    .unwrap_or_else(|_| "{}".to_string());
    println!("{s}");
}
