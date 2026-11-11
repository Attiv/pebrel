# An explicit inference lane keeps one admission slot free

## Status

Implemented. Format and architecture gates pass locally. Rust compilation, the
native test suite and a live SSH-plus-AI-CLI reproduction are **not** claimed:
local builds are forbidden by the repository rule, so those checks belong to the
GitHub release workflow. No real-model latency or cost measurement was taken.

## Context

Command AI explicit search and AI CLI message prediction both call the shared
`ai_inference::PreparedClient`. Admission was one flat process-wide counter with
a cap of two in-flight blocking HTTP requests. A user reported that with an SSH
tab running an AI CLI, `Cmd+Shift+Enter` failed on **every** tab, including local
ones, and recovered only after the SSH session was closed. The model relay logged
no command-AI request at all.

The per-pane `ssh_destination` / `ALT_SCREEN` guards in
`gpui_shell/terminal/view/ai_search.rs` cannot explain a cross-tab failure. The
process-wide admission counter can: `prompt_suggestions::service::Service::predict`
runs synchronously in the runtime API HTTP handler and calls the same
`ai_inference::request`, so hook-triggered predictions can occupy both slots.
`Slot::acquire()` then fails before the URL is parsed or a socket is opened, which
is exactly the observed "no request reached the relay" outcome. The spec's
"drain explicit search first" only orders the per-pane `ai_search_job` queue; it
does not cover an independent capability entering the same transport.

## Evidence

- `nebula_app/src/ai_inference.rs`: `ACTIVE`, `Slot::acquire`, the five-second
  `timeout_global`, 64 KiB response limit, no redirects and no retries.
- `nebula_app/src/ai_commands.rs`: `Prepared::request` served both
  `Purpose::Search` and `Purpose::Complete` through one unclassified call.
- `nebula_app/src/prompt_suggestions/client.rs` and `service.rs`: an independent,
  synchronous prediction path into the same `request`.
- `ai_inference/tests.rs` previously pinned "third `Slot::acquire()` fails" as
  the entire contract; `.trellis/spec/nebula/backend/command-ai.md` and
  [native runtime predictions](../prompt_suggestions/2026-10-29-native-runtime-predictions.md)
  both recorded "two concurrent slots" without a priority rule.
- [Native command AI ownership](../ai_commands/2026-11-06-native-command-ai-ownership.md)
  states that the blocking transport cannot abort an already-sent request:
  cancellation suppresses publication but the slot stays held until the deadline.
  Widening a cap of non-cancellable requests therefore does not address the cause.
- Reproduction configuration had `prompt_suggestions_enabled=1`,
  `ai_hooks_codex=1`, `ai_hooks_opencode=1`, and the same provider (`custom-3`,
  a loopback relay) selected for prediction and command AI.

## Decision

Introduce a `Lane` classification on the shared transport. `TOTAL_SLOTS` stays
at 2; `BACKGROUND_SLOTS` is 1. `Slot::acquire(lane)` compares the shared
`ACTIVE` count against the lane's cap, so:

- `Lane::Explicit` (`ai_commands::Purpose::Search`) may use the full budget;
- `Lane::Background` (`ai_commands::Purpose::Complete` and every
  `prompt_suggestions` prediction) may hold at most one slot in total;
- when one background request is in flight, an explicit request can still take
  the second slot; a second background request is refused with `Failure::Busy`.

`PreparedClient::request` takes the lane as its first argument. Callers declare
their class; the transport remains the single owner of the counter, the caps and
the `Drop` release. The wire payload, provider, credentials, deadline, response
limit, redirect and cancellation contracts are unchanged. No new dependency,
global service, thread, lock or cross-layer callback is added, and no in-flight
request is aborted. Background callers already treat `Busy` as an honest
abstention: `service.rs` uses `.ok().flatten()`, and `ai_prediction.rs` only
publishes `Ok(Some(suffix))`.

## Rejected alternatives

- **Raise the flat cap to 4**: only lowers the collision probability; hook bursts
  still fill the budget, worst-case paid concurrent requests double, and the
  number of non-cancellable in-flight requests grows. It also edits a reviewed
  budget without a regression that justifies it.
- **Separate per-capability caps (2 + 2)**: gives explicit search a guarantee but
  silently doubles total model traffic and 429 exposure for a defect fix.
- **Abortable HTTP / immediate slot release**: the transport is blocking ureq;
  this is a larger threading and lifetime change, and the ownership note
  explicitly leaves it out of scope.
- **A global "explicit request pending" flag checked by `Service::predict`**:
  cannot free a slot already held by an in-flight prediction, and adds a shared
  mutable signal between capabilities for no benefit over admission classes.
- **Queue or retry the explicit request**: contradicts the checked-in
  "Busy, no automatic retry" contract and hides the latency from the user.

## Consequences

- A user-initiated command search can no longer be starved by background
  predictions. It can still be refused when two explicit requests are genuinely
  in flight, which remains the honest `Busy` outcome.
- Background prediction throughput drops from two concurrent requests to one.
  Both features are opportunistic; a dropped prediction produces no suggestion
  and no UI error. Measured quality impact on a real provider is not established.
- Total concurrency, paid request exposure, deadlines and cancellation semantics
  are unchanged. No setting, persisted format, protocol or UI text changes.
  Per the README rule this is a defect fix, so neither README is edited.

## Validation

- `ai_inference/tests.rs`: existing auth/redirect/response-limit/cancellation
  cases now pass `Lane::Explicit`; the renamed total-cap test still proves a
  third explicit slot is refused and returns `Busy`; the new
  `background_predictions_never_displace_an_explicit_request` proves (a) a second
  background slot is refused, (b) a background `request` maps to `Busy` without
  reaching the server, (c) an explicit request completes over loopback HTTP while
  a background slot is held, and (d) slots return to zero.
- `cargo fmt --all -- --check`: clean (no diff).
- `python3 scripts/check_architecture.py --base HEAD`: passes; only pre-existing
  line-budget tightening notices.
- **Not run locally (by rule)**: `cargo build/check/test`, the native SSH session
  reproduction and real-model acceptance. These require the GitHub release
  workflow. This note does not claim they passed.

## Supersedes

The slot-admission clause of
[native runtime predictions](../prompt_suggestions/2026-10-29-native-runtime-predictions.md)
and the equivalent "two process-wide inference slots" wording in the command AI
spec. All other rationale in those records remains authoritative.

## Revisit when

Measured evidence shows background prediction starvation harms the product, an
abortable transport becomes available, a third capability enters the shared
transport, or a real-provider measurement justifies a different total budget.
Any such change needs its own reviewed contract and positive/negative tests.
