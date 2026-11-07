# Fork 2.5.0 Windows/macOS publication scope

## Status

Approved by the fork owner on 2026-11-07; implemented in the version-scoped manifest.

## Context

The owner requested publication of the local command AI feature as 2.5.0 and
explicitly retained the previous Windows/macOS-only release scope.

## Evidence

- [Stable manifest](../../../scripts/stable_release.py) owns the asset set.
- [Native planner](../../../scripts/ci_plan.py) derives release hosts from it.
- [Stable workflow](../../../.github/workflows/release.yml) derives package
  inclusion and retains same-source native, freshness and conformance gates.
- Before the manifest change, the extended release/planner tests failed for
  2.5.0 because five unrequested Linux/Android/relay assets remained selected.

## Decision

Add 2.5.0 to the existing explicit Windows/macOS version set. Keep the immutable
2.4.0 manifest and all other historical/future manifests unchanged. Publish six
products plus SHA256SUMS; retain both native architectures on Windows and macOS.

## Rejected alternatives

- Reusing the published 2.4.0 tag would break source/binary reproducibility.
- Building Linux/Android or copying a new workflow would exceed the approved scope.
- Weakening tests or reusing stale packages would remove existing release gates.
- A generic configurable platform registry would duplicate existing authority.

## Consequences

Linux/Android products are not newly validated by this release. macOS stays
Preview/ad-hoc, without a notarization or interactive installation claim.
Checked-in notes keep supported pending hashes; publication renders fresh hashes.

## Validation

Extended manifest tests require the exact six products, reject missing/extra
assets and retain ordinary future manifests. Planner tests cover push, dispatch
and reusable release calls for both scoped versions. Hosted release success and
public asset verification are separate from local tests and trigger acceptance.

## Supersedes

Extends [2.4.0 scope](2026-11-03-fork-windows-macos-release-scope.md) for 2.5.0 only;
does not replace its rationale or ordinary full-platform CI.

## Revisit when

A later fork release explicitly changes its platform scope.
