#!/usr/bin/env python3
"""The advisory complexity report reproduces the frozen measurements and warns baseline-relatively.

Specification: the documentation contract's complexity report, which `docs_metrics`
owns. Three groups of tests:

- **Independent fixtures.** The 17 cases below were authored without seeing the
  reference implementation and hand-counted before either ran. Their expected
  values are copied here verbatim, so this suite checks the port against
  independently established counts rather than against the reference's own
  expectations.
- **Comparison semantics.** A fixture repository holds a candidate tree, a policy
  and a small pinned baseline, and the tests drive `scripts/docs.py report` as a
  user would: a new unit, an unchanged over-threshold unit, growth over the
  recorded baseline, a rename with and without a reviewed mapping, an ambiguous
  heading hierarchy, a missing baseline record, a measurement-only family, an
  override, the unavailable parser and rejected policy values.
- **Unavailable reports.** Every report that does not run says
  `report unavailable:` with its cause, on the stream and with the exit status
  its kind of failure already used, and appends the same line to a requested
  summary that keeps whatever an earlier step wrote. This covers setup failures
  before measurement, which happen before the optional parser is imported.

The Markdown parser is an optional dependency of the report only. The cases that
measure documents are skipped without it, with the install command
(`scripts/docs-report-requirements.txt` pins the wheels and the `check` workflow
installs them before running this suite, so CI executes it rather than skipping
it); the setup and summary-write cases run either way.
"""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import docs_test_support as support

try:
    import markdown_it
except ImportError as exc:  # the report's optional dependency, absent by default
    markdown_it = None
    PARSER_SKIP = (
        "markdown_it is not installed (%s); install scripts/docs-report-requirements.txt "
        "with `python3 -m pip install --require-hashes -r scripts/docs-report-requirements.txt`"
        % exc
    )
else:
    PARSER_SKIP = ""

import docs_metrics

requires_parser = unittest.skipIf(markdown_it is None, PARSER_SKIP)

# The report runs as `docs.py` does from a shell, so the snapshot readers, exit
# statuses and the lazy parser import are exercised for real.
INTERPRETER = sys.executable


class Fixture:
    """One independently expected case, copied from the frozen fixture corpus."""

    def __init__(self, name, rules, source, lines, tokens, headings, preamble,
                 sections, section_paragraphs, ambiguous=()):
        self.name = name
        self.rules = rules
        self.source = source
        self.lines = lines
        self.tokens = tokens
        self.headings = headings
        self.preamble = preamble
        self.sections = sections
        self.section_paragraphs = section_paragraphs
        self.ambiguous = ambiguous


