//! Shared application state, held in Tauri's managed state.

use crate::settings::Settings;
use crate::usage::aggregator::Aggregator;
use tokio::sync::Mutex;

pub struct AppState {
    pub settings: Mutex<Settings>,
    pub aggregator: Mutex<Aggregator>,
    /// Shared HTTP client; retained for ad-hoc fetches (e.g. manual re-auth).
    #[allow(dead_code)]
    pub http: reqwest::Client,
}

impl AppState {
    pub fn new(settings: Settings, aggregator: Aggregator, http: reqwest::Client) -> Self {
        Self {
            settings: Mutex::new(settings),
            aggregator: Mutex::new(aggregator),
            http,
        }
    }
}
