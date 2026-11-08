# Native trailing display is editor metadata, not command input

## Status

Accepted, 2026-11-08.

## Context

ZLE suggestions can exist outside BUFFER and change during an asynchronous redraw. A consumer needs that distinction without teaching terminal core about AI, reading history files or inferring text from painted cells.

## Evidence

[The integration](../../../../nebula_terminal/src/tty/completion.sh) extends private editor reports with optional `utf8+postdisplay` state. A real asynchronous zsh-autosuggestions run showed that its FD callback can call `zle -R` without immediately invoking `line-pre-redraw`; that hook alone left the consumer unaware until another key event.

## Decision

- Report actual editable text/caret separately from bounded trailing display; preserve old report formats and the existing bounded OSC transport. Missing capability is not an invented empty suggestion.
- Use the ordinary redraw hook for native changes and a thin wrapper around the plugin's public `autosuggest-suggest` widget for its asynchronous result. Preserve the original widget, arguments and return value. Do not modify user key bindings or private plugin functions.
- Check installation at prompt boundaries so repeated initialization or plugin reload does not create recursive wrapper chains. Private query widgets remain distinct from user text and must not clear the native suggestion.
- Keep this interface generic editor metadata. The application alone decides presentation, model admission and acceptance sources.

## Rejected alternatives

- Only `line-pre-redraw`: contradicted by the real asynchronous callback experiment.
- Polling or patching private plugin functions: unnecessary lifetime/coupling cost.
- Treating POSTDISPLAY as BUFFER or parsing gray cells: corrupts native input identity.

## Consequences

The integration has a narrow public-widget compatibility seam for zsh-autosuggestions. Unsupported/old peers retain ordinary behavior. Trailing text is local ephemeral state, not history/model context; existing transport and text limits still apply.

## Validation

Parser regressions separate UTF-8 input and suffix and reject malformed/oversized state while accepting old formats. Real zsh/plugin PTY checks cover unchanged private observation, repeated initialization, plugin reload, preserved bindings and adoption without execution. The wrapper preserves return status in code; an interrupted additional return-status/undo experiment is not counted as native evidence. Native application checks separately cover source choice. Other plugin families are not established by these results.

## Supersedes

None; this is an optional extension of the existing native-editor report.

## Revisit when

The plugin removes or changes its public suggest widget, or another editor requires a different bounded decoration notification.