FIXTURES = (
    Fixture(
        "tight-list",
        "list-item paragraphs are outside narrative counting; a headingless "
        "document has its sole preamble",
        "- alpha\n- beta\n- gamma\n",
        lines=3, tokens=6, headings=0, preamble=0, sections=(), section_paragraphs=(),
    ),
    Fixture(
        "loose-list",
        "blank lines between items make the list loose; its paragraphs stay "
        "outside the narrative count",
        "- alpha\n\n- beta\n\n- gamma\n",
        lines=5, tokens=6, headings=0, preamble=0, sections=(), section_paragraphs=(),
    ),
    Fixture(
        "table-only-preamble",
        "tables are excluded, so a table-only preamble has zero narrative paragraphs",
        "| a | b |\n| --- | --- |\n| 1 | 2 |\n\n# Usage\n\nIntro.\n",
        lines=7, tokens=18, headings=1, preamble=0,
        sections=(((1, "Usage"),),), section_paragraphs=(1,),
    ),
    Fixture(
        "list-only-preamble",
        "list-only content before the first section heading forms a preamble "
        "with zero narrative paragraphs",
        "- step one\n- step two\n\n## Install\n\nRun it.\n",
        lines=6, tokens=10, headings=1, preamble=0,
        sections=(((2, "Install"),),), section_paragraphs=(1,),
    ),
    Fixture(
        "blockquote-heading-inside-section",
        "headings in blockquotes count toward density but section nothing; "
        "blockquote paragraphs are narrative and belong to the enclosing section",
        "# Guide\n\n> ## In the quote\n>\n> Quoted prose.\n\nBody prose.\n",
        lines=7, tokens=13, headings=2, preamble=None,
        sections=(((1, "Guide"),),), section_paragraphs=(2,),
    ),
    Fixture(
        "setext-headings",
        "setext headings parse; each section-beginning heading ends the prior "
        "section of any level",
        "Title\n=====\n\n## Sub\n\nText.\n\nAnother\n-------\n\nMore.\n",
        lines=11, tokens=8, headings=3, preamble=None,
        sections=(((1, "Title"),), ((1, "Title"), (2, "Sub")), ((1, "Title"), (2, "Another"))),
        section_paragraphs=(0, 1, 1),
    ),
    Fixture(
        "fenced-hash-lines",
        "heading-like text inside a fence is not a heading; a code-only "
        "preamble has zero narrative paragraphs",
        "```text\n# not a heading\n## also not\n```\n\n# Real heading\n\nDone.\n",
        lines=8, tokens=13, headings=1, preamble=0,
        sections=(((1, "Real heading"),),), section_paragraphs=(1,),
    ),
    Fixture(
        "heading-inside-list",
        "a heading inside a list item counts toward density and begins no section",
        "- ## Step\n- second item\n\n# Real\n\nBody.\n",
        lines=6, tokens=9, headings=2, preamble=0,
        sections=(((1, "Real"),),), section_paragraphs=(1,),
    ),
    Fixture(
        "headingless-with-prose",
        "a headingless document has its sole preamble and its top-level "
        "paragraphs are preamble narrative",
        "Just prose.\n\nSecond paragraph.\n",
        lines=3, tokens=4, headings=0, preamble=2, sections=(), section_paragraphs=(),
    ),
    Fixture(
        "headingless-empty",
        "a headingless document has its sole preamble even when empty",
        "",
        lines=0, tokens=0, headings=0, preamble=0, sections=(), section_paragraphs=(),
    ),
    Fixture(
        "no-preamble-before-first-heading",
        "blank lines produce no parsed content, so a document whose first "
        "parsed content is a heading records no preamble",
        "\n# Only\n\nBody.\n",
        lines=4, tokens=3, headings=1, preamble=None,
        sections=(((1, "Only"),),), section_paragraphs=(1,),
    ),
    Fixture(
        "duplicate-hierarchy",
        "duplicate full hierarchies in one document are ambiguous and reported",
        "# A\n\nx.\n\n## Notes\n\ny.\n\n## Notes\n\nz.\n",
        lines=11, tokens=9, headings=3, preamble=None,
        sections=(((1, "A"),), ((1, "A"), (2, "Notes")), ((1, "A"), (2, "Notes"))),
        section_paragraphs=(1, 1, 1),
        ambiguous=((((1, "A"), (2, "Notes")), 2),),
    ),
    Fixture(
        "frontmatter-excluded",
        "YAML frontmatter is excluded from the body; the heading after it is "
        "the first parsed content, so there is no preamble",
        "---\nstatus: accepted\n---\n\n# Decision\n\nBody.\n",
        lines=4, tokens=3, headings=1, preamble=None,
        sections=(((1, "Decision"),),), section_paragraphs=(1,),
    ),
    Fixture(
        "blockquote-preamble-paragraph",
        "blockquote paragraphs are narrative and can populate the preamble; a "
        "heading inside a blockquote begins no section",
        "> ## Quoted\n>\n> One.\n\n# Top\n\nTwo.\n",
        lines=7, tokens=9, headings=2, preamble=1,
        sections=(((1, "Top"),),), section_paragraphs=(1,),
    ),
    Fixture(
        "nested-and-repeated-levels",
        "hierarchy is the stack of enclosing section headings by level; each "
        "heading ends the prior section of any level",
        "# One\n\n## Two\n\n### Three\n\n# Four\n\n## Two\n\n### Three\n",
        lines=11, tokens=12, headings=6, preamble=None,
        sections=(
            ((1, "One"),),
            ((1, "One"), (2, "Two")),
            ((1, "One"), (2, "Two"), (3, "Three")),
            ((1, "Four"),),
            ((1, "Four"), (2, "Two")),
            ((1, "Four"), (2, "Two"), (3, "Three")),
        ),
        section_paragraphs=(0, 0, 0, 0, 0, 0),
    ),
    Fixture(
        "indented-code-hash-lines",
        "heading-like text in indented code is not a heading; a code-only "
        "preamble has zero narrative paragraphs",
        "    # indented\n    code\n\n# Real\n",
        lines=4, tokens=5, headings=1, preamble=0,
        sections=(((1, "Real"),),), section_paragraphs=(0,),
    ),
    Fixture(
        "section-content-classes",
        "loose list paragraphs and table cells are excluded from a section's "
        "narrative count while top-level paragraphs are counted",
        "# Section\n\nP1.\n\n- item\n\n  item paragraph\n\nP2.\n\n| a |\n| - |\n| 1 |\n",
        lines=13, tokens=17, headings=1, preamble=None,
        sections=(((1, "Section"),),), section_paragraphs=(2,),
    ),
)


@requires_parser
class IndependentFixtureTests(unittest.TestCase):
    """The ported measurement agrees with the frozen, independently authored fixtures."""

    def test_every_independent_fixture_matches(self):
        self.assertEqual(len(FIXTURES), 17, "the frozen corpus holds 17 fixtures")
        for fixture in FIXTURES:
            with self.subTest(fixture=fixture.name, rules=fixture.rules):
                measured = docs_metrics.measure_document(fixture.source)
                self.assertEqual(measured["lines"], fixture.lines, "body lines")
                self.assertEqual(measured["tokens"], fixture.tokens, "whitespace units")
                self.assertEqual(measured["headings"], fixture.headings, "headings")
                sections = [section for section in measured["sections"]
                            if section["identity"] != "preamble"]
                preambles = [section for section in measured["sections"]
                             if section["identity"] == "preamble"]
                expected_preamble = None if fixture.preamble is None else fixture.preamble
                actual_preamble = preambles[0]["paragraphs"] if preambles else None
                self.assertEqual(actual_preamble, expected_preamble, "preamble paragraphs")
                self.assertEqual(
                    tuple(tuple(tuple(step) for step in section["identity"])
                          for section in sections),
                    fixture.sections, "section hierarchies")
                self.assertEqual(
                    tuple(section["paragraphs"] for section in sections),
                    fixture.section_paragraphs, "section paragraphs")
                self.assertEqual(self.ambiguity(measured), fixture.ambiguous,
                                 "ambiguous hierarchies and their occurrences")

    @staticmethod
    def ambiguity(measured):
        groups = []
        for identity in measured["ambiguous_identities"]:
            decoded = json.loads(identity)
            occurrences = sum(
                1 for section in measured["sections"]
                if json.dumps(section["identity"], ensure_ascii=False) == identity)
            groups.append((tuple(tuple(step) for step in decoded), occurrences))
        return tuple(groups)


