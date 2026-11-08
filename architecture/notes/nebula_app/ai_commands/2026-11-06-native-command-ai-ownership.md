# Native command AI ownership

## Status

Implemented for explicit in-place command search and opt-in suffix completion through the same prepared inference boundary. Verification below does not establish untested platforms or real-model quality.

Native-zsh source priority and acceptance defaults are partially superseded by [native-history coexistence](2026-11-08-native-history-coexistence.md); the remaining ownership decisions stand.

## Context

Command generation must use the terminal input location without taking over Shell editing or accidentally sending a natural-language request to the foreground program. Existing AI CLI next-message prediction has different prompts, context and turn ownership. Reusing its whole service would conflate these lifecycles.

## Evidence

- `ai_commands.rs` constructs a wire object from input, normalized OS and Shell only. Pane identity, cwd, native line/caret and configuration revision stay local.
- `ai_inference.rs` extracts the existing prepared credential/pooled HTTP boundary. `prompt_suggestions/client.rs` retains its original purpose payload and schema.
- `gpui_shell/terminal/view/ai_search.rs` owns the query control and asynchronous job. `editor.rs` separates pure Observe from explicit Accept/List; observation timeout has no fallback Tab.
- `completion::command_ai_shell` checks execution scope; native owner/prompt and focused view checks establish input ownership. A locally launched tab is not proof of local Shell context.
- `completion::command_prediction_eligible` owns prefix/local-result admission; `terminal/view/ai_prediction.rs` schedules passive capture and completion from events. A cache-hit paint can mark local readiness but cannot initiate inference.
- Search/native and workspace tests exercise the actual query, PTY and action paths. A rejected action alone did not prove foreground propagation: a literal shortcut allowlist also swallowed the chord after unbinding.
- The terminal ancestor focus scope also contains reading/search inputs. It could retain AI mode after another input owner took focus; a terminal-level Blur check was not an AI ownership boundary.

## Decision

Keep a temporary GPUI query owner at the prompt location. Copy the verified native line into it, leaving the original Shell line/caret intact. Query text, IME and paste remain local. Explicit generation returns text only; adoption requeries and compares the original owner/prompt/line/caret before using the proven native edit path, with no Enter. Escape reveals the unmodified draft; a changed context never receives restored bytes.

Give the AI panel its own focus scope spanning query/result/adopt/copy controls. Leaving that scope cancels AI; internal control focus does not. Reading/search mode is mutually exclusive, and deferred workspace focus resolves to the active AI input rather than the underlying Shell handle.

Use one prepared command-AI provider snapshot and the existing process-wide HTTP admission. Cache successful preparation only so a transient preparation failure is retryable. Preserve message-prediction prompts, configuration and turn policy. Each pane keeps one actual job and at most the latest explicit pending intent; publication is invalidated on edits, context/configuration changes and destruction.

The current blocking transport cannot immediately abort already-sent HTTP. Cancellation suppresses publication; the request may retain its slot until the bounded deadline. Do not claim that closing the query stops remote computation or cost.

Keep automatic prediction in Shell-native editing, separate from the temporary query control. After idle and completed empty local results, observe a nonempty single-line end-caret prefix and request its suffix once. Local candidates arriving later still win. Search and prediction share the same actual job lane; explicit queued search has priority. Requery the native prefix before suffix adoption, and suppress a pending prediction when a completion/submission key arrives before any AI candidate, so an old local Tab reply cannot adopt a newly arrived model result. AI source decoration is display-only.

Validate the complete local request identity at the final native snapshot callback as well as model publication. Cwd or provider revision can change after sending Observe but before its reply, without an intervening paint; matching only native line/caret/epoch would authorize a stale candidate.

Apply Escape dismissal across native-key bookkeeping, not only before the key is written. Valid Win32 input records preserve the prompt while advancing its input epoch; otherwise an unchanged dismissed prefix can acquire a new idle timer. Preserve the real packet and in-flight job, and let a subsequent genuine input change rearm prediction.

Use the shared configurable action/keymap and propagate an unconsumed foreground or unbound chord through the existing terminal encoder. Input eligibility does not authorize swallowing keys for a foreground program.

## Rejected alternatives

- A command-manager mode or global search dialog: the agreed interaction belongs at the terminal input.
- Reconstructing a Shell draft from keys/grid text: it cannot establish native caret/editor ownership.
- Clearing the native line on entry and restoring it on cancel: races can destroy a draft or restore into another prompt.
- Appending a result through `runtime_prompt`: search adoption is a verified full-line edit, not append/submit.
- Sending history, cwd, filenames or output to improve relevance: these are outside the agreed contextual whitelist.
- A new Agent process, HTTP stack or generic provider framework: existing bounded transport is sufficient.

## Consequences

Manual generation works only in supported integrated local editors; remote/guest/unverified or foreground input stays outside admission. Context-free generation can be uncertain or inappropriate; it may abstain, and the user edits/submits explicitly. Text already typed by the user may itself contain sensitive parameters; the whitelist is not complete secret redaction.

No query/result persistence, saved-command migration, automatic provider selection or Agent-model rewrite is introduced. Ordinary local completion/search remains independent of model availability.

## Validation

Existing message-prediction, keymap, editor, settings, completion and backup regressions passed. Real macOS arm64 zsh/PTY/GPUI acceptance verified a Unicode draft with a middle caret, query/cancel preservation, no execution on adoption and execution only on a separate Enter. Delayed loopback HTTP cases cover query/reopen/focus/config/cwd/destroy/latest-intent behavior. The exact commands and counters are recorded with the task verification, not copied as a universal performance claim.

Prediction acceptance additionally covers all three completion modes with Unicode prefixes, fresh rejection of a Shell-side changed prefix, early/late local-result priority, both directions of the shared request lane and Escape suppression. Controlled-clock tests cover idle scheduling and excluded inputs. Loopback timings include the idle threshold and are not measurements of an actual model.

An isolated temporary application bundle allowed native visual inspection of query/result/key hints and controls. Deactivation cancelled that visual handoff as designed; its log is not substituted for the separate successful no-hold editing test. Marked-text API coverage does not prove an OS IME candidate window. Windows/Linux/Bash native coverage and a complete theme/DPI matrix were not established by this macOS run; no real-model latency/accuracy claim is made.

## Supersedes

None. Existing completion and AI CLI message-prediction ownership contracts remain authoritative for their respective capabilities.

## Revisit when

Richer external context, remote editors, immediate HTTP abort, changed native editing protocols or measured model/slot contention require a separately reviewed contract.
