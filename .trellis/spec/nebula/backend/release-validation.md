# Native synchronization and release validation

## 1. Scope / trigger

Use for fork/upstream integration, stable platform scope, and native initialization caller changes. The authoritative current-release policy is architecture/notes/packaging/2026-11-03-fork-windows-macos-release-scope.md; asset lists remain owned by scripts/stable_release.py, not copied here.

## 2. Signatures

- stable_release.expected_asset_names(version: str) -> tuple[str, ...].
- ci_plan.plan_matrices(event_name, payload, changed_paths=None, *, release_version="") -> (native_matrix, release_matrix).
- preview_release.validate_evidence(directory, commit, *, windows_arm64=False, include_linux=True).
- gpui_shell::init(cx: &mut App, startup_window: config::StartupWindow); test callers use existing StartupWindow::load(None), not the former Option<PathBuf> argument.
- Settings line-level persistence lives in private nebula_settings/src/persistence.rs with unchanged root reexports; RawSettings values remain private.

## 3. Contracts

Stable prepare validates package version/tag and derives include_linux/include_android from expected_asset_names. The validated version passes through the reusable native workflow to the planner. Ordinary calls preserve the full matrix; the reviewed 2.4.0 fork exception selects Windows/macOS only. Required check names do not change.

Every included native/package job must succeed for the same source; excluded Linux/Android jobs must be skipped. Windows ARM64 and both macOS conformance/LaunchServices receipts stay mandatory. Upload exact validated manifest paths; refuse existing mismatched tags/releases. Source notes and Changelog share supported pending hashes, replaced by real hashes only in rendered public notes.

Local product tests need absolute matching PEBREL_CONFIG_DIR/NEBULA_CONFIG_DIR, isolated HOME/XDG where applicable and scrubbed runtime/config routing. Preserve pinned CARGO_HOME/RUSTUP_HOME. Python helpers need Python >=3.11; select the actual subprocess interpreter, not a shell alias. Mac GPUI builds require the official Metal Toolchain, verified with xcrun metal --version. CI uses the pinned nextest runner and .config/nextest.toml fixture scheduling; Cargo's same-process parallel runner does not apply those groups.

## 4. Validation and error matrix

| Condition | Required result |
| --- | --- |
| Ordinary call, no release version | Existing five-host matrix |
| Matching 2.4.0 stable release | Manifest-selected four native hosts |
| PR/schedule/merge-group narrowing or mismatched push tag | PlanError; no narrowed matrix |
| Invalid scope, selected skip/failure/cancel, unexpected excluded execution | Aggregate/publication blocked |
| Missing/wrong-source conformance or wrong public asset set/hash | Verification rejects |
| Old init(cx, None) caller | Compile failure; synchronize every cfg caller |
| Local interactive OS test cannot complete | Record unverified boundary; do not delete test or waive hosted full-native gate |

## 5. Good / base / bad cases

Good: matching tag/version and complete manifest-bound same-source Win/Mac evidence. Base: historical release/default native callers retain their old contracts. Bad: copying upstream packages/receipts, uploading nonexistent globs, treating untyped idle as success, or claiming mock Cargo lines prove a native build.

## 6. Required tests

Keep positive/negative assertions in test_stable_release, test_ci_plan, test_preview_release, test_android_release and test_pebrel_branding. Branding validates fork latest-download ownership and complete notes/Changelog hash equivalence, including missing/mismatched hash rejection. Compile all init callers including ignored/non-host cfg tests. Run source-preservation tests and actual applicable native suites; record commands, source and unrun hardware boundaries separately.

## 7. Wrong vs correct

Wrong: replace develop with upstream, pick ours/theirs wholesale, change budgets, delete failures or add compatibility wrappers just for stale tests.

Correct: refresh own develop and the latest published upstream source, merge normally preserving both parents, reconcile domain contracts, and use the current initializer directly:

```rust
crate::gpui_shell::init(cx, crate::gpui_shell::config::StartupWindow::load(None));
```

Wrong: claim publication from a push or Actions artifact. Correct: verify triggered source/ref, then independently verify public title/tag/body/names/labels/sizes/digests after successful hosted publication.

## Scenario: bounded native boot readiness

### 1. Scope / trigger

Use when changing packaged boot conformance, its API probes or polling. Rationale: `architecture/notes/scripts/conformance/2026-11-08-shared-boot-readiness-deadline.md`.

### 2. Signatures

