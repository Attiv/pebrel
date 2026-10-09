use super::*;
use crate::gpui_shell::terminal::view::TerminalLaunch;
use gpui::TestAppContext;

struct SessionBytesGuard {
    path: PathBuf,
    bytes: Option<Vec<u8>>,
}

impl SessionBytesGuard {
    fn capture() -> Self {
        let path = crate::display::nebula_data_dir().join("session.json");
        Self { bytes: std::fs::read(&path).ok(), path }
    }
}

impl Drop for SessionBytesGuard {
    fn drop(&mut self) {
        if let Some(bytes) = &self.bytes {
            let _ = std::fs::write(&self.path, bytes);
        } else {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

struct WindowSessionFixture {
    // Restore the snapshot before releasing the shared storage lock.
    _session: SessionBytesGuard,
    cx: TestAppContext,
    _serialization: std::sync::MutexGuard<'static, ()>,
}

impl Drop for WindowSessionFixture {
    fn drop(&mut self) {
        // The GPUI test macro quits after local guards drop. Finish its session
        // writes now, while the original bytes and serialization are still held.
        self.cx.run_until_parked();
        self.cx.quit();
        self.cx.run_until_parked();
    }
}

fn initialize_test(cx: &mut TestAppContext) -> WindowSessionFixture {
    let serialization = crate::gpui_shell::settings_fixture::lock_theme_studio();
    let fixture = WindowSessionFixture {
        _session: SessionBytesGuard::capture(),
        cx: cx.clone(),
        _serialization: serialization,
    };
    cx.update(|cx| {
        gpui_component::init(cx);
        crate::gpui_shell::math_view::register(cx);
        crate::gpui_shell::file_editor::init(cx);
        super::super::init(cx);
        initialize(cx, crate::runtime_api::RuntimeHub::new());
    });
    fixture
}

fn open_test_window(cx: &mut App, count: usize) -> (u64, Entity<NebulaWorkspace>) {
    let (id, workspace) =
        open_workspace_window(cx, WorkspaceStartup::Empty, None, None, false, WindowRole::Regular)
            .unwrap();
    let entry = entry_by_id(id, cx).unwrap();
    entry
        .handle
        .update(cx, |_, window, cx| {
            workspace.update(cx, |workspace, cx| {
                for index in 0..count {
                    let pane = workspace.new_pane(
                        (80, 24),
                        TerminalLaunch::Local {
                            cwd: Some(PathBuf::from(format!("test-project-{index}"))),
                            shell: Some(nebula_terminal::tty::Shell::new(
                                "pebrel-test-missing-shell-executable".into(),
                                vec![],
                            )),
                            shell_name: None,
                        },
                        None,
                        window,
                        cx,
                    );
                    workspace.insert_tab_at(
                        index,
                        WorkspaceTab::Terminal {
                            tree: SplitTree::leaf(pane.id),
                            focused: pane.id,
                            panes: vec![pane],
                            zoomed: false,
                            broadcast: false,
                        },
                        TabMeta::default(),
                    );
                }
            });
        })
        .unwrap();
    (id, workspace)
}

#[gpui::test]
fn runtime_close_captures_actual_bounds_before_the_bounds_observer_runs(cx: &mut TestAppContext) {
    let _fixture = initialize_test(cx);
    cx.update(|cx| {
        let (id, workspace) = open_test_window(cx, 1);
        let entry = entry_by_id(id, cx).unwrap();
        let expected = entry
            .handle
            .update(cx, |_, window, cx| {
                window.resize(size(px(1190.0), px(800.0)));
                let actual = window.window_bounds().get_bounds();
                workspace.update(cx, |workspace, cx| {
                    workspace
                        .execute_runtime_command(
                            &RuntimeCommand::CloseWindow { window_id: Some(id) },
                            window,
                            cx,
                        )
                        .unwrap();
                });
                actual
            })
            .unwrap();
        let saved = cx.global::<WindowRegistry>().session_persistence.update_windows().unwrap();
        let state = saved[0].window.expect("closing captures native geometry");
        assert_eq!((state.width, state.height), (1190, 800));
        assert_eq!(state.x, Some(f32::from(expected.origin.x).round() as i32));
        assert_eq!(state.y, Some(f32::from(expected.origin.y).round() as i32));
    });
}

#[gpui::test]
fn closing_empty_final_window_preserves_geometry_without_resurrecting_tabs(
    cx: &mut TestAppContext,
) {
    let _fixture = initialize_test(cx);
    cx.update(|cx| {
        let (id, workspace) = open_test_window(cx, 0);
        let entry = entry_by_id(id, cx).unwrap();
        entry
            .handle
            .update(cx, |_, window, cx| {
                window.resize(size(px(1190.0), px(800.0)));
                workspace.update(cx, |workspace, cx| {
                    workspace
                        .execute_runtime_command(
                            &RuntimeCommand::CloseWindow { window_id: Some(id) },
                            window,
                            cx,
                        )
                        .unwrap();
                });
            })
            .unwrap();
        let saved = cx.global::<WindowRegistry>().session_persistence.update_windows().unwrap();
        assert!(saved[0].tabs.is_empty());
        let state = saved[0].window.expect("empty final window retains its geometry");
        assert_eq!((state.width, state.height), (1190, 800));
    });
}

#[gpui::test]
fn quitting_settings_only_window_captures_native_geometry(cx: &mut TestAppContext) {
    let _fixture = initialize_test(cx);
    cx.update(|cx| {
        let (id, workspace) = open_test_window(cx, 0);
        let entry = entry_by_id(id, cx).unwrap();
        entry
            .handle
            .update(cx, |_, window, cx| {
                workspace.update(cx, |workspace, cx| workspace.open_settings(window, cx));
                window.resize(size(px(1190.0), px(800.0)));
            })
            .unwrap();
        save_combined_session(cx, true).unwrap();
        let saved = cx
            .global::<WindowRegistry>()
            .session_persistence
            .update_windows()
            .expect("quitting Settings persists its geometry");
        let saved = &saved[0];
        assert!(saved.tabs.is_empty());
        assert!(saved.clean_exit);
        let geometry = saved.window.unwrap();
        assert_eq!((geometry.width, geometry.height), (1190, 800));
    });
}

#[gpui::test]
fn startup_restores_geometry_when_terminal_session_restore_is_disabled(cx: &mut TestAppContext) {
    let _fixture = initialize_test(cx);
    let _settings = crate::gpui_shell::settings_fixture::SettingsBytesGuard::capture();
    let path = nebula_settings::settings_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "restore_session=0\n").unwrap();
    let mut previous = crate::session::Session::new(
        0,
        vec![crate::session::TabSession::single(
            "/saved-workspace".into(),
            Some("Saved tab".into()),
            None,
        )],
    );
    previous.window = Some(crate::session::WindowState {
        x: Some(200),
        y: Some(80),
        width: 1190,
        height: 800,
        maximized: false,
    });
    crate::session::try_save(&previous).unwrap();
    cx.update(|cx| {
        assert!(!nebula_settings::RuntimeSettings::load().restore_session);
        let (_, workspace) = open_workspace_window(
            cx,
            WorkspaceStartup::RestoreOrDefault,
            None,
            None,
            false,
            WindowRole::Regular,
        )
        .unwrap();
        let saved = workspace.read(cx).snapshot_session(cx);
        assert_eq!(saved.window, previous.window);
        assert_eq!(saved.tabs.len(), 1, "startup still creates one fresh terminal");
        assert_ne!(saved.tabs[0].custom_name, previous.tabs[0].custom_name);
        workspace.update(cx, |workspace, cx| workspace.shutdown_terminal_panes(cx));
    });
}

#[gpui::test]
fn moving_last_tab_closes_only_source_after_transfer(cx: &mut TestAppContext) {
    let _fixture = initialize_test(cx);
    let (source_id, source, other_id, other, moved_view) = cx.update(|cx| {
        let (source_id, source) = open_test_window(cx, 1);
        let (other_id, other) = open_test_window(cx, 1);
        let moved_view = source.read(cx).tabs[0].focused_view().unwrap().clone();
        source.update(cx, |source, cx| source.schedule_move_tab_to_new_window(0, cx));
        (source_id, source, other_id, other, moved_view)
    });
    cx.run_until_parked();
    cx.update(|cx| {
        assert!(entry_by_id(source_id, cx).is_none());
        assert!(source.read(cx).tabs.is_empty());
        assert_eq!(other.read(cx).tabs.len(), 1);
        assert!(entry_by_id(other_id, cx).is_some());
        let entries = &cx.global::<WindowRegistry>().entries;
        assert_eq!(entries.len(), 2);
        let target = entries
            .iter()
            .find(|entry| entry.runtime_window_id != other_id)
            .unwrap()
            .workspace
            .upgrade()
            .unwrap();
        assert_eq!(target.read(cx).tabs[0].focused_view().unwrap(), &moved_view);
        assert_eq!(combined_session(None, cx).unwrap().tabs.len(), 2);
    });
}

#[gpui::test]
fn moving_one_of_two_tabs_preserves_source_and_identity(cx: &mut TestAppContext) {
    let _fixture = initialize_test(cx);
    let (source_id, source, moved_view) = cx.update(|cx| {
        let (id, source) = open_test_window(cx, 2);
        let moved_view = source.read(cx).tabs[1].focused_view().unwrap().clone();
        source.update(cx, |source, cx| source.schedule_move_tab_to_new_window(1, cx));
        (id, source, moved_view)
    });
    cx.run_until_parked();
    cx.update(|cx| {
        assert!(entry_by_id(source_id, cx).is_some());
        assert_eq!(source.read(cx).tabs.len(), 1);
        let target = cx
            .global::<WindowRegistry>()
            .entries
            .iter()
            .find(|entry| entry.runtime_window_id != source_id)
            .unwrap()
            .workspace
            .upgrade()
            .unwrap();
        assert_eq!(target.read(cx).tabs[0].focused_view().unwrap(), &moved_view);
        assert_eq!(combined_session(None, cx).unwrap().tabs.len(), 2);
    });
}
