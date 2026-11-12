"""The advisory complexity report's text.

`docs_report` decides which units warn and validates the policy; this module
turns those results into the report a person reads. It is separate only to keep
each report module inside the repository's 20,000-byte authored-source gate.

The text carries every required element of a finding — path, measured value,
threshold, the baseline value or "new", and a semantic remediation — and is
deterministic: path-sorted, no timestamps, no host paths, no locale-dependent
formatting. Numbers come from the policy or the baseline, never from constants
here.
"""

import json

import docs_metrics
from docs_metrics import LABELS


def render(report):
    """The whole report text, ending in one newline."""
    settings, baseline = report.settings, report.baseline
    documents, code_files = report.documents, report.code_files
    warnings, directories = report.warnings, report.directories
    new_units = [path for path in sorted(documents)
                 if baseline.record(path, settings.mappings) is None]
    lines = [
        "Advisory complexity report against the fixed baseline %s (advisory only: findings "
        "never fail CI and never split a document)." % baseline.revision,
        "baseline: %s; snapshot: %s; parser: markdown-it-py %s"
        % (baseline.path, report.snapshot_label, report.parser_version),
        "",
        "ordinary documents: %d (baseline %d) | sections: %d (baseline %d) | narrative "
        "paragraphs: %d (baseline %d)" % (
            len(documents), len(baseline.documents), sections(documents.values()),
            sections(baseline.documents.values()), paragraphs(documents.values()),
            paragraphs(baseline.documents.values())),
        "",
        "warnings: %d" % len(warnings),
    ]
    for warning in warnings:
        lines.append("  [%s] %s: %d %s > %d; %s" % (
            LABELS[warning["metric"]], warning["path"], warning["value"], warning["unit"],
            warning["threshold"],
            "new" if warning["baseline"] is None else "baseline %d" % warning["baseline"]))
        lines.append("    remedy: %s" % remedy(warning["metric"], warning["record"]))
    lines.extend(["", "above the threshold and not above their recorded baseline (no warning): "
                  "%d" % len(report.held)])
    for entry in report.held:
        lines.append("  [%s] %s: %d %s > %d; baseline %d" % (
            LABELS[entry["metric"]], entry["path"], entry["value"], entry["unit"],
            entry["threshold"], entry["baseline"]))
    lines.extend(["", "new units without a baseline record: %d" % len(new_units)])
    for path in new_units:
        record = documents[path]
        lines.append("  %s: %d body lines, %d whitespace units, %d headings" % (
            path, record["lines"], record["tokens"], record["headings"]))
    lines.extend(["", "measurement only (no warnings)", "  " + directory_line(
        baseline, documents, directories)])
    lines.append("  " + paragraph_line(settings, baseline, documents))
    lines.extend("  " + detail for detail in ambiguity_details(documents))
    lines.append("  " + code_line(baseline, code_files))
    sources = set(settings.mappings.values())
    missing = [name for name in sorted(baseline.documents)
               if name not in documents and name not in sources]
    lines.extend(["", "baseline records missing from the candidate tree: %d" % len(missing)])
    lines.extend("  %s" % path for path in missing)
    lines.extend(["", "overrides in force: %d" % len(settings.overrides)])
    for (scope, metric), entry in sorted(settings.overrides.items()):
        lines.append("  %s [%s] threshold %d" % (scope, LABELS[metric], entry["value"]))
        lines.append("    reason: %s" % entry["reason"])
        lines.append("    authorization: %s; review trigger: %s"
                     % (entry["authorization"], entry["review_trigger"]))
    return "\n".join(lines) + "\n"


def remedy(metric, record):
    """A semantic remediation: where authority should move, never how to fit the metric."""
    if metric == "heading_density":
        return ("the document carries many section headings: give each subject its own "
                "authoritative document, keep one owner per section, and link the documents "
                "from the index")
    if len([section for section in record["sections"]
            if section["identity"] != "preamble"]) >= 2:
        return ("the document already has section headings: move the sibling sections into "
                "their own authoritative documents and link them from the index")
    return ("the document has no sibling section to move: give its distinct subjects their own "
            "authoritative documents and link them from the index")


