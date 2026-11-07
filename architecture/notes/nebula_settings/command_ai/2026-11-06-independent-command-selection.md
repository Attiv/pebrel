# Independent command AI selection

## Status

Implemented and covered by settings round-trip/reset and application backup/settings-control tests.

## Context

Explicit command generation and optional automatic completion share one provider selection, but must not change an Agent CLI model or the existing AI CLI next-message predictor. Automatic requests need a separate opt-in from manual generation.

## Evidence

`nebula_settings::CommandAi` owns provider/model identifiers and `prediction_enabled`; runtime export, defaults, reset and validated updates use that type. Provider metadata/credentials remain in the existing application provider store. The settings controls and encrypted-backup metadata regression exercise these same values.

## Decision

Persist `command_ai_provider_id`, `command_ai_model` and `command_ai_prediction_enabled` as independent runtime keys. Missing/invalid/partial settings never enable automatic prediction. Manual search requires a configured explicit provider/model but does not require the prediction flag. Empty selection can be saved without a live provider.

Keep validation and updates in settings core, with no new production dependency. Preserve unrelated/unknown settings and existing message/CLI selection. Successful application settings/provider/credential mutations invalidate the prepared command snapshot; backup restore reloads metadata and UI drafts. Identifiers are not credentials and do not create another key file.

## Rejected alternatives

- Sharing `PromptSuggestions` persisted identity: two different features would silently change each other's provider and enablement.
- Automatically selecting the first provider or enabling prediction after setup: neither is the user's explicit opt-in.
- Saving raw queries/generated commands in settings: no persistence need justifies the additional data.

## Consequences

Old settings load with manual generation unconfigured and automatic prediction off. Reset removes only the new keys; backup treats them as existing settings metadata. No saved-command/provider/history format migration or credential duplication is needed. Inference/input ownership is documented in the application [command AI note](../../nebula_app/ai_commands/2026-11-06-native-command-ai-ownership.md).

## Validation

Settings tests cover defaults, partial/invalid values, explicit selection without prediction, round-trip/clearing, line injection and reset preserving unrelated data. Application tests cover provider eligibility, rendered save/switch/clear controls and encrypted-backup metadata. Existing typed-i18n/fallback contracts passed; this is not a native-platform UI guarantee.

## Supersedes

None.

## Revisit when

Search and prediction require genuinely independent provider selections or the accepted persistent/backup contract changes.
