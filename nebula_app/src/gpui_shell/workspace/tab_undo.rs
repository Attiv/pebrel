//! Explicit close ownership and bounded, descriptor-only tab undo.
//!
//! Closed PTYs still shut down immediately. History retains only restart
//! inputs, never view entities, terminal grids, drafts, or arbitrary commands.

use super::*;

const CLOSED_TAB_LIMIT: usize = 32;
pub(super) const KEY_CONTEXT: &str = "NebulaWorkspace";
pub(super) const REOPEN_KEY_CONTEXT: &str =
    "(NebulaWorkspace || NebulaTerminal) && !Input && !FileEditor";

#[derive(Clone)]
enum ClosedContent {
    Unsupported,
    Settings,
    Terminal(crate::session::TabSession),
    LocalFile(std::path::PathBuf),
}

#[derive(Clone)]
pub(super) struct ClosedTab {
    at: usize,
    meta: TabMeta,
    content: ClosedContent,
}

impl NebulaWorkspace {
    fn closed_tab_descriptor(&self, ix: usize, cx: &App) -> Option<ClosedTab> {
        let content = match self.tabs.get(ix)? {
            WorkspaceTab::Terminal { .. } => {
                ClosedContent::Terminal(self.snapshot_terminal_for_restart(ix, true, cx)?)
            },
            WorkspaceTab::Image { view } => ClosedContent::LocalFile(view.read(cx).path.clone()),
            WorkspaceTab::Document { view, .. } => {
                let view = view.read(cx);
                if !view.is_local_path(&view.path) {
                    ClosedContent::Unsupported
                } else {
                    ClosedContent::LocalFile(view.path.clone())
                }
            },
            WorkspaceTab::Code { view, .. } => {
                let view = view.read(cx);
                if !view.is_regular_path(&view.path) {
                    ClosedContent::Unsupported
                } else {
                    ClosedContent::LocalFile(view.path.clone())
                }
            },
            WorkspaceTab::Settings { .. } => ClosedContent::Settings,
        };
        Some(ClosedTab { at: ix, meta: self.meta(ix), content })
    }

    fn remember_closed_tab(&mut self, closed: ClosedTab) {
        if self.closed_tabs.len() == CLOSED_TAB_LIMIT {
            self.closed_tabs.remove(0);
        }
        self.closed_tabs.push(closed);
    }

    pub(super) fn remember_settings_closed(&mut self) {
        self.remember_closed_tab(ClosedTab {
            at: self.tabs.len(),
            meta: TabMeta::default(),
            content: ClosedContent::Settings,
        });
    }

