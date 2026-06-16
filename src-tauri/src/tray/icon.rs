//! Renders the tray icon bitmap: either compact numbers or dual progress bars,
//! colored by utilization thresholds.
//!
//! The output is a 36×36 RGBA buffer (a comfortable size for both the macOS
//! menu bar at 2x and the Windows notification area, downscaled by the OS).

use crate::settings::{DisplayStyle, Settings, Thresholds};
use crate::tray::theme::Appearance;
use crate::usage::types::{DisplayMode, ProviderId, UsageSnapshot};
use ab_glyph::{Font, FontRef, PxScale};
use image::{imageops, Rgba, RgbaImage};

const SIZE: u32 = 36;

/// Square size of the provider-glyph tray icon. The menu bar renders this
/// alongside the percentage *title* (see `update_tray`), matching the reference
/// of "provider mark + 5h %".
const GLYPH_SIZE: u32 = 36;

/// Embedded compact font for number rendering.
static FONT_BYTES: &[u8] = include_bytes!("../../assets/fonts/Inter-Bold.ttf");

/// Result of rendering: raw RGBA + dimensions, ready for `tauri::image::Image`.
pub struct RenderedIcon {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// On macOS, whether the icon should be set as a template (monochrome).
    pub is_template: bool,
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
fn threshold_color(util: f32, t: &Thresholds) -> Rgba<u8> {
    if util >= t.danger {
        Rgba([0xD9, 0x4A, 0x3A, 0xFF]) // red
    } else if util >= t.warn {
        Rgba([0xE0, 0xA9, 0x2E, 0xFF]) // amber
    } else {
        Rgba([0x4C, 0xAF, 0x50, 0xFF]) // green
    }
}

/// Render the tray icon for the active provider's snapshot.
///
/// Used on Windows/Linux, which have no separate tray title — the percentage is
/// baked into the bitmap. macOS instead uses `render_provider_glyph` + a text
/// title, so this whole path (and its helpers) is dead code there.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn render_icon(
    snapshot: &UsageSnapshot,
    settings: &Settings,
    appearance: Appearance,
) -> RenderedIcon {
    let mut img = RgbaImage::new(SIZE, SIZE);

    match settings.display_style {
        DisplayStyle::Bars => render_bars(&mut img, snapshot, settings),
        DisplayStyle::Numbers => render_numbers(&mut img, snapshot, settings, appearance),
    }

    // macOS template icons must be monochrome; the progress-bar style is
    // intentionally colored, so it is never a template. Numbers in light/dark
    // adapt via template on macOS.
    let is_template = cfg!(target_os = "macos") && settings.display_style == DisplayStyle::Numbers;

    RenderedIcon {
        rgba: img.into_raw(),
        width: SIZE,
        height: SIZE,
        is_template,
    }
}

/// Raw provider-mark PNG (white-on-transparent or color) used as the source for
/// the template glyph. We only consume its alpha channel, so the fill color
/// doesn't matter — the silhouette is what we tint.
fn provider_glyph_png(provider: ProviderId) -> &'static [u8] {
    match provider {
        ProviderId::Claude => include_bytes!("../../icons/providers/claude-color.png"),
        // API-key providers have no bundled brand mark yet; reuse the neutral
        // Codex silhouette so the tray still renders a glyph.
        _ => include_bytes!("../../icons/providers/codex-white.png"),
    }
}

/// Render the active provider's logo as a monochrome template glyph for the menu
/// bar. The PNG's alpha channel is used as a mask and painted in the foreground
/// color; on macOS the result is flagged as a template so the system tints it
/// for light/dark menu bars. The 5h percentage is shown separately as the tray
/// *title* (see `update_tray`), so this icon carries no text.
pub fn render_provider_glyph(provider: ProviderId, appearance: Appearance) -> RenderedIcon {
    let fg = appearance.foreground();
    let mut out = RgbaImage::new(GLYPH_SIZE, GLYPH_SIZE);

    if let Ok(src) = image::load_from_memory(provider_glyph_png(provider)) {
        // Fit the mark inside a small inset so it reads cleanly at menu-bar size.
        let inset = 3u32;
        let target = GLYPH_SIZE - inset * 2;
        let scaled = imageops::resize(
            &src.to_rgba8(),
            target,
            target,
            imageops::FilterType::Lanczos3,
        );

        // Some brand PNGs (e.g. claude-color.png) are fully opaque with the mark
        // drawn in dark ink on a light field, so the alpha channel can't be used
        // as a silhouette mask — it would paint the whole square. Detect that and
        // fall back to a luminance-derived mask (dark pixels = mark) for opaque
        // images, while transparent silhouettes (codex-*) keep using alpha.
        let opaque = scaled.pixels().all(|p| p.0[3] == 255);
        for (x, y, px) in scaled.enumerate_pixels() {
            let mask = if opaque {
                // Rec. 601 luma; invert so dark ink → high coverage.
                let [r, g, b, _] = px.0;
                let luma = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
                255u8.saturating_sub(luma.round() as u8)
            } else {
                px.0[3]
            };
            if mask == 0 {
                continue;
            }
            out.put_pixel(x + inset, y + inset, Rgba([fg[0], fg[1], fg[2], mask]));
        }
    }

    RenderedIcon {
        rgba: out.into_raw(),
        width: GLYPH_SIZE,
        height: GLYPH_SIZE,
        // Template on macOS so the system handles light/dark inversion.
        is_template: cfg!(target_os = "macos"),
    }
}