def sections(records):
    return sum(len(record["sections"]) for record in records)


def paragraphs(records):
    return sum(section["paragraphs"] for record in records for section in record["sections"])


def directory_line(baseline, documents, directories):
    """Direct ordinary documents per indexed directory, with the baseline delta."""
    current = docs_metrics.directory_density(documents, directories)
    return "directory density: " + " | ".join(
        "%s %d (%s)" % (name, current[name], delta(current[name],
                                                   baseline.directories.get(name, 0)))
        for name in directories)


def paragraph_line(settings, baseline, documents):
    """Narrative paragraphs per section: counted, compared, never warned on."""
    new, missing, ambiguous = section_changes(baseline, documents, settings.mappings)
    current = paragraphs(documents.values())
    peak = max((record["max_section_paragraphs"] for record in documents.values()), default=0)
    return ("paragraph density: %s in %s (%s); %s, %d missing, %s (treated as new); "
            "max %s in one section" % (
                plural(current, "narrative paragraph"),
                plural(sections(documents.values()), "section"),
                delta(current, paragraphs(baseline.documents.values())),
                plural(new, "new section identity", "new section identities"), missing,
                plural(ambiguous, "ambiguous hierarchy", "ambiguous hierarchies"),
                plural(peak, "paragraph")))


def section_changes(baseline, documents, mappings):
    """New, missing and ambiguous section identities against the baseline records."""
    new = missing = ambiguous = 0
    for path, record in documents.items():
        base = baseline.record(path, mappings)
        current = identity_counts(record["sections"])
        recorded = identity_counts(base["sections"] if base else [])
        flags = set(record["ambiguous_identities"])
        for identity, count in current.items():
            # A duplicate hierarchy cannot be matched to one recorded section, so
            # every occurrence is treated as new rather than half-matched.
            new += count if identity in flags else max(0, count - recorded.get(identity, 0))
        for identity, count in recorded.items():
            missing += max(0, count - current.get(identity, 0))
        ambiguous += len(flags)
    return new, missing, ambiguous


def identity_counts(sections_list):
    """Section identity -> occurrences; identity is the exact heading hierarchy."""
    counts = {}
    for section in sections_list:
        key = json.dumps(section["identity"], ensure_ascii=False)
        counts[key] = counts.get(key, 0) + 1
    return counts


def ambiguity_details(documents):
    """One line per ambiguous hierarchy, named by path and full identity."""
    details = []
    for path in sorted(documents):
        record = documents[path]
        counts = identity_counts(record["sections"])
        for identity in record["ambiguous_identities"]:
            details.append("%s: %s (%d occurrences; treated as new)" % (
                path, describe_identity(json.loads(identity)), counts.get(identity, 0)))
    return details


def describe_identity(identity):
    if identity == "preamble":
        return "preamble"
    return " > ".join("%r (h%d)" % (step[1], step[0]) for step in identity)


def code_line(baseline, code_files):
    """Physical source lines by audit category: counted, compared, never warned on."""
    parts = []
    for category in sorted(set(code_files["by_category"])
                           | set(baseline.code_files["by_category"])):
        current = code_files["by_category"].get(category, {"files": 0, "lines": 0})
        recorded = baseline.code_files["by_category"].get(category, {"files": 0, "lines": 0})
        parts.append("%s %d files %d lines (%s)" % (
            category, current["files"], current["lines"],
            delta(current["lines"], recorded["lines"])))
    detail = " | ".join(parts)
    return ("code-file length: %d files, %d lines (%s)%s" % (
        code_files["total"], code_files["total_lines"],
        delta(code_files["total_lines"], baseline.code_files.get("total_lines", 0)),
        "; " + detail if detail else ""))


def plural(count, singular, many=None):
    return "%d %s" % (count, singular if count == 1 else many or singular + "s")


def delta(current, recorded):
    change = current - recorded
    return "baseline %d, %s" % (recorded, "0" if change == 0 else "%+d" % change)