POLICY = support.load_policy()

# The pinned baseline is a private record, so the exported policy carries none and
# every fixture here supplies the baseline its report compares against. A tree
# holding this repository's overlay pins its own baseline instead.
FIXTURE_BASELINE = {"revision": "a" * 40, "path": "docs/baseline-fixture.json"}
BASELINE = POLICY.get("baseline", FIXTURE_BASELINE)
REVISION = BASELINE["revision"]
BASELINE_FILE = BASELINE["path"]


def document(title, body):
    """A fixture document whose measured body is exactly `body`."""
    return ("---\ntitle: %s\nsummary: A fixture record for the complexity report.\n"
            "keywords:\n  - prototype\n---\n\n%s" % (title, body))


def sized(title, lines):
    """A document whose body measures exactly `lines` physical lines."""
    # `document` keeps the blank line after the frontmatter, which is measured.
    return document(title, "# %s\n\n%s" % (title, "x\n" * (lines - 3)))


def record(lines, headings=1, sections=(((1, "Record"), 1),)):
    """A baseline document record, written by hand rather than measured."""
    return {
        "lines": lines,
        "tokens": 10,
        "headings": headings,
        "sections": [{"identity": identity, "paragraphs": count}
                     for identity, count in sections],
        "max_section_paragraphs": max((count for _identity, count in sections), default=0),
        "ambiguous_identities": [],
    }


def baseline_file(documents, directories=None, code_files=None):
    return {
        "revision": REVISION,
        "documents": documents,
        "directories": directories or {},
        "code_files": code_files or {"files": [], "by_category": {}},
    }


# A summary file the shared CI job has already written to, so an append that
# replaced its content instead of adding to it would be visible.
SUMMARY_SEED = "<!-- summary written by an earlier step -->\n"

# The authoritative exclusion predicate ships beside this suite. A fixture that
# measures code files commits its own copy, so the snapshot under test supplies
# the policy its categories are computed with instead of borrowing one.
CHECKER_PATH = "scripts/check-staged-token-limit.py"
CHECKER_SOURCE = (support.SCRIPTS / "check-staged-token-limit.py").read_text(encoding="utf-8")

# The fixture documents live in a synthetic root that the fixture policy declares
# and indexes itself, so no case depends on a directory only this private tree owns.
FIXTURE_DIRECTORY = "docs/pages"


class RepositoryCase(unittest.TestCase):
    """A fixture repository holding documents, a policy and a pinned baseline."""

    def setUp(self):
        self.thresholds = POLICY["thresholds"]
        self.policy_document = self.policy()

    def policy(self, **changes):
        document = support.policy()
        document["active_directories"] = [FIXTURE_DIRECTORY]
        document["indexed_directories"] = list(dict.fromkeys(
            list(document["indexed_directories"]) + [FIXTURE_DIRECTORY]))
        document["baseline"] = dict(BASELINE)
        document.update(changes)
        return document

    def repository(self, documents, baseline_documents, baseline=None, policy=None):
        repo = support.TempRepository(self, policy_document=policy or self.policy_document)
        for name, text in documents.items():
            repo.write(name, text)
        repo.write(BASELINE_FILE, json.dumps(
            baseline or baseline_file(baseline_documents), indent=1) + "\n")
        repo.commit()
        return repo

    def report(self, repo, *args, env=None, flags=()):
        completed = subprocess.run(
            [INTERPRETER, *flags, str(support.ENTRY_POINT), "report",
             "--root", str(repo.root), *args],
            cwd=repo.root, env=env or repo.env, capture_output=True, text=True)
        return completed

    def assertRan(self, result):
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def assertNoWarning(self, result):
        self.assertRan(result)
        self.assertIn("warnings: 0\n", result.stdout)
        self.assertNotIn("remedy:", result.stdout)

    def assertToolError(self, result, *fragments):
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        for fragment in fragments:
            self.assertIn(fragment, result.stderr)
        self.assertEqual(result.stdout, "")

    def summary_file(self, seed=SUMMARY_SEED):
        """A pre-populated summary file, as the shared CI step summary would be."""
        holder = tempfile.TemporaryDirectory()
        self.addCleanup(holder.cleanup)
        path = Path(holder.name) / "summary.md"
        path.write_text(seed, encoding="utf-8")
        return path

    def assertUnavailable(self, result, summary, *fragments):
        """A failed report says so on stderr, exits 2, and appends the same line.

        The summary keeps whatever an earlier step wrote: the diagnostic is
        added to it, never substituted for it.
        """
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(result.stdout, "")
        lines = result.stderr.splitlines()
        self.assertEqual(len(lines), 1, result.stderr)
        self.assertTrue(lines[0].startswith("report unavailable: "), lines[0])
        for fragment in fragments:
            self.assertIn(fragment, lines[0])
        self.assertEqual(summary.read_text(encoding="utf-8"),
                         SUMMARY_SEED + lines[0] + "\n")


@requires_parser
class ReportCase(RepositoryCase):
    """The fixture cases that measure documents, so they need the parser."""


