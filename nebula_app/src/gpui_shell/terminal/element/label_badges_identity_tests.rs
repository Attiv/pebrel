//! Foreground identity boundaries use actual TerminalView state, not a parser mock.

use gpui::TestAppContext;
use nebula_terminal::event::Event;
use nebula_terminal::render::{RenderSnapshot, SnapshotConfig};

use super::super::{LabelKind, classify_badge_rows};
use super::badge_provider;
use crate::display::AiSessionIdentity;
use crate::gpui_shell::terminal::view::{TerminalView, startup_tests::open};

pub(super) fn feed(view: &mut TerminalView, bytes: &[u8]) {
    let mut term = view.session.as_ref().unwrap().term.lock();
    let mut parser: nebula_terminal::vte::ansi::Processor =
        nebula_terminal::vte::ansi::Processor::new();
    parser.advance(&mut *term, bytes);
}

#[gpui::test]
fn retained_agent_resume_identity_cannot_badge_another_foreground_program(cx: &mut TestAppContext) {
    let (view, window, _) = open(cx);
    view.update(window, |view, _| {
        view.ai_session =
            Some(AiSessionIdentity { source: "codex".into(), session_id: "old-codex".into() });
        view.running_program = Some("claude".into());
        assert_eq!(
            view.runtime_agent().unwrap().kind,
            "codex",
            "retained resume identity remains unchanged"
        );
        let current = badge_provider(view, true).unwrap();
        assert_eq!(current.kind, "claude", "new decoration belongs to the foreground process");
        feed(view, "[INFO] live log\r\n⏺ Read(file)".as_bytes());
        let term = view.session.as_ref().unwrap().term.lock();
        let snapshot = RenderSnapshot::capture(&term, &SnapshotConfig { rows: 24, cols: 80 });
        let labels =
            classify_badge_rows(&snapshot, true, Some(&current.kind), |_| None, |_, _| false);
        assert_eq!(labels[0].unwrap().kind, LabelKind::Info);
        assert_eq!(labels[1].unwrap().kind, LabelKind::Tool);
        drop(term);
        view.running_program = Some("gemini".into());
        let current = badge_provider(view, true).unwrap();
        assert_eq!(current.kind, "gemini");
        let labels =
            classify_badge_rows(&snapshot, true, Some(&current.kind), |_| None, |_, _| false);
        assert_eq!(
            labels[0].unwrap().kind,
            LabelKind::Info,
            "ordinary logs do not require an Agent"
        );
        assert!(labels[1].is_none());
        view.running_program = None;
        assert!(
            badge_provider(view, true).is_none(),
            "history cannot claim a finished foreground process"
        );
        assert!(badge_provider(view, false).is_none(), "disabled feature does not query identity");
    });
}

#[gpui::test]
fn disproved_foreground_identity_is_not_badge_authority(cx: &mut TestAppContext) {
    let (view, window, _) = open(cx);
    view.update(window, |view, cx| {
        // Actual command-start logic disproves an interactive-shell 'command'.
        view.suggest.last_committed = "zsh".into();
        view.process_event(Event::CommandStart, cx);
        view.running_program = Some("codex".into());
        view.ai_session =
            Some(AiSessionIdentity { source: "codex".into(), session_id: "stale".into() });
        assert!(
            badge_provider(view, true).is_none(),
            "disproved process evidence wins over stale identity"
        );
    });
}
