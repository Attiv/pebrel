# Configured SSH forwarding contracts

## 1. Scope / Trigger

Configured RemoteForward startup crosses config, pooled authenticated transport and pane warning presentation. Refusal is not authentication failure; optional-forward policy must not erase actual transport failures.

## 2. Signatures

- `SshDestination::exit_on_forward_failure: bool` is private ephemeral invocation data, default false.
- `forward::open_remote_forward_on(&AcquiredSession, remote_port: u16, local_port: u16) -> Result<PortForward, ForwardError>` is the private shared request/route authority.
- `lifecycle::ensure_configured_remote_forwards(&SshDestination, &AcquiredSession) -> Result<Option<ForwardWarning>, SessionError>` retains accepted owners and coalesces unavailable routes.
- `SshEventHost::ssh_forward_warning(ForwardWarning)` delivers warning facts separately from `ssh_stage(SshStage)`.

## 3. Contracts

- Parse `exitonforwardfailure` from system-expanded OpenSSH output and conservative fallback with case-insensitive yes/no and first-value-wins scalar semantics. Invalid values do not silently become false.
- Existing loopback/nonzero-port restrictions remain unchanged. No server config, key or trust-policy changes.
- Request on the same acquired session that owns incoming forwarded channels. Accepted owners are transport-owned; policy is freshly resolved per invocation, not a pool-key dimension.
- `ForwardWarning { remote_port, local_port, reason, failed_count }` contains one first-failure route/reason plus aggregate count, not an unbounded error vector.
- Queue genuine Ready before the independent warning on one ordered pane channel. Ready remains Ready; closed/invalid panes do not acquire someone else's warning.
- New warning text uses typed IDs and named placeholders via shared presentation formatting. Never send it to remote stdin or mark a refused tunnel active.

## 4. Validation & Error Matrix

| Result | Config unset/no | Config explicit yes | Manual forwarding |
| --- | --- | --- | --- |
| Accepted | retain/reuse owner | retain/reuse owner | return owner |
| russh RequestDenied | coalesced visible warning; continue shell | contextual startup failure | error |
| Existing remote-port route conflict | warn; preserve incumbent | fail this startup; preserve incumbent | conflict error |
| Disconnect/send/I/O/internal route failure | fatal | fatal | error |
| Invalid config/policy/endpoint | resolution failure | resolution failure | resolution failure |

Rejected requests clear Pending and create no owner. Denial cannot be cached as success. Failure of one strict pane must not disconnect another pane's healthy shared transport.

Cleanup-lock corruption after the async request remains fatal even when the server returned RequestDenied; never silently skip cleanup and emit an optional warning. Conflict includes Pending reservations, so diagnostics/translated reasons say reserved rather than asserting active forwarding.

## 5. Good / Base / Bad Cases

- Good: accepted listener is reused by another pane on the same transport, with no duplicate global request.
- Base: optional refused listener leaves routes/owners empty and the accepted shell receives user bytes; warning truthfully states unavailable forwarding.
- Bad: socket disconnect is converted into warning-only Ready, or a conflict overwrites the active route.

## 6. Tests Required

- Loopback raw refusal asserts pinned RequestDenied/string, then optional setup still opens usable PTY/shell.
- Strict refusal, manual refusal, broken transport/internal poison, pending cleanup, accepted owner reuse, real forwarded bytes, incumbent conflicts, retry and bounded-summary counts.
- Deterministically poll setup to Pending, corrupt the route lock before a refused reply resumes, and assert fatal cleanup error rather than ForwardWarning. Assert a Pending conflict preserves its reservation and never describes the route as active.
- Expanded/fallback unset/no/yes, invalid values, first-value-wins and offline system OpenSSH fixture.
- Ordered pane warning delivery, true Ready state, rendered visible warning and closed/invalid-pane suppression.
- Current-source checks and architecture ratchet; cached binary tests are not current-source proof. Native package/user-host verification is separately recorded.

## 7. Wrong vs Correct

Wrong: propagate every configured-forward error before shell, or catch every `SessionError` and continue.

Correct: classify concrete `ForwardError`; only an optional reason under `!resolved.exit_on_forward_failure` contributes a warning. Every other error remains fatal. Manual operations return errors regardless of config's optional startup policy.

The confirmed contract is SSH-only. Chinese-input hypotheses or diagnostics are outside this task and must not be presented as repaired.
