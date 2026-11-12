"""Advisory complexity report against the policy's fixed baseline.

The report measures the candidate snapshot with `docs_metrics`, compares it
with the pinned baseline the policy names, and warns only where a unit exceeds
its threshold **and** is new or above its recorded baseline value, so an
unchanged document never re-warns. The three measurement-only families are
reported with their counts and their baseline deltas and never warn: the policy
records no reference value for them. Findings name the path, the value, the
threshold, the baseline value (or "new") and a semantic remediation; nothing
here deletes lines or splits a document for a metric.

Thresholds, overrides and the baseline live only in `scripts/docs-policy.json`.
An override records its scope, metric, value, reason, authorization and review
trigger; it may relax the adopted threshold for that one scope and is refused
when the unit already exceeded the adopted threshold at the baseline, so it
cannot silence an existing exceedance. It is separate from
`scripts/source-size-exceptions.json`, and there is no blanket exemption.

Exit status: **0 whenever the report ran, with or without findings**, because the
tool is advisory and the `check` workflow runs it with `continue-on-error`; a
finding must not read as a failure. **3** when the optional parser is missing:
one visible line beginning `report unavailable:` names the dependency and the
requirements file, the same line is appended to `--summary-file`, and no hard
check is suppressed. **2** for any other failed report, which says
`report unavailable:` and its cause on stderr. Every post-parse report failure
takes that path, including the root, snapshot and policy setup `docs.py` performs
before `run`, so a report that did not run cannot read as an empty one that did.

The summary file is appended to, because `$GITHUB_STEP_SUMMARY` is shared with
the other steps in the job. Nothing outside that file is written.
"""

import json
import posixpath
import re
import sys
from typing import NamedTuple

import docs_metrics
import docs_report_text
import docs_snapshot

REQUIREMENTS = "scripts/docs-report-requirements.txt"

OVERRIDE_FIELDS = ("scope", "metric", "value", "reason", "authorization", "review_trigger")

# Owned by the report alone; the hard checks must pass while one of these is
# missing, so a missing advisory key cannot block a commit or a CI gate.
REPORT_POLICY_KEYS = ("thresholds", "baseline", "baseline_mappings", "overrides")


class ReportError(ValueError):
    """The policy, the baseline or the candidate tree cannot be reported on."""


class Baseline:
    """The pinned measurements the report compares against."""

    def __init__(self, revision, path, document):
        self.revision = revision
        self.path = path
        self.documents = document["documents"]
        self.directories = document["directories"]
        self.code_files = document["code_files"]

    def record(self, path, mappings):
        """The baseline record for `path`, following a reviewed rename mapping."""
        return self.documents.get(mappings.get(path, path))


class Settings:
    """The validated policy values the report uses."""

    def __init__(self, thresholds, overrides, mappings):
        self.thresholds = thresholds
        self.overrides = overrides
        self.mappings = mappings

    def threshold(self, path, metric):
        entry = self.overrides.get((path, metric))
        return self.thresholds[metric] if entry is None else entry["value"]


class Report(NamedTuple):
    """One measured candidate tree, ready to render."""

    settings: "Settings"
    baseline: "Baseline"
    documents: dict
    code_files: dict
    warnings: list
    held: list
    parser_version: str
    snapshot_label: str
    directories: tuple


def run(snapshot, policy, policy_path, summary_file):
    """Report on `snapshot`; return the process exit status."""
    try:
        version = docs_metrics.parser_version()
    except ImportError as exc:
        return unavailable(
            "the optional parser (markdown_it) is not installed (%s); install %s"
            % (exc, REQUIREMENTS), summary_file, 3, sys.stdout)
    try:
        text = build_report(snapshot, policy, policy_path, version)
    except (ReportError, docs_metrics.MeasurementError,
            docs_snapshot.SnapshotError, OSError) as exc:
        return unavailable(str(exc), summary_file)
    return _write(text, summary_file, 0, sys.stdout)


def unavailable(cause, summary_file, status=2, stream=None):
    """Announce a report that did not run, and append the same line to the summary.

    The one diagnostic path for every post-parse report failure: `docs.py`
    routes its setup failures here too. A missing parser keeps its stdout stream
    and status 3; every other failure is a tool error on stderr with status 2.
    """
    return _write("report unavailable: %s\n" % cause, summary_file, status,
                  sys.stderr if stream is None else stream)


def _write(text, summary_file, status, stream):
    """Put the report text on `stream`, then append it; a failed append is never success."""
    stream.write(text)
    if summary_file is None:
        return status
    try:
        with open(summary_file, "a", encoding="utf-8", newline="\n") as handle:
            handle.write(text)
    except OSError as exc:
        sys.stderr.write("docs: report: cannot write %s: %s\n" % (summary_file, exc))
        return 2
    return status


