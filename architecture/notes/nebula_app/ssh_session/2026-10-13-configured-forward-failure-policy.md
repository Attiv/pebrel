# Configured SSH forwarding failure policy

## Status

Implemented and SSH-loopback verified on 2026-10-13; native package verification is recorded separately in the task evidence.

## Context

Manual OpenSSH could open a shell while the panel stopped with `The request was rejected by the other party`. Configured RemoteForward startup propagated every setup error, although the effective OpenSSH policy was `ExitOnForwardFailure no`.

## Evidence

- Pinned russh 0.62.2 constructs RequestDenied for global forwarding requests, not private-key authentication. The server's precise refusal cause is not included in that result.
- The new loopback regression initially failed with `optional RemoteForward must not stop login: Some(RequestDenied)` while its server accepted authentication, PTY and shell.
- `ssh_session/config.rs` now owns expanded/fallback policy parsing; `forward.rs` classifies concrete refusal/conflict versus fatal transport/internal failures.
- `lifecycle.rs` applies invocation policy to the already acquired authenticated transport, then reports genuine Ready and an independent bounded warning fact.
- First current-source SSH pass: 94 passed, one existing ignored, zero failed. This is not verification against the user's server.
- Independent review reproduced route corruption arising after Pending registration: refused-request cleanup could silently skip a poisoned lock and still return an optional warning. A regression failed before cleanup-lock errors were made fatal. Another regression proved that a Pending conflict must be described as reserved, not already active.

## Decision

Default/unset/no treats configured forwarding unavailability as a warning and continues shell startup. Explicit yes remains fatal to that startup. Disconnect/send/I/O/poisoned-route errors remain fatal under either policy; explicitly requested manual forwarding still returns failure.

This classification also covers internal failures during post-request cleanup, not only the initial route-lock acquisition. A RequestDenied result cannot be downgraded to optional when its Pending reservation cannot be safely cleaned. Conflict wording describes reservation, since an incumbent can still be Pending rather than Active.

Keep accepted owners with their transport, clear refused Pending routes, reuse accepted identical owners and preserve incumbent routes on conflicts. Read policy on each invocation rather than freezing it in the transport pool identity. Request the listener on the acquired transport instead of re-resolving credentials and possibly selecting another pooled connection.

Warnings are coalesced into one count/first-route summary, delivered after genuine shell readiness on the same ordered pane event channel, translated in the UI and surfaced as a nonblocking pane warning. They do not mutate SshStage or enter remote stdin.

Expanded parsing moved to the existing configuration adapter to preserve the facade's physical-line limit and consolidate the actual configuration responsibility. No budget increase or authentication rewrite is needed.

## Rejected alternatives

- Catch every boxed error and continue: hides unusable transports and corrupt state.
- Ignore all forwarding failures: violates explicit yes and manual-operation expectations.
- Report Failed then Ready: fabricates progress, invalidates pane input state and confuses retry/connection UI.
- Log only: leaves the user believing an unavailable tunnel is active.
- Re-resolve/authenticate for each configured request: can separate the request/owner from the transport receiving forwarded channels.
- Change server policy or guess port-conflict cause: no server-side evidence establishes that cause.

## Consequences

Optional remote forwarding no longer gates ordinary login, but its unavailability remains visible. Healthy transports/accepted owners can still be shared by other panes when a strict invocation fails. There are no persisted schema, host trust, key, dependency-pin or Chinese-input changes.

## Validation

Current-source tests cover unset/no/yes parsing, offline OpenSSH expansion, refused forwarding followed by usable shell bytes, strict refusal, real disconnect/internal errors, manual refusal cleanup, accepted-owner reuse/conflict isolation and denial retry. UI warning ownership/rendering and native package checks are additional gates; state/loopback tests do not prove a real user-host or input-method fix.

Final review: SSH 96 passed / one existing ignored; rendered warning UI two passed; i18n 24 passed / one existing ignored; CI/release helpers 59 passed; real GPUI product type check, format, architecture and platform-cfg gates passed. Legacy-only checking has seven unrelated errors reproduced identically on clean `a20d3cf5`; it is not claimed as passing and was not broadened into this GPUI repair.

## Supersedes

Clarifies the failure-policy omission in [configured-forward lifecycle](2026-10-08-configured-remote-forward-lifecycle.md), retaining its transport-owned lifetime decision.

## Revisit when

Revisit for dynamic/non-loopback forwarding, different warning UX, transport replacement, or cancellable request deadlines. A timeout change needs separate late-accept/cleanup ownership evidence.
