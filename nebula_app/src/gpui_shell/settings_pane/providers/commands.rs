//! Command AI metadata stays independent from Agent and next-message settings.

use super::*;
use crate::i18n::Message;
use gpui_component::menu::PopupMenuItem;
use nebula_settings::{CommandAi, CommandAiSource};

pub(super) struct CommandAiSettingsUi {
    provider_id: String,
    model: Entity<InputState>,
    prediction_enabled: bool,
    tab_source: CommandAiSource,
    right_source: CommandAiSource,
    error: Option<Message>,
    save_error: Option<String>,
    saved: bool,
    _subscription: Subscription,
}

fn available(provider: &AiProvider) -> bool {
    crate::prompt_suggestions::client::supported(provider)
        && !provider.base_url.trim().is_empty()
        && (!provider.kind.requires_api_key() || provider.api_key_set)
}

fn validation(settings: &CommandAi, providers: &[AiProvider]) -> Option<Message> {
    if settings.provider_id.is_empty() && settings.model.is_empty() && !settings.prediction_enabled
    {
        return None;
    }
    if !providers.iter().any(|provider| provider.id == settings.provider_id && available(provider))
    {
        Some(Message::AiCommandChooseProvider)
    } else if !settings.is_configured() {
        Some(Message::AiCommandInvalidModel)
    } else {
        None
    }
}

impl SettingsPane {
    pub(in crate::gpui_shell::settings_pane) fn reset_command_ai_settings(&mut self) {
        self.provider_ui.commands = None;
    }

