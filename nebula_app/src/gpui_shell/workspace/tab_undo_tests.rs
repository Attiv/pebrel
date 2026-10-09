//! Real workspace close/restore lifecycle and document safety regressions.

use super::*;
use crate::session::{LaunchSession, LayoutSession, SplitAxis, TabSession};
use gpui::{Keystroke, TestAppContext, VisualTestContext};

fn press(combo: &str, cx: &mut VisualTestContext) {
    let keystroke = Keystroke::parse(combo).unwrap();
    cx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui::KeyUpEvent { keystroke });
    cx.run_until_parked();
}

struct Fixture {
    workspace: Entity<NebulaWorkspace>,
    _settings_lock: std::sync::MutexGuard<'static, ()>,
    directory: tempfile::TempDir,
    launch: LaunchSession,
}

fn open(cx: &mut TestAppContext) -> (Fixture, VisualTestContext) {
    let settings_lock = crate::gpui_shell::settings_fixture::lock_theme_studio();
    let directory = tempfile::tempdir().unwrap();
    let launch = LaunchSession::Shell {
        name: "Undo fixture".into(),
        program: directory.path().join("missing-undo-shell").to_string_lossy().into_owned(),
        args: Vec::new(),
    };
    let hub = crate::runtime_api::RuntimeHub::new();
    cx.update(|cx| {
        gpui_component::init(cx);
        crate::gpui_shell::math_view::register(cx);
        crate::gpui_shell::file_editor::init(cx);
        super::init(cx);
        windowing::initialize(cx, hub.clone());
        cx.set_global(crate::gpui_shell::config::Settings::load(nebula_settings::ThemeName::Nord));
        cx.set_reduce_motion(true);
    });
    let mut workspace_out = None;
    let (_, window) = cx.add_window_view(|window, cx| {
        let workspace = cx.new(|cx| {
            NebulaWorkspace::new(
                window,
                None,
                None,
                1,
                hub,
                windowing::WorkspaceStartup::Empty,
                windowing::WindowRole::Regular,
                cx,
            )
        });
        workspace.update(cx, |workspace, cx| {
            workspace.add_terminal_with(
                launch.clone(),
                Some(directory.path().to_owned()),
                None,
                window,
                cx,
            );
            workspace.focus_active(window, cx);
        });
        workspace_out = Some(workspace.clone());
        Root::new(workspace, window, cx)
    });
    window.run_until_parked();
    (
        Fixture {
            workspace: workspace_out.unwrap(),
            directory,
            launch,
            _settings_lock: settings_lock,
        },
        window.clone(),
    )
}

#[gpui::test]
fn closed_history_is_bounded_lifo_and_restores_fresh_terminal_layout(cx: &mut TestAppContext) {
    let (fixture, mut cx) = open(cx);
    let cwd = fixture.directory.path().to_string_lossy().into_owned();
    let pane = |name: &str| LayoutSession::Pane {
        custom_name: Some(name.into()),
        cwd: cwd.clone(),
        launch: Some(fixture.launch.clone()),
        agent: None,
    };
    let original = TabSession {
        cwd: cwd.clone(),
        custom_name: Some("Split tab".into()),
        color: Some(Rgb::new(12, 34, 56)),
        launch: Some(fixture.launch.clone()),
        active_pane: 1,
        layout: Some(LayoutSession::Split {
            axis: SplitAxis::LeftRight,
            ratio_permille: 370,
            first: Box::new(pane("Left")),
            second: Box::new(pane("Right")),
        }),
    };
    let old_ids = cx.update(|window, cx| {
        fixture.workspace.update(cx, |workspace, cx| {
            assert!(workspace.restore_tab(&original, false, window, cx));
            let ids = match &mut workspace.tabs[1] {
                WorkspaceTab::Terminal { panes, tree, zoomed, broadcast, .. } => {
                    *zoomed = true;
                    *broadcast = true;
                    panes.reverse();
                    tree.leaves()
                },
                _ => unreachable!(),
            };
            workspace.close_tab(1, window, cx);
            assert_eq!(workspace.closed_tabs.len(), 1);
            assert!(workspace.reopen_closed_tab(window, cx));
            assert_eq!(workspace.snapshot_session(cx).tabs[1], original);
            let WorkspaceTab::Terminal { tree, focused, zoomed, broadcast, panes } =
                &workspace.tabs[1]
            else {
                unreachable!()
            };
            assert_eq!(*focused, tree.leaves()[1]);
            assert!(!zoomed && !broadcast, "unsafe transient input modes must not resume");
            assert!(tree.leaves().iter().all(|id| !ids.contains(id)));
            assert!(panes.iter().all(|pane| !pane.view.read(cx).recovery_pending()));
            workspace.close_tab(1, window, cx);
            for index in 0..33 {
                let mut tab = original.clone();
                tab.custom_name = Some(format!("closed-{index}"));
                assert!(workspace.restore_tab(&tab, false, window, cx));
                workspace.close_tab(1, window, cx);
            }
            assert_eq!(workspace.closed_tabs.len(), 32);
            for index in (1..33).rev() {
                assert!(workspace.reopen_closed_tab(window, cx));
                assert_eq!(
                    workspace.meta(workspace.active).custom_name,
                    Some(format!("closed-{index}"))
                );
            }
            assert!(!workspace.reopen_closed_tab(window, cx));
            ids
        })
    });
    assert_eq!(old_ids.len(), 2);
}

