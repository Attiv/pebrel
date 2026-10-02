//! Codex tool-summary emphasis is a display-only interpretation of a real VT snapshot.

use nebula_terminal::event::VoidListener;
use nebula_terminal::render::{BgRun, CellRun, RenderSnapshot, SnapshotConfig};
use nebula_terminal::term::test::TermSize;
use nebula_terminal::term::{Config, Term};
use nebula_terminal::vte::ansi::{self, Color, NamedColor};

use super::classify_codex_summary_rows;
use super::codex_emphasis::{
    CodexSummaryKind, CodexSummaryPalette, marker_bounds, should_paint_codex_marker,
    summary_foreground,
};
use crate::display::color::Rgb;
use gpui::{Bounds, point, px, size};

fn capture(bytes: &[u8]) -> RenderSnapshot {
    let mut term = Term::new(Config::default(), &TermSize::new(90, 8), VoidListener);
    let mut parser: ansi::Processor = ansi::Processor::new();
    parser.advance(&mut term, bytes);
    RenderSnapshot::capture(&term, &SnapshotConfig { rows: 8, cols: 90 })
}

#[test]
fn codex_tool_summaries_are_classified_without_changing_the_terminal_grid() {
    let snapshot = capture(
        "• Ran python3 - <<'PY' …\r\n• Failed (exit 1) flutter analyze\r\n• Added test/example.dart (+98 -0)\r\n• Explored\r\n  ordinary body"
            .as_bytes(),
    );
    let original: Vec<_> = snapshot
        .segments
        .iter()
        .flat_map(|segment| {
            segment.cells.iter().map(|cell| (segment.row, cell.col, cell.text.clone()))
        })
        .collect();

    let rows = classify_codex_summary_rows(&snapshot, true);
    assert_eq!(rows[0].unwrap().kind, CodexSummaryKind::Success);
    assert_eq!((rows[0].unwrap().heading_start, rows[0].unwrap().heading_end), (2, 5));
    assert_eq!(rows[1].unwrap().kind, CodexSummaryKind::Failure);
    assert_eq!((rows[1].unwrap().heading_start, rows[1].unwrap().heading_end), (2, 8));
    assert_eq!(rows[2].unwrap().kind, CodexSummaryKind::Success);
    assert_eq!(rows[3].unwrap().kind, CodexSummaryKind::Neutral);
    assert!(rows[4].is_none());
    assert_eq!(
        original,
        snapshot
            .segments
            .iter()
            .flat_map(|segment| segment.cells.iter().map(|cell| (
                segment.row,
                cell.col,
                cell.text.clone()
            )))
            .collect::<Vec<_>>(),
    );
}

#[test]
fn summary_shapes_are_ignored_in_other_programs_and_non_summary_lines() {
    let snapshot = capture(
        b"\xe2\x80\xa2 Ran ls\r\n  \xe2\x80\xa2 Ran ls\r\n\xe2\x80\xa2 Running ls\r\nRan ls\r\n\xe2\x80\xa2 Failed test\r\n\xe2\x80\xa2 Failed (exit 1) test",
    );
    assert!(
        classify_codex_summary_rows(&snapshot, false).is_empty(),
        "unrelated terminals take the no-allocation fast path"
    );
    let rows = classify_codex_summary_rows(&snapshot, true);
    assert_eq!(rows[0].unwrap().kind, CodexSummaryKind::Success);
    assert!(rows[1].is_none(), "indented quoted output is not a Codex summary");
    assert!(rows[2].is_none());
    assert!(rows[3].is_none());
    assert!(rows[4].is_none(), "failure requires an exit status");
    assert_eq!(rows[5].unwrap().kind, CodexSummaryKind::Failure);
}

#[test]
fn emphasis_stops_after_the_status_word_and_leaves_command_text_alone() {
    let snapshot = capture("• Ran python3 -m pytest".as_bytes());
    let row = classify_codex_summary_rows(&snapshot, true)[0].unwrap();
    assert!(row.emphasizes(0), "the status bullet is part of the emphasis");
    assert!(!row.emphasizes(1));
    for col in 2..5 {
        assert!(row.emphasizes(col));
    }
    assert!(!row.emphasizes(5));
    assert!(!row.emphasizes(6), "command text keeps its original styling");
}

