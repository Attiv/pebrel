# Luau plugin runtime boundary

## 1. Scope / trigger

Use when changing Luau coroutine host calls, plugin permissions, VM budgets or protected-call behavior. Rationale and evidence live in architecture/notes/nebula_app/plugins/lua/2026-11-03-pre-yield-permission-denial.md.

## 2. Signatures

- plugins::package::allow_method(permissions: &[String], method: &str) -> Result<()> is the single permission/public-runtime-method rule; Package::allow_method delegates to it.
- Lua plugin commands call ctx.runtime.call(method, params); the host bridge yields the managed runtime request to Rust.
- Each execute invocation owns Budget.denial: Arc<Mutex<Option<Error>>>; it is not global or persisted.

## 3. Contracts

Validate the runtime method in Rust before a protected call can fail to yield. Declared permissions are only needs: the method must also belong to the host's public runtime surface. After each VM resume, check the invocation's sticky error before interpreting a successful value, issuing any host call or accepting a later permitted request. A pcall cannot erase rejection. New execute invocations own independent rejection state.

Keep memory, interrupt/deadline, request-count, cancellation/error and JSON-validation boundaries intact. Runtime I/O happens only outside the active Luau stack. Invalid method types retain invalid_runtime_request; do not relabel them as successful protected calls.

## 4. Validation / error matrix

| Input/state | Required result |
| --- | --- |
| Undeclared host method inside pcall | permission_denied; zero runtime calls |
| Catch rejection then try a declared method | Same invocation fails; still zero host calls |
| New independent valid invocation | Permitted request executes normally, not contaminated |
| nil/number/table runtime method | invalid_runtime_request |
| events.subscribe | stream_not_supported, not a one-shot stream bypass |
| Declared but nonpublic method | Existing boundary error, no expanded host authority |
| VM limit reached | Existing timeout/interrupt/memory failure, no fabricated success |

## 5. Good / base / bad cases

Good: declared one-shot public runtime method, valid parameters and budget. Base: a script that never requests host capabilities returns its normal JSON value. Bad: protected-call catches an unvalidated yield error and returns true despite an attempted forbidden request.

## 6. Required tests

Preserve denied_lua_call_inside_pcall_never_reaches_runtime. caught_denial_blocks_later_permitted_calls_only_in_the_same_invocation must assert permission_denied, zero runtime calls, then exactly one call in an independent invocation. protected_invalid_runtime_method_keeps_its_request_error covers nil/number/table. Keep native error receipts, budget loops/memory, ordinary valid-call and package-boundary tests.

## 7. Wrong vs correct

Wrong: check permissions only after receiving a coroutine yield, or change unwrap_err to accept a caught denial.

Correct: use the same allow_method before yielding and at the runtime boundary; hold the first request rejection in the invocation owner and reject before interpreting the resumed value or dispatching another host request. This restores the prior fail-closed contract without adding a permission policy or dependency.