#[gpui::test]
fn missing_file_restore_keeps_history_until_an_open_is_accepted(cx: &mut TestAppContext) {
    let (fixture, mut cx) = open(cx);
    let path = fixture.directory.path().join("retry.txt");
    std::fs::write(&path, "read me").unwrap();
    cx.update(|window, cx| {
        fixture.workspace.update(cx, |workspace, cx| {
            workspace.open_document_path(path.clone(), window, cx);
            workspace.close_tab(workspace.active, window, cx);
        })
    });
    std::fs::remove_file(&path).unwrap();
    cx.update(|window, cx| {
        fixture.workspace.update(cx, |workspace, cx| {
            assert!(!workspace.reopen_closed_tab(window, cx));
            assert_eq!(workspace.closed_tabs.len(), 1);
            assert_eq!(workspace.tabs.len(), 1);
        })
    });
    std::fs::write(&path, "restored file").unwrap();
    cx.update(|window, cx| {
        fixture.workspace.update(cx, |workspace, cx| {
            assert!(workspace.reopen_closed_tab(window, cx));
            assert_eq!(workspace.closed_tabs.len(), 0);
            assert_eq!(workspace.tabs.len(), 2);
        })
    });
}

#[gpui::test]
fn command_z_keeps_file_editor_text_undo_and_cancelled_close_out_of_history(
    cx: &mut TestAppContext,
) {
    if crate::platform::Platform::current() != crate::platform::Platform::MacOS {
        return;
    }
    let (fixture, mut cx) = open(cx);
    cx.update(|window, cx| {
        fixture.workspace.update(cx, |workspace, cx| {
            workspace.add_terminal_with(fixture.launch.clone(), None, None, window, cx);
            workspace.close_tab(workspace.active, window, cx);
        })
    });
    let path = fixture.directory.path().join("draft.txt");
    std::fs::write(&path, "Original").unwrap();
    let file = cx.update(|window, cx| {
        fixture.workspace.update(cx, |workspace, cx| {
            workspace.open_document_path(path, window, cx);
            workspace.tabs[workspace.active].file_editor(cx).unwrap()
        })
    });
    cx.run_until_parked();
    press("cmd-/", &mut cx);
    press("cmd-a", &mut cx);
    cx.simulate_input("Changed");
    cx.run_until_parked();
    assert_eq!(file.read_with(&cx, |file, cx| file.draft(cx)), "Changed");
    cx.update(|window, cx| {
        fixture.workspace.update(cx, |workspace, cx| {
            workspace.close_tab(workspace.active, window, cx);
            assert_eq!(workspace.closed_tabs.len(), 1);
        })
    });
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    let cancel = cx.update(|_, cx| {
        crate::gpui_shell::config::ui_language(cx).text(crate::i18n::Message::EditorCancel)
    });
    cx.simulate_prompt_answer(cancel);
    cx.run_until_parked();
    fixture.workspace.read_with(&cx, |workspace, _| {
        assert_eq!(workspace.tabs.len(), 2);
        assert_eq!(workspace.closed_tabs.len(), 1);
    });
    press("cmd-z", &mut cx);
    assert_eq!(file.read_with(&cx, |file, cx| file.draft(cx)), "Original");
    assert_eq!(fixture.workspace.read_with(&cx, |workspace, _| workspace.tabs.len()), 2);
}

#[gpui::test]
fn arbitrary_pending_shell_command_is_not_replayed_by_restore(cx: &mut TestAppContext) {
    let (fixture, mut cx) = open(cx);
    cx.update(|window, cx| {
        fixture.workspace.update(cx, |workspace, cx| {
            workspace.add_terminal_with(fixture.launch.clone(), None, None, window, cx);
            let old = workspace.tabs[1].focused_view().unwrap().clone();
            old.update(cx, |view, cx| {
                view.run_command("echo arbitrary-command-must-not-replay".into(), cx)
            });
            workspace.close_tab(1, window, cx);
            assert!(workspace.reopen_closed_tab(window, cx));
            let restored = workspace.tabs[1].focused_view().unwrap().read(cx);
            assert_eq!(restored.session_agent(), None);
            assert!(!restored.recovery_pending());
            assert_ne!(old.entity_id(), workspace.tabs[1].focused_view().unwrap().entity_id());
        })
    });
}