def build_report(snapshot, policy, policy_path, parser_version):
    """Validate the policy and baseline, measure, and render the report."""
    settings = validate_policy(policy, policy_path)
    baseline = load_baseline(snapshot, policy, policy_path)
    documents = measure_documents(snapshot, policy)
    check_mappings(settings, baseline, documents, policy_path)
    check_overrides(settings, baseline, documents, policy_path)
    code_files = docs_metrics.measure_code_files(snapshot)
    warnings, held = collect_warnings(settings, baseline, documents)
    return docs_report_text.render(Report(
        settings, baseline, documents, code_files, warnings, held, parser_version,
        snapshot.label, tuple(policy["indexed_directories"])))


def validate_policy(policy, policy_path):
    """Reject a policy the report cannot compare against, naming the file and key."""
    missing = [key for key in REPORT_POLICY_KEYS if key not in policy]
    if missing:
        raise ReportError("%s is missing %s, which only the report reads; the hard checks do "
                          "not require it" % (policy_path, ", ".join(missing)))
    thresholds = metric_table(policy, "thresholds", docs_metrics.ADOPTED, policy_path)
    entry = policy.get("baseline")
    if not isinstance(entry, dict) or set(entry) != {"revision", "path"}:
        raise ReportError("%s: 'baseline' must hold exactly 'revision' and 'path'" % policy_path)
    if not isinstance(entry["revision"], str) or not re.fullmatch(
            r"[0-9a-f]{7,40}", entry["revision"]):
        raise ReportError("%s: 'baseline.revision' must be a commit SHA" % policy_path)
    if not repo_path(entry["path"]):
        raise ReportError("%s: 'baseline.path' must be a repository-relative path" % policy_path)
    return Settings(thresholds,
                    override_table(policy.get("overrides", []), thresholds, policy_path),
                    baseline_mappings(policy.get("baseline_mappings", {}), policy_path))


def metric_table(policy, key, names, policy_path):
    """One metric table, which must name exactly the metrics the code reads."""
    table = policy.get(key)
    if not isinstance(table, dict) or sorted(table) != sorted(names):
        raise ReportError("%s: '%s' must name exactly %s"
                          % (policy_path, key, ", ".join(names)))
    for name, value in table.items():
        if not positive_int(value):
            raise ReportError("%s: '%s.%s' must be a positive integer" % (policy_path, key, name))
    return table


def baseline_mappings(raw, policy_path):
    """Reviewed renames: a candidate path carries the baseline record of another path."""
    if not isinstance(raw, dict):
        raise ReportError("%s: 'baseline_mappings' must map a current path to the baseline "
                          "path it replaces" % policy_path)
    mappings = {}
    sources = set()
    for target, source in raw.items():
        for value in (target, source):
            if not repo_path(value):
                raise ReportError("%s: baseline mapping %r is not a normalised repository path"
                                  % (policy_path, value))
        if target == source or source in sources:
            raise ReportError("%s: baseline mapping %r is either a self-map or a baseline "
                              "record carried twice" % (policy_path, target))
        sources.add(source)
        mappings[target] = source
    return mappings


def override_table(raw, thresholds, policy_path):
    """Validated overrides, keyed by their one scope and metric."""
    if not isinstance(raw, list):
        raise ReportError("%s: 'overrides' must be a list" % policy_path)
    overrides = {}
    for entry in raw:
        if not isinstance(entry, dict):
            raise ReportError("%s: every override must be an object" % policy_path)
        missing = [name for name in OVERRIDE_FIELDS if name not in entry]
        extra = sorted(name for name in entry if name not in OVERRIDE_FIELDS)
        if missing or extra:
            raise ReportError("%s: an override records scope, metric, value, reason, "
                              "authorization and review trigger; missing %s, unknown %s"
                              % (policy_path, missing, extra))
        for field in ("scope", "reason", "authorization", "review_trigger"):
            if not isinstance(entry[field], str) or not entry[field].strip():
                raise ReportError("%s: an override needs a non-empty '%s'" % (policy_path, field))
        if not repo_path(entry["scope"]):
            raise ReportError("%s: override scope %r is not a normalised repository path"
                              % (policy_path, entry["scope"]))
        metric = entry["metric"]
        if metric not in thresholds:
            raise ReportError("%s: override metric %r is not one of %s"
                              % (policy_path, metric, ", ".join(thresholds)))
        if not positive_int(entry["value"]) or entry["value"] < thresholds[metric]:
            raise ReportError("%s: override value for %s must be an integer at least its "
                              "adopted threshold %d; an override relaxes a threshold, it never "
                              "tightens one" % (policy_path, metric, thresholds[metric]))
        if (entry["scope"], metric) in overrides:
            raise ReportError("%s: two overrides name %s for %s"
                              % (policy_path, entry["scope"], metric))
        overrides[(entry["scope"], metric)] = entry
    return overrides


