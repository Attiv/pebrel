# Fork 2.5.1 ARM macOS and Windows release scope

## Status

Approved by the fork owner on 2026-11-08; implemented as a 2.5.1-only scope override.

## Context

After the Intel packaged boot blocker, the owner requested a fresh local macOS
ARM package first and then a 2.5.1 release without macOS Intel. Windows x64 and
ARM64 remain included. This is an explicit product-scope change, not a waiver of
any retained platform's readiness or conformance requirements.

## Evidence

- The [asset manifest](../../../scripts/stable_release.py) previously selected
  both macOS products for 2.5.1, while the stable workflow had a fixed two-runner
  macOS matrix and aggregate evidence required both LaunchServices receipts.
- New manifest/planner/workflow/evidence regressions failed against that state:
  removing an Intel build alone would still leave native or aggregate obligations.
- [Release planning](../../../scripts/ci_plan.py), the
  [stable workflow](../../../.github/workflows/release.yml) and
  [evidence validator](../../../scripts/preview_release.py) consume the manifest's
  selected macOS architectures after this change.

## Decision

Exclude only the 2.5.1 macOS x64 DMG from the authoritative manifest. Derive the
macOS package matrix from that manifest and reuse its runner/architecture mapping
for native planning and aggregate receipts. The release contains five products
plus SHA256SUMS: one ARM macOS DMG and Windows x64/ARM64 ZIPs and installers.

The evidence validator's optional architecture scope defaults to both macOS
architectures, preserving ordinary and historical callers. Every selected runtime
and installed-copy LaunchServices receipt remains mandatory for the same source.
Invalid or empty architecture scopes fail rather than disabling verification.

## Rejected alternatives

- Skipping Intel steps inside an allocated job leaves misleading native/matrix
  and aggregate obligations instead of implementing the approved scope.
- Merely removing its DMG upload leaves the aggregate expecting an Intel receipt.
- Removing the validator's Intel default would weaken unrelated preview callers.
- A new platform flag registry would duplicate the existing manifest authority.
- Ignoring retained failures or reusing stale products would bypass release gates.

## Consequences

Only 2.5.1 omits macOS Intel; older release manifests and ordinary full native CI
keep their original matrices. Linux/Android/relay remain outside this release.
macOS ARM remains Preview/ad-hoc, without a notarization or installation claim.
The owner-requested local ARM build and hosted release artifacts are separate
fresh builds; this scope decision does not itself prove either build succeeded.

## Validation

Tests assert the exact five products, manifest-derived single macOS package row,
three selected native hosts, rejection of an extra Intel DMG and missing retained
assets. Aggregate tests pass without excluded Intel receipts but fail when any
retained Windows/macOS runtime or ARM LaunchServices receipt is missing or has a
wrong source. Old default evidence still requires both macOS architectures.

## Supersedes

[The earlier 2.5.1 dual-macOS scope](2026-11-08-fork-251-windows-macos-release-scope.md)
only. Published 2.5.0 and older manifests/tags and ordinary full CI are unchanged.

## Revisit when

A subsequent fork release explicitly restores Intel or changes its platform scope.
