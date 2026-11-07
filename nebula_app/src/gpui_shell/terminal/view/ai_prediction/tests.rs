use super::*;
use gpui::TestAppContext;
use nebula_terminal::event_loop::Msg;

fn local(view: &mut TerminalView, cx: &mut Context<TerminalView>) {
    view.suggest.suggest_env = crate::display::SuggestEnv::Local;
    view.suggest.completion_shell_report(crate::completion_context::SHELL_VAR, "zsh:prediction");
    view.completion_editor.advertise("zsh:prediction");
    view.session.as_ref().unwrap().term.lock().nebula_add_prompt_mark();
    view.cursor_window_active = true;
    view.cursor_pane_focused = true;
    view.ghost_enabled = true;
    cx.global_mut::<Settings>().command_ai = nebula_settings::CommandAi {
        provider_id: "prediction-fixture".into(),
        model: "fixture".into(),
        prediction_enabled: true,
    };
}

fn tick(window: &gpui::VisualTestContext) {
    for _ in 0..200 {
        if !window.executor().tick() {
            break;
        }
    }
}

#[test]
fn prefix_admission_requires_completed_empty_local_sources_and_exact_end_caret() {
    use crate::completion::command_prediction_eligible as eligible;
    assert!(eligible("  printf '中文😀", "  printf '中文😀".len(), true, false));
    for line in ["", "   ", "a\nb", "a\tb", "a\u{2028}b", &"x".repeat(4097)] {
        assert!(!eligible(line, line.len(), true, false));
    }
    assert!(!eligible("abc", 2, true, false));
    assert!(!eligible("abc", 3, false, false));
    assert!(!eligible("abc", 3, true, true));
}

#[gpui::test]
fn debounce_observes_at_500ms_and_typing_restarts_it(cx: &mut TestAppContext) {
    let (view, window, receiver) = super::super::startup_tests::open(cx);
    view.update(window, |view, cx| {
        local(view, cx);
        view.schedule_ai_prediction(cx);
    });
    tick(window);
    window.executor().advance_clock(Duration::from_millis(499));
    tick(window);
    assert!(!receiver.try_iter().any(|m| matches!(m, Msg::Input(_))));
    view.update(window, |view, cx| {
        view.invalidate_ai_prediction();
        view.prompt_input_epoch += 1;
        view.schedule_ai_prediction(cx);
    });
    tick(window);
    window.executor().advance_clock(Duration::from_millis(499));
    tick(window);
    assert!(!receiver.try_iter().any(|m| matches!(m, Msg::Input(_))));
    window.executor().advance_clock(Duration::from_millis(1));
    tick(window);
    let inputs: Vec<_> = receiver
        .try_iter()
        .filter_map(|m| match m {
            Msg::Input(b) => Some(b.into_owned()),
            _ => None,
        })
        .collect();
    assert_eq!(inputs, [b"\x1b[45~".to_vec()]);
    view.read_with(window, |view, _| assert!(view.ai_prediction.observing));
}

#[gpui::test]
fn disabled_unfocused_ime_remote_guest_and_foreground_never_observe(cx: &mut TestAppContext) {
    let (view, window, receiver) = super::super::startup_tests::open(cx);
    for case in
        ["disabled", "ghost", "focus", "ime", "ssh", "wsl", "guest", "owner", "foreground", "tui"]
    {
        view.update(window, |view, cx| {
            local(view, cx);
            view.invalidate_ai_prediction();
            view.prompt_input_epoch += 1;
            match case {
                "disabled" => cx.global_mut::<Settings>().command_ai.prediction_enabled = false,
                "ghost" => view.ghost_enabled = false,
                "focus" => view.cursor_pane_focused = false,
                "ime" => view.marked_text = Some("组合".into()),
                "ssh" => view.ssh_destination = Some("host".into()),
                "wsl" => {
                    view.suggest.suggest_env =
                        crate::display::SuggestEnv::Wsl { distro: "fixture".into() }
                },
                "guest" => {
                    view.suggest.suggest_env =
                        crate::display::SuggestEnv::Ssh { destination: "nested".into() }
                },
                "owner" => {
                    view.completion_editor.advertise("fish:unsupported");
                },
                "foreground" => view.session.as_ref().unwrap().term.lock().nebula_end_prompt(),
                "tui" => super::super::startup_tests::feed(view, b"\x1b[?1049h"),
                _ => unreachable!(),
            }
            view.schedule_ai_prediction(cx);
            assert!(view.ai_prediction.timer.is_none(), "{case}");
            view.marked_text = None;
            view.ssh_destination = None;
            super::super::startup_tests::feed(view, b"\x1b[?1049l");
        });
    }
    assert!(!receiver.try_iter().any(|m| matches!(m, Msg::Input(_))));
}

