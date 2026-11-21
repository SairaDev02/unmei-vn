"""Fail when a change adds a comment, so the comment count can only fall.

`docs/agents/comments.md` owns what a comment is for. This gate enforces the
budget, not the judgement: it compares each file's comment count against a
checked-in baseline and reports where a new comment should live instead.

    python3 scripts/no-new-comments.py            # check (the CI gate)
    python3 scripts/no-new-comments.py --tighten  # lower the baseline

`--tighten` only ever lowers an entry. It refuses to raise one, so the baseline
cannot grow back through a convenient rewrite.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from comment_scan import Comment, in_scope, scan_tracked  # noqa: E402

BASELINE = Path(".comment-baseline.json")

WHERE_IT_GOES = (
    "A comment is not the place for it:\n"
    "  - a decision the code cannot express -> an ADR in docs/adr/\n"
    "  - a hack kept alive by its rationale  -> an issue on the tracker\n"
    "  - narration, a banner, a restated name -> deleted\n"
    "Rustdoc, Python docstrings and tool directives are exempt."
)


def tracked() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", "-z"], capture_output=True, text=True, check=True
    ).stdout
    return [p for p in out.split("\0") if p]


def counts(comments: list[Comment]) -> dict[str, int]:
    per: dict[str, int] = defaultdict(int)
    for comment in comments:
        per[comment.path] += 1
    return dict(per)


def load() -> dict[str, int]:
    if not BASELINE.exists():
        return {}
    return json.loads(BASELINE.read_text(encoding="utf-8"))


def write(baseline: dict[str, int]) -> None:
    text = json.dumps(dict(sorted(baseline.items())), indent=2) + "\n"
    BASELINE.write_text(text, encoding="utf-8")


def excess(current: dict[str, int], baseline: dict[str, int]) -> list[tuple[str, int, int]]:
    return [
        (path, count, baseline.get(path, 0))
        for path, count in sorted(current.items())
        if count > baseline.get(path, 0)
    ]


def report(over: list[tuple[str, int, int]], comments: list[Comment]) -> None:
    by_path: dict[str, list[Comment]] = defaultdict(list)
    for comment in comments:
        by_path[comment.path].append(comment)
    for path, count, allowed in over:
        print(f"error: {path}: {count} comment(s), baseline {allowed}")
        for comment in by_path[path][allowed:]:
            print(f"  {path}:{comment.start}: {comment.text.splitlines()[0][:100]}")
    print()
    print(WHERE_IT_GOES)
    print(f"\n{len(over)} file(s) above the baseline.")


def tighten(current: dict[str, int]) -> int:
    baseline = load()
    if not BASELINE.exists():
        write(current)
        print(f"baseline created: {sum(current.values())} comments in {len(current)} file(s)")
        return 0
    raised = [p for p, n in current.items() if p in baseline and n > baseline[p]]
    if raised:
        print("refusing to tighten: these files would need MORE comments than the baseline:")
        for path in raised:
            print(f"  {path}: {current[path]} > {baseline[path]}")
        print("\nDelete the new comment, or move its content to an ADR or an issue first.")
        return 1
    # A file the baseline has never counted is admitted, not raised: otherwise a
    # new file could hold no comment at all and the gate would be unsatisfiable.
    admitted = sorted(p for p in current if p not in baseline)
    merged = {p: n for p, n in baseline.items() if current.get(p, 0) > 0}
    for path, count in current.items():
        merged[path] = min(count, baseline.get(path, count))
    write(merged)
    for path in admitted:
        print(f"admitted new file: {path} ({current[path]} comment(s))")
    dropped = sum(baseline.values()) - sum(merged.values())
    print(f"baseline tightened: {sum(baseline.values())} -> {sum(merged.values())} comments ({dropped} shed)")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tighten", action="store_true", help="lower the baseline to the tree")
    args = parser.parse_args()

    comments = [c for c in scan_tracked(tracked()) if in_scope(c)]
    current = counts(comments)
    if args.tighten:
        return tighten(current)

    over = excess(current, load())
    if over:
        report(over, comments)
        return 1
    print(f"ok: {sum(current.values())} comments, within the {sum(load().values())}-comment baseline")
    return 0


if __name__ == "__main__":
    sys.exit(main())
