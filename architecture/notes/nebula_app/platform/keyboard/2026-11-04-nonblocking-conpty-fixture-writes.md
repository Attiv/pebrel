# Native keyboard fixtures must distinguish pending from closed writes

## Status

Implemented. Portable contract tests pass; Windows native acceptance requires the new CI run.

## Context

Release run 41756104642 failed one Windows x64 native test after 7.985 seconds:
modified_enter_reaches_native_and_byte_readers_through_conpty called write_all and
received WriteZero. The ready receipt had already been observed. Other target
native checks passed, but that does not validate this host's result or authorize
ignoring its publication gate.

## Evidence

- [The native keyboard fixture](../../../../../nebula_app/src/platform/keyboard/tests.rs)
  owns its ConPTY and requires four program/mode/reader combinations to produce
  exact native-record or byte results. Ready proves reader setup, not writability.
- [The Windows writer](../../../../../nebula_terminal/src/tty/windows/blocking.rs)
  maps piper Pending to zero, as required by the existing event-loop contract.
  Locked piper 0.2.4 can fairly yield while an open pipe has free capacity; its
  closed result is also zero.
- [The event loop](../../../../../nebula_terminal/src/event_loop.rs) preserves
  pending bytes on zero/transient errors instead of calling blocking write_all.
- A portable replay using the locked piper dependency reproduced the exact
  WriteZero on an open, nonfull pipe with no queued bytes. The hosted log lacks
  pipe state, so its precise zero cannot be attributed to yielding alone.

## Decision

- Keep production nonblocking write/event-loop semantics and input encoders.
- Expose only the writer's read-only underlying closure state. Child liveness
  alone cannot detect a failed writer worker.
- The fixture advances only by accepted bytes, with its existing ten-second
  polling bound. Open zero, Interrupted and WouldBlock may retry within that
  bound; real close/WriteZero/BrokenPipe/other errors remain failures.
- Keep consuming conout, detect fixture errors/child exit, and preserve owned
  PTY/temporary-directory RAII cleanup. A completed result receipt is checked
  before treating normal child exit as premature.
- [The private progress rule](../../../../../nebula_app/src/platform/keyboard/tests/write_progress.rs)
  is called by the real fixture and portable tests. It is not a runtime blocking
  API or configurable retry framework.
- Retain all four encoded-input/result assertions and both reader contracts.

## Rejected alternatives

- Skipping the native test, weakening publication gates or calling another
  architecture's success proof for Windows x64.
- Adding a fixed pre-write sleep or ignoring WriteZero without distinguishing
  pending from closed; neither establishes valid progress.
- Retrying write_all from byte zero after partial progress, which can replay
  part of an Enter record.
- Changing the production writer's IO contract or exporting a broad blocking
  helper for this test-owned polling requirement.

## Consequences

The fixture tolerates the existing nonblocking contract but still fails on true
transport/reader failures and deadlines. No new dependency, global retry policy,
mode waiver, scheduling exemption, child ownership or input transformation exists.
The closure query identifies a stopped receiving worker, not a successful child
operation; the native result receipt remains the correctness authority.

## Validation

The actual writer's real-piper open-yield/closed regression and the progress
partial/transient/closed/error/count/deadline tests pass in portable checks.
Portable probes read the actual writer methods/test and actual fixture helpers;
only unregistered IOCP interest and the OS PTY/child are substituted on Mac.
They also verify no payload replay across 1000 open pipes, immediate closed-pipe
failure, error-receipt rejection and child-exit rejection. This is not Windows
ConPTY, installed-application or hardware acceptance. The complete native Windows
x64/ARM suites and original keyboard fixture remain required in hosted CI.

## Supersedes

None.

## Revisit when

The locked piper/Windows adapter contract changes or the fixture is migrated to
an existing evented owner instead of test-local finite polling.
