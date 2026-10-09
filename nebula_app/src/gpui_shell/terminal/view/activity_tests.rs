use super::startup_tests::{feed, open};
use super::*;
use crate::ai_agents::{AgentStatus, AgentStatusSource};
use crate::ai_hook::AiHookEvent;
use gpui::TestAppContext;

fn hook(session: &str, name: &str, sequence: u64) -> AiHookEvent {
    let payload = serde_json::json!({
        "hook_event_name": name,
        "session_id": session,
        "bridge_sequence": sequence,
    });
    let wire = format!("nebula-hook/1 source=claude pane=42\n{payload}");
    crate::ai_hook::parse_remote_envelope(wire.as_bytes(), Some(42)).unwrap()
}

#[gpui::test]
fn unintegrated_ssh_agent_exit_restores_protocol_only_after_the_shell_returns(
    cx: &mut TestAppContext,
) {
    let (view, window, _) = open(cx);
    for alternate in [false, true] {
        view.update(window, |view, cx| {
            let (session, _, _, proxy) = session::test_session_with_events();
            view.session = Some(session);
            view.clear_foreground_agent_state(cx);
            view.suggest.suggest_env =
                crate::display::SuggestEnv::Ssh { destination: "fish.test".into() };
            view.session.as_ref().unwrap().term.lock().set_options(nebula_terminal::term::Config {
                kitty_keyboard: true,
                ..Default::default()
            });
            screen(view, "user@host:~$ claude");
            view.suggest.line_buf = "claude".into();
            view.commit_line(cx);
            assert!(
                view.suggest.pending_command_prompt.is_some(),
                "use the real confirmed shell submission"
            );
            let id = if alternate { "no-osc-alt" } else { "no-osc-primary" };
            assert!(view.handle_ai_hook(&hook(id, "SessionStart", 1), cx));
            nebula_terminal::event_loop::StreamProcessor::default().feed(
                &mut view.session.as_ref().unwrap().term.lock(),
                &proxy,
                b"\x1b[>31u\x1b[?1000h\x1b[?1006h",
            );
            if alternate {
                feed(view, b"\x1b[?1049h\x1b[>31u");
            }
            screen(view, "AI is still open");
            assert!(view.handle_ai_hook(&hook(id, "Stop", 2), cx));
            view.refresh_agent_screen_state(cx);
            assert!(
                view.term_mode()
                    .contains(TermMode::REPORT_ALL_KEYS_AS_ESC | TermMode::MOUSE_REPORT_CLICK)
            );
            assert!(view.handle_ai_hook(&hook(id, "SessionEnd", 3), cx));
            view.refresh_agent_screen_state(cx);
            assert!(
                view.term_mode().contains(TermMode::REPORT_ALL_KEYS_AS_ESC),
                "SessionEnd can precede VT cleanup"
            );
            screen(view, "user@host:~$ ");
            let cursor = view.session.as_ref().unwrap().term.lock().grid().cursor.point;
            view.refresh_agent_screen_state(cx);
            assert!(
                !view
                    .term_mode()
                    .intersects(TermMode::KITTY_KEYBOARD_PROTOCOL | TermMode::MOUSE_MODE)
            );
            assert_eq!(view.term_mode().contains(TermMode::ALT_SCREEN), alternate);
            let term = view.session.as_ref().unwrap().term.lock();
            assert_eq!(term.grid().cursor.point, cursor);
            assert!(crate::display::nebula_shell_prompt_restored_from_raw_grid(
                &term,
                "user@host:~$",
                &view.suggest.suggest_env,
                true
            ));
            drop(term);
            if alternate {
                screen(view, "user@host:~$ claude");
                view.suggest.line_buf = "claude".into();
                view.commit_line(cx);
                assert!(
                    view.input_protocol.is_some(),
                    "next shell submission retains ownership on its preserved buffer"
                );
                assert!(view.handle_ai_hook(&hook("no-osc-alt-next", "SessionStart", 1), cx));
                nebula_terminal::event_loop::StreamProcessor::default().feed(
                    &mut view.session.as_ref().unwrap().term.lock(),
                    &proxy,
                    b"\x1b[>31u\x1b[?1000h",
                );
                assert!(!view.session.as_ref().unwrap().term.lock().nebula_shell_input_screen());
                screen(view, "next AI is open");
                assert!(view.handle_ai_hook(&hook("no-osc-alt-next", "SessionEnd", 2), cx));
                screen(view, "user@host:~$ ");
                view.refresh_agent_screen_state(cx);
                assert!(
                    !view
                        .term_mode()
                        .intersects(TermMode::KITTY_KEYBOARD_PROTOCOL | TermMode::MOUSE_MODE)
                );
                assert!(view.session.as_ref().unwrap().term.lock().nebula_shell_input_screen());
            }
        });
    }
}

