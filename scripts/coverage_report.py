#!/usr/bin/env python3
"""Check one item-seven coverage measurement against its reporting contract.

The method is owned by ``docs/testing-coverage-evidence.md``. These are the
agreement checks the 0218 baseline ran from ``.github/coverage-item7/analyze.py``:
they never threshold a percentage, they only refuse to call a measurement
complete when the three views disagree, when an exported path falls outside the
engine/CLI allowlist, or when a metric is missing. The slot assembly lives in
``coverage_slot.py``.
"""

import json
import re
import sys

PACKAGE_PREFIXES = (("engine/src/", "unmei-engine"), ("cli/src/", "unmei"))
METRICS = ("lines", "functions", "regions")


def die(message):
    print("FAIL: " + message)
    sys.exit(1)


def normalise(path):
    return path.replace("\\", "/")


def allowlist_key(path):
    """Return (package, repo-relative source path) for an allowlisted file."""
    path = normalise(path)
    for prefix, package in PACKAGE_PREFIXES:
        index = path.find("/" + prefix)
        if index >= 0:
            return package, path[index + 1 :]
        if path.startswith(prefix):
            return package, path
    return None, None


def load_json(path):
    with open(path, encoding="utf-8") as handle:
        raw = json.load(handle)
    data = raw.get("data") or []
    if len(data) != 1:
        die("%s carries %d data entries; exactly one combined profile set is required" % (path, len(data)))
    return raw, data[0]


def metric(summary, name):
    value = summary.get(name)
    if value is None:
        die("metric %r is unavailable in an exported summary" % name)
    return int(value["covered"]), int(value["count"])


def check_allowlist(entry):
    files = {}
    offenders = []
    for item in entry["files"]:
        package, key = allowlist_key(item["filename"])
        if package is None:
            offenders.append(normalise(item["filename"]))
            continue
        if key in files:
            die("source file %s appears twice in one export" % key)
        files[key] = (package, item)
    if offenders:
        die(
            "%d exported source path(s) fall outside the engine/src + cli/src allowlist:\n  %s\n"
            "record and apply explicit exclusions, then regenerate every view from the same profiles"
            % (len(offenders), "\n  ".join(sorted(offenders)))
        )
    if not files:
        die("the export lists no allowlisted source files")
    return files


PAIR = re.compile(r"\((\d+)/(\d+)\)")


def parse_summary(path):
    """Parse `cargo llvm-cov report` text rows into {key: {metric: (covered, count)}}."""
    rows, total = {}, None
    header = None
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            line = line.rstrip("\n")
            if not line.strip():
                continue
            if header is None:
                header = line
                continue
            if set(line.strip()) <= {"-"}:
                continue
            parts = line.split()
            name, cells = parts[0], parts[1:]
            if len(cells) != 12:
                die("unrecognised report row (want 12 metric cells) in %s: %r" % (path, line))
            try:
                row = {}
                for offset, metric_name in ((0, "regions"), (3, "functions"), (6, "lines"), (9, "branches")):
                    count, missed = int(cells[offset]), int(cells[offset + 1])
                    row[metric_name] = (count - missed, count)
            except ValueError:
                die("unrecognised report row (non-numeric metric) in %s: %r" % (path, line))
            if name == "TOTAL":
                total = row
            else:
                rows[normalise(name)] = row
    if header is None or total is None:
        die("%s carries no table header or no TOTAL row" % path)
    for column in ("Regions", "Functions", "Lines"):
        if column not in header:
            die("%s header lacks the %s column: %r" % (path, column, header))
    return rows, total


ROW = re.compile(r"<tr class='light-row'>(.*?)</tr>", re.S)
LINK = re.compile(r"<a href='[^']*'>([^<]+)</a>")
# The html index column order is function, line, region, branch.
HTML_METRICS = ("functions", "lines", "regions", "branches")


def parse_html_index(path):
    with open(path, encoding="utf-8") as handle:
        text = handle.read()
    rows = {}
    for row in ROW.findall(text):
        match = LINK.search(row)
        if not match:
            die("an html index row carries no filename link")
        name = normalise(match.group(1))
        pairs = [(int(covered), int(count)) for covered, count in PAIR.findall(row)]
        if len(pairs) != 4:
            die("unrecognised html index row for %s: %r" % (name, row))
        rows[name] = dict(zip(HTML_METRICS, pairs))
    if not rows:
        die("%s lists no source rows" % path)
    return rows


def keyed(rows):
    keyed_rows = {}
    for name, row in rows.items():
        key = allowlist_key(name)[1] or name
        if key in keyed_rows:
            die("source file %s appears twice in one view" % key)
        keyed_rows[key] = row
    return keyed_rows


