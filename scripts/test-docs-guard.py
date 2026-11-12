#!/usr/bin/env python3
"""The pre-commit bootstrap rejects a validator that is not the staged one.

Specification: the documentation contract's staged-validator rule. Each test
builds a real temporary Git repository holding a copy of the module closure,
because the index and the working tree can only diverge in Git: a
filesystem-only check cannot reproduce the failure this guard exists for.

Exit 0 is a match, 1 is a mismatch, 2 is a tool error. A mismatch names the path
and the command that fixes it, and never stages or writes anything itself.
"""

import ast
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import docs_guard
import docs_test_support as support

CLOSURE = (
    "scripts/docs.py",
    "scripts/docs_destinations.py",
    "scripts/docs_frontmatter.py",
    "scripts/docs_policy.py",
    "scripts/docs_snapshot.py",
    "scripts/docs_index.py",
    "scripts/docs_links.py",
    "scripts/docs_guard.py",
)

INTERPRETER = sys.executable


def module_import_closure():
    """The `docs_*` modules `scripts/docs.py` reaches through module-level imports.

    Only the module body is walked. A lazy import inside a function runs after
    the guard has finished, so it is not part of the bootstrap closure; that is
    the split the advisory report's parser follows for the same reason.
    """
    pending = ["scripts/docs.py"]
    closure = set()
    while pending:
        path = pending.pop()
        if path in closure:
            continue
        closure.add(path)
        tree = ast.parse((support.REPO_ROOT / path).read_text(encoding="utf-8"))
        imports = []
        for node in tree.body:
            if isinstance(node, ast.Import):
                imports.extend(alias.name for alias in node.names)
            elif isinstance(node, ast.ImportFrom):
                imports.append(node.module or "")
        pending.extend("scripts/%s.py" % name for name in imports if name.startswith("docs_"))
    return closure


class ClosureTests(unittest.TestCase):
    def test_the_declared_closure_is_the_real_import_closure(self):
        declared = set(docs_guard.MODULES)
        self.assertEqual(len(declared), len(docs_guard.MODULES), "a module is declared twice")
        self.assertEqual(declared, module_import_closure() | {"scripts/docs_guard.py"})
        for name in declared:
            self.assertTrue((support.REPO_ROOT / name).is_file(), "%s does not exist" % name)


class GuardCase(unittest.TestCase):
    """A fixture repository holding a copy of the closure, as a checkout would."""

    def setUp(self):
        self.repo = support.TempRepository(self)
        self.write_closure()
        self.repo.commit()

    def write_closure(self, skip=()):
        for name in CLOSURE:
            if name in skip:
                continue
            target = self.repo.root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes((support.REPO_ROOT / name).read_bytes())

    def append_module(self, name, line):
        target = self.repo.root / name
        target.write_bytes(target.read_bytes() + line.encode("utf-8"))

    def replace_in_module(self, name, old, new):
        target = self.repo.root / name
        text = target.read_bytes().decode("utf-8")
        self.assertIn(old, text)
        target.write_bytes(text.replace(old, new, 1).encode("utf-8"))

    def run_guard(self, root=None):
        root = str(root or self.repo.root)
        return subprocess.run(
            [INTERPRETER, str(support.SCRIPTS / "docs_guard.py"), "--root", root],
            cwd=root, capture_output=True, text=True, env=self.repo.env,
        )

    def assertGuardFails(self, result, *fragments):
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        for fragment in fragments:
            self.assertIn(fragment, result.stderr)


