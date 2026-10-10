# Deterministic remote ownership validation before publication

## Status

Recovery follow-up; publication still requires all existing gates.

## Context

The source-only release recovery retained immutable tags and full native tests.
The first corrected version exposed a further scheduling issue in a test fixture.

## Evidence

Run [38028310952](https://github.com/Attiv/pebrel/actions/runs/38028310952), at
`81ff32d3e19f57edc289db9570232dec7082c737`, passed Linux and both Windows native
suites. On macOS ARM, the unsupported remote close regression started a real SSH
worker whose completion woke GPUI from outside its deterministic test scheduler.
The local seven-test tab undo group passes with the corrected fixture.

## Decision

Use a test-only remote identity constructor with a scheduler-controlled local
load. Retain the actual remote document source identity, assert that identity,
and preserve both close-history and subsequent undo assertions. Production remote
loading remains unchanged. Preserve v2.2.2 and build the correction as v2.2.3.

## Rejected alternatives

- Retrying until the SSH timing race disappears: leaves a nondeterministic fixture.
- Removing the remote case or skipping native validation: weakens ownership coverage.
- Moving the tag or reusing old binaries: breaks the release source/evidence identity.

## Consequences

All product packages must be freshly built for another patch version. No production
threading, publication dependency or platform requirement changes.

## Validation

The seven local tab undo regressions pass, including remote and merge cases.
Hosted native tests and final public asset/hash verification remain required.

## Supersedes

None; follows [the initial recovery decision](2026-10-10-source-only-tag-recovery.md).

## Revisit when

GPUI provides a supported deterministic SSH transport fixture for ownership tests.
