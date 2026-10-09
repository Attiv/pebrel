use nebula_terminal::event::VoidListener;
use nebula_terminal::render::{CellRun, RenderSnapshot, SnapshotConfig};
use nebula_terminal::term::test::TermSize;
use nebula_terminal::term::{Config, Term};
use nebula_terminal::vte::ansi;

use super::super::label_badges_classifier::LabelKind;
use super::*;

fn capture(text: &str, cols: u16) -> RenderSnapshot {
    let mut term = Term::new(Config::default(), &TermSize::new(cols as usize, 12), VoidListener);
    let mut parser: ansi::Processor = ansi::Processor::new();
    parser.advance(&mut term, text.as_bytes());
    RenderSnapshot::capture(&term, &SnapshotConfig { rows: 12, cols })
}

fn rows(snapshot: &RenderSnapshot) -> Vec<Option<Label>> {
    classify_badge_rows(snapshot, true, None, |_| None, |_, _| false)
}

#[test]
fn disabled_setting_does_not_scan_prefixes_or_query_projection() {
    let snapshot = capture("[INFO] ready\r\n⏺ Read(file)", 80);
    assert!(
        classify_badge_rows(
            &snapshot,
            false,
            Some("claude"),
            |_| panic!("Codex callback on disabled path"),
            |_, _| panic!("projection on disabled path")
        )
        .is_empty()
    );
}

#[test]
fn viewport_classification_keeps_original_cells_and_provider_identity() {
    let snapshot = capture("[INFO] ready\r\n⏺ Bash(cargo test)\r\nordinary text", 80);
    let original = snapshot
        .segments
        .iter()
        .flat_map(|segment| segment.cells.iter().map(|cell| cell.text.clone()))
        .collect::<Vec<_>>();
    let badges = classify_badge_rows(&snapshot, true, Some("claude"), |_| None, |_, _| false);
    assert_eq!(badges[0].unwrap().kind, LabelKind::Info);
    assert_eq!(badges[1].unwrap().kind, LabelKind::Tool);
    assert!(badges[2].is_none());
    assert!(rows(&snapshot)[1].is_none());
    assert_eq!(
        original,
        snapshot
            .segments
            .iter()
            .flat_map(|segment| segment.cells.iter().map(|cell| cell.text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn ansi_background_selection_cursor_overlay_and_nonplain_cells_stay_native() {
    assert!(rows(&capture("\x1b[41m[INFO] ready", 80))[0].is_none());
    assert!(rows(&capture("[I\u{301}NFO] ready", 80))[0].is_none());
    assert!(rows(&capture("\x1b[4m[WARN] ready", 80))[0].is_none());
    let mut snapshot = capture("[ERROR] ready", 80);
    assert!(rows(&snapshot)[0].is_some());
    snapshot.selection_runs.push(CellRun { row: 0, start: 1, end: 2 });
    assert!(rows(&snapshot)[0].is_none());
    snapshot.selection_runs.clear();
    snapshot.cursor.as_mut().unwrap().row = 0;
    snapshot.cursor.as_mut().unwrap().col = 3;
    assert!(rows(&snapshot)[0].is_none());
    snapshot.cursor = None;
    assert!(
        classify_badge_rows(&snapshot, true, None, |_| None, |row, col| row == 0 && col == 3)[0]
            .is_none()
    );
    assert!(
        classify_badge_rows(&snapshot, true, None, |_| None, |row, col| row == 0 && col == 7)[0]
            .is_some()
    );
}

#[test]
fn fenced_examples_are_not_decorated_and_recognition_resumes_after_close() {
    for fence in ["```", "~~~"] {
        let snapshot =
            capture(&format!("{fence}log\r\n[INFO] example\r\n{fence}\r\n[WARN] live"), 80);
        let badges = rows(&snapshot);
        assert!(badges[1].is_none());
        assert_eq!(badges[3].unwrap().kind, LabelKind::Warn);
    }
}

#[test]
fn codex_badges_reuse_existing_summary_ranges_only_in_codex_panes() {
    let snapshot = capture("• Ran ls\r\n• Explored", 80);
    let summary =
        |row| Some(Label { start: 2, end: if row == 0 { 5 } else { 10 }, kind: LabelKind::Tool });
    let badges = classify_badge_rows(&snapshot, true, Some("codex"), summary, |_, _| false);
    assert_eq!(badges[0].unwrap().end, 5);
    assert_eq!(badges[1].unwrap().end, 10);
    assert!(
        classify_badge_rows(&snapshot, true, Some("claude"), summary, |_, _| false)
            .iter()
            .all(Option::is_none)
    );
}

#[test]
fn viewport_clip_does_not_invent_a_log_message() {
    let snapshot = capture("[INFO]", 6);
    assert!(rows(&snapshot)[0].is_none());
}
