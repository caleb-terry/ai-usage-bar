//! Light/dark appearance detection for tray icon rendering.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Appearance {
    // `Light` is only constructed on Windows; macOS uses template icons.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    Light,
    Dark,
}

impl Appearance {
    /// The foreground color to draw glyphs/bars with for contrast against the
    /// menu bar / tray background.
    pub fn foreground(self) -> [u8; 4] {
        match self {
            // On a light menu bar, draw dark. On a dark bar, draw light.
            Appearance::Light => [0, 0, 0, 255],
            Appearance::Dark => [255, 255, 255, 255],
        }
    }
}

/// Detect the current system appearance.
///
/// On macOS the tray icon is a template (auto-inverted by the system), so this
/// is mainly used by Windows and by the non-template number rendering.
#[cfg(target_os = "macos")]
pub fn detect() -> Appearance {
    // macOS template icons adapt automatically; default to Dark foreground
    // (white) which the system inverts as needed. We still report Dark so the
    // non-template paths render light glyphs.
    Appearance::Dark
}

#[cfg(target_os = "windows")]
pub fn detect() -> Appearance {
    use windows_registry::CURRENT_USER;
    // AppsUseLightTheme: 1 = light apps, 0 = dark.
    let key = CURRENT_USER.open(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    if let Ok(key) = key {
        if let Ok(val) = key.get_u32("SystemUsesLightTheme") {
            return if val == 1 {
                Appearance::Light
            } else {
                Appearance::Dark
            };
        }
    }
    Appearance::Dark
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn detect() -> Appearance {
    Appearance::Dark
}
