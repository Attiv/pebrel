# Window geometry survives combined and empty sessions

## Status

Implemented; validation is listed below. Native desktop acceptance is separate.

## Context

The GPUI window tracked its last normal logical rectangle, but ordinary startup
did not reliably recover it. Users also expect placement to survive deliberately
closing every terminal tab and turning off terminal-session restoration.

## Evidence

- [Window combination](../../../../nebula_app/src/session/window_layout.rs)
  previously stored geometry only inside `window_layout`, while ordinary startup
  read `Session.window`.
- Startup gated that read on both the restore-tabs preference and
  `session::should_restore`, which deliberately rejects empty terminal sessions.
- Empty-window removal made a fresh session without the closing window's bounds.
- Bounds observers are asynchronous; a close immediately after a native resize
  can run before the observer updates the cached normal rectangle.

## Decision

Keep geometry in the existing atomic v4 session document. Combined snapshots
expose the active window's geometry at the compatible top level, falling back to
the first known window when the active one has none. Loading older combined
documents promotes their active boundary's geometry only when the top-level
field is absent. Window boundaries and terminal-tab semantics remain unchanged.

Restore valid ordinary-window placement independently of whether terminal tabs
are restored. An explicit empty final session still means no restored tabs, but
it retains its last geometry. Capture actual native bounds before close/quit
snapshots, retaining the last normal rectangle during maximization/fullscreen
and excluding Quick Terminal from ordinary placement ownership.

Empty checkpoints still cannot erase the retained terminal snapshot. At quit,
an actual constructed window's geometry may be saved even with no terminal tabs.
If a previous snapshot exists, merge only its new top-level rectangle while
retaining the previous tabs and window boundaries; assigning the new rectangle
to old window identities would invent ownership. An explicitly cleared snapshot
stays empty, and the first successful quit still freezes subsequent teardown writes.

The GPUI geometry adapter owns display matching, bounds fitting and native
capture. Select the current display with the greatest useful saved-rectangle
intersection; otherwise use the primary display and center offscreen bounds.
Reject zero or implausibly large saved dimensions before window construction.
The existing startup-grid adapter continues to size new windows from the base
font; restored windows instead derive the first PTY grid from their actual bounds.

## Rejected alternatives

- A separate settings file or geometry service would fork persistence ownership
  and break the atomic relationship with window boundaries.
- Applying the tab crash-loop breaker to geometry would keep discarding placement
  even though no terminal or provider process is involved in restoring bounds.
- Always clamping onto the primary display loses a valid secondary-display origin.
- Treating fullscreen/maximized native size as the normal rectangle destroys the
  user's eventual restore size; Quick Terminal has its own monitor/size policy.

## Consequences

Older readers continue to see compatible flat tabs and the optional window field.
No schema version, dependency, background service or OS-position API is added.
Disconnected displays and corrupt sizes fall back to reachable geometry rather
than preventing startup. Geometry is still quantized to logical integer pixels.

## Validation

- Session regressions verify active geometry, legacy nested fallback, explicit
  top-level authority, empty geometry and unchanged multiwindow boundaries.
- GPUI regressions resize then close before observer delivery, including an empty
  final window. Geometry tests cover secondary origins, offscreen fitting, invalid
  sizes and normal/maximized/fullscreen/Quick Terminal ownership.
- Quit regressions cover a first settings-only window, an older retained snapshot,
  explicit tab clearing, unchanged window boundaries and frozen repeated quit.
  A real cold-start fixture disables terminal restoration but restores placement;
  these fixtures share the settings/session storage serialization guard. All
  window-transfer fixtures complete GPUI quit before restoring the original
  session bytes and releasing the lock, including the macro's later teardown.
- First-frame sizing tests retain the base-font grid and device-pixel tolerance.
- Native macOS placement across real display changes, maximize/fullscreen return
  and restart with restored PTYs still require desktop acceptance; automated
  state tests do not claim that visual/platform verification.

## Supersedes

None. Extends the existing session format and preserves
[first-frame geometry](../gpui_shell/workspace/2026-09-22-first-frame-startup.md).

## Revisit when

GPUI exposes portable display identity/DPI restoration, or the session storage
and ordinary-window lifecycle no longer share one atomic snapshot owner.