/// Two stacked horizontal bars (primary on top, secondary below), each colored
/// by its own utilization. Spend-cap renders a single centered bar.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn render_bars(img: &mut RgbaImage, snapshot: &UsageSnapshot, settings: &Settings) {
    let track = Rgba([0x44, 0x44, 0x44, 0xFF]);
    let pad = 4u32;
    let bar_h = 9u32;
    let gap = 4u32;
    let w = SIZE - pad * 2;

    let draw_bar = |img: &mut RgbaImage, y: u32, util: f32| {
        // track
        fill_rect(img, pad, y, w, bar_h, track);
        let filled = ((util.clamp(0.0, 100.0) / 100.0) * w as f32).round() as u32;
        if filled > 0 {
            fill_rect(
                img,
                pad,
                y,
                filled,
                bar_h,
                threshold_color(util, &settings.thresholds),
            );
        }
    };

    match &snapshot.mode {
        DisplayMode::Session { primary, secondary } => {
            let total_h = bar_h * 2 + gap;
            let y0 = (SIZE - total_h) / 2;
            draw_bar(img, y0, primary.utilization);
            let sec_util = secondary.as_ref().map(|s| s.utilization).unwrap_or(0.0);
            draw_bar(img, y0 + bar_h + gap, sec_util);
        }
        DisplayMode::SpendCap { utilization, .. } => {
            let y0 = (SIZE - bar_h) / 2;
            draw_bar(img, y0, *utilization);
        }
        DisplayMode::CreditBalance { .. }
        | DisplayMode::Unauthenticated
        | DisplayMode::ApiKeyOnly => {
            // No percentage to chart (credit balance / key-only / signed-out):
            // a single dim track stands in for "no usage bar".
            let y0 = (SIZE - bar_h) / 2;
            fill_rect(img, pad, y0, w, bar_h, track);
        }
    }
}