    pub(super) fn prepare_command_ai_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.provider_ui.commands.is_some() {
            return;
        }
        let settings = &self.runtime.command_ai;
        let model = cx.new(|cx| InputState::new(window, cx).default_value(settings.model.clone()));
        let subscription = cx.subscribe(&model, |this: &mut Self, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                if let Some(ui) = this.provider_ui.commands.as_mut() {
                    ui.error = None;
                    ui.save_error = None;
                    ui.saved = false;
                }
                cx.notify();
            }
        });
        self.provider_ui.commands = Some(CommandAiSettingsUi {
            provider_id: settings.provider_id.clone(),
            model,
            prediction_enabled: settings.prediction_enabled,
            tab_source: settings.tab_source,
            right_source: settings.right_source,
            error: None,
            save_error: None,
            saved: false,
            _subscription: subscription,
        });
    }

    fn save_command_ai_settings(&mut self, prediction: Option<bool>, cx: &mut Context<Self>) {
        let Some(ui) = self.provider_ui.commands.as_ref() else { return };
        let next = CommandAi {
            provider_id: ui.provider_id.clone(),
            model: ui.model.read(cx).value().trim().to_owned(),
            prediction_enabled: prediction.unwrap_or(ui.prediction_enabled),
            tab_source: ui.tab_source,
            right_source: ui.right_source,
        };
        if let Some(error) = validation(&next, &self.provider_store.providers) {
            self.provider_ui.commands.as_mut().unwrap().error = Some(error);
            cx.notify();
            return;
        }
        let Some(updates) = next.updates() else { return };
        let result = self.try_persist(&updates, cx);
        let ui = self.provider_ui.commands.as_mut().unwrap();
        ui.error = None;
        ui.save_error = result.as_ref().err().map(ToString::to_string);
        ui.saved = result.is_ok();
        if result.is_ok() {
            ui.prediction_enabled = next.prediction_enabled;
        }
        cx.notify();
    }

    fn set_command_ai_prediction(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if enabled {
            self.save_command_ai_settings(Some(true), cx);
            return;
        }
        // A removed provider or an invalid draft must never prevent disabling paid requests.
        let result = self.try_persist(&[(CommandAi::PREDICTION_KEY, "0".into())], cx);
        if let Some(ui) = self.provider_ui.commands.as_mut() {
            ui.error = None;
            ui.save_error = result.as_ref().err().map(ToString::to_string);
            ui.saved = result.is_ok();
            if result.is_ok() {
                ui.prediction_enabled = false;
            }
        }
        cx.notify();
    }

    fn command_ai_source_control(&self, tab: bool, cx: &mut Context<Self>) -> gpui::Div {
        let ui = self.provider_ui.commands.as_ref().unwrap();
        let language = crate::gpui_shell::config::ui_language(cx);
        let selected = if tab { ui.tab_source } else { ui.right_source };
        let owner = cx.entity().downgrade();
        let button =
            Button::new(if tab { "command-ai-tab-source" } else { "command-ai-right-source" })
                .debug_selector(move || {
                    if tab { "command-ai-tab-source" } else { "command-ai-right-source" }.to_owned()
                })
                .label(language.text(if selected == CommandAiSource::Ai {
                    Message::AiCommandPredictionSource
                } else {
                    Message::AiCommandNativeHistory
                }))
                .w_full()
                .h(px(34.0))
                .map(ProviderSelectTrigger::new)
                .dropdown_menu(move |mut menu, _, _| {
                    for source in [CommandAiSource::Ai, CommandAiSource::History] {
                        let owner = owner.clone();
                        menu = menu.item(
                            PopupMenuItem::new(language.text(if source == CommandAiSource::Ai {
                                Message::AiCommandPredictionSource
                            } else {
                                Message::AiCommandNativeHistory
                            }))
                            .checked(source == selected)
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    if let Some(ui) = this.provider_ui.commands.as_mut() {
                                        if tab {
                                            ui.tab_source = source;
                                        } else {
                                            ui.right_source = source;
                                        }
                                        ui.saved = false;
                                    }
                                    cx.notify();
                                });
                            }),
                        );
                    }
                    menu
                });
        v_flex()
            .flex_1()
            .min_w(px(180.0))
            .gap_1()
            .child(language.text(if tab {
                Message::AiCommandAcceptTab
            } else {
                Message::AiCommandAcceptRight
            }))
            .child(button)
    }

    pub(super) fn command_ai_settings(&self, cx: &mut Context<Self>) -> gpui::Div {
        let Some(ui) = self.provider_ui.commands.as_ref() else { return div() };
        let language = crate::gpui_shell::config::ui_language(cx);
        let providers: Vec<_> =
            self.provider_store.providers.iter().filter(|p| available(p)).cloned().collect();
        let no_providers = providers.is_empty();
        let selected = ui.provider_id.clone();
        let label = if selected.is_empty() {
            language.text(Message::AiCommandNotConfigured).to_owned()
        } else {
            providers
                .iter()
                .find(|p| p.id == selected)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| language.text(Message::AiCommandChooseProvider).to_owned())
        };
        let owner = cx.entity().downgrade();
        let picker = Button::new("command-ai-provider")
            .debug_selector(|| "command-ai-provider".to_owned())
            .label(label)
            .w_full()
            .h(px(34.0))
            .map(ProviderSelectTrigger::new)
            .dropdown_menu(move |mut menu, _, _| {
                let clear_owner = owner.clone();
                menu = menu.item(
                    PopupMenuItem::new(language.text(Message::AiCommandNotConfigured))
                        .checked(selected.is_empty())
                        .on_click(move |_, window, cx| {
                            let _ = clear_owner.update(cx, |this, cx| {
                                if let Some(ui) = this.provider_ui.commands.as_mut() {
                                    ui.provider_id.clear();
                                    ui.prediction_enabled = false;
                                    ui.model
                                        .update(cx, |input, cx| input.set_value("", window, cx));
                                    ui.error = None;
                                    ui.save_error = None;
                                    ui.saved = false;
                                }
                                cx.notify();
                            });
                        }),
                );
                for provider in &providers {
                    let owner = owner.clone();
                    let provider = provider.clone();
                    menu = menu.item(
                        PopupMenuItem::new(provider.name.clone())
                            .checked(provider.id == selected)
                            .on_click(move |_, window, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    if let Some(ui) = this.provider_ui.commands.as_mut() {
                                        if ui.provider_id != provider.id {
                                            ui.provider_id = provider.id.clone();
                                            ui.model.update(cx, |input, cx| {
                                                input.set_value(provider.model.clone(), window, cx)
                                            });
                                        }
                                        ui.error = None;
                                        ui.save_error = None;
                                        ui.saved = false;
                                    }
                                    cx.notify();
                                });
                            }),
                    );
                }
                menu
            });
        let model = ui.model.read(cx).value();
        let draft = CommandAi {
            provider_id: ui.provider_id.clone(),
            model: model.trim().to_owned(),
            prediction_enabled: ui.prediction_enabled,
            tab_source: ui.tab_source,
            right_source: ui.right_source,
        };
        let dirty = draft != self.runtime.command_ai;
        let invalid = validation(&draft, &self.provider_store.providers);
        let configured = draft.is_configured() && invalid.is_none();
        let status = if let Some(error) = &ui.save_error {
            Some((language.format(Message::AiCommandSaveFailed, &[("error", error)]), true))
        } else if let Some(error) = ui.error.or(invalid) {
            Some((language.text(error).to_owned(), true))
        } else if ui.saved && !dirty {
            Some((language.text(Message::AiCommandSaved).to_owned(), false))
        } else if no_providers {
            Some((language.text(Message::AiCommandNoProviders).to_owned(), false))
        } else {
            None
        };
        div()
            .w_full()
            .pb_4()
            .border_b_1()
            .border_color(cx.theme().border.opacity(0.6))
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(15.0))
                            .child(language.text(Message::AiCommandSettingsTitle)),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(cx.theme().muted_foreground)
                            .child(language.text(Message::AiCommandSettingsDescription)),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .mt_3()
                    .gap_3()
                    .flex_wrap()
                    .items_end()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w(px(180.0))
                            .gap_1()
                            .child(language.text(Message::AiCommandProvider))
                            .child(picker),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w(px(180.0))
                            .gap_1()
                            .child(language.text(Message::AiCommandModel))
                            .child(Input::new(&ui.model).h(px(34.0))),
                    )
                    .child(
                        Button::new("command-ai-save")
                            .debug_selector(|| "command-ai-save".to_owned())
                            .label(language.text(Message::CommonSave))
                            .h(px(34.0))
                            .disabled(!dirty || invalid.is_some())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.save_command_ai_settings(None, cx)
                            })),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .mt_3()
                    .items_center()
                    .gap_4()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(language.text(Message::AiCommandPrediction))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(cx.theme().muted_foreground)
                                    .child(language.text(Message::AiCommandPredictionDescription)),
                            ),
                    )
                    .child(
                        crate::gpui_shell::widgets::NebulaSwitch::new("command-ai-prediction")
                            .checked(ui.prediction_enabled)
                            .disabled(!ui.prediction_enabled && !configured)
                            .on_click(cx.listener(|this, enabled: &bool, _, cx| {
                                this.set_command_ai_prediction(*enabled, cx)
                            })),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .mt_3()
                    .gap_3()
                    .flex_wrap()
                    .child(self.command_ai_source_control(true, cx))
                    .child(self.command_ai_source_control(false, cx)),
            )
            .when_some(status, |view, (text, error)| {
                view.child(
                    div()
                        .mt_2()
                        .text_size(px(12.0))
                        .text_color(if error {
                            cx.theme().danger
                        } else {
                            cx.theme().muted_foreground
                        })
                        .child(text),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_is_explicit_and_clearing_does_not_require_a_live_provider() {
        assert_eq!(validation(&CommandAi::default(), &[]), None);
        let configured = CommandAi {
            provider_id: "local".into(),
            model: "fixture".into(),
            prediction_enabled: false,
            ..CommandAi::default()
        };
        assert_eq!(validation(&configured, &[]), Some(Message::AiCommandChooseProvider));
        let provider = AiProvider::preset(ProviderKind::Ollama, "local");
        assert_eq!(validation(&configured, &[provider.clone()]), None);
        assert_eq!(
            validation(&CommandAi { model: "bad model".into(), ..configured }, &[provider]),
            Some(Message::AiCommandInvalidModel)
        );
    }

    #[test]
    fn picker_requires_supported_configured_provider_without_touching_credentials() {
        let mut provider = AiProvider::preset(ProviderKind::Custom, "fixture");
        provider.base_url = "https://example.invalid/v1".into();
        provider.model = "fixture-model".into();
        assert!(!available(&provider));
        provider.api_key_set = true;
        assert!(available(&provider));
        provider.enabled = false;
        assert!(!available(&provider));
        provider.enabled = true;
        provider.kind = ProviderKind::Google;
        assert!(!available(&provider));
        provider.kind = ProviderKind::AzureOpenAi;
        assert!(!available(&provider));
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
#[path = "commands_tests.rs"]
mod interaction_tests;
