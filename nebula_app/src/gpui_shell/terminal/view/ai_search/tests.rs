use super::*;
use gpui::{EntityInputHandler as _, TestAppContext};
use nebula_terminal::event_loop::Msg;

fn local(view: &mut TerminalView, cx: &mut Context<TerminalView>) {
    view.suggest.suggest_env = crate::display::SuggestEnv::Local;
    view.suggest.completion_shell_report(crate::completion_context::SHELL_VAR, "zsh:fixture");
    view.completion_editor.advertise("zsh:fixture");
    view.session.as_ref().unwrap().term.lock().nebula_add_prompt_mark();
    view.cursor_window_active = true;
    view.output_visible = true;
    cx.global_mut::<Settings>().command_ai = nebula_settings::CommandAi {
        provider_id: "ai-search-test".into(),
        model: "fixture-model".into(),
        prediction_enabled: false,
    };
}

#[gpui::test]
fn capture_preserves_early_query_input_and_cancel_writes_nothing(cx: &mut TestAppContext) {
    let (view, window, receiver) = super::super::startup_tests::open(cx);
    view.update(window, |view, cx| local(view, cx));
    window.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.toggle_ai_search(window, cx);
            let input = view.ai_search.as_ref().unwrap().input.clone();
            input.update(cx, |input, cx| {
                input.replace_text_in_range(None, "新需求", window, cx);
            });
            view.handle_completion_editor_report(
                "pebrel_editor",
                "zsh:fixture\nutf8\n5\necho 中文",
                cx,
            );
            view.ai_search_chrome(window, cx);
            let search = view.ai_search.as_ref().unwrap();
            assert_eq!(search.input.read(cx).value().as_ref(), "echo 中文新需求");
            assert_eq!(search.original.as_ref().unwrap().cursor, 5);
            assert!(view.ai_search_job.is_none());
            view.close_ai_search(cx);
        });
    });
    window.run_until_parked();
    let bytes: Vec<_> = receiver
        .try_iter()
        .filter_map(|msg| match msg {
            Msg::Input(bytes) => Some(bytes.into_owned()),
            _ => None,
        })
        .collect();
    assert_eq!(bytes, [b"\x1b[45~".to_vec()], "only the private read-only query reaches PTY");
}

#[gpui::test]
fn composition_enter_never_generates_or_sends_query_to_shell(cx: &mut TestAppContext) {
    let (view, window, receiver) = super::super::startup_tests::open(cx);
    view.update(window, |view, cx| local(view, cx));
    window.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.toggle_ai_search(window, cx);
            view.handle_completion_editor_report("pebrel_editor", "zsh:fixture\nutf8\n0\n", cx);
            view.ai_search_chrome(window, cx);
            let input = view.ai_search.as_ref().unwrap().input.clone();
            input.update(cx, |input, cx| {
                input.replace_and_mark_text_in_range(None, "查找进程", None, window, cx);
                cx.emit(InputEvent::PressEnter { secondary: false, shift: false });
            });
        });
    });
    window.run_until_parked();
    view.read_with(window, |view, _| assert!(view.ai_search_job.is_none()));
    let bytes: Vec<_> = receiver
        .try_iter()
        .filter_map(|msg| match msg {
            Msg::Input(bytes) => Some(bytes.into_owned()),
            _ => None,
        })
        .collect();
    assert_eq!(bytes, [b"\x1b[45~".to_vec()]);
}

