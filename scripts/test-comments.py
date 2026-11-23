#!/usr/bin/env python3
"""Logic tests for the comment gate: `no-new-comments.py` and `comment_scan.py`.

Every gate run happens in a throwaway repository, so these tests never read this
repository's tree or baseline. Standard library only, no network.
"""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
GATE = HERE / "no-new-comments.py"
sys.path.insert(0, str(HERE))

from comment_scan import in_scope, scan_rust, scan_yaml  # noqa: E402

RUST = "fn main() {\n    // keep: the order matters\n    let x = 1;\n}\n"
PYTHON = "x = 1\n# keep: why x is one\n"
SHELL = "#!/bin/sh\n# keep: why this runs\necho hi\n"
WORKFLOW = (
    "name: ci\n"
    "jobs:\n"
    "  check:\n"
    "    steps:\n"
    "      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1\n"
)


def git(repo: Path, *args: str) -> None:
    subprocess.run(["git", *args], cwd=repo, check=True, capture_output=True)


class Repo:
    def __init__(self, root: Path) -> None:
        self.root = root
        git(root, "init", "-q")

    def write(self, rel: str, text: str, track: bool = True) -> None:
        path = self.root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(text.encode("utf-8"))
        if track:
            git(self.root, "add", rel)

    def gate(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(GATE), *args],
            cwd=self.root, capture_output=True, text=True,
        )

    def baseline(self) -> dict[str, list[str]]:
        raw = json.loads((self.root / ".comment-baseline.json").read_text(encoding="utf-8"))
        return {path: digests.split() for path, digests in raw.items()}


