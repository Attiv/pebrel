# Acceptance source defaults remain independent command-AI settings

## Status

Accepted, 2026-11-08.

## Context

Coexisting native-history and AI candidates require a default source for each acceptance key. Reinterpreting the existing completion-style setting would change unrelated native completion behavior and could not express the two independent choices.

## Evidence

[CommandAi](../../../../nebula_settings/src/command_ai.rs) already owns an independent provider/model and default-off prediction flag. Its new typed `CommandAiSource` has only `ai` and `history`; settings tests cover the two choices through defaults, parsing, roundtrip and reset.

## Decision

Persist `command_ai_tab_source` and `command_ai_right_source` in the existing settings format. Default Tab to AI and Right to history; allow both keys to choose the same source. Missing or invalid values retain those defaults. Do not enable inference just because a source preference exists. Reset/backup and hot invalidation use the existing command-AI paths; manual chooser selection is ephemeral application state, not another persistent preference.

## Rejected alternatives

- A second global default plus per-key settings: redundant authority.
- Changing the ordinary completion-style setting: affects non-coexistence paths the user did not request.
- Persisting candidate text or manual selection: stale sensitive data with no configuration value.

## Consequences

Old configuration remains valid and prediction remains opt-in. No production dependency, migration service or new file format is needed. Application routing applies these preferences only at the admitted coexistence boundary.

## Validation

Settings tests exercise legal/invalid values, old configuration, roundtrip and independent reset; application UI and encrypted-backup regressions protect the connected selection path. Key-routing tests cover both assignments and manual override.

## Supersedes

None; extends the [independent command-AI selection](2026-11-06-independent-command-selection.md) without changing provider/model ownership.

## Revisit when

There is an approved requirement for more native sources or different acceptance keys; do not generalize the enum into a provider framework in advance.