#[gpui::test]
fn ai_acceptance_requeries_unicode_prefix_never_submits_and_escape_suppresses(
    cx: &mut TestAppContext,
) {
    let (view, window, receiver) = super::super::startup_tests::open(cx);
    for mode in [CompletionStyle::Inline, CompletionStyle::Popup, CompletionStyle::Hybrid] {
        view.update(window, |view, cx| {
            local(view, cx);
            view.prompt_input_epoch += 1;
            view.completion_style = mode;
            view.ai_prediction.observing = true;
            assert!(view.observe_completion_editor(cx));
            view.handle_completion_editor_report(
                "pebrel_editor",
                "zsh:prediction\nutf8\n14\nprintf '中文",
                cx,
            );
        });
        // A verified baseline is independent of the model reply in this editor-adoption fixture.
        view.update(window, |view, _| {
            let (identity, shell) = view.ai_search_identity().unwrap();
            let snapshot =
                view.completion_editor.snapshot(view.prompt_input_epoch).unwrap().clone();
            view.ai_prediction.request = Some(Request { identity, shell, snapshot });
            view.ai_prediction.candidate = Some(NebulaCompletionItem {
                label: "printf '中文😀'".into(),
                insert: "😀'".into(),
                replace_chars: 0,
                replace_after_chars: 0,
                kind: NebulaCompletionKind::Ai,
            });
            view.present_ai_prediction();
        });
        receiver.try_iter().for_each(drop);
        view.update(window, |view, cx| {
            assert!(
                view.prediction_key(
                    if mode == CompletionStyle::Hybrid { "right" } else { "tab" },
                    cx
                )
            );
            assert!(view.ai_prediction.verifying);
            view.handle_completion_editor_report(
                "pebrel_editor",
                "zsh:prediction\nutf8\n14\nprintf '中文",
                cx,
            );
        });
        let bytes: Vec<u8> = receiver
            .try_iter()
            .filter_map(|m| match m {
                Msg::Input(b) => Some(b.into_owned()),
                _ => None,
            })
            .flatten()
            .collect();
        assert_eq!(bytes, [b"\x1b[45~".as_slice(), "😀'".as_bytes()].concat());
        assert!(!bytes.contains(&b'\r') && !bytes.contains(&b'\n'));
    }
    view.update(window, |view, cx| {
        local(view, cx);
        view.ai_prediction.candidate = Some(NebulaCompletionItem {
            label: "abc".into(),
            insert: "bc".into(),
            replace_chars: 0,
            replace_after_chars: 0,
            kind: NebulaCompletionKind::Ai,
        });
        assert!(view.prediction_key("escape", cx));
        view.schedule_ai_prediction(cx);
        assert!(view.ai_prediction.timer.is_none());
        view.completion_style = CompletionStyle::Popup;
        view.ai_prediction.candidate = Some(NebulaCompletionItem {
            label: "abc".into(),
            insert: "bc".into(),
            replace_chars: 0,
            replace_after_chars: 0,
            kind: NebulaCompletionKind::Ai,
        });
        view.present_ai_prediction();
        assert!(
            !view.handle_completion_key("enter", cx),
            "Enter stays native submission, never AI adoption"
        );
        assert!(!view.has_ai_prediction() && view.suggest.completion_items.is_empty());
    });
}

#[gpui::test]
fn passive_observation_never_blocks_enter_or_replays_tab_on_timeout(cx: &mut TestAppContext) {
    let (view, window, receiver) = super::super::startup_tests::open(cx);
    view.update(window, |view, cx| {
        local(view, cx);
        view.ai_prediction.observing = true;
        assert!(view.observe_completion_editor(cx));
        assert!(!view.handle_completion_key("enter", cx));
        assert!(!view.completion_editor.is_querying());
        view.ai_prediction.observing = true;
        assert!(view.observe_completion_editor(cx));
    });
    tick(window);
    window.executor().advance_clock(Duration::from_secs(1));
    tick(window);
    view.read_with(window, |view, _| assert!(!view.ai_prediction.observing));
    let bytes: Vec<_> = receiver
        .try_iter()
        .filter_map(|m| match m {
            Msg::Input(b) => Some(b.into_owned()),
            _ => None,
        })
        .collect();
    assert_eq!(bytes, [b"\x1b[45~".to_vec(), b"\x1b[45~".to_vec()]);
}

#[gpui::test]
fn real_local_calculation_wins_before_prediction_and_context_changes_cancel_timer(
    cx: &mut TestAppContext,
) {
    let (view, window, receiver) = super::super::startup_tests::open(cx);
    view.update(window, |view, cx| {
        local(view, cx);
        view.ai_prediction.observing = true;
        assert!(view.observe_completion_editor(cx));
        view.handle_completion_editor_report(
            "pebrel_editor",
            "zsh:prediction\nutf8\n6\ngit sw",
            cx,
        );
    });
    window.run_until_parked();
    view.read_with(window, |view, _| {
        assert!(!view.suggest.suggestion.is_empty() || !view.suggest.completion_items.is_empty());
        assert!(view.ai_search_job.is_none() && !view.has_ai_prediction());
    });
    receiver.try_iter().for_each(drop);
    view.update(window, |view, cx| {
        view.prompt_input_epoch += 1;
        view.schedule_ai_prediction(cx);
        view.cwd.push_str("/changed-before-idle");
    });
    tick(window);
    window.executor().advance_clock(IDLE);
    tick(window);
    assert!(!receiver.try_iter().any(|m| matches!(m, Msg::Input(_))));
}

