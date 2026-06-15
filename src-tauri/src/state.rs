//! Shared application state, held in Tauri's managed state.

use crate::notify::NotifyState;
use crate::settings::Settings;
use crate::status::Incident;
use crate::usage::aggregator::Aggregator;
use tokio::sync::Mutex;

pub struct AppState {
    pub settings: Mutex<Settings>,
    pub aggregator: Mutex<Aggregator>,
    /// Per-provider memory for edge-triggered quota notifications.
    pub notify: Mutex<NotifyState>,
    /// Latest provider service-status incidents, refreshed by the poll loop
    /// when `check_provider_status` is enabled. Empty when disabled or all
    /// providers are operational.
    pub incidents: Mutex<Vec<Incident>>,
    /// Shared HTTP client, used for status polling and ad-hoc fetches.
    pub http: reqwest::Client,
}

impl AppState {
    pub fn new(settings: Settings, aggregator: Aggregator, http: reqwest::Client) -> Self {
        Self {
            settings: Mutex::new(settings),
            aggregator: Mutex::new(aggregator),
            notify: Mutex::new(NotifyState::default()),
            incidents: Mutex::new(Vec::new()),
            http,
        }
    }
}
