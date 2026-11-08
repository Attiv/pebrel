# Boot readiness shares one deadline with the process-tree probe

## Status

Accepted, 2026-11-08.

## Context

Boot conformance queried the native process tree once before starting its existing startup-text poll. A process query that was not yet available therefore failed immediately even when a real tree and screen could become available within the already-permitted startup interval.

## Evidence

Same-source release `42467879349`, attempt 2, passed all four native suites. The packaged Intel application painted a window, but boot conformance failed `pane.procs/process_query_failed` after 2102 ms. The artifact omitted the server's structured root PID/reason, so it does not establish whether that particular query was transient. A deterministic test of [the real boot case](../../../../scripts/conformance/cases/boot.py) demonstrates the separate ordering defect: a failed first query followed by a real valid tree was rejected before the original readiness budget could apply.

## Decision

- Require both a nonempty process tree for the original window/pane and the existing first-screen condition inside one original startup deadline, default 20 seconds. Do not add a second waiting window or change the product's process-query limit.
- During that startup probe only, `process_query_failed` may remain not-ready. Wrong targets, malformed replies and unrelated API errors fail immediately. Permanent query failures still fail at the same deadline, retaining their original diagnostic details.
- Bound each RPC by the smaller of its existing client timeout and remaining readiness time. Do not issue a request with no time remaining, reset the deadline, or accept a result that arrives after it. Bound polling sleep by remaining time too.
- Pass that absolute monotonic deadline into polling instead of reconstructing it from remaining seconds at a later call boundary. A controlled scheduling-delay counterexample extended a four-second failure window to five seconds; the optional poll deadline now only tightens its existing relative cap, with unchanged default callers.
- Preserve the default behavior of other API/read callers; the optional per-request timeout exists only to tighten a caller's budget. Keep all real process-tree, screen and golden assertions.

## Rejected alternatives

- Another blind same-source rerun: it does not correct the demonstrated readiness ordering.
- Higher timeouts, skipped probes, empty/fabricated trees or changed goldens: each weakens the native gate.
- Catch every API failure as retryable: would hide wrong target, authentication and protocol defects.
- Claim the original Intel query was definitely transient: its dropped details do not support that conclusion.

## Consequences

The gate allows legitimate readiness within its original budget while becoming stricter about target identity and late replies. No application runtime API, process-tree implementation, platform scope or package behavior changes. Failed queries become diagnosable rather than losing their server reason.

## Validation

[Harness/boot tests](../../../../scripts/conformance/tests/test_harness.py) exercise transient-to-valid, permanent failure, required process-field/root validation, later fatal errors without stale-error masking, preserved details, decreasing RPC budgets, exhausted budgets, delayed poll entry and late-result rejection. Default polling and an absolute deadline's inability to extend its relative budget have separate regressions. The existing resize fixture keeps its read-count/geometry/cleanup assertions without prescribing a finite number of monotonic-clock reads. Fresh same-source packaged native conformance is still required before publication; unit tests alone are not that evidence.

## Supersedes

None. This corrects readiness ordering without relaxing the existing common golden or adding a native-test exception.

## Revisit when

The runtime process-query failure categories change, or a new startup phase needs to share the same readiness deadline.
