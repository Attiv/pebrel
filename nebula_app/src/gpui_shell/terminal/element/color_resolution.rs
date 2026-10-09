//! Terminal snapshot and theme color adaptation outside the terminal lock.
//! Palette lookup and contrast/remapping rules remain owned by their existing resolvers.

use gpui::{App, Rgba};
use nebula_terminal::render::RenderSnapshot;
use nebula_terminal::term::color::Colors;
use nebula_terminal::vte::ansi::Color;

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

/// [`super::TerminalElement::resolve_app_colors`] 的实际逻辑（脱开 GPUI 实体，可测）。
pub(super) fn resolve_app_colors_into(
    snap: &mut RenderSnapshot,
    theme: &Palette,
    overrides: &Colors,
    resolver: &mut crate::display::terminal_color::TerminalColorResolver,
) {
    use crate::display::content::is_terminal_graphic;
    use crate::display::terminal_color::is_fixed_color;

    for run in &mut snap.bg_runs {
        let base = rgb_from_rgba(theme.resolve(run.color, overrides, false));
        let resolved = resolver.resolve_background(base, is_fixed_color(run.color, overrides));
        if resolved != base {
            run.color = Color::Spec(resolved.0);
        }
    }
    for cell in snap.segments.iter_mut().flat_map(|segment| segment.cells.iter_mut()) {
        // 图形字符的颜色表达图形本身，不是正文对比度——图标被「矫正」成另一个
        // 颜色就是另一张图了。
        let graphic = cell.text.chars().next().is_some_and(is_terminal_graphic);
        if graphic {
            continue;
        }
        // 对比度是一对颜色的属性：这个前景可不可读，取决于它**这一格**底下是
        // 什么，而不是主题底色。默认底色的格子没有 bg run，所以 `SnapCell::bg`
        // 单独带着这个值。
        cell.fg = resolve_text_foreground(cell.fg, cell.bg, cell.bold, theme, overrides, resolver);
    }
}

pub(super) fn resolve_text_foreground(
    fg: Color,
    bg: Color,
    bold: bool,
    theme: &Palette,
    overrides: &Colors,
    resolver: &mut crate::display::terminal_color::TerminalColorResolver,
) -> Color {
    use crate::display::terminal_color::is_fixed_color;
    let bg_base = rgb_from_rgba(theme.resolve(bg, overrides, false));
    let bg = resolver.resolve_background(bg_base, is_fixed_color(bg, overrides));
    // Resolve bold before contrast adjustment; Spec must retain that brightening.
    let base = rgb_from_rgba(theme.resolve(fg, overrides, bold));
    let resolved = resolver.resolve_foreground(
        base,
        bg,
        true,
        rgb_from_rgba(theme.foreground),
        rgb_from_rgba(theme.background),
    );
    if resolved != base { Color::Spec(resolved.0) } else { fg }
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
