#!/usr/bin/env python3
"""Enforce an authored-file size budget against a change, never the whole tree.

The default mode reads the staged Git blobs only: what the commit will contain.
``--base`` switches to a revision range, which is how CI checks a pull request or
a push without a local hook.

A file may exceed the budget when a maintainer decision is recorded in
``scripts/source-size-exceptions.json``, which is read from the index (what the
commit will contain) and then from the working tree, or, in range mode, from the
revision being checked. The recorded byte count is a ceiling: a file that grows
past it is a violation again. Both modes and ``scripts/audit-source-size.py``
share one verdict, so an exception means one thing everywhere.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path, PurePosixPath

MAX_TOKENS = 5_000
MAX_BYTES = MAX_TOKENS * 4
EXCEPTIONS_PATH = "scripts/source-size-exceptions.json"
LOCKFILES = {
    "Cargo.lock", "package-lock.json", "npm-shrinkwrap.json", "pnpm-lock.yaml",
    "yarn.lock", "bun.lock", "bun.lockb", "uv.lock", "poetry.lock",
    "Pipfile.lock", "Gemfile.lock", "composer.lock", "pubspec.lock",
    "flake.lock", "mix.lock", "go.sum",
}
GENERATED_DIRS = {"generated", "__generated__", "vendor", "third_party", "snapshots"}
GENERATED_SUFFIXES = (
    ".pb.rs", ".g.rs", ".gen.rs", ".generated.rs",
    ".gen.ts", ".generated.ts", ".gen.js", ".generated.js",
    ".min.js", ".min.css",
)
BINARY_SUFFIXES = {
    ".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".ico",
    ".wav", ".ogg", ".mp3", ".flac", ".mp4", ".webm",
    ".woff", ".woff2", ".ttf", ".otf", ".pdf", ".zip",
    ".gz", ".wasm", ".exe", ".dll", ".so", ".dylib", ".bin",
}


DOC_DIRS = {"doc", "docs", "documentation"}
TEST_DIRS = {"test", "tests", "__tests__", "testing"}
DOC_SUFFIXES = {".md", ".mdx", ".rst", ".adoc", ".asciidoc", ".txt"}
DOC_NAMES = {
    "readme", "changelog", "contributing", "code_of_conduct",
    "security", "license", "notice", "authors", "copying",
}


def git(*args: str) -> bytes:
    return subprocess.run(
        ["git", *args], check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE
    ).stdout


def excluded(path: str) -> bool:
    parts = PurePosixPath(path).parts
    name = parts[-1]
    lower_name = name.lower()
    basename = PurePosixPath(lower_name).stem
    return (
        name in LOCKFILES
        or name.endswith(".lock")
        or any(part.lower() in DOC_DIRS | TEST_DIRS for part in parts[:-1])
        or PurePosixPath(lower_name).suffix in DOC_SUFFIXES
        or lower_name in DOC_NAMES
        or basename in {"test", "tests"}
        or basename.startswith(("test_", "test-", "spec_", "spec-"))
        or basename.endswith(("_test", "_tests", ".test", ".spec"))
        or any(part in GENERATED_DIRS for part in parts[:-1])
        or path.startswith("cli/tests/fixtures/")
        or name.endswith(GENERATED_SUFFIXES)
        or PurePosixPath(name).suffix.lower() in BINARY_SUFFIXES
    )


def load_exceptions(source: str, max_bytes: int = MAX_BYTES) -> list[dict]:
    """Parse a recorded source-size exception list.

    One schema for both consumers: this hook reads it from the index, and
    ``audit-source-size.py`` loads this function from the audited revision. Each
    entry names the path, the byte count the maintainer decision authorized, a
    reason, and optionally where the decision is recorded. The count is a
    ceiling: a file that grows past it is a violation again.
    """
    try:
        data = json.loads(source)
    except json.JSONDecodeError as exc:
        raise ValueError(f"cannot parse {EXCEPTIONS_PATH}: {exc}") from exc
    entries = data.get("exceptions") if isinstance(data, dict) else None
    if not isinstance(entries, list):
        raise ValueError(f"{EXCEPTIONS_PATH} must hold an 'exceptions' list")
    policy: list[dict] = []
    for raw in entries:
        if not isinstance(raw, dict):
            raise ValueError(f"{EXCEPTIONS_PATH}: every exception must be an object")
        path, recorded, reason = raw.get("path"), raw.get("bytes"), raw.get("reason")
        if not isinstance(path, str) or not path:
            raise ValueError(f"{EXCEPTIONS_PATH}: every exception needs a non-empty 'path'")
        if not isinstance(recorded, int) or recorded <= max_bytes:
            raise ValueError(f"{EXCEPTIONS_PATH}: {path!r} needs 'bytes' above {max_bytes:,}")
        if not isinstance(reason, str) or not reason.strip():
            raise ValueError(f"{EXCEPTIONS_PATH}: {path!r} needs a non-empty 'reason'")
        policy.append(
            {
                "path": path,
                "bytes": recorded,
                "reason": reason,
                "authorization": raw.get("authorization", ""),
            }
        )
    return policy


def exception_source() -> str | None:
    """The exception list the commit will contain: the index, else the worktree."""
    try:
        return git("show", f":{EXCEPTIONS_PATH}").decode("utf-8")
    except subprocess.CalledProcessError:
        pass
    try:
        return Path(EXCEPTIONS_PATH).read_text(encoding="utf-8")
    except OSError:
        return None


def staged_exceptions() -> list[dict]:
    source = exception_source()
    return [] if source is None else load_exceptions(source, MAX_BYTES)


def tree_exceptions(rev: str) -> list[dict]:
    """The exception list ``rev`` records, or none when it records none."""
    try:
        source = git("show", f"{rev}:{EXCEPTIONS_PATH}").decode("utf-8")
    except subprocess.CalledProcessError:
        return []
    return load_exceptions(source, MAX_BYTES)


def ceilings(entries: list[dict]) -> dict[str, int]:
    return {entry["path"]: entry["bytes"] for entry in entries}


def size_verdict(path: str, size: int, limits: dict[str, int]) -> tuple[str, int | None]:
    """The one size rule: skip, authorized under a recorded ceiling, or a violation."""
    if excluded(path) or size <= MAX_BYTES:
        return "skip", None
    ceiling = limits.get(path)
    if ceiling is not None and size <= ceiling:
        return "authorized", ceiling
    return "violation", ceiling


def sort_oversized(
    files: list[tuple[str, int]], limits: dict[str, int]
) -> tuple[list[tuple[str, int, int | None]], list[tuple[str, int, int]]]:
    """Return (violations, authorized) with the recorded ceiling when one applies."""
    violations: list[tuple[str, int, int | None]] = []
    authorized: list[tuple[str, int, int]] = []
    for path, size in files:
        verdict, ceiling = size_verdict(path, size, limits)
        if verdict == "authorized":
            authorized.append((path, size, ceiling))
        elif verdict == "violation":
            violations.append((path, size, ceiling))
    return violations, authorized


def index_files() -> list[tuple[str, int]]:
    """The staged authored files, with the size of the blob the commit will hold."""
    changed = set(git("diff", "--cached", "--name-only", "-z", "--no-ext-diff", "--diff-filter=ACMRT").split(b"\0"))
    files: list[tuple[str, int]] = []
    for entry in git("ls-files", "--stage", "-z").split(b"\0"):
        if not entry:
            continue
        metadata, sep, raw_path = entry.partition(b"\t")
        if not sep or raw_path not in changed:
            continue
        mode, oid, stage = metadata.split()
        if stage != b"0" or mode not in (b"100644", b"100755"):
            continue  # Unmerged entries, symlinks, and submodules are not authored files.
        files.append((os.fsdecode(raw_path), int(git("cat-file", "-s", oid.decode("ascii")).strip())))
    return files


def tree_files(rev: str, changed: set[bytes]) -> list[tuple[str, int]]:
    """The regular files of ``rev`` that the change touched, with their blob sizes."""
    files: list[tuple[str, int]] = []
    for entry in git("ls-tree", "-r", "-z", "--full-tree", rev).split(b"\0"):
        if not entry:
            continue
        metadata, sep, raw_path = entry.partition(b"\t")
        if not sep or raw_path not in changed:
            continue
        mode, kind, oid = metadata.split(b" ")
        if kind != b"blob" or mode not in (b"100644", b"100755"):
            continue
        files.append((os.fsdecode(raw_path), int(git("cat-file", "-s", oid.decode("ascii")).strip())))
    return files


def oversized_staged_files() -> tuple[list[tuple[str, int, int | None]], list[tuple[str, int, int]]]:
    """The staged change's violations and authorized files."""
    return sort_oversized(index_files(), ceilings(staged_exceptions()))


