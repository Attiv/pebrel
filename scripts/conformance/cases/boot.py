from __future__ import annotations

import time

from conformance.harness import (
    ApiFailure, ConformanceContext, PROTOCOL_NAME, PROTOCOL_VERSION, path_shape, require,
)


def run(ctx: ConformanceContext) -> dict[str, object]:
    started = time.monotonic()
    snapshot = ctx.snapshot()
    ctx.refresh_targets(snapshot)
    windows = snapshot.get("windows") or []
    require(len(windows) == 1, f"isolated boot created {len(windows)} windows instead of one")
    window = windows[0]
    require(window.get("tabs"), "boot window has no tab")
    require(snapshot.get("protocol_version") == PROTOCOL_VERSION, "snapshot protocol mismatch")
    require(ctx.description.get("protocol") == PROTOCOL_NAME, "describe protocol mismatch")
    require(
        ctx.description.get("protocol_version") == PROTOCOL_VERSION,
        "describe protocol version mismatch",
    )
    required_methods = {
        "runtime.snapshot",
        "window.close",
        "tab.new",
        "tab.rename",
        "pane.split",
        "pane.resize",
        "pane.prompt",
        "pane.paste",
        "pane.read",
        "pane.procs",
        "pane.send_key",
    }
    capabilities = set(ctx.description.get("capabilities") or [])
    missing = sorted(required_methods - capabilities)
    require(not missing, f"runtime.describe omits required methods: {missing}")

    shell = ctx.detect_shell()
    require(
        shell in {"powershell", "pwsh", "cmd", "bash", "zsh", "fish", "nu", "sh", "dash"},
        f"unrecognized default shell: {shell}",
    )
    tab = ctx.tab_for_pane(snapshot, ctx.pane_id)
    pane = next(item for item in tab["panes"] if item["id"] == ctx.pane_id)
    cwd = pane.get("cwd", "")
    require(path_shape(cwd) != "relative", f"pane cwd is not absolute: {cwd!r}")

    # Runtime discovery and a child PID can exist before a cold shell emits any
    # text. Finish the bounded boot phase before the echo case submits input;
    # its own output deadline and single submission remain unchanged.
    deadline = time.monotonic() + ctx.startup_timeout
    last_process_error = None

    def readiness() -> dict[str, object]:
        nonlocal last_process_error
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            return {"deadline_exhausted": True, "process_error": last_process_error}
        try:
            tree = ctx.api(
                "pane.procs", {"window_id": ctx.window_id, "pane_id": ctx.pane_id},
                timeout=min(ctx.client.timeout, remaining),
            )
        except ApiFailure as error:
            if error.error.get("code") != "process_query_failed":
                raise
            last_process_error = error.error
            return {"process_error": error.error}
        require(isinstance(tree, dict), "pane.procs did not return an object")
        require(
            type(tree.get("window_id")) is int and tree["window_id"] == ctx.window_id
            and type(tree.get("pane_id")) is int and tree["pane_id"] == ctx.pane_id,
            "pane.procs returned a different window or pane",
        )
        processes = tree.get("processes")
        require(isinstance(processes, list) and processes, "pane.procs returned an empty process tree")
        root_pid = tree.get("root_pid")
        require(
            type(root_pid) is int and root_pid > 0
            and all(
                isinstance(process, dict)
                and type(process.get("pid")) is int and process["pid"] > 0
                and "parent_pid" in process
                and (process["parent_pid"] is None or (
                    type(process["parent_pid"]) is int and process["parent_pid"] > 0
                ))
                and isinstance(process.get("executable"), str)
                and isinstance(process.get("display_name"), str)
                and type(process.get("depth")) is int and process["depth"] >= 0
                for process in processes
            )
            and any(
                process["pid"] == root_pid and process["parent_pid"] is None and process["depth"] == 0
                for process in processes
            ),
            "pane.procs returned a malformed process tree or root",
        )
        last_process_error = None
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            return {"deadline_exhausted": True}
        read = ctx.read(timeout=min(ctx.client.timeout, remaining))
        return {"text": read["text"], "within_deadline": time.monotonic() < deadline}

    ctx.poll(
        readiness,
        lambda observed: bool(observed.get("text", "").strip()) and observed.get("within_deadline"),
        "default shell did not render initial output with a valid process tree",
        max(0, deadline - time.monotonic()),
        deadline=deadline,
    )

    return {
        "window_count": 1,
        "default_tab_present": True,
        "runtime_methods_present": True,
        "process_tree_available": True,
        "shell_category": shell,
        "cwd_shape": path_shape(cwd),
        "startup_ms": ctx.startup_ms + round((time.monotonic() - started) * 1000),
    }
