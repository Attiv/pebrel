//! SSH pane construction, configuration and connection facts at the legacy window boundary.

use super::*;

pub(super) fn apply_terminal_config(pane: &Pane, config: &UiConfig) {
    let mut options = config.term_options();
    if pane.ssh_destination.is_some() {
        options = crate::ssh_session::terminal_config(options);
    }
    pane.terminal.lock().set_options(options);
}

#[cfg(windows)]
impl WindowContext {
    /// 创建由远端 PTY 通道驱动的 Pane，并复用本地终端的解析、渲染和事件协议。
    /// 这样传输层只负责字节流，输入、缩放与终端状态无需维护两套实现。
    pub(super) fn create_ssh_pane(
        size_info: &crate::display::SizeInfo,
        window_id: WindowId,
        config: &UiConfig,
        proxy: &EventLoopProxy<Event>,
        pane_id: PaneId,
        destination: String,
        remote_cwd: Option<String>,
    ) -> Result<Pane, Box<dyn Error>> {
        let window_route = Arc::new(AtomicU64::new(window_id.into()));
        let event_proxy = EventProxy::new_tab(proxy.clone(), window_route.clone(), pane_id);
        let terminal = Arc::new(FairMutex::new(Term::new(
            crate::ssh_session::terminal_config(config.term_options()),
            size_info,
            event_proxy.clone(),
        )));
        let sender = crate::ssh_session::spawn_session_at(
            destination.clone(),
            remote_cwd,
            (*size_info).into(),
            terminal.clone(),
            event_proxy.clone(),
        )?;
        if config.cursor.style().blinking {
            event_proxy.send_event(TerminalEvent::CursorBlinkingChange.into());
        }
        let mut nebula_state = NebulaPaneState::default();
        nebula_state.suggest_env =
            crate::display::SuggestEnv::Ssh { destination: destination.clone() };
        Ok(Pane {
            terminal,
            notifier: Notifier(sender),
            search_state: Default::default(),
            inline_search_state: Default::default(),
            id: pane_id,
            title: String::from("ssh"),
            exec_context: None,
            ssh_destination: Some(destination),
            nebula_state,
            intro_cols: None,
            shell_pid: 0,
            window_route,
        })
    }
}

impl WindowContext {
    /// 把后台 SSH runtime 的阶段上报转给 display，顺带补上这个 pane 的目标
    /// 地址——事件本身只带 pane 身份，地址在 pane 上。
    pub fn ssh_connect_stage(&mut self, pane: PaneId, stage: crate::ssh_session::SshStage) {
        let destination = self
            .pane_index(pane)
            .and_then(|index| self.panes[index].ssh_destination.clone())
            .unwrap_or_default();
        self.display.ssh_connect_stage(pane, destination, stage);
    }

    pub fn ssh_connection_event(&mut self, pane: PaneId, event: crate::ssh_session::SshEvent) {
        // Late results for closed panes must never become another pane's notice.
        let Some(destination) = self.pane(pane).and_then(|pane| pane.ssh_destination.as_deref())
        else {
            return;
        };
        match event {
            crate::ssh_session::SshEvent::Stage(stage) => self.ssh_connect_stage(pane, stage),
            crate::ssh_session::SshEvent::ForwardWarning(warning) => {
                let text = crate::display::ssh_connect::forward_warning_message(
                    &warning,
                    destination,
                    self.display.ui_language(),
                );
                log::warn!("SSH pane={pane} warning: {text}");
                self.display.push_toast(text, crate::display::ToastKind::Warning);
            },
        }
    }
}