def load_baseline(snapshot, policy, policy_path):
    """Read and validate the pinned baseline the policy names."""
    entry = policy["baseline"]
    try:
        raw = snapshot.read(entry["path"])
    except docs_snapshot.SnapshotError as exc:
        raise ReportError("the policy's baseline cannot be read from the %s snapshot: %s"
                          % (snapshot.label, exc)) from exc
    try:
        document = json.loads(raw.decode("utf-8"))
    except (ValueError, UnicodeDecodeError) as exc:
        raise ReportError("%s is not a readable baseline: %s" % (entry["path"], exc)) from exc
    if not isinstance(document, dict):
        raise ReportError("%s must hold one baseline object" % entry["path"])
    if document.get("revision") != entry["revision"]:
        raise ReportError("%s records revision %r but %s pins %r; a baseline revision change "
                          "needs a maintainer decision and before/after counts"
                          % (entry["path"], document.get("revision"), policy_path,
                             entry["revision"]))
    validate_baseline_records(document, entry["path"])
    return Baseline(entry["revision"], entry["path"], document)


def validate_baseline_records(document, path):
    """A baseline the report cannot compare against is a tool error, not a silent pass."""
    for key in ("documents", "directories", "code_files"):
        if not isinstance(document.get(key), dict):
            raise ReportError("%s is missing its '%s' records" % (path, key))
    for name, record in document["documents"].items():
        sections = record.get("sections") if isinstance(record, dict) else None
        if not isinstance(sections, list) or not all(
                isinstance(section, dict) and isinstance(section.get("paragraphs"), int)
                and "identity" in section for section in sections):
            raise ReportError("%s: document record %r has no usable 'sections'" % (path, name))
        for key in ("lines", "tokens", "headings"):
            if not isinstance(record.get(key), int) or isinstance(record.get(key), bool):
                raise ReportError("%s: document record %r has no integer '%s'"
                                  % (path, name, key))
    if not all(isinstance(value, int) and not isinstance(value, bool)
               for value in document["directories"].values()):
        raise ReportError("%s: a directory count is not an integer" % path)
    code = document["code_files"]
    if not isinstance(code.get("files"), list) or not isinstance(code.get("by_category"), dict) \
            or not all(isinstance(record, dict) and isinstance(record.get("files"), int)
                       and isinstance(record.get("lines"), int)
                       for record in code["by_category"].values()):
        raise ReportError("%s: 'code_files' needs a 'files' list and measured categories" % path)


def check_mappings(settings, baseline, documents, policy_path):
    """Both ends of a reviewed mapping must exist where they are claimed to."""
    for target, source in sorted(settings.mappings.items()):
        if source not in baseline.documents:
            raise ReportError("%s: baseline mapping source %r is not in %s; a mapping carries a "
                              "recorded measurement" % (policy_path, source, baseline.path))
        if target not in documents:
            raise ReportError("%s: baseline mapping target %r is not an ordinary document in "
                              "the candidate tree" % (policy_path, target))
        if target in baseline.documents:
            raise ReportError("%s: baseline mapping target %r already has a baseline record"
                              % (policy_path, target))


def check_overrides(settings, baseline, documents, policy_path):
    """An override may relax a threshold; it may not silence a baseline exceedance."""
    for scope, metric in sorted(settings.overrides):
        if scope not in documents and scope not in baseline.documents:
            raise ReportError("%s: override scope %r names no measured document"
                              % (policy_path, scope))
        record = baseline.record(scope, settings.mappings)
        exceeded = (record is not None
                    and record[docs_metrics.FIELDS[metric]] > settings.thresholds[metric])
        if exceeded:
            raise ReportError(
                "%s: override for %s on %s is refused: the unit already exceeded its adopted "
                "threshold at the baseline (%d %s), so an override would silence an existing "
                "exceedance" % (policy_path, scope, metric,
                                record[docs_metrics.FIELDS[metric]],
                                docs_metrics.FIELDS[metric]))


def measure_documents(snapshot, policy):
    """Every ordinary document in the candidate snapshot, measured."""
    documents = {}
    for path in snapshot.paths():
        if docs_snapshot.classify(path, policy).kind != "ordinary":
            continue
        documents[path] = docs_metrics.measure_document(snapshot.text(path))
    return documents


def collect_warnings(settings, baseline, documents):
    """A unit warns only above its threshold and above its recorded baseline value.

    A unit the baseline already recorded above its threshold is not news: it is
    returned separately so the report can show it as unchanged rather than silent.
    """
    warnings = []
    held = []
    for path in sorted(documents):
        record = documents[path]
        base = baseline.record(path, settings.mappings)
        for metric, _family, unit, key in docs_metrics.FAMILIES:
            threshold = settings.threshold(path, metric)
            if record[key] <= threshold:
                continue
            value = None if base is None else base[key]
            entry = {"metric": metric, "path": path, "unit": unit, "record": record,
                     "value": record[key], "threshold": threshold, "baseline": value}
            (warnings if value is None or record[key] > value else held).append(entry)
    order = {metric: index for index, metric in enumerate(docs_metrics.ADOPTED)}
    for entries in (warnings, held):
        entries.sort(key=lambda item: (order[item["metric"]], item["path"]))
    return warnings, held


def positive_int(value):
    return isinstance(value, int) and not isinstance(value, bool) and value > 0


def repo_path(value):
    """A normalised repository-relative POSIX path, with no escape and no drive letter."""
    return (isinstance(value, str) and bool(value) and not value.startswith("/")
            and "\\" not in value and ":" not in value
            and posixpath.normpath(value) == value and ".." not in value.split("/"))
