#!/usr/bin/env python3
"""Publish one item-seven coverage measurement as the candidate record slot.

The method is owned by ``docs/testing-coverage-evidence.md`` and the checks by
``coverage_report.py``. ``slot`` writes the ``coverage`` slot of the candidate
record as JSON and as a job-summary rendering, so a
downstream record reads a status and a reason instead of an absent job.

Exit status is non-zero only for a failed check or unreadable input: a
measurement failure is the slot's own status, so an advisory coverage problem
never has to look like a broken workflow.
"""

import argparse
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import coverage_report  # noqa: E402

SLOT = "coverage"
METRIC_LABEL = "production-file coverage, including inline test code"
REPORT_FILTER = r"(^|/)(app|\.pi-tmp)/|(^|/)build\.rs$"
LIMITATIONS = (
    "`unmei-app` is excluded: no app, renderer or player source is measured, so this "
    "report says nothing about the Bevy runtime.",
    "Windows-only source is compiled out on the Ubuntu runner, so no Windows coverage "
    "is claimed.",
    "Only exported files count: a compiled file that emits no instrumented regions is "
    "absent from the denominator rather than zero-covered.",
    "A hit is not a meaningful assertion, and nothing gates on this number.",
)
# Each step's failure lands in the slot instead of failing the run. The first
# non-success outcome decides the slot: a test or witness failure means the
# measurement is not evidence, while a tool or export problem means it could
# not be established.
STEPS = (
    ("toolchain", "inconclusive", "the pinned 1.98.1 toolchain or llvm-tools-preview could not be installed"),
    ("install", "inconclusive", "cargo-llvm-cov 0.9.1 could not be installed or its crate digest did not check out"),
    ("witness", "failed", "the subprocess-capture witness failed, so the profile set is not evidence"),
    ("measure", "failed", "the intended non-ignored engine and CLI tests did not pass"),
    ("export", "inconclusive", "the reports could not be exported from the profile set"),
    ("verify", "failed", "the report views, the source allowlist or the arithmetic did not agree"),
)

CAPTURE = re.compile(r"capture proved by (\S+)")
CHECK = re.compile(r"witness target (\S+): (\d+)/(\d+) lines covered")
PROFILES_BEFORE = re.compile(r"profraw before \S+: (\d+)")
PROFILES_AFTER = re.compile(r"profraw after \S+: (\d+)")
CLEARED = re.compile(r"profraw now: (\d+)")


def read_text(path):
    try:
        return path.read_text(encoding="utf-8")
    except OSError:
        return ""


def witness_facts(directory):
    """Read the witness receipts the capture step left behind, or report not-run."""
    facts = {
        "status": "not-run",
        "proving_test": None,
        "file": None,
        "covered_lines": None,
        "total_lines": None,
        "negative_control": None,
        "child_profiles": None,
        "state_cleared": None,
    }
    if directory is None or not directory.is_dir():
        return facts
    capture = CAPTURE.search(read_text(directory / "CAPTURE.txt"))
    if capture:
        facts["proving_test"] = capture.group(1)
    for path in sorted(directory.glob("*-check.txt")):
        text = read_text(path)
        match = CHECK.search(text)
        if not match:
            continue
        covered, count = int(match.group(2)), int(match.group(3))
        if path.name == "negative-check.txt":
            facts["negative_control"] = {"file": match.group(1), "covered_lines": covered, "total_lines": count}
        elif "(expected covered)" in text:
            facts.update(file=match.group(1), covered_lines=covered, total_lines=count)
            profiles = read_text(directory / (path.name[: -len("-check.txt")] + "-profiles.txt"))
            before, after = PROFILES_BEFORE.search(profiles), PROFILES_AFTER.search(profiles)
            if before and after:
                facts["child_profiles"] = {"before": int(before.group(1)), "after": int(after.group(1))}
    cleared = CLEARED.search(read_text(directory / "cleared.txt"))
    if cleared:
        facts["state_cleared"] = int(cleared.group(1)) == 0
    negative = facts["negative_control"] or {}
    passed = (
        bool(facts["proving_test"])
        and (facts["covered_lines"] or 0) > 0
        and negative.get("covered_lines") == 0
        and facts["state_cleared"] is True
    )
    if facts["proving_test"] or facts["covered_lines"] is not None:
        facts["status"] = "passed" if passed else "failed"
    return facts


