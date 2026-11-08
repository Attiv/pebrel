//! Native trailing display stays Shell-owned; the chooser only owns explicit adoption.
use super::*;
use nebula_settings::CommandAiSource;

impl TerminalView {
    pub(in crate::gpui_shell::terminal::view) fn native_prediction_coexists(&self) -> bool {
        self.ai_prediction
            .request
            .as_ref()
            .is_some_and(|r| r.shell == "zsh" && r.snapshot.native_suffix.is_some())
    }

    fn coexistence_item(&self, source: CommandAiSource) -> Option<NebulaCompletionItem> {
        match source {
            CommandAiSource::Ai => self.ai_prediction.candidate.clone(),
            CommandAiSource::History => {
                let snapshot = &self.ai_prediction.request.as_ref()?.snapshot;
                let suffix = snapshot.native_suffix.as_ref().filter(|s| !s.is_empty())?;
                Some(NebulaCompletionItem {
                    label: format!("{}{suffix}", snapshot.line),
                    insert: suffix.clone(),
                    replace_chars: 0,
                    replace_after_chars: 0,
                    kind: NebulaCompletionKind::History,
                })
            },
        }
    }

    pub(in crate::gpui_shell::terminal::view) fn present_native_prediction_choices(
        &mut self,
    ) -> bool {
        if !self.native_prediction_coexists() {
            return false;
        }
        // The Shell already paints its own suffix; never paint a second inline suggestion over it.
        self.suggest.suggestion.clear();
        self.suggest.suggestion_edit = None;
        self.suggest.completion_popup_requested = true;
        self.suggest.completion_items = [CommandAiSource::History, CommandAiSource::Ai]
            .into_iter()
            .filter_map(|source| self.coexistence_item(source))
            .collect();
        self.suggest.completion_selected = self.ai_prediction.manual_source.and_then(|source| {
            self.suggest.completion_items.iter().position(|item| {
                (item.kind == NebulaCompletionKind::Ai) == (source == CommandAiSource::Ai)
            })
        });
        true
    }

    pub(in crate::gpui_shell::terminal::view) fn select_native_prediction_choice(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        self.validate_ai_prediction(cx);
        if !self.native_prediction_coexists() {
            return false;
        }
        let Some(item) = self.suggest.completion_items.get(index) else {
            return false;
        };
        self.ai_prediction.manual_source = Some(if item.kind == NebulaCompletionKind::Ai {
            CommandAiSource::Ai
        } else {
            CommandAiSource::History
        });
        self.present_native_prediction_choices();
        cx.notify();
        true
    }

    /// Some(false) deliberately bypasses application completion and preserves the original key.
    pub(in crate::gpui_shell::terminal::view) fn native_prediction_key(
        &mut self,
        key: &str,
        cx: &mut Context<Self>,
    ) -> Option<bool> {
        self.validate_ai_prediction(cx);
        if !self.native_prediction_coexists() {
            return None;
        }
        if key == "enter" {
            self.invalidate_ai_prediction();
            return Some(false);
        }
        if key == "escape" {
            self.dismiss_ai_prediction();
            return Some(true);
        }
        if matches!(key, "up" | "down") {
            let count = self.suggest.completion_items.len();
            if count == 0 {
                return Some(false);
            }
            let selected =
                self.suggest.completion_selected.unwrap_or(if key == "up" { 0 } else { count - 1 });
            let next =
                if key == "up" { (selected + count - 1) % count } else { (selected + 1) % count };
            return Some(self.select_native_prediction_choice(next, cx));
        }
        if !matches!(key, "tab" | "right") {
            return Some(false);
        }
        if self.ai_prediction.verifying {
            return Some(true);
        }
        let settings = &cx.global::<Settings>().command_ai;
        let source = self.ai_prediction.manual_source.unwrap_or(if key == "tab" {
            settings.tab_source
        } else {
            settings.right_source
        });
        if self.coexistence_item(source).is_none() {
            return Some(false);
        }
        self.ai_prediction.accepting_source = Some(source);
        self.ai_prediction.verifying = true;
        if !self.observe_completion_editor(cx) {
            self.invalidate_ai_prediction();
        }
        Some(true)
    }

    pub(in crate::gpui_shell::terminal::view) fn finish_native_prediction_adoption(
        &mut self,
        snapshot: &editor::Snapshot,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(source) = self.ai_prediction.accepting_source.take() else {
            return false;
        };
        let valid = self.ai_prediction.request.as_ref().is_some_and(|r| &r.snapshot == snapshot);
        let item = valid.then(|| self.coexistence_item(source)).flatten();
        self.invalidate_ai_prediction();
        if let Some(mut item) = item {
            item.kind = NebulaCompletionKind::Command;
            self.accept_completion_item(item, cx);
        }
        true
    }

    pub(in crate::gpui_shell::terminal::view) fn native_prediction_hint_changed(
        &mut self,
        snapshot: editor::Snapshot,
        cx: &mut Context<Self>,
    ) {
        self.validate_ai_prediction(cx);
        let Some(request) = &self.ai_prediction.request else {
            return;
        };
        if request.shell != "zsh" || snapshot == request.snapshot {
            return;
        }
        if self.ai_prediction.verifying
            || request.snapshot.line != snapshot.line
            || request.snapshot.cursor != snapshot.cursor
        {
            self.invalidate_ai_prediction();
        } else {
            // Same model input, new local decoration: keep the AI reply/selection, replace only the native candidate.
            self.ai_prediction.request.as_mut().unwrap().snapshot = snapshot;
            self.present_ai_prediction();
        }
        cx.notify();
    }
}
