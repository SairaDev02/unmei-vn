#!/usr/bin/env python3
"""Logic tests for `scripts/coverage_slot.py`.

No cargo, rustup or network: the script runs through its own CLI against
synthetic reports shaped like the ones `cargo-llvm-cov 0.9.1` writes, plus
synthetic witness receipts shaped like the ones the baseline run retained.
"""

import contextlib
import importlib.util
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("coverage_slot.py")


def load_module():
    spec = importlib.util.spec_from_file_location("unmei_coverage_slot", SCRIPT)
    if spec is None or spec.loader is None:  # pragma: no cover - defensive
        raise ImportError(f"cannot load {SCRIPT}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def summary_json(lines, functions, regions):
    def block(pair, branch=False):
        covered, count = pair
        entry = {"count": count, "covered": covered, "notcovered": count - covered}
        if branch:
            entry["percent"] = 0
        else:
            entry["percent"] = (100.0 * covered / count) if count else 0.0
        return entry

    return {
        "lines": block(lines),
        "functions": block(functions),
        "regions": block(regions),
        "branches": block((0, 0), branch=True),
    }


class Workspace:
    """A temporary directory holding one consistent three-view measurement."""

    def __init__(self, root):
        self.root = root

    def views(self, files):
        """Write total-consistent summary/JSON/HTML for {path: (l, f, r)}."""
        total = {
            "lines": (0, 0),
            "functions": (0, 0),
            "regions": (0, 0),
        }
        entries = []
        for path, counts in files.items():
            summary = summary_json(counts["lines"], counts["functions"], counts["regions"])
            entries.append({"filename": "/home/runner/work/unmei-engine/unmei-engine/" + path, "summary": summary})
            for name in total:
                covered, count = total[name]
                hit = summary[name]
                total[name] = (covered + hit["covered"], count + hit["count"])
        export = {
            "data": [
                {
                    "files": entries,
                    "totals": {
                        name: summary_json(total["lines"], total["functions"], total["regions"])[name]
                        for name in ("lines", "functions", "regions", "branches")
                    },
                    "functions": [],
                }
            ],
            "cargo_llvm_cov": {"version": "0.9.1"},
        }
        json_path = self.root / "coverage.json"
        json_path.write_text(json.dumps(export), encoding="utf-8")
        summary_path = self.root / "summary.txt"
        summary_path.write_text(self.summary_text(files), encoding="utf-8")
        html_path = self.root / "index.html"
        html_path.write_text(self.html_text(files), encoding="utf-8")
        return json_path, summary_path, html_path

    def summary_text(self, files):
        rows = [
            "Filename                          Regions    Missed Regions     Cover   Functions"
            "  Missed Functions  Executed       Lines      Missed Lines     Cover    Branches"
            "   Missed Branches     Cover",
            "-" * 60,
        ]
        totals = {"lines": [0, 0], "functions": [0, 0], "regions": [0, 0]}
        for path, counts in files.items():
            rows.append(self.summary_row(path, counts))
            for name in totals:
                totals[name][0] += counts[name][0]
                totals[name][1] += counts[name][1]
        rows.append(self.summary_row("TOTAL", {name: tuple(value) for name, value in totals.items()}))
        return "\n".join(rows) + "\n"

    @staticmethod
    def summary_row(name, counts):
        def cells(pair):
            covered, count = pair
            share = "%.2f%%" % (100.0 * covered / count) if count else "-"
            return ["%d" % count, "%d" % (count - covered), share]

        return "  ".join(
            ["%-32s" % name] + cells(counts["regions"]) + cells(counts["functions"]) + cells(counts["lines"])
            + ["0", "0", "-"]
        )

    def html_text(self, files):
        rows = []
        for path, counts in files.items():
            pairs = [
                self.pair(counts["functions"]),
                self.pair(counts["lines"]),
                self.pair(counts["regions"]),
                "- (0/0)",
            ]
            rows.append(
                "<tr class='light-row'><td><pre><a href='x'>%s</a></pre></td>%s</tr>"
                % (path, "".join("<td><pre>%s</pre></td>" % pair for pair in pairs))
            )
        return "<table>" + "".join(rows) + "</table>"

    @staticmethod
    def pair(counts):
        covered, count = counts
        share = ("%.2f%%" % (100.0 * covered / count)) if count else "-"
        return "%s (%d/%d)" % (share, covered, count)


class CoverageSlotTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.module = load_module()

    def setUp(self):
        tempdir = tempfile.TemporaryDirectory()
        self.addCleanup(tempdir.cleanup)
        self.root = Path(tempdir.name)
        self.files = {
            "engine/src/runner.rs": {"lines": (88, 100), "functions": (9, 10), "regions": (19, 20)},
            "cli/src/main.rs": {"lines": (17, 25), "functions": (2, 4), "regions": (36, 40)},
        }

    def run_cli(self, *argv):
        previous = sys.argv
        sys.argv = ["coverage_slot.py", *argv]
        self.addCleanup(setattr, sys, "argv", previous)
        out = io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(out):
            try:
                return self.module.main(), out.getvalue()
            finally:
                sys.stdout = sys.__stdout__
                sys.stderr = sys.__stderr__

    # --- verify -----------------------------------------------------------

    def test_three_agreeing_views_pass_and_the_counts_are_summed(self):
        json_path, summary, html = Workspace(self.root).views(self.files)
        out = self.root / "VERIFY.md"
        code, printed = self.run_cli(
            "verify", "--json", str(json_path), "--summary", str(summary), "--html", str(html), "--out", str(out)
        )
        self.assertEqual(code, 0)
        self.assertIn("| aggregate | lines | 105 | 125 | 84.00% |", printed)
        self.assertIn("| unmei-engine | lines | 88 | 100 | 88.00% |", printed)
        self.assertIn("| unmei | lines | 17 | 25 | 68.00% |", printed)
        self.assertIn("production-file coverage, including inline test code", out.read_text(encoding="utf-8"))

    def test_a_per_file_row_that_disagrees_with_the_export_fails(self):
        json_path, summary, html = Workspace(self.root).views(self.files)
        rows = summary.read_text(encoding="utf-8").splitlines()
        cells = rows[2].split()
        cells[2] = str(int(cells[2]) + 1)
        rows[2] = " ".join(cells)
        summary.write_text("\n".join(rows) + "\n", encoding="utf-8")
        with self.assertRaises(SystemExit) as caught:
            self.run_cli("verify", "--json", str(json_path), "--summary", str(summary), "--html", str(html))
        self.assertEqual(caught.exception.code, 1)

    def test_an_html_row_that_disagrees_with_the_export_fails(self):
        json_path, summary, html = Workspace(self.root).views(self.files)
        html.write_text(html.read_text(encoding="utf-8").replace("(19/20)", "(18/20)"), encoding="utf-8")
        with self.assertRaises(SystemExit):
            self.run_cli("verify", "--json", str(json_path), "--summary", str(summary), "--html", str(html))

    def test_a_totals_row_that_disagrees_with_the_summed_export_fails(self):
        json_path, summary, html = Workspace(self.root).views(self.files)
        rows = summary.read_text(encoding="utf-8").splitlines()
        rows[-1] = rows[-1].replace("125", "126", 1)
        summary.write_text("\n".join(rows) + "\n", encoding="utf-8")
        with self.assertRaises(SystemExit):
            self.run_cli("verify", "--json", str(json_path), "--summary", str(summary), "--html", str(html))

    def test_an_app_path_outside_the_allowlist_fails(self):
        files = dict(self.files)
        files["app/src/main.rs"] = {"lines": (1, 2), "functions": (1, 1), "regions": (1, 2)}
        json_path, summary, html = Workspace(self.root).views(files)
        with self.assertRaises(SystemExit):
            self.run_cli("verify", "--json", str(json_path), "--summary", str(summary), "--html", str(html))

    def test_branch_counters_appearing_fails(self):
        json_path, summary, html = Workspace(self.root).views(self.files)
        export = json.loads(json_path.read_text(encoding="utf-8"))
        export["data"][0]["totals"]["branches"].update(count=4, covered=1)
        json_path.write_text(json.dumps(export), encoding="utf-8")
        with self.assertRaises(SystemExit):
            self.run_cli("verify", "--json", str(json_path), "--summary", str(summary), "--html", str(html))

    # --- witness ----------------------------------------------------------

    def witness_export(self, covered):
        summary = summary_json((covered, 279), (1, 1), (1, 1))
        export = {
            "data": [
                {
                    "files": [{"filename": "/w/cli/src/main.rs", "summary": summary}],
                    "totals": summary,
                }
            ],
            "cargo_llvm_cov": {"version": "0.9.1"},
        }
        path = self.root / "witness.json"
        path.write_text(json.dumps(export), encoding="utf-8")
        return path

    def test_a_child_hit_proves_capture(self):
        code, printed = self.run_cli(
            "witness", "--json", str(self.witness_export(22)), "--file", "cli/src/main.rs", "--expect", "covered"
        )
        self.assertEqual(code, 0)
        self.assertIn("22/279 lines covered", printed)

    def test_no_child_hit_fails_the_capture_expectation(self):
        with self.assertRaises(SystemExit):
            self.run_cli(
                "witness", "--json", str(self.witness_export(0)), "--file", "cli/src/main.rs", "--expect", "covered"
            )

    def test_an_in_process_hit_fails_the_negative_control(self):
        with self.assertRaises(SystemExit):
            self.run_cli(
                "witness", "--json", str(self.witness_export(3)), "--file", "cli/src/main.rs", "--expect", "uncovered"
            )

    # --- slot -------------------------------------------------------------

    def slot(self, *extra, **kwargs):
        json_path, summary, html = kwargs.get("views") or Workspace(self.root).views(self.files)
        out = self.root / "coverage-slot.json"
        summary_out = self.root / "coverage-slot.md"
        argv = [
            "slot",
            "--sha", "a" * 40,
            "--json", str(json_path),
            "--summary", str(summary),
            "--html", str(html),
            "--runner-label", "ubuntu-24.04",
            "--os-version", "24.04",
            "--arch", "x86_64",
            "--toolchain", "1.98.1",
            "--tool-version", "0.9.1",
            "--tool-digest", "abc",
            "--tool-digest-expected", "abc",
            "--bundle-name", "coverage-%s-attempt1" % ("a" * 40),
            "--retention-days", "90",
            "--out", str(out),
            "--summary-out", str(summary_out),
            *extra,
        ]
        code, printed = self.run_cli(*argv)
        return code, printed, json.loads(out.read_text(encoding="utf-8")), summary_out.read_text(encoding="utf-8")

    def test_a_clean_run_publishes_a_passed_slot(self):
        code, printed, slot, rendered = self.slot(
            "--outcome", "toolchain=success", "--outcome", "install=success", "--outcome", "witness=success",
            "--outcome", "measure=success", "--outcome", "export=success", "--outcome", "verify=success",
        )
        self.assertEqual(code, 0)
        self.assertEqual(slot["status"], "passed")
        self.assertIsNone(slot["reason"])
        self.assertEqual(slot["slot"], "coverage")
        self.assertEqual(slot["metric"], "production-file coverage, including inline test code")
        self.assertNotIn("github.com", json.dumps(slot))
        self.assertEqual(slot["counts"]["aggregate"]["lines"], {"covered": 105, "total": 125, "percent": 84.0})
        self.assertEqual(slot["scope"]["allowlisted_files"], 2)
        self.assertEqual(slot["scope"]["report_allowlist"], ["engine/src/**", "cli/src/**"])
        self.assertTrue(slot["tool"]["cargo_llvm_cov"]["digest_verified"])
        self.assertTrue(slot["tool"]["llvm_tools_preview"])
        self.assertEqual(slot["artifacts"], {
            "bundle": slot["artifacts"]["bundle"],
            "slot": slot["artifacts"]["bundle"] + "-slot",
            "retention_days": 90,
        })
        self.assertTrue(any("unmei-app" in line for line in slot["limitations"]))
        self.assertIn("## Coverage slot — passed", rendered)
        self.assertIn("unmei-app", rendered)
        self.assertNotIn("::warning::", printed)

    def test_the_slot_names_no_evidence_path(self):
        # The retained baseline bundle stays private and is excluded from the
        # export, so the record must not carry its path, in the JSON or in the
        # summary. The public baseline is the first public candidate's own
        # report, so nothing here needs to point at an earlier bundle.
        _code, _printed, slot, rendered = self.slot(
            "--outcome", "toolchain=success", "--outcome", "install=success",
            "--outcome", "witness=success", "--outcome", "measure=success",
            "--outcome", "export=success", "--outcome", "verify=success",
        )
        for text in (json.dumps(slot), rendered):
            self.assertNotIn("docs/evidence", text)
            self.assertNotIn("historical_baseline", text)

    def test_each_failure_lands_in_the_slot_with_its_own_status(self):
        cases = {
            ("toolchain", "failure"): "inconclusive",
            ("install", "failure"): "inconclusive",
            ("witness", "failure"): "failed",
            ("measure", "failure"): "failed",
            ("export", "failure"): "inconclusive",
            ("verify", "failure"): "failed",
        }
        for (failing, outcome), expected in cases.items():
            with self.subTest(step=failing):
                steps = ["toolchain", "install", "witness", "measure", "export", "verify"]
                argv = []
                for step in steps:
                    state = outcome if step == failing else "skipped"
                    if step != failing and steps.index(step) < steps.index(failing):
                        state = "success"
                    argv.extend(["--outcome", "%s=%s" % (step, state)])
                code, printed, slot, rendered = self.slot(*argv)
                self.assertEqual(code, 0, "a measurement failure must not fail the workflow")
                self.assertEqual(slot["status"], expected)
                self.assertIn("step outcome: " + outcome, slot["reason"])
                self.assertIn("::warning::coverage slot %s" % expected, printed)
                self.assertIn("Reason: ", rendered)

    def test_a_cancelled_step_is_reported_as_not_run(self):
        code, _printed, slot, _rendered = self.slot("--outcome", "toolchain=cancelled")
        self.assertEqual(code, 0)
        self.assertEqual(slot["status"], "inconclusive")
        self.assertIn("toolchain", slot["reason"])

    def test_a_digest_mismatch_is_visible_in_the_tool_identity(self):
        json_path, summary, html = Workspace(self.root).views(self.files)
        out = self.root / "coverage-slot.json"
        self.run_cli(
            "slot", "--sha", "b" * 40, "--json", str(json_path), "--summary", str(summary), "--html", str(html),
            "--tool-digest", "aaa", "--tool-digest-expected", "bbb",
            "--outcome", "toolchain=success", "--outcome", "install=failure",
            "--out", str(out),
        )
        slot = json.loads(out.read_text(encoding="utf-8"))
        self.assertFalse(slot["tool"]["cargo_llvm_cov"]["digest_verified"])
        self.assertEqual(slot["status"], "inconclusive")

    def test_an_unmeasurable_run_publishes_null_counts(self):
        out = self.root / "coverage-slot.json"
        code, _printed = self.run_cli(
            "slot", "--sha", "c" * 40, "--outcome", "toolchain=failure", "--out", str(out)
        )
        self.assertEqual(code, 0)
        slot = json.loads(out.read_text(encoding="utf-8"))
        self.assertIsNone(slot["counts"])
        self.assertIsNone(slot["scope"]["allowlisted_files"])
        self.assertEqual(slot["witness"]["status"], "not-run")
        self.assertFalse(slot["tool"]["llvm_tools_preview"])

    def test_stdout_without_out_carries_the_slot_json_alone(self):
        previous = sys.argv
        sys.argv = ["coverage_slot.py", "slot", "--sha", "9" * 40, "--outcome", "toolchain=failure"]
        self.addCleanup(setattr, sys, "argv", previous)
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(self.module.main(), 0)
        self.assertEqual(json.loads(out.getvalue())["status"], "inconclusive")

    def test_json_without_the_other_two_views_is_refused(self):
        json_path, _summary, _html = Workspace(self.root).views(self.files)
        with self.assertRaises(SystemExit):
            self.run_cli("slot", "--sha", "d" * 40, "--json", str(json_path))

    def test_an_unknown_step_name_is_refused(self):
        with self.assertRaises(SystemExit):
            self.run_cli("slot", "--sha", "e" * 40, "--outcome", "gate=success")

    def test_witness_receipts_are_read_from_the_captured_state(self):
        directory = self.root / "witness"
        directory.mkdir()
        (directory / "CAPTURE.txt").write_text("capture proved by help_shows_contract_surface\n", encoding="utf-8")
        (directory / "primary-check.txt").write_text(
            "witness target cli/src/main.rs: 22/279 lines covered (expected covered), cargo-llvm-cov 0.9.1\n",
            encoding="utf-8",
        )
        (directory / "primary-profiles.txt").write_text(
            "profraw before help_shows_contract_surface: 0\nprofraw after help_shows_contract_surface: 2\n",
            encoding="utf-8",
        )
        (directory / "negative-check.txt").write_text(
            "witness target cli/src/main.rs: 0/279 lines covered (expected uncovered), cargo-llvm-cov 0.9.1\n",
            encoding="utf-8",
        )
        (directory / "cleared.txt").write_text("witness state cleared, profraw now: 0\n", encoding="utf-8")
        out = self.root / "coverage-slot.json"
        self.run_cli(
            "slot", "--sha", "f" * 40, "--witness-dir", str(directory),
            "--outcome", "toolchain=success", "--outcome", "install=success", "--outcome", "witness=success",
            "--outcome", "measure=success", "--outcome", "export=success", "--outcome", "verify=success",
            "--out", str(out),
        )
        witness = json.loads(out.read_text(encoding="utf-8"))["witness"]
        self.assertEqual(witness["status"], "passed")
        self.assertEqual(witness["proving_test"], "help_shows_contract_surface")
        self.assertEqual(witness["covered_lines"], 22)
        self.assertEqual(witness["negative_control"]["covered_lines"], 0)
        self.assertEqual(witness["child_profiles"], {"before": 0, "after": 2})
        self.assertTrue(witness["state_cleared"])

    def test_a_positive_witness_without_a_child_hit_is_not_passed(self):
        directory = self.root / "witness"
        directory.mkdir()
        (directory / "CAPTURE.txt").write_text("capture proved by help_shows_contract_surface\n", encoding="utf-8")
        (directory / "primary-check.txt").write_text(
            "witness target cli/src/main.rs: 0/279 lines covered (expected covered)\n", encoding="utf-8"
        )
        (directory / "negative-check.txt").write_text(
            "witness target cli/src/main.rs: 0/279 lines covered (expected uncovered)\n", encoding="utf-8"
        )
        (directory / "cleared.txt").write_text("witness state cleared, profraw now: 0\n", encoding="utf-8")
        facts = self.module.witness_facts(directory)
        self.assertEqual(facts["status"], "failed")

    def test_a_non_zero_cleared_count_is_not_a_pass(self):
        directory = self.root / "witness"
        directory.mkdir()
        (directory / "CAPTURE.txt").write_text("capture proved by help_shows_contract_surface\n", encoding="utf-8")
        (directory / "primary-check.txt").write_text(
            "witness target cli/src/main.rs: 22/279 lines covered (expected covered)\n", encoding="utf-8"
        )
        (directory / "negative-check.txt").write_text(
            "witness target cli/src/main.rs: 0/279 lines covered (expected uncovered)\n", encoding="utf-8"
        )
        (directory / "cleared.txt").write_text("profraw now: 3\n", encoding="utf-8")
        self.assertEqual(self.module.witness_facts(directory)["status"], "failed")


if __name__ == "__main__":
    unittest.main(verbosity=2)