#[gpui::test]
fn shell_return_before_the_watchdog_recovers_before_runtime_key_encoding(cx: &mut TestAppContext) {
    let (view, window, receiver) = open(cx);
    view.update(window, |view, cx| {
        let (_, _, _, proxy) = session::test_session_with_events();
        view.suggest.suggest_env =
            crate::display::SuggestEnv::Ssh { destination: "fish.test".into() };
        view.session.as_ref().unwrap().term.lock().set_options(nebula_terminal::term::Config {
            kitty_keyboard: true,
            ..Default::default()
        });
        screen(view, "user@host:~$ claude");
        view.suggest.line_buf = "claude".into();
        view.commit_line(cx);
        let mut stream = nebula_terminal::event_loop::StreamProcessor::default();
        stream.feed(
            &mut view.session.as_ref().unwrap().term.lock(),
            &proxy,
            b"\x1b[>31u\x1b[?1000h\x1b[?1006h\r\x1b[2Kuser@host:~$ ",
        );
        // No explicit refresh_agent_screen_state tick or UI output event.
        view.runtime_send_key(
            crate::runtime_api::RuntimeKey::Backspace,
            crate::runtime_api::RuntimeKeyModifiers { control: true, ..Default::default() },
            1,
            cx,
        )
        .unwrap();
        assert!(
            matches!(receiver.try_recv().unwrap(), Msg::Input(bytes) if bytes.as_ref() == b"\x08")
        );
        assert!(
            !view.term_mode().intersects(TermMode::KITTY_KEYBOARD_PROTOCOL | TermMode::MOUSE_MODE)
        );
    });
}

#[gpui::test]
fn a_failed_native_input_recovery_keeps_the_prompt_evidence_for_retry(cx: &mut TestAppContext) {
    let (view, window, _) = open(cx);
    view.update(window, |view, cx| {
        view.suggest.pending_command_prompt = Some("C:\\work>".into());
        view.capture_input_protocol();
        view.mark_command_running();
        view.native_prompt_seen = true;
        view.native_prompt_epoch = Some(view.prompt_input_epoch);
        let root = crate::process_tree::ProcessEntry {
            pid: 1,
            parent_pid: 0,
            depth: 0,
            executable: "cmd.exe".into(),
        };
        view.apply_prompt_process_probe_with_lease(
            view.command_started,
            view.prompt_input_epoch,
            None,
            Ok(vec![root]),
            cx,
        );
        assert_eq!(
            view.native_prompt_epoch,
            Some(view.prompt_input_epoch),
            "failed recovery must retain the marker"
        );
        assert!(view.command_running);
    });
}

#[gpui::test]
fn runtime_protocol_recovery_does_not_consume_an_echo_barrier_on_control_only_output(
    cx: &mut TestAppContext,
) {
    let (view, window, receiver) = open(cx);
    view.update(window, |view, cx| {
        let (_, _, _, proxy) = session::test_session_with_events();
        view.suggest.suggest_env =
            crate::display::SuggestEnv::Ssh { destination: "fish.test".into() };
        screen(view, "user@host:~$ ");
        view.runtime_prompt("claude".into(), true, cx).unwrap();
        assert!(
            view.input_protocol.is_some(),
            "runtime submission captures its confirmed idle prompt"
        );
        assert!(
            matches!(receiver.try_recv().unwrap(), Msg::Input(bytes) if bytes.as_ref() == b"claude")
        );
        let mut stream = nebula_terminal::event_loop::StreamProcessor::default();
        stream.feed(&mut view.session.as_ref().unwrap().term.lock(), &proxy, b"\x1b[?1006h");
        view.process_event(TermEvent::Wakeup, cx);
        assert!(
            view.pending_runtime_submit.is_some(),
            "control-only output is not the submitted command returning"
        );
        assert!(receiver.try_recv().is_err());
        stream.feed(&mut view.session.as_ref().unwrap().term.lock(), &proxy, b"claude");
        view.process_event(TermEvent::Wakeup, cx);
        assert!(
            matches!(receiver.try_recv().unwrap(), Msg::Input(bytes) if bytes.as_ref() == b"\r")
        );
    });
}

