"""Offline installer contract tests, no network or real HOME writes."""
import hashlib
import io
import json
import os
import platform
import shutil
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
        self.env = dict(os.environ, HOME=str(self.root / "home"), PATH=f"{self.bin}:{os.environ['PATH']}", CURL_ARGS=str(self.root / "curl-args"), ASSETS=str(self.assets), REQUESTS=str(self.root / "requests"), MOCK_OS="Linux", MOCK_ARCH="x86_64")
        self.env["SHELL"] = "/bin/bash"
        self.env.pop("ZDOTDIR", None)
        self.env.pop("TSK_VERSION", None)
        self.env.pop("TSK_INSTALL_DIR", None)
        # Packaging CI sets CI=true; clear it so Herdr-prompt branches stay deterministic.
        self.env.pop("CI", None)
        self.setup_log = self.root / "setup-herdr.log"
        self.command("uname", '#!/bin/sh\ncase "$1" in -s) echo "$MOCK_OS";; -m) echo "$MOCK_ARCH";; esac\n')
        self.command("curl", '''#!/usr/bin/env python3
import json, os, pathlib, sys
args = sys.argv[1:]
url = args[-1]
with open(os.environ["CURL_ARGS"], "a") as out: out.write(json.dumps(args) + "\\n")
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

    def archive(self, target="x86_64-unknown-linux-musl", bad_checksum=False, member="tsk", record_setup=False):
        archive = self.assets / f"tsk-v1.2.3-{target}.tar.gz"
        with tarfile.open(archive, "w:gz") as out:
            if record_setup:
                data = b"""#!/bin/sh
if [ "${1:-}" = setup ] && [ "${2:-}" = herdr ]; then
    if [ -n "${TSK_SETUP_LOG:-}" ]; then
        printf '%s\\n' "$*" >> "$TSK_SETUP_LOG"
    fi
    exit 0
fi
echo installed-fixture
"""
            else:
                data = b"#!/bin/sh\necho installed-fixture\n"
            info = tarfile.TarInfo(member)
            info.size = len(data)
            info.mode = 0o755
            out.addfile(info, io.BytesIO(data))
        digest = "0" * 64 if bad_checksum else hashlib.sha256(archive.read_bytes()).hexdigest()
        (self.assets / "SHA256SUMS").write_text(f"{digest}  {archive.name}\n")

    def run_install(self, **env):
        return subprocess.run(["sh", str(INSTALLER)], env=dict(self.env, **env), cwd=self.root, text=True, capture_output=True, stdin=subprocess.DEVNULL, start_new_session=True)

    def run_install_with_answer(self, answer, **env):
        """Drive the Herdr prompt over a PTY so stdin is a TTY."""
        import errno
        import pty
        import select
        import time

        master, slave = pty.openpty()
        environment = dict(self.env, **env)
        process = None
        transcript = b""
        try:
            process = subprocess.Popen(
                ["sh", str(INSTALLER)],
                stdin=slave,
                stdout=slave,
                stderr=slave,
                env=environment,
                cwd=self.root,
                start_new_session=True,
            )
            os.close(slave)
            slave = None
            replied = False
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                if not select.select([master], [], [], 0.1)[0]:
                    if process.poll() is not None:
                        break
                    continue
                try:
                    chunk = os.read(master, 8192)
                except OSError as error:
                    if error.errno == errno.EIO:
                        break
                    raise
                if not chunk:
                    break
                transcript += chunk
                if b"[y/N]" in transcript and not replied:
                    os.write(master, answer)
                    replied = True
            code = process.wait(timeout=5)
            output = transcript.decode(errors="replace")
            return subprocess.CompletedProcess(["sh", str(INSTALLER)], code, output, "")
        finally:
            if process is not None and process.poll() is None:
                process.kill()
                process.wait()
            if slave is not None:
                os.close(slave)
            os.close(master)

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

    def test_path_setup_preserves_config_and_is_idempotent(self):
        self.archive()
        home = Path(self.env["HOME"])
        home.mkdir()
        rc = home / ".bashrc"
        rc.write_text("# existing config without final newline")
        for _ in range(2):
            result = self.run_install()
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("Reopen your terminal", result.stdout)
            self.assertIn("export PATH=", result.stdout)
        self.assertTrue(rc.read_text().startswith("# existing config without final newline\n"))
        self.assertEqual(rc.read_text().count("# tsk PATH"), 1)
        self.assertTrue((home / ".profile").exists())
        # Sourcing both login and interactive config twice must not duplicate PATH.
        command = '. "$HOME/.profile"; . "$HOME/.bashrc"; . "$HOME/.bashrc"; printf "%s" "$PATH"'
        activated = subprocess.run(["sh", "-c", command], env=self.env, text=True, capture_output=True, check=True)
        self.assertEqual(activated.stdout.split(":").count(str(home / ".local/bin")), 1)

    def test_bash_uses_existing_login_profile_without_shadowing_it(self):
        self.archive()
        home = Path(self.env["HOME"])
        home.mkdir()
        profile = home / ".bash_login"
        profile.write_text("# existing login\n")
        result = self.run_install()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse((home / ".bash_profile").exists())
        self.assertIn("# tsk PATH", profile.read_text())

    def test_zsh_uses_zdotdir_and_printed_export_handles_shell_metacharacters(self):
        self.archive()
        zdot = self.root / "zsh-config"
        dest = self.root / "bin ' $(touch INJECTED) $x `touch INJECTED`"
        result = self.run_install(SHELL="/bin/zsh", ZDOTDIR=str(zdot), TSK_INSTALL_DIR=str(dest))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((zdot / ".zshrc").exists())
        self.assertFalse((Path(self.env["HOME"]) / ".zshrc").exists())
        export = next(line.strip() for line in result.stdout.splitlines() if line.strip().startswith("export PATH="))
        for command in [export, '. "$1"']:
            activated = subprocess.run(["sh", "-c", command + '; command -v tsk', "sh", str(zdot / ".zshrc")], env=self.env, cwd=self.root, text=True, capture_output=True, check=True)
            self.assertEqual(activated.stdout.strip(), str(dest / "tsk"))
        self.assertFalse((self.root / "INJECTED").exists())

    def test_existing_path_needs_no_shell_edits(self):
        self.archive()
        home = Path(self.env["HOME"])
        result = self.run_install(PATH=f"{home / '.local/bin'}:{self.env['PATH']}")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse((home / ".bashrc").exists())
        self.assertFalse((home / ".profile").exists())
        self.assertNotIn("Reopen your terminal", result.stdout)

    def test_failed_download_never_edits_shell_config(self):
        self.archive(bad_checksum=True)
        result = self.run_install()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((Path(self.env["HOME"]) / ".bashrc").exists())
        self.assertFalse((Path(self.env["HOME"]) / ".profile").exists())

    def test_unsafe_or_unsupported_shell_config_keeps_install_and_gives_manual_guidance(self):
        self.archive()
        home = Path(self.env["HOME"])
        home.mkdir()
        target = self.root / "dotfile"
        target.write_text("# leave this alone\n")
        rc = home / ".zshrc"
        rc.symlink_to(target)
        result = self.run_install(SHELL="/bin/zsh")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(target.read_text(), "# leave this alone\n")
        self.assertIn("Could not update", result.stderr)
        self.assertIn("export PATH=", result.stdout)
        self.assertNotIn("Reopen your terminal", result.stdout)
        rc.unlink()
        result = self.run_install(SHELL="/bin/fish")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("configure PATH manually", result.stderr)
        self.assertFalse(rc.exists())
        self.assertFalse((home / ".config/fish").exists())

    def test_piped_script_configures_default_zsh_and_new_shell_finds_tsk(self):
        self.archive()
        environment = dict(self.env, SHELL="/bin/zsh")
        result = subprocess.run(["sh"], input=INSTALLER.read_text(), env=environment, cwd=self.root, text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        home = Path(self.env["HOME"])
        self.assertTrue((home / ".zshrc").exists())
        # Execute the generated source, not just a substring assertion.
        activated = subprocess.run(["sh", "-c", '. "$HOME/.zshrc"; tsk'], env=environment, text=True, capture_output=True, check=True)
        self.assertEqual(activated.stdout.strip(), "installed-fixture")

    @unittest.skipIf(os.geteuid() == 0, "root bypasses write permissions")
    def test_unwritable_startup_file_keeps_binary_and_reports_manual_setup(self):
        self.archive()
        home = Path(self.env["HOME"])
        home.mkdir()
        rc = home / ".zshrc"
        rc.write_text("# read only\n")
        rc.chmod(0o400)
        result = self.run_install(SHELL="/bin/zsh")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(rc.read_text(), "# read only\n")
        self.assertIn("configure PATH manually", result.stderr)
        self.assertNotIn("Reopen your terminal", result.stdout)
        self.assertTrue((home / ".local/bin/tsk").exists())

    def test_fresh_interactive_shells_find_the_installed_binary(self):
        self.archive()
        for name in ["bash", "zsh"]:
            with self.subTest(shell=name):
                shell = shutil.which(name)
                if not shell:
                    self.skipTest(f"{name} is not installed")
                result = self.run_install(SHELL=shell)
                self.assertEqual(result.returncode, 0, result.stderr)
                fresh = subprocess.run([shell, "-i", "-c", "tsk"], env=self.env, stdin=subprocess.DEVNULL, text=True, capture_output=True)
                self.assertEqual(fresh.returncode, 0, fresh.stderr)
                self.assertTrue(fresh.stdout.rstrip().endswith("installed-fixture"), fresh.stdout)

    def test_path_separator_directories_are_refused_before_asset_download(self):
        self.archive()
        for name in ["bad:bin", "bad\nbin"]:
            result = self.run_install(TSK_VERSION="v1.2.3", TSK_INSTALL_DIR=str(self.root / name))
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("PATH separators", result.stderr)
            self.assertFalse((self.root / "requests").exists())
            self.assertFalse((self.root / name).exists())

    def test_glob_characters_in_install_directory_are_literal(self):
        self.archive()
        for name in ["tsk*", "tsk?", "tsk[ab]"]:
            with self.subTest(name=name):
                dest = self.root / name
                result = self.run_install(TSK_INSTALL_DIR=str(dest), PATH=f"{self.root / 'tsk-old'}:{self.root / 'tska'}:{self.env['PATH']}")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("Reopen your terminal", result.stdout)
                self.assertIn(str(dest), result.stdout)
                active = subprocess.run(["sh", "-c", '. "$HOME/.bashrc"; command -v tsk'], env=self.env, text=True, capture_output=True, check=True)
                self.assertEqual(active.stdout.strip(), str(dest / "tsk"))

    def test_partial_bash_setup_names_skipped_file_and_keeps_successful_edit(self):
        self.archive()
        home = Path(self.env["HOME"])
        home.mkdir()
        target = self.root / "login-config"
        target.write_text("# managed elsewhere\n")
        login = home / ".bash_profile"
        login.symlink_to(target)
        result = self.run_install()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("# tsk PATH", (home / ".bashrc").read_text())
        self.assertEqual(target.read_text(), "# managed elsewhere\n")
        self.assertIn(str(login), result.stderr)
        self.assertIn("successful edits were kept", result.stderr)
        self.assertIn("export PATH=", result.stdout)

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

    def test_destination_guards_refuse_before_downloading_assets(self):
        self.archive()
        dest = self.root / "directory-destination"
        (dest / "tsk").mkdir(parents=True)
        marker = dest / "tsk/keep"
        marker.write_text("untouched")
        for directory, message in [("relative-bin", "absolute path"), (str(dest), "destination is a directory")]:
            with self.subTest(directory=directory):
                (self.root / "requests").unlink(missing_ok=True)
                result = self.run_install(TSK_VERSION="v1.2.3", TSK_INSTALL_DIR=directory)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(message, result.stderr)
                self.assertFalse((self.root / "requests").exists())
                self.assertFalse((self.root / "relative-bin").exists())
                self.assertEqual(list((dest / "tsk").iterdir()), [marker])
                self.assertEqual(marker.read_text(), "untouched")

    def test_every_download_restricts_protocol_redirects_and_tls(self):
        self.archive()
        result = self.run_install()
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = [json.loads(line) for line in (self.root / "curl-args").read_text().splitlines()]
        self.assertEqual(len(calls), 3)
        self.assertTrue(calls[0][-1].endswith("/releases/latest"))
        self.assertTrue(calls[1][-1].endswith(".tar.gz"))
        self.assertTrue(calls[2][-1].endswith("/SHA256SUMS"))
        for args in calls:
            with self.subTest(url=args[-1]):
                for flag in ["--proto", "--proto-redir"]:
                    self.assertIn(flag, args)
                    self.assertEqual(args[args.index(flag) + 1], "=https")
                self.assertIn("--tlsv1.2", args)
                self.assertNotIn("--insecure", args)
                self.assertNotIn("-k", args)

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

    def test_herdr_absent_stays_silent_about_plugin_setup(self):
        self.archive(record_setup=True)
        result = self.run_install(TSK_SETUP_LOG=str(self.setup_log), PATH=f"{self.env['PATH']}")
        self.assertEqual(result.returncode, 0, result.stderr)
        combined = result.stdout + result.stderr
        self.assertNotIn("Herdr detected", combined)
        self.assertNotIn("setup herdr", combined)
        self.assertFalse(self.setup_log.exists())

    def test_herdr_present_without_tty_skips_with_guidance(self):
        self.archive(record_setup=True)
        self.command("herdr", "#!/bin/sh\nexit 0\n")
        result = self.run_install(TSK_SETUP_LOG=str(self.setup_log))
        self.assertEqual(result.returncode, 0, result.stderr)
        combined = result.stdout + result.stderr
        self.assertIn("Herdr detected", combined)
        self.assertIn("Skipping plugin setup (no TTY)", combined)
        installed = self.root / "home/.local/bin/tsk"
        self.assertIn(f"{installed} setup herdr", combined)
        self.assertFalse(self.setup_log.exists())

    def test_herdr_present_in_ci_skips_without_asking(self):
        self.archive(record_setup=True)
        self.command("herdr", "#!/bin/sh\nexit 0\n")
        result = self.run_install(TSK_SETUP_LOG=str(self.setup_log), CI="1")
        self.assertEqual(result.returncode, 0, result.stderr)
        combined = result.stdout + result.stderr
        self.assertIn("Skipping plugin setup (CI)", combined)
        self.assertIn("setup herdr", combined)
        self.assertNotIn("[y/N]", combined)
        self.assertFalse(self.setup_log.exists())

    @unittest.skipUnless(os.name == "posix", "PTY prompt requires POSIX")
    def test_herdr_prompt_yes_runs_installed_tsk_setup(self):
        self.archive(record_setup=True)
        self.command("herdr", "#!/bin/sh\nexit 0\n")
        result = self.run_install_with_answer(b"y\n", TSK_SETUP_LOG=str(self.setup_log))
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertIn("[y/N]", result.stdout)
        self.assertIn("Running ", result.stdout)
        installed = self.root / "home/.local/bin/tsk"
        self.assertTrue(self.setup_log.exists(), result.stdout)
        self.assertEqual(self.setup_log.read_text().strip(), "setup herdr")
        self.assertTrue(installed.exists())

    @unittest.skipUnless(os.name == "posix", "PTY prompt requires POSIX")
    def test_herdr_prompt_no_skips_setup_with_guidance(self):
        self.archive(record_setup=True)
        self.command("herdr", "#!/bin/sh\nexit 0\n")
        result = self.run_install_with_answer(b"n\n", TSK_SETUP_LOG=str(self.setup_log))
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertIn("[y/N]", result.stdout)
        self.assertIn("Skipped Herdr plugin setup", result.stdout)
        installed = self.root / "home/.local/bin/tsk"
        self.assertIn(f"{installed} setup herdr", result.stdout)
        self.assertFalse(self.setup_log.exists())

    @unittest.skipUnless(os.name == "posix", "PTY prompt requires POSIX")
    def test_herdr_setup_failure_keeps_install_and_prints_guidance(self):
        archive = self.assets / "tsk-v1.2.3-x86_64-unknown-linux-musl.tar.gz"
        with tarfile.open(archive, "w:gz") as out:
            data = b"""#!/bin/sh
if [ "${1:-}" = setup ] && [ "${2:-}" = herdr ]; then
    echo setup-failed >&2
    exit 7
fi
echo installed-fixture
"""
            info = tarfile.TarInfo("tsk")
            info.size = len(data)
            info.mode = 0o755
            out.addfile(info, io.BytesIO(data))
        digest = hashlib.sha256(archive.read_bytes()).hexdigest()
        (self.assets / "SHA256SUMS").write_text(f"{digest}  {archive.name}\n")
        self.command("herdr", "#!/bin/sh\nexit 0\n")
        result = self.run_install_with_answer(b"y\n")
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertIn("tsk setup herdr failed", result.stdout)
        self.assertTrue((self.root / "home/.local/bin/tsk").exists())


if __name__ == "__main__":
    unittest.main()
