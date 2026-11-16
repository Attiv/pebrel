# Interface motion preference and interruption

## Status

Implemented; macOS build, control/state regressions and architecture checks pass.
Native visual acceptance remains separate from the rendered-control tests.

## Context

The persistent `animations` preference already existed, but sidebar/detail
transitions, settings capsules, focus underlines and the theme reveal did not all
consult it. Normal keyboard work could consequently trigger layout transitions,
and disabling effects while closing a panel could retain an invisible layout slot.
The settings-page entry animation also restarted from zero on rapid navigation.

## Evidence

- Regression tests in `gpui_shell/config.rs` and `workspace/details_panel_tests.rs`
  failed with the interface preference off: workspace motion remained enabled and
  the closing details slot still existed.
- `gpui_shell/motion.rs` tests exercise the real shared `Tween` with controlled
  frame deltas: interruption preserves displayed opacity, disabled motion settles,
  and the exact CSS curve is verified at a known parametric point.
- GPUI's current `App::reduce_motion` is a host flag, initially false in the pinned
  runtime; application-owned effects additionally need native preference sampling.
- Existing Agents/Backup centering tests rejected an added block wrapper. The
  fade now supplies opacity to the original layout node, preserving flex ownership.
- The native APIs are read-only: [macOS NSWorkspace Reduce Motion](https://developer.apple.com/documentation/appkit/nsworkspace/accessibilitydisplayshouldreducemotion)
  and [Windows client-area animations](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-systemparametersinfow).

## Decision

- Keep the existing settings authority and saved key/default/reset behavior. Views
  read cached preferences; the normal tab-motion predicate includes this gate.
- Cache native Reduce Motion at startup and refresh when a workspace regains
  activation. The window-owned GPUI subscription detaches with its owner. Linux
  retains GPUI's host preference; this change introduces no desktop portal service.
- Do not rewrite the host accessibility flag with the user's preference. This
  gate controls application-owned interface effects; independently configured
  video/shader playback and the component library's existing dialog internals retain
  their owners and controls.
- Keyboard-driven transitions are immediate. Disabling motion settles/removes
  closing slots and stale fold state, rather than merely stopping paint frames.
- Occasional pointer-driven settings/details content uses the existing shared
  `Tween`, opacity only, 180ms, cubic-bezier(0.23, 1, 0.32, 1). Reduced motion uses
  the shared shorter 120ms policy without movement. No wrapper or previous page
  is retained; opacity must not change the control layout or hit regions.
- Switch state feedback crossfades two static endpoints for 160ms, retaining the
  displayed blend when interrupted. Changed-setting rails are static; they do not
  animate functional layout. Existing mouse panel layout transitions are preserved
  under their existing tab-animation setting and the additional global gate.

## Rejected alternatives

- A motion library or another preference duplicates existing ownership and adds
  a production dependency without a concrete need.
- Retrying keyed zero-opacity entry animations on every navigation causes flashes
  and cannot carry the current displayed state through interruption.
- Per-frame native preference queries and global accessibility-flag toggling add
  hot-path work, refresh loops, or conflate the user's setting with the OS setting.
- Animating terminal data, keyboard shortcuts, or permanent setting-row geometry
  delays frequent work and moves content users are reading.

## Consequences

No additional thread, task, cached page, asset, or persistence format is introduced.
Native preference changes are sampled on startup/refocus, not subscribed while a
window remains continuously focused. Existing component-layer and background
animations are not presented as newly controlled effects.

## Validation

Focused real-control tests cover pointer navigation, keyboard search, keyboard
panel closing, the visible persistent switch and disabling an active close. Pure
motion tests cover interruption, settle, reduced duration and easing. Full native
and architecture checks are recorded by the integration work; these tests are not
a claim of native visual acceptance or remote-server end-to-end access.

## Supersedes

None.

## Revisit when

The pinned GPUI/component libraries expose a native, observable motion policy;
live accessibility changes without refocusing become a requirement; or existing
layout transitions are replaced through a separately validated UI change.