class DivergenceTests(GuardCase):
    def test_a_working_tree_edit_the_index_does_not_hold_is_rejected(self):
        self.append_module("scripts/docs.py", "# an unstaged edit\n")
        result = self.run_guard()
        self.assertGuardFails(result, "scripts/docs.py", "fix: git add scripts/docs.py")

    def test_a_module_that_is_not_in_the_index_is_rejected(self):
        self.repo.git("rm", "-q", "--cached", "scripts/docs_index.py")
        result = self.run_guard()
        self.assertGuardFails(result, "scripts/docs_index.py", "is not in the Git index",
                              "fix: git add scripts/docs_index.py")

    def test_a_module_missing_from_the_working_tree_is_rejected(self):
        self.repo.remove("scripts/docs_snapshot.py")
        result = self.run_guard()
        self.assertGuardFails(result, "scripts/docs_snapshot.py",
                              "missing from the working tree",
                              "fix: git checkout -- scripts/docs_snapshot.py")

    def test_an_unmerged_module_is_rejected(self):
        line = 'DEFAULT_POLICY = "scripts/docs-policy.json"'
        self.repo.git("checkout", "-q", "-b", "other")
        self.replace_in_module("scripts/docs.py", line, 'DEFAULT_POLICY = "other.json"')
        self.repo.commit("other")
        self.repo.git("checkout", "-q", "-")
        self.replace_in_module("scripts/docs.py", line, 'DEFAULT_POLICY = "default.json"')
        self.repo.commit("default")
        self.repo.git("merge", "other", check=False)

        result = self.run_guard()
        self.assertGuardFails(result, "scripts/docs.py", "unmerged",
                              "resolve the merge conflict for scripts/docs.py")

    def test_a_crlf_checkout_of_the_same_content_passes(self):
        """`core.autocrlf` stores LF in the index and writes CRLF on disk."""
        self.repo.git("config", "core.autocrlf", "true")
        for name in CLOSURE:
            target = self.repo.root / name
            # Normalise first: the fixture's own checkout may hold either ending.
            data = target.read_bytes().replace(b"\r\n", b"\n")
            target.write_bytes(data.replace(b"\n", b"\r\n"))
        # Staging normalises back to the index's LF bytes, so this stages nothing;
        # the point of the case is the CRLF working copy above.
        self.repo.git("add", "-A")
        self.assertIn(b"\r\n", (self.repo.root / "scripts/docs.py").read_bytes())
        self.assertNotIn(b"\r", self.repo.git("show", ":scripts/docs.py").encode("utf-8"))

        result = self.run_guard()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_a_changed_file_outside_the_closure_is_ignored(self):
        self.repo.write("README.md", "# A working-tree edit\n")
        self.repo.write("docs/adr/0002-b.md", "# Staged but outside the closure\n")
        self.repo.git("add", "docs/adr/0002-b.md")

        result = self.run_guard()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_a_missing_repository_is_a_tool_error(self):
        holder = tempfile.TemporaryDirectory()
        self.addCleanup(holder.cleanup)
        result = self.run_guard(root=holder.name)
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("docs-guard:", result.stderr)
        self.assertNotIn("fix:", result.stderr)


class HookPathTests(unittest.TestCase):
    """The hook's two commands, in the hook's order, against one staged state."""

    def setUp(self):
        # The fixture activates the shipped `docs/adr` group, so `generate`
        # writes the index this fixture checks against.
        self.repo = support.TempRepository(
            self, support.policy(active_directories=["docs/adr"]))
        for name in CLOSURE:
            target = self.repo.root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes((support.REPO_ROOT / name).read_bytes())
        self.repo.write("docs/adr/0001-a.md",
                        support.document("Alpha", "A fixture record.", "prototype"))
        self.assertHookRun("scripts/docs.py", "generate", expected=0)
        self.repo.commit()

    def document(self, summary, extra):
        text = support.document("Alpha", summary, "prototype", extra)
        self.repo.write("docs/adr/0001-a.md", text)

    def assertHookRun(self, *args, expected):
        completed = subprocess.run(
            [INTERPRETER, *args], cwd=self.repo.root, env=self.repo.env,
            capture_output=True, text=True,
        )
        self.assertEqual(completed.returncode, expected,
                         completed.stdout + completed.stderr)
        return completed

    def test_the_guard_accepts_the_closure_and_the_staged_check_judges_the_index(self):
        self.document("The staged record.", "owner: someone\n")
        self.repo.git("add", "docs/adr/0001-a.md")
        # The working-tree copy differs from the staged one, so only a check that
        # reads the index can report the staged field.
        self.document("The worktree record.", "audience: someone\n")

        self.assertHookRun("scripts/docs_guard.py", expected=0)
        staged = self.assertHookRun("scripts/docs.py", "check", "--staged", expected=1)
        self.assertIn("docs/adr/0001-a.md", staged.stderr)
        self.assertIn("unknown field 'owner'", staged.stderr)
        self.assertNotIn("audience", staged.stderr)


class HookOrderTests(unittest.TestCase):
    """Pin the command order in `.githooks/pre-commit` by reading it.

    This proves the four commands appear in the required order in the file. It
    does not prove the shell reaches each command on every commit path, and it
    does not run them; `HookPathTests` and the other suites do that.
    """

    def setUp(self):
        self.hook = (support.REPO_ROOT / ".githooks/pre-commit").read_text(encoding="utf-8")

    def test_the_size_check_precedes_the_guard_which_precedes_the_staged_check(self):
        order = (
            "scripts/check-staged-token-limit.py",
            "scripts/docs_guard.py",
            "scripts/docs.py check --staged",
            "cargo fmt --all -- --check",
        )
        positions = [self.hook.find(command) for command in order]
        self.assertNotIn(-1, positions, "a hook command is missing: %r" % (order,))
        self.assertEqual(positions, sorted(positions), "the hook runs them out of order")

    def test_the_hook_never_stages_or_commits(self):
        self.assertNotIn("git add", self.hook)
        self.assertNotIn("git commit", self.hook)


if __name__ == "__main__":
    unittest.main()
