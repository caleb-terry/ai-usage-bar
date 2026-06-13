//! The tray right-click context menu.

use crate::settings::{ActiveProvider, DisplayStyle, Settings};
use crate::usage::types::ProviderId;
use tauri::menu::{CheckMenuItemBuilder, Menu, MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Runtime};

/// Stable menu item ids referenced by the event handler in `lib.rs`.
pub mod ids {
    pub const PROVIDER_AUTO: &str = "provider_auto";
    pub const PROVIDER_CLAUDE: &str = "provider_claude";
    pub const PROVIDER_CODEX: &str = "provider_codex";
    pub const STYLE_NUMBERS: &str = "style_numbers";
    pub const STYLE_BARS: &str = "style_bars";
    pub const TOGGLE_REMAINING: &str = "toggle_remaining";
    pub const REFRESH: &str = "refresh";
    pub const SETTINGS: &str = "settings";
    pub const LAUNCH_AT_LOGIN: &str = "launch_at_login";
    pub const QUIT: &str = "quit";
}

/// Build the full context menu reflecting the current settings as checkmarks.
pub fn build_menu<R: Runtime>(
    app: &AppHandle<R>,
    settings: &Settings,
    status_line: &str,
) -> tauri::Result<Menu<R>> {
    // Non-interactive status header (e.g. "Claude Code · max · 5h 42% · Wk 18%").
    let header = MenuItemBuilder::with_id("header", status_line)
        .enabled(false)
        .build(app)?;

    // Provider submenu.
    let provider_submenu = SubmenuBuilder::new(app, "Provider")
        .item(
            &CheckMenuItemBuilder::with_id(ids::PROVIDER_AUTO, "Auto (highest usage)")
                .checked(settings.active_provider == ActiveProvider::Auto)
                .build(app)?,
        )
        .item(
            &CheckMenuItemBuilder::with_id(ids::PROVIDER_CLAUDE, ProviderId::Claude.label())
                .checked(settings.active_provider == ActiveProvider::Claude)
                .build(app)?,
        )
        .item(
            &CheckMenuItemBuilder::with_id(ids::PROVIDER_CODEX, ProviderId::Codex.label())
                .checked(settings.active_provider == ActiveProvider::Codex)
                .build(app)?,
        )
        .build()?;

    // Display-style submenu.
    let style_submenu = SubmenuBuilder::new(app, "Display style")
        .item(
            &CheckMenuItemBuilder::with_id(ids::STYLE_NUMBERS, "Numbers")
                .checked(settings.display_style == DisplayStyle::Numbers)
                .build(app)?,
        )
        .item(
            &CheckMenuItemBuilder::with_id(ids::STYLE_BARS, "Progress bars")
                .checked(settings.display_style == DisplayStyle::Bars)
                .build(app)?,
        )
        .build()?;

    let toggle_remaining = CheckMenuItemBuilder::with_id(ids::TOGGLE_REMAINING, "Show remaining %")
        .checked(settings.show_remaining)
        .build(app)?;

    let launch_at_login = CheckMenuItemBuilder::with_id(ids::LAUNCH_AT_LOGIN, "Launch at login")
        .checked(settings.launch_at_login)
        .build(app)?;

    let refresh = MenuItemBuilder::with_id(ids::REFRESH, "Refresh now").build(app)?;
    let settings_item = MenuItemBuilder::with_id(ids::SETTINGS, "Settings…").build(app)?;
    let quit = MenuItemBuilder::with_id(ids::QUIT, "Quit AI Usage Bar").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&header)
        .separator()
        .item(&provider_submenu)
        .item(&style_submenu)
        .item(&toggle_remaining)
        .separator()
        .item(&refresh)
        .item(&settings_item)
        .item(&launch_at_login)
        .separator()
        .item(&quit)
        .build()?;

    Ok(menu)
}
