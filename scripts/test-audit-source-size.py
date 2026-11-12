#!/usr/bin/env python3
"""Focused category and violation tests for the whole-tree size audit.

Fixtures commit their own ``scripts/check-staged-token-limit.py`` so the
audit's revision-pinned policy is the thing under test. Most use a stub that
exempts only ``checker-exempt/`` paths; one exercises the real checker policy.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

AUDIT = Path(__file__).with_name("audit-source-size.py")
CHECKER = Path(__file__).with_name("check-staged-token-limit.py")
STUB_CHECKER = (
    '"""Fixture policy: only paths under checker-exempt/ are excluded."""\n'
    "\n"
    "\n"
    "def excluded(path):\n"
    '    return path.startswith("checker-exempt/")\n'
)


class AuditSourceSizeTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.addCleanup(self.tempdir.cleanup)
        self.root = Path(self.tempdir.name)
        self.env = os.environ.copy()
        for key in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_COMMON_DIR"):
            self.env.pop(key, None)
        self.git("init", "-q")
        self.git("config", "user.name", "Test")
        self.git("config", "user.email", "test@example.org")
        self.write("scripts/check-staged-token-limit.py", STUB_CHECKER)
        self.git("add", "scripts/check-staged-token-limit.py")
        self.git("commit", "-qm", "fixture policy")

    def git(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["git", *args], cwd=self.root, env=self.env, check=True,
            capture_output=True, text=True,
        )

    def write(self, name: str, text: str) -> None:
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def size(self, name: str, size: int) -> None:
        self.write(name, "x" * size)

    def audit(self, rev: str = "HEAD") -> tuple[subprocess.CompletedProcess[str], dict]:
        json_path = self.root / "inventory.json"
        result = subprocess.run(
            [sys.executable, str(AUDIT), "--rev", rev, "--json", str(json_path)],
            cwd=self.root, env=self.env, capture_output=True, text=True,
        )
        self.assertTrue(json_path.exists(), f"audit produced no inventory: {result.stderr}")
        return result, json.loads(json_path.read_text(encoding="utf-8"))

    def exceptions(self, *entries: tuple[str, int]) -> None:
        payload = {
            "exceptions": [
                {
                    "path": path,
                    "bytes": size,
                    "reason": f"fixture exception for {path}",
                    "authorization": "fixture maintainer decision",
                }
                for path, size in entries
            ]
        }
        self.write(
            "scripts/source-size-exceptions.json", json.dumps(payload, indent=1) + "\n"
        )

    def test_revision_pinned_policy_and_scope_classification(self) -> None:
        files = {
            "engine/src/production.rs": 20_001,
            "engine/src/boundary.rs": 20_000,
            "app/src/tests.rs": 30_000,
            "app/examples/harness.rs": 20_001,
            "docs/guide.md": 30_000,
            "docs/evidence/run.log": 30_000,
            "checker-exempt/big.rs": 30_000,
            "scripts/tool.py": 10,
        }
        for path, size in files.items():
            self.size(path, size)
        self.git("add", ".")
        self.git("commit", "-qm", "baseline")

        result, inventory = self.audit()

        self.assertEqual(result.returncode, 1)
        categories = {f["path"]: f["category"] for f in inventory["files"]}
        self.assertEqual(categories["engine/src/production.rs"], "production")
        self.assertEqual(categories["engine/src/boundary.rs"], "production")
        self.assertEqual(categories["app/src/tests.rs"], "test")
        self.assertEqual(categories["app/examples/harness.rs"], "example")
        self.assertEqual(categories["docs/guide.md"], "documentation")
        self.assertEqual(categories["docs/evidence/run.log"], "evidence")
        self.assertEqual(categories["checker-exempt/big.rs"], "checker-excluded")
        self.assertEqual(categories["scripts/tool.py"], "production")
        # The fixture stub only exempts checker-exempt/; docs would be excluded
        # by the repository's live checker, so this proves the policy is pinned.
        self.assertTrue(inventory["policy"].endswith(":scripts/check-staged-token-limit.py"))
        self.assertIn(inventory["revision"], inventory["policy"])
        self.assertIn("engine/src/production.rs", result.stdout)
        self.assertIn("app/examples/harness.rs", result.stdout)
        violations = result.stdout.split("out of scope:")[0]
        self.assertNotIn("app/src/tests.rs", violations)
        self.assertNotIn("docs/guide.md", violations)
        self.assertNotIn("docs/evidence/run.log", violations)
        self.assertNotIn("checker-exempt/big.rs", violations)

    def test_real_policy_audits_document_named_production_source(self) -> None:
        self.write("scripts/check-staged-token-limit.py", CHECKER.read_text(encoding="utf-8"))
        self.size("app/src/failure/notice.rs", 20_001)
        self.size("NOTICE", 20_001)
        self.size("README.md", 20_001)
        self.git("add", ".")
        self.git("commit", "-qm", "real policy")

        result, inventory = self.audit()

        self.assertEqual(result.returncode, 1, result.stdout)
        files = {item["path"]: item for item in inventory["files"]}
        self.assertEqual(files["app/src/failure/notice.rs"]["category"], "production")
        self.assertFalse(files["app/src/failure/notice.rs"]["checker_excluded"])
        self.assertTrue(files["NOTICE"]["checker_excluded"])
        self.assertTrue(files["README.md"]["checker_excluded"])
        self.assertIn("app/src/failure/notice.rs", result.stdout)

    def test_clean_tree_reports_no_violations(self) -> None:
        self.size("engine/src/small.rs", 20_000)
        self.git("add", ".")
        self.git("commit", "-qm", "baseline")
        result, inventory = self.audit()
        self.assertEqual(result.returncode, 0)
        small = next(f for f in inventory["files"] if f["path"] == "engine/src/small.rs")
        self.assertEqual(small["bytes"], 20_000)
        self.assertIn("in-scope production violations: 0", result.stdout)

    def test_reads_the_pinned_tree_not_the_working_copy(self) -> None:
        self.size("engine/src/big.rs", 30_000)
        self.git("add", ".")
        self.git("commit", "-qm", "baseline")
        self.size("engine/src/big.rs", 10)
        result, inventory = self.audit()
        self.assertEqual(result.returncode, 1)
        big = next(f for f in inventory["files"] if f["path"] == "engine/src/big.rs")
        self.assertEqual(big["bytes"], 30_000)

    def test_recorded_exception_reports_but_does_not_fail(self) -> None:
        self.size("engine/src/big.rs", 30_000)
        self.exceptions(("engine/src/big.rs", 30_000))
        self.git("add", ".")
        self.git("commit", "-qm", "exception")

        result, inventory = self.audit()

        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertEqual(result.stdout.count("authorized exceptions: 1"), 1)
        self.assertEqual([e["path"] for e in inventory["exceptions"]], ["engine/src/big.rs"])
        self.assertEqual(inventory["stale_exceptions"], [])
        self.assertEqual(inventory["expired_exceptions"], [])
        # The exception is reported where it applies, not silently dropped.
        section = result.stdout.split("authorized exceptions:")[1]
        self.assertIn("engine/src/big.rs", section)
        self.assertIn("in-scope production violations: 0", result.stdout)

    def test_exception_beyond_its_recorded_bytes_still_fails(self) -> None:
        self.size("engine/src/big.rs", 30_000)
        self.exceptions(("engine/src/big.rs", 25_000))
        self.git("add", ".")
        self.git("commit", "-qm", "exception")

        result, inventory = self.audit()

        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout.count("authorized exceptions: 0"), 1)
        self.assertEqual([e["path"] for e in inventory["expired_exceptions"]], ["engine/src/big.rs"])
        self.assertIn("in-scope production violations: 1", result.stdout)

    def test_stale_exception_is_reported_and_passes(self) -> None:
        self.size("engine/src/small.rs", 19_000)
        self.exceptions(("engine/src/small.rs", 30_000))
        self.git("add", ".")
        self.git("commit", "-qm", "stale exception")

        result, inventory = self.audit()

        self.assertEqual(result.returncode, 0)
        self.assertEqual([e["path"] for e in inventory["stale_exceptions"]], ["engine/src/small.rs"])
        self.assertEqual(inventory["exceptions"], [])

    def test_exceptions_are_revision_pinned(self) -> None:
        self.size("engine/src/big.rs", 30_000)
        self.exceptions(("engine/src/big.rs", 30_000))
        self.git("add", ".")
        self.git("commit", "-qm", "exception")
        (self.root / "scripts/source-size-exceptions.json").unlink()
        self.git("add", "-A")
        self.git("commit", "-qm", "drop exception")

        with_exception, older = self.audit("HEAD~1")
        without, newer = self.audit("HEAD")

        self.assertEqual(with_exception.returncode, 0)
        self.assertEqual([e["path"] for e in older["exceptions"]], ["engine/src/big.rs"])
        # A revision that records no exception cannot inherit the current one.
        self.assertEqual(without.returncode, 1)
        self.assertEqual(newer["exceptions"], [])


if __name__ == "__main__":
    unittest.main()
