//! Cosmetic interpretation of Codex's visible tool-summary rows.
//! Never mutate the terminal grid or use these labels as task-state evidence.

use gpui::{Bounds, Pixels, Window, fill, point, px, size};
use nebula_terminal::render::{RenderSnapshot, SnapCell};
use nebula_terminal::vte::ansi::{Color, NamedColor};

use super::rgba_rgb;
use crate::display::color::Rgb;

const PREFIX_COLS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CodexSummaryKind {
    Success,
    Failure,
    Neutral,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CodexSummaryRow {
    pub row: u16,
    pub heading_start: u16,
    pub heading_end: u16,
    pub kind: CodexSummaryKind,
}

impl CodexSummaryRow {
    pub fn emphasizes(self, col: u16) -> bool {
        col == 0 || (self.heading_start..self.heading_end).contains(&col)
    }

    pub fn emphasizes_cell(self, cell: &SnapCell) -> bool {
        self.emphasizes(cell.col) && cell.bg == Color::Named(NamedColor::Background)
    }
}

pub(super) fn should_paint_codex_marker(row: CodexSummaryRow, snapshot: &RenderSnapshot) -> bool {
    !snapshot.bg_runs.iter().any(|run| run.row == row.row)
        && !snapshot.selection_runs.iter().any(|run| run.row == row.row)
}

pub(super) fn marker_bounds(cell: Bounds<Pixels>) -> Bounds<Pixels> {
    Bounds::new(
        point(cell.origin.x - px(6.0), cell.origin.y + cell.size.height * 0.15),
        size(px(2.0), cell.size.height * 0.70),
    )
}

pub(super) fn paint_codex_markers(
    rows: &[Option<CodexSummaryRow>],
    snapshot: &RenderSnapshot,
    window: &mut Window,
    row_bounds: impl Fn(u16) -> Bounds<Pixels>,
    color_for: impl Fn(CodexSummaryKind) -> Rgb,
    blocked: impl Fn(u16) -> bool,
) {
    for row in rows.iter().flatten().copied() {
        if !should_paint_codex_marker(row, snapshot) || blocked(row.row) {
            continue;
        }
        let rect = row_bounds(row.row);
        window.paint_quad(fill(marker_bounds(rect), rgba_rgb(color_for(row.kind), 0.80)));
    }
}

pub(super) fn summary_foreground(
    kind: CodexSummaryKind,
    success: Rgb,
    danger: Rgb,
    normal: Rgb,
    cell_background: Rgb,
    theme_background: Rgb,
) -> Rgb {
    let accent = match kind {
        CodexSummaryKind::Success => success,
        CodexSummaryKind::Failure => danger,
        CodexSummaryKind::Neutral => return normal,
    };
    crate::display::terminal_color::ensure_contrast(
        accent,
        cell_background,
        normal,
        theme_background,
        crate::display::ui::tokens::terminal_feedback::FIXED_TEXT_MIN_CONTRAST,
    )
}

/// One entry per viewport row makes painting a cell an O(1) lookup.
/// Only the first 32 columns are examined, and only after pane identity says Codex.
pub(super) fn classify_codex_summary_rows(
    snapshot: &RenderSnapshot,
    is_codex: bool,
) -> Vec<Option<CodexSummaryRow>> {
    if !is_codex {
        return Vec::new();
    }
    let mut rows = vec![None; snapshot.rows as usize];

    let mut current_row = None;
    let mut prefix = [' '; PREFIX_COLS];
    for segment in &snapshot.segments {
        if segment.row as usize >= rows.len() {
            continue;
        }
        if current_row != Some(segment.row) {
            if let Some(row) = current_row {
                rows[row as usize] = classify_prefix(row, &prefix);
            }
            current_row = Some(segment.row);
            prefix.fill(' ');
        }
        for cell in &segment.cells {
            if let Some(slot) = prefix.get_mut(cell.col as usize) {
                *slot = cell.text.chars().next().unwrap_or(' ');
            }
        }
    }
    if let Some(row) = current_row {
        rows[row as usize] = classify_prefix(row, &prefix);
    }
    rows
}

fn classify_prefix(row: u16, prefix: &[char; PREFIX_COLS]) -> Option<CodexSummaryRow> {
    if prefix[0] != '•' || prefix[1] != ' ' {
        return None;
    }
    let tail = &prefix[2..];
    let has = |word: &str| {
        tail.iter().copied().take(word.len()).eq(word.chars()) && tail[word.len()] == ' '
    };
    let (kind, heading_end) = if has("Ran") {
        (CodexSummaryKind::Success, 5)
    } else if has("Added") {
        (CodexSummaryKind::Success, 7)
    } else if has("Explored") {
        (CodexSummaryKind::Neutral, 10)
    } else if has("Failed")
        && tail[7..].iter().copied().take(6).eq("(exit ".chars())
        && tail[13..].iter().take(8).any(|ch| *ch == ')')
    {
        (CodexSummaryKind::Failure, 8)
    } else {
        return None;
    };
    Some(CodexSummaryRow { row, heading_start: 2, heading_end, kind })
}