class BaselineComparisonTests(ReportCase):
    def test_a_new_document_over_a_threshold_warns_as_new(self):
        limit = self.thresholds["document_lines"]
        repo = self.repository(
            {"docs/pages/0001-a.md": sized("Alpha", 5),
             "docs/pages/0002-b.md": sized("Beta", limit + 1)},
            {"docs/pages/0001-a.md": record(5)})
        result = self.report(repo)
        self.assertRan(result)
        self.assertIn("warnings: 1\n", result.stdout)
        self.assertIn("[document length: body lines] docs/pages/0002-b.md: %d body lines "
                      "> %d; new" % (limit + 1, limit), result.stdout)
        self.assertIn("remedy: ", result.stdout)
        self.assertIn("docs/pages/0002-b.md: %d body lines, " % (limit + 1), result.stdout)

    def test_an_unchanged_document_over_the_threshold_does_not_re_warn(self):
        limit = self.thresholds["document_lines"]
        repo = self.repository(
            {"docs/pages/0001-a.md": sized("Alpha", limit + 5)},
            {"docs/pages/0001-a.md": record(limit + 5)})
        result = self.report(repo)
        self.assertNoWarning(result)
        self.assertIn("above the threshold and not above their recorded baseline "
                      "(no warning): 1", result.stdout)
        self.assertIn("[document length: body lines] docs/pages/0001-a.md: %d body lines "
                      "> %d; baseline %d" % (limit + 5, limit, limit + 5), result.stdout)

    def test_a_document_exceeding_its_recorded_baseline_warns(self):
        limit = self.thresholds["document_lines"]
        repo = self.repository(
            {"docs/pages/0001-a.md": sized("Alpha", limit + 6)},
            {"docs/pages/0001-a.md": record(limit + 5)})
        result = self.report(repo)
        self.assertRan(result)
        self.assertIn("warnings: 1\n", result.stdout)
        self.assertIn("[document length: body lines] docs/pages/0001-a.md: %d body lines "
                      "> %d; baseline %d" % (limit + 6, limit, limit + 5), result.stdout)

    def test_a_rename_without_a_mapping_is_new_and_warns(self):
        limit = self.thresholds["document_lines"]
        repo = self.repository(
            {"docs/pages/0003-renamed.md": sized("Renamed", limit + 5)},
            {"docs/pages/0001-a.md": record(limit + 5)})
        result = self.report(repo)
        self.assertRan(result)
        self.assertIn("[document length: body lines] docs/pages/0003-renamed.md: "
                      "%d body lines > %d; new" % (limit + 5, limit), result.stdout)
        self.assertIn("baseline records missing from the candidate tree: 1", result.stdout)
        self.assertIn("docs/pages/0001-a.md", result.stdout)

    def test_a_reviewed_mapping_carries_the_old_measurement(self):
        limit = self.thresholds["document_lines"]
        policy = self.policy(baseline_mappings={
            "docs/pages/0003-renamed.md": "docs/pages/0001-a.md"})
        repo = self.repository(
            {"docs/pages/0003-renamed.md": sized("Renamed", limit + 5)},
            {"docs/pages/0001-a.md": record(limit + 5)}, policy=policy)
        result = self.report(repo)
        self.assertNoWarning(result)
        self.assertIn("[document length: body lines] docs/pages/0003-renamed.md: "
                      "%d body lines > %d; baseline %d" % (limit + 5, limit, limit + 5),
                      result.stdout)
        self.assertIn("baseline records missing from the candidate tree: 0", result.stdout)

    def test_a_duplicate_heading_hierarchy_is_ambiguous_and_treated_as_new(self):
        body = "# A\n\nx.\n\n## Notes\n\ny.\n\n## Notes\n\nz.\n"
        sections = (((1, "A"),), 1), (((1, "A"), (2, "Notes")), 1), \
            (((1, "A"), (2, "Notes")), 1)
        repo = self.repository(
            {"docs/pages/0001-a.md": document("A", body)},
            {"docs/pages/0001-a.md": record(11, headings=3, sections=sections)})
        result = self.report(repo)
        self.assertNoWarning(result)
        self.assertIn("2 new section identities, 0 missing, 1 ambiguous hierarchy "
                      "(treated as new)", result.stdout)
        self.assertIn("docs/pages/0001-a.md: 'A' (h1) > 'Notes' (h2) (2 occurrences; "
                      "treated as new)", result.stdout)

    def test_measurement_only_families_report_counts_and_deltas_only(self):
        """The report holds no reference value for these families, so none is printed."""
        paragraphs = document("Alpha", "# Section\n\nOne.\n\nTwo.\n\nThree.\n")
        repo = self.repository(
            {"docs/pages/0001-a.md": paragraphs,
             "docs/pages/0002-b.md": sized("Beta", 5),
             CHECKER_PATH: CHECKER_SOURCE,
             "scripts/example.py": "one = 1\ntwo = 2\nthree = 3\nfour = 4\nfive = 5\n"},
            {"docs/pages/0001-a.md": record(8, sections=(((1, "Section"), 1),))})
        result = self.report(repo)
        self.assertNoWarning(result)
        self.assertIn("measurement only (no warnings)", result.stdout)
        self.assertIn("| docs/pages 2 (baseline 0, +2)", result.stdout)
        self.assertIn("paragraph density: 4 narrative paragraphs in 2 sections "
                      "(baseline 1, +3)", result.stdout)
        self.assertIn("code-file length: 2 files, 209 lines (baseline 0, +209); "
                      "production 2 files 209 lines (baseline 0, +209)", result.stdout)
        self.assertNotIn("above the recorded reference", result.stdout)
        self.assertNotIn("measurement_only", result.stdout)