#[gpui::test]
fn adoption_rechecks_full_original_and_never_appends_enter(cx: &mut TestAppContext) {
    let (view, window, receiver) = super::super::startup_tests::open(cx);
    view.update(window, |view, cx| local(view, cx));
    window.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.toggle_ai_search(window, cx);
            view.handle_completion_editor_report(
                "pebrel_editor",
                "zsh:fixture\nutf8\n5\necho 中文",
                cx,
            );
            view.ai_search_chrome(window, cx);
            view.ai_search.as_mut().unwrap().result = Some(SearchResult {
                command: "echo adopted".into(),
                explanation: "fixture".into(),
            });
            view.adopt_ai_search(cx);
            view.handle_completion_editor_report(
                "pebrel_editor",
                "zsh:fixture\nutf8\n5\necho changed",
                cx,
            );
            assert_eq!(
                view.ai_search.as_ref().unwrap().status,
                Some(Message::AiCommandSearchStale)
            );
            assert!(view.ai_search.is_some(), "a stale draft must retain the query");
        });
    });
    let before: Vec<_> = receiver
        .try_iter()
        .filter_map(|msg| match msg {
            Msg::Input(bytes) => Some(bytes.into_owned()),
            _ => None,
        })
        .collect();
    assert_eq!(before, [b"\x1b[45~".to_vec(), b"\x1b[45~".to_vec()]);
    window.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.ai_search.as_mut().unwrap().result = Some(SearchResult {
                command: "echo adopted".into(),
                explanation: "fixture".into(),
            });
            view.adopt_ai_search(cx);
            view.handle_completion_editor_report(
                "pebrel_editor",
                "zsh:fixture\nutf8\n5\necho 中文",
                cx,
            );
            assert!(view.ai_search.is_none());
            let _ = window;
        });
    });
    let bytes: Vec<u8> = receiver
        .try_iter()
        .filter_map(|msg| match msg {
            Msg::Input(bytes) => Some(bytes.into_owned()),
            _ => None,
        })
        .flatten()
        .collect();
    assert!(bytes.ends_with(b"echo adopted"));
    assert!(!bytes.contains(&b'\r') && !bytes.contains(&b'\n'));
}

#[gpui::test]
fn observe_timeout_never_falls_back_to_tab(cx: &mut TestAppContext) {
    let (view, window, receiver) = super::super::startup_tests::open(cx);
    view.update(window, |view, cx| {
        local(view, cx);
        assert!(view.observe_completion_editor(cx));
    });
    window.executor().advance_clock(std::time::Duration::from_secs(1));
    window.run_until_parked();
    let bytes: Vec<_> = receiver
        .try_iter()
        .filter_map(|msg| match msg {
            Msg::Input(bytes) => Some(bytes.into_owned()),
            _ => None,
        })
        .collect();
    assert_eq!(bytes, [b"\x1b[45~".to_vec()]);
    view.read_with(window, |view, _| assert!(!view.completion_editor.ready()));
}

#[gpui::test]
fn guest_or_changed_owner_cannot_keep_ai_query(cx: &mut TestAppContext) {
    let (view, window, receiver) = super::super::startup_tests::open(cx);
    view.update(window, |view, cx| local(view, cx));
    window.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.toggle_ai_search(window, cx);
            assert!(view.ai_search.is_some());
            view.suggest.suggest_env = crate::display::SuggestEnv::Wsl { distro: "Debian".into() };
            view.validate_ai_search(cx);
            assert!(view.ai_search.is_none());
            assert!(view.ai_search_identity().is_none());
        });
    });
    assert_eq!(receiver.try_iter().filter(|msg| matches!(msg, Msg::Input(_))).count(), 1);
}

#[gpui::test]
fn unsupported_native_contexts_never_capture_or_request(cx: &mut TestAppContext) {
    for case in [
        "ssh",
        "wsl",
        "guest",
        "wsl-execution",
        "owner",
        "editor",
        "fish",
        "foreground",
        "tui",
        "ime",
    ] {
        let (view, window, receiver) = super::super::startup_tests::open(cx);
        window.update(|window, cx| {
            view.update(cx, |view, cx| {
                local(view, cx);
                match case {
                    "ssh" => view.ssh_destination = Some("fixture.invalid".into()),
                    "wsl" => {
                        view.suggest.suggest_env =
                            crate::display::SuggestEnv::Wsl { distro: "fixture".into() }
                    },
                    "guest" => {
                        view.suggest.suggest_env = crate::display::SuggestEnv::Shell {
                            scope: crate::nebula_history::HistoryScope::Local,
                        }
                    },
                    "wsl-execution" => {
                        view.exec_context =
                            Some(crate::runtime_exec::PaneExecContext::from_pty_options(
                                &nebula_terminal::tty::Options {
                                    shell: Some(nebula_terminal::tty::Shell::new(
                                        "wsl.exe".into(),
                                        vec![],
                                    )),
                                    ..Default::default()
                                },
                            ))
                    },
                    "owner" => {
                        view.completion_editor.advertise("zsh:other-owner");
                    },
                    "editor" => view.completion_editor = editor::Editor::default(),
                    "fish" => {
                        view.suggest.completion_shell_report(
                            crate::completion_context::SHELL_VAR,
                            "fish:fixture",
                        );
                        view.completion_editor.advertise("fish:fixture");
                    },
                    "foreground" => view.session.as_ref().unwrap().term.lock().nebula_end_prompt(),
                    "tui" => super::super::startup_tests::feed(view, b"\x1b[?1049h"),
                    "ime" => view.marked_text = Some("组合".into()),
                    _ => unreachable!(),
                }
                assert!(view.ai_search_identity().is_none(), "{case}");
                view.toggle_ai_search(window, cx);
                assert!(view.ai_search.is_none() && view.ai_search_job.is_none(), "{case}");
            });
        });
        assert!(receiver.try_iter().all(|msg| !matches!(msg, Msg::Input(_))), "{case}");
    }
}

