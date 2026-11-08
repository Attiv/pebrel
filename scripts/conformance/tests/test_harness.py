from __future__ import annotations

import json
from contextlib import contextmanager
import plistlib
import re
import subprocess
import sys
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch
import zipfile
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parents[2]
if str(SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPTS_DIR))

from conformance.harness import (  # noqa: E402
    ApiFailure,
    ConformanceError,
    ResolvedApp,
    compare_reports,
    find_last_line_match,
    filter_flat,
    flatten,
    normalize_path,
    normalize_text,
    shell_category,
    SkipCase,
)
from conformance.macos_launch import capture_screenshot


class BootReadinessTests(unittest.TestCase):
    @contextmanager
    def clock(self):
        now = SimpleNamespace(value=0.0)
        with patch("time.monotonic", side_effect=lambda: now.value), patch(
            "time.sleep", side_effect=lambda duration: setattr(now, "value", now.value + duration)
        ):
            yield now

    def context(self, outputs):
        from conformance.harness import ConformanceContext, PROTOCOL_NAME, PROTOCOL_VERSION

        pane = {"id": 1, "cwd": r"C:\fixture"}
        tab = {"panes": [pane]}
        snapshot = {"protocol_version": PROTOCOL_VERSION, "windows": [{"tabs": [tab]}]}
        context = SimpleNamespace(
            snapshot=lambda: snapshot, refresh_targets=lambda value: None,
            window_id=1, pane_id=1, startup_ms=10, startup_timeout=4,
            client=SimpleNamespace(timeout=5.0),
            description={"protocol": PROTOCOL_NAME, "protocol_version": PROTOCOL_VERSION,
                         "capabilities": ["runtime.snapshot", "window.close", "tab.new",
                                          "tab.rename", "pane.split", "pane.resize", "pane.prompt",
                                          "pane.paste", "pane.read", "pane.procs", "pane.send_key"]},
            detect_shell=lambda: "powershell",
            api=Mock(return_value={
                "window_id": 1, "pane_id": 1, "root_pid": 42,
                "processes": [{
                    "pid": 42, "parent_pid": None, "executable": "powershell.exe",
                    "display_name": "powershell.exe", "depth": 0,
                }],
            }),
            tab_for_pane=lambda *args: tab, read=Mock(side_effect=outputs),
        )
        context.poll = ConformanceContext.poll.__get__(context)
        return context

    def test_runtime_discovery_alone_does_not_finish_cold_shell_boot(self):
        from conformance.cases.boot import run

        context = self.context([{"text": ""}, {"text": " \n"}, {"text": "PS C:\\fixture> "}])
        with self.clock():
            result = run(context)
        self.assertEqual(context.read.call_count, 3)
        self.assertTrue(result["default_tab_present"])
        self.assertGreater(result["startup_ms"], context.startup_ms)

    def test_silent_shell_fails_boot_instead_of_being_marked_ready(self):
        from conformance.cases.boot import run

        context = self.context(None)
        context.read.side_effect = None
        context.read.return_value = {"text": ""}
        with self.clock():
            with self.assertRaisesRegex(ConformanceError, "did not render initial output"):
                run(context)

    def test_boot_waits_for_a_transient_process_query_then_requires_the_actual_tree(self):
        from conformance.cases.boot import run

        context = self.context([{"text": "PS C:\\fixture> "}])
        tree = context.api.return_value
        context.api.side_effect = [
            ApiFailure("pane.procs", {
                "code": "process_query_failed", "message": "process tree unavailable",
                "details": {"root_pid": 42, "reason": "fixture transient query failure"},
            }),
            tree,
        ]
        with self.clock():
            result = run(context)
        self.assertTrue(result["process_tree_available"])
        self.assertEqual(context.api.call_count, 2)
        self.assertEqual(context.read.call_count, 1)

    def test_boot_permanent_process_failure_expires_in_the_original_readiness_budget(self):
        from conformance.cases.boot import run

        context = self.context(None)
        context.api.side_effect = ApiFailure("pane.procs", {
            "code": "process_query_failed", "message": "process tree unavailable",
            "details": {"root_pid": 42, "reason": "fixture permanent query failure"},
        })
        with self.clock() as now:
            with self.assertRaisesRegex(ConformanceError, "root_pid.*42") as failed:
                run(context)
        self.assertIn("fixture permanent query failure", str(failed.exception))
        self.assertGreater(context.api.call_count, 1)
        self.assertLessEqual(now.value, context.startup_timeout)

    def test_boot_rejects_a_different_panes_tree_without_waiting_for_a_later_valid_one(self):
        from conformance.cases.boot import run

        for wrong in ({"pane_id": 9}, {"window_id": 9}):
            with self.subTest(wrong=wrong):
                context = self.context([{"text": "PS C:\\fixture> "}])
                tree = context.api.return_value
                context.api.side_effect = [{**tree, **wrong}, tree]
                with self.assertRaises(ConformanceError):
                    run(context)
                self.assertEqual(context.api.call_count, 1)

    def test_boot_does_not_retry_other_api_failures(self):
        from conformance.cases.boot import run

        context = self.context([{"text": "PS C:\\fixture> "}])
        context.api.side_effect = [
            ApiFailure("pane.procs", {"code": "invalid_target", "message": "wrong pane"}),
            context.api.return_value,
        ]
        with self.assertRaisesRegex(ApiFailure, "invalid_target"):
            run(context)
        self.assertEqual(context.api.call_count, 1)

    def test_boot_rejects_empty_or_malformed_process_trees_without_retrying_them(self):
        from conformance.cases.boot import run

        context = self.context([{"text": "PS C:\\fixture> "}])
        tree = context.api.return_value
        for invalid in (
            None, [], {**tree, "processes": []}, {**tree, "root_pid": 0},
            {**tree, "processes": [None]}, {**tree, "processes": [{"pid": 99}]},
            {**tree, "processes": [{"pid": 42}]},
            {**tree, "processes": [{**tree["processes"][0], "pid": 42.0}]},
            {**tree, "processes": [{**tree["processes"][0], "depth": 1}]},
            {**tree, "processes": [{**tree["processes"][0], "parent_pid": 7}]},
        ):
            with self.subTest(tree=invalid):
                context = self.context([{"text": "PS C:\\fixture> "}])
                context.api.side_effect = [invalid, tree]
                with self.assertRaises(ConformanceError):
                    run(context)
                self.assertEqual(context.api.call_count, 1)

    def test_boot_propagates_a_later_fatal_error_instead_of_the_old_transient_details(self):
        from conformance.cases.boot import run

        for fatal in (
            ApiFailure("pane.procs", {"code": "invalid_target", "message": "final wrong pane"}),
            {"window_id": 1, "pane_id": 9},
        ):
            with self.subTest(fatal=fatal):
                context = self.context([{"text": "PS C:\\fixture> "}])
                context.api.side_effect = [
                    ApiFailure("pane.procs", {
                        "code": "process_query_failed", "details": {"reason": "earlier transient"},
                    }),
                    fatal,
                ]
                with self.clock(), self.assertRaises(ConformanceError) as failed:
                    run(context)
                self.assertNotIn("earlier transient", str(failed.exception))
                if isinstance(fatal, ApiFailure):
                    self.assertIs(failed.exception, fatal)
                else:
                    self.assertIn("different window or pane", str(failed.exception))
                self.assertEqual(context.api.call_count, 2)
                context.read.assert_not_called()

    def test_boot_does_not_increase_the_existing_rpc_timeout(self):
        from conformance.cases.boot import run

        context = self.context([{"text": "PS C:\\fixture> "}])
        context.client.timeout = 0.25
        with self.clock():
            run(context)
        self.assertEqual(context.api.call_args.kwargs["timeout"], 0.25)
        self.assertEqual(context.read.call_args.kwargs["timeout"], 0.25)

    def test_read_forwards_the_optional_budget_without_changing_default_normalization(self):
        from conformance.harness import ConformanceContext

        context = self.context(None)
        context.api.return_value = {"text": "fixture prompt\r\n"}
        self.assertEqual(ConformanceContext.read(context, timeout=0.25)["text"], "fixture prompt")
        context.api.assert_called_once_with(
            "pane.read", {"window_id": 1, "pane_id": 1, "lines": 120}, timeout=0.25,
        )
        context.api.reset_mock()
        ConformanceContext.read(context, pane_id=7, lines=3)
        context.api.assert_called_once_with(
            "pane.read", {"window_id": 1, "pane_id": 7, "lines": 3}, timeout=None,
        )

    def test_boot_caps_each_rpc_by_the_same_remaining_readiness_budget(self):
        from conformance.cases.boot import run

        context = self.context(None)
        tree = context.api.return_value
        calls = []
        with self.clock() as now:
            def processes(*args, timeout=None):
                calls.append(timeout)
                now.value += 2.0 if len(calls) == 1 else 1.0
                if len(calls) == 1:
                    raise ApiFailure("pane.procs", {"code": "process_query_failed"})
                return tree

            def output(*args, timeout=None):
                calls.append(timeout)
                now.value += 0.5
                return {"text": "PS C:\\fixture> "}

            context.api.side_effect = processes
            context.read.side_effect = output
            result = run(context)
        self.assertTrue(result["process_tree_available"])
        self.assertEqual(len(calls), 3)
        for observed, expected in zip(calls, (4.0, 1.9, 0.9)):
            self.assertAlmostEqual(observed, expected)
        self.assertLessEqual(now.value, context.startup_timeout)

    def test_boot_does_not_start_a_read_after_process_query_consumes_the_budget(self):
        from conformance.cases.boot import run

        context = self.context([{"text": "PS C:\\fixture> "}])
        tree = context.api.return_value
        with self.clock() as now:
            def processes(*args, timeout=None):
                now.value += timeout if timeout is not None else context.client.timeout
                return tree

            context.api.side_effect = processes
            with self.assertRaises(ConformanceError):
                run(context)
        context.read.assert_not_called()
        self.assertEqual(context.api.call_count, 1)
        self.assertEqual(now.value, context.startup_timeout)

    def test_boot_does_not_reset_its_absolute_deadline_when_poll_entry_is_delayed(self):
        from conformance.cases.boot import run

        context = self.context(None)
        context.api.side_effect = ApiFailure("pane.procs", {
            "code": "process_query_failed", "details": {"reason": "permanent query failure"},
        })
        original_poll = context.poll
        with self.clock() as now:
            def delayed_poll(*args, **kwargs):
                now.value += 1.0
                return original_poll(*args, **kwargs)

            context.poll = delayed_poll
            with self.assertRaisesRegex(ConformanceError, "permanent query failure"):
                run(context)
        self.assertLessEqual(now.value, context.startup_timeout)

    def test_boot_does_not_accept_output_received_after_the_readiness_deadline(self):
        from conformance.cases.boot import run

        context = self.context(None)
        with self.clock() as now:
            def late_output(*args, timeout=None):
                now.value += context.startup_timeout + 0.1
                return {"text": "PS C:\\fixture> "}

            context.read.side_effect = late_output
            with self.assertRaises(ConformanceError):
                run(context)
        self.assertEqual(context.read.call_count, 1)

    def test_poll_preserves_its_default_and_absolute_deadlines_can_only_tighten_it(self):
        for deadline, expected in ((None, 10.0), (2.0, 2.0), (20.0, 10.0)):
            with self.subTest(deadline=deadline), self.clock() as now:
                context = self.context(None)
                with self.assertRaisesRegex(ConformanceError, "fixture not ready"):
                    context.poll(lambda: False, bool, "fixture not ready", deadline=deadline)
                self.assertEqual(now.value, expected)

    def test_poll_does_not_accept_a_late_valid_probe_with_an_absolute_deadline(self):
        context = self.context(None)
        with self.clock() as now:
            def late_probe():
                now.value = 2.0
                return True

            with self.assertRaises(ConformanceError):
                context.poll(late_probe, bool, "fixture not ready", deadline=1.0)

    def test_process_api_failure_keeps_structured_details_in_case_diagnostics(self):
        from conformance.run import case_error

        error = ApiFailure("pane.procs", {
            "code": "process_query_failed", "message": "process tree unavailable",
            "details": {"root_pid": 42, "reason": "fixture precise failure reason"},
        })
        message = case_error(error)
        self.assertIn("root_pid", message)
        self.assertIn("42", message)
        self.assertIn("fixture precise failure reason", message)