#[gpui::test]
fn protocol_recovery_does_not_reset_new_negotiation_or_a_pre_echo_prompt(cx: &mut TestAppContext) {
    let (view, window, _) = open(cx);
    view.update(window, |view, cx| {
        let (session, _, _, proxy) = session::test_session_with_events();
        view.session = Some(session);
        view.suggest.suggest_env =
            crate::display::SuggestEnv::Ssh { destination: "fish.test".into() };
        screen(view, "user@host:~$ ");
        view.suggest.pending_command_prompt = Some("user@host:~$".into());
        view.capture_input_protocol();
        view.input_protocol_owner_exited();
        assert!(!view.recover_input_protocol_at_prompt(), "no new output, no shell return");
        view.session.as_ref().unwrap().term.lock().set_options(nebula_terminal::term::Config {
            kitty_keyboard: true,
            ..Default::default()
        });
        let mut stream = nebula_terminal::event_loop::StreamProcessor::default();
        stream.feed(
            &mut view.session.as_ref().unwrap().term.lock(),
            &proxy,
            b"\x1b]133;D;0\x07\x1b]133;C\x07\x1b[>31u\x1b[?1000h",
        );
        assert!(
            !view.recover_input_protocol_at_prompt(),
            "old SessionEnd cannot reset a new owner"
        );
        assert!(
            view.term_mode()
                .contains(TermMode::REPORT_ALL_KEYS_AS_ESC | TermMode::MOUSE_REPORT_CLICK)
        );
        view.clear_foreground_agent_state(cx); // delayed UI projection must not touch new modes
        assert!(view.term_mode().contains(TermMode::REPORT_ALL_KEYS_AS_ESC));
    });
}

#[gpui::test]
fn local_protocol_recovery_rejects_output_arriving_during_a_process_probe(cx: &mut TestAppContext) {
    let (view, window, _) = open(cx);
    view.update(window, |view, cx| {
        let (session, _, _, proxy) = session::test_session_with_events();
        view.session = Some(session);
        view.suggest.suggest_env = crate::display::SuggestEnv::Local;
        screen(view, "user@host:~$ ");
        view.suggest.pending_command_prompt = Some("user@host:~$".into());
        view.capture_input_protocol();
        view.mark_command_running();
        view.session.as_ref().unwrap().term.lock().set_options(nebula_terminal::term::Config {
            kitty_keyboard: true,
            ..Default::default()
        });
        let stale = view.input_protocol_probe();
        let mut stream = nebula_terminal::event_loop::StreamProcessor::default();
        stream.feed(
            &mut view.session.as_ref().unwrap().term.lock(),
            &proxy,
            b"\x1b[>31u\x1b[?1000h",
        );
        view.apply_prompt_process_probe_with_lease(
            view.command_started,
            view.prompt_input_epoch,
            stale,
            Ok(Vec::new()),
            cx,
        );
        assert!(view.command_running);
        assert!(view.term_mode().contains(TermMode::REPORT_ALL_KEYS_AS_ESC));
        let fresh = view.input_protocol_probe();
        view.apply_prompt_process_probe_with_lease(
            view.command_started,
            view.prompt_input_epoch,
            fresh,
            Ok(Vec::new()),
            cx,
        );
        assert!(!view.command_running);
        assert!(
            !view.term_mode().intersects(TermMode::KITTY_KEYBOARD_PROTOCOL | TermMode::MOUSE_MODE)
        );
    });
}

