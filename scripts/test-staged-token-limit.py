#!/usr/bin/env python3
"""Focused Git-index regression tests for the staged token limit."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

CHECKER = Path(__file__).with_name("check-staged-token-limit.py")


class StagedLimitTests(unittest.TestCase):
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

    def git(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["git", *args], cwd=self.root, env=self.env, check=True,
            capture_output=True, text=True,
        )

    def write(self, name: str, text: str) -> None:
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def check(self) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(CHECKER)], cwd=self.root, env=self.env,
            capture_output=True, text=True,
        )

    def exceptions(self, *entries: tuple[str, int], text: str | None = None) -> None:
        if text is None:
            payload = {
                "exceptions": [
                    {"path": path, "bytes": size, "reason": f"fixture exception for {path}"}
                    for path, size in entries
                ]
            }
            text = json.dumps(payload, indent=1) + "\n"
        self.write("scripts/source-size-exceptions.json", text)

    def test_exact_boundary_allowed_and_one_byte_over_rejected(self) -> None:
        self.write("engine/src/boundary.rs", "x" * 20_000)
        self.git("add", "engine/src/boundary.rs")
        self.assertEqual(self.check().returncode, 0)
        self.write("engine/src/boundary.rs", "x" * 20_001)
        self.git("add", "engine/src/boundary.rs")
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn("5,000.25 tokens", result.stderr)
        self.assertIn("Split each oversized file", result.stderr)
        self.assertIn("git add <paths>", result.stderr)
        self.assertIn("Test and documentation files are exempt", result.stderr)

    def test_reads_index_not_unstaged_worktree(self) -> None:
        self.write("app/src/short.rs", "a" * 100)
        self.git("add", "app/src/short.rs")
        self.write("app/src/short.rs", "x" * 30_000)
        self.assertEqual(self.check().returncode, 0)
        self.write("app/src/long.rs", "x" * 30_000)
        self.git("add", "app/src/long.rs")
        self.write("app/src/long.rs", "x" * 10)
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn("app/src/long.rs", result.stderr)
        self.assertNotIn("app/src/short.rs", result.stderr)

    def test_ignores_unstaged_and_deleted_files(self) -> None:
        self.write("new.rs", "x" * 30_000)
        self.assertEqual(self.check().returncode, 0)
        self.git("add", "new.rs")
        self.git("commit", "-qm", "baseline")
        (self.root / "new.rs").unlink()
        self.git("add", "-u")
        self.assertEqual(self.check().returncode, 0)

    def test_ignores_lockfiles_codegen_fixtures_and_assets(self) -> None:
        for path in (
            "Cargo.lock", "pnpm-lock.yaml", "src/generated/big.rs",
            "app/src/interface.pb.rs", "cli/tests/fixtures/output.ron",
            "assets/photo.png", "vendor/lib.rs",
        ):
            self.write(path, "x" * 30_000)
            self.git("add", path)
        self.assertEqual(self.check().returncode, 0)

    def test_ignores_all_test_locations_and_filenames(self) -> None:
        for path in (
            "app/src/tests.rs", "engine/tests/parser_integration.rs",
            "engine/src/parser/tests.rs", "cli/src/command_test.rs",
            "cli/src/command_tests.rs", "scripts/test-staged-token-limit.py",
            "scripts/test_check.py", "app/src/ui.test.ts",
            "app/src/ui.spec.ts", "examples/__tests__/demo.rs",
        ):
            self.write(path, "x" * 30_000)
            self.git("add", path)
        self.assertEqual(self.check().returncode, 0)

    def test_ignores_documentation_directories_and_files(self) -> None:
        for path in (
            "docs/spec.html", "engine/docs/rust_example.rs", "README.md",
            "manual/design.mdx", "CHANGELOG", "CONTRIBUTING.md",
            "examples/guide.rst", "docs/guide.rs", "CONTRIBUTING",
            "NOTICE", "NoTiCe.Md", "LICENSE.txt",
        ):
            self.write(path, "x" * 30_000)
            self.git("add", path)
        self.assertEqual(self.check().returncode, 0)

    def test_non_test_production_source_still_blocked(self) -> None:
        for path in ("engine/src/testimonials.rs", "cli/src/logic.rs"):
            self.write(path, "x" * 30_000)
            self.git("add", path)
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn("engine/src/testimonials.rs", result.stderr)
        self.assertIn("cli/src/logic.rs", result.stderr)

    def test_document_named_production_sources_are_not_exempt(self) -> None:
        paths = (
            "app/src/failure/notice.rs", "app/src/license.rs",
            "engine/src/security.rs", "scripts/readme.py", "src/changelog.ts",
            "src/contributing.js", "src/code_of_conduct.rs",
            "src/authors.py", "src/copying.sh",
        )
        for path in paths:
            self.write(path, "x" * 20_001)
        self.git("add", *paths)

        result = self.check()

        self.assertEqual(result.returncode, 1, result.stderr)
        for path in paths:
            with self.subTest(path=path):
                self.assertIn(path, result.stderr)

    def test_handles_renames_and_spaces_in_filenames(self) -> None:
        self.write("old.rs", "x" * 21_000)
        self.git("add", "old.rs")
        self.git("commit", "-qm", "baseline")
        self.git("mv", "old.rs", "new name with spaces.rs")
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn("new name with spaces.rs", result.stderr)

    @unittest.skipIf(os.name == "nt", "Windows filenames cannot contain newlines")
    def test_handles_newlines_in_filenames(self) -> None:
        self.write("old.rs", "x" * 21_000)
        self.git("add", "old.rs")
        self.git("commit", "-qm", "baseline")
        self.git("mv", "old.rs", "new name\nwith newline.rs")
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn("new name\\nwith newline.rs", result.stderr)

    def test_multiple_offending_files_are_reported(self) -> None:
        self.write("docs/spec.md", "x" * 24_000)
        self.write("engine/src/logic.rs", "x" * 23_000)
        self.git("add", "docs/spec.md", "engine/src/logic.rs")
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertNotIn("docs/spec.md", result.stderr)
        self.assertIn("engine/src/logic.rs", result.stderr)

    def test_recorded_exception_allows_a_staged_oversized_file(self) -> None:
        self.write("app/src/credentials.rs", "x" * 49_941)
        self.exceptions(("app/src/credentials.rs", 49_941))
        self.git("add", ".")
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("recorded exception", result.stderr)
        self.assertIn("app/src/credentials.rs", result.stderr)

    def test_exception_growth_past_recorded_bytes_is_rejected(self) -> None:
        self.write("app/src/credentials.rs", "x" * 49_941)
        self.exceptions(("app/src/credentials.rs", 40_000))
        self.git("add", ".")
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn("app/src/credentials.rs", result.stderr)
        self.assertIn("past its recorded", result.stderr)

    def test_exception_for_another_path_does_not_apply(self) -> None:
        self.write("app/src/credentials.rs", "x" * 30_000)
        self.exceptions(("app/src/other.rs", 30_000))
        self.git("add", ".")
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn("app/src/credentials.rs", result.stderr)

    def test_exception_list_is_read_from_the_index(self) -> None:
        self.write("app/src/credentials.rs", "x" * 30_000)
        self.exceptions(("app/src/credentials.rs", 30_000))
        self.git("add", ".")
        # The working tree list is looser than the staged one; the index wins,
        # because the index is what the commit will contain.
        self.exceptions(("app/src/credentials.rs", 60_000))
        self.assertEqual(self.check().returncode, 0)
        self.exceptions(text='{"exceptions": []}\n')
        self.assertEqual(self.check().returncode, 0)

    def test_malformed_exception_file_fails_closed(self) -> None:
        self.write("app/src/credentials.rs", "x" * 30_000)
        self.exceptions(("app/src/credentials.rs", 30_000), text="{not json\n")
        self.git("add", ".")
        result = self.check()
        self.assertEqual(result.returncode, 2)
        self.assertIn("cannot check staged file sizes", result.stderr)


if __name__ == "__main__":
    unittest.main()
