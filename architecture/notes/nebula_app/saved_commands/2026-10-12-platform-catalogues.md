# Built-in platform command catalogues

## Status

Implemented; local verification passed. Maintainer review pending.

## Context

The command manager previously selected a small development-recipe list from the
focused session's host platform. The user requested richer Linux commands, then
explicitly chose separate Linux, macOS and Windows groups with all catalogues
bundled. Session-based hiding no longer serves that workflow, including users
preparing commands for another machine.

## Evidence

- [Catalogue](../../../../nebula_app/src/saved_commands/builtins.rs) previously
  contained host/remote conditionals and a test requiring <=13 recipes per platform.
- [Organization](../../../../nebula_app/src/saved_commands/groups.rs) already
  distinguishes absent membership (default), explicit null (ungrouped), and an
  explicit group override in the version-1 command store.
- [GPUI manager](../../../../nebula_app/src/gpui_shell/workspace/command_manager.rs)
  already supplies folder-scoped/global search and a virtualized list; a new
  renderer or hierarchy is unnecessary.
- User approval in the current task requires at least 60 Linux, 40 macOS and
  40 Windows platform templates, with the original common recipes retained.

## Decision

The saved_commands capability owns one static typed catalogue and one system-group
registry. Linux, macOS and Windows recipe modules have their actual platform
responsibility; shared recipes occur once in the original builtin group.
The GPUI adapter consumes all groups on every host rather than choosing a platform.

Each recipe retains a stable builtin ID, typed localized purpose label and editable
single-line shell text. All templates use append_enter=false. Windows group naming
states PowerShell, and distro/third-party-tool requirements appear in recipe labels.

Default membership comes from static catalogue metadata, never from parsing command
strings in the view. Explicit group and null overrides take precedence. Unknown
historical builtin IDs retain the legacy builtin fallback. Domain validation and
UI delete eligibility share the same protected-system-group query. Protected-group
delete requests are rejected before taking a file lock or preparing directories;
an immutable registry rejection must not create storage or depend on a competing
writer.

System groups are virtual: merely browsing does not seed custom groups, migrate a
file, or consume the 40-command/40-custom-group capacity. User changes retain the
existing lock, reload and atomic-write transaction. Existing IDs, hidden records
and the version-1 field shape are preserved.

## Rejected alternatives

- **Continue hiding by current session:** conflicts with the explicitly requested
  all-platform catalogue and still obscures preparation for another machine.
- **Nested platform/subcategory tree:** adds new navigation and persistence
  contracts although existing flat folders and label-based search meet the need.
- **Three copies of common development commands:** introduces duplicate global
  results and inconsistent hidden/organization state for the same command.
- **Auto-seed platform groups into the user's store:** rewrites preferences just
  by viewing and mixes application-owned definitions with custom capacity.
- **Raise the old small-catalogue cap:** would retain an obsolete product assumption.
  Coverage and invariants now test completeness, unique IDs, platform ownership,
  nonempty single-line text and review-before-execution, with invalid cases.

## Consequences

- All templates can be inspected on any host; this is not a guarantee that the
  current shell or machine has the matching tools. Users select the right group
  and edit parameters before submitting the inserted line.
- The original builtin folder and recipe IDs remain compatibility inputs. Only
  implicit platform-specific defaults change; explicit user organization wins.
- A new application loads old v1 files without automatic rewrite. An old binary
  does not recognize newly persisted platform-system-group membership references.
  For downgrade, restore a pre-change store backup or explicitly remove/reassign
  those references before returning to the older binary. Viewing alone introduces
  no persisted downgrade dependency. No broader store migration is added.
- Rendering keeps the existing virtual list; lookup of a default group uses static
  metadata, not a newly localized/allocated catalogue for each lookup.
- No new runtime dependency, subprocess, disk scan or asynchronous lifecycle is
  introduced by catalogue display. This is not a universal performance claim.

## Validation

- [Catalogue tests](../../../../nebula_app/src/saved_commands/builtins/tests.rs)
  preserve every existing recipe ID/text, enforce platform coverage, and reject
  duplicate, missing, unassigned and autoexecuting synthetic catalogues.
- Domain tests exercise explicit/null overrides, protected virtual groups,
  impersonation rejection, legacy files, hidden records, concurrent updates and
  unchanged custom-store capacity.
- [Real GPUI tests](../../../../nebula_app/src/gpui_shell/workspace/command_manager/group_tests.rs)
  cover folder-padding clicks, keyboard navigation/scrolling, cross-platform and
  scoped search, copy/edit-to-custom/hide controls, and store preservation.
- Final local results: domain 16/16, real GPUI control tests 10/10, independent
  and product i18n contracts each 24 passed with one existing manual benchmark
  ignored, file budgets 2/2; actual product type-check, fmt and architecture passed.
  Full commands and unavailable native/visual paths are recorded in the Trellis task. Test-support drawing/clipboard is not native visual acceptance
  or execution of every platform-specific shell command.

## Supersedes

None. The <=13 assertion was a local product-scope assumption, not an architecture
budget, dependency contract or persisted custom-command limit.

## Revisit when

A real nested-organization workflow, import/export catalogue format, default reset
operation, or old-binary downgrade requirement needs a new product contract.
Reconsider lookup cost only with representative evidence of a larger catalogue.
