from __future__ import annotations

import copy
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

from mobile.tools import build_ghostty as ghostty
from scripts import android_release as android
from scripts.preview_release import sha256


class GhosttyBuildEnvironmentTests(unittest.TestCase):
    def setUp(self):
        root = Path(__file__).resolve().parents[2]
        temporary_root = root / "tmp"
        temporary_root.mkdir(exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(dir=temporary_root)
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.repo = self.root / "pebrel"
        self.sources = self.repo / "build/sources/hash/ghostty"
        self.sources.mkdir(parents=True)
        self.ndk = self.root / "ndk"
        self.output = self.repo / "build"
        self.clean_environment = {
            key: value for key, value in os.environ.items()
            if not key.startswith("GIT_")
        }
        self.git("init", "--quiet")
        self.commit(self.repo)
        self.git("tag", "v2.1.1")

    def git(self, *arguments, cwd=None, environment=None, check=True):
        return subprocess.run(
            ["git", "-C", str(cwd or self.repo), *arguments],
            env=self.clean_environment if environment is None else environment,
            capture_output=True, text=True, check=check,
        )

    def commit(self, directory):
        self.git(
            "-c", "user.name=Build Regression",
            "-c", "user.email=build-test@example.invalid",
            "-c", "commit.gpgsign=false",
            "commit", "--allow-empty", "--no-verify", "-qm", "fixture",
            cwd=directory,
        )

    def environment(self):
        with mock.patch.dict(os.environ, self.clean_environment, clear=True):
            return ghostty.build_environment(self.sources, self.ndk, self.output)

    def test_archive_cannot_inherit_parent_release_tag(self):
        inherited = self.git("describe", "--exact-match", "--tags", cwd=self.sources)
        self.assertEqual(inherited.stdout.strip(), "v2.1.1")
        isolated = self.git(
            "describe", "--exact-match", "--tags", cwd=self.sources,
            environment=self.environment(), check=False,
        )
        self.assertNotEqual(isolated.returncode, 0)
        self.assertEqual(isolated.stdout.strip(), "")

    def test_environment_is_local_and_removes_explicit_parent_repository(self):
        inherited = self.clean_environment.copy()
        inherited.update({
            "GIT_DIR": str(self.repo / ".git"),
            "GIT_WORK_TREE": str(self.repo),
            "GIT_COMMON_DIR": str(self.repo / ".git"),
            "GIT_CEILING_DIRECTORIES": str(self.root),
            "BUILD_TEST_SENTINEL": "preserved",
        })
        with mock.patch.dict(os.environ, inherited, clear=True):
            original = dict(os.environ)
            environment = ghostty.build_environment(self.sources, self.ndk, self.output)
            self.assertEqual(dict(os.environ), original)
        for key in ("GIT_DIR", "GIT_WORK_TREE", "GIT_COMMON_DIR"):
            self.assertNotIn(key, environment)
        self.assertEqual(environment["GIT_CEILING_DIRECTORIES"], str(self.sources.resolve().parent))
        self.assertEqual(environment["ANDROID_NDK_HOME"], str(self.ndk))
        self.assertEqual(environment["ZIG_GLOBAL_CACHE_DIR"], str(self.output / "cache/global"))
        self.assertEqual(environment["BUILD_TEST_SENTINEL"], "preserved")

    def test_source_root_repository_remains_discoverable(self):
        self.git("init", "--quiet", cwd=self.sources)
        self.commit(self.sources)
        self.git("tag", "v1.3.0", cwd=self.sources)
        detected = self.git(
            "describe", "--exact-match", "--tags", cwd=self.sources,
            environment=self.environment(),
        )
        self.assertEqual(detected.stdout.strip(), "v1.3.0")


class AndroidReleaseTests(unittest.TestCase):
    def identity(self):
        metadata = {"applicationId": android.APPLICATION_ID, "variantName": "preview", "elements": [
            {"filters": [], "outputFile": "app-preview.apk", "versionCode": 18, "versionName": "2.0.0-preview"},
        ]}
        badging = f"package: name='{android.APPLICATION_ID}' versionCode='18' versionName='2.0.0-preview' platformBuildVersionName='15'\n"
        signature = f"Signer #1 certificate SHA-256 digest: {android.CERTIFICATE_SHA256}\n"
        return metadata, badging, signature

    def test_accepts_current_preview_and_incremented_version(self):
        self.assertEqual(android.validate_identity(*self.identity(), "2.0.0"), 18)
        self.assertEqual(android.asset_name("2.0.0"), "Pebrel-v2.0.0-android-universal-preview.apk")

    def test_rejects_production_debug_and_split_variants(self):
        metadata, badging, signature = self.identity()
        for field, value in (("applicationId", android.APPLICATION_ID.removesuffix(".preview")),
                             ("variantName", "debug")):
            changed = copy.deepcopy(metadata)
            changed[field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                android.validate_identity(changed, badging, signature, "2.0.0")
        for value in ([{"filterType": "ABI", "value": "arm64-v8a"}], None):
            changed = copy.deepcopy(metadata)
            changed["elements"][0]["filters"] = value
            with self.assertRaises(ValueError):
                android.validate_identity(changed, badging, signature, "2.0.0")
        with self.assertRaisesRegex(ValueError, "debuggable"):
            android.validate_identity(metadata, badging + "application-debuggable\n", signature, "2.0.0")

    def test_rejects_wrong_signer_version_and_compiled_package(self):
        metadata, badging, signature = self.identity()
        for changed in ("", signature.replace(android.CERTIFICATE_SHA256, "a" * 64),
                        signature + signature.replace("#1", "#2")):
            with self.assertRaisesRegex(ValueError, "signer"):
                android.validate_identity(metadata, badging, changed, "2.0.0")
        for code in (0, 17, True, "18"):
            changed = copy.deepcopy(metadata)
            changed["elements"][0]["versionCode"] = code
            with self.assertRaisesRegex(ValueError, "version"):
                android.validate_identity(changed, badging, signature, "2.0.0")
        with self.assertRaisesRegex(ValueError, "version"):
            android.validate_identity(metadata, badging, signature, "2.0.1")
        with self.assertRaisesRegex(ValueError, "Compiled"):
            android.validate_identity(metadata, badging.replace("versionCode='18'", "versionCode='19'"), signature, "2.0.0")

    def test_reports_require_executed_tests_and_reject_failures(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaisesRegex(ValueError, "Missing"):
                android.test_summary(root)
            report = root / "TEST-unit.xml"
            report.write_text('<testsuite failures="0" errors="0"><testcase name="pass"/><testcase name="skip"><skipped/></testcase></testsuite>', encoding="utf-8")
            self.assertEqual(android.test_summary(root), {"passed": 1, "skipped": 1})
            invalid = [
                '<testsuite><testcase><failure/></testcase></testsuite>',
                '<testsuite errors="1"><testcase/></testsuite>',
                '<testsuite><testcase><skipped/></testcase></testsuite>',
                '<testsuite tests="100"/>',
            ]
            for text in invalid:
                report.write_text(text, encoding="utf-8")
                with self.subTest(text=text), self.assertRaises(ValueError):
                    android.test_summary(root)

    def test_aggregate_binds_evidence_to_source_apk_and_both_test_suites(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            name = android.asset_name("2.0.0")
            apk = root / name
            apk.write_bytes(b"APK byte identity fixture, not an installable package")
            data = {"schema_version": 1, "status": "passed", "commit": "a" * 40, "version": "2.0.0",
                    "application_id": android.APPLICATION_ID, "version_code": 18,
                    "certificate_sha256": android.CERTIFICATE_SHA256, "apk": name, "sha256": sha256(apk),
                    "tests": {"unit": {"passed": 1}, "instrumented": {"passed": 1}}}
            report = root / "report.json"
            report.write_text(json.dumps(data), encoding="utf-8")
            android.validate_evidence(report, root, "2.0.0", "a" * 40)
            for field, value in (("commit", "b" * 40), ("sha256", "b" * 64), ("version_code", 17),
                                 ("certificate_sha256", "b" * 64), ("tests", {"unit": {"passed": 1}})):
                invalid = {**data, field: value}
                report.write_text(json.dumps(invalid), encoding="utf-8")
                with self.subTest(field=field), self.assertRaises(ValueError):
                    android.validate_evidence(report, root, "2.0.0", "a" * 40)

    def test_release_workflow_requires_android_and_reuses_preview_signing(self):
        root = Path(__file__).resolve().parents[2]
        workflow = (root / ".github/workflows/release.yml").read_text(encoding="utf-8")
        android_workflow = (root / ".github/workflows/android-release.yml").read_text(encoding="utf-8")
        self.assertIn("uses: ./.github/workflows/android-release.yml", workflow)
        self.assertIn("needs: [prepare, linux, macos, windows, windows-arm64, android]", workflow)
        self.assertIn("release-dist/*.apk", workflow)
        for required in (":app:testDebugUnitTest", ":app:lintPreview", ":app:assemblePreview",
                         ":app:connectedPreviewAndroidTest", "--commit", "stable-package-android",
                         "stable-evidence-android", "aarch64-unknown-linux-musl", "x86_64-unknown-linux-musl"):
            self.assertIn(required, android_workflow)
        self.assertNotIn("continue-on-error", android_workflow)
        self.assertNotIn("assembleDebug", android_workflow)
        self.assertNotIn("keytool -genkey", android_workflow)


if __name__ == "__main__":
    unittest.main()