#[gpui::test]
fn pi_redraw_anchor_follows_accepted_hook_and_command_lifecycle(cx: &mut TestAppContext) {
    use nebula_terminal::term::test::TermSize;
    use nebula_terminal::vte::ansi::Processor;

    let (view, window, _) = open(cx);
    view.update(window, |view, cx| {
        let event = |kind: &str, seq: u64| {
            crate::ai_hook::parse_remote_envelope(
                format!("nebula-hook/1 source=pi pane=42\n{{\"kind\":\"{kind}\",\"session_id\":\"redraw-policy\",\"bridge_sequence\":{seq}}}").as_bytes(),
                Some(42),
            ).unwrap()
        };
        let check = |view: &mut TerminalView, expected: usize| {
            let mut term = view.session.as_ref().unwrap().term.lock();
            term.resize(TermSize::new(60, 5));
            let mut stream = Processor::<nebula_terminal::vte::ansi::StdSyncHandler>::default();
            let text = (0..12).map(|n| format!("ROW_{n:03}")).collect::<Vec<_>>().join("\r\n");
            let frame = format!("\x1b[?2026h\x1b[2J\x1b[H\x1b[3J{text}\x1b[?2026l");
            term.scroll_display(Scroll::Bottom);
            stream.advance(&mut *term, frame.as_bytes());
            term.scroll_display(Scroll::Top);
            term.scroll_display(Scroll::Delta(-2));
            stream.advance(&mut *term, frame.as_bytes());
            assert_eq!(term.grid().display_offset(), expected);
        };
        check(view, 0);
        assert!(view.handle_ai_hook(&event("session-start", 1), cx));
        check(view, 5);
        assert!(view.handle_ai_hook(&event("done", 2), cx));
        check(view, 5); // 回合结束不是 Pi 进程退出。
        assert!(view.handle_ai_hook(&event("session-end", 3), cx));
        check(view, 0);
        assert!(view.handle_ai_hook(&event("session-start", 4), cx));
        check(view, 5);
        view.finish_foreground_command(Some(0), cx);
        check(view, 0);
    });
}

fn screen(view: &mut TerminalView, text: &str) {
    feed(view, format!("\x1b[2J\x1b[H{}", text.replace('\n', "\r\n")).as_bytes());
}

#[gpui::test]
fn codex_shortcuts_footer_recovers_identity_inside_a_nested_shell(cx: &mut TestAppContext) {
    let (view, window, _) = open(cx);
    view.update(window, |view, cx| {
        view.running_program = Some("zsh".into());
        screen(
            view,
            "• Ran cargo test\n\n› Ask Codex to do anything\n\n  GPT-6-Sol xhigh · ~/projects/pokemon\n  ? for shortcuts  ⚠ 8 warnings · f2 to view",
        );
        assert!(view.runtime_agent().is_none());
        view.refresh_agent_screen_state(cx);
        assert_eq!(view.runtime_agent().unwrap().kind, "codex");
    });
}

#[gpui::test]
fn hook_lifecycle_cannot_be_rewritten_by_screen_words_or_idle_samples(cx: &mut TestAppContext) {
    let (view, window, _) = open(cx);
    view.update(window, |view, cx| {
        view.handle_ai_hook(&hook("authority", "UserPromptSubmit", 1), cx);
        for text in [
            "code: contains = [\"(y/n)\"]\n❯ \n? for shortcuts",
            "Do you want to proceed?\n❯ 1. Yes\n2. No\nEsc to cancel",
            "────────\n❯ \n────────\n? for shortcuts",
        ] {
            screen(view, text);
            for _ in 0..6 {
                view.refresh_agent_screen_state(cx);
            }
            assert_eq!(view.agent_activity.status(), AgentStatus::Working, "{text}");
            assert_eq!(view.agent_activity.source(), AgentStatusSource::Hook);
        }
        screen(view, "Do you want to proceed?\n❯ 1. Yes\n2. No\nEsc to cancel");
        view.handle_ai_hook(&hook("authority", "Stop", 2), cx);
        view.refresh_agent_screen_state(cx);
        assert_eq!(view.agent_activity.status(), AgentStatus::Done, "Stop is a terminal event");
        view.handle_ai_hook(&hook("authority", "PermissionRequest", 3), cx);
        assert_eq!(view.agent_activity.status(), AgentStatus::Blocked);
        screen(view, "────────\n❯ \n────────\n? for shortcuts");
        for _ in 0..6 {
            view.refresh_agent_screen_state(cx);
        }
        assert_eq!(
            view.agent_activity.status(),
            AgentStatus::Blocked,
            "only the owner can resolve its request"
        );
        view.handle_ai_hook(&hook("authority", "PostToolUse", 4), cx);
        assert_eq!(view.agent_activity.status(), AgentStatus::Working);
    });
}

