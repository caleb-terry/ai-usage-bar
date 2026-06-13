//! System tray / menu bar presentation.

pub mod icon;
pub mod menu;
pub mod theme;

#[cfg_attr(target_os = "macos", allow(unused_imports))]
pub use icon::render_icon;
pub use icon::render_provider_glyph;
