"""Pin the owner-approved draft-only release handoff without calling GitHub.

This deliberately guards the workflow's current one-line gh invocation. Changing
that handoff's shape requires updating/reviewing this safety test too.
"""
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / ".github/workflows/release.yml"


class WorkflowTests(unittest.TestCase):
    def test_release_handoff_creates_only_a_draft_for_an_existing_tag(self):
        source = WORKFLOW.read_text()
        commands = re.findall(r"^        run: (gh release create[^\n]+)$", source, re.M)
        self.assertEqual(len(commands), 1, "release creation must use the reviewed handoff")
        # Do not execute arbitrary future workflow shell in the test environment.
        self.assertEqual(commands[0], 'gh release create "$TAG" dist-release/* --verify-tag --draft --title "tsk $TAG" --notes-file packaging/RELEASE-NOTES.md')
        self.assertEqual(source.count("gh release "), 1)
        triggers = source.split("on:\n", 1)[1].split("\npermissions:", 1)[0]
        self.assertEqual(re.findall(r"^  ([a-z_]+):", triggers, re.M), ["workflow_dispatch"])
        self.assertEqual(source.count("ref: ${{ needs.resolve.outputs.commit }}"), 2)
        self.assertIn('test "$(gh api "repos/$GITHUB_REPOSITORY/commits/refs%2Ftags%2F$TAG" --jq .sha)" = "$RELEASE_COMMIT"', source)
        with tempfile.TemporaryDirectory(prefix="tsk-draft-handoff-") as temp:
            root = Path(temp)
            commands_dir = root / "bin"
            commands_dir.mkdir()
            mock = commands_dir / "gh"
            mock.write_text('#!/usr/bin/env python3\nimport json, os, pathlib, sys\npathlib.Path(os.environ["GH_CAPTURE"]).write_text(json.dumps(sys.argv[1:]))\n')
            mock.chmod(0o755)
            assets = root / "dist-release"
            assets.mkdir()
            names = ["tsk-v1.2.3-aarch64-apple-darwin.tar.gz", "SHA256SUMS", "tsk.rb", "install.sh"]
            for name in names:
                (assets / name).write_text("fixture")
            capture = root / "args.json"
            environment = dict(os.environ, PATH=f"{commands_dir}:{os.environ['PATH']}", TAG="v1.2.3", GH_CAPTURE=str(capture))
            for key in ["GH_TOKEN", "GITHUB_TOKEN"]:
                environment.pop(key, None)
            subprocess.run(["bash", "-eu", "-c", commands[0]], cwd=root, env=environment, check=True, capture_output=True)
            arguments = json.loads(capture.read_text())
            self.assertEqual(arguments[:3], ["release", "create", "v1.2.3"])
            self.assertEqual(set(arguments[3:3 + len(names)]), {f"dist-release/{name}" for name in names})
            self.assertEqual(arguments[3 + len(names):], ["--verify-tag", "--draft", "--title", "tsk v1.2.3", "--notes-file", "packaging/RELEASE-NOTES.md"])


if __name__ == "__main__":
    unittest.main()
