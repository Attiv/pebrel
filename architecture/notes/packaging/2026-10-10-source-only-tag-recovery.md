# Recovering a tag that has no published product packages

## Status

Recovery decision; publication remains subject to the existing native-test and
package-conformance gates.

## Context

The fork's v2.2.1 tag exposed GitHub's generated source archives but no installable
Release assets. An uploaded Actions artifact and a public GitHub Release are
separate states; successful packaging does not mean a release was published.

## Evidence

On 2026-10-10 the fork's Releases API returned an empty list. Stable release run
[37950883110](https://github.com/Attiv/pebrel/actions/runs/37950883110), at
`d3a010b817c17119c58eaf09b7958e3f5d14953a`, successfully built all platform packages
and aggregated `Pebrel-v2.2.1`. Native tests failed on all five hosts, so the
verified-publication job was skipped by its existing dependency gate.

The failures included stale Agent identity used for badges, released Undo/Redo
keys suppressing text actions, selecting an active tab not returning focus, and
unsupported document closes omitted from undo history. Windows additionally
exposed a parser/UI protocol-restoration ordering race and persistence fixtures
that inherited the hosted runner's elevated identity. Ordinary persistence is
intentionally unavailable to isolated privileged windows; this is not a reason
to remove production isolation.

## Decision

Repair the failing behaviors and make ordinary-window fixtures select their
intended persistence scope explicitly. Keep the isolated-window negative tests.
Preserve v2.2.1; build the corrected source as v2.2.2 with synchronized version
metadata and bilingual release notes. Use the existing aggregate and native-test
gates before publication, and verify the actual public asset set afterward.

## Rejected alternatives

- Publishing the old artifact despite failed native tests: package conformance
  does not establish the keyboard and lifecycle contracts exercised by that suite.
- Skipping failed tests or weakening assertions: this would conceal real behavior
  regressions and confuse runner privilege with ordinary-window product behavior.
- Moving the old tag to the repair commit: this changes the identity of an already
  visible source version and breaks reproducibility.
- Creating a source-only Release before builds finish: generated archives are not
  substitute installers, and a public Release should represent verified packages.

## Consequences

The repaired packages have a new patch version. The old tag stays reproducible
and may continue to offer source archives without being an installable release.
No workflow dependency, platform requirement, or production privilege isolation
is weakened. Corrected packages must be rebuilt rather than renamed from the
previous run.

## Validation

The original failing regressions remain enabled. An additional cross-platform
protocol test proves already-restored input modes are acknowledged while newer
owners remain protected. A shortcut negative control ensures ordinary cleared
workspace actions do not reappear inside text controls. Hosted native validation
and final Release asset/hash checks are still required independently of local tests.

## Supersedes

None.

## Revisit when

The publication transaction, immutable-version policy, or privilege-isolation
contract changes with an explicit reviewed replacement and regression coverage.