def slot_status(outcomes):
    for step, status, reason in STEPS:
        outcome = outcomes.get(step, "not-run")
        if outcome != "success":
            return status, "%s (step outcome: %s)" % (reason, outcome)
    return "passed", None


def slot_counts(args):
    if not args.json:
        return None, None
    raw, entry = coverage_report.load_json(args.json)
    files = coverage_report.check_allowlist(entry)
    counts = {
        scope: {
            name: {
                "covered": covered,
                "total": count,
                "percent": coverage_report.percent_value(covered, count),
            }
            for name, (covered, count) in metrics.items()
        }
        for scope, metrics in coverage_report.summed(files).items()
    }
    return counts, {
        "allowlisted_files": len(files),
        "cargo_llvm_cov": (raw.get("cargo_llvm_cov") or {}).get("version"),
    }


def markdown(payload):
    witness = payload["witness"]
    counts = payload["counts"]
    tool = payload["tool"]["cargo_llvm_cov"]
    lines = [
        "## Coverage slot — %s" % payload["status"],
        "",
        "Metric: **%s**. Advisory: nothing gates on it." % METRIC_LABEL,
        "",
        "- measured SHA: `%s`" % payload["measured_sha"],
        "- scope: `unmei-engine` (with `network`) and `unmei`, default test profile, "
        "`--lib --bins --tests`, no ignored or live tests",
        "- report allowlist: `engine/src/**`, `cli/src/**`",
        "- tool: cargo-llvm-cov %s (crate sha256 %s, %s), Rust %s with llvm-tools-preview"
        % (tool["version"] or "unavailable", tool["archive_sha256"] or "unrecorded",
           "digest verified" if tool["digest_verified"] else "digest NOT verified",
           payload["tool"]["toolchain"] or "unavailable"),
    ]
    if witness["status"] == "passed":
        lines.append(
            "- witness: passed by `%s` (%s %s/%s lines, negative control %s/%s, state cleared)"
            % (witness["proving_test"], witness["file"], witness["covered_lines"], witness["total_lines"],
               witness["negative_control"]["covered_lines"], witness["negative_control"]["total_lines"])
        )
    else:
        lines.append("- witness: %s" % witness["status"])
    lines.append("- bundle: `%s`, retained %d days"
                 % (payload["artifacts"]["bundle"] or "unnamed", payload["artifacts"]["retention_days"]))
    lines.append("")
    if counts is None:
        lines.append("Counts unavailable: no verified export was produced.")
    else:
        lines.extend(["| Scope | Lines | Functions | Regions |", "| --- | --- | --- | --- |"])
        for scope in ("aggregate", "unmei-engine", "unmei"):
            cells = []
            for name in coverage_report.METRICS:
                entry = counts[scope][name]
                share = "n/a" if entry["percent"] is None else "%.2f%%" % entry["percent"]
                cells.append("%d/%d — %s" % (entry["covered"], entry["total"], share))
            lines.append("| %s | %s |" % (scope, " | ".join(cells)))
    lines.extend(["", "Limitations:", ""])
    lines.extend("- %s" % limitation for limitation in LIMITATIONS)
    if payload["reason"]:
        lines.extend(["", "Reason: %s" % payload["reason"]])
    return "\n".join(lines) + "\n"