class NormalizationTests(unittest.TestCase):
    def test_normalize_path_handles_drive_and_posix_paths(self) -> None:
        self.assertEqual(normalize_path(r"C:\Users\Test\nebula"), "/c/Users/Test/nebula")
        self.assertEqual(normalize_path("/tmp//nebula/"), "/tmp/nebula")
        self.assertEqual(normalize_path("/"), "/")

    def test_normalize_text_unifies_newlines_and_trailing_space(self) -> None:
        self.assertEqual(normalize_text("alpha  \r\nbeta\r\n\r\n"), "alpha\nbeta")

    def test_shell_category_accepts_paths_and_labels(self) -> None:
        self.assertEqual(shell_category(r"C:\Windows\System32\WindowsPowerShell.exe"), "powershell")
        self.assertEqual(shell_category("/bin/zsh"), "zsh")
        self.assertEqual(shell_category("Nushell"), "nu")
        self.assertIsNone(shell_category("unknown-shell"))

    def test_line_match_accepts_output_attached_to_a_wrapped_command(self) -> None:
        text = (
            "❯ Write-Output ('NEBULA_COLS_2_' + "
            "$Host.UI.RawUI.WindowSNEBULA_COLS_2_57\n❯"
        )
        match = find_last_line_match(re.compile(r"NEBULA_COLS_2_(\d+)"), text)
        self.assertIsNotNone(match)
        assert match is not None
        self.assertEqual(match.group(1), "57")


