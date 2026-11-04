# Fork 2.4.0 Windows/macOS publication scope

## Status

Implemented for the explicitly approved Attiv/pebrel 2.4.0 release scope.

## Context

The fork owner requested latest upstream source plus existing fork behavior on
`develop`, matching 2.4.0 metadata and publication for Windows/macOS only. The
ordinary stable pipeline previously required Linux and Android packages even
when those products were not requested. Upstream's one-off 2.4.0 workflow has a
separate no-regression-suite policy; that exception does not apply to this fork.

## Evidence

- [Stable manifest](../../../scripts/stable_release.py) already owns immutable
  historical asset contracts and version-scoped exceptions.
- [Stable workflow](../../../.github/workflows/release.yml) gates publication on
  same-source native suites and complete package/conformance validation.
- [Native planner](../../../scripts/ci_plan.py) owns host matrices and required
  reports, while [evidence validation](../../../scripts/preview_release.py) checks
  platform/source identity and installed macOS LaunchServices receipts.
- The user's approved scope contains Windows x64/ARM64 ZIPs/installers and both
  macOS Preview DMGs, not Linux/Android/relay products.

## Decision

- Add only the fork 2.4.0 manifest exception; all other versions retain their
  existing sets. Derive package inclusion and release native hosts from that
  authority instead of adding an independently maintained platform registry.
- The stable prepare job validates the version before passing it to the reusable
  native workflow. Ordinary calls remain five-host; the 2.4.0 release selects
  both Windows and both macOS hosts. Reject release narrowing for PR, scheduled,
  merge-group and mismatched push-tag events.
- Excluded Linux/Android jobs must be skipped. Every selected job must succeed;
  failures, cancellation, selected skips, invalid flags and unexpected excluded
  execution block collection. Existing required report names remain, with Linux
  explicitly reported as not executed for this release.
- Exclude Linux conformance only explicitly, retaining default full-platform
  validation for ordinary Preview calls. Both Windows architectures, both macOS
  conformance reports and installed launch receipts remain mandatory and bound
  to the same source commit.
- Publish exactly the validated manifest paths; never rely on nonexistent Linux
  globs, use stale binaries or fabricate passing evidence. Keep freshness,
  signature, version, tag immutability and public asset checks unchanged.
- Source notes use supported pending digests until fresh fork packages are built;
  public notes receive actual digests through the existing renderer. Upstream
  binary hashes cannot describe rebuilt customized packages.

## Rejected alternatives

- Uploading upstream binaries would erase customization and misstate provenance.
- Skipping regression/conformance suites or weakening existing failure gates
  would adopt an unapproved validation waiver.
- Requiring unrequested Linux/Android builds would violate the release scope.
- A generic future platform-config system or copied workflow would duplicate the
  existing manifest authority without a requested use case.

## Consequences

Only Windows/macOS product builds and assets are selected for this release;
Ubuntu lint/orchestration is not a Linux product build. Linux/Android behavior is
not newly validated. Both macOS assets retain Preview/ad-hoc distribution; this
policy does not claim notarization, physical installation or visual acceptance.
Future releases use their existing full manifest unless separately reviewed.

## Validation

Release helper tests cover exact six-asset membership, missing/extra packages,
historical/future manifests, optional-job status/flag failures and exact upload
paths. Native-planner tests cover four hosts, ordinary five hosts, unchanged
required-report names and forbidden narrowing. Evidence tests retain same-source,
both-architecture and installed-launch positive/negative cases. Hosted test and
package results must be recorded by the release run; this note does not invent
successful hosted execution.

## Supersedes

None. This does not change the upstream one-off release policy or ordinary CI.

## Revisit when

A later fork release explicitly changes its platform scope or the manifest
contract is replaced by a reviewed release policy.