/// Compact two-line numbers, e.g. "42" over "18". Spend-cap shows a single value.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn render_numbers(
    img: &mut RgbaImage,
    snapshot: &UsageSnapshot,
    settings: &Settings,
    appearance: Appearance,
) {
    let font = FontRef::try_from_slice(FONT_BYTES).expect("embedded font");
    let fg = Rgba(appearance.foreground());

    let (top, bottom): (String, Option<String>) = match &snapshot.mode {
        DisplayMode::Session { primary, secondary } => {
            let p = settings.display_pct(primary.utilization).round() as i32;
            let s = secondary
                .as_ref()
                .map(|w| settings.display_pct(w.utilization).round() as i32);
            (format!("{p}"), s.map(|v| format!("{v}")))
        }
        DisplayMode::SpendCap { utilization, .. } => {
            let v = settings.display_pct(*utilization).round() as i32;
            (format!("{v}"), Some("cap".to_string()))
        }
        // Credit balance: bake the dollar figure into the icon. Use the same
        // formatter as the tray title so macOS-Numbers (which shows both the
        // baked glyph *and* a title) renders one consistent value, e.g.
        // "$18.50" — not "$18" beside "$18.50".
        DisplayMode::CreditBalance { balance_cents } => {
            (crate::usage::types::format_usd_cents(*balance_cents), None)
        }
        DisplayMode::Unauthenticated => ("—".to_string(), None),
        DisplayMode::ApiKeyOnly => ("key".to_string(), None),
    };

    match bottom {
        Some(bottom) => {
            draw_text_centered(img, &font, &top, 17.0, 2.0, fg);
            draw_text_centered(img, &font, &bottom, 17.0, 18.0, fg);
        }
        None => {
            draw_text_centered(img, &font, &top, 22.0, 7.0, fg);
        }
    }
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
fn fill_rect(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, color: Rgba<u8>) {
    for dy in 0..h {
        for dx in 0..w {
            let px = x + dx;
            let py = y + dy;
            if px < img.width() && py < img.height() {
                img.put_pixel(px, py, color);
            }
        }
    }
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
fn draw_text_centered(
    img: &mut RgbaImage,
    font: &FontRef,
    text: &str,
    px: f32,
    y_top: f32,
    color: Rgba<u8>,
) {
    let scale = PxScale::from(px);
    let scaled = font.as_scaled(scale);
    use ab_glyph::ScaleFont;

    // Measure width to center horizontally.
    let mut width = 0.0f32;
    for c in text.chars() {
        let g = scaled.scaled_glyph(c);
        width += scaled.h_advance(g.id);
    }
    let mut x = (SIZE as f32 - width) / 2.0;

    for c in text.chars() {
        let glyph = scaled.scaled_glyph(c);
        let h_advance = scaled.h_advance(glyph.id);
        if let Some(outlined) = font.outline_glyph(ab_glyph::Glyph {
            id: glyph.id,
            scale,
            position: ab_glyph::point(x, y_top + scaled.ascent()),
        }) {
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, coverage| {
                let ix = bounds.min.x as i32 + gx as i32;
                let iy = bounds.min.y as i32 + gy as i32;
                if ix >= 0 && iy >= 0 && (ix as u32) < img.width() && (iy as u32) < img.height() {
                    blend_pixel(img, ix as u32, iy as u32, color, coverage);
                }
            });
        }
        x += h_advance;
    }
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
fn blend_pixel(img: &mut RgbaImage, x: u32, y: u32, color: Rgba<u8>, coverage: f32) {
    let a = coverage.clamp(0.0, 1.0);
    let existing = img.get_pixel(x, y).0;
    let out = [
        blend(existing[0], color.0[0], a),
        blend(existing[1], color.0[1], a),
        blend(existing[2], color.0[2], a),
        ((existing[3] as f32) * (1.0 - a) + 255.0 * a) as u8,
    ];
    img.put_pixel(x, y, Rgba(out));
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
fn blend(bg: u8, fg: u8, a: f32) -> u8 {
    ((bg as f32) * (1.0 - a) + (fg as f32) * a).round() as u8
}

/// The provider logo bytes (PNG) for use as an overlay/badge in the detail UI.
/// Tray icons stay text/bar-based for legibility at small sizes.
#[allow(dead_code)] // consumed by the detail-panel asset endpoint
pub fn provider_logo_png(provider: ProviderId, appearance: Appearance) -> &'static [u8] {
    match (provider, appearance) {
        (ProviderId::Claude, _) => include_bytes!("../../icons/providers/claude-color.png"),
        (_, Appearance::Light) => include_bytes!("../../icons/providers/codex-black.png"),
        (_, Appearance::Dark) => include_bytes!("../../icons/providers/codex-white.png"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::types::WindowUsage;

    fn snap(mode: DisplayMode) -> UsageSnapshot {
        UsageSnapshot {
            provider: ProviderId::Claude,
            plan_label: "max".into(),
            mode,
            fetched_at: chrono::Utc::now(),
            stale: false,
            extras: Default::default(),
        }
    }

    #[test]
    fn renders_bars_to_expected_size() {
        let s = snap(DisplayMode::Session {
            primary: WindowUsage::new(42.0, "5h", None),
            secondary: Some(WindowUsage::new(18.0, "Week", None)),
        });
        let settings = Settings {
            display_style: DisplayStyle::Bars,
            ..Default::default()
        };
        let out = render_icon(&s, &settings, Appearance::Dark);
        assert_eq!(out.width, SIZE);
        assert_eq!(out.height, SIZE);
        assert_eq!(out.rgba.len() as u32, SIZE * SIZE * 4);
        assert!(!out.is_template);
    }

    #[test]
    fn numbers_template_on_macos() {
        let s = snap(DisplayMode::Session {
            primary: WindowUsage::new(5.0, "5h", None),
            secondary: None,
        });
        let settings = Settings::default(); // Numbers by default
        let out = render_icon(&s, &settings, Appearance::Dark);
        assert_eq!(out.is_template, cfg!(target_os = "macos"));
    }

    #[test]
    fn threshold_colors() {
        let t = Thresholds::default();
        assert_eq!(threshold_color(10.0, &t).0, [0x4C, 0xAF, 0x50, 0xFF]);
        assert_eq!(threshold_color(60.0, &t).0, [0xE0, 0xA9, 0x2E, 0xFF]);
        assert_eq!(threshold_color(90.0, &t).0, [0xD9, 0x4A, 0x3A, 0xFF]);
    }
}