class GateTest(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.repo = Repo(Path(self._tmp.name))
        self.repo.write("src/main.rs", RUST)
        self.repo.write("tool.py", PYTHON)
        self.repo.write("run.sh", SHELL)
        self.repo.write(".github/workflows/ci.yml", WORKFLOW)
        created = self.repo.gate("--tighten")
        self.assertEqual(created.returncode, 0, created.stdout + created.stderr)
        self.assert_green()

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def assert_green(self) -> subprocess.CompletedProcess[str]:
        run = self.repo.gate()
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        return run

    def assert_red(self, *names: str) -> subprocess.CompletedProcess[str]:
        run = self.repo.gate()
        self.assertEqual(run.returncode, 1, run.stdout + run.stderr)
        for name in names:
            self.assertIn(name, run.stdout)
        return run

    def test_new_line_comment_in_a_tracked_file_is_red(self) -> None:
        self.repo.write("src/main.rs", RUST.replace("let x", "// Set x.\n    let x"))
        self.assert_red("src/main.rs:3", "// Set x.")

    def test_new_hash_comment_is_red_in_python_and_shell(self) -> None:
        self.repo.write("tool.py", PYTHON + "# Section: tools.\n")
        self.repo.write("run.sh", SHELL + "# Print it.\n")
        self.assert_red("tool.py:3", "run.sh:4")

    def test_rustdoc_docstring_and_directives_are_exempt(self) -> None:
        self.repo.write("src/main.rs", "/// Entry point.\n" + RUST)
        self.repo.write(
            "tool.py",
            '"""Module docstring."""\nimport os  # noqa: F401\n' + PYTHON,
        )
        self.repo.write("run.sh", SHELL + "# shellcheck disable=SC2086\n")
        self.assert_green()

    def test_shebang_is_exempt(self) -> None:
        self.repo.write("new.py", "#!/usr/bin/env python3\nprint(1)\n")
        self.assert_green()

    def test_newly_tracked_file_without_an_entry_is_red(self) -> None:
        self.repo.write("src/lib.rs", "// fresh\nfn f() {}\n")
        self.assert_red("src/lib.rs:1", "baseline 0")

    def test_untracked_file_is_invisible(self) -> None:
        self.repo.write("scratch.rs", "// anything at all\n", track=False)
        self.assert_green()

    def test_net_zero_swap_is_red(self) -> None:
        swapped = RUST.replace("// keep: the order matters", "// Increment the counter.")
        self.repo.write("src/main.rs", swapped)
        self.assert_red("src/main.rs:2", "// Increment the counter.")

    def test_moving_a_comment_stays_green(self) -> None:
        moved = "// keep: the order matters\nfn main() {\n    let x = 1;\n}\n"
        self.repo.write("src/main.rs", moved)
        self.assert_green()

    def test_duplicating_an_existing_comment_is_red(self) -> None:
        self.repo.write("src/main.rs", RUST + "// keep: the order matters\n")
        self.assert_red("src/main.rs:5")

    def test_tighten_lowers_an_entry(self) -> None:
        self.repo.write("src/main.rs", "fn main() {\n    let x = 1;\n}\n")
        run = self.repo.gate("--tighten")
        self.assertEqual(run.returncode, 0, run.stdout)
        self.assertNotIn("src/main.rs", self.repo.baseline())
        self.assertEqual(len(self.repo.baseline()["tool.py"]), 1)

    def test_tighten_refuses_to_raise_an_entry(self) -> None:
        self.repo.write("tool.py", PYTHON + "# more\n")
        before = self.repo.baseline()
        run = self.repo.gate("--tighten")
        self.assertEqual(run.returncode, 1, run.stdout)
        self.assertIn("tool.py", run.stdout)
        self.assertEqual(self.repo.baseline(), before)

    def test_tighten_refuses_a_swap_without_reword(self) -> None:
        self.repo.write("tool.py", PYTHON.replace("why x is one", "x is one"))
        before = self.repo.baseline()
        run = self.repo.gate("--tighten")
        self.assertEqual(run.returncode, 1, run.stdout)
        self.assertIn("--reword tool.py", run.stdout)
        self.assertEqual(self.repo.baseline(), before)

    def test_reword_admits_a_swap_that_keeps_the_count(self) -> None:
        self.repo.write("tool.py", PYTHON.replace("why x is one", "x is one"))
        run = self.repo.gate("--tighten", "--reword", "tool.py")
        self.assertEqual(run.returncode, 0, run.stdout)
        self.assert_green()

    def test_reword_still_refuses_a_rise(self) -> None:
        self.repo.write("tool.py", PYTHON.replace("why x is one", "x is one") + "# extra\n")
        run = self.repo.gate("--tighten", "--reword", "tool.py")
        self.assertEqual(run.returncode, 1, run.stdout)

    def test_reword_is_scoped_to_the_named_file(self) -> None:
        self.repo.write("tool.py", PYTHON.replace("why x is one", "x is one"))
        self.repo.write("run.sh", SHELL.replace("why this runs", "runs"))
        run = self.repo.gate("--tighten", "--reword", "tool.py")
        self.assertEqual(run.returncode, 1, run.stdout)
        self.assertIn("run.sh", run.stdout)

    def test_tighten_refuses_a_new_file_with_comments(self) -> None:
        self.repo.write("src/lib.rs", "// fresh\nfn f() {}\n")
        before = self.repo.baseline()
        run = self.repo.gate("--tighten")
        self.assertEqual(run.returncode, 1, run.stdout)
        self.assertIn("--admit src/lib.rs", run.stdout)
        self.assertEqual(self.repo.baseline(), before)

    def test_admit_takes_in_the_named_file(self) -> None:
        self.repo.write("src/lib.rs", "// fresh\nfn f() {}\n")
        run = self.repo.gate("--tighten", "--admit", "src/lib.rs")
        self.assertEqual(run.returncode, 0, run.stdout)
        self.assertIn("admitted new file: src/lib.rs", run.stdout)
        self.assertEqual(len(self.repo.baseline()["src/lib.rs"]), 1)
        self.assert_green()

    def test_admit_is_scoped_to_the_named_file(self) -> None:
        self.repo.write("src/lib.rs", "// fresh\nfn f() {}\n")
        self.repo.write("src/other.rs", "// also fresh\nfn g() {}\n")
        run = self.repo.gate("--tighten", "--admit", "src/lib.rs")
        self.assertEqual(run.returncode, 1, run.stdout)
        self.assertIn("--admit src/other.rs", run.stdout)
        self.assertNotIn("src/lib.rs", self.repo.baseline())

    def test_a_rename_needs_admit_and_drops_the_old_entry(self) -> None:
        git(self.repo.root, "mv", "tool.py", "renamed.py")
        before = self.repo.baseline()["tool.py"]
        self.assertEqual(self.repo.gate("--tighten").returncode, 1)
        run = self.repo.gate("--tighten", "--admit", "renamed.py")
        self.assertEqual(run.returncode, 0, run.stdout)
        self.assertNotIn("tool.py", self.repo.baseline())
        self.assertEqual(self.repo.baseline()["renamed.py"], before)
        self.assert_green()

    def test_a_new_file_with_only_exempt_comments_needs_no_admit(self) -> None:
        self.repo.write("src/lib.rs", "/// Item doc.\nfn f() {}\n")
        self.repo.write("new.py", '#!/usr/bin/env python3\n"""Doc."""\nimport os  # noqa: F401\n')
        run = self.repo.gate("--tighten")
        self.assertEqual(run.returncode, 0, run.stdout)
        self.assertNotIn("src/lib.rs", self.repo.baseline())
        self.assertNotIn("new.py", self.repo.baseline())

    def test_admit_needs_tighten(self) -> None:
        run = self.repo.gate("--admit", "tool.py")
        self.assertEqual(run.returncode, 2, run.stdout + run.stderr)
        self.assertIn("--admit needs --tighten", run.stderr)

    def test_a_dependabot_pin_bump_needs_no_baseline_edit(self) -> None:
        before = self.repo.baseline()
        bumped = WORKFLOW.replace("3d3c42e5", "a1b2c3d4").replace("v7.0.1", "v7.1.0")
        self.repo.write(".github/workflows/ci.yml", bumped)
        self.assert_green()
        self.assertEqual(self.repo.baseline(), before)

    def test_a_yaml_version_comment_on_its_own_line_is_red(self) -> None:
        self.repo.write(".github/workflows/ci.yml", WORKFLOW + "# v2.9.3\n")
        self.assert_red(".github/workflows/ci.yml:6", "# v2.9.3")

    def test_a_yaml_trailing_version_comment_with_more_text_is_red(self) -> None:
        self.repo.write(
            ".github/workflows/ci.yml", WORKFLOW.replace("# v7.0.1", "# v7.0.2, pinned for the probe")
        )
        self.assert_red(".github/workflows/ci.yml:5", "# v7.0.2, pinned for the probe")

    def test_a_trailing_version_comment_outside_yaml_is_red(self) -> None:
        self.repo.write("tool.py", PYTHON + "y = 2  # v2.9.3\n")
        self.repo.write("run.sh", SHELL + "true  # v2.9.3\n")
        self.repo.write("pyproject.toml", 'name = "x"  # v2.9.3\n')
        self.assert_red("tool.py:3", "run.sh:4", "pyproject.toml:1")


class YamlVersionPinTest(unittest.TestCase):
    def scoped(self, src: str, path: str = ".github/workflows/ci.yml") -> list[str]:
        return [c.text for c in scan_yaml(path, src) if in_scope(c)]

    def test_a_trailing_version_pin_in_any_accepted_shape_is_out_of_scope(self) -> None:
        for body in ("# v7", "# v2.9.3", "# v1.0.6", "#   v2.9.3  "):
            with self.subTest(body=body):
                self.assertEqual(self.scoped(f"- uses: a@b {body}\n"), [])

    def test_a_trailing_comment_that_is_not_only_a_version_stays_in_scope(self) -> None:
        for body in ("# v2.9.3 pinned for the probe", "# v2.9.3, pinned for the probe",
                     "# version 2.9.3", "# v2.9.3-rc1"):
            with self.subTest(body=body):
                self.assertEqual(self.scoped(f"- uses: a@b {body}\n"), [body])

    def test_a_version_on_its_own_line_stays_in_scope(self) -> None:
        self.assertEqual(self.scoped("# v2.9.3\n"), ["# v2.9.3"])

    def test_both_yaml_suffixes_are_exempt(self) -> None:
        for path in (".github/workflows/ci.yml", "ci.yaml"):
            with self.subTest(path=path):
                self.assertEqual(self.scoped("- uses: a@b # v2.9.3\n", path), [])


class RustBlockTest(unittest.TestCase):
    def scoped(self, src: str) -> list[str]:
        return [c.text for c in scan_rust("x.rs", src) if in_scope(c)]

    def test_nested_block_comment_is_one_comment(self) -> None:
        src = "/* outer /* inner */ still outer */\nfn f() {}\n"
        self.assertEqual(self.scoped(src), ["/* outer /* inner */ still outer */"])

    def test_block_comment_spans_its_lines(self) -> None:
        [comment] = scan_rust("x.rs", "fn f() {}\n/* a\n   b */\nfn g() {}\n")
        self.assertEqual((comment.start, comment.end, comment.kind), (2, 3, "block"))

    def test_block_doc_comments_are_exempt(self) -> None:
        self.assertEqual(self.scoped("/** item doc */\n/*! module doc */\nfn f() {}\n"), [])


if __name__ == "__main__":
    unittest.main()
