import hashlib
import importlib.util
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
        self.assertIn('TSK_STATE_DIR', formula)

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
