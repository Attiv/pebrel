# OpenSSH RemoteForward endpoint format

## Status

Implemented on 2026-10-08.

## Context

Configured loopback TCP forwarding was implemented, but an SSH pane could still
fail to resolve a configuration that worked with the system SSH client. The parser
tests used handwritten output with bare hosts instead of OpenSSH's rendered form.

## Evidence

- An offline `ssh -G -F` fixture renders IPv4 addresses and hostnames as
  `[127.0.0.1]:9000` and `[localhost]:3000` in `remoteforward` lines.
- `parse_remote_forward` accepted only bare `127.0.0.1` and `localhost`. Its
  rejection made `parse_resolved_config` discard otherwise successful expansion.
- Resolution then entered the conservative raw-config fallback, where an `Include`
  directive is intentionally rejected. The reported fallback failure hid the
  actual incompatibility at the expanded-output boundary.
- The new literal-output and real offline OpenSSH regressions both failed before
  the parser fix; the existing bare-host fixture did not detect the defect.

## Decision

Accept the exact single-bracket forms of the two already-supported loopback hosts
in the shared forwarding parser. Keep OpenSSH authoritative for `Include` and
other advanced configuration, and retain the existing transport and owner model.

## Rejected alternatives

- Stripping arbitrary bracket characters would also accept malformed or nested
  endpoints that OpenSSH does not render.
- Extending the fallback to expand `Include` would duplicate system-SSH behavior
  without repairing the original parsing failure.
- Broadening destination or bind addresses is unrelated to this format repair.

## Consequences

Both expanded and raw configuration paths recognize the same loopback endpoints.
Non-loopback hosts, IPv6, Unix sockets, malformed brackets and invalid ports remain
unsupported. No credentials, persisted format or session lifetime changes.

## Validation

`ssh_session::tests` covers real bracketed output, rejection boundaries and an
offline `ssh -G` fixture using `Include`. Pure fixtures run without OpenSSH; the
system integration case explicitly reports a skip if the executable is absent.
Run `cargo test --locked -p nebula --bin pebrel --features gpui-shell ssh_session::`.
These checks do not connect to a user's server or claim native end-to-end access.

## Supersedes

None. This supplements the configured-remote-forward lifecycle decision.

## Revisit when

Revisit endpoint representation if forwarding gains IPv6, Unix sockets, arbitrary
bind addresses or a new system-SSH output form.