    pub(super) fn reopen_closed_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mut closed) = self.closed_tabs.last().cloned() else { return false };
        let previous_count = self.tabs.len();
        let at = closed.at.min(previous_count);
        let restored = match &closed.content {
            ClosedContent::Unsupported => {
                // This close owns one undo operation even though its content
                // cannot safely be restored. Never undo an older tab instead.
                self.closed_tabs.pop();
                return true;
            },
            ClosedContent::Settings => {
                self.clear_reader_focus(cx);
                self.open_settings(window, cx);
                self.closed_tabs.pop();
                return true;
            },
            ClosedContent::Terminal(tab) => {
                let resume_ai = nebula_settings::RuntimeSettings::load().resume_ai;
                self.restore_tab_at(tab, resume_ai, at, window, cx).then_some(at)
            },
            ClosedContent::LocalFile(path) => {
                // Never consume a failed/missing file restore, and never fall
                // back to a system handler that might execute an unknown file.
                if !path.is_file() || !crate::gpui_shell::doc_tabs::openable_in_app(path) {
                    None
                } else {
                    self.open_document_path(path.clone(), window, cx);
                    if self.tabs.len() > previous_count {
                        self.move_tab(self.active, at, window, cx);
                        Some(at)
                    } else {
                        // Existing file-opening policy reuses the same path.
                        // Preserve its current metadata and unsaved draft.
                        Some(self.active)
                    }
                }
            },
        };
        let Some(index) = restored else {
            let language = crate::gpui_shell::config::ui_language(cx);
            crate::gpui_shell::toast::banner(
                window,
                cx,
                crate::display::ToastKind::Warning,
                language.text(crate::i18n::Message::WorkspaceTabRestoreFailed),
            );
            return false;
        };
        if self.tabs.len() > previous_count {
            closed.meta.runtime_id = self.meta(index).runtime_id;
            closed.meta.has_bell = false;
            self.tab_meta[index] = closed.meta;
        }
        self.closed_tabs.pop();
        self.clear_reader_focus(cx);
        self.leave_settings(window, cx);
        self.active = index;
        self.reveal_active_tab();
        self.focus_active(window, cx);
        self.sync_side_panel_to_active(true, cx);
        cx.notify();
        true
    }

    /// 关一个 pane（pane 退出 / ctrl+shift+w）。树裁定结局：最后一个叶子
    /// 关整个 tab，否则兄弟收编、焦点交给幸存子树首叶。
    pub(super) fn close_pane(
        &mut self,
        tab_ix: usize,
        pane_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(edit) = self.pane_rename.as_ref()
            && self.tab_of_pane(edit.pane_id) == Some(tab_ix)
            && let Some(WorkspaceTab::Terminal { panes, .. }) = self.tabs.get(tab_ix)
            && panes.iter().any(|pane| pane.id == pane_id)
        {
            if edit.pane_id == pane_id {
                self.pane_rename = None;
            } else if panes.len() == 2 {
                // The survivor's header disappears when the split collapses.
                self.commit_pane_rename(false, window, cx);
            }
        }
        let outcome = match self.tabs.get_mut(tab_ix) {
            Some(WorkspaceTab::Terminal { tree, .. }) => tree.remove_leaf(pane_id),
            _ => return,
        };
        if !matches!(outcome, RemoveOutcome::NotFound) {
            self.runtime_hub.record_pane_closed(self.runtime_window_id, pane_id);
        }
        match outcome {
            RemoveOutcome::NotFound => {},
            RemoveOutcome::WasRoot => self.close_tab(tab_ix, window, cx),
            RemoveOutcome::Collapsed(next_focus) => {
                if let Some(WorkspaceTab::Terminal { panes, focused, zoomed, broadcast, .. }) =
                    self.tabs.get_mut(tab_ix)
                {
                    if let Some(pos) = panes.iter().position(|pane| pane.id == pane_id) {
                        let pane = panes.remove(pos);
                        pane.view.read(cx).shutdown();
                    }
                    if *focused == pane_id {
                        *focused = next_focus;
                    }
                    *zoomed = false;
                    // 收敛到单 pane：广播没有语义了，留着开关状态只会骗人
                    // ——标题条此时也不再绘制，用户根本没有入口关掉它。
                    if panes.len() < 2 {
                        *broadcast = false;
                    }
                }
                self.remote_browser.forget(pane_id);
                self.pane_bounds.borrow_mut().remove(&pane_id);
                self.mark_structural_resize(tab_ix, cx);
                if tab_ix == self.active {
                    if self.pane_rename.is_none() {
                        self.focus_active(window, cx);
                    }
                    self.sync_side_panel_to_active(true, cx);
                }
                cx.notify();
            },
        }
    }

    /// 与旧壳 `busy_process_in` 同一判据，只把查询落到 GPUI 的 Pane 实体。
    pub(super) fn busy_process_in_tab(
        &self,
        tab_ix: usize,
        pane_id: Option<u64>,
        cx: &App,
    ) -> Option<String> {
        let WorkspaceTab::Terminal { panes, .. } = self.tabs.get(tab_ix)? else { return None };
        panes
            .iter()
            .filter(|pane| pane_id.is_none_or(|id| pane.id == id))
            .find_map(|pane| pane.view.read(cx).busy_process())
    }

    /// 系统标题栏关闭的是整个窗口，必须把所有 Tab/Pane 都纳入同一份旧壳
    /// `busy_child(shell_pid)` 判据；只检查当前 Tab 会漏掉后台仍在编译的任务。
    pub(super) fn busy_process_in_window(&self, cx: &App) -> Option<String> {
        (0..self.tabs.len()).find_map(|tab_ix| self.busy_process_in_tab(tab_ix, None, cx))
    }

    /// 整 tab 关闭（侧栏 ×）逐 pane 回收会话；最后一个 tab 按驻留设置关窗。
    /// 实体引用清零后 `TerminalView::drop` 再兜底。
    pub(super) fn close_tab(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.guard_file_tab_close(ix, window, cx) {
            return;
        }
        self.finish_close_tab(ix, window, cx);
    }

    pub(super) fn finish_close_tab(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Focus mode is scoped to the current document view. Closing any tab
        // must first restore workspace chrome so an old entity cannot leave the
        // next active tab in an immersive layout.
        self.clear_reader_focus(cx);
        let descriptor = self.closed_tab_descriptor(ix, cx);
        let Some((tab, _meta)) = self.remove_tab_at(ix) else { return };
        if let Some(descriptor) = descriptor {
            self.remember_closed_tab(descriptor);
        }
        if let WorkspaceTab::Terminal { panes, .. } = &tab {
            let mut bounds = self.pane_bounds.borrow_mut();
            for pane in panes {
                self.runtime_hub.record_pane_closed(self.runtime_window_id, pane.id);
                pane.view.read(cx).shutdown();
                self.remote_browser.forget(pane.id);
                bounds.remove(&pane.id);
            }
        }

        if self.tabs.is_empty() {
            if self.settings_tab_open {
                self.open_settings(window, cx);
            } else {
                self.close_empty_workspace(window, cx);
                return;
            }
        }
        if ix < self.active {
            self.active -= 1;
        }
        self.active = self.active.min(self.tabs.len().saturating_sub(1));
        if let Err(error) = windowing::save_current_window_session(
            self.runtime_window_id,
            self.snapshot_session(cx),
            session_persistence::SaveReason::TabsClosed,
            cx,
        ) {
            log::warn!("Could not save closed tabs: {error}");
        }
        self.reveal_active_tab();
        self.focus_active(window, cx);
        self.sync_side_panel_to_active(true, cx);
        cx.notify();
    }

    /// ctrl+shift+w（对齐旧壳 CloseTab 语义）：tab 有分屏时关聚焦 pane，
    /// 单 pane 时关整个 tab；设置 tab 直接关 tab。
    pub(super) fn close_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_open {
            self.close_settings(window, cx);
            return;
        }
        match self.tabs.get(self.active) {
            Some(WorkspaceTab::Terminal { panes, focused, .. }) if panes.len() > 1 => {
                let (tab_ix, pane_id) = (self.active, *focused);
                self.request_close_pane(tab_ix, pane_id, window, cx);
            },
            Some(WorkspaceTab::Terminal { .. }) => {
                self.request_close_tab(self.active, window, cx);
            },
            Some(_) => self.close_tab(self.active, window, cx),
            None => {},
        }
    }
}
