//! Tauri IPC commands invoked from the React UI.

use crate::settings::Settings;
use crate::state::AppState;
use crate::status::Incident;
use crate::usage::selector;
use crate::usage::types::{ProviderId, UsageSnapshot};
use serde::Serialize;
use std::collections::HashMap;
use tauri::{AppHandle, State};

/// All current snapshots plus the resolved active provider, for the UI.
#[derive(Debug, Serialize)]
pub struct UsageReport {
    pub snapshots: HashMap<ProviderId, UsageSnapshot>,
    pub active: Option<ProviderId>,
    pub settings: Settings,
    /// Active provider service-status incidents (empty when all operational or
    /// status polling is disabled).
    pub incidents: Vec<Incident>,
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    Ok(state.settings.lock().await.clone())
}

/// Persist a full settings object from the Settings UI. Routes through the same
/// canonical `apply_settings` path the tray menu uses, so a change made in the
/// window immediately syncs autostart, re-renders the tray, and re-emits
/// `usage-updated` — rather than silently waiting for the next poll.
#[tauri::command]
pub async fn set_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    crate::apply_settings(&app, settings).await;
    Ok(())
}

#[tauri::command]
pub async fn get_usage(state: State<'_, AppState>) -> Result<UsageReport, String> {
    let settings = state.settings.lock().await.clone();
    let snapshots = state.aggregator.lock().await.all_cached().clone();
    let active = selector::resolve_active(&settings, &snapshots);
    let incidents = state.incidents.lock().await.clone();
    Ok(UsageReport {
        snapshots,
        active,
        settings,
        incidents,
    })
}

/// Launch the user's preferred terminal application. Best-effort; see
/// `crate::launch_terminal`.
#[tauri::command]
pub async fn open_terminal(state: State<'_, AppState>) -> Result<(), String> {
    let terminal = state.settings.lock().await.default_terminal;
    crate::launch_terminal(terminal);
    Ok(())
}

/// Force an immediate poll of all enabled providers, returning the fresh report.
/// Also busts the local cost-scan cache so the next cost read rescans logs.
#[tauri::command]
pub async fn refresh_now(state: State<'_, AppState>) -> Result<UsageReport, String> {
    let settings = state.settings.lock().await.clone();
    {
        let mut aggregator = state.aggregator.lock().await;
        aggregator.poll_enabled(&settings).await;
    }
    crate::cost::invalidate();
    get_usage(state).await
}

/// Store (or clear) the API key for an API-key provider, then bust any cached
/// usage so the next poll picks up the new credential. Passing an empty string
/// clears the key. Returns an error for subscription providers (Claude/Codex),
/// which authenticate via their CLI, not a stored key.
#[tauri::command]
pub async fn set_api_key(
    state: State<'_, AppState>,
    provider: ProviderId,
    key: String,
) -> Result<(), String> {
    if crate::providers::api_key::env_var(provider).is_none() {
        return Err(format!("{} is not an API-key provider", provider.as_str()));
    }
    let trimmed = key.trim();
    crate::providers::api_key::store_key(
        provider,
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        },
    )
    .map_err(|e| e.to_string())?;

    // Re-poll so the UI reflects the change immediately.
    let settings = state.settings.lock().await.clone();
    state.aggregator.lock().await.poll_enabled(&settings).await;
    Ok(())
}

/// Report which API-key providers currently have a key stored (so the settings
/// UI can show a "configured" state without ever returning the secret).
#[tauri::command]
pub async fn api_key_status(
    _state: State<'_, AppState>,
) -> Result<HashMap<ProviderId, bool>, String> {
    let mut out = HashMap::new();
    for id in ProviderId::ALL {
        if crate::providers::api_key::env_var(id).is_some() {
            out.insert(id, crate::providers::api_key::load_key(id).is_some());
        }
    }
    Ok(out)
}

/// Compute (or reuse a cached) local cost summary from session logs.
///
/// Returns `None` when the user has the cost summary disabled, so the UI can
/// skip rendering the section entirely rather than show a zeroed-out card.
/// Scanning reads many files, so it runs on a blocking thread.
#[tauri::command]
pub async fn get_cost_summary(
    state: State<'_, AppState>,
) -> Result<Option<crate::cost::CostSummary>, String> {
    let settings = state.settings.lock().await.clone();
    if !settings.show_cost_summary {
        return Ok(None);
    }
    let providers = settings.enabled_providers.clone();
    let days = settings.cost_history_days();
    let summary =
        tauri::async_runtime::spawn_blocking(move || crate::cost::summary_cached(&providers, days))
            .await
            .map_err(|e| e.to_string())?;
    Ok(Some(summary))
}
