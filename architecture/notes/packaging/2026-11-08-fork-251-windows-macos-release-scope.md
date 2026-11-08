# Fork 2.5.1 Windows/macOS publication scope

## Status

Approved by the fork owner on 2026-11-08; implemented in the version-scoped manifest.

## Context

The owner requested CI for 2.5.1 after approving coexistence of native zsh history
suggestions and AI command completion. This patch retains the preceding fork
release's Windows/macOS-only product scope.

## Evidence

- [Stable manifest](../../../scripts/stable_release.py) owns the exact asset set.
- [Native planner](../../../scripts/ci_plan.py) derives release hosts from it.
- [Stable workflow](../../../.github/workflows/release.yml) retains same-source
  native, packaging freshness, aggregate conformance and publication gates.
- The extended 2.5.1 manifest/planner tests failed on the prior manifest because
  Linux/Android/relay products and the Linux native host remained selected.

## Decision

Add only 2.5.1 to the existing explicit Windows/macOS version set. Publish six
products plus SHA256SUMS. Keep both native architectures on Windows and macOS,
all earlier version manifests, and ordinary full-platform CI unchanged.

## Rejected alternatives

- Moving the public 2.5.0 tag would break source/binary reproducibility.
- Narrowing every future version would exceed this release-specific approval.
- A new workflow, stale artifacts or waived native checks would bypass the
  established same-source publication transaction.

## Consequences

Linux/Android/relay products are outside this release. macOS remains
Preview/ad-hoc without notarization or interactive installation claims.
Source notes and Changelog retain supported pending hashes; publication renders
fresh product hashes rather than modifying a published version's source notes.

## Validation

Manifest tests cover the exact six products and reject missing/extra assets.
Planner tests cover matching tag, dispatch and reusable release callers for
2.5.1; ordinary versions retain their existing full matrices. Hosted success and
public asset verification are separate from local preparation or CI triggering.

## Supersedes

Extends the [2.5.0 scope](2026-11-07-fork-250-windows-macos-release-scope.md)
for 2.5.1 only; does not replace its rationale or ordinary full-platform CI.

## Revisit when

A later fork release explicitly changes its platform scope.