#[gpui::test]
fn rendered_query_owns_paste_undo_ime_and_escape_in_a_narrow_pane(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        crate::gpui_shell::scientific_render::init(cx);
        crate::gpui_shell::terminal::init(cx);
        cx.set_global(Settings::load(nebula_settings::ThemeName::Nord));
    });
    let mut output = None;
    let (_, window) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            TerminalView::new(
                44,
                (40, 16),
                TerminalLaunch::Local {
                    cwd: None,
                    shell: Some(nebula_terminal::tty::Shell::new(
                        "pebrel-test-missing-shell-executable".into(),
                        vec![],
                    )),
                    shell_name: None,
                },
                window,
                cx,
            )
        });
        let receiver = view.update(cx, |view, cx| {
            let receiver = view.install_completion_test_session();
            local(view, cx);
            receiver
        });
        window.focus(&view.read(cx).focus_handle.clone(), cx);
        output = Some((view.clone(), receiver));
        gpui_component::Root::new(view, window, cx)
    });
    let (view, receiver) = output.unwrap();
    window.update(|window, _| window.activate_window());
    window.run_until_parked();
    let broadcast = std::rc::Rc::new(std::cell::Cell::new(0));
    let observed = broadcast.clone();
    let _events = view.update(window, |_, cx| {
        cx.subscribe(&view, move |_, _, event, _| {
            if matches!(event, TerminalViewEvent::UserInput(_)) {
                observed.set(observed.get() + 1);
            }
        })
    });
    window.simulate_resize(gpui::size(px(340.0), px(280.0)));
    window.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.toggle_ai_search(window, cx);
            view.handle_completion_editor_report(
                "pebrel_editor",
                "zsh:fixture\nutf8\n5\necho 中文😀",
                cx,
            );
        });
        window.draw(cx).clear(cx);
    });
    window.run_until_parked();
    window.update(|window, cx| window.draw(cx).clear(cx));
    let panel = window.debug_bounds("terminal-ai-search").expect("painted in-place query");
    assert!(panel.left() >= px(0.0) && panel.right() <= px(340.0));
    assert!(panel.top() >= px(0.0) && panel.bottom() <= px(280.0));
    let modifier = if cfg!(target_os = "macos") { "cmd" } else { "ctrl" };
    window.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("粘贴查询".into())));
    window.simulate_keystrokes(&format!("{modifier}-a {modifier}-v"));
    window.run_until_parked();
    view.read_with(window, |view, cx| {
        assert_eq!(view.ai_search.as_ref().unwrap().input.read(cx).value().as_ref(), "粘贴查询");
        assert!(view.ai_search_job.is_none());
    });
    window.simulate_keystrokes(&format!("{modifier}-z"));
    window.run_until_parked();
    view.read_with(window, |view, cx| {
        assert_eq!(view.ai_search.as_ref().unwrap().input.read(cx).value().as_ref(), "echo 中文😀")
    });
    let command = "  printf '精确复制😀' ";
    view.update(window, |view, cx| {
        view.ai_search.as_mut().unwrap().result =
            Some(SearchResult { command: command.into(), explanation: "fixture".into() });
        cx.notify();
    });
    window.update(|window, cx| window.draw(cx).clear(cx));
    let panel = window.debug_bounds("terminal-ai-search").unwrap();
    window.simulate_event(gpui::ScrollWheelEvent {
        position: panel.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.0), px(-1000.0))),
        modifiers: gpui::Modifiers::default(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    window.update(|window, cx| window.draw(cx).clear(cx));
    let copy = window.debug_bounds("ai-search-copy").expect("copy control is visible").center();
    assert!(panel.contains(&copy), "the copy hit target must be reachable inside the pane");
    window.simulate_click(copy, gpui::Modifiers::default());
    window.run_until_parked();
    window.update(|_, cx| {
        assert_eq!(cx.read_from_clipboard().unwrap().text().as_deref(), Some(command))
    });
    view.read_with(window, |view, cx| {
        let search = view.ai_search.as_ref().expect("copy stays in the owning AI surface");
        assert!(search.feedback.read(cx).is_copied());
    });
    window.update(|window, cx| {
        view.update(cx, |view, cx| {
            let input = view.ai_search.as_ref().unwrap().input.clone();
            input.update(cx, |input, cx| {
                input.replace_and_mark_text_in_range(
                    Some(0.."echo 中文😀".encode_utf16().count()),
                    "组合输入",
                    None,
                    window,
                    cx,
                )
            });
        })
    });
    window.simulate_keystrokes("enter");
    window.run_until_parked();
    view.read_with(window, |view, cx| {
        assert!(!view.ai_search.as_ref().unwrap().feedback.read(cx).is_copied());
    });
    window.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(view.ai_search_job.is_none());
            view.ai_search
                .as_ref()
                .unwrap()
                .input
                .update(cx, |input, cx| input.unmark_text(window, cx));
        })
    });
    window.simulate_keystrokes("escape");
    window.run_until_parked();
    view.read_with(window, |view, _| assert!(view.ai_search.is_none()));
    let bytes: Vec<_> = receiver
        .try_iter()
        .filter_map(|msg| match msg {
            Msg::Input(bytes) => Some(bytes.into_owned()),
            _ => None,
        })
        .collect();
    assert_eq!(
        bytes,
        [b"\x1b[45~".to_vec()],
        "real child input never types, pastes or sends Escape to PTY"
    );
    assert_eq!(broadcast.get(), 0, "AI input never emits terminal broadcast events");

    window.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.toggle_ai_search(window, cx);
            view.handle_completion_editor_report(
                "pebrel_editor",
                "zsh:fixture\nutf8\n5\necho 中文😀",
                cx,
            );
        });
        window.draw(cx).clear(cx);
    });
    window.run_until_parked();
    window.update(|window, cx| {
        assert!(
            !view.read(cx).focus_handle.is_focused(window),
            "query owns focus before departure"
        );
    });
    window.simulate_click(gpui::point(px(4.0), px(270.0)), gpui::Modifiers::default());
    window.update(|window, cx| window.draw(cx).clear(cx));
    window.run_until_parked();
    window.update(|window, cx| assert!(view.read(cx).focus_handle.is_focused(window)));
    view.read_with(window, |view, _| {
        assert!(view.ai_search.is_none(), "returning focus to native input leaves AI mode");
    });
    assert_eq!(
        receiver.try_iter().filter(|msg| matches!(msg, Msg::Input(_))).count(),
        1,
        "focus departure emits only the initial observation, never restored draft bytes"
    );

    for commands in [false, true] {
        window.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.toggle_ai_search(window, cx);
                view.handle_completion_editor_report(
                    "pebrel_editor",
                    "zsh:fixture\nutf8\n5\necho 中文😀",
                    cx,
                );
            });
            window.draw(cx).clear(cx);
        });
        window.run_until_parked();
        window.update(|window, cx| {
            let focus = view.read(cx).focus_handle(cx);
            window.defer(cx, move |window, cx| window.focus(&focus, cx));
        });
        window.run_until_parked();
        window.update(|window, cx| {
            window.draw(cx).clear(cx);
            view.update(cx, |view, cx| {
                let search = view.ai_search.as_ref().expect("workspace refocus keeps AI input");
                assert!(search.input.read(cx).focus_handle(cx).is_focused(window));
                view.open_reading(commands, window, cx);
                assert!(view.ai_search.is_none(), "a different input owner cancels AI immediately");
                assert!(view.ai_search_identity().is_none());
                view.toggle_ai_search(window, cx);
                assert!(view.ai_search.is_none(), "reading and AI must not own input together");
            });
            window.draw(cx).clear(cx);
        });
        window.run_until_parked();
        window.simulate_input("reading query");
        window.update(|window, cx| window.draw(cx).clear(cx));
        window.run_until_parked();
        window.update(|window, cx| {
            view.update(cx, |view, cx| {
                assert_eq!(
                    view.reading.panel.as_ref().unwrap().input.read(cx).value().as_ref(),
                    "reading query"
                );
                view.close_reading(window, cx);
            });
            window.draw(cx).clear(cx);
        });
        window.run_until_parked();
        assert_eq!(receiver.try_iter().filter(|msg| matches!(msg, Msg::Input(_))).count(), 1);
        assert_eq!(broadcast.get(), 0);
    }
}