def slot(args):
    if args.json and not (args.summary and args.html):
        coverage_report.die("--json needs --summary and --html so the three views are checked against each other")
    counts, report = slot_counts(args)
    status, reason = slot_status(args.outcomes)
    observed = (args.tool_digest or "").strip().lower()
    expected = (args.tool_digest_expected or "").strip().lower()
    payload = {
        "slot": SLOT,
        "status": status,
        "reason": reason,
        "metric": METRIC_LABEL,
        "measured_sha": args.sha,
        "scope": {
            "packages": ["unmei-engine", "unmei"],
            "features": ["unmei-engine/network"],
            "cargo_targets": ["--lib", "--bins", "--tests"],
            "profile": "default test profile",
            "ignored_or_live_tests": "none selected: no --ignored, no --include-ignored",
            "report_allowlist": ["engine/src/**", "cli/src/**"],
            "report_filter": args.report_filter,
            "allowlisted_files": report["allowlisted_files"] if report else None,
            "packages_excluded": {"unmei-app": "outside the item-seven measured scope"},
        },
        "counts": counts,
        "tool": {
            "cargo_llvm_cov": {
                "version": (args.tool_version or "").strip() or (report["cargo_llvm_cov"] if report else None),
                "archive_sha256": observed or None,
                "expected_sha256": expected or None,
                "digest_verified": bool(observed) and observed == expected,
            },
            "toolchain": args.toolchain or None,
            # The one step installs the channel and the component, so its
            # success is what proves the preview tools are present.
            "llvm_tools_preview": args.outcomes.get("toolchain") == "success",
        },
        "witness": witness_facts(Path(args.witness_dir) if args.witness_dir else None),
        "runner": {
            "label": args.runner_label or "unavailable",
            "image": args.runner_image or "unavailable",
            "os_version": args.os_version or "unavailable",
            "arch": args.arch or "unavailable",
        },
        "limitations": list(LIMITATIONS),
        "artifacts": {
            "bundle": args.bundle_name or None,
            "slot": args.slot_artifact_name or (args.bundle_name + "-slot" if args.bundle_name else None),
            "retention_days": args.retention_days,
        },
        "step_outcomes": dict(args.outcomes),
    }
    text = json.dumps(payload, indent=2) + "\n"
    if args.out:
        with open(args.out, "w", encoding="utf-8") as handle:
            handle.write(text)
    else:
        print(text, end="")
    rendered = markdown(payload)
    if args.summary_out:
        with open(args.summary_out, "w", encoding="utf-8") as handle:
            handle.write(rendered)
    if status != "passed":
        # A runner annotation keeps an advisory result visible even though the
        # job stays green. Both lines go to stderr so `--out`-less stdout stays
        # the slot JSON alone.
        print("::warning::coverage slot %s: %s" % (status, reason), file=sys.stderr)
    print("coverage slot %s written for %s" % (status, args.sha), file=sys.stderr)
    return 0


def verify(args):
    text = coverage_report.verify_measurement(args.json, args.summary, args.html, METRIC_LABEL)
    print(text)
    if args.out:
        with open(args.out, "w", encoding="utf-8") as handle:
            handle.write(text)
    return 0


def witness(args):
    message = coverage_report.witness_check(args.json, args.file, args.expect)
    print(message)
    if args.out:
        with open(args.out, "w", encoding="utf-8") as handle:
            handle.write(message + "\n")
    return 0


def outcome_pair(text):
    step, separator, outcome = text.partition("=")
    if not separator or step not in {name for name, _status, _reason in STEPS}:
        raise argparse.ArgumentTypeError("%r is not <step>=<outcome> for this workflow" % text)
    return step, outcome


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="mode", required=True)

    wit = sub.add_parser("witness", help="check a single-test child-capture export")
    wit.add_argument("--json", required=True)
    wit.add_argument("--file", default="cli/src/main.rs")
    wit.add_argument("--expect", choices=("covered", "uncovered"), default="covered")
    wit.add_argument("--out")
    wit.set_defaults(func=witness)

    ver = sub.add_parser("verify", help="check one combined measurement's three views")
    ver.add_argument("--json", required=True)
    ver.add_argument("--summary", required=True)
    ver.add_argument("--html", required=True, help="path to the html index.html")
    ver.add_argument("--out")
    ver.set_defaults(func=verify)

    sl = sub.add_parser("slot", help="write the coverage slot JSON and its summary")
    sl.add_argument("--sha", required=True)
    sl.add_argument("--json")
    sl.add_argument("--summary")
    sl.add_argument("--html")
    sl.add_argument("--report-filter", default=REPORT_FILTER)
    sl.add_argument("--witness-dir")
    sl.add_argument("--runner-label", default="")
    sl.add_argument("--runner-image", default="")
    sl.add_argument("--os-version", default="")
    sl.add_argument("--arch", default="")
    sl.add_argument("--toolchain", default="")
    sl.add_argument("--tool-version", default="")
    sl.add_argument("--tool-digest", default="")
    sl.add_argument("--tool-digest-expected", default="")
    sl.add_argument("--bundle-name", default="")
    sl.add_argument("--slot-artifact-name", default="")
    sl.add_argument("--retention-days", type=int, default=90)
    sl.add_argument("--outcome", action="append", default=[], type=outcome_pair, metavar="STEP=OUTCOME")
    sl.add_argument("--out")
    sl.add_argument("--summary-out")
    sl.set_defaults(func=slot)

    args = parser.parse_args()
    if args.func is slot:
        args.outcomes = dict(args.outcome)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
