//! Fixed-grid rounded label backdrops and contrasting foregrounds.

use gpui::{Bounds, Pixels, Rgba, Window, fill, point, px, size};

use super::label_badges_classifier::{Label, LabelKind, badge_geometry};
use super::{Palette, rgb_from_rgba};

/// Badge identity must be a live foreground observation, not retained resume metadata.
pub(super) fn badge_provider(
    view: &super::TerminalView,
    enabled: bool,
) -> Option<crate::runtime_api::RuntimeAgent> {
    enabled.then(|| view.runtime_agent()).flatten()
}

#[derive(Clone, Copy)]
pub(super) struct BadgeColors {
    pub background: Rgba,
    pub foreground: Rgba,
}

pub(super) struct LabelPalette {
    neutral: BadgeColors,
    warning: BadgeColors,
    danger: BadgeColors,
}

impl LabelPalette {
    pub fn new(theme: &Palette) -> Self {
        let colors = |background: Rgba| {
            let background = Rgba { a: 1.0, ..background };
            let rgb = rgb_from_rgba(background);
            let white = gpui::rgb(0xffffff);
            let black = gpui::rgb(0);
            // The better black/white endpoint always exceeds 4.5:1, even with
            // custom ANSI colors. The opaque fill keeps this independent of wallpaper.
            let foreground =
                if rgb_from_rgba(white).contrast(*rgb) >= rgb_from_rgba(black).contrast(*rgb) {
                    white
                } else {
                    black
                };
            BadgeColors { background, foreground }
        };
        Self {
            neutral: colors(if theme.is_dark() {
                gpui::rgb(0x303648)
            } else {
                gpui::rgb(0x424a5a)
            }),
            warning: colors(theme.ansi[3]),
            danger: colors(theme.ansi[1]),
        }
    }

    pub fn colors(&self, kind: LabelKind) -> BadgeColors {
        match kind {
            LabelKind::Info | LabelKind::Tool => self.neutral,
            LabelKind::Warn => self.warning,
            LabelKind::Error | LabelKind::Failure => self.danger,
        }
    }
}

pub(super) fn paint_label_badges(
    rows: &[Option<Label>],
    palette: &LabelPalette,
    window: &mut Window,
    row_bounds: impl Fn(u16) -> Bounds<Pixels>,
    cell_width: Pixels,
) {
    for (row, label) in
        rows.iter().enumerate().filter_map(|(row, label)| label.map(|label| (row, label)))
    {
        let cell = row_bounds(row as u16);
        let Some(geometry) =
            badge_geometry(cell_width.as_f32(), cell.size.height.as_f32(), label.start, label.end)
        else {
            continue;
        };
        let bounds = Bounds::new(
            point(cell.origin.x + px(geometry.x), cell.origin.y + px(geometry.y)),
            size(px(geometry.width), px(geometry.height)),
        );
        window.paint_quad(
            fill(bounds, palette.colors(label.kind).background).corner_radii(px(geometry.radius)),
        );
    }
}

#[cfg(test)]
#[path = "label_badges_tests.rs"]
mod tests;

#[cfg(all(test, feature = "gpui-test-support"))]
#[path = "label_badges_visual_tests.rs"]
mod visual_tests;

#[cfg(all(test, feature = "gpui-test-support"))]
#[path = "label_badges_identity_tests.rs"]
mod identity_tests;