class ComparisonTests(unittest.TestCase):
    def test_new_ssh_capability_cannot_remain_silently_skipped(self) -> None:
        from conformance.cases.ssh_loop import run

        with self.assertRaises(SkipCase):
            run(SimpleNamespace(description={"capabilities": []}))
        with self.assertRaises(ConformanceError):
            run(SimpleNamespace(description={"capabilities": ["ssh.open"]}))

    def test_flatten_and_filter_apply_glob_rules(self) -> None:
        flat = flatten({"cases": {"boot": {"status": "passed", "duration_ms": 4}}})
        self.assertEqual(
            filter_flat(flat, ["cases.*.duration_ms"]),
            {"cases.boot.status": "passed"},
        )

    def test_cross_platform_whitelist_ignores_only_documented_fields(self) -> None:
        first = {
            "schema_version": 1,
            "platform": "windows-x86_64",
            "cases": {
                "boot": {"status": "passed", "shell_category": "powershell"},
                "echo": {"status": "passed", "tail_contains_marker": True},
                "ssh_loop": {"status": "skipped", "reason": "missing"},
            },
            "summary": {"total": 3, "passed": 2, "failed": 0, "skipped": 1},
        }
        second = json.loads(json.dumps(first))
        second["platform"] = "linux-x86_64"
        second["cases"]["boot"]["shell_category"] = "bash"
        second["cases"]["ssh_loop"] = {"status": "passed"}
        second["summary"]["passed"] = 3
        second["summary"]["skipped"] = 0

        with tempfile.TemporaryDirectory() as directory:
            golden = Path(directory)
            (golden / "whitelist.json").write_text(
                json.dumps(
                    {
                        "volatile": {},
                        "cross_platform": {
                            "platform": "identity",
                            "cases.boot.shell_category": "default shell",
                            "cases.ssh_loop.*": "optional",
                            "summary.passed": "optional",
                            "summary.skipped": "optional",
                        },
                    }
                ),
                encoding="utf-8",
            )
            self.assertEqual(compare_reports([first, second], golden), [])
            second["cases"]["echo"]["tail_contains_marker"] = False
            errors = compare_reports([first, second], golden)
            self.assertTrue(any("tail_contains_marker" in error for error in errors))

    def test_platform_golden_serves_every_architecture_of_a_family(self) -> None:
        from conformance.harness import compare_platform_golden, stable_flat

        report = {
            "schema_version": 1,
            "platform": "windows-x86_64",
            "cases": {"boot": {"status": "passed", "duration_ms": 3}},
            "summary": {"total": 1, "passed": 1, "failed": 0, "skipped": 0},
        }
        with tempfile.TemporaryDirectory() as directory:
            golden = Path(directory)
            (golden / "whitelist.json").write_text(
                json.dumps({"volatile": {"cases.*.duration_ms": "timing"}, "cross_platform": {}}),
                encoding="utf-8",
            )
            expected = stable_flat(report, golden)
            self.assertEqual(expected["platform"], "windows")
            (golden / "windows.json").write_text(json.dumps(expected), encoding="utf-8")
            arm64 = json.loads(json.dumps(report))
            arm64["platform"] = "windows-aarch64"
            self.assertEqual(compare_platform_golden(arm64, golden), [])
            arm64["cases"]["boot"]["status"] = "failed"
            self.assertTrue(compare_platform_golden(arm64, golden))


