use super::*;

fn classify(text: &str, provider: Option<&str>) -> Option<Label> {
    classify_prefix(&text.chars().take(PREFIX_COLS).collect::<Vec<_>>(), provider)
}

fn expected(start: u16, end: u16, kind: LabelKind) -> Option<Label> {
    Some(Label { start, end, kind })
}

#[test]
fn recognizes_structured_log_levels_without_badging_the_message() {
    for (level, kind) in
        [("INFO", LabelKind::Info), ("WARN", LabelKind::Warn), ("ERROR", LabelKind::Error)]
    {
        let end = level.len() as u16;
        assert_eq!(
            classify(&format!("[{level}] server started"), None),
            expected(1, end + 1, kind)
        );
        assert_eq!(classify(&format!("{level}: server started"), None), expected(0, end, kind));
        assert_eq!(classify(&format!("{level}  server started"), None), expected(0, end, kind));
    }
}

#[test]
fn recognizes_level_after_an_explicit_timestamp() {
    assert_eq!(classify("17:22:36 INFO server started", None), expected(9, 13, LabelKind::Info));
    assert_eq!(classify("[17:22:36.125] [WARN] retrying", None), expected(16, 20, LabelKind::Warn));
    assert_eq!(
        classify("2026-10-09T17:22:36Z ERROR: failed", None),
        expected(21, 26, LabelKind::Error)
    );
    assert_eq!(
        classify("2026-10-09 17:22:36.123 INFO service::ready", None),
        expected(24, 28, LabelKind::Info)
    );
}

#[test]
fn rejects_words_shell_commands_code_and_indented_quoted_output() {
    for text in [
        "information",
        "WARNING: bad",
        "ERROR_CODE = 1",
        "INFO=value",
        "info: lowercase",
        "echo INFO: example",
        "$ INFO: command",
        "  [INFO] quoted",
        "> [WARN] quoted",
        "INFO is an acronym",
        "[INFO]",
        "[WARN]ed",
        "[ERROR] = value",
        "WARN:",
        "2026-99-99 INFO not a timestamp",
        "[module] [INFO] no arbitrary prefix",
        "42 INFO no timestamp",
        "[17:22] INFO no complete time",
    ] {
        assert_eq!(classify(text, None), None, "unexpected badge for {text:?}");
    }
}

#[test]
fn claude_tool_names_require_provider_identity_and_native_structure() {
    assert_eq!(classify("⏺ Bash(cargo test)", Some("claude")), expected(2, 6, LabelKind::Tool));
    assert_eq!(classify("⏺ Read(src/main.rs)", Some("claude")), expected(2, 6, LabelKind::Tool));
    assert_eq!(classify("⏺ Update(src/main.rs)", Some("claude")), expected(2, 8, LabelKind::Tool));
    for provider in [None, Some("codex"), Some("gemini")] {
        assert_eq!(classify("⏺ Bash(cargo test)", provider), None);
    }
    for text in [
        "Bash(cargo test)",
        "  ⏺ Bash(cargo test)",
        "⏺ Bash is a tool",
        "⏺ Unknown(command)",
        "⏺ Read",
        "⏺ Bash(",
    ] {
        assert_eq!(classify(text, Some("claude")), None, "unexpected Claude tool for {text:?}");
    }
}

#[test]
fn bounded_prefix_does_not_accept_clipped_labels_or_delimiters() {
    assert_eq!(classify_prefix(&['I', 'N', 'F', 'O'], None), None);
    assert_eq!(classify_prefix(&['[', 'E', 'R', 'R', 'O', 'R', ']'], None), None);
    assert_eq!(classify_prefix(&['⏺', ' ', 'R', 'e', 'a', 'd'], Some("claude")), None);
    assert_eq!(classify_prefix(&[], None), None);
    assert_eq!(classify("17:22:36 INFO", None), None);
}

#[test]
fn label_ranges_leave_adjacent_selection_and_cursor_cells_alone() {
    let label = Label { start: 2, end: 6, kind: LabelKind::Tool };
    assert!(!label.contains(1));
    assert!(label.contains(2));
    assert!(label.contains(5));
    assert!(!label.contains(6));
    assert!(!label.overlaps(3, 3), "an empty range never overlaps");
    assert!(!label.overlaps(5, 4), "invalid reversed runs cannot cover cells");
    assert!(!label.overlaps(0, 2));
    assert!(!label.overlaps(6, 8));
    assert!(label.overlaps(1, 3));
    assert!(label.overlaps(5, 7));
}

#[test]
fn drawable_badges_require_unclipped_unblocked_cells() {
    let label = Label { start: 2, end: 6, kind: LabelKind::Tool };
    assert!(label.drawable(6, |_| false));
    assert!(!label.drawable(5, |_| false), "partial labels stay native");
    for blocked in [2, 3, 5] {
        assert!(
            !label.drawable(80, |col| col == blocked),
            "any selected/cursor/background/overlay cell blocks the whole badge"
        );
    }
    assert!(label.drawable(80, |col| col == 6), "adjacent cells remain independent");
    assert!(!Label { start: 6, end: 6, kind: LabelKind::Info }.drawable(80, |_| false));
}

#[test]
fn badge_backdrop_stays_in_fixed_grid_cells_at_narrow_and_hidpi_sizes() {
    for (cell_width, line_height) in [(6.0, 12.0), (12.0, 24.0), (8.5, 19.0)] {
        let geometry = badge_geometry(cell_width, line_height, 2, 6).unwrap();
        assert_eq!(geometry.x, cell_width * 2.0);
        assert_eq!(geometry.width, cell_width * 4.0);
        assert!(geometry.y >= 0.0);
        assert!(geometry.y + geometry.height <= line_height);
        assert!(geometry.radius > 0.0);
        assert!(geometry.radius <= geometry.height * 0.5);
        assert!(geometry.radius <= geometry.width * 0.5);
    }
    assert!(badge_geometry(0.0, 20.0, 2, 6).is_none());
    assert!(badge_geometry(10.0, 0.0, 2, 6).is_none());
    assert!(badge_geometry(10.0, 20.0, 6, 2).is_none());
}