#[gpui::test]
fn nested_agent_hooks_do_not_finish_or_replace_the_primary_turn(cx: &mut TestAppContext) {
    let (view, window, _) = open(cx);
    view.update(window, |view, cx| {
        let mut main = hook("primary", "UserPromptSubmit", 1);
        main.agent_pid = Some(100);
        view.handle_ai_hook(&main, cx);
        for (index, name) in
            ["SessionStart", "Stop", "PermissionRequest", "SessionEnd"].iter().enumerate()
        {
            let mut nested = hook("nested", name, index as u64 + 1);
            nested.agent_pid = Some(200);
            view.handle_ai_hook(&nested, cx);
            assert_eq!(view.agent_activity.status(), AgentStatus::Working, "nested {name}");
            assert_eq!(view.ai_session.as_ref().unwrap().session_id, "primary");
        }
    });
}

#[gpui::test]
fn shell_metadata_cannot_replace_a_hook_owned_agent_identity(cx: &mut TestAppContext) {
    let (view, window, _) = open(cx);
    view.update(window, |view, cx| {
        view.handle_ai_hook(&hook("hook-owned-identity", "UserPromptSubmit", 1), cx);
        view.suggest.last_committed = "cc --resume".into();
        for title in ["NEBULA|/project|main|node", "NEBULA|/project|main"] {
            view.process_event(TermEvent::Title(title.into()), cx);
            view.on_command_start(cx);
            assert_eq!(view.running_program.as_deref(), Some("claude"));
            assert_eq!(view.ai_session.as_ref().unwrap().session_id, "hook-owned-identity");
            assert_eq!(view.agent_activity.source(), AgentStatusSource::Hook);
            assert_eq!(view.agent_activity.status(), AgentStatus::Working);
        }
    });
}

#[gpui::test]
fn command_end_clears_progress_and_stale_agent_state(cx: &mut TestAppContext) {
    let (view, window, _) = open(cx);
    view.update(window, |view, cx| {
        for code in [0, 1] {
            view.suggest.last_committed = "pi update --extensions".into();
            view.on_command_start(cx);
            view.process_event(TermEvent::Progress { state: 3, value: None }, cx);
            assert_eq!(view.sidebar_activity(), SidebarActivity::Running);
            view.process_event(TermEvent::CommandDone { exit_code: Some(code) }, cx);
            screen(view, "◦ Working (1s • esc to interrupt)\n› Ask Codex to do anything");
            view.refresh_agent_screen_state(cx);
            assert_eq!(view.progress, crate::taskbar::TaskProgress::None);
            assert_eq!(view.agent_activity.status(), AgentStatus::Unknown);
            assert_eq!(
                view.sidebar_activity(),
                if code == 0 { SidebarActivity::Idle } else { SidebarActivity::CommandFailed }
            );
        }
    });
}

#[gpui::test]
fn failed_local_snapshot_does_not_finish_a_shell_builtin(cx: &mut TestAppContext) {
    let (view, window, _) = open(cx);
    view.update(window, |view, cx| {
        view.suggest.last_committed = "Start-Sleep -Seconds 30".into();
        view.suggest.suggest_env = crate::display::SuggestEnv::Local;
        view.on_command_start(cx);
        view.session.as_mut().unwrap().shell_pid = u32::MAX;
        view.command_started = Some(std::time::Instant::now() - std::time::Duration::from_secs(4));
        view.reconcile_shell_activity(cx);
        assert!(!view.command_running_disproved);
        assert_eq!(view.sidebar_activity(), SidebarActivity::Running);
    });
}

