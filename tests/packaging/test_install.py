"""Offline installer contract tests, no network or real HOME writes."""
import hashlib
import io
import os
import platform
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
INSTALLER = ROOT / "site/public/install.sh"


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.bin = self.root / "commands"
        self.bin.mkdir()
        self.assets = self.root / "assets"
        self.assets.mkdir()
        self.env = dict(os.environ, HOME=str(self.root / "home"), PATH=f"{self.bin}:{os.environ['PATH']}", ASSETS=str(self.assets), REQUESTS=str(self.root / "requests"), MOCK_OS="Linux", MOCK_ARCH="x86_64")
        self.env.pop("TSK_VERSION", None)
        self.env.pop("TSK_INSTALL_DIR", None)
        self.command("uname", '#!/bin/sh\ncase "$1" in -s) echo "$MOCK_OS";; -m) echo "$MOCK_ARCH";; esac\n')
        self.command("curl", '''#!/usr/bin/env python3
import os, pathlib, sys
args = sys.argv[1:]
url = args[-1]
with open(os.environ["REQUESTS"], "a") as out: out.write(url + "\\n")
if os.environ.get("FAIL_DOWNLOAD"): sys.exit(22)
if url.endswith("/releases/latest"):
    print("https://github.com/smarzban/herdr-tsk/releases/tag/v1.2.3", end="")
else:
    assert "/releases/download/v1.2.3/" in url, url
    path = pathlib.Path(os.environ["ASSETS"]) / url.rsplit("/", 1)[-1]
    if not path.exists(): sys.exit(22)
    pathlib.Path(args[args.index("-o") + 1]).write_bytes(path.read_bytes())
''')

    def command(self, name, source):
        path = self.bin / name
        path.write_text(source)
        path.chmod(0o755)

    def archive(self, target="x86_64-unknown-linux-musl", bad_checksum=False, member="tsk"):
        archive = self.assets / f"tsk-v1.2.3-{target}.tar.gz"
        with tarfile.open(archive, "w:gz") as out:
            data = b"#!/bin/sh\necho installed-fixture\n"
            info = tarfile.TarInfo(member)
            info.size = len(data)
            info.mode = 0o755
            out.addfile(info, io.BytesIO(data))
        digest = "0" * 64 if bad_checksum else hashlib.sha256(archive.read_bytes()).hexdigest()
        (self.assets / "SHA256SUMS").write_text(f"{digest}  {archive.name}\n")

    def run_install(self, **env):
        return subprocess.run(["sh", str(INSTALLER)], env=dict(self.env, **env), text=True, capture_output=True)

    @unittest.skipUnless(os.environ.get("TSK_TEST_BINARY"), "set TSK_TEST_BINARY to smoke a built executable")
    def test_installed_real_binary_runs_isolated_cli(self):
        system, arch = platform.system(), platform.machine()
        cpu = "aarch64" if arch in ("arm64", "aarch64") else "x86_64"
        target = cpu + ("-apple-darwin" if system == "Darwin" else "-unknown-linux-musl")
        archive = self.assets / f"tsk-v1.2.3-{target}.tar.gz"
        # v1.2.3 is the offline HTTP fixture tag, not a public release claim.
        with tarfile.open(archive, "w:gz") as bundle:
            bundle.add(os.environ["TSK_TEST_BINARY"], arcname="tsk")
        digest = hashlib.sha256(archive.read_bytes()).hexdigest()
        (self.assets / "SHA256SUMS").write_text(f"{digest}  {archive.name}\n")
        result = self.run_install(MOCK_OS=system, MOCK_ARCH=arch)
        self.assertEqual(result.returncode, 0, result.stderr)
        installed = self.root / "home/.local/bin/tsk"
        isolated = dict(self.env, TSK_STATE_DIR=str(self.root / "state"), TSK_CONFIG_DIR=str(self.root / "config"))
        subprocess.run([str(installed), "add", "--desk", "-t", "Installed smoke"], env=isolated, check=True, capture_output=True)
        listed = subprocess.run([str(installed), "list", "--desk"], env=isolated, check=True, text=True, capture_output=True)
        self.assertIn("Installed smoke", listed.stdout)

    def test_latest_release_and_all_platforms(self):
        for system, arch, target in [("Linux", "x86_64", "x86_64-unknown-linux-musl"), ("Linux", "aarch64", "aarch64-unknown-linux-musl"), ("Darwin", "arm64", "aarch64-apple-darwin"), ("Darwin", "x86_64", "x86_64-apple-darwin")]:
            with self.subTest(target=target):
                self.archive(target)
                result = self.run_install(MOCK_OS=system, MOCK_ARCH=arch)
                self.assertEqual(result.returncode, 0, result.stderr)
                installed = self.root / "home/.local/bin/tsk"
                self.assertTrue(os.access(installed, os.X_OK))
                self.assertIn("installed-fixture", installed.read_text())
        requests = (self.root / "requests").read_text()
        self.assertIn("/releases/latest", requests)
        self.assertNotIn("/main/", requests)

    def test_explicit_version_custom_directory_and_upgrade(self):
        self.archive()
        dest = self.root / "custom dir"
        dest.mkdir()
        (dest / "tsk").write_text("old binary")
        result = self.run_install(TSK_VERSION="v1.2.3", TSK_INSTALL_DIR=str(dest))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("/releases/latest", (self.root / "requests").read_text())
        self.assertIn("installed-fixture", (dest / "tsk").read_text())

    def test_corrupt_download_preserves_existing_binary(self):
        self.archive(bad_checksum=True)
        dest = self.root / "bin"
        dest.mkdir()
        (dest / "tsk").write_text("old binary")
        result = self.run_install(TSK_INSTALL_DIR=str(dest))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("checksum", result.stderr)
        self.assertEqual((dest / "tsk").read_text(), "old binary")

    def test_refuses_unsupported_platform_and_invalid_version_without_network(self):
        for options in [dict(MOCK_OS="Windows"), dict(MOCK_ARCH="riscv64"), dict(TSK_VERSION="main"), dict(TSK_VERSION="v1.2.3/../../main"), dict(TSK_VERSION="v1.2.3-rc1")]:
            with self.subTest(options=options):
                result = self.run_install(**options)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse((self.root / "requests").exists())

    def test_missing_asset_and_missing_executable_do_not_install(self):
        for member in [None, "not-tsk"]:
            if member:
                self.archive(member=member)
            result = self.run_install()
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((self.root / "home/.local/bin/tsk").exists())

    def test_refuses_symlink_destination(self):
        self.archive()
        target = self.root / "untouched"
        target.write_text("keep")
        dest = self.root / "bin"
        dest.mkdir()
        (dest / "tsk").symlink_to(target)
        result = self.run_install(TSK_INSTALL_DIR=str(dest))
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(target.read_text(), "keep")

    def test_help_and_unknown_arguments_do_not_download(self):
        for argument, expected in [("--help", 0), ("--version", 1)]:
            result = subprocess.run(["sh", str(INSTALLER), argument], env=self.env, text=True, capture_output=True)
            self.assertEqual(result.returncode, expected, result.stderr)
            self.assertFalse((self.root / "requests").exists())

    def test_network_failure_never_installs(self):
        result = self.run_install(FAIL_DOWNLOAD="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.root / "home/.local/bin/tsk").exists())

    def test_duplicate_checksum_is_refused(self):
        self.archive()
        sums = self.assets / "SHA256SUMS"
        sums.write_text(sums.read_text() * 2)
        result = self.run_install()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.root / "home/.local/bin/tsk").exists())


if __name__ == "__main__":
    unittest.main()