The existing boot case uses `ConformanceContext.startup_timeout`; API/read calls may receive an optional tighter timeout. `ConformanceContext.poll(..., timeout=10.0, *, deadline=None)` optionally clamps its relative limit to the supplied absolute monotonic deadline; BOOT passes the same original deadline. Other callers' defaults remain unchanged.

### 3. Contracts

One original startup deadline covers a real nonempty tree for the original window/pane and the existing first-screen condition. Only this startup probe may treat `pane.procs/process_query_failed` as not-ready; other API/target/schema failures propagate. Permanent failures retain original structured details and fail by the same deadline. RPCs and sleep use remaining time; a late valid result cannot become success. No golden, product timeout or platform exception changes.

### 4. Validation & error matrix

| Condition | Required result |
| --- | --- |
| Query temporarily unavailable, then real matching tree and text before deadline | Pass actual readiness |
| Permanently unavailable query or absent screen | Fail within original budget, retain diagnostic |
| Wrong window/pane, malformed tree or unrelated API error | Immediate failure |
| Budget exhausted before a request | No new RPC |
| Valid response arrives after deadline | Failure, not late success |

### 5. Good / base / bad cases

Good: real target/tree/text ready within one budget. Base: immediately ready boot and default API callers behave unchanged. Bad: extra waiting windows, blanket retries, fake/empty process trees, late success or treating the original Intel error as conclusively transient without its reason.

### 6. Tests required

Run the real boot case through transient/valid, permanent, wrong-target and malformed-response fixtures; validate required native process fields/root identity, details, decreasing per-RPC time, no reset/no request after expiry, delayed poll entry and late-result rejection. Absolute poll deadlines may only tighten its existing relative cap; cover default callers too. Preserve resize observation order/geometry/cleanup and other conformance tests. Native packaged receipts still require a fresh same-source hosted run.

### 7. Wrong vs correct

Wrong: one fatal probe before the startup poll, or a second full timeout after it. Correct: one fixed deadline shared by the mandatory native process-tree and screen readiness checks, with genuine failure preserved.

## Scenario: nonblocking Windows keyboard fixture writes

### 1. Scope / trigger

Use when a test writes directly to the Windows PTY adapter. The reviewed incident and alternatives are in `architecture/notes/nebula_app/platform/keyboard/2026-11-04-nonblocking-conpty-fixture-writes.md`.

### 2. Signatures

- `UnblockedWriter::is_closed(&self) -> bool` is a read-only piper closure query; production `Write` and event-loop behavior are unchanged.
- Private `write_progress::advance(input: &[u8], written: io::Result<usize>, closed: bool, deadline: Instant) -> io::Result<&[u8]>` returns the unwritten suffix.

### 3. Contracts

Piper Pending maps to zero, including fairness yields on an open, nonfull pipe. Ready receipts establish reader setup, not writer readiness. The native fixture advances only accepted bytes, drains conout, checks fixture error/child exit and retains its ten-second deadline. A result receipt is read before rejecting normal child exit. No input encoding, mode, reader assertion or owned-PTY cleanup changes.

### 4. Validation & error matrix

| Condition | Required result |
| --- | --- |
| Open zero / Interrupted / WouldBlock | Same pending suffix; bounded retry |
| Partial accepted write | Advance exactly by accepted bytes; no replay |
| Closed zero or incomplete input | WriteZero; no retry |
| Real WriteZero / BrokenPipe / other error | Propagate failure |
| Impossible write count / expired deadline | InvalidData / TimedOut |
| Fixture error / premature child exit | Immediate failure with context |

### 5. Good / base / bad cases

Good: pending open writes complete once and produce exact native/byte results. Base: existing production polling retains pending bytes. Bad: retrying `write_all` from byte zero, sleeping before an assumed-blocking write, or masking closed-pipe failures.

### 6. Tests required

Run the real-piper open-yield/closed regression and progress partial/transient/closed/error/count/deadline tests. Preserve all four scenarios in `modified_enter_reaches_native_and_byte_readers_through_conpty`. Portable actual-source checks cannot replace both hosted Windows native suites or establish the hidden pipe state of a previous failure.

### 7. Wrong vs correct

Wrong: `pty.writer().write_all(&input).unwrap()` on the Pending-to-zero adapter. Correct: the test-owned finite progress loop, with read-only closure evidence and exact remaining bytes. Never weaken the Release gate to make a fixture pass.
