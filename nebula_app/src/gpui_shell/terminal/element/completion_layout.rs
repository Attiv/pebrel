//! Completion-popup pixel geometry shared by painting and hit testing.
//!
//! The terminal grid stays in cell units; this module owns the conversion to
//! device-independent popup bounds, icon/tag column arithmetic and scrolling.

use gpui::{Bounds, Pixels, point, px, size};

use super::{
    COMPLETION_COLUMN_GAP, COMPLETION_ICON_WIDTH, COMPLETION_PANEL_GAP, COMPLETION_PANEL_PAD,
    COMPLETION_PANEL_WIDTH, COMPLETION_ROW_HEIGHT, COMPLETION_ROW_LEFT_PAD,
    COMPLETION_ROW_RIGHT_PAD, COMPLETION_TAG_WIDTH,
};

/// 绘制与命中共用的像素几何。这里刻意不再用终端 cell 拼凑 UI 列：
/// 22px 图标槽、两段 8px 间距和 46px 标签槽必须与 HTML 原型一一对应。
#[derive(Clone, Copy)]
pub(in crate::gpui_shell::terminal) struct CompletionPopupLayout {
    pub(super) content_x: Pixels,
    pub(super) content_y: Pixels,
    pub(super) content_width: Pixels,
    pub row_height: Pixels,
    pub offset: usize,
    pub rows: usize,
    pub label_width: usize,
    pub selected: Option<usize>,
}

impl CompletionPopupLayout {
    pub(in crate::gpui_shell::terminal) fn content_bounds(
        self,
        origin: gpui::Point<Pixels>,
    ) -> Bounds<Pixels> {
        Bounds::new(
            point(origin.x + self.content_x, origin.y + self.content_y),
            size(self.content_width, self.row_height * self.rows),
        )
    }

    pub(in crate::gpui_shell::terminal) fn panel_bounds(
        self,
        origin: gpui::Point<Pixels>,
    ) -> Bounds<Pixels> {
        let content = self.content_bounds(origin);
        let pad = px(COMPLETION_PANEL_PAD);
        Bounds::new(
            point(content.origin.x - pad, content.origin.y - pad),
            size(content.size.width + pad * 2.0, content.size.height + pad * 2.0),
        )
    }

    pub(super) fn row_bounds(self, origin: gpui::Point<Pixels>, row: usize) -> Bounds<Pixels> {
        let content = self.content_bounds(origin);
        Bounds::new(
            point(content.origin.x, content.origin.y + self.row_height * row),
            size(content.size.width, self.row_height),
        )
    }

    pub(in crate::gpui_shell::terminal) fn scrollbar_bounds(
        self,
        origin: gpui::Point<Pixels>,
        count: usize,
    ) -> Option<(Bounds<Pixels>, Bounds<Pixels>)> {
        if count <= self.rows {
            return None;
        }
        let content = self.content_bounds(origin);
        let track_height = content.size.height - px(4.0);
        let track = Bounds::new(
            point(content.right() + px(1.0), content.origin.y + px(2.0)),
            size(px(2.0), track_height),
        );
        let thumb_height =
            (track_height * (self.rows as f32 / count as f32)).max(px(14.0)).min(track_height);
        let progress = self.offset.min(count - self.rows) as f32 / (count - self.rows) as f32;
        let thumb = Bounds::new(
            point(track.origin.x, track.origin.y + (track_height - thumb_height) * progress),
            size(track.size.width, thumb_height),
        );
        Some((track, thumb))
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::gpui_shell::terminal) fn completion_popup_layout(
    items: &[crate::display::NebulaCompletionItem],
    selected: Option<usize>,
    offset: usize,
    cursor_row: usize,
    cursor_col: usize,
    screen_lines: usize,
    columns: usize,
    cell_width: Pixels,
    line_height: Pixels,
) -> Option<CompletionPopupLayout> {
    if columns < 12 || screen_lines < 2 || items.is_empty() {
        return None;
    }

    let cell_width = cell_width.as_f32();
    let line_height = line_height.as_f32();
    if cell_width <= 0.0 || line_height <= 0.0 {
        return None;
    }

    let viewport_width = cell_width * columns as f32;
    let viewport_height = line_height * screen_lines as f32;
    let panel_width = COMPLETION_PANEL_WIDTH.min(viewport_width);
    let content_width = panel_width - COMPLETION_PANEL_PAD * 2.0;
    let fixed_columns = COMPLETION_ROW_LEFT_PAD
        + COMPLETION_ICON_WIDTH
        + COMPLETION_COLUMN_GAP * 2.0
        + COMPLETION_TAG_WIDTH
        + COMPLETION_ROW_RIGHT_PAD;
    let label_width = ((content_width - fixed_columns) / cell_width).floor() as isize;
    if label_width <= 0 {
        return None;
    }

    let cursor_row = cursor_row.min(screen_lines - 1);
    let cursor_col = cursor_col.min(columns - 1);
    let cursor_top = line_height * cursor_row as f32;
    let cursor_bottom = cursor_top + line_height;
    let below_space = (viewport_height - cursor_bottom - COMPLETION_PANEL_GAP).max(0.0);
    let above_space = (cursor_top - COMPLETION_PANEL_GAP).max(0.0);
    let rows_that_fit = |space: f32| {
        ((space - COMPLETION_PANEL_PAD * 2.0).max(0.0) / COMPLETION_ROW_HEIGHT).floor() as usize
    };
    let below = rows_that_fit(below_space);
    let above = rows_that_fit(above_space);
    let want = items.len().min(8);
    let place_below = below >= want || below >= above;
    let rows = want.min(if place_below { below } else { above });
    if rows == 0 {
        return None;
    }

    let panel_height = COMPLETION_ROW_HEIGHT * rows as f32 + COMPLETION_PANEL_PAD * 2.0;
    let panel_y = if place_below {
        cursor_bottom + COMPLETION_PANEL_GAP
    } else {
        (cursor_top - COMPLETION_PANEL_GAP - panel_height).max(0.0)
    };

    // 光标处优先对齐列表内容；若 520px 面板越过右边界，整块向左滑，
    // 而不是压缩图标/标签列，窄窗里只牺牲中间文字宽度。
    let desired_panel_x = cell_width * cursor_col as f32 - COMPLETION_PANEL_PAD;
    let max_panel_x = (viewport_width - panel_width).max(0.0);
    let panel_x = desired_panel_x.clamp(0.0, max_panel_x);

    let selected = selected.filter(|index| *index < items.len());
    let offset = offset.min(items.len().saturating_sub(rows));
    Some(CompletionPopupLayout {
        content_x: px(panel_x + COMPLETION_PANEL_PAD),
        content_y: px(panel_y + COMPLETION_PANEL_PAD),
        content_width: px(content_width),
        row_height: px(COMPLETION_ROW_HEIGHT),
        offset,
        rows,
        label_width: label_width as usize,
        selected,
    })
}
