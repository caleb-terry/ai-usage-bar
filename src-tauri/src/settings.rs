//! User-configurable, locally-persisted settings.
//!
//! Stored as JSON in the platform config directory. The frontend reads and
//! writes these via Tauri IPC commands; the tray and scheduler observe them.

use crate::usage::types::ProviderId;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Which provider the single tray icon currently displays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ActiveProvider {
    /// Pick whichever enabled provider has the highest peak utilization.
    #[default]
    Auto,
    Claude,
    Codex,
}

/// How the tray icon presents usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DisplayStyle {
    #[default]
    Numbers,
    Bars,
}

/// Green/yellow/red thresholds for utilization coloring.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Thresholds {
    /// Below this is green.
    pub warn: f32,
    /// At/above this is red; between warn..danger is yellow.
    pub danger: f32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            warn: 50.0,
            danger: 80.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Providers the user has enabled. Auto-populated on first run from which
    /// credentials are detected.
    pub enabled_providers: Vec<ProviderId>,
    pub active_provider: ActiveProvider,
    pub display_style: DisplayStyle,
    /// When true, the icon/detail shows remaining % instead of used %.
    pub show_remaining: bool,
    /// Poll interval in seconds (clamped 60..=900).
    pub poll_interval_secs: u64,
    pub thresholds: Thresholds,
    /// Windows-only: open the floating panel on left-click (vs. settings).
    pub windows_float_panel: bool,
    pub launch_at_login: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled_providers: ProviderId::ALL.to_vec(),
            active_provider: ActiveProvider::Auto,
            display_style: DisplayStyle::Numbers,
            show_remaining: false,
            poll_interval_secs: 180,
            thresholds: Thresholds::default(),
            windows_float_panel: true,
            launch_at_login: false,
        }
    }
}

impl Settings {
    pub fn poll_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.poll_interval_secs.clamp(60, 900))
    }

    pub fn provider_enabled(&self, id: ProviderId) -> bool {
        self.enabled_providers.contains(&id)
    }

    /// Apply the show-remaining preference to a raw utilization.
    pub fn display_pct(&self, utilization: f32) -> f32 {
        if self.show_remaining {
            100.0 - utilization
        } else {
            utilization
        }
    }
}

fn settings_path() -> PathBuf {
    let dir = directories::ProjectDirs::from("dev", "calebterry", "ai-usage-bar")
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    dir.join("settings.json")
}

pub fn load() -> Settings {
    let path = settings_path();
    match std::fs::read_to_string(&path) {
        Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

pub fn save(settings: &Settings) -> std::io::Result<()> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_enable_both_providers() {
        let s = Settings::default();
        assert!(s.provider_enabled(ProviderId::Claude));
        assert!(s.provider_enabled(ProviderId::Codex));
    }

    #[test]
    fn poll_interval_is_clamped() {
        let mut s = Settings {
            poll_interval_secs: 5,
            ..Default::default()
        };
        assert_eq!(s.poll_interval().as_secs(), 60);
        s.poll_interval_secs = 99999;
        assert_eq!(s.poll_interval().as_secs(), 900);
    }

    #[test]
    fn display_pct_respects_remaining() {
        let mut s = Settings::default();
        assert_eq!(s.display_pct(30.0), 30.0);
        s.show_remaining = true;
        assert_eq!(s.display_pct(30.0), 70.0);
    }

    #[test]
    fn roundtrips_through_json() {
        let s = Settings {
            show_remaining: true,
            poll_interval_secs: 120,
            active_provider: ActiveProvider::Claude,
            display_style: DisplayStyle::Bars,
            ..Default::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }
}
