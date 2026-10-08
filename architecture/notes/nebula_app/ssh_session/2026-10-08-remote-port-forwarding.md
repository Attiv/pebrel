# SSH remote port forwarding

## Status

Reviewed for implementation on 2026-10-08.

## Context

The SSH port-forward dialog supports both local forwarding and remote forwarding. Remote forwarding asks the SSH server to listen on a loopback port; each forwarded-tcpip channel then needs a local destination in the client process. The authenticated SSH transport remains shared with the terminal and other operations.

## Evidence

- `nebula_app/src/ssh_session/forward.rs` owns forward setup and teardown.
- `nebula_app/src/ssh_session.rs` owns the authenticated client handler and pooled transport.
- `nebula_app/src/gpui_shell/codex_workspace/94ed571e-dbac-4d4a-9f54-abcdd842203f/ssh_dialog.rs` owns direction selection and per-forward controls.
- A route must not become usable until the server accepts the forwarding request; parallel creation for the same remote port must not replace the winning destination.

## Decision

Keep the remote-port-to-local-port route table alongside the pooled SSH session because incoming forwarded-tcpip channels are delivered to that session's handler. Reserve each requested port as pending before sending the SSH request, activate the route only after acceptance, and reject channels for pending or absent routes. Dropping the forward removes its route and requests server-side cancellation while retaining the shared SSH session.

## Rejected alternatives

- A second SSH process would duplicate authentication and connection ownership.
- A global route table would disconnect channel routing from the session that receives forwarded-tcpip channels.
- Publishing the destination before server acceptance permits uncommitted routes to receive traffic and concurrent setup failures to corrupt another route.

## Consequences

Remote forwards share connection pooling but have an explicit direction and lifecycle. The route table is ephemeral and scoped to its SSH transport. New direction and completion labels use typed catalog identifiers.

## Validation

Run the targeted SSH forwarding tests and the application formatting/build checks. UI click-path and native-language visual acceptance remain separate from protocol tests.

## Supersedes

None.

## Revisit when

Revisit ownership if remote forwards become persistent across transport replacement, require dynamic port allocation, or need a destination other than loopback.
