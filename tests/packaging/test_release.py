import hashlib
import importlib.util
import io
import re
from pathlib import Path
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("release", ROOT / "scripts/release.py")
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.binary = self.root / "binary"
        self.binary.write_text("fixture")
        self.binary.chmod(0o755)
        self.out = self.root / "out"

    def test_assemble_refuses_every_existing_output_without_touching_it(self):
        for target in release.TARGETS:
            release.package("v1.2.3", target, self.binary, self.out)
        victim = self.root / "victim"
        victim.write_text("keep me")
        for name in ["install.sh", "SHA256SUMS", "tsk.rb"]:
            for kind in ["file", "symlink", "dangling"]:
                with self.subTest(name=name, kind=kind):
                    path = self.out / name
                    if kind == "file": path.write_text("keep me")
                    else: path.symlink_to(victim if kind == "symlink" else self.root / "absent")
                    try:
                        with self.assertRaises((ValueError, FileExistsError)):
                            release.assemble("v1.2.3", self.out)
                        self.assertEqual(victim.read_text(), "keep me")
                        self.assertFalse((self.root / "absent").exists())
                        self.assertEqual({p.name for p in self.out.iterdir() if not p.name.endswith(".tar.gz")}, {name})
                    finally:
                        for output in ["install.sh", "SHA256SUMS", "tsk.rb"]:
                            (self.out / output).unlink(missing_ok=True)

    def test_package_and_formula_pin_all_platforms(self):
        for target in release.TARGETS:
            release.package("v1.2.3", target, self.binary, self.out)
        release.assemble("v1.2.3", self.out)
        formula = (self.out / "tsk.rb").read_text()
        checksums = (self.out / "SHA256SUMS").read_text()
        for target in release.TARGETS:
            name = f"tsk-v1.2.3-{target}.tar.gz"
            archive = self.out / name
            digest = hashlib.sha256(archive.read_bytes()).hexdigest()
            self.assertIn(f"/releases/download/v1.2.3/{name}", formula)
            self.assertIn(digest, formula)
            self.assertIn(f"{digest}  {name}", checksums)
            with tarfile.open(archive) as contents:
                self.assertEqual(contents.getnames(), ["tsk", "LICENSE", "README.md"])
                self.assertEqual(contents.getmember("tsk").mode, 0o755)
        self.assertNotIn("/latest/", formula)
        self.assertNotIn("/main/", formula)
        self.assertIn('version "1.2.3"', formula)
        self.assertIn('bin.install "tsk"', formula)
        self.assertIn('def caveats', formula)
        self.assertIn('tsk setup herdr', formula)
        self.assertIn('TSK_STATE_DIR', formula)

    def test_formula_platform_blocks_pair_the_correct_url_and_digest(self):
        for target in release.TARGETS:
            self.binary.write_text(f"distinct fixture for {target}")
            release.package("v1.2.3", target, self.binary, self.out)
        release.assemble("v1.2.3", self.out)
        formula = (self.out / "tsk.rb").read_text()
        expected = {
            "macos": {"arm": "aarch64-apple-darwin", "intel": "x86_64-apple-darwin"},
            "linux": {"arm": "aarch64-unknown-linux-musl", "intel": "x86_64-unknown-linux-musl"},
        }
        for system, cpus in expected.items():
            os_blocks = re.findall(rf"^  on_{system} do\n(.*?)^  end$", formula, re.M | re.S)
            self.assertEqual(len(os_blocks), 1)
            for cpu, target in cpus.items():
                with self.subTest(system=system, cpu=cpu):
                    blocks = re.findall(rf"^    on_{cpu} do\n(.*?)^    end$", os_blocks[0], re.M | re.S)
                    self.assertEqual(len(blocks), 1)
                    archive = self.out / f"tsk-v1.2.3-{target}.tar.gz"
                    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
                    self.assertEqual(re.findall(r'url "([^"\n]+)"', blocks[0]), [f"https://github.com/smarzban/herdr-tsk/releases/download/v1.2.3/{archive.name}"])
                    self.assertEqual(re.findall(r'sha256 "([^"\n]+)"', blocks[0]), [digest])

    def test_installer_asset_is_exact_and_has_one_matching_checksum(self):
        for target in release.TARGETS:
            release.package("v1.2.3", target, self.binary, self.out)
        release.assemble("v1.2.3", self.out)
        installed_script = (self.out / "install.sh").read_bytes()
        self.assertEqual(installed_script, (ROOT / "site/public/install.sh").read_bytes())
        entries = [line.split() for line in (self.out / "SHA256SUMS").read_text().splitlines()]
        self.assertEqual([entry for entry in entries if entry[-1] == "install.sh"], [[hashlib.sha256(installed_script).hexdigest(), "install.sh"]])

    def test_assemble_refuses_invalid_archive_contents_before_output(self):
        for target in release.TARGETS:
            release.package("v1.2.3", target, self.binary, self.out)
        archive = self.out / f"tsk-v1.2.3-{release.TARGETS[0]}.tar.gz"
        for invalid in ["missing", "extra", "traversal", "symlink", "hardlink", "directory", "empty", "nonexecutable"]:
            with self.subTest(invalid=invalid):
                names = ["tsk", "LICENSE", "README.md"]
                if invalid == "missing": names.pop()
                if invalid == "extra": names.append("unexpected")
                if invalid == "traversal": names[0] = "../tsk"
                with tarfile.open(archive, "w:gz") as bundle:
                    for index, name in enumerate(names):
                        info = tarfile.TarInfo(name)
                        data = b"fixture"
                        info.mode = 0o755 if index == 0 else 0o644
                        if index == 0:
                            if invalid == "empty": data = b""
                            if invalid == "nonexecutable": info.mode = 0o644
                            if invalid in ("symlink", "hardlink", "directory"):
                                info.type = {"symlink": tarfile.SYMTYPE, "hardlink": tarfile.LNKTYPE, "directory": tarfile.DIRTYPE}[invalid]
                                info.linkname = "LICENSE" if invalid != "directory" else ""
                        info.size = len(data) if info.isfile() else 0
                        bundle.addfile(info, io.BytesIO(data) if info.isfile() else None)
                with self.assertRaisesRegex(ValueError, "unexpected archive contents|invalid executable"):
                    release.assemble("v1.2.3", self.out)
                for name in ["tsk.rb", "SHA256SUMS", "install.sh"]:
                    self.assertFalse((self.out / name).exists())

    def test_invalid_tags_targets_and_missing_platform_refused(self):
        for tag in ["main", "v1.2.3/evil", "v1.2.3-rc1"]:
            with self.assertRaises(ValueError):
                release.package(tag, release.TARGETS[0], self.binary, self.out)
        with self.assertRaises(ValueError):
            release.package("v1.2.3", "windows", self.binary, self.out)
        release.package("v1.2.3", release.TARGETS[0], self.binary, self.out)
        with self.assertRaises(ValueError):
            release.assemble("v1.2.3", self.out)
        self.assertFalse((self.out / "tsk.rb").exists())

    def test_package_refuses_overwrite_and_nonexecutable(self):
        release.package("v1.2.3", release.TARGETS[0], self.binary, self.out)
        with self.assertRaises(ValueError):
            release.package("v1.2.3", release.TARGETS[0], self.binary, self.out)
        self.binary.chmod(0o644)
        with self.assertRaises(ValueError):
            release.package("v1.2.3", release.TARGETS[1], self.binary, self.out)

    def test_version_must_match_manifest(self):
        manifest = self.root / "Cargo.toml"
        manifest.write_text('[package]\nversion = "1.2.3"\n')
        release.check_version("v1.2.3", manifest)
        with self.assertRaises(ValueError):
            release.check_version("v1.2.4", manifest)


if __name__ == "__main__":
    unittest.main()