class OverrideTests(ReportCase):
    def override(self, scope, value, metric="document_lines", **fields):
        entry = {"scope": scope, "metric": metric, "value": value,
                 "reason": "A reviewed exception.", "authorization": "Maintainer decision.",
                 "review_trigger": "When the document grows past the override."}
        entry.update(fields)
        return entry

    def test_an_override_applies_only_to_its_named_scope(self):
        limit = self.thresholds["document_lines"]
        policy = self.policy(overrides=[self.override("docs/pages/0001-a.md", limit + 20)])
        repo = self.repository(
            {"docs/pages/0001-a.md": sized("Alpha", limit + 10),
             "docs/pages/0002-b.md": sized("Beta", limit + 10)},
            {"docs/pages/0001-a.md": record(limit - 5),
             "docs/pages/0002-b.md": record(limit - 5)}, policy=policy)
        result = self.report(repo)
        self.assertRan(result)
        self.assertIn("warnings: 1\n", result.stdout)
        self.assertNotIn("docs/pages/0001-a.md: %d body lines >" % (limit + 10), result.stdout)
        self.assertIn("[document length: body lines] docs/pages/0002-b.md: %d body lines "
                      "> %d; baseline %d" % (limit + 10, limit, limit - 5), result.stdout)
        self.assertIn("overrides in force: 1", result.stdout)
        self.assertIn("docs/pages/0001-a.md [document length: body lines] threshold %d"
                      % (limit + 20), result.stdout)
        self.assertIn("reason: A reviewed exception.", result.stdout)
        self.assertIn("authorization: Maintainer decision.; review trigger: When the document "
                      "grows past the override.", result.stdout)

    def test_an_override_cannot_silence_a_baseline_exceedance(self):
        limit = self.thresholds["document_lines"]
        policy = self.policy(overrides=[self.override("docs/pages/0001-a.md", limit + 20)])
        repo = self.repository(
            {"docs/pages/0001-a.md": sized("Alpha", limit + 1)},
            {"docs/pages/0001-a.md": record(limit + 1)}, policy=policy)
        self.assertToolError(self.report(repo), "refused", "silence an existing exceedance")


class PolicyValidationTests(ReportCase):
    def test_a_bad_override_field_is_unavailable(self):
        entry = {"scope": "docs/pages/0001-a.md", "metric": "document_lines",
                 "value": self.thresholds["document_lines"], "reason": "r",
                 "authorization": "a"}
        policy = self.policy(overrides=[entry])
        repo = self.repository({"docs/pages/0001-a.md": sized("Alpha", 5)},
                               {"docs/pages/0001-a.md": record(5)}, policy=policy)
        summary = self.summary_file()
        self.assertUnavailable(self.report(repo, "--summary-file", str(summary)), summary,
                               "missing", "review_trigger")

    def test_an_unknown_metric_name_is_unavailable(self):
        policy = self.policy(thresholds={"document_bytes": 100})
        repo = self.repository({"docs/pages/0001-a.md": sized("Alpha", 5)},
                               {"docs/pages/0001-a.md": record(5)}, policy=policy)
        summary = self.summary_file()
        self.assertUnavailable(self.report(repo, "--summary-file", str(summary)), summary,
                               "thresholds", "document_lines")

    def test_a_missing_baseline_file_is_unavailable(self):
        repo = support.TempRepository(self, policy_document=self.policy_document)
        repo.write("docs/pages/0001-a.md", sized("Alpha", 5))
        repo.commit()
        summary = self.summary_file()
        self.assertUnavailable(self.report(repo, "--summary-file", str(summary)), summary,
                               "baseline cannot be read", BASELINE_FILE)

    def test_a_baseline_at_another_revision_is_unavailable(self):
        stale = baseline_file({"docs/pages/0001-a.md": record(5)})
        stale["revision"] = "0" * 40
        repo = self.repository({"docs/pages/0001-a.md": sized("Alpha", 5)}, {},
                               baseline=stale)
        summary = self.summary_file()
        self.assertUnavailable(self.report(repo, "--summary-file", str(summary)), summary,
                               "records revision", REVISION)

    def test_the_policy_holds_no_reference_for_a_measurement_only_family(self):
        """Those families are counted and compared; no value is approved as a reference."""
        self.assertNotIn("measurement_only", sorted(POLICY))

    def hard_checks_pass(self, repo):
        """`check` and `coverage` succeed, so only the report judges its own configuration."""
        for command in ("check", "coverage"):
            with self.subTest(command=command):
                result = repo.run(command)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_a_policy_missing_a_report_only_key_does_not_stop_the_hard_checks(self):
        """The report owns its keys: a missing advisory key must not block a commit or CI."""
        broken = self.policy()
        del broken["thresholds"]
        repo = self.repository({"docs/pages/0001-a.md": sized("Alpha", 5)},
                               {"docs/pages/0001-a.md": record(5)}, policy=broken)
        self.assertRan(repo.generate())
        repo.commit("index")
        self.hard_checks_pass(repo)
        summary = self.summary_file()
        self.assertUnavailable(self.report(repo, "--summary-file", str(summary)), summary,
                               "thresholds", "which only the report reads")

    def test_invalid_report_configuration_does_not_stop_the_hard_checks(self):
        """A stale baseline revision is a visible report error, not a hard-check failure."""
        stale = baseline_file({"docs/pages/0001-a.md": record(5)})
        stale["revision"] = "0" * 40
        repo = self.repository({"docs/pages/0001-a.md": sized("Alpha", 5)}, {},
                               baseline=stale)
        self.assertRan(repo.generate())
        repo.commit("index")
        self.hard_checks_pass(repo)
        summary = self.summary_file()
        self.assertUnavailable(self.report(repo, "--summary-file", str(summary)), summary,
                               "records revision", REVISION)


