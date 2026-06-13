//! Tauri IPC commands invoked from the React UI.

use crate::settings::Settings;
use crate::state::AppState;
use crate::usage::selector;
use crate::usage::types::{ProviderId, UsageSnapshot};
use serde::Serialize;
use std::collections::HashMap;
use tauri::State;

/// All current snapshots plus the resolved active provider, for the UI.
#[derive(Debug, Serialize)]
pub struct UsageReport {
    pub snapshots: HashMap<ProviderId, UsageSnapshot>,
    pub active: Option<ProviderId>,
    pub settings: Settings,
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    Ok(state.settings.lock().await.clone())
}

#[tauri::command]
pub async fn set_settings(state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
    crate::settings::save(&settings).map_err(|e| e.to_string())?;
    *state.settings.lock().await = settings;
    Ok(())
}

#[tauri::command]
pub async fn get_usage(state: State<'_, AppState>) -> Result<UsageReport, String> {
    let settings = state.settings.lock().await.clone();
    let aggregator = state.aggregator.lock().await;
    let snapshots = aggregator.all_cached().clone();
    let active = selector::resolve_active(&settings, &snapshots);
    Ok(UsageReport {
        snapshots,
        active,
        settings,
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
#[tauri::command]
pub async fn refresh_now(state: State<'_, AppState>) -> Result<UsageReport, String> {
    let settings = state.settings.lock().await.clone();
    {
        let mut aggregator = state.aggregator.lock().await;
        aggregator.poll_enabled(&settings).await;
    }
    get_usage(state).await
}
