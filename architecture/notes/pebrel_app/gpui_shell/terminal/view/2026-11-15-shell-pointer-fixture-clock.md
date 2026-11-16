# Shell pointer fixtures do not depend on the runner clock

## Status

Accepted.

## Context

A shell pointer query records `Request.created: Instant`, and
`pebrel_app/src/gpui_shell/terminal/view/shell_pointer.rs::observe_shell_pointer`
discards any reply older than one second. That guard is a production contract:
a late reply must never retarget a newer click or edit a prompt that has moved.

The functional fixtures in
`pebrel_app/src/gpui_shell/terminal/view/pointer/tests/shell_pointer.rs`
drive the view synchronously and assert the emitted edit bytes. They do not
intend to exercise the one-second guard, but they inherit it: `gesture()` starts
the clock, and `report()` delivers the reply after a `draw()` whose duration
depends entirely on how loaded the CI runner is.

## Evidence

- Run `43705129814` (fork 2.5.8, `e4117505`) ran 3711 native tests on macOS 26:
  3710 passed, 1 failed, 46 skipped. The only failure was
  `shell_pointer_bind_x_redraw_restores_only_the_verified_selection`, with
  `left: []` / `right: [27, 52, 27, 91, 52, 53, 126]` (`wire(4)`), i.e. the
  reply was discarded before any bytes were written.
- That test took 1.633 s; the same test took 0.206 s and passed in run
  `43372734030` (fork 2.5.7, `c614c751`, 3705/3705 passed). Every neighbour in
  the same suite was also 2–3× slower in the failed run, so the runner was
  generally slow rather than the test path changing.
- Between `c614c751` and `e4117505`, the only terminal-source change was
  `pebrel_terminal/src/tty/pointer.sh` (Bash 5.2 capability gate); the Rust
  pointer implementation and this fixture were untouched. The upstream merge
  was conflict-free.
- The seven other failing jobs in that run were policy jobs reporting
  `NATIVE_RESULT: failure`, not independent test failures. All packaging jobs
  succeeded and produced the 190 MB `Pebrel-v2.5.8` artifact.
- Expiry is already covered deliberately, with a synthetic stale request, by
  `shell_pointer/tests.rs::shell_pointer_expired_reply_does_not_edit_or_exhaust_future_requests`.

## Decision

Before delivering a `pebrel_pointer` reply, the functional fixtures call
`refresh_pending_shell_pointer_reply()`, a `#[cfg(test)]` helper that resets
`Request.created` to `Instant::now()`. Only `pointer/tests/` uses this path.
Production code is unchanged and still enforces the one-second guard; the
dedicated expiry test keeps using a deliberately two-second-old request and is
not refreshed.

## Rejected alternatives

- **Raise or remove the one-second guard.** It is a product safety contract, not
  a test budget; weakening it would let genuinely late replies edit a newer
  prompt, and would be "raising a limit to make a gate pass".
- **Delete or skip the failing test.** It is the only coverage for restoring a
  verified selection across a redraw.
- **Rerun the failed job and move on.** The 2.5.8 packages were already built,
  but a load-dependent fixture would keep failing on slow runners. The user
  explicitly asked for this class of failure to stop recurring.
- **Replace wall-clock expiry with a logical generation in production.** The
  existing epoch / origin / dimension checks do not cover a shell that rewrites
  the same line within one generation; changing that contract for a flaky test
  is disproportionate and cannot be validated locally (no local Rust builds).

## Consequences

- The functional pointer fixtures are now independent of CI runner latency and
  still assert the same edit bytes.
- The `#[cfg(test)]` helper does not exist in release binaries, so the 2.5.8
  packages are byte-for-byte the same product code as the failed run.
- The dedicated expiry test remains the single authority on stale replies.
  Future fixtures that *do* want to assert expiry must bypass `report()` and
  construct the request themselves, as `shell_pointer/tests.rs` does.

## Validation

`cargo fmt --all -- --check` (exit 0), `git diff --check` (clean), and the
existing `scripts/tests/test_stable_release.py` gate (33 tests) pass locally.
Rust compilation and the full native suite are delegated to the release
workflow; the follow-up run is the authority for this fix.

## Supersedes

None.

## Revisit when

The pointer reply guard gains an injectable clock, or a fixture in
`pointer/tests/` needs to assert wall-clock expiry. In either case remove the
refresh helper and give the fixture an explicit clock.
