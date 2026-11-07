//! In-place AI query ownership; the native Shell draft is untouched until explicit adoption.

use super::*;
use crate::ai_commands::{self, Failure, SearchResult};
use crate::gpui_shell::copy_feedback::CopyFeedback;
use crate::i18n::Message;
use gpui::{Entity, Subscription};
use gpui_component::{
    Disableable as _, IconName,
    button::{Button, ButtonVariants as _},
    input::{Input, InputEvent, InputState},
};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Identity {
    generation: u64,
    epoch: u64,
    owner: String,
    cwd: String,
    config: u64,
}

pub(super) struct Search {
    input: Entity<InputState>,
    focus: FocusHandle,
    original: Option<editor::Snapshot>,
    identity: Identity,
    shell: &'static str,
    revision: u64,
    seed: Option<String>,
    result: Option<SearchResult>,
    status: Option<Message>,
    verifying: bool,
    queued: bool,
    window: gpui::AnyWindowHandle,
    feedback: Entity<CopyFeedback>,
    _events: Subscription,
    _feedback: Subscription,
    _focus: Subscription,
}

pub(super) struct Job {
    pub(super) cancelled: Arc<AtomicBool>,
    pub(super) revision: u64,
    pub(super) prediction: bool,
    pub(super) _task: gpui::Task<()>,
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

impl TerminalView {
    pub(in crate::gpui_shell::terminal) fn ai_search_active(&self) -> bool {
        self.ai_search.is_some()
    }

    pub(super) fn ai_search_focus(&self, cx: &App) -> Option<FocusHandle> {
        self.ai_search.as_ref().map(|search| search.input.read(cx).focus_handle(cx))
    }

    pub(super) fn ai_search_identity(&self) -> Option<(Identity, &'static str)> {
        if self.exited.is_some()
            || self.session.is_none()
            || self.ssh_destination.is_some()
            || self.marked_text.is_some()
            || self.answer_reader.is_some()
            || self.reading.panel.is_some()
            || !self.output_visible
            || !self.cursor_window_active
            || self.runtime_agent().is_some()
        {
            return None;
        }
        let owner = self.completion_editor.owner()?;
        if !self.suggest.completion_context.owns_current(owner) {
            return None;
        }
        let shell = crate::completion::command_ai_shell(
            &self.suggest.suggest_env,
            self.exec_context.as_ref(),
            owner,
        )?;
        let term = self.session.as_ref()?.term.lock();
        if !term.nebula_prompt_active()
            || term.mode().intersects(TermMode::ALT_SCREEN | TermMode::VI)
        {
            return None;
        }
        Some((
            Identity {
                generation: self.ai_search_generation,
                epoch: self.prompt_input_epoch,
                owner: owner.to_owned(),
                cwd: self.cwd.clone(),
                config: ai_commands::revision(),
            },
            shell,
        ))
    }

