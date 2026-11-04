import re
import tomllib
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


class PebrelBrandingTests(unittest.TestCase):
    def source(self, path):
        return (ROOT / path).read_text(encoding="utf-8-sig")

    def manifest(self, path):
        return tomllib.loads(self.source(path))

    def test_visible_identity_and_cli_use_pebrel(self):
        brand = self.source("nebula_app/src/brand.rs")
        cli = self.source("nebula_app/src/cli.rs")
        self.assertIn('pub const NAME: &str = "Pebrel";', brand)
        self.assertIn('bin_name = "pebrel"', cli)
        self.assertIn('display_name = crate::brand::NAME', cli)
        self.assertIn('version = env!("VERSION")', cli)
        self.assertEqual(
            [target["name"] for target in self.manifest("nebula_app/Cargo.toml")["bin"]],
            ["pebrel", "pebrel-cli"],
        )
        self.assertEqual(
            [target["name"] for target in self.manifest("nebula_hook/Cargo.toml")["bin"]],
            ["pebrel-hook"],
        )
        window = self.source("nebula_app/src/config/window.rs")
        self.assertIn('DEFAULT_NAME: &str = crate::brand::NAME;', window)
        self.assertIn('DEFAULT_CLASS: &str = "Pebrel";', window)
        gpui = self.source("nebula_app/src/gpui_shell/workspace/windowing.rs") + self.source("nebula_app/src/gpui_shell/workspace/windowing/startup_geometry.rs")
        self.assertIn('window.set_window_title(crate::brand::NAME)', gpui)
        self.assertIn("startup_geometry::options(", gpui)
        geometry = self.source(
            "nebula_app/src/gpui_shell/workspace/windowing/startup_geometry.rs"
        )
        for fallback in ('pebrel', 'pebrel-quick-terminal'):
            self.assertRegex(geometry, rf'application_id\(\s*"{fallback}"\s*,?\s*\)')
        for constant, desktop in (
            ('LINUX_APP_ID', 'io.github.kuddev.pebrel'),
            ('LINUX_PREVIEW_APP_ID', 'io.github.kuddev.pebrel.preview'),
        ):
            self.assertIn(f'pub const {constant}: &str = "{desktop}";', brand)
            entry = self.source(f'packaging/linux/{desktop}.desktop')
            self.assertIn(f'StartupWMClass={desktop}', entry.splitlines())
            self.assertIn(f'Icon={desktop}', entry.splitlines())
        welcome = self.source("nebula_app/src/window_context/welcome.rs")
        self.assertIn('crate::brand::NAME', welcome)
        self.assertNotIn('Nebula Terminal', welcome)

    def test_installer_migrates_names_and_keeps_upgrade_identity(self):
        installer = self.source("scripts/installer.iss")
        for expected in (
            "AppName=Pebrel",
            "AppId={{61022144-7D0A-4E54-94F2-C329A8F58656}",
            "DefaultDirName={code:DefaultInstallDir}",
            "UsePreviousAppDir=yes",
            r"Software\Pebrel",
            r"App Paths\pebrel.exe",
            r"shell\Pebrel",
            r'Source: "{#BuildRoot}\pebrel.exe";',
            r'Source: "{#BuildRoot}\pebrel-hook.exe";',
            'RunOnceId: "RemovePebrelAiHooks"',
            'english.OpenInPebrel=Open in Pebrel',
            'chinesesimplified.OpenInPebrel=在 Pebrel 中打开',
            '#include "installer-migration.iss"',
        ):
            with self.subTest(expected=expected):
                self.assertIn(expected, installer)
        migration = self.source("scripts/installer-migration.iss")
        self.assertIn(r"{localappdata}\Programs\Pebrel", migration)
        self.assertIn(r"Software\Nebula Terminal", migration)

    def test_pebrel_assets_are_default_and_keep_explicit_legacy_alias(self):
        for path in ("scripts/package-release.ps1", "scripts/build-installer.ps1"):
            source = self.source(path)
            with self.subTest(path=path):
                self.assertIn("[ValidateSet('NebulaTerminal', 'Pebrel')]", source)
                self.assertIn("$PackageBrand = 'Pebrel'", source)
                self.assertIn(".VersionInfo.ProductName -ne 'Pebrel'", source)
                self.assertIn("gpui-shell", source)
                self.assertIn("Stale binary:", source)
        installer = self.source("scripts/installer.iss")
        self.assertIn('#define PackageBrand "Pebrel"', installer)
        self.assertIn(
            "OutputBaseFilename={#PackageBrand}-v{#AppVersion}-windows-{#Architecture}-setup",
            installer,
        )
        builder = self.source("scripts/build-installer.ps1")
        self.assertIn("[ValidateSet('x64', 'arm64')]", builder)
        self.assertIn('"/DPackageBrand=$PackageBrand"', builder)
        self.assertIn('"/DArchitecture=$Architecture"', builder)

    def test_update_asset_validation_shares_current_and_legacy_names(self):
        check = self.source("nebula_app/src/update_check.rs")
        download = self.source("nebula_app/src/update_download.rs")
        self.assertIn('format!("Pebrel-{version}-windows-x64-setup.exe")', check)
        self.assertIn('format!("Pebrel-v{version}-windows-x64-setup.exe")', check)
        self.assertIn('format!("NebulaTerminal-{version}-windows-x64-setup.exe")', check)
        self.assertIn("windows_x64_installer_names(version)", check)
        # Discovery and validation now route through the shared native selector.
        # Retain the Windows brand contract while also checking the macOS path;
        # executable Rust regressions cover accepted brands and rejected URLs.
        assets = self.source("nebula_app/src/update_check/assets.rs")
        self.assertIn("assets::native_names(&version)", check)
        self.assertIn("assets::native_names(&asset.version)", download)
        self.assertIn("super::windows_x64_installer_names(version).to_vec()", assets)
        self.assertIn("Platform::MacOS => macos_names(version, std::env::consts::ARCH)", assets)
        self.assertIn("if !names.contains(&asset.name)", download)

    def test_notification_identity_uses_pebrel(self):
        source = self.source("nebula_app/src/platform/notifications.rs")
        brand = self.source("nebula_app/src/brand.rs")
        taskbar = self.source("nebula_app/src/app_icon/windows/taskbar.rs")
        self.assertIn('WINDOWS_APP_ID: &str = "com.pebrel.terminal";', brand)
        self.assertIn('AUMID: &str = crate::brand::WINDOWS_APP_ID;', source)
        self.assertIn('set_string(&store, &APP_ID, crate::brand::WINDOWS_APP_ID)', taskbar)
        self.assertIn('set_reg_sz(&subkey, "DisplayName", crate::brand::NAME)', source)

    def test_config_directory_uses_pebrel_and_migrates_legacy_data(self):
        paths = self.source("nebula_settings/src/paths.rs")
        self.assertIn('"PEBREL_CONFIG_DIR", "NEBULA_CONFIG_DIR"', paths)
        self.assertIn('default_dir("Pebrel")', paths)
        self.assertIn('default_dir("Nebula")', paths)
        self.assertIn('"pebrel_settings.txt"', paths)
        migration = self.source("nebula_settings/src/paths/migration.rs")
        self.assertIn('("nebula_settings.txt", "pebrel_settings.txt")', migration)
        startup = self.source("nebula_app/src/main.rs")
        self.assertIn("nebula_settings::migrate_legacy_data()", startup)

    def test_project_identity_without_local_preview_notice(self):
        readme = self.source("README.md")
        self.assertIn('<h1 align="center">Pebrel</h1>', readme)
        self.assertIn("Pebrel (formerly Nebula)", readme)
        for path in ("README.md", "README.zh-CN.md"):
            with self.subTest(path=path):
                self.assert_fork_download(self.source(path))
        self.assertNotIn("Local branding preview", readme)
        self.assertNotIn("本地品牌预览", readme)
        self.assertNotIn("A name change does not create a new published release", readme)

    def assert_fork_download(self, readme):
        self.assertIn("https://github.com/Attiv/pebrel/releases/latest", readme)
        self.assertNotIn("https://github.com/Kuddev/pebrel/releases/latest", readme)

    def test_fork_download_rejects_an_upstream_latest_link(self):
        readme = self.source("README.md")
        for wrong in (
            readme.replace("Attiv/pebrel/releases/latest", "Kuddev/pebrel/releases/latest"),
            readme + "\nhttps://github.com/Kuddev/pebrel/releases/latest\n",
        ):
            with self.subTest(readme=wrong[-100:]):
                with self.assertRaises(AssertionError):
                    self.assert_fork_download(wrong)

    def test_current_changelog_matches_bilingual_release_notes(self):
        version = self.manifest("nebula_app/Cargo.toml")["package"]["version"]
        notes = self.source(f"docs/release-notes/v{version}.md")
        self.assert_release_documents(notes, self.source("CHANGELOG.md"), version)

    def assert_release_documents(self, notes, changelog, version):
        self.assertTrue(notes.startswith(f"# Pebrel {version}\n"))
        body = notes.split("## English\n", 1)[1]
        expected = re.sub(r"^##(?=#* )", "###", "## English\n" + body, flags=re.MULTILINE)
        entry = changelog.split(f"## {version} -", 1)[1].split("\n## ", 1)[0]
        actual = "### English\n" + entry.split("### English\n", 1)[1]
        self.assertEqual(actual.strip(), expected.strip())

    def test_current_release_documents_reject_missing_or_different_checksums(self):
        version = self.manifest("nebula_app/Cargo.toml")["package"]["version"]
        notes = self.source(f"docs/release-notes/v{version}.md")
        changelog = self.source("CHANGELOG.md")
        match = re.search(r"(`[^`]+`:\s*`)(PENDING FINAL BUILD|[0-9a-fA-F]{64})`", notes)
        self.assertIsNotNone(match)
        checksum = match.group(0)
        prefix, digest = match.groups()
        changed = prefix + ("0" * 64 if digest != "0" * 64 else "f" * 64) + "`"
        for case, wrong in (
            ("missing", changelog.replace(checksum, "", 1)),
            ("different", changelog.replace(checksum, changed, 1)),
        ):
            with self.subTest(case=case):
                self.assertNotEqual(wrong, changelog)
                with self.assertRaises(AssertionError):
                    self.assert_release_documents(notes, wrong, version)


if __name__ == "__main__":
    unittest.main()