class UnavailableReportTests(ReportCase):
    """A failure while measuring names itself instead of reading as an empty report."""

    def test_a_policy_that_cannot_be_compiled_is_unavailable(self):
        """The snapshot's own exclusion policy does not compile, so nothing is categorized."""
        repo = self.repository({"docs/pages/0001-a.md": sized("Alpha", 5),
                                CHECKER_PATH: "def broken(:\n",
                                "scripts/helper.py": "one = 1\n"},
                               {"docs/pages/0001-a.md": record(5)})
        summary = self.summary_file()
        # The default selector is the worktree, which holds the same broken file.
        self.assertUnavailable(self.report(repo, "--summary-file", str(summary)), summary,
                               CHECKER_PATH, "cannot load checker policy",
                               "the worktree snapshot cannot supply")

    def test_a_document_that_cannot_be_read_is_unavailable(self):
        repo = self.repository({"docs/pages/0001-a.md": sized("Alpha", 5)},
                               {"docs/pages/0001-a.md": record(5)})
        (repo.root / "docs/pages/0002-b.md").write_bytes(
            b"---\ntitle: B\n---\n\n\xff\xfe\n")
        repo.commit("a document that is not UTF-8")
        summary = self.summary_file()
        self.assertUnavailable(self.report(repo, "--summary-file", str(summary)), summary,
                               "docs/pages/0002-b.md", "not valid UTF-8")


class CandidateSnapshotTests(ReportCase):
    """Code-file categories and lines describe the one snapshot the report selected.

    Every fixture commits its own `scripts/check-staged-token-limit.py`, so the
    snapshot under test supplies the exclusion policy its categories use and the
    cases cannot pass on a policy borrowed from another snapshot or from this
    repository. The expected lines are literal: the fixture's own bytes are
    hand-counted and the committed checker copy's 204 lines are stated here, so a
    change to either shows up as a failing expectation rather than as a helper
    that agrees with the implementation by construction.
    """

    def code_repository(self, checker=CHECKER_SOURCE):
        """A fixture whose one commit carries the source-size policy it is measured with."""
        return self.repository(
            {"docs/pages/0001-a.md": sized("Alpha", 5),
             CHECKER_PATH: checker,
             "scripts/committed.py": "committed_a = 1\ncommitted_b = 2\ncommitted_c = 3\n"},
            {"docs/pages/0001-a.md": record(5)})

    def stage(self, repo, files, worktree=None):
        """Stage `files`, then rewrite `worktree` so the two copies differ."""
        for name, text in files.items():
            repo.write(name, text)
        repo.git("add", "--", *files)
        for name, text in (worktree or {}).items():
            repo.write(name, text)

    def code_line(self, result):
        """The report's one measurement-only code-file line."""
        self.assertRan(result)
        self.assertIn("measurement only (no warnings)", result.stdout)
        lines = [line.strip() for line in result.stdout.splitlines()
                 if line.startswith("  code-file length: ")]
        self.assertEqual(len(lines), 1, result.stdout)
        return lines[0]

    def test_a_staged_candidate_is_categorized_from_its_own_policy_and_bytes(self):
        repo = self.code_repository()
        base = repo.git("rev-parse", "HEAD").strip()
        self.stage(
            repo,
            {"scripts/new_helper.py": "def helper():\n    return 1\n\n\nVALUE = 2\nNOTE = 3\n",
             "tests/test_new_helper.py": "one\ntwo\nthree\nfour\nfive\n",
             "app/examples/demo.py": "a\nb\nc\nd\n",
             "examples/demo.py": "w\nx\ny\nz\n",
             "vendor/vendored.py": "v1\nv2\n"},
            # The same path with different bytes on disk: a staged report must
            # count the staged blob, not the working-tree copy.
            worktree={"scripts/new_helper.py":
                      "def helper():\n    return 1\n\n\nVALUE = 2\n"
                      "NOTE = 3\nEXTRA = 4\nMORE = 5\nALSO = 6\n"})

        staged = self.code_line(self.report(repo, "--staged"))
        self.assertEqual(staged, (
            "code-file length: 7 files, 228 lines (baseline 0, +228); checker-excluded "
            "2 files 7 lines (baseline 0, +7) | content 1 files 4 lines (baseline 0, +4) | "
            "example 1 files 4 lines (baseline 0, +4) | production 3 files 213 lines "
            "(baseline 0, +213)"))

        worktree_line = self.code_line(self.report(repo, "--worktree"))
        self.assertEqual(worktree_line, (
            "code-file length: 7 files, 231 lines (baseline 0, +231); checker-excluded "
            "2 files 7 lines (baseline 0, +7) | content 1 files 4 lines (baseline 0, +4) | "
            "example 1 files 4 lines (baseline 0, +4) | production 3 files 216 lines "
            "(baseline 0, +216)"))

        earlier = self.code_line(self.report(repo, "--checkout", "--rev", base))
        self.assertEqual(earlier, (
            "code-file length: 2 files, 207 lines (baseline 0, +207); production "
            "2 files 207 lines (baseline 0, +207)"))

        # Committing exactly what was staged must reproduce the staged verdict.
        repo.git("commit", "-qm", "the staged candidate")
        self.assertEqual(self.code_line(self.report(repo, "--staged")), staged)
        self.assertEqual(self.code_line(self.report(repo, "--checkout")), staged)
        self.assertEqual(self.code_line(self.report(repo, "--worktree")), worktree_line)

    def test_the_exclusion_policy_comes_from_the_candidate_not_from_head(self):
        """A staged checker decides the staged categories; HEAD's would differ."""
        divergent = ('"""Fixture policy: only this one legacy path is excluded."""\n'
                     "\n\n"
                     'EXCLUDED = {"scripts/legacy_helper.py"}\n'
                     "\n\n"
                     "def excluded(path):\n"
                     "    return path in EXCLUDED\n")
        repo = self.code_repository()
        base = repo.git("rev-parse", "HEAD").strip()
        self.stage(repo, {
            CHECKER_PATH: divergent,
            "scripts/legacy_helper.py": "legacy_one = 1\nlegacy_two = 2\nlegacy_three = 3\n",
            "scripts/test_fixture_probe.py": "probe_one = 1\nprobe_two = 2\n",
        })

        # This repository's checker exempts test files and nothing else here, so
        # a HEAD-derived or local-copy policy would swap both categories.
        staged = self.code_line(self.report(repo, "--staged"))
        self.assertEqual(staged, (
            "code-file length: 4 files, 16 lines (baseline 0, +16); checker-excluded "
            "1 files 3 lines (baseline 0, +3) | production 2 files 11 lines "
            "(baseline 0, +11) | test 1 files 2 lines (baseline 0, +2)"))

        repo.git("commit", "-qm", "the divergent policy")
        self.assertEqual(self.code_line(self.report(repo, "--checkout")), staged)
        # The earlier commit keeps its own policy, so its categories differ.
        earlier = self.code_line(self.report(repo, "--checkout", "--rev", base))
        self.assertEqual(earlier, (
            "code-file length: 2 files, 207 lines (baseline 0, +207); production "
            "2 files 207 lines (baseline 0, +207)"))

    def test_a_snapshot_without_its_policy_is_unavailable(self):
        """A copy exists in this repository; a historical fallback must not be used."""
        self.assertTrue((support.SCRIPTS / "check-staged-token-limit.py").is_file())
        repo = self.repository({"docs/pages/0001-a.md": sized("Alpha", 5),
                                "scripts/helper.py": "one = 1\ntwo = 2\n"},
                               {"docs/pages/0001-a.md": record(5)})
        for selector, label in (("--worktree", "worktree"),
                                ("--staged", "index"),
                                (("--checkout", "--rev", "HEAD"), "HEAD")):
            with self.subTest(selector=label):
                args = (selector,) if isinstance(selector, str) else selector
                summary = self.summary_file()
                self.assertUnavailable(
                    self.report(repo, *args, "--summary-file", str(summary)), summary,
                    CHECKER_PATH, "the %s snapshot cannot supply" % label)

    def test_a_policy_that_is_not_a_predicate_is_unavailable(self):
        repo = self.repository({"docs/pages/0001-a.md": sized("Alpha", 5),
                                CHECKER_PATH: "# a checker with no predicate\n",
                                "scripts/helper.py": "one = 1\n"},
                               {"docs/pages/0001-a.md": record(5)})
        summary = self.summary_file()
        self.assertUnavailable(self.report(repo, "--summary-file", str(summary)), summary,
                               CHECKER_PATH, "defines no 'excluded' predicate")


