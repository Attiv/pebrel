//! Idle suffix ownership. Search and prediction retain the same actual HTTP job until it finishes.
use super::*;
use crate::display::{CompletionStyle, NebulaCompletionItem, NebulaCompletionKind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

mod coexistence;

const IDLE: Duration = Duration::from_millis(500);

#[derive(Clone)]
struct Request {
    identity: ai_search::Identity,
    snapshot: editor::Snapshot,
    shell: &'static str,
}

#[derive(Default)]
pub(super) struct Prediction {
    timer: Option<gpui::Task<()>>,
    observing: bool,
    verifying: bool,
    local_ready: bool,
    request: Option<Request>,
    candidate: Option<NebulaCompletionItem>,
    attempted: Option<u64>,
    dismissed: Option<u64>,
    queued: bool,
    sent: bool,
    manual_source: Option<nebula_settings::CommandAiSource>,
    accepting_source: Option<nebula_settings::CommandAiSource>,
}

impl TerminalView {
    fn prediction_enabled(&self, cx: &App) -> bool {
        self.ghost_enabled
            && self.cursor_pane_focused
            && self.ai_search.is_none()
            && self.ai_search_identity().is_some()
            && cx
                .try_global::<Settings>()
                .is_some_and(|s| s.command_ai.prediction_enabled && s.command_ai.is_configured())
    }

    pub(super) fn invalidate_ai_prediction(&mut self) {
        if self.ai_prediction.observing || self.ai_prediction.verifying {
            self.completion_editor.invalidate();
            self.editor_query_task = None;
        }
        if self.ai_prediction.candidate.take().is_some() || self.native_prediction_coexists() {
            self.suggest.clear_completion_hints();
            self.suggest.completion_popup_requested = false;
        }
        self.ai_prediction = Prediction {
            attempted: self.ai_prediction.attempted,
            dismissed: self.ai_prediction.dismissed,
            ..Default::default()
        };
        if let Some(job) = &self.ai_search_job
            && job.prediction
        {
            job.cancelled.store(true, Ordering::Release);
        }
    }

    pub(super) fn dismiss_ai_prediction(&mut self) {
        self.invalidate_ai_prediction();
        self.ai_prediction.dismissed = Some(self.prompt_input_epoch);
    }

    pub(super) fn validate_ai_prediction(&mut self, cx: &App) {
        if let Some(request) = &self.ai_prediction.request
            && (!self.prediction_enabled(cx)
                || self.ai_search_identity().as_ref().map(|(id, _)| id) != Some(&request.identity))
        {
            self.invalidate_ai_prediction();
        }
    }

    pub(super) fn schedule_ai_prediction(&mut self, cx: &mut Context<Self>) {
        if !self.prediction_enabled(cx)
            || self.ai_prediction.attempted == Some(self.prompt_input_epoch)
            || self.ai_prediction.dismissed == Some(self.prompt_input_epoch)
        {
            return;
        }
        self.ai_prediction.timer = None;
        let epoch = self.prompt_input_epoch;
        let identity = self.ai_search_identity().map(|(identity, _)| identity);
        self.ai_prediction.timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(IDLE).await;
            let _ = this.update(cx, |view, cx| {
                view.ai_prediction.timer = None;
                if view.prompt_input_epoch != epoch
                    || !view.prediction_enabled(cx)
                    || view.ai_search_identity().map(|(identity, _)| identity) != identity
                {
                    return;
                }
                view.ai_prediction.attempted = Some(epoch);
                view.ai_prediction.observing = true;
                if !view.observe_completion_editor(cx) {
                    view.ai_prediction.observing = false;
                }
            });
        }));
    }

    pub(super) fn prediction_observation_failed(&mut self) {
        self.ai_prediction.observing = false;
        self.ai_prediction.verifying = false;
    }

    pub(super) fn prediction_snapshot(
        &mut self,
        snapshot: editor::Snapshot,
        cx: &mut Context<Self>,
    ) {
        self.validate_ai_prediction(cx);
        if !self.prediction_enabled(cx) {
            self.invalidate_ai_prediction();
            return;
        }
        if self.ai_prediction.verifying {
            if self.finish_native_prediction_adoption(&snapshot, cx) {
                return;
            }
            self.ai_prediction.verifying = false;
            let valid = self.ai_prediction.request.as_ref().is_some_and(|r| r.snapshot == snapshot);
            if valid && let Some(mut item) = self.ai_prediction.candidate.clone() {
                self.invalidate_ai_prediction();
                // Origin is already verified; the generic edit adapter must not re-enter verification.
                item.kind = NebulaCompletionKind::Command;
                self.accept_completion_item(item, cx);
            } else {
                self.invalidate_ai_prediction();
            }
            return;
        }
        if !std::mem::take(&mut self.ai_prediction.observing) {
            return;
        }
        if !crate::completion::command_prediction_eligible(
            &snapshot.line,
            snapshot.cursor,
            true,
            false,
        ) {
            return;
        }
        let Some((identity, shell)) = self.ai_search_identity() else {
            return;
        };
        self.ai_prediction.request = Some(Request { identity, snapshot: snapshot.clone(), shell });
        self.refresh_suggestion_from_snapshot(Some(snapshot.line), self.suggest_anchor, cx);
        self.present_ai_prediction();
        self.try_ai_prediction(cx);
    }

    /// May be called by a cache-hit render, but only records local readiness; never starts work.
    pub(super) fn mark_prediction_local_ready(&mut self, line: &str, cursor: usize) {
        if self
            .ai_prediction
            .request
            .as_ref()
            .is_some_and(|r| r.snapshot.line == line && r.snapshot.cursor == cursor)
        {
            self.ai_prediction.local_ready = true;
        }
    }

    fn local_prediction_competitor(&self) -> bool {
        if self.native_prediction_coexists() {
            return false;
        }
        !self.suggest.suggestion.is_empty()
            && self
                .suggest
                .suggestion_edit
                .as_ref()
                .is_none_or(|item| item.kind != NebulaCompletionKind::Ai)
            || self
                .suggest
                .completion_items
                .iter()
                .any(|item| item.kind != NebulaCompletionKind::Ai)
    }

    pub(super) fn try_ai_prediction(&mut self, cx: &mut Context<Self>) {
        self.validate_ai_prediction(cx);
        let Some(request) = self.ai_prediction.request.clone() else {
            return;
        };
        if self.ai_prediction.sent
            || self.ai_prediction.candidate.is_some()
            || !crate::completion::command_prediction_eligible(
                &request.snapshot.line,
                request.snapshot.cursor,
                self.ai_prediction.local_ready,
                self.local_prediction_competitor(),
            )
        {
            return;
        }
        if self.ai_search_job.is_some() {
            self.ai_prediction.queued = true;
            return;
        }
        self.ai_prediction.queued = false;
        self.ai_prediction.sent = true;
        // A stable revision is attempted once, including abstention/network/busy outcomes.
        self.ai_prediction.local_ready = false;
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let input = request.snapshot.line.clone();
        let shell = request.shell;
        let work = cx.background_spawn(async move {
            crate::ai_commands::prepare()?.complete(
                &input,
                std::env::consts::OS,
                shell,
                &worker_cancelled,
            )
        });
        let task = cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |view, cx| {
                let cancelled = view
                    .ai_search_job
                    .as_ref()
                    .is_none_or(|job| job.cancelled.load(Ordering::Acquire));
                view.ai_search_job = None;
                view.validate_ai_prediction(cx);
                if !cancelled
                    && view.ai_prediction.request.as_ref().is_some_and(|r| {
                        r.identity == request.identity
                            && r.snapshot.line == request.snapshot.line
                            && r.snapshot.cursor == request.snapshot.cursor
                    })
                    && !view.local_prediction_competitor()
                    && let Ok(Some(suffix)) = result
                    && view.completion_editor.permits_insert(&suffix)
                {
                    let item = NebulaCompletionItem {
                        label: format!("{}{suffix}", request.snapshot.line),
                        insert: suffix.clone(),
                        replace_chars: 0,
                        replace_after_chars: 0,
                        kind: NebulaCompletionKind::Ai,
                    };
                    view.ai_prediction.candidate = Some(item.clone());
                    view.present_ai_prediction();
                }
                view.drain_ai_command_queue(cx);
                cx.notify();
            });
        });
        self.ai_search_job = Some(ai_search::Job {
            cancelled,
            revision: self.prompt_input_epoch,
            prediction: true,
            _task: task,
        });
    }

    pub(super) fn present_ai_prediction(&mut self) {
        if self.present_native_prediction_choices() {
            return;
        }
        let Some(item) = self.ai_prediction.candidate.clone() else {
            return;
        };
        if self.local_prediction_competitor() {
            return;
        }
        if self.completion_style != CompletionStyle::Popup {
            self.suggest.suggestion = item.insert.clone();
            self.suggest.suggestion_edit = Some(item.clone());
        }
        if self.completion_style == CompletionStyle::Popup
            || self.suggest.completion_popup_requested
        {
            self.suggest.completion_items = vec![item];
        }
    }

    pub(super) fn drain_ai_command_queue(&mut self, cx: &mut Context<Self>) {
        if self.ai_search_queued() {
            self.generate_ai_search(cx);
        } else if self.ai_search.is_none() && self.ai_prediction.queued {
            self.try_ai_prediction(cx);
        }
    }

    /// Keep existing mode navigation, but native verification and no-Enter acceptance are AI-owned.
    pub(super) fn prediction_key(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        self.validate_ai_prediction(cx);
        if self.ai_prediction.observing && matches!(key, "enter" | "tab" | "right") {
            self.invalidate_ai_prediction();
            return false;
        }
        if key == "escape" {
            let owned = self.ai_prediction.candidate.is_some() || self.ai_prediction.observing;
            self.dismiss_ai_prediction();
            return owned;
        }
        let Some(item) = self.ai_prediction.candidate.clone() else {
            // A Tab issued before receipt must never implicitly adopt a late AI result.
            if matches!(key, "enter" | "tab" | "right") {
                self.invalidate_ai_prediction();
            }
            return false;
        };
        if key == "enter" {
            self.invalidate_ai_prediction();
            return false;
        }
        if self.ai_prediction.verifying {
            return matches!(key, "tab" | "right");
        }
        if key == "tab" && self.completion_style == CompletionStyle::Hybrid {
            if !self.suggest.completion_popup_requested {
                self.suggest.completion_popup_requested = true;
                self.suggest.completion_items = vec![item];
                self.suggest.completion_selected = Some(0);
                cx.notify();
                return true;
            }
            return true;
        }
        if matches!(key, "tab" | "right") {
            self.ai_prediction.verifying = true;
            if !self.observe_completion_editor(cx) {
                self.invalidate_ai_prediction();
            }
            return true;
        }
        false
    }

    pub(in crate::gpui_shell::terminal) fn has_ai_prediction(&self) -> bool {
        self.ai_prediction.candidate.is_some()
    }

    #[cfg(test)]
    pub(super) fn prediction_queued_for_test(&self) -> bool {
        self.ai_prediction.queued
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests;
