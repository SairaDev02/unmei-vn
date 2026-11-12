#!/usr/bin/env python3
"""Whole-tree source-size audit against a pinned Git revision.

It enumerates every tracked regular file in the Git tree, counts the **actual
Git blob bytes** of each (5,000 tokens = 20,000 bytes), and applies the
exclusion predicate **from the audited revision itself**
(``<rev>:scripts/check-staged-token-limit.py``), so a pinned run keeps the policy
that was in force at that revision. Only a revision that predates the checker
falls back to the working-tree copy. This is a read-only, manual audit: it never
reads or writes the index or the working tree, and it is not wired into CI or
the commit hook.

The checker's predicate exempts lockfiles, generated/codegen and vendored
directories, generated CLI fixtures, generated source suffixes and binary
assets; it also exempts test files and documentation. The report's per-file
categories (test, documentation, evidence, example) stay in the tool so a run
against an older revision still separates refactor targets from out-of-scope
baggage without weakening the checker.

Usage:
    python scripts/audit-source-size.py --rev <sha>
    python scripts/audit-source-size.py --rev <sha> --json inventory.json

Exit code is 1 when an in-scope production or developer-tooling file exceeds
20,000 bytes and is not covered by a recorded exception; 0 otherwise. Recorded
exceptions live in ``scripts/source-size-exceptions.json`` and are read from the
audited revision, so an older tree cannot inherit a later decision.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
import types
from pathlib import PurePosixPath

MAX_BYTES = 20_000
CHECKER = os.path.join(os.path.dirname(os.path.abspath(__file__)), "check-staged-token-limit.py")
CHECKER_REPO_PATH = "scripts/check-staged-token-limit.py"
EXCEPTIONS_REPO_PATH = "scripts/source-size-exceptions.json"
IN_SCOPE = ("production", "example")
REGULAR_MODES = {b"100644", b"100755"}
CODE_SUFFIXES = (".rs", ".py", ".sh")


def git(*args: str, input: bytes | None = None) -> bytes:
    return subprocess.run(
        ["git", *args], check=True, input=input, stdout=subprocess.PIPE, stderr=subprocess.PIPE
    ).stdout


def compile_checker(source: str, origin: str) -> types.ModuleType:
    """Compile the exclusion predicate from a checker's source text.

    One loader for both consumers: the audit compiles the checker a revision
    records, and `scripts/docs_metrics.py` compiles the one a candidate snapshot
    holds, so the report's categories cannot drift from this gate's rules.
    """
    sys.dont_write_bytecode = True  # Loading the policy must not leave cache files behind.
    module = types.ModuleType("staged_token_limit")
    module.__file__ = origin
    try:
        exec(compile(source, origin, "exec"), module.__dict__)
    except SyntaxError as exc:
        raise ValueError(f"cannot load checker policy from {origin!r}: {exc}") from exc
    return module


def checker_policy(sha: str) -> tuple[types.ModuleType, str, str]:
    """Compile the exclusion predicate that was in force at the audited revision.

    Reading the policy from the scanned tree keeps a pinned run reproducible
    when the working-tree checker changes later. A revision that predates the
    checker falls back to the local copy and says so on stderr.
    """
    origin = f"{sha}:{CHECKER_REPO_PATH}"
    try:
        source = git("show", origin).decode("utf-8")
    except subprocess.CalledProcessError:
        origin = CHECKER
        source = open(CHECKER, encoding="utf-8").read()
        print(f"audit-source-size: {sha} has no checker; using {CHECKER}", file=sys.stderr)
    try:
        module = compile_checker(source, origin)
    except ValueError as exc:
        raise SystemExit(str(exc)) from exc
    return module, origin, source


def tracked_regular_files(rev: str) -> list[tuple[str, str]]:
    entries: list[tuple[str, str]] = []
    for entry in git("ls-tree", "-r", "-z", "--full-tree", rev).split(b"\0"):
        if not entry:
            continue
        metadata, sep, raw_path = entry.partition(b"\t")
        if not sep:
            continue
        mode, kind, oid = metadata.split(b" ")
        if kind != b"blob" or mode not in REGULAR_MODES:
            continue  # Symlinks and submodules are not authored regular files.
        entries.append((os.fsdecode(raw_path), oid.decode("ascii")))
    return entries


def blob_sizes(oids: list[str]) -> dict[str, int]:
    output = git("cat-file", "--batch-check", input=b"\n".join(o.encode("ascii") for o in oids))
    sizes: dict[str, int] = {}
    for line in output.decode("ascii").splitlines():
        parts = line.split()
        if len(parts) == 3 and parts[1] == "blob":
            sizes[parts[0]] = int(parts[2])
    return sizes


def classify(path: str, checker_excluded: bool) -> str:
    """Split paths into checker exemptions, refactor scope, and out-of-scope baggage.

    The test/documentation/evidence/example categories come from the rule that
    no size requirement is imposed on tests or docs and from the repeated
    exemptions recorded beside it; they are reporting metadata, not additions to
    the checker's exclusion predicate.
    """
    if checker_excluded:
        return "checker-excluded"
    parts = PurePosixPath(path).parts
    name = parts[-1]
    if "tests" in parts[:-1] or name in {"tests.rs", "tests.py"} or name.startswith(
        ("test_", "test-")
    ) or name.endswith(("_test.rs", "_tests.rs")):
        return "test"
    if name.endswith(".md"):
        return "documentation"
    if path.startswith("docs/evidence/"):
        return "evidence"
    if path.startswith("app/examples/"):
        return "example"
    if path.startswith("examples/"):
        return "content"
    if path == ".githooks/pre-commit" or name.endswith(CODE_SUFFIXES):
        return "production"
    return "config"


def classify_paths(paths: list[str], checker: types.ModuleType) -> dict[str, str]:
    """The category `classify` records for each path under `checker`'s predicate.

    The one classifier: `scan` and the advisory report both go through
    `classify`, so a staged path is categorized by the same rules as its
    committed equivalent.
    """
    return {path: classify(path, bool(checker.excluded(path))) for path in paths}


def exceptions_policy(sha: str, checker: types.ModuleType) -> list[dict]:
    """Read the maintainer-authorized exceptions recorded at the audited revision.

    Like the exclusion predicate, the list is revision-pinned: a revision that
    records no exception has none, so auditing an older tree cannot inherit a
    later decision. The schema lives in the checker (``load_exceptions``), so the
    hook and this audit cannot drift; revisions whose checker predates that
    loader keep the frozen parser below so their captures still reproduce.
    """
    origin = f"{sha}:{EXCEPTIONS_REPO_PATH}"
    try:
        source = git("show", origin).decode("utf-8")
    except subprocess.CalledProcessError:
        return []
    loader = getattr(checker, "load_exceptions", None)
    if loader is not None:
        try:
            return loader(source, MAX_BYTES)
        except ValueError as exc:
            raise ValueError(f"cannot load {origin!r}: {exc}") from exc
    return _legacy_exceptions(source, origin)


def _legacy_exceptions(source: str, origin: str) -> list[dict]:
    """Frozen copy of the parser used before the checker owned the schema.

    It exists only so a receipt captured at such a revision still replays; do
    not extend it.
    """
    try:
        data = json.loads(source)
    except json.JSONDecodeError as exc:
        raise ValueError(f"cannot parse {origin!r}: {exc}") from exc
    entries = data.get("exceptions") if isinstance(data, dict) else None
    if not isinstance(entries, list):
        raise ValueError(f"{origin!r} must hold an 'exceptions' list")
    policy: list[dict] = []
    for raw in entries:
        if not isinstance(raw, dict):
            raise ValueError(f"{origin!r}: every exception must be an object")
        path, recorded, reason = raw.get("path"), raw.get("bytes"), raw.get("reason")
        if not isinstance(path, str) or not path:
            raise ValueError(f"{origin!r}: every exception needs a non-empty 'path'")
        if not isinstance(recorded, int) or recorded <= MAX_BYTES:
            raise ValueError(f"{origin!r}: {path!r} needs 'bytes' above {MAX_BYTES:,}")
        if not isinstance(reason, str) or not reason.strip():
            raise ValueError(f"{origin!r}: {path!r} needs a non-empty 'reason'")
        policy.append(
            {
                "path": path,
                "bytes": recorded,
                "reason": reason,
                "authorization": raw.get("authorization", ""),
            }
        )
    return policy


def apply_exceptions(
    files: list[dict], entries: list[dict]
) -> tuple[list[dict], list[dict], list[dict]]:
    """Sort recorded exceptions into applied, expired and stale.

    Applied: the exact path is in refactor scope, is still over the cap, and has
    not grown past the byte count the decision authorized. Expired: it grew past
    that count, so the file is a violation again. Stale: the entry no longer
    describes a violation (the file is compliant or gone); it must be removed,
    but it does not fail the audit.
    """
    by_path = {f["path"]: f for f in files}
    applied: list[dict] = []
    expired: list[dict] = []
    stale: list[dict] = []
    for entry in entries:
        target = by_path.get(entry["path"])
        record = dict(entry)
        record["observed_bytes"] = target["bytes"] if target else None
        if target is None or target["category"] not in IN_SCOPE or target["bytes"] <= MAX_BYTES:
            stale.append(record)
        elif target["bytes"] > entry["bytes"]:
            expired.append(record)
        else:
            record["oid"] = target["oid"]
            applied.append(record)
    return applied, expired, stale


def scan(rev: str) -> dict:
    sha = git("rev-parse", rev).decode("ascii").strip()
    checker, policy, policy_source = checker_policy(sha)
    entries = tracked_regular_files(sha)
    size_of = blob_sizes([oid for _, oid in entries])
    files = []
    for path, oid in entries:
        excluded = bool(checker.excluded(path))
        files.append(
            {
                "path": path,
                "oid": oid,
                "bytes": size_of[oid],
                "category": classify(path, excluded),
                "checker_excluded": excluded,
            }
        )
    files.sort(key=lambda item: item["path"])
    applied, expired, stale = apply_exceptions(files, exceptions_policy(sha, checker))
    return {
        "revision": sha,
        "max_bytes": MAX_BYTES,
        "policy": policy,
        "policy_sha256": hashlib.sha256(policy_source.encode("utf-8")).hexdigest(),
        "files": files,
        "exceptions": applied,
        "expired_exceptions": expired,
        "stale_exceptions": stale,
    }


def over_budget(scan_result: dict) -> list[dict]:
    return [f for f in scan_result["files"] if f["bytes"] > scan_result["max_bytes"]]


def render(scan_result: dict) -> str:
    files = scan_result["files"]
    over = over_budget(scan_result)
    # Applied exceptions are reported in their own section, not as violations.
    excepted = {item["path"] for item in scan_result["exceptions"]}
    by_category: dict[str, int] = {}
    for item in files:
        by_category[item["category"]] = by_category.get(item["category"], 0) + 1
    lines = [
        f"revision: {scan_result['revision']}",
        f"tracked regular files: {len(files)}",
    ]
    for category in sorted(by_category):
        lines.append(f"  {category}: {by_category[category]}")
    lines.append(f"over {scan_result['max_bytes']:,} bytes: {len(over)}")
    headings = {
        "production": "in-scope production violations",
        "example": "in-scope developer-tooling (examples) violations",
        "test": "out of scope: tests",
        "documentation": "out of scope: documentation",
        "evidence": "out of scope: committed evidence artefacts (not source)",
        "content": "out of scope: demo-pack content",
        "checker-excluded": "checker-excluded: lockfiles/generated/vendor/assets",
    }
    for category in ("production", "example"):
        rows = [f for f in over if f["category"] == category and f["path"] not in excepted]
        lines.append(f"\n{headings[category]}: {len(rows)}")
        for item in rows:
            excess = item["bytes"] - scan_result["max_bytes"]
            lines.append(
                f"  {item['bytes']:>7} +{excess:>6} {item['path']!r} {item['oid']}"
            )
    lines.append(f"\nauthorized exceptions: {len(scan_result['exceptions'])}")
    for item in scan_result["exceptions"]:
        lines.append(
            f"  {item['observed_bytes']:>7} (recorded {item['bytes']:>6}) "
            f"{item['path']!r} {item['oid']} — {item['reason']}"
        )
    lines.append(
        "\nexpired exceptions (grew past the recorded size, so violations again): "
        f"{len(scan_result['expired_exceptions'])}"
    )
    for item in scan_result["expired_exceptions"]:
        lines.append(
            f"  {item['observed_bytes']:>7} > {item['bytes']:>6} {item['path']!r} — {item['reason']}"
        )
    lines.append(
        "\nstale exceptions (no longer over budget; remove the entry): "
        f"{len(scan_result['stale_exceptions'])}"
    )
    for item in scan_result["stale_exceptions"]:
        observed = item["observed_bytes"]
        shown = "missing" if observed is None else f"{observed:>7}"
        lines.append(f"  {shown} <= {item['bytes']:>6} {item['path']!r} — {item['reason']}")
    for category in ("test", "documentation", "evidence", "content", "checker-excluded"):
        rows = [f for f in over if f["category"] == category]
        lines.append(f"\n{headings[category]}: {len(rows)}")
        for item in rows:
            excess = item["bytes"] - scan_result["max_bytes"]
            lines.append(
                f"  {item['bytes']:>7} +{excess:>6} {item['path']!r} {item['oid']}"
            )
    return "\n".join(lines) + "\n"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rev", default="HEAD", help="Git revision to audit (default: HEAD)")
    parser.add_argument("--json", metavar="PATH", help="write the full inventory as JSON")
    args = parser.parse_args(argv)

    try:
        result = scan(args.rev)
    except (OSError, ValueError, subprocess.CalledProcessError) as exc:
        print(f"audit-source-size: {exc}", file=sys.stderr)
        return 2

    sys.stdout.write(render(result))
    if args.json:
        with open(args.json, "w", encoding="utf-8") as handle:
            json.dump(result, handle, indent=1, ensure_ascii=False)
            handle.write("\n")
    applied = {item["path"] for item in result["exceptions"]}
    violations = [
        f
        for f in over_budget(result)
        if f["category"] in IN_SCOPE and f["path"] not in applied
    ]
    return 1 if violations else 0


if __name__ == "__main__":
    raise SystemExit(main())
