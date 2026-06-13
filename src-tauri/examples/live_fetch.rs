//! Manual live smoke test: fetch real usage for one or both providers using the
//! local CLI credentials, and print the normalized snapshot.
//!
//! Usage:
//!   cargo run --example live_fetch            # both providers
//!   cargo run --example live_fetch codex      # one provider
//!
//! Note: Claude credentials live in the macOS Keychain, so the first run may
//! trigger a one-time keychain authorization prompt — click "Always Allow".

use ai_usage_bar_lib::providers::fetch_once;
use ai_usage_bar_lib::usage::types::ProviderId;

#[tokio::main]
async fn main() {
    let which: Vec<ProviderId> = match std::env::args().nth(1).as_deref() {
        Some("claude") => vec![ProviderId::Claude],
        Some("codex") => vec![ProviderId::Codex],
        _ => ProviderId::ALL.to_vec(),
    };

    for id in which {
        println!("\n=== {} ===", id.label());
        match fetch_once(id).await {
            Ok(snap) => {
                println!("plan: {}", snap.plan_label);
                println!("mode: {:#?}", snap.mode);
                if let Some(peak) = snap.peak_utilization() {
                    println!("peak utilization: {peak:.1}%");
                }
                println!("extras: {:?}", snap.extras);
            }
            Err(e) => println!("error: {e}"),
        }
    }
}