    pub(in crate::gpui_shell) fn toggle_ai_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.ai_search.is_some() {
            self.close_ai_search(cx);
            window.focus(&self.focus_handle, cx);
            return true;
        }
        self.invalidate_ai_prediction();
        self.ai_search_generation = self.ai_search_generation.wrapping_add(1);
        self.sync_native_prompt();
        if self.session.as_ref().is_some_and(|session| {
            let term = session.term.lock();
            !term.nebula_prompt_active()
                || term.mode().intersects(TermMode::ALT_SCREEN | TermMode::VI)
        }) {
            // An application shortcut must not consume a foreground program's original chord.
            return false;
        }
        let language = crate::gpui_shell::config::ui_language(cx);
        let Some((identity, shell)) = self.ai_search_identity() else {
            crate::gpui_shell::toast::toast(
                window,
                cx,
                crate::display::ToastKind::Warning,
                language.text(Message::AiCommandSearchUnavailable),
            );
            return true;
        };
        let configured =
            cx.try_global::<Settings>().is_some_and(|settings| settings.command_ai.is_configured());
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(language.text(Message::AiCommandSearchPlaceholder))
        });
        let events =
            cx.subscribe_in(&input, window, |view, input, event: &InputEvent, window, cx| {
                match event {
                    InputEvent::Change => {
                        if let Some(search) = view.ai_search.as_mut() {
                            search.revision = search.revision.wrapping_add(1);
                            search.result = None;
                            search.queued = false;
                            search.feedback.update(cx, |feedback, cx| feedback.clear(cx));
                            if search.original.is_some()
                                && search.status != Some(Message::AiCommandSearchSetup)
                            {
                                search.status = None;
                            }
                        }
                        if let Some(job) = &view.ai_search_job {
                            job.cancelled.store(true, Ordering::Release);
                        }
                        cx.notify();
                    },
                    InputEvent::PressEnter { .. } => {
                        use gpui::EntityInputHandler as _;
                        let composing = input
                            .update(cx, |input, cx| input.marked_text_range(window, cx).is_some());
                        if !composing {
                            view.generate_ai_search(cx);
                        }
                    },
                    InputEvent::Focus | InputEvent::Blur => cx.notify(),
                }
            });
        let feedback = cx.new(|_| CopyFeedback::new());
        let feedback_events = cx.observe(&feedback, |_, _, cx| cx.notify());
        let focus = cx.focus_handle();
        let focus_events = cx.on_focus_out(&focus, window, |view, _, _, cx| {
            view.close_ai_search(cx);
        });
        self.suggestion_task = None;
        self.suggest.clear_completion_hints();
        self.ai_search = Some(Search {
            input: input.clone(),
            focus,
            original: None,
            identity,
            shell,
            revision: 0,
            seed: None,
            result: None,
            status: Some(if configured {
                Message::AiCommandSearchCapture
            } else {
                Message::AiCommandSearchSetup
            }),
            verifying: false,
            queued: false,
            window: window.window_handle(),
            feedback,
            _events: events,
            _feedback: feedback_events,
            _focus: focus_events,
        });
        input.update(cx, |input, cx| input.focus(window, cx));
        if !self.observe_completion_editor(cx) {
            self.ai_search_observation_failed(cx);
        }
        cx.notify();
        true
    }

    pub(super) fn close_ai_search(&mut self, cx: &mut Context<Self>) {
        if self.ai_search.take().is_some() {
            self.ai_search_generation = self.ai_search_generation.wrapping_add(1);
            if let Some(job) = &self.ai_search_job {
                job.cancelled.store(true, Ordering::Release);
            }
            self.completion_editor.invalidate();
            self.editor_query_task = None;
            cx.notify();
        }
    }

    pub(super) fn validate_ai_search(&mut self, cx: &mut Context<Self>) {
        if let Some(search) = &self.ai_search
            && self.ai_search_identity().as_ref().map(|(identity, _)| identity)
                != Some(&search.identity)
        {
            self.close_ai_search(cx);
        }
    }

    pub(super) fn ai_search_observation_failed(&mut self, cx: &mut Context<Self>) {
        if let Some(search) = self.ai_search.as_mut() {
            search.verifying = false;
            search.status = Some(Message::AiCommandSearchUnavailable);
            cx.notify();
        }
    }

    pub(super) fn handle_ai_editor_snapshot(
        &mut self,
        snapshot: editor::Snapshot,
        cx: &mut Context<Self>,
    ) {
        self.validate_ai_search(cx);
        let Some(search) = self.ai_search.as_mut() else { return };
        if search.verifying {
            search.verifying = false;
            if search.original.as_ref() != Some(&snapshot) {
                search.result = None;
                search.status = Some(Message::AiCommandSearchStale);
                cx.notify();
                return;
            }
            let Some(result) = search.result.clone() else { return };
            if !self.completion_editor.permits_insert(&result.command) {
                search.status = Some(Message::AiCommandSearchInvalid);
                cx.notify();
                return;
            }
            let window = search.window;
            let item = crate::display::NebulaCompletionItem {
                label: result.command.clone(),
                insert: result.command,
                replace_chars: snapshot.line[..snapshot.cursor].chars().count(),
                replace_after_chars: snapshot.line[snapshot.cursor..].chars().count(),
                kind: crate::display::NebulaCompletionKind::Command,
            };
            let saved = self.ai_search.take();
            if !self.accept_completion_item(item, cx) {
                self.ai_search = saved;
                if let Some(search) = self.ai_search.as_mut() {
                    search.status = Some(Message::AiCommandSearchStale);
                }
                cx.notify();
                return;
            }
            let focus = self.focus_handle.clone();
            cx.defer(move |cx| {
                let _ = window.update(cx, |_, window, cx| window.focus(&focus, cx));
            });
        } else if search.original.is_none() {
            search.seed = Some(snapshot.line.clone());
            search.original = Some(snapshot);
            if search.status == Some(Message::AiCommandSearchCapture) {
                search.status = None;
            }
        }
        cx.notify();
    }

    pub(super) fn generate_ai_search(&mut self, cx: &mut Context<Self>) {
        self.validate_ai_search(cx);
        let Some(search) = self.ai_search.as_mut() else { return };
        if search.original.is_none() || search.seed.is_some() || search.verifying {
            return;
        }
        if let Some(job) = &self.ai_search_job {
            if job.prediction
                || job.revision != search.revision
                || job.cancelled.load(Ordering::Acquire)
            {
                job.cancelled.store(true, Ordering::Release);
                search.queued = true;
                search.status = Some(Message::AiCommandSearchLoading);
                cx.notify();
            }
            return;
        }
        let input = search.input.read(cx).value().to_string();
        if input.trim().is_empty() {
            search.status = Some(Message::AiCommandSearchNoResult);
            cx.notify();
            return;
        }
        let revision = search.revision;
        let identity = search.identity.clone();
        let shell = search.shell;
        search.status = Some(Message::AiCommandSearchLoading);
        search.result = None;
        search.feedback.update(cx, |feedback, cx| feedback.clear(cx));
        search.queued = false;
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let work = cx.background_spawn(async move {
            ai_commands::prepare()?.search(&input, std::env::consts::OS, shell, &worker_cancelled)
        });
        let task = cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |view, cx| {
                let cancelled = view
                    .ai_search_job
                    .as_ref()
                    .is_none_or(|job| job.cancelled.load(Ordering::Acquire));
                view.ai_search_job = None;
                view.validate_ai_search(cx);
                if let Some(search) = view.ai_search.as_mut() {
                    if !cancelled && search.identity == identity && search.revision == revision {
                        match result {
                            Ok(Some(result)) => {
                                search.result = Some(result);
                                search.status = None;
                            },
                            Ok(None) => search.status = Some(Message::AiCommandSearchNoResult),
                            Err(Failure::Cancelled) => {},
                            Err(Failure::Unavailable) => {
                                search.status = Some(Message::AiCommandSearchSetup)
                            },
                            Err(Failure::Busy) => {
                                search.status = Some(Message::AiCommandSearchBusy)
                            },
                            Err(Failure::Network) => {
                                search.status = Some(Message::AiCommandSearchNetwork)
                            },
                            Err(Failure::InvalidResponse) => {
                                search.status = Some(Message::AiCommandSearchInvalid)
                            },
                        }
                    }
                }
                view.drain_ai_command_queue(cx);
                cx.notify();
            });
        });
        self.ai_search_job = Some(Job { cancelled, revision, prediction: false, _task: task });
        cx.notify();
    }

    pub(super) fn ai_search_queued(&self) -> bool {
        self.ai_search.as_ref().is_some_and(|search| search.queued)
    }

    pub(super) fn ai_search_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ai_search_composing(window, cx) {
            cx.propagate();
            return;
        }
        self.adopt_ai_search(cx);
    }

    fn adopt_ai_search(&mut self, cx: &mut Context<Self>) {
        self.validate_ai_search(cx);
        let Some(search) = self.ai_search.as_mut() else { return };
        if search.result.is_none() || search.verifying {
            return;
        }
        search.verifying = true;
        if !self.observe_completion_editor(cx) {
            self.ai_search_observation_failed(cx);
        }
    }

    fn ai_search_composing(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        use gpui::EntityInputHandler as _;
        self.ai_search.as_ref().is_some_and(|search| {
            search.input.update(cx, |input, cx| input.marked_text_range(window, cx).is_some())
        })
    }

    fn ai_search_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.ai_search_composing(window, cx) {
            return;
        }
        let mods = &event.keystroke.modifiers;
        if mods.control || mods.alt || mods.platform || mods.function {
            return;
        }
        let Some(search) = self.ai_search.as_ref() else { return };
        if event.keystroke.key == "escape" {
            self.close_ai_search(cx);
            window.focus(&self.focus_handle, cx);
            cx.stop_propagation();
        } else if event.keystroke.key == "tab" && !event.keystroke.modifiers.shift {
            if search.result.is_some() {
                self.adopt_ai_search(cx);
            }
            cx.stop_propagation();
        }
    }

    pub(super) fn ai_search_chrome(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        self.validate_ai_search(cx);
        let search = self.ai_search.as_mut()?;
        if let Some(seed) = search.seed.take() {
            let early_input = search.input.read(cx).value();
            search.input.update(cx, |input, cx| {
                input.set_value(format!("{seed}{early_input}"), window, cx)
            });
        }
        let language = crate::gpui_shell::config::ui_language(cx);
        let t = |message| language.text(message);
        let row = self
            .session
            .as_ref()
            .and_then(|session| {
                let term = session.term.lock();
                nebula_terminal::term::point_to_viewport_from(
                    term.viewport_origin_for(self.rows),
                    term.grid().cursor.point,
                )
                .map(|point| point.line)
            })
            .unwrap_or(0);
        let pane_height = self.line_height * self.rows as f32;
        let top =
            (self.line_height * row as f32 + px(8.0)).min((pane_height - px(160.0)).max(px(8.0)));
        let height = (pane_height + px(8.0) - top).max(px(0.0));
        let copied = search.feedback.read(cx).is_copied();
        let status = search.status;
        let result = search.result.clone();
        let mut panel = div()
            .id("terminal-ai-search")
            .debug_selector(|| "terminal-ai-search".to_owned())
            .track_focus(&search.focus)
            .absolute()
            .top(top)
            .left(px(12.0))
            .right(px(12.0))
            .occlude()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().popover)
            .text_color(cx.theme().foreground)
            .max_h(height)
            .overflow_y_scroll()
            .p_2()
            .flex()
            .flex_col()
            .gap_2()
            .shadow_sm()
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| cx.stop_propagation()))
            .on_key_down(cx.listener(Self::ai_search_key))
            .child(
                div().text_sm().font_weight(FontWeight::MEDIUM).child(t(Message::AiCommandSearch)),
            )
            .child(Input::new(&search.input).appearance(false).focus_bordered(true))
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(cx.theme().muted_foreground)
                    .child(t(Message::AiCommandSearchHint)),
            );
        if let Some(status) = status {
            panel = panel.child(div().text_sm().child(t(status)));
        }
        if status == Some(Message::AiCommandSearchSetup) {
            panel = panel.child(
                Button::new("ai-search-setup")
                    .small()
                    .label(t(Message::AiCommandChooseProvider))
                    .on_click(|_, window, cx| {
                        window.dispatch_action(
                            Box::new(crate::gpui_shell::workspace::OpenSettings),
                            cx,
                        )
                    }),
            );
        }
        if let Some(result) = result {
            panel = panel
                .child(
                    div()
                        .text_sm()
                        .whitespace_normal()
                        .font_family(cx.theme().mono_font_family.clone())
                        .child(result.command),
                )
                .child(div().text_sm().whitespace_normal().child(result.explanation))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            Button::new("ai-search-adopt")
                                .small()
                                .label(t(Message::AiCommandSearchAdopt))
                                .disabled(search.verifying)
                                .on_click(cx.listener(|view, _, _, cx| view.adopt_ai_search(cx))),
                        )
                        .child(
                            Button::new("ai-search-copy")
                                .debug_selector(|| "ai-search-copy".to_owned())
                                .small()
                                .ghost()
                                .icon(if copied { IconName::Check } else { IconName::Copy })
                                .label(t(if copied {
                                    Message::AiCommandSearchCopied
                                } else {
                                    Message::AiCommandSearchCopy
                                }))
                                .on_click(cx.listener(|view, _, _, cx| {
                                    if let Some(search) = &view.ai_search
                                        && let Some(result) = &search.result
                                    {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            result.command.clone(),
                                        ));
                                        search
                                            .feedback
                                            .update(cx, |feedback, cx| feedback.mark_copied(cx));
                                    }
                                })),
                        ),
                );
        }
        Some(panel.into_any_element())
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests;

#[cfg(all(test, feature = "gpui-test-support"))]
mod native_tests;
