//! Detail panel shown on tray left-click.
//!
//! On Windows this is a frameless floating panel positioned near the tray. On
//! macOS it behaves as a small popover anchored to the menu bar item. Both reuse
//! the same `panel` webview window declared in `tauri.conf.json`; only the
//! positioning differs by platform.

use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_positioner::{Position, WindowExt};

/// Toggle the detail panel's visibility, positioning it appropriately for the
/// current platform before showing.
pub fn toggle<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let Some(window) = app.get_webview_window("panel") else {
        return Ok(());
    };

    if window.is_visible().unwrap_or(false) {
        window.hide()?;
        return Ok(());
    }

    // Position near the tray icon. On macOS the menu bar is at the top; on
    // Windows the notification area is bottom-right.
    #[cfg(target_os = "macos")]
    let _ = window.move_window(Position::TrayCenter);
    #[cfg(target_os = "windows")]
    let _ = window.move_window(Position::TrayBottomCenter);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = window.move_window(Position::TopRight);

    window.show()?;
    window.set_focus()?;
    Ok(())
}
