//! Independent provider selection for explicit command search and opt-in completion.

use crate::{PromptSuggestions, RawSettings};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandAiSource {
    Ai,
    History,
}

impl CommandAiSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ai => "ai",
            Self::History => "history",
        }
    }
    fn read(value: Option<&str>, fallback: Self) -> Self {
        match value {
            Some("ai") => Self::Ai,
            Some("history") => Self::History,
            _ => fallback,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandAi {
    pub provider_id: String,
    pub model: String,
    pub prediction_enabled: bool,
    pub tab_source: CommandAiSource,
    pub right_source: CommandAiSource,
}

impl Default for CommandAi {
    fn default() -> Self {
        Self {
            provider_id: String::new(),
            model: String::new(),
            prediction_enabled: false,
            tab_source: CommandAiSource::Ai,
            right_source: CommandAiSource::History,
        }
    }
}

impl CommandAi {
    pub const PROVIDER_KEY: &'static str = "command_ai_provider_id";
    pub const MODEL_KEY: &'static str = "command_ai_model";
    pub const PREDICTION_KEY: &'static str = "command_ai_prediction_enabled";
    pub const TAB_SOURCE_KEY: &'static str = "command_ai_tab_source";
    pub const RIGHT_SOURCE_KEY: &'static str = "command_ai_right_source";
    pub const MAX_VALUE_CHARS: usize = PromptSuggestions::MAX_VALUE_CHARS;

    pub(crate) fn from_raw(raw: &RawSettings) -> Self {
        let read =
            |key| raw.value(key).filter(|value| Self::valid_value(value)).unwrap_or("").to_owned();
        let mut value = Self {
            provider_id: read(Self::PROVIDER_KEY),
            model: read(Self::MODEL_KEY),
            prediction_enabled: raw.bool_on(Self::PREDICTION_KEY).unwrap_or(false),
            tab_source: CommandAiSource::read(raw.value(Self::TAB_SOURCE_KEY), CommandAiSource::Ai),
            right_source: CommandAiSource::read(
                raw.value(Self::RIGHT_SOURCE_KEY),
                CommandAiSource::History,
            ),
        };
        value.prediction_enabled &= value.is_configured();
        value
    }

    pub fn is_configured(&self) -> bool {
        !self.provider_id.is_empty()
            && !self.model.is_empty()
            && Self::valid_value(&self.provider_id)
            && Self::valid_value(&self.model)
    }

    pub fn valid_value(value: &str) -> bool {
        PromptSuggestions::valid_value(value)
    }

    /// Validate metadata before it can introduce lines or an incomplete opt-in.
    pub fn updates(&self) -> Option<[(&'static str, String); 5]> {
        if !Self::valid_value(&self.provider_id)
            || !Self::valid_value(&self.model)
            || (self.prediction_enabled && !self.is_configured())
        {
            return None;
        }
        Some([
            (Self::PROVIDER_KEY, self.provider_id.clone()),
            (Self::MODEL_KEY, self.model.clone()),
            (Self::PREDICTION_KEY, (self.prediction_enabled as u8).to_string()),
            (Self::TAB_SOURCE_KEY, self.tab_source.as_str().to_owned()),
            (Self::RIGHT_SOURCE_KEY, self.right_source.as_str().to_owned()),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RuntimeSettings, apply_updates};

    fn read(text: &str) -> CommandAi {
        RuntimeSettings::from_raw(&RawSettings::from_text(text)).command_ai
    }

    #[test]
    fn acceptance_sources_default_validate_round_trip_and_reset_independently() {
        use CommandAiSource::{Ai, History};
        assert_eq!((read("").tab_source, read("").right_source), (Ai, History));
        assert_eq!(
            (
                read("command_ai_tab_source=invalid\ncommand_ai_right_source=invalid\n").tab_source,
                read("command_ai_tab_source=invalid\ncommand_ai_right_source=invalid\n")
                    .right_source
            ),
            (Ai, History)
        );
        for tab_source in [Ai, History] {
            for right_source in [Ai, History] {
                let expected = CommandAi { tab_source, right_source, ..Default::default() };
                let saved = apply_updates("unknown=keep\n", &expected.updates().unwrap());
                assert_eq!(read(&saved), expected);
                assert!(saved.starts_with("unknown=keep\n"));
            }
        }
        assert_eq!(
            crate::ui_value_kind(CommandAi::TAB_SOURCE_KEY),
            Some(crate::UiValueKind::String)
        );
        assert_eq!(
            crate::ui_value_kind(CommandAi::RIGHT_SOURCE_KEY),
            Some(crate::UiValueKind::String)
        );
    }

    #[test]
    fn old_partial_and_invalid_settings_never_enable_prediction() {
        for text in [
            "",
            "ghost=1\nprompt_suggestions_enabled=1\n",
            "command_ai_prediction_enabled=1\n",
            "command_ai_prediction_enabled=true\ncommand_ai_model=model\n",
            "command_ai_prediction_enabled=invalid\ncommand_ai_provider_id=one\ncommand_ai_model=model\n",
            "command_ai_prediction_enabled=1\ncommand_ai_provider_id=one\ncommand_ai_model=not valid\n",
        ] {
            assert!(!read(text).prediction_enabled);
        }
        assert_eq!(read(""), CommandAi::default());
    }

    #[test]
    fn manual_search_does_not_require_automatic_prediction() {
        let settings = read("command_ai_provider_id=one\ncommand_ai_model=vendor/model\n");
        assert!(settings.is_configured());
        assert!(!settings.prediction_enabled);
    }

    #[test]
    fn config_defaults_keep_metadata_strings_and_prediction_boolean() {
        assert_eq!(crate::ui_value_kind(CommandAi::PROVIDER_KEY), Some(crate::UiValueKind::String));
        assert_eq!(crate::ui_value_kind(CommandAi::MODEL_KEY), Some(crate::UiValueKind::String));
        assert_eq!(
            crate::ui_value_kind(CommandAi::PREDICTION_KEY),
            Some(crate::UiValueKind::Boolean)
        );
    }

    #[test]
    fn selection_round_trips_without_mutating_other_ai_or_shell_preferences() {
        let expected = CommandAi {
            provider_id: "custom-2".into(),
            model: "vendor/model".into(),
            prediction_enabled: true,
            ..CommandAi::default()
        };
        let original = "ghost=0\ncompletion_style=popup\nprompt_suggestions_enabled=1\nprompt_suggestions_provider_id=chat\nprompt_suggestions_model=chat-model\ncli_model=keep\nunknown=value\n";
        let saved = apply_updates(original, &expected.updates().unwrap());
        assert!(saved.starts_with(original));
        assert_eq!(read(&saved), expected);
        let cleared = CommandAi::default();
        let saved = apply_updates(&saved, &cleared.updates().unwrap());
        assert_eq!(read(&saved), cleared);
        assert!(saved.starts_with(original));
        assert_eq!(saved.matches("command_ai_model=").count(), 1);
    }

    #[test]
    fn invalid_identifiers_reject_line_injection_and_incomplete_opt_ins() {
        for model in [
            "model\nother=1".into(),
            "model\rnext".into(),
            "model name".into(),
            "model\u{2028}next".into(),
            "a".repeat(CommandAi::MAX_VALUE_CHARS + 1),
        ] {
            let value = CommandAi {
                provider_id: "one".into(),
                model,
                prediction_enabled: false,
                ..CommandAi::default()
            };
            assert!(value.updates().is_none());
        }
        assert!(CommandAi { prediction_enabled: true, ..CommandAi::default() }.updates().is_none());
        assert!(
            CommandAi {
                provider_id: "one\ninjected=1".into(),
                model: "model".into(),
                prediction_enabled: true,
                ..CommandAi::default()
            }
            .updates()
            .is_none()
        );
    }
}