class ArgumentParsingTests(RepositoryCase):
    """`--help` and a malformed argument stay argparse's business, before any report."""

    def test_help_and_a_malformed_argument_leave_the_summary_untouched(self):
        repo = self.repository({"docs/pages/0001-a.md": sized("Alpha", 5)},
                               {"docs/pages/0001-a.md": record(5)})
        summary = self.summary_file()
        helped = self.report(repo, "--summary-file", str(summary), "--help")
        self.assertEqual(helped.returncode, 0, helped.stderr)
        self.assertIn("usage: docs.py", helped.stdout)
        self.assertEqual(helped.stderr, "")
        rejected = self.report(repo, "--summary-file", str(summary), "--nonsense")
        self.assertEqual(rejected.returncode, 2, rejected.stdout + rejected.stderr)
        self.assertEqual(rejected.stdout, "")
        self.assertIn("usage: docs.py", rejected.stderr)
        self.assertIn("--nonsense", rejected.stderr)
        self.assertEqual(summary.read_text(encoding="utf-8"), SUMMARY_SEED)


class UnavailableParserTests(RepositoryCase):
    """The missing-parser case forces the absence itself, so it never skips."""

    def test_a_missing_parser_prints_one_line_and_exits_three(self):
        repo = self.repository({"docs/pages/0001-a.md": sized("Alpha", 5)},
                               {"docs/pages/0001-a.md": record(5)})
        # `-S` keeps an interpreter's own site-packages out of `sys.path`, so the
        # simulation is the same on a host that already has the parser installed.
        env = {key: value for key, value in os.environ.items() if key != "PYTHONPATH"}
        summary = self.summary_file()
        result = self.report(repo, "--summary-file", str(summary), env=env,
                             flags=("-S",))
        self.assertEqual(result.returncode, 3, result.stdout + result.stderr)
        lines = [line for line in result.stdout.splitlines() if line.strip()]
        self.assertEqual(len(lines), 1, result.stdout)
        self.assertTrue(lines[0].startswith("report unavailable:"), lines[0])
        self.assertIn("markdown_it", lines[0])
        self.assertIn("scripts/docs-report-requirements.txt", lines[0])
        self.assertEqual(summary.read_text(encoding="utf-8"),
                         SUMMARY_SEED + lines[0] + "\n")
        self.assertEqual(result.stderr, "")


