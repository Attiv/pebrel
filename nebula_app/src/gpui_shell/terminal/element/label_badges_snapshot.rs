//! Viewport adaptation and native-style guards for cosmetic terminal labels.

use nebula_terminal::render::RenderSnapshot;
use nebula_terminal::vte::ansi::{Color, NamedColor};

use super::label_badges_classifier::{Label, PREFIX_COLS, classify_prefix};

pub(super) fn classify_badge_rows(
    snapshot: &RenderSnapshot,
    enabled: bool,
    provider: Option<&str>,
    codex_label: impl Fn(u16) -> Option<Label>,
    blocked: impl Fn(u16, u16) -> bool,
) -> Vec<Option<Label>> {
    if !enabled {
        return Vec::new();
    }
    let mut rows = vec![None; snapshot.rows as usize];
    let mut current_row = None;
    let mut prefix = [' '; PREFIX_COLS];
    let mut fence = None;
    let mut label = |row, prefix: &[char]| {
        if fenced(prefix, &mut fence) {
            return None;
        }
        classify_prefix(prefix, provider)
            .or_else(|| (provider == Some("codex")).then(|| codex_label(row)).flatten())
            .filter(|label| label.drawable(snapshot.cols, |col| blocked(row, col)))
    };
    for segment in &snapshot.segments {
        if segment.row >= snapshot.rows {
            continue;
        }
        if current_row != Some(segment.row) {
            if let Some(row) = current_row {
                rows[row as usize] = label(row, &prefix);
            }
            current_row = Some(segment.row);
            prefix.fill(' ');
        }
        for cell in segment.cells.iter().take_while(|cell| usize::from(cell.col) < PREFIX_COLS) {
            prefix[cell.col as usize] = cell.text.chars().next().unwrap_or(' ');
        }
    }
    if let Some(row) = current_row {
        rows[row as usize] = label(row, &prefix);
    }
    // Explicit backgrounds and selections keep their native meaning, even when
    // the theme resolver maps the color back to the default surface.
    for run in &snapshot.bg_runs {
        clear_overlapping(&mut rows, run.row, run.start, run.end);
    }
    for run in &snapshot.selection_runs {
        clear_overlapping(&mut rows, run.row, run.start, run.end);
    }
    if let Some(cursor) = &snapshot.cursor {
        clear_overlapping(
            &mut rows,
            cursor.row,
            cursor.col,
            cursor.col.saturating_add(if cursor.wide { 2 } else { 1 }),
        );
    }
    for segment in &snapshot.segments {
        if rows.get(segment.row as usize).is_none_or(Option::is_none) {
            continue;
        }
        for cell in segment.cells.iter().take_while(|cell| usize::from(cell.col) < PREFIX_COLS) {
            if segment.wide
                || cell.text.len() != 1
                || cell.underline
                || cell.strikethrough
                || cell.bg != Color::Named(NamedColor::Background)
            {
                clear_overlapping(
                    &mut rows,
                    segment.row,
                    cell.col,
                    cell.col.saturating_add(segment.step()),
                );
            }
        }
    }
    rows
}

fn clear_overlapping(rows: &mut [Option<Label>], row: u16, start: u16, end: u16) {
    if let Some(slot) = rows.get_mut(row as usize) {
        if slot.is_some_and(|label| label.overlaps(start, end)) {
            *slot = None;
        }
    }
}

/// Only visible fences are knowable without reading or retaining scrollback.
fn fenced(prefix: &[char], active: &mut Option<(char, usize)>) -> bool {
    let start = prefix.iter().take(4).take_while(|ch| **ch == ' ').count();
    let marker = prefix.get(start).copied().filter(|ch| matches!(ch, '`' | '~'));
    if start <= 3 {
        if let Some(marker) = marker {
            let count = prefix[start..].iter().take_while(|ch| **ch == marker).count();
            if count >= 3 {
                match *active {
                    None => *active = Some((marker, count)),
                    Some((opening, length))
                        if opening == marker
                            && count >= length
                            && prefix[start + count..].iter().all(|ch| *ch == ' ') =>
                    {
                        *active = None
                    },
                    _ => {},
                }
                return true;
            }
        }
    }
    active.is_some()
}

#[cfg(test)]
#[path = "label_badges_snapshot_tests.rs"]
mod tests;
