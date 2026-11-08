# Native history and AI share a chooser, not a painted suffix

## Status

Accepted, 2026-11-08.

## Context

Shell history suggestions are not ordinary application completion results. ZLE paints its trailing display without putting it in the editable buffer. Application-only local-first arbitration therefore cannot prevent two independently painted suffixes or establish which source an acceptance key means.

## Evidence

The integrated [editor](../../../../nebula_app/src/gpui_shell/terminal/view/editor.rs) separates real input from optional native trailing display. [Coexistence routing](../../../../nebula_app/src/gpui_shell/terminal/view/ai_prediction/coexistence.rs) reuses the existing completion chooser and verified editor adoption. The native fixture uses real zsh 5.9 and pinned zsh-autosuggestions; captured HTTP contains only input/OS/Shell, and execution markers appear only after a separate Enter.

## Decision

- Keep native gray text Shell-owned; display history and AI in the existing source-labelled popup, never another inline suffix over it. Known native-suggestion capability uses this presentation even while its suffix is empty, because history can arrive asynchronously.
- Keep per-key defaults in settings; a deliberate candidate selection overrides both defaults. Unavailable sources return the original Shell key path, including the user's own bindings, rather than selecting another application candidate.
- Allow parallel inference in this scoped native-zsh path. Retain opt-in, idle gating, the shared actual request lane and manual-search priority. Other completion modes retain their existing arbitration.
- Requery exact native input, caret and suffix before adopting a chosen coexistence candidate. Hint-only changes may update the local history row without changing model input or stealing manual selection; changes during verification reject adoption.
- Keep native hint text ephemeral and local. Explicit AI search compares actual draft identity rather than confusing trailing decoration with editable input.

## Rejected alternatives

- Disabling the user's history plugin or requiring mutually exclusive AI/history: the user explicitly requested both.
- Two independently painted gray suffixes or sending one key to both owners: ambiguous presentation and duplicate insertion.
- Reading history files or scraping rendered colors: unnecessary access and no reliable native editing identity.
- Treating every hint change as a new model input: the hint is not sent to inference and does not justify another request.

## Consequences

Parallel mode can make more model requests than local-first fallback. Generic Shell state remains in the editor boundary; product source choice remains in the application. The [terminal protocol note](../../nebula_terminal/tty/2026-11-08-native-trailing-display.md) and [settings note](../../nebula_settings/command_ai/2026-11-08-per-key-acceptance-sources.md) own their respective contracts.

## Validation

Prediction/editor/routing/settings regressions cover configurable sources, manual selection, stale native hints, no-submit and normal fallback. The native fixture covers default Tab→AI, reassigned Tab→history and manual history selection overriding Right→AI using the real plugin and real PTY. This is not other-plugin, Windows/Linux, real OS IME or all-theme/DPI acceptance.

## Supersedes

The single-source local-priority and mode-default adoption clauses of [the original command-AI note](2026-11-06-native-command-ai-ownership.md), only for admitted native-zsh coexistence. Its other identity, HTTP, privacy and search-ownership decisions remain.

## Revisit when

Another native editor can advertise equivalent bounded suggestion state, or a plugin changes its public widget/notification contract.
