# GUI Agent discovery and login-shell paths

## Status

Implemented; locally verified on Apple Silicon with an isolated native GUI.

## Context

A GUI process can have a different PATH from its terminals. Claude Code and
Codex installed under a user npm prefix were reported absent, which correctly
disabled first-time Hook installation but made an existing installation unusable.
Adding subprocess discovery to the old per-Agent provider scan would also start
the user's shell up to 27 times during a single refresh.

## Evidence

- `nebula_app/src/ai_hook/integrations.rs` owns executable aliases, validation,
  installation inspection and the batch installed-Agent query.
- `nebula_app/src/platform/agent_path.rs` owns login-shell startup, bounded
  process output and PATH decoding. It reuses `platform::process_output`.
- `gpui_shell/settings_pane/providers/brand.rs` already discovers installations
  on a background executor; the Agents pane also inspects in the background.
- The original production discovery returned `None` for installed npm-prefix
  CLIs under a minimal PATH. Its regression failed before the repair.

## Decision

- Preserve existing PATH and fallback precedence, adding `.npm-global/bin`.
- Only unresolved Agents use the default interactive login shell's exported
  PATH. A fixed POSIX child prints a NUL-delimited record; startup banners are
  ignored, non-UTF-8 paths survive, and relative/duplicate directories are dropped.
- The platform adapter limits a probe to 2 seconds and 64 KiB, reaps its process
  group, and returns no extra paths on failure. Windows retains existing discovery.
- Batch inspection probes the shell at most once. Do not cache paths indefinitely
  or mutate the parent's environment; explicit refresh observes current installs.
- Keep executable validation and Hook availability/ownership checks unchanged.
  Detection does not authorize overwriting unrelated Agent configuration.

## Rejected alternatives

- Only adding fixed prefix/version directories: misses custom shell-managed paths.
- Parsing shell startup files: cannot reproduce evaluated version-manager setup.
- Changing global PATH: changes unrelated terminal/process environments.
- Running a shell per Agent or adding a permanent path cache: multiplies startup
  work or hides changes after a shell/installation update.
- Bypassing disabled Hook controls: conceals missing configuration or helper files.

## Consequences

Slow or unsuccessful startup scripts can leave only fallback installations visible.
This is bounded discovery, not a guarantee for every shell/plugin configuration.
No new dependency, persisted setting or background service is introduced.

## Validation

The platform regressions cover a minimal parent PATH, a real macOS zsh startup
file, prefixes with spaces, non-executable files, broken links, one-probe batching,
banner/byte-safe decoding, oversized output, failure and timeout. Domain tests
retain alias ownership and file-removal checks. Real GPUI control tests cover first
installation, helper-disabled state, keyboard input, in-flight clicks, write failure
and destruction. A fresh release GUI launched with a minimal PATH and isolated
home/config detected a Claude npm prefix and a Codex shell-only prefix. Both real
switches completed installation and displayed the installed state. This does not
establish other-host or end-to-end Agent event-delivery acceptance.

## Supersedes

None.

## Revisit when

Measured shell startup exceeds the probe bounds, a supported shell needs a
different startup contract, or callers require an owned executable/environment
snapshot rather than an installation-presence query.