class ArchiveSafetyTests(unittest.TestCase):
    def test_new_portable_package_resolves_only_the_product_executable(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / "Pebrel-v1.6.0-windows-x64.zip"
            with zipfile.ZipFile(archive, "w") as output:
                output.writestr("pebrel.exe", b"MZfixture")
                output.writestr("runtime/pebrel-hook.exe", b"MZhelper")
                output.writestr("runtime/OpenConsole.exe", b"MZhost")
            app = ResolvedApp(archive)
            try:
                self.assertEqual(app.executable.name, "pebrel.exe")
                self.assertEqual(app.executable.read_bytes(), b"MZfixture")
            finally:
                app.close()

    def test_duplicate_product_binaries_remain_ambiguous(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "pebrel.exe").write_bytes(b"MZnew")
            (root / "nebula.exe").write_bytes(b"MZold")
            with self.assertRaisesRegex(ConformanceError, "2 product executables"):
                ResolvedApp(root)

    def test_macos_bundle_resolves_declared_executable_and_rejects_other_paths(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve() / "Pebrel Preview.app"
            binary = root / "Contents" / "MacOS" / "pebrel"
            binary.parent.mkdir(parents=True)
            binary.write_bytes(b"fixture")
            info = root / "Contents" / "Info.plist"
            info.write_bytes(plistlib.dumps({"CFBundleExecutable": "pebrel"}))
            app = ResolvedApp(root)
            self.assertEqual(app.executable, binary)
            app.close()
            info.write_bytes(plistlib.dumps({"CFBundleExecutable": "../elsewhere"}))
            with self.assertRaisesRegex(ConformanceError, "unexpected app executable"):
                ResolvedApp(root)

    def test_zip_member_cannot_escape_extraction_root(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / "bad.zip"
            with zipfile.ZipFile(archive, "w") as output:
                output.writestr("../outside/nebula", "not an executable")
            with self.assertRaisesRegex(ConformanceError, "escapes extraction root"):
                ResolvedApp(archive)
            self.assertFalse((root.parent / "outside" / "nebula").exists())


class LaunchCaptureTests(unittest.TestCase):
    def test_screenshot_requires_success_and_a_nonempty_file(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            destination = root / "capture.png"
            with (root / "capture.log").open("wb") as log, patch(
                "conformance.macos_launch.subprocess.run", return_value=SimpleNamespace(returncode=0),
            ):
                self.assertEqual(capture_screenshot(destination, log), "unavailable")
                destination.write_bytes(b"")
                self.assertEqual(capture_screenshot(destination, log), "unavailable")
                destination.write_bytes(b"screenshot")
                self.assertEqual(capture_screenshot(destination, log), "captured")

    def test_screenshot_failure_does_not_fail_the_launch_test(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            destination = root / "capture.png"
            destination.write_bytes(b"partial")
            with (root / "capture.log").open("wb") as log, patch(
                "conformance.macos_launch.subprocess.run", return_value=SimpleNamespace(returncode=1),
            ):
                self.assertEqual(capture_screenshot(destination, log), "unavailable")

    def test_screenshot_errors_do_not_fail_the_launch_test(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with (root / "capture.log").open("wb") as log:
                for error in (OSError("capture unavailable"), subprocess.TimeoutExpired("screencapture", 15)):
                    with self.subTest(error=error), patch(
                        "conformance.macos_launch.subprocess.run", side_effect=error,
                    ):
                        self.assertEqual(capture_screenshot(root / "capture.png", log), "unavailable")


if __name__ == "__main__":
    unittest.main()
