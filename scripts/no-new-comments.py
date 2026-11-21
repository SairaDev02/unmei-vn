"""Fail when a change adds or rewords a comment, so the comment set can only shrink.

`docs/agents/comments.md` owns what a comment is for. This gate enforces the
budget, not the judgement: it compares each file's comments against a checked-in
baseline of per-comment digests and reports where a new comment should live
instead. ADR 0016 records why the baseline holds digests rather than counts.

    python3 scripts/no-new-comments.py                          # check (the CI gate)
    python3 scripts/no-new-comments.py --tighten                # shrink the baseline
    python3 scripts/no-new-comments.py --tighten --reword PATH  # accept a reworded comment
    python3 scripts/no-new-comments.py --tighten --admit PATH   # accept a new file's comments

`--tighten` drops digests the tree no longer holds and refuses every new one, so
no comment enters silently. `--reword PATH` accepts new digests in a counted file
only while its count does not rise. `--admit PATH` accepts the comments of a file
the baseline has never counted, a renamed one included. Either way the change
shows in the baseline diff for review.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from collections import Counter, defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from comment_scan import Comment, in_scope, scan_tracked  # noqa: E402

BASELINE = Path(".comment-baseline.json")

WHERE_IT_GOES = (
    "A comment is not the place for it:\n"
    "  - a decision the code cannot express -> an ADR in docs/adr/\n"
    "  - a hack kept alive by its rationale  -> an issue on the tracker\n"
    "  - narration, a banner, a restated name -> deleted\n"
    "Rustdoc, Python docstrings and tool directives are exempt.\n"
    "A deliberate rewording: --tighten --reword PATH, reviewed in the baseline diff."
)


def tracked() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", "-z"], capture_output=True, text=True, check=True
    ).stdout
    return [p for p in out.split("\0") if p]


def digest(comment: Comment) -> str:
    """Identity of a comment, independent of its position and indentation."""
    return hashlib.sha256(" ".join(comment.text.split()).encode("utf-8")).hexdigest()[:8]


def digests(comments: list[Comment]) -> dict[str, list[str]]:
    per: dict[str, list[str]] = defaultdict(list)
    for comment in comments:
        per[comment.path].append(digest(comment))
    return {path: sorted(found) for path, found in per.items()}


def load() -> dict[str, list[str]]:
    if not BASELINE.exists():
        return {}
    raw = json.loads(BASELINE.read_text(encoding="utf-8"))
    return {path: found.split() for path, found in raw.items()}


def write(baseline: dict[str, list[str]]) -> None:
    flat = {path: " ".join(sorted(found)) for path, found in sorted(baseline.items())}
    BASELINE.write_bytes((json.dumps(flat, indent=2) + "\n").encode("utf-8"))


def new_in(current: list[str], allowed: list[str]) -> Counter[str]:
    return Counter(current) - Counter(allowed)


def excess(comments: list[Comment], baseline: dict[str, list[str]]) -> list[Comment]:
    """The comments whose digest the baseline does not hold, latest copy first claimed."""
    by_path: dict[str, list[Comment]] = defaultdict(list)
    for comment in comments:
        by_path[comment.path].append(comment)
    out = []
    for path, found in sorted(by_path.items()):
        extra = new_in([digest(c) for c in found], baseline.get(path, []))
        for comment in reversed(found):
            if extra[digest(comment)] > 0:
                extra[digest(comment)] -= 1
                out.append(comment)
    return sorted(out, key=lambda c: (c.path, c.start))


def report(over: list[Comment], baseline: dict[str, list[str]]) -> None:
    for path in sorted({c.path for c in over}):
        print(f"error: {path}: comment(s) not in the baseline (baseline {len(baseline.get(path, []))})")
        for comment in (c for c in over if c.path == path):
            print(f"  {path}:{comment.start}: {comment.text.splitlines()[0][:100]}")
    print()
    print(WHERE_IT_GOES)
    print(f"\n{len(over)} comment(s) not in the baseline.")


def tighten(current: dict[str, list[str]], reword: set[str], admit: set[str]) -> int:
    if not BASELINE.exists():
        write(current)
        print(f"baseline created: {sum(map(len, current.values()))} comments in {len(current)} file(s)")
        return 0
    baseline = load()
    raised, reworded, unadmitted = [], [], []
    for path, found in sorted(current.items()):
        if path not in baseline:
            if path not in admit:
                unadmitted.append(f"  {path}: {len(found)} comment(s); rerun with --admit {path} once each meets the policy")
            continue
        if not new_in(found, baseline[path]):
            continue
        if len(found) > len(baseline[path]):
            raised.append(f"  {path}: {len(found)} > {len(baseline[path])}")
        elif path not in reword:
            reworded.append(f"  {path}: rerun with --reword {path} if the rewording is deliberate")
    if raised or reworded or unadmitted:
        if unadmitted:
            print("refusing to tighten: these files are new to the baseline and hold comments:")
            print("\n".join(unadmitted))
        if raised:
            print("refusing to tighten: these files would need MORE comments than the baseline:")
            print("\n".join(raised))
        if reworded:
            print("refusing to tighten: these files hold a comment the baseline does not:")
            print("\n".join(reworded))
        print("\nDelete the new comment, or move its content to an ADR or an issue first.")
        return 1
    for path in sorted(p for p in current if p not in baseline):
        print(f"admitted new file: {path} ({len(current[path])} comment(s))")
    write(current)
    before, after = sum(map(len, baseline.values())), sum(map(len, current.values()))
    print(f"baseline tightened: {before} -> {after} comments")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tighten", action="store_true", help="shrink the baseline to the tree")
    parser.add_argument("--reword", action="append", default=[], metavar="PATH",
                        help="with --tighten, accept new comments in PATH while its count does not rise")
    parser.add_argument("--admit", action="append", default=[], metavar="PATH",
                        help="with --tighten, accept the comments of PATH, a file the baseline has never counted")
    args = parser.parse_args()
    if args.reword and not args.tighten:
        parser.error("--reword needs --tighten")
    if args.admit and not args.tighten:
        parser.error("--admit needs --tighten")

    comments = [c for c in scan_tracked(tracked()) if in_scope(c)]
    if args.tighten:
        return tighten(digests(comments), set(args.reword), set(args.admit))

    baseline = load()
    over = excess(comments, baseline)
    if over:
        report(over, baseline)
        return 1
    print(f"ok: {len(comments)} comments, all in the {sum(map(len, baseline.values()))}-comment baseline")
    return 0


if __name__ == "__main__":
    sys.exit(main())