#[gpui::test]
fn closing_settings_is_the_next_undo_without_reopening_an_older_terminal(cx: &mut TestAppContext) {
    let (fixture, mut cx) = open(cx);
    cx.update(|window, cx| {
        fixture.workspace.update(cx, |workspace, cx| {
            workspace.add_terminal_with(fixture.launch.clone(), None, None, window, cx);
            workspace.close_tab(1, window, cx);
            workspace.open_settings(window, cx);
            workspace.close_settings(window, cx);
            assert_eq!(
                workspace.closed_tabs.len(),
                2,
                "Settings closes must participate in LIFO undo"
            );
            assert!(workspace.reopen_closed_tab(window, cx));
            assert!(workspace.settings_tab_open && workspace.settings_open);
            assert_eq!(
                workspace.tabs.len(),
                1,
                "Settings stays a singleton outside terminal tab order"
            );
            assert_eq!(workspace.closed_tabs.len(), 1);
        })
    });
}

#[gpui::test]
fn undo_uses_existing_ai_resume_preference_without_replaying_arbitrary_tasks(
    cx: &mut TestAppContext,
) {
    let (fixture, mut cx) = open(cx);
    let _settings_bytes = crate::gpui_shell::settings_fixture::SettingsBytesGuard::capture();
    for resume_ai in [false, true] {
        nebula_settings::persist_keys(&[("resume_ai", (if resume_ai { "1" } else { "0" }).into())])
            .unwrap();
        let agent = crate::session::AgentSession {
            source: "codex".into(),
            session_id: Some("saved-undo-fixture".into()),
            session_file: None,
        };
        cx.update(|window, cx| {
            fixture.workspace.update(cx, |workspace, cx| {
                workspace.add_terminal_with(fixture.launch.clone(), None, None, window, cx);
                let index = workspace.active;
                let original = workspace.tabs[index].focused_view().unwrap().clone();
                original.update(cx, |view, cx| view.restore_agent(agent.clone(), cx));
                assert!(original.read(cx).session_agent().is_some());
                workspace.close_tab(index, window, cx);
                assert!(workspace.reopen_closed_tab(window, cx));
                let reopened = workspace.tabs[index].focused_view().unwrap().read(cx);
                assert_eq!(reopened.recovery_pending(), resume_ai);
                assert_eq!(reopened.session_agent(), resume_ai.then(|| agent.clone()));
            })
        });
    }
}

#[gpui::test]
fn unsupported_remote_and_merge_closes_cannot_undo_an_older_tab_on_the_same_keypress(
    cx: &mut TestAppContext,
) {
    let (fixture, mut cx) = open(cx);
    for remote in [true, false] {
        cx.update(|window, cx| {
            fixture.workspace.update(cx, |workspace, cx| {
                workspace.add_terminal_with(fixture.launch.clone(), None, None, window, cx);
                workspace.close_tab(workspace.active, window, cx);
                if remote {
                    workspace.open_remote_document(
                        "pebrel-test@127.0.0.1:1".into(),
                        "/unsupported.txt".into(),
                        window,
                        cx,
                    );
                } else {
                    let view = cx.new(|cx| {
                        crate::gpui_shell::code_tab::CodeTabView::new_git_merge(
                            crate::display::side_panel::GitLocation::Local {
                                root: fixture.directory.path().to_owned(),
                            },
                            "conflict.txt".into(),
                            window,
                            cx,
                        )
                    });
                    let subscription = cx.subscribe(&view, |_, _, _, _| {});
                    workspace
                        .insert_new_tab(WorkspaceTab::Code { view, _subscription: subscription });
                }
                workspace.close_tab(workspace.active, window, cx);
                assert_eq!(
                    workspace.closed_tabs.len(),
                    2,
                    "unsupported closes still own their place in history"
                );
                assert!(
                    workspace.reopen_closed_tab(window, cx),
                    "the unsupported item handles this keypress"
                );
                assert_eq!(workspace.tabs.len(), 1, "never restore an older tab on this keypress");
                assert_eq!(workspace.closed_tabs.len(), 1);
                assert!(workspace.reopen_closed_tab(window, cx));
                assert_eq!(workspace.tabs.len(), 2);
                workspace.closed_tabs.clear();
                workspace.close_tab(workspace.active, window, cx);
                workspace.closed_tabs.clear();
            })
        });
    }
}
