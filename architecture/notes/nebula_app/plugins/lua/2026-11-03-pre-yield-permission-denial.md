# Invocation-owned plugin denial before Luau yields

## Status

Implemented to preserve the existing denied-request contract during the Luau
migration. Native verification uses the freshly rebuilt product test graph.

## Context

Plugin Runtime requests are transported by a managed coroutine. Permission
checks formerly occurred only after the host driver received that yield. The
Luau migration exposed a case where a protected call could catch an error before
the yield reached the driver, return normally and never execute authorization.
The existing denied-pcall regression returned a successful boolean instead of
its required `permission_denied` receipt, though the Runtime callback was not
invoked. A caught pre-yield error is not evidence a request was authorized.

## Evidence

- [Plugin request/driver](../../../../../nebula_app/src/plugins/lua.rs) binds the
  request function, resumes the managed coroutine and owns invocation limits.
- [Package authorization](../../../../../nebula_app/src/plugins/package.rs) checks
  declared one-shot permissions and the shared reviewed host capability boundary.
- [Regressions](../../../../../nebula_app/src/plugins/tests.rs) preserve denied
  protected-call execution, independent invocation and native/Luau boundary checks.
- [VM limits](../../../../../nebula_app/src/luau.rs) already prevent protected calls
  from swallowing execution quotas; that quota owner does not grant permissions.

## Decision

Authorize the method through the same package rule before attempting the Luau
request yield. Capture only the immutable permission list for that callback;
package filesystem metadata and native command execution remain with their
existing owners. The driver still validates the actual yielded request before
Runtime dispatch.

A denied request sets an invocation-owned receipt. Every later preflight in the
same invocation returns that denial, and the driver checks the receipt after
VM resume even if a protected call caught the immediate error. Neither normal
return nor another nominally permitted request can turn that denial into
successful execution or reach Runtime. Native operation errors remain ordinary
catchable results; no permission, Runtime method or process-global flag is added.

Each invocation creates a fresh receipt owner and releases it with the VM. A
later independent invocation can call an allowed method normally; a failed
plugin command does not poison another plugin invocation or a user's terminal.
Existing source/JSON/VM/deadline/call-count limits remain in effect.

## Rejected alternatives

- Replacing the original `unwrap_err` assertion with a successful return would
  hide the missing authorization path and weaken the established safety contract.
- A Lua-only check or post-yield-only rejection can be bypassed by a protected
  pre-yield failure and does not establish the Rust authorization boundary.
- Copying a second permission rule would let native and Luau paths diverge.
- A global denied flag would contaminate independent invocations and ownership.
- Granting the denied method or changing the rejection type would expand the
  plugin API instead of repairing its existing boundary.

## Consequences

The permission decision remains single-authority and the denial lifetime remains
one command. Protected calls cannot produce successful receipts after a denied
Runtime attempt. This is a cold-path check, not a claim about broader runtime
performance or support for additional protected-call yield behaviors.

## Validation

The original denied-pcall regression is retained. A new regression tries a
permitted Runtime method after catching a denied method, asserts the original
error code and zero callbacks, then proves an independent permitted invocation
calls Runtime once and returns its result. Existing direct/native/core-control,
JSON and VM-limit tests remain applicable. Test execution must use the newly
rebuilt binary; an old snapshot cannot verify this implementation.

## Supersedes

None. This fulfills the existing plugin capability boundary and complements,
rather than replaces, the Luau VM quota decision.

## Revisit when

The plugin transport supports another protected-call/yield mechanism or a
resident invocation lifecycle. Preserve equivalent pre-dispatch authorization,
unrecoverable denial within an invocation and independent-invocation isolation.
