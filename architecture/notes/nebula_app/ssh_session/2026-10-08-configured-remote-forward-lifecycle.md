# SSH config RemoteForward lifecycle

## Status

Implemented on 2026-10-08.

## Context

SSH pane connections resolved `~/.ssh/config` for identity and endpoint but did not activate configured `RemoteForward` directives. Existing forwarding routes belong to the authenticated SSH transport, which is pooled across panes.

## Evidence

- `nebula_app/src/ssh_session.rs` parses `ssh -G` output and owns the connection pool.
- `nebula_app/src/ssh_session/config.rs` is the conservative fallback parser.
- `nebula_app/src/ssh_session/forward.rs` registers routes on the session receiving forwarded-tcpip channels.
- `nebula_app/src/ssh_session/lifecycle.rs` opens pane shells after session acquisition.

## Decision

Resolve loopback TCP `RemoteForward` entries from both config paths and activate each on the acquired pooled session before opening the pane shell. Retain each forwarding handle with the pooled session so pane teardown does not cancel forwarding needed by another pane. Key active handles by remote and local ports to avoid duplicate requests on reuse.

## Rejected alternatives

- Starting a separate SSH transport would split forwarded-tcpip channel routing from the terminal's authenticated connection.
- Storing handles in a pane would make the first pane to close cancel forwarding for other panes sharing the connection.
- Accepting non-loopback bind or destination addresses would exceed the forwarding implementation's loopback-only behavior.

## Consequences

Configured forwards are active for the lifetime of the pooled SSH connection. Unsupported endpoint forms fail config resolution instead of appearing to work without a route. The existing manual forwarding UI continues using the same route table and handle lifecycle.

## Validation

Parser tests cover repeated `ssh -G` entries, fallback config entries, and invalid ports or non-loopback endpoints. Run targeted SSH tests, formatting, and diff checks.

## Supersedes

None.

## Revisit when

Revisit ownership if configured forwards must survive transport replacement, support dynamic remote ports, or bind beyond loopback.
