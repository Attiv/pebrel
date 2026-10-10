//! Adapter ownership for shells without OSC command boundaries. The terminal
//! core alone owns mode snapshots; this adapter retains evidence and its lease.
use gpui::Context;
use nebula_terminal::term::InputModeLease;

use super::TerminalView;

pub(super) struct PendingRecovery {
    lease: InputModeLease,
    prompt: String,
    owner_exited: bool,
}

impl TerminalView {
    pub(super) fn prepare_terminal_input(&mut self, cx: &mut Context<Self>) {
        if self.accepts_input() && self.recover_input_protocol_at_prompt() {
            self.finish_foreground_command(None, cx);
        }
    }
    pub(super) fn capture_input_protocol(&mut self) {
        let Some(prompt) = self.suggest.pending_command_prompt.clone() else { return };
        self.input_protocol = self.session.as_ref().and_then(|session| {
            let lease = session.term.lock().nebula_begin_shell_input()?;
            Some(PendingRecovery { lease, prompt, owner_exited: false })
        });
    }

    pub(super) fn input_protocol_owner_exited(&mut self) {
        if let Some(pending) = &mut self.input_protocol {
            pending.owner_exited = true;
        }
    }

    pub(super) fn input_protocol_probe(&self) -> Option<InputModeLease> {
        let pending = self.input_protocol.as_ref()?;
        let term = self.session.as_ref()?.term.lock();
        term.nebula_owns_input_modes(pending.lease)
            .then(|| term.nebula_input_mode_lease())
            .flatten()
    }

    pub(super) fn input_protocol_probe_current(&self, probe: Option<InputModeLease>) -> bool {
        probe.is_none_or(|probe| {
            self.session
                .as_ref()
                .is_some_and(|session| session.term.lock().nebula_input_mode_lease() == Some(probe))
        })
    }

    /// Prompt observation and recovery share the parser's Term lock. An accepted
    /// SessionEnd is necessary to accept a prompt in an abandoned ALT buffer;
    /// neither silence nor TurnDone is such evidence. Local unhooked shells
    /// still require the existing process probe before recovery.
    pub(super) fn recover_input_protocol_at_prompt(&mut self) -> bool {
        if self.pending_runtime_submit.is_some()
            || self.pending_shell_command.is_some()
            || self.recovery.preparing()
        {
            return false;
        }
        let Some(pending) = &self.input_protocol else { return false };
        let Some(session) = &self.session else { return false };
        let mut term = session.term.lock();
        if !term.nebula_owns_input_modes(pending.lease) {
            self.input_protocol = None;
            return false;
        }
        if term.nebula_input_mode_lease() == Some(pending.lease) {
            return false; // Do not mistake the pre-echo prompt for a return.
        }
        if (pending.owner_exited || !self.suggest.suggest_env.is_this_machine())
            && crate::display::nebula_shell_prompt_restored_from_raw_grid(
                &term,
                &pending.prompt,
                &self.suggest.suggest_env,
                pending.owner_exited,
            )
        {
            let lease = term.nebula_input_mode_lease().unwrap();
            term.nebula_restore_input_modes(lease);
            self.input_protocol = None;
            return true;
        }
        false
    }

    pub(super) fn recover_input_protocol_after_process_exit(
        &mut self,
        probe: Option<InputModeLease>,
    ) -> bool {
        let Some(pending) = &self.input_protocol else { return true };
        let Some(session) = &self.session else { return false };
        let mut term = session.term.lock();
        // OSC 133;A/D can restore the protocol synchronously before its UI
        // event is consumed. A vanished core lease needs acknowledgement,
        // not another restoration. A newer active owner must still be rejected.
        if term.nebula_input_mode_lease().is_none() {
            self.input_protocol = None;
            return true;
        }
        let Some(probe) = probe else { return false };
        if term.nebula_owns_input_modes(pending.lease) && term.nebula_restore_input_modes(probe) {
            self.input_protocol = None;
            return true;
        }
        false
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::super::session;
    use super::super::startup_tests::open;
    use super::*;
    use gpui::TestAppContext;
    use nebula_terminal::event_loop::StreamProcessor;

    #[gpui::test]
    fn process_exit_accepts_modes_restored_by_parser_but_not_a_new_owner(cx: &mut TestAppContext) {
        let (view, window, _) = open(cx);
        view.update(window, |view, _cx| {
            let (session, _input, _events, proxy) = session::test_session_with_events();
            view.session = Some(session);
            view.suggest.pending_command_prompt = Some("C:\\work>".into());
            view.capture_input_protocol();
            assert!(view.input_protocol.is_some());
            assert!(!view.recover_input_protocol_after_process_exit(None));
            StreamProcessor::default().feed(
                &mut view.session.as_ref().unwrap().term.lock(),
                &proxy,
                b"\x1b]133;A\x07C:\\work>",
            );
            assert!(view.session.as_ref().unwrap().term.lock().nebula_input_mode_lease().is_none());
            assert!(view.recover_input_protocol_after_process_exit(None));
            assert!(view.input_protocol.is_none());

            view.capture_input_protocol();
            let newer = view.session.as_ref().unwrap().term.lock().nebula_begin_shell_input();
            assert!(newer.is_some());
            assert!(!view.recover_input_protocol_after_process_exit(None));
            assert!(!view.recover_input_protocol_after_process_exit(newer));
            assert_eq!(view.session.as_ref().unwrap().term.lock().nebula_input_mode_lease(), newer);
        });
    }
}
