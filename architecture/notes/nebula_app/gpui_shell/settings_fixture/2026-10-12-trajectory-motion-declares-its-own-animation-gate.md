# Trajectory motion tests declare their own animation gate

## Status

Implemented; the hostile-input reproduction and the whole native binary suite pass
locally. Native platform jobs are still pending.

## Context

The cursor trajectory cases in
[`cursor/tests.rs`](../../../../../nebula_app/src/gpui_shell/terminal/view/cursor/tests.rs)
assert interpolated geometry, and
[`details_panel::tests::switching_to_instant_removes_closed_panel_geometry`](../../../../../nebula_app/src/gpui_shell/workspace/details_panel_tests.rs)
asserts that a closing panel stays armed. Both take their `Settings` from the
module helper `open()`, which installs `Settings::load(...)` read from the real
settings path. `visual_cursor` decides whether to animate through
`animations_enabled(cx)`, which merges `SystemMotion` with `Settings::animations`.

The settings switch fixtures persist `animations=0` into that same file for as long
as their `SettingsBytesGuard` lives. `.config/nextest.toml` serializes those writers
against each other in the `theme-studio` group, but every other process keeps
reading the file while a writer holds it — the boundary
[the 2026-10-02 note](2026-10-02-font-controls-nextest-isolation.md) already
documents. The trajectory cases pinned `cursor_motion` and left `animations` to
whatever bytes happened to be on disk, so their result depended on scheduling
rather than on the behaviour under test.

## Evidence

The `v2.2.4` stable release run failed two of them on `ubuntu-24.04`
(`cursor/tests.rs:44:13` and `cursor/tests.rs:209:13`) while `windows-2022`,
`windows-11-arm`, `macos-26` and `macos-26-intel` passed the same cases.

Pointing `PEBREL_CONFIG_DIR` at a directory whose `pebrel_settings.txt` contains
`animations=0` reproduces it deterministically:

- Before the change, 6 trajectory cases fail plus
  `switching_to_instant_removes_closed_panel_geometry`, reporting the two CI panic
  locations and the same `left: Some(Position { col: 4.0, row: 0.0 })`.
- With `animations=1` in the same directory, all of them pass — the file byte is
  the only input that changed.
- After the change, `animations=0` with
  `-p nebula --bin pebrel --features gpui-test-support`: 2313 passed, 0 failed.

## Decision

The trajectory helper `smooth_motion` installs `cursor_motion = Smooth` **and**
`animations = true` on the in-memory snapshot, and the details panel case pins
`animations` on the settings it installs. Both reuse the pinning idiom the two
neighbouring motion cases in `details_panel_tests` already use.

## Rejected alternatives

- Joining the `theme-studio` mutex and nextest group: that contract exists for
  fixtures whose assertion *is* a persisted value, where a before/after file
  snapshot must not be interleaved. These assertions are about interpolated
  geometry, so their dependence on the file is incidental; serializing them would
  satisfy the letter of the contract and keep the accidental input.
- Removing or loosening the trajectory assertions, or enabling nextest retries:
  hides the interference instead of removing the shared input.
- Changing `visual_cursor`'s gates or the `Settings::animations` precedence:
  turning animations off must keep snapping the cursor. The product behaviour is
  intended and the test asks for the animated path explicitly.

## Consequences

Animation-preference-dependent cases now declare the gate they assert on instead of
observing the file. The settings file stays shared: a test whose assertion is a
persisted value must still join both halves of the fixture contract documented in
[`settings_fixture`](../../../../../nebula_app/src/gpui_shell/settings_fixture.rs).

## Validation

`PEBREL_CONFIG_DIR=<dir with animations=0> cargo test --config .github/ci-profile.toml
--profile ci -p nebula --bin pebrel --features gpui-test-support` passes, and the
same command with `animations=1` or without the override is unchanged.
`rustfmt --check` is clean for both edited files. Native platform jobs must still
confirm the unchanged cases on the real runners.

## Supersedes

None.

## Revisit when

Settings fixtures receive an independent configuration directory, or `open()` stops
loading the real settings file.