def compare(candidate_name, rows, expected):
    """Compare one view's per-file metric pairs against the JSON export."""
    mismatches = []
    for key in sorted(set(rows) | set(expected)):
        if key not in rows:
            mismatches.append("%s: missing from the %s view" % (key, candidate_name))
            continue
        if key not in expected:
            mismatches.append("%s: present in the %s view only" % (key, candidate_name))
            continue
        for name in METRICS:
            if rows[key][name] != expected[key][name]:
                mismatches.append(
                    "%s: %s %s shows %d/%d, the json export shows %d/%d"
                    % (key, candidate_name, name, rows[key][name][0], rows[key][name][1],
                       expected[key][name][0], expected[key][name][1])
                )
    if mismatches:
        die("%d disagreement(s) between the json export and the %s view:\n  %s"
            % (len(mismatches), candidate_name, "\n  ".join(mismatches)))


def summed(files):
    """Sum per-file counts into the aggregate and the two package scopes."""
    result = {scope: {name: (0, 0) for name in METRICS} for scope in ("aggregate", "unmei-engine", "unmei")}
    for _key, (package, item) in files.items():
        for name in METRICS:
            covered, count = metric(item["summary"], name)
            for scope in ("aggregate", package):
                previous = result[scope][name]
                result[scope][name] = (previous[0] + covered, previous[1] + count)
    return result


def percent_value(covered, count):
    return None if count == 0 else round(100.0 * covered / count, 2)


def percent_text(covered, count):
    value = percent_value(covered, count)
    return "n/a" if value is None else "%.2f%%" % value


def verify_measurement(json_path, summary_path, html_path, metric_label):
    """Return the verified-measurement markdown, or exit on a disagreement."""
    raw, entry = load_json(json_path)
    files = check_allowlist(entry)
    expected = {key: {name: metric(item["summary"], name) for name in METRICS} for key, (_p, item) in files.items()}

    summary_rows, summary_total = parse_summary(summary_path)
    compare("text summary", keyed(summary_rows), expected)
    compare("html index", keyed(parse_html_index(html_path)), expected)

    result = summed(files)
    for name in METRICS:
        if summary_total[name] != result["aggregate"][name]:
            die("the text TOTAL row reports %s %d/%d but the summed json counts are %d/%d"
                % (name, summary_total[name][0], summary_total[name][1],
                   result["aggregate"][name][0], result["aggregate"][name][1]))
        if metric(entry["totals"], name) != result["aggregate"][name]:
            die("the json totals for %s disagree with the summed per-file counts" % name)
    if metric(entry["totals"], "branches") != (0, 0):
        die("branch counters appeared; this measurement selects no branch instrumentation")

    lines = [
        "## Verified measurement",
        "",
        "- allowlisted source files: %d (%d engine, %d cli)" % (
            len(files),
            sum(1 for package, _ in files.values() if package == "unmei-engine"),
            sum(1 for package, _ in files.values() if package == "unmei"),
        ),
        "- source paths outside `engine/src/**` and `cli/src/**`: none",
        "- text summary, html index and json export agree per file and per total",
        "- branch/condition counters: unavailable (no branch or MC/DC instrumentation selected)",
        "- cargo-llvm-cov: %s" % (raw.get("cargo_llvm_cov") or {}).get("version", "unavailable"),
        "",
        "Coverage is **%s**. Counts are summed; percentages are covered/total for those" % metric_label,
        "summed counts, never an average of file percentages.",
        "",
        "| Scope | Metric | Covered | Total | Percent |",
        "| --- | --- | --- | --- | --- |",
    ]
    for scope in ("aggregate", "unmei-engine", "unmei"):
        for name in METRICS:
            covered, count = result[scope][name]
            lines.append("| %s | %s | %d | %d | %s |" % (scope, name, covered, count, percent_text(covered, count)))
    return "\n".join(lines + [""])


def witness_check(json_path, target, expect):
    """Return the witness receipt line, or exit when the capture expectation fails."""
    raw, entry = load_json(json_path)
    files = check_allowlist(entry)
    if target not in files:
        if expect == "uncovered":
            return "witness target %s: absent from the export (no child code compiled in)" % target
        die("%s is absent from the witness export" % target)
    covered, count = metric(files[target][1]["summary"], "lines")
    print("witness target %s: %d/%d lines covered" % (target, covered, count))
    if expect == "covered" and covered == 0:
        die("%s recorded no hit; the child profile did not reach the merged report" % target)
    if expect == "uncovered" and covered != 0:
        die("%s recorded %d hit line(s) without the child process; the negative control "
            "no longer isolates child startup" % (target, covered))
    return "witness target %s: %d/%d lines covered (expected %s), cargo-llvm-cov %s" % (
        target, covered, count, expect, (raw.get("cargo_llvm_cov") or {}).get("version", "unavailable")
    )