def oversized_changed_files(
    base: str, head: str
) -> tuple[list[tuple[str, int, int | None]], list[tuple[str, int, int]]]:
    """The change between ``base`` and ``head``: violations and authorized files."""
    changed = set(git(
        "diff", "--name-only", "-z", "--no-ext-diff", "--diff-filter=ACMRT", f"{base}...{head}"
    ).split(b"\0"))
    return sort_oversized(tree_files(head, changed), ceilings(tree_exceptions(head)))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--base", metavar="REV",
        help="check the change between REV and --head instead of the staged index",
    )
    parser.add_argument(
        "--head", metavar="REV", default=None,
        help="the revision to check with --base (default: HEAD)",
    )
    args = parser.parse_args(argv)
    if args.base is None and args.head is not None:
        parser.error("--head needs --base")
    ranged = args.base is not None
    label, scope = ("source-size", "changed") if ranged else ("pre-commit", "staged")
    try:
        if ranged:
            violations, authorized = oversized_changed_files(args.base, args.head or "HEAD")
        else:
            violations, authorized = oversized_staged_files()
    except (OSError, ValueError, subprocess.CalledProcessError) as exc:
        print(f"{label}: cannot check {scope} file sizes: {exc}", file=sys.stderr)
        return 2
    for path, size, ceiling in authorized:
        print(
            f"{label}: {path!r} ({size:,} bytes) is a recorded exception "
            f"(authorized up to {ceiling:,} bytes).",
            file=sys.stderr,
        )
    if not violations:
        return 0

    print(
        f"{label}: {scope} authored files exceed {MAX_TOKENS:,} tokens "
        f"({MAX_BYTES:,} bytes; tokens = bytes / 4):",
        file=sys.stderr,
    )
    for path, size, ceiling in violations:
        grown = "" if ceiling is None else f" — grew past its recorded {ceiling:,} bytes"
        print(
            f"  {path!r}: {size:,} bytes ({size / 4:,.2f} tokens){grown}",
            file=sys.stderr,
        )
    then = (
        "Commit the split files and push again."
        if ranged else
        "Stage the split files (git add <paths>) and retry the commit."
    )
    print(
        f"\nSplit each oversized file before {'merging' if ranged else 'committing'}:\n"
        "  1. Extract cohesive features, types, or helpers into separate source files.\n"
        "     Rust: move related declarations into a module (e.g. foo.rs), then\n"
        "     update mod/use declarations and call sites.\n"
        "  2. Update imports and tests; keep each authored production source file\n"
        "     at or below 20,000 bytes (5,000 tokens by bytes / 4).\n"
        f"  3. {then}\n"
        "     Test and documentation files are exempt from this size check.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