#[gpui::test]
fn completion_keys_before_receipt_cancel_publication_but_retain_the_actual_lane(
    cx: &mut TestAppContext,
) {
    let (view, window, _) = super::super::startup_tests::open(cx);
    for key in ["enter", "tab", "right"] {
        view.update(window, |view, cx| {
            local(view, cx);
            let cancelled = Arc::new(AtomicBool::new(false));
            let task = cx.spawn(async |_, _| std::future::pending::<()>().await);
            view.ai_search_job = Some(ai_search::Job {
                cancelled: cancelled.clone(),
                revision: 1,
                prediction: true,
                _task: task,
            });
            assert!(!view.prediction_key(key, cx));
            assert!(cancelled.load(Ordering::Acquire));
            assert!(
                view.ai_search_job.is_some(),
                "publication cancellation cannot release a blocking HTTP lane"
            );
        });
    }
}

#[gpui::test]
fn adoption_rechecks_cwd_and_config_changes_after_its_final_observe(cx: &mut TestAppContext) {
    let (view, window, receiver) = super::super::startup_tests::open(cx);
    for case in ["cwd", "config"] {
        view.update(window, |view, cx| {
            local(view, cx);
            view.prompt_input_epoch += 1;
            view.ai_prediction.observing = true;
            assert!(view.observe_completion_editor(cx));
            view.handle_completion_editor_report(
                "pebrel_editor",
                "zsh:prediction\nutf8\n3\nabc",
                cx,
            );
            let (identity, shell) = view.ai_search_identity().unwrap();
            let snapshot =
                view.completion_editor.snapshot(view.prompt_input_epoch).unwrap().clone();
            view.ai_prediction.request = Some(Request { identity, shell, snapshot });
            view.ai_prediction.candidate = Some(NebulaCompletionItem {
                label: "abcd".into(),
                insert: "d".into(),
                replace_chars: 0,
                replace_after_chars: 0,
                kind: NebulaCompletionKind::Ai,
            });
            view.present_ai_prediction();
            receiver.try_iter().for_each(drop);
            assert!(view.prediction_key("right", cx));
            assert!(view.ai_prediction.verifying);
            if case == "cwd" {
                view.cwd.push_str("/changed-during-observe");
            } else {
                crate::ai_commands::invalidate_config();
            }
            // No render/settings callback between this mutation and the native reply.
            view.handle_completion_editor_report(
                "pebrel_editor",
                "zsh:prediction\nutf8\n3\nabc",
                cx,
            );
            assert!(!view.has_ai_prediction(), "{case}: stale candidate must be discarded");
        });
        let bytes: Vec<_> = receiver
            .try_iter()
            .filter_map(|msg| match msg {
                Msg::Input(bytes) => Some(bytes.into_owned()),
                _ => None,
            })
            .collect();
        assert_eq!(bytes, [b"\x1b[45~".to_vec()], "{case}: only observation may reach PTY");
    }
}

#[gpui::test]
fn escape_while_http_is_pending_stays_native_without_rearming_prediction(cx: &mut TestAppContext) {
    let (view, window, receiver) = super::super::startup_tests::open(cx);
    let expected = window.update(|_, cx| {
        view.update(cx, |view, cx| {
            local(view, cx);
            let keystroke = gpui::Keystroke::parse("escape").unwrap();
            // Exercise a prompt-preserving Win32 packet without claiming native OS encoding.
            let expected = b"\x1b[27;1;27;1;0;1_\x1b[27;1;27;0;0;1_".to_vec();
            assert!(super::super::super::event_mailbox::preserves_native_prompt(&expected));
            let cancelled = Arc::new(AtomicBool::new(false));
            let task = cx.spawn(async |_, _| std::future::pending::<()>().await);
            view.ai_search_job = Some(ai_search::Job {
                cancelled: cancelled.clone(),
                revision: view.prompt_input_epoch,
                prediction: true,
                _task: task,
            });
            view.ai_prediction.attempted = Some(view.prompt_input_epoch);
            assert!(!view.handle_completion_key("escape", cx));
            view.write_user_key(keystroke, expected.clone(), cx);
            assert!(cancelled.load(Ordering::Acquire));
            assert!(view.ai_search_job.is_some(), "retain the actual blocking lane");
            assert!(
                view.prediction_enabled(cx),
                "the fixture must retain an eligible native prompt after Escape"
            );
            assert!(
                view.ai_prediction.timer.is_none(),
                "Escape itself cannot count as fresh prediction input"
            );
            view.write_user_text("x".into(), false, b"x".to_vec(), cx);
            assert!(view.ai_prediction.timer.is_some(), "the next real edit can predict again");
            expected
        })
    });
    let bytes: Vec<_> = receiver
        .try_iter()
        .filter_map(|msg| match msg {
            Msg::Input(bytes) => Some(bytes.into_owned()),
            _ => None,
        })
        .collect();
    assert_eq!(
        bytes,
        [expected, b"x".to_vec()],
        "without a candidate, Escape still reaches the Shell"
    );
}
