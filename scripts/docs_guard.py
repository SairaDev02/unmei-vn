"""Pre-commit bootstrap: the validator on disk must be the validator in the index.

The hook runs `scripts/docs.py` from the working tree while it validates the
*staged* snapshot. An unstaged edit to a validator module would judge the staged
documents with code that is not being committed, so this guard compares the two
first, before any validator module is imported.

The closure is explicit (:data:`MODULES`) because discovering it by importing
`docs.py` would run the code under test. Each module's index blob
(`git show :<path>`) is compared with its working-tree file after normalising
CRLF to LF: this clone checks out with `core.autocrlf=true`, so the raw bytes
legitimately differ for content Git considers identical. An index entry that is
missing, unmerged or object-less, and a working-tree file that is missing, are
each reported with the command that fixes that path.

This guards accidental mismatch, not a malicious bypass: it reads Git content
but cannot stop someone determined to edit it. CI independently checks the
committed implementation (`python3 scripts/docs.py check --checkout`), so an
edit hidden from this guard still has to survive the committed tree.

Exit status: 0 match, 1 mismatch, 2 tool error.
"""

import argparse
import subprocess
import sys
from pathlib import Path

# The modules the hook runs before and during the staged check, plus the guard
# itself. `docs_metrics.py` and `docs_report.py` are not listed: they arrive with
# the advisory report and are imported lazily inside the `report` command
# the hook never runs. A new module-level import in `scripts/docs.py` belongs
# here; `test-docs-guard.py` derives the closure with `ast` and fails until it is
# declared.
MODULES = (
    "scripts/docs.py",
    "scripts/docs_destinations.py",
    "scripts/docs_policy.py",
    "scripts/docs_frontmatter.py",
    "scripts/docs_snapshot.py",
    "scripts/docs_index.py",
    "scripts/docs_links.py",
    "scripts/docs_guard.py",
)

# `git add -N` records an intent-to-add entry with no object behind it, so there
# is no staged content to run: that counts as absent, not as a tool error.
NO_OBJECT = "0" * 40


class GuardError(Exception):
    """The comparison could not be performed."""


# Deliberately local: the guard compares validator code before any of it is
# executed, so it cannot import `docs_snapshot` for these two Git helpers. The
# duplication is the correctness boundary, not an oversight.
def git(root, *args):
    """Run one Git command, turning a failure into a tool error."""
    completed = subprocess.run(
        ["git", *args], cwd=str(root),
        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    if completed.returncode != 0:
        detail = completed.stderr.decode("utf-8", "replace").strip().splitlines()
        raise GuardError("git %s failed: %s" % (
            " ".join(args), detail[0] if detail else "no output"))
    return completed.stdout


def repository_root(start):
    """The working-tree root of the repository containing `start`."""
    return Path(git(start, "rev-parse", "--show-toplevel").decode("utf-8").strip()).resolve()


def index_entries(root):
    """The index records for the closure, keyed by path; a path may hold stages 1-3."""
    out = git(root, "ls-files", "--stage", "-z", "--", *MODULES).decode("utf-8")
    entries = {}
    for record in out.split("\0"):
        if not record:
            continue
        meta, _, path = record.partition("\t")
        _mode, sha, stage = meta.split(" ")
        entries.setdefault(path, []).append((sha, stage))
    return entries


def content(data):
    """Compare content, not checkout artefacts, so a CRLF working copy still matches."""
    return data.replace(b"\r\n", b"\n")


def problem_for(root, path, entries):
    """The mismatch to report for `path`, or None when the working-tree copy is the staged one."""
    records = entries.get(path, [])
    if not records:
        return "%s is not in the Git index\n  fix: git add %s" % (path, path)
    unmerged = sorted({stage for _sha, stage in records if stage != "0"})
    if unmerged:
        return ("%s is unmerged in the Git index (stages %s)\n"
                "  fix: resolve the merge conflict for %s"
                % (path, ", ".join(unmerged), path))
    if records[0][0] == NO_OBJECT:
        return "%s has no staged content yet\n  fix: git add %s" % (path, path)
    try:
        working = (root / path).read_bytes()
    except FileNotFoundError:
        return ("%s is in the Git index but missing from the working tree\n"
                "  fix: git checkout -- %s" % (path, path))
    if content(git(root, "show", ":" + path)) == content(working):
        return None
    return ("%s differs between the working tree and the Git index\n"
            "  fix: git add %s" % (path, path))


def parse_args(argv):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--root", default=None,
                        help="repository root (default: the repository holding the cwd)")
    return parser.parse_args(argv)


def main(argv):
    args = parse_args(argv)
    try:
        root = Path(args.root).resolve() if args.root else repository_root(Path.cwd())
        entries = index_entries(root)
        problems = [problem for problem in
                    (problem_for(root, path, entries) for path in MODULES) if problem]
    except (GuardError, OSError) as exc:
        sys.stderr.write("docs-guard: %s\n" % exc)
        return 2
    if problems:
        for problem in problems:
            sys.stderr.write(problem + "\n")
        sys.stderr.write("docs-guard: %d module%s not staged as they are on disk; "
                         "nothing was staged or written.\n"
                         % (len(problems), "" if len(problems) == 1 else "s"))
        return 1
    sys.stdout.write("docs-guard: %d validator modules match the Git index.\n" % len(MODULES))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