class ReportSetupFailureTests(RepositoryCase):
    """A report that fails before measurement still says so, and still appends.

    Root discovery, snapshot construction and policy loading all happen before
    `docs_report.run`, and before the optional parser is imported, so these
    cases need no parser and never skip.
    """

    def setUp(self):
        super().setUp()
        self.repo = self.repository({"docs/pages/0001-a.md": sized("Alpha", 5)},
                                    {"docs/pages/0001-a.md": record(5)})

    def test_an_unknown_revision_is_unavailable(self):
        summary = self.summary_file()
        self.assertUnavailable(
            self.report(self.repo, "--checkout", "--rev", "0000000",
                        "--summary-file", str(summary)),
            summary, "0000000")

    def test_a_policy_absent_from_the_chosen_revision_is_unavailable(self):
        self.repo.remove(support.POLICY_PATH)
        self.repo.commit("a revision without the policy")
        summary = self.summary_file()
        self.assertUnavailable(
            self.report(self.repo, "--checkout", "--rev", "HEAD",
                        "--summary-file", str(summary)),
            summary, "scripts/docs-policy.json is not in HEAD")

    def test_a_revision_without_checkout_is_unavailable(self):
        summary = self.summary_file()
        self.assertUnavailable(
            self.report(self.repo, "--rev", "HEAD", "--summary-file", str(summary)),
            summary, "--rev needs --checkout")

    def test_a_root_outside_a_repository_is_unavailable(self):
        holder = tempfile.TemporaryDirectory()
        self.addCleanup(holder.cleanup)
        summary = self.summary_file()
        env = {key: value for key, value in os.environ.items()
               if key not in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE",
                              "GIT_COMMON_DIR")}
        result = subprocess.run(
            [INTERPRETER, str(support.ENTRY_POINT), "report",
             "--summary-file", str(summary)],
            cwd=holder.name, env=env, capture_output=True, text=True)
        self.assertUnavailable(result, summary, "not a git repository")


class SummaryWriteFailureTests(ReportCase):
    """An unwritable summary never hides the report and never reads as success."""

    def unwritable_summary(self):
        """A summary path under a directory that does not exist, so appending fails."""
        holder = tempfile.TemporaryDirectory()
        self.addCleanup(holder.cleanup)
        return Path(holder.name) / "absent" / "summary.md"

    def test_a_failed_append_keeps_the_report(self):
        repo = self.repository({"docs/pages/0001-a.md": sized("Alpha", 5)},
                               {"docs/pages/0001-a.md": record(5)})
        summary = self.unwritable_summary()
        result = self.report(repo, "--summary-file", str(summary))
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("warnings: 0\n", result.stdout)
        self.assertIn("cannot write %s" % summary, result.stderr)
        self.assertFalse(summary.exists())

    def test_a_failed_append_keeps_the_original_failure(self):
        repo = support.TempRepository(self, policy_document=self.policy_document)
        repo.write("docs/pages/0001-a.md", sized("Alpha", 5))
        repo.commit()
        summary = self.unwritable_summary()
        result = self.report(repo, "--summary-file", str(summary))
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(result.stdout, "")
        self.assertIn("report unavailable: ", result.stderr)
        self.assertIn("baseline cannot be read", result.stderr)
        self.assertIn("cannot write %s" % summary, result.stderr)
        self.assertFalse(summary.exists())


class DeterminismTests(ReportCase):
    def test_repeated_reports_are_byte_identical(self):
        limit = self.thresholds["document_lines"]
        repo = self.repository(
            {"docs/pages/0001-a.md": sized("Alpha", limit + 5),
             "docs/pages/0002-b.md": document("Beta", "# Beta\n\nBody.\n")},
            {"docs/pages/0001-a.md": record(limit + 5),
             "docs/pages/0002-b.md": record(3, sections=(((1, "Beta"), 1),))})
        first = self.report(repo)
        second = self.report(repo)
        self.assertRan(first)
        self.assertEqual(first.stdout, second.stdout)
        with tempfile.TemporaryDirectory() as summaries:
            one, two = Path(summaries) / "one.md", Path(summaries) / "two.md"
            self.assertRan(self.report(repo, "--summary-file", str(one)))
            self.assertRan(self.report(repo, "--summary-file", str(two)))
            self.assertEqual(one.read_bytes(), two.read_bytes())
            self.assertEqual(one.read_text(encoding="utf-8"), first.stdout)

    def test_a_report_writes_only_the_named_summary_file(self):
        repo = self.repository(
            {"docs/pages/0001-a.md": sized("Alpha", self.thresholds["document_lines"] + 5)},
            {"docs/pages/0001-a.md": record(self.thresholds["document_lines"] + 5)})
        before = repo.tree_state()
        with tempfile.TemporaryDirectory() as summaries:
            summary = Path(summaries) / "summary.md"
            self.assertRan(self.report(repo, "--summary-file", str(summary)))
            self.assertTrue(summary.is_file())
        self.assertEqual(repo.tree_state(), before)


if __name__ == "__main__":
    unittest.main()
