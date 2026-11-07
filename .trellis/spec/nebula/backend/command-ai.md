# Command AI Contracts

## 1. Scope / Trigger

Read when changing command-purpose inference, native AI input ownership, suffix scheduling, the shared request lane, provider invalidation or command settings. The application [ownership note](../../../../architecture/notes/nebula_app/ai_commands/2026-11-06-native-command-ai-ownership.md) records rationale; the [settings note](../../../../architecture/notes/nebula_settings/command_ai/2026-11-06-independent-command-selection.md) owns persistence consequences.

## 2. Signatures

`ai_commands` exposes application-private operations:

```rust
fn revision() -> u64;
fn invalidate_config();
fn prepare() -> Result<Arc<Prepared>, Failure>;
// Prepared methods:
fn search(&self, input: &str, os: &str, shell: &str, cancelled: &AtomicBool)
    -> Result<Option<SearchResult>, Failure>;
fn complete(&self, input: &str, os: &str, shell: &str, cancelled: &AtomicBool)
    -> Result<Option<String>, Failure>; // validated suffix, not a replacement prefix
// Application-owned pure prefix/local-result admission:
fn command_prediction_eligible(line: &str, cursor: usize, local_ready: bool, local_useful: bool)
    -> bool;
```

Prepare in background, cache successful snapshots by revision, and retry transient preparation failures without requiring a settings edit. Existing provider/credential/settings successful mutations invalidate the revision. Existing AI CLI message-prediction prompt/schema/turn policy stays in `prompt_suggestions`, not this capability.

## 3. Request, Response and Settings Contracts

- Wire context is constructed explicitly as `{"input":...,"os":...,"shell":...}`. OS is macos/linux/windows; Shell is the supported normalized local type. Fixed instructions, model routing and authentication are protocol metadata. Cwd/owner/prompt/query revisions/native line/caret are local identity, never automatically serialized.
- Search JSON has exactly `command: string|null` and `explanation: string`; completion JSON has exactly `command: string|null`. A completion full command must begin with the exact input and add a nonempty suffix. Preserve whitespace and quoting.
- Input/command bounds: 4096 UTF-8 bytes; explanation: 240 characters. Generated commands are single-line and control-free, including Unicode line separators. HTTP envelopes must be complete and unambiguous.
- Reuse prepared credentials, pooled existing transport, two process-wide inference slots, a five-second HTTP deadline, 64 KiB response body limit, no redirects and no automatic retries. Cancellation suppresses publication; already-started blocking HTTP may run to its deadline.
- One actual command-AI request per pane and at most one latest pending intent. Cancelling UI does not make an old actual request disappear from admission accounting.
- `CommandAi` owns `command_ai_provider_id`, `command_ai_model`, `command_ai_prediction_enabled`; old/invalid/partial configuration cannot enable prediction. Manual search is independent of the default-off automatic flag. Reset/backup preserve unrelated settings and existing Agent/message-model selections.
- Editor Observe is pure: no popup/accept action and no fallback Tab on timeout. Native owner/prompt/local execution are required. AI query editing, paste and IME never become PTY/broadcast input. Receiving a result never edits Shell input; explicit adoption rechecks the original native line/caret and appends no Enter.
- The AI panel has its own focus scope: query/result/Copy/Adopt share it; leaving it cancels. Reading/search input is mutually exclusive. Workspace focus must resolve the active query handle rather than blindly focusing the Shell ancestor.
- Automatic prediction requires opt-in plus normal local completion enabled, 500 ms idle, a verified local owner, no competing query/IME/foreground mode, a nonempty single-line end-caret prefix and completed local calculation without useful results. Local results arriving later still win. A paint cache hit may mark readiness but cannot start HTTP/preparation.
- Use the same actual lane for prediction and search; drain explicit search first. Prediction source labels are display-only. Tab/Right follows the current completion mode, fresh native verification precedes suffix insertion, and Escape suppresses unchanged input. Enter/Tab/Right while no AI candidate exists cancels pending publication before the original Shell/local action, retaining actual in-flight accounting.

## 4. Validation and Error Matrix

| Condition | Required result |
| --- | --- |
| Unconfigured/unsupported provider or editor | Unavailable/setup, no guessed input/provider fallback |
| Invalid/oversized contextual input | No HTTP request |
| Null command | Honest no-result, no Shell mutation |
| Unknown fields, refused/truncated/multiple envelope, control/multiline command | InvalidResponse, never input text |
| Exact nonempty suffix | Candidate only; preserve the user's prefix |
| Slot cap reached | Busy, no automatic retry |
| Query/owner/prompt/focus/cwd/config changes or view destruction | Cancel publication and reject adoption |
| Draft/caret changed before adoption | Recoverable stale state, no blind clear/append/restore |
| Cwd/config changes while final native Observe is pending, without render | Revalidate complete request identity at the reply; native line/caret equality alone is insufficient |
| Foreground/TUI or unbound shortcut | Preserve normal action propagation/terminal encoding; refusal is not permission to swallow the chord |
| Leaving AI for terminal/reading input | Cancel or keep a deliberately valid AI focus owner before any new text can route |

## 5. Good / Base / Bad Cases

- Good: explicit local query produces a validated command, native requery matches, Tab edits only, later Enter executes.
- Base: settings unset, prediction off, empty automatic prefix or unsupported context makes no model call; existing local completion/search remains usable.
- Bad: serializing a pane object, accepting against a screen/key mirror, sending query text to PTY, clearing a draft on mode entry, relabeling an AI candidate as local history, or starting another per-pane request while cancelled HTTP still runs.

## 6. Required Tests and Assertion Points

- Shared transport/command loopback tests: exact contextual field set/value, all protocols, bounds/null-vs-invalid/prefix, no redirect, cancellation, actual slot cap and retryable preparation failure.
- Preserve old `prompt_suggestions` prompt/client/service regressions and local command-library matching/persistence/run preferences.
- Real rendered controls: native draft preserved during query/IME/paste/cancel, exact Unicode/space-preserving Copy, Copy remains within AI scope, terminal and reading owner transitions invalidate, setup and configurable shortcut override/unbind work.
- Delayed real HTTP: query/reopen/focus/config/cwd/destroy and latest explicit intent; capture only first/latest, prove no second actual request while first is blocked.
- Native Shell/PTY: compare real line/caret, not only grid state. A harmless execution marker must remain absent after adoption and appear only after a separate Enter. Native proof requires result JSON `Ok`, not LLDB exit status or ignored-test presence.
- Prediction: controlled idle clock/restart and all admission negatives; Unicode suffix/no-submit in every completion mode; Shell-side prefix change rejected by requery; local-first zero HTTP and late-local priority; both shared-lane directions and no Escape retry; completion key before response cannot adopt a late model result or release its actual slot early.
- Use a valid Win32 Escape packet through the shared user-key write path to assert post-write epoch changes do not rearm dismissed input; preserve packet/actual-slot state and verify a later real input change can rearm.
- Active/drawn windows are required for real focus/Blur assertions. Marked-text tests are not OS candidate-window evidence; state which hosts/editors/visual states were actually checked.

## 7. Wrong vs Correct

Wrong: use the whole terminal's `contains_focused` as the AI owner. It also includes the terminal itself and unrelated reading inputs; an AI-looking surface can survive while Shell receives keys.

Correct: own a separate AI panel focus scope, keep internal control transitions valid, cancel on leaving it, exclude competing input modes, and expose the active AI query through `Focusable`. Keep the original native buffer unchanged until a verified explicit edit.
