# Shell input protocol recovery

## Status

Implemented and locally reviewed; native installed-app acceptance remains pending.

## Context

An exited AI CLI can leave CSI-u keyboard and SGR mouse reporting enabled.
The shell then receives application control reports instead of ordinary input;
visible fragments such as `95u` and `58;28M` are consistent with that state.
An AI turn ending is not the CLI exiting. SSH EOF is also distinct from an AI
exit that returns to the same live remote shell.

## Evidence

The [stream regressions](../../../../nebula_terminal/src/event_loop.rs) failed
before restoration was added. The [view regressions](../../../../nebula_app/src/gpui_shell/terminal/view/activity_tests.rs)
also reproduced a remote shell without OSC integration: a real confirmed shell
submission, application negotiation, accepted SessionEnd, then the original
prompt. Synchronous OSC handling alone did not cover this path.

Resetting an abandoned alternate buffer by swapping screens discarded the
visible returned prompt and cursor. Consuming a native prompt before a failed
lease recovery also prevented retry. Both have regression coverage.

## Decision

[PromptState](../../../../nebula_terminal/src/term/prompt.rs) owns the input-mode
baseline and both keyboard stacks. Capture occurs at confirmed shell submission
or parsed OSC command start. OSC command completion or a new primary-screen
prompt restores only input contracts, synchronously in stream order. Prompt
geometry, grid contents and application-owned non-input modes remain separate.

For shells without OSC boundaries, the [GPUI adapter](../../../../nebula_app/src/gpui_shell/terminal/view/input_protocol.rs)
holds an opaque command lease and the existing confirmed prompt. A generation
identifies the owner; a nonempty stream feed advances its output revision. Prompt
verification and restoration share the Term lock. Local unhooked shells retain
the existing process proof; asynchronous probes must revalidate the exact lease.
Old command results cannot erase a newer application's negotiation.

An accepted owner exit permits recovery in an abandoned alternate buffer without
swapping its visible grid. Both keyboard stacks receive the shell contract, so a
late screen switch cannot resurrect leaked flags. The recovered-buffer permission
is revoked on the next confirmed shell submission or screen switch.

Pending echo barriers and restoration preparation block prompt recovery. Runtime
submission reuses the shared idle-prompt readiness proof, now returning its owned
prompt, rather than introducing another prompt grammar. Failed SSH/exited panes
block protocol writes while retaining local selection and scrolling.

## Rejected alternatives

- Global protocol disabling breaks live TUIs and legitimate shell negotiation.
- TurnDone, silence or a resize does not prove application exit.
- An asynchronous UI CommandDone reset can clobber later bytes in the same stream.
- Unconditional ALT switching loses visible shell state and cursor position.
- Restoring a guessed default loses the shell's own keyboard, paste or Win32 mode.
- A second polling service would duplicate existing process/lifecycle ownership.

## Consequences

No dependency, persistent format or thread is added. Ordinary output adds one
revision update per nonempty feed, not per byte. Baseline capture clones existing
bounded keyboard stacks only at shell boundaries. Current configuration still
prevents restoration from re-enabling a disabled Kitty protocol.

Mac Cmd+Delete and Cmd+Left/Right adapt to Ctrl+U/A/E before the existing encoder
selects ordinary or negotiated input. Option+Delete retains word deletion. This
does not globally replace the active application's keyboard protocol.

## Validation

Tests cover every split of OSC endings and a multi-command stream, shell-owned
flags, both stacks, empty feeds, resize, stale output/owners, late ALT switching,
consecutive recovered-buffer submissions, native prompt retry and runtime echo
barriers. Focused GPUI controls exercise Mac shortcuts and failed-session input.

The ignored cost probe is reproducible in
[prompt tests](../../../../nebula_terminal/src/term/prompt.rs). One Mac Intel debug
run measured 84.49 ms for 10,000 plain 256-byte feeds, 23.02 ms for 10,000 empty-stack
command boundaries and 2.43 ms for 1,000 boundaries with a 4,096-frame stack. These
are post-change local measurements, not a before/after comparison or a universal
performance guarantee. Real Windows console readers and installed-app SSH/AI
acceptance have not been exercised by this macOS test run.

## Supersedes

None. Existing native prompt coordinate and epoch ownership remains unchanged.

## Revisit when

Remote shells provide authoritative command boundaries everywhere, or a transport
can supply a stronger owner/exit proof than the current prompt and hook evidence.
