//! Terminal snapshot and theme color adaptation outside the terminal lock.
//! Palette lookup and contrast/remapping rules remain owned by their existing resolvers.

use gpui::{App, Rgba};

use super::Palette;

fn rgba_channels(color: Rgba) -> (u8, u8, u8) {
    (
        (color.r * 255.0).round() as u8,
        (color.g * 255.0).round() as u8,
        (color.b * 255.0).round() as u8,
    )
}

pub(super) fn is_default_host_cursor(palette: &Palette) -> bool {
    rgba_channels(palette.cursor) == default_cursor_rgb()
}

pub(super) fn is_default_selection(palette: &Palette) -> bool {
    rgba_channels(palette.selection) == default_cursor_rgb()
        && (palette.selection.a - 0.60).abs() < 0.05
}

fn default_cursor_rgb() -> (u8, u8, u8) {
    match crate::config::color::NEBULA_DEFAULT_CURSOR.background {
        crate::display::color::CellRgb::Rgb(rgb) => (rgb.r, rgb.g, rgb.b),
        _ => (0x49, 0x4d, 0x72),
    }
}

pub(in crate::gpui_shell::terminal) fn rgb_from_rgba(color: Rgba) -> crate::display::color::Rgb {
    let (r, g, b) = rgba_channels(color);
    crate::display::color::Rgb::new(r, g, b)
}

pub(in crate::gpui_shell::terminal) fn rgba_rgb(
    color: crate::display::color::Rgb,
    alpha: f32,
) -> Rgba {
    Rgba {
        r: f32::from(color.r) / 255.0,
        g: f32::from(color.g) / 255.0,
        b: f32::from(color.b) / 255.0,
        a: alpha,
    }
}

pub(super) fn themed_anchor(palette: &Palette, cx: &App) -> (crate::display::color::Rgb, bool) {
    let sk = crate::gpui_shell::theme::resolved_skin(cx);
    // ANSI magenta = index 5；旧壳 `display.colors[NamedColor::Magenta]`。
    let magenta = rgb_from_rgba(palette.ansi[5]);
    let mix = if sk.is_light {
        crate::display::ui::tokens::terminal_feedback::ANCHOR_NEUTRAL_MIX_LIGHT
    } else {
        crate::display::ui::tokens::terminal_feedback::ANCHOR_NEUTRAL_MIX_DARK
    };
    (crate::display::content::mix_rgb(magenta, sk.ink_dim, mix), sk.is_light)
}
