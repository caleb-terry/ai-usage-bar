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

/// UI display language. `System` follows the OS locale; otherwise an explicit
/// locale. Only English ships today, but the enum is the extension point so the
/// stored preference is forward-compatible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Language {
    #[default]
    System,
    En,
}

/// Terminal application launched by the "Open Terminal" action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TerminalApp {
    /// macOS Terminal.app / the platform default terminal.
    #[default]
    Terminal,
    // kebab-case would serialize this as "i-term"; the frontend sends "iterm",
    // so pin the wire value explicitly or the whole settings payload fails to
    // deserialize when iTerm is selected.
    #[serde(rename = "iterm")]
    ITerm,
    Warp,
    Ghostty,
}

impl TerminalApp {
    /// The macOS application name passed to `open -a`.
    pub fn macos_app(self) -> &'static str {
        match self {
            TerminalApp::Terminal => "Terminal",
            TerminalApp::ITerm => "iTerm",
            TerminalApp::Warp => "Warp",
            TerminalApp::Ghostty => "Ghostty",
        }
    }
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
    /// UI display language. Takes effect after the window reloads.
    pub language: Language,
    /// Terminal launched by the "Open Terminal" action.
    pub default_terminal: TerminalApp,
    /// Show the local cost summary (today + history window) in the menu/panel.
    pub show_cost_summary: bool,
    /// History window for the cost summary, in days (clamped 1..=90).
    pub cost_history_days: u32,
    /// Poll provider status pages and surface incidents in the icon/menu.
    pub check_provider_status: bool,
    /// Notify when the 5-hour session quota hits 0% and when it returns.
    pub session_quota_notifications: bool,
    /// Warn when session/weekly remaining crosses the configured thresholds.
    pub quota_warning_notifications: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            // Default to the two zero-config subscription providers (creds are
            // auto-detected from the CLIs). API-key providers stay off until the
            // user adds a key, so first run doesn't fetch six unauthenticated
            // endpoints.
            enabled_providers: vec![ProviderId::Claude, ProviderId::Codex],
            active_provider: ActiveProvider::Auto,
            display_style: DisplayStyle::Numbers,
            show_remaining: false,
            poll_interval_secs: 180,
            thresholds: Thresholds::default(),
            windows_float_panel: true,
            launch_at_login: false,
            language: Language::default(),
            default_terminal: TerminalApp::default(),
            show_cost_summary: true,
            cost_history_days: 30,
            check_provider_status: true,
            session_quota_notifications: true,
            quota_warning_notifications: true,
        }
    }
}

impl Settings {
    pub fn poll_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.poll_interval_secs.clamp(60, 900))
    }

    /// Cost-summary history window in days, clamped to a sane 1..=90 range.
    pub fn cost_history_days(&self) -> u32 {
        self.cost_history_days.clamp(1, 90)
    }

    pub fn provider_enabled(&self, id: ProviderId) -> bool {
        self.enabled_providers.contains(&id)
    }

    /// Apply the show-remaining preference to a raw utilization. Delegates to
    /// the single implementation in `usage::types` so the tray figure and the
    /// settings figure can never drift.
    pub fn display_pct(&self, utilization: f32) -> f32 {
        crate::usage::types::display_pct(utilization, self.show_remaining)
    }
}

/// Absolute path of the persisted settings file. Crate-visible so the
/// first-run check in `lib.rs` keys off the same path rather than rebuilding it.
pub(crate) fn settings_path() -> PathBuf {
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
    fn cost_history_days_is_clamped() {
        let mut s = Settings {
            cost_history_days: 0,
            ..Default::default()
        };
        assert_eq!(s.cost_history_days(), 1);
        s.cost_history_days = 9999;
        assert_eq!(s.cost_history_days(), 90);
        s.cost_history_days = 30;
        assert_eq!(s.cost_history_days(), 30);
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
            language: Language::En,
            default_terminal: TerminalApp::Ghostty,
            show_cost_summary: false,
            cost_history_days: 7,
            check_provider_status: false,
            session_quota_notifications: false,
            quota_warning_notifications: false,
            ..Default::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }
}