#[gpui::test]
fn ssh_shell_prompt_return_clears_a_command_without_a_done_marker(cx: &mut TestAppContext) {
    let (view, window, _) = open(cx);
    view.update(window, |view, cx| {
        view.suggest.suggest_env =
            crate::display::SuggestEnv::Ssh { destination: "user@example.test".into() };
        view.suggest.last_committed = "sleep 30".into();
        view.suggest.pending_command_prompt = Some("user@host:~$".into());
        view.on_command_start(cx);
        screen(view, "quiet remote work");
        for _ in 0..6 {
            view.refresh_agent_screen_state(cx);
        }
        assert_eq!(view.sidebar_activity(), SidebarActivity::Running, "silence is not completion");
        screen(view, "user@host:~$ next command");
        view.refresh_agent_screen_state(cx);
        assert_eq!(
            view.sidebar_activity(),
            SidebarActivity::Running,
            "nonempty draft is not an empty prompt"
        );
        screen(view, "user@host:~$ ");
        view.refresh_agent_screen_state(cx);
        assert_eq!(view.sidebar_activity(), SidebarActivity::Idle);
        assert!(view.running_program.is_none());
        assert!(view.command_started.is_none());
    });
}

#[gpui::test]
fn output_rule_source_does_not_turn_a_working_codex_into_attention(cx: &mut TestAppContext) {
    let (view, window, _) = open(cx);
    view.update(window, |view, cx| {
        view.running_program = Some("codex".into());
        screen(view, "  { contains = [\"(y/n)\"] },\n  { contains = [\"[y/n]\"] },\n]\n\n◦ Working (12m 36s • esc to interrupt)\n\n› Ask Codex to do anything");
        view.refresh_agent_screen_state(cx);
        assert_eq!(view.agent_activity.status(), AgentStatus::Working);
        assert_eq!(view.sidebar_activity(), SidebarActivity::Running);
    });
}

#[gpui::test]
fn ignored_idle_notification_does_not_reopen_the_hook_ordering_gate(cx: &mut TestAppContext) {
    let (view, window, _) = open(cx);
    view.update(window, |view, cx| {
        for name in ["UserPromptSubmit", "Stop", "Notification", "PostToolUse"] {
            let wire = format!("nebula-hook/1 source=claude\n{{\"hook_event_name\":\"{name}\",\"session_id\":\"idle-notification-order\"}}");
            let event = crate::ai_hook::parse_remote_envelope(wire.as_bytes(), Some(42)).unwrap();
            view.handle_ai_hook(&event, cx);
            if name != "UserPromptSubmit" {
                assert_eq!(view.agent_activity.status(), AgentStatus::Done, "{name}");
            }
        }
    });
}

#[gpui::test]
fn ssh_bell_does_not_report_a_blocked_or_finished_command(cx: &mut TestAppContext) {
    let (view, window, _) = open(cx);
    view.update(window, |view, cx| {
        view.suggest.suggest_env =
            crate::display::SuggestEnv::Ssh { destination: "user@example.test".into() };
        view.suggest.last_committed = "sleep 30".into();
        view.on_command_start(cx);
        view.on_bell(cx);
        assert_eq!(view.sidebar_activity(), SidebarActivity::Running);
        assert!(!view.awaiting_input);
    });
}

#[gpui::test]
fn claude_shift_enter_reaches_pty_without_committing_shell_history(cx: &mut TestAppContext) {
    let (view, window, receiver) = open(cx);
    window.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.running_program = Some("claude".into());
            view.suggest.last_committed = "claude".into();
            view.suggest.line_buf = "a multiline prompt".into();
            feed(view, b"\x1b[?9001h");
            view.on_key_down(
                &KeyDownEvent {
                    keystroke: gpui::Keystroke::parse("shift-enter").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                },
                window,
                cx,
            );
            assert_eq!(view.suggest.last_committed, "claude");
            assert!(
                view.suggest.line_buf.is_empty(),
                "multiline invalidates the single-line shadow"
            );
        });
    });
    let bytes: Vec<u8> = receiver
        .try_iter()
        .filter_map(|message| match message {
            Msg::Input(bytes) => Some(bytes.into_owned()),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(bytes, b"\n");
}