#[test]
fn marker_does_not_paint_over_program_background_or_selection() {
    let mut snapshot = capture("• Failed (exit 1) test".as_bytes());
    let row = classify_codex_summary_rows(&snapshot, true)[0].unwrap();
    assert!(should_paint_codex_marker(row, &snapshot));
    snapshot.bg_runs.push(BgRun { row: 0, start: 0, end: 1, color: Color::Named(NamedColor::Red) });
    assert!(!should_paint_codex_marker(row, &snapshot));
    snapshot.bg_runs.clear();
    snapshot.selection_runs.push(CellRun { row: 0, start: 2, end: 8 });
    assert!(!should_paint_codex_marker(row, &snapshot));
}

#[test]
fn marker_sits_in_the_gutter_instead_of_overdrawing_the_status_bullet() {
    let cell = Bounds::new(point(px(20.0), px(40.0)), size(px(12.0), px(20.0)));
    let marker = marker_bounds(cell);
    assert!(marker.origin.x < cell.origin.x);
    assert_eq!(marker.origin.x, px(14.0));
    assert_eq!(marker.origin.y, px(43.0));
    assert_eq!(marker.size.width, px(2.0));
    assert_eq!(marker.size.height, px(14.0));
}

#[test]
fn explicit_program_background_keeps_its_own_heading_style() {
    let snapshot = capture("\x1b[41m• Ran ls".as_bytes());
    let row = classify_codex_summary_rows(&snapshot, true)[0].unwrap();
    let bullet = snapshot
        .segments
        .iter()
        .find(|segment| segment.row == 0)
        .and_then(|segment| segment.cells.iter().find(|cell| cell.col == 0))
        .unwrap();
    assert!(row.emphasizes(0));
    assert!(!row.emphasizes_cell(bullet));

    let snapshot = capture("• Ran ls".as_bytes());
    let row = classify_codex_summary_rows(&snapshot, true)[0].unwrap();
    let bullet = snapshot
        .segments
        .iter()
        .find(|segment| segment.row == 0)
        .and_then(|segment| segment.cells.iter().find(|cell| cell.col == 0))
        .unwrap();
    assert!(row.emphasizes_cell(bullet));
}

#[test]
fn status_accents_remain_readable_on_light_and_dark_terminal_surfaces() {
    let foreground = Rgb::new(0x21, 0x25, 0x2b);
    let background = Rgb::new(0xf8, 0xf9, 0xfb);
    let success = Rgb::new(0x60, 0xe8, 0x6e);
    let danger = Rgb::new(0xff, 0x6b, 0x81);
    for kind in [CodexSummaryKind::Success, CodexSummaryKind::Failure] {
        let color = summary_foreground(kind, success, danger, foreground, background, background);
        assert!(color.contrast(*background) >= 4.5, "{kind:?} is unreadable on light theme");
    }

    let foreground = Rgb::new(0xd6, 0xda, 0xea);
    let background = Rgb::new(0x08, 0x0a, 0x18);
    for kind in [CodexSummaryKind::Success, CodexSummaryKind::Failure] {
        let color = summary_foreground(kind, success, danger, foreground, background, background);
        assert!(color.contrast(*background) >= 4.5, "{kind:?} is unreadable on dark theme");
    }
    assert_eq!(
        summary_foreground(
            CodexSummaryKind::Neutral,
            success,
            danger,
            foreground,
            background,
            background
        ),
        foreground,
    );
}

#[test]
fn summary_palette_preserves_theme_colors_and_contrast_adjustment() {
    let dark = super::Palette::default();
    let mut light = dark.clone();
    light.foreground = gpui::rgb(0x21252b);
    light.background = gpui::rgb(0xf8f9fb);
    for theme in [dark, light] {
        let palette = CodexSummaryPalette::new(&theme);
        let success = super::rgb_from_rgba(theme.ansi[2]);
        let failure = super::rgb_from_rgba(theme.ansi[1]);
        let normal = super::rgb_from_rgba(theme.foreground);
        let background = super::rgb_from_rgba(theme.background);
        for kind in
            [CodexSummaryKind::Success, CodexSummaryKind::Failure, CodexSummaryKind::Neutral]
        {
            assert_eq!(
                palette.color(kind),
                summary_foreground(kind, success, failure, normal, background, background),
            );
        }
    }
}
