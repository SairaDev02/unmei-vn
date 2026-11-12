#!/usr/bin/env python3
"""Logic tests for `scripts/candidate_record.py`.

These tests never touch the network and never read this repository's history.
Eligibility runs against a throwaway repository whose `origin` is a bare directory,
records and appendices are written into temporary directories, and the verify
kinds hash synthetic bytes.
"""

from __future__ import annotations

import copy
import hashlib
import importlib
import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("candidate_record.py")

MOMENT = "2026-01-01T00:00:00Z"
CANDIDATE = "a" * 40
COORDINATOR = "b" * 40
# An export names the private source commit, which is never the public candidate.
SOURCE = "c" * 40
DIGEST_1 = "1" * 64
DIGEST_2 = "2" * 64
DIGEST_3 = "3" * 64
VERSION = "0.1.0"
PLAYER_WINDOWS = f"unmei-play-{VERSION}-x86_64-windows.zip"
PLAYER_LINUX = f"unmei-play-{VERSION}-x86_64-linux.tar.gz"
ENGINE_CRATE = f"unmei-engine-{VERSION}.crate"
MAIN_REF = "refs/heads/main"


def facts(**overrides):
    base = {
        "candidate": {"sha": CANDIDATE, "version": VERSION},
        "coordinator": {"revision": COORDINATOR, "run_id": "100", "run_attempt": 1},
        "rehearsal": False,
        "environment": {
            "toolchain": {"rust": "1.91.0", "cargo": "1.91.0"},
            "cargo_lock": {"sha256": DIGEST_1, "bytes": 4200},
            "targets": ["x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu"],
            "profile": "release",
            "runner_images": {"windows": "windows-2025", "linux": "ubuntu-24.04"},
        },
        "jobs": [{"name": "packaging-linux", "run_attempt": 1,
                  "conclusion": "success", "earlier_failures": []}],
        "hashes": [
            {"job": "packaging-windows", "name": PLAYER_WINDOWS, "kind": "archive",
             "sha256": DIGEST_1, "bytes": 100},
            {"job": "packaging-windows", "name": ENGINE_CRATE, "kind": "crate",
             "sha256": DIGEST_1, "bytes": 200},
            {"job": "packaging-windows", "name": "unmei-play.exe", "kind": "executable",
             "sha256": DIGEST_1, "bytes": 50},
            {"job": "packaging-linux", "name": PLAYER_LINUX, "kind": "archive",
             "sha256": DIGEST_2, "bytes": 100},
            {"job": "packaging-linux", "name": ENGINE_CRATE, "kind": "crate",
             "sha256": DIGEST_1, "bytes": 200},
        ],
        "artifacts": [{"name": "linux-package", "id": "77",
                       "digest": f"sha256:{DIGEST_2}", "bytes": 300}],
        "slots": {"gate": {"status": "passed", "reason": None,
                           "hashes": [{"job": "packaging-linux", "name": ENGINE_CRATE}]}},
    }
    base.update(overrides)
    return base


def set_hash(base, job, name, digest):
    base = copy.deepcopy(base)
    for entry in base["hashes"]:
        if entry["job"] == job and entry["name"] == name:
            entry["sha256"] = digest
    return base


def load_candidate_record():
    spec = importlib.util.spec_from_file_location("unmei_candidate_record_under_test", SCRIPT)
    if spec is None or spec.loader is None:  # pragma: no cover - defensive
        raise ImportError(f"cannot load {SCRIPT}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def digest_of(data):
    return hashlib.sha256(data).hexdigest()


class EligibilityTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        load_candidate_record()
        cls.lib = sys.modules["candidate_record_lib"]

    def test_a_sha_that_is_not_40_lowercase_hex_is_refused(self) -> None:
        for sha in ("abc", "A" * 40, "g" * 40, "", None, "a" * 41, "a" * 39):
            reasons = self.lib.shape_refusals(sha, MAIN_REF)
            self.assertTrue(reasons, sha)
            self.assertIn("40 lowercase hex", reasons[0])

    def test_a_dispatch_off_main_is_refused(self) -> None:
        for ref in ("refs/heads/side", "refs/pull/1/merge", "refs/tags/v0.1.0", ""):
            reasons = self.lib.shape_refusals(CANDIDATE, ref)
            self.assertEqual(len(reasons), 1)
            self.assertIn("only refs/heads/main produces a candidate", reasons[0])

    def test_a_missing_commit_is_refused(self) -> None:
        reasons = self.lib.history_refusals(CANDIDATE, False, False)
        self.assertIn("missing after a full-history fetch", reasons[0])

    def test_a_commit_outside_main_is_refused(self) -> None:
        reasons = self.lib.history_refusals(CANDIDATE, True, False)
        self.assertIn("not equal to or an ancestor of origin/main", reasons[0])

    def test_a_coordinator_revision_that_is_not_a_sha_is_refused(self) -> None:
        self.assertEqual(self.lib.revision_refusals(COORDINATOR), [])
        self.assertIn("not 40 lowercase hex", self.lib.revision_refusals("main")[0])

    def test_every_reason_is_collected_before_any_history_is_read(self) -> None:
        reasons = self.lib.eligibility_refusals("abc", "refs/heads/side")
        self.assertEqual(len(reasons), 2)
        with self.assertRaises(self.lib.Refusal) as raised:
            self.lib.require_eligible("abc", "refs/heads/side")
        self.assertIn("refs/heads/main", str(raised.exception))

    def test_the_result_keeps_the_coordinator_revision_apart(self) -> None:
        result = self.lib.eligibility_result(CANDIDATE, VERSION, COORDINATOR)
        self.assertTrue(result["eligible"])
        self.assertEqual(result["candidate_sha"], CANDIDATE)
        self.assertEqual(result["coordinator_revision"], COORDINATOR)
        self.assertEqual(result["version"], VERSION)


class EligibilityCommandTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        load_candidate_record()
        cls.scratch = tempfile.TemporaryDirectory()
        cls.root = Path(cls.scratch.name)
        cls.repo = cls.root / "repo"
        cls.repo.mkdir()
        cls.git("init", "-b", "main", cwd=cls.repo)
        cls.git("config", "user.email", "test@example.invalid", cwd=cls.repo)
        cls.git("config", "user.name", "Test", cwd=cls.repo)
        (cls.repo / "Cargo.toml").write_text(
            '[workspace.package]\nversion = "0.1.0"\n', encoding="utf-8")
        cls.git("add", "Cargo.toml", cwd=cls.repo)
        cls.git("commit", "-m", "main", cwd=cls.repo)
        cls.git("init", "--bare", str(cls.root / "origin.git"), cwd=cls.root)
        cls.git("remote", "add", "origin", str(cls.root / "origin.git"), cwd=cls.repo)
        cls.git("push", "origin", "main", cwd=cls.repo)
        cls.main_sha = cls.rev("HEAD", cwd=cls.repo)
        cls.git("checkout", "-b", "side", cwd=cls.repo)
        (cls.repo / "side.txt").write_text("side\n", encoding="utf-8")
        cls.git("add", "side.txt", cwd=cls.repo)
        cls.git("commit", "-m", "side", cwd=cls.repo)
        cls.side_sha = cls.rev("HEAD", cwd=cls.repo)
        cls.git("checkout", "main", cwd=cls.repo)

    @classmethod
    def tearDownClass(cls) -> None:
        cls.scratch.cleanup()

    @classmethod
    def git(cls, *args, cwd):
        result = subprocess.run(["git", *args], cwd=str(cwd), text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        if result.returncode != 0:
            raise AssertionError(f"git {' '.join(args)}: {result.stderr}")
        return result

    @classmethod
    def rev(cls, ref, cwd):
        return cls.git("rev-parse", ref, cwd=cwd).stdout.strip()

    def run_cli(self, *args):
        return subprocess.run([sys.executable, str(SCRIPT), *args], cwd=str(self.root),
                              text=True, stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE)

    def eligibility(self, sha, ref=MAIN_REF):
        return self.run_cli("eligibility", "--sha", sha, "--ref", ref,
                            "--coordinator-revision", COORDINATOR,
                            "--repo", str(self.repo))

    def test_an_ancestor_of_main_is_eligible_and_reports_the_version(self) -> None:
        out = self.root / "eligible.json"
        result = self.run_cli("eligibility", "--sha", self.main_sha, "--ref", MAIN_REF,
                              "--coordinator-revision", COORDINATOR,
                              "--repo", str(self.repo), "--json-out", str(out))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("version: 0.1.0", result.stdout)
        self.assertIn("recorded separately, not the candidate", result.stdout)
        payload = json.loads(out.read_text(encoding="utf-8"))
        self.assertEqual(payload["candidate_sha"], self.main_sha)
        self.assertEqual(payload["coordinator_revision"], COORDINATOR)
        self.assertEqual(payload["version"], "0.1.0")

    def test_a_commit_on_a_side_branch_is_refused(self) -> None:
        result = self.eligibility(self.side_sha)
        self.assertEqual(result.returncode, 1)
        self.assertIn("not equal to or an ancestor of origin/main", result.stderr)

    def test_a_commit_that_is_not_in_the_clone_is_refused(self) -> None:
        result = self.eligibility("0" * 40)
        self.assertEqual(result.returncode, 1)
        self.assertIn("missing after a full-history fetch", result.stderr)

    def test_a_short_sha_is_refused_before_git_runs(self) -> None:
        result = self.eligibility(self.main_sha[:8])
        self.assertEqual(result.returncode, 1)
        self.assertIn("40 lowercase hex", result.stderr)

    def test_a_dispatch_from_another_ref_is_refused(self) -> None:
        result = self.eligibility(self.main_sha, ref="refs/heads/side")
        self.assertEqual(result.returncode, 1)
        self.assertIn("only refs/heads/main produces a candidate", result.stderr)


class RecordTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        load_candidate_record()
        cls.lib = sys.modules["candidate_record_lib"]
        cls.facts = sys.modules["candidate_record_facts"]

    def test_the_record_carries_every_field_and_the_twelve_slots(self) -> None:
        record = self.lib.build_record(facts(), None, MOMENT)
        self.assertEqual(record["statement"], "machine evidence only, not certified")
        self.assertEqual(record["schema"], 1)
        self.assertEqual(record["recorded_at"], MOMENT)
        self.assertFalse(record["rehearsal"])
        self.assertEqual(record["candidate"], {"sha": CANDIDATE, "version": VERSION})
        self.assertEqual(record["coordinator"],
                         {"revision": COORDINATOR, "run_id": "100", "run_attempt": 1})
        environment = record["environment"]
        self.assertEqual(environment["toolchain"]["rust"], "1.91.0")
        self.assertEqual(environment["cargo_lock"], {"sha256": DIGEST_1, "bytes": 4200})
        self.assertEqual(len(environment["targets"]), 2)
        self.assertEqual(environment["profile"], "release")
        self.assertEqual(environment["runner_images"]["linux"], "ubuntu-24.04")
        self.assertEqual(record["jobs"][0]["earlier_failures"], [])
        self.assertEqual(len(record["hashes"]), 5)
        for entry in record["hashes"]:
            self.assertIn(entry["kind"], self.facts.HASH_KINDS)
            self.assertIn(entry["name"], (PLAYER_WINDOWS, PLAYER_LINUX, ENGINE_CRATE,
                                          "unmei-play.exe"))
            self.assertGreater(entry["bytes"], 0)
        self.assertEqual(record["artifacts"][0]["digest"], f"sha256:{DIGEST_2}")
        self.assertEqual([slot["name"] for slot in record["slots"]], list(self.facts.SLOTS))
        self.assertEqual(len(record["slots"]), 12)
        for slot in record["slots"]:
            self.assertIn(slot["status"], self.facts.SLOT_STATUSES)
        self.assertEqual([residual["id"] for residual in record["residuals"]],
                         [key for key, _ in self.facts.RESIDUALS])
        for residual in record["residuals"]:
            self.assertEqual(residual["status"], "pending")

    def test_a_slot_the_facts_file_omits_is_not_run(self) -> None:
        record = self.lib.build_record(facts(), None, MOMENT)
        self.assertEqual(self.lib.slot_of(record, "gate")["status"], "passed")
        for name in ("coverage", "accept-linux", "human"):
            self.assertEqual(self.lib.slot_of(record, name)["status"], "not-run")

    def test_a_slot_keeps_the_hashes_it_cited(self) -> None:
        record = self.lib.build_record(facts(), None, MOMENT)
        gate = self.lib.slot_of(record, "gate")
        self.assertEqual(gate["hashes"],
                         [{"job": "packaging-linux", "name": ENGINE_CRATE}])

    def test_crate_parity_passes_when_the_two_crate_hashes_match(self) -> None:
        record = self.lib.build_record(facts(), None, MOMENT)
        self.assertEqual(self.lib.slot_of(record, "crate-parity")["status"], "passed")
        self.assertEqual(len(self.lib.slot_of(record, "crate-parity")["hashes"]), 2)

    def test_crate_parity_fails_on_a_mismatch_and_overrides_a_supplied_pass(self) -> None:
        mismatched = facts(slots={"crate-parity": {"status": "passed", "reason": None,
                                                   "hashes": []}})
        mismatched = set_hash(mismatched, "packaging-linux", ENGINE_CRATE, DIGEST_3)
        record = self.lib.build_record(mismatched, None, MOMENT)
        parity = self.lib.slot_of(record, "crate-parity")
        self.assertEqual(parity["status"], "failed")
        self.assertIn("hashes differ", parity["reason"])

    def test_crate_parity_is_not_run_when_a_packaging_job_reports_no_crate(self) -> None:
        partial = copy.deepcopy(facts())
        partial["hashes"] = [entry for entry in partial["hashes"]
                             if entry["job"] != "packaging-linux"
                             or entry["name"] != ENGINE_CRATE]
        record = self.lib.build_record(partial, None, MOMENT)
        parity = self.lib.slot_of(record, "crate-parity")
        self.assertEqual(parity["status"], "not-run")
        self.assertIn("reported no", parity["reason"])

    def test_a_rerun_that_replaced_a_hash_resets_the_slots_that_cited_it(self) -> None:
        first = self.lib.build_record(facts(), None, MOMENT)
        second_facts = facts()
        second_facts["coordinator"]["run_attempt"] = 2
        second_facts["hashes"] = [entry for entry in second_facts["hashes"]
                                  if entry["name"] != ENGINE_CRATE] + [
            {"job": "packaging-windows", "name": ENGINE_CRATE, "kind": "crate",
             "sha256": DIGEST_3, "bytes": 200},
            {"job": "packaging-linux", "name": ENGINE_CRATE, "kind": "crate",
             "sha256": DIGEST_3, "bytes": 200}]
        second_facts["slots"]["gate"] = {
            "status": "passed", "reason": None,
            "hashes": [{"job": "packaging-linux", "name": ENGINE_CRATE}]}
        second_facts["slots"]["deny"] = {
            "status": "passed", "reason": None,
            "hashes": [{"job": "packaging-linux", "name": PLAYER_LINUX}]}

        second = self.lib.build_record(second_facts, first, MOMENT)
        gate = self.lib.slot_of(second, "gate")
        self.assertEqual(gate["status"], "pending")
        self.assertIn(f"packaging-linux:{ENGINE_CRATE}", gate["reason"])
        self.assertIn("re-run replaced", gate["reason"])
        # A slot that cited bytes the re-run did not replace keeps its result.
        self.assertEqual(self.lib.slot_of(second, "deny")["status"], "passed")
        # Parity is derived from the new bytes, so it never goes pending.
        self.assertEqual(self.lib.slot_of(second, "crate-parity")["status"], "passed")

    def test_an_unchanged_rerun_resets_nothing(self) -> None:
        first = self.lib.build_record(facts(), None, MOMENT)
        second_facts = facts()
        second_facts["coordinator"]["run_attempt"] = 2
        second = self.lib.build_record(second_facts, first, MOMENT)
        self.assertEqual(self.lib.slot_of(second, "gate")["status"], "passed")
        self.assertEqual(self.lib.replaced_hashes(first, {
            self.lib.hash_key(entry): entry for entry in second["hashes"]}), {})

    def test_a_rehearsal_flag_cannot_be_cleared_by_a_rerun(self) -> None:
        first = self.lib.build_record(facts(rehearsal=True), None, MOMENT)
        second_facts = facts(rehearsal=False)
        second_facts["coordinator"]["run_attempt"] = 2
        second = self.lib.build_record(second_facts, first, MOMENT)
        self.assertTrue(second["rehearsal"])
        self.assertFalse(self.lib.is_candidate_evidence(second))
        self.assertTrue(self.lib.is_candidate_evidence(first | {"rehearsal": False}))

    def test_a_new_run_id_is_not_stamped_by_an_earlier_rehearsal(self) -> None:
        first = self.lib.build_record(facts(rehearsal=True), None, MOMENT)
        second_facts = facts(rehearsal=False)
        second_facts["coordinator"]["run_id"] = "200"
        second = self.lib.build_record(second_facts, first, MOMENT)
        self.assertFalse(second["rehearsal"])
        self.assertTrue(self.lib.is_candidate_evidence(second))

    def test_a_certification_field_is_refused_wherever_it_hides(self) -> None:
        top = facts()
        top["certified"] = True
        with self.assertRaises(self.lib.Refusal) as raised:
            self.lib.build_record(top, None, MOMENT)
        self.assertIn("facts.certified", str(raised.exception))

        nested = facts()
        nested["slots"]["human"] = {"status": "passed", "reason": None, "hashes": [],
                                    "certification": "signed"}
        with self.assertRaises(self.lib.Refusal) as raised:
            self.lib.build_record(nested, None, MOMENT)
        self.assertIn("facts.slots.human.certification", str(raised.exception))

    def test_the_record_never_carries_a_certification_key(self) -> None:
        record = self.lib.build_record(facts(), None, MOMENT)
        self.assertEqual(self.facts.certification_paths(record), [])
        self.assertNotIn('"certified"', json.dumps(record))
        self.assertNotIn('"certification"', json.dumps(record))

    def test_an_unknown_slot_name_is_refused(self) -> None:
        wrong = facts(slots={"gates": {"status": "passed", "reason": None, "hashes": []}})
        with self.assertRaises(self.lib.Refusal) as raised:
            self.lib.build_record(wrong, None, MOMENT)
        self.assertIn("unknown slot(s) gates", str(raised.exception))
        self.assertIn("crate-parity", str(raised.exception))

    def test_an_unknown_status_is_refused(self) -> None:
        wrong = facts()
        wrong["slots"]["gate"] = {"status": "green", "reason": None, "hashes": []}
        with self.assertRaises(self.lib.Refusal) as raised:
            self.lib.build_record(wrong, None, MOMENT)
        self.assertIn("is not one of passed, failed, pending, not-run, inconclusive",
                      str(raised.exception))

    def test_a_residual_is_never_a_pass(self) -> None:
        for status in ("passed", "failed"):
            wrong = facts(residuals={"linux-rendered-walk": {"status": status}})
            with self.assertRaises(self.lib.Refusal) as raised:
                self.lib.build_record(wrong, None, MOMENT)
            self.assertIn("a residual is never a pass", str(raised.exception))

        inconclusive = facts(residuals={"linux-rendered-walk": {
            "status": "inconclusive", "reason": "the maintainer has no Linux GPU"}})
        record = self.lib.build_record(inconclusive, None, MOMENT)
        residual = next(item for item in record["residuals"]
                        if item["id"] == "linux-rendered-walk")
        self.assertEqual(residual["status"], "inconclusive")

    def test_an_unknown_residual_is_refused(self) -> None:
        wrong = facts(residuals={"linux-rendered-walk-2": {"status": "pending"}})
        with self.assertRaises(self.lib.Refusal) as raised:
            self.lib.build_record(wrong, None, MOMENT)
        self.assertIn("unknown residual(s)", str(raised.exception))

    def test_a_malformed_hash_is_refused(self) -> None:
        for broken in (
            {"job": "packaging-linux", "name": "x.crate", "kind": "tarball",
             "sha256": DIGEST_1, "bytes": 1},
            {"job": "packaging-linux", "name": "x.crate", "kind": "crate",
             "sha256": "ABC", "bytes": 1},
            {"job": "packaging-linux", "name": "x.crate", "kind": "crate",
             "sha256": DIGEST_1, "bytes": -1},
        ):
            wrong = copy.deepcopy(facts())
            wrong["hashes"].append(broken)
            with self.assertRaises(self.lib.Refusal):
                self.lib.build_record(wrong, None, MOMENT)

    def test_one_artifact_one_hash(self) -> None:
        wrong = copy.deepcopy(facts())
        wrong["hashes"].append({"job": "packaging-linux", "name": PLAYER_LINUX,
                                "kind": "archive", "sha256": DIGEST_3, "bytes": 100})
        with self.assertRaises(self.lib.Refusal) as raised:
            self.lib.build_record(wrong, None, MOMENT)
        self.assertIn("lists " + PLAYER_LINUX + " twice", str(raised.exception))


class RecordCommandTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        load_candidate_record()
        cls.facts = sys.modules["candidate_record_facts"]

    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.addCleanup(self.tempdir.cleanup)
        self.root = Path(self.tempdir.name)

    def write_facts(self, payload, name="facts.json"):
        path = self.root / name
        path.write_text(json.dumps(payload), encoding="utf-8")
        return path

    def record(self, facts_path, out, *extra):
        return subprocess.run(
            [sys.executable, str(SCRIPT), "record", "--facts", str(facts_path),
             "--message", str(self.write_message()), "--out", str(out),
             "--recorded-at", MOMENT, *extra],
            cwd=str(self.root), text=True, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE)

    def write_message(self, text="A commit that is not an export\n"):
        path = self.root / "candidate-message.txt"
        path.write_text(text, encoding="utf-8")
        return path

    def test_the_record_and_its_sha256_are_written_once_and_never_edited(self) -> None:
        out = self.root / "record"
        first = self.record(self.write_facts(facts()), out)
        self.assertEqual(first.returncode, 0, first.stderr)
        record_path = out / "candidate-record.json"
        sum_path = out / "candidate-record.json.sha256"
        data = record_path.read_bytes()
        digest = digest_of(data)
        self.assertIn(digest, first.stdout)
        self.assertEqual(sum_path.read_text(encoding="utf-8"),
                         f"{digest}  candidate-record.json\n")
        self.assertEqual(json.loads(data)["statement"],
                         "machine evidence only, not certified")

        second = self.record(self.write_facts(facts(), "facts2.json"), out)
        self.assertEqual(second.returncode, 1)
        self.assertIn("written once and never edited", second.stderr)
        self.assertEqual(record_path.read_bytes(), data)
        self.assertEqual(sum_path.read_text(encoding="utf-8"),
                         f"{digest}  candidate-record.json\n")

    def test_the_command_lists_every_slot_and_residual(self) -> None:
        result = self.record(self.write_facts(facts()), self.root / "record")
        self.assertEqual(result.returncode, 0, result.stderr)
        for name in self.facts.SLOTS:
            self.assertIn(name, result.stdout)
        for name, _ in self.facts.RESIDUALS:
            self.assertIn(name, result.stdout)
        self.assertIn("residuals (never a pass)", result.stdout)

    def test_the_earlier_record_resets_a_slot_through_the_command(self) -> None:
        out = self.root / "attempt1"
        self.assertEqual(self.record(self.write_facts(facts()), out).returncode, 0)
        second_facts = facts()
        second_facts["coordinator"]["run_attempt"] = 2
        second_facts["hashes"] = [
            entry for entry in second_facts["hashes"]
            if entry["name"] != ENGINE_CRATE] + [
            {"job": "packaging-windows", "name": ENGINE_CRATE, "kind": "crate",
             "sha256": DIGEST_3, "bytes": 200},
            {"job": "packaging-linux", "name": ENGINE_CRATE, "kind": "crate",
             "sha256": DIGEST_3, "bytes": 200}]
        result = self.record(self.write_facts(second_facts, "facts2.json"),
                             self.root / "attempt2",
                             "--previous", str(out / "candidate-record.json"))
        self.assertEqual(result.returncode, 0, result.stderr)
        second = json.loads((self.root / "attempt2" / "candidate-record.json")
                            .read_text(encoding="utf-8"))
        gate = next(slot for slot in second["slots"] if slot["name"] == "gate")
        self.assertEqual(gate["status"], "pending")

    def test_a_rehearsal_record_says_so_in_the_output(self) -> None:
        result = self.record(self.write_facts(facts(rehearsal=True)), self.root / "record")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("not candidate evidence", result.stdout)


class ProvenanceTests(unittest.TestCase):
    """The record copies an export's provenance lines out of the candidate's message."""

    @classmethod
    def setUpClass(cls) -> None:
        load_candidate_record()
        cls.lib = sys.modules["candidate_record_lib"]
        cls.format = sys.modules["export_provenance"]

    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.addCleanup(self.tempdir.cleanup)
        self.root = Path(self.tempdir.name)

    def export_message(self, number=1):
        """An export commit's message, written by the format the tool ships."""
        return self.format.Record(number, VERSION, SOURCE, DIGEST_2, DIGEST_3).message()

    def record(self, message, out, name="facts.json"):
        facts_path = self.root / name
        facts_path.write_text(json.dumps(facts()), encoding="utf-8")
        message_path = self.root / (name + ".message")
        message_path.write_text(message, encoding="utf-8")
        return subprocess.run(
            [sys.executable, str(SCRIPT), "record", "--facts", str(facts_path),
             "--message", str(message_path), "--out", str(self.root / out),
             "--recorded-at", MOMENT],
            cwd=str(self.root), text=True, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE)

    def written(self, out):
        path = self.root / out / "candidate-record.json"
        return json.loads(path.read_text(encoding="utf-8"))

    def test_an_export_commit_fills_the_object_with_its_four_values(self) -> None:
        result = self.record(self.export_message(), "record")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.written("record")["provenance"],
                         {"number": 1, "source": SOURCE,
                          "allowlist_digest": DIGEST_2, "manifest_digest": DIGEST_3})
        self.assertIn("export 1", result.stdout)

    def test_the_object_copies_the_number_the_message_carries(self) -> None:
        result = self.record(self.export_message(number=7), "record")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.written("record")["provenance"]["number"], 7)

    def test_any_other_commit_leaves_it_null_and_still_writes_the_record(self) -> None:
        others = {
            "rehearsal": "Rehearsal of the 0.1.0 candidate\n",
            "merge": "merge: the crate overlay and the record\n",
            "mentions": "Note that Export 1: Unmei 0.1.0 names its source\n",
            "subject-only": "Export 1: Unmei 0.1.0\n",
            "trailer": self.export_message()
                       + "Signed-off-by: Someone <someone@example.invalid>\n",
        }
        for name, message in others.items():
            with self.subTest(name):
                result = self.record(message, name, name=name + ".json")
                self.assertEqual(result.returncode, 0, result.stderr)
                record = self.written(name)
                self.assertIsNone(record["provenance"])
                self.assertEqual(record["candidate"]["sha"], CANDIDATE)
                self.assertIn("provenance: null", result.stdout)

    def test_an_export_message_with_an_unusable_value_is_refused(self) -> None:
        message = self.format.Record(1, VERSION, SOURCE, "not-a-digest",
                                     DIGEST_3).message()
        result = self.record(message, "refused")
        self.assertEqual(result.returncode, 1)
        self.assertIn("allowlist digest", result.stderr)
        self.assertFalse((self.root / "refused").exists())


class VerifyTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        load_candidate_record()
        cls.lib = sys.modules["candidate_record_lib"]
        cls.facts = sys.modules["candidate_record_facts"]
        cls.appendices = sys.modules["candidate_record_appendix"]

    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.addCleanup(self.tempdir.cleanup)
        self.root = Path(self.tempdir.name)

    def write_record(self, crate_digest, **overrides):
        payload = set_hash(set_hash(facts(**overrides), "packaging-linux",
                                    ENGINE_CRATE, crate_digest),
                           "packaging-windows", ENGINE_CRATE, crate_digest)
        record = self.lib.build_record(payload, None, MOMENT)
        path = self.root / "candidate-record.json"
        path.write_bytes(self.lib.record_bytes(record))
        return path, record

    def write_appendix(self, record_path, record, stage="engine-publication"):
        path = self.root / f"appendix-{stage}.md"
        path.write_text(self.appendices.template(
            stage, record, self.lib.sha256_file(record_path), "Jesse", MOMENT,
            "Windows 11, RTX 3070"), encoding="utf-8")
        return path

    def verify(self, *args):
        return subprocess.run([sys.executable, str(SCRIPT), "verify", *args,
                               "--time", MOMENT],
                              cwd=str(self.root), text=True,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE)

    def test_a_matching_local_crate_verifies_and_appends_above_the_sign_off(self) -> None:
        payload = b"the staged .crate bytes"
        digest = digest_of(payload)
        record_path, record = self.write_record(digest)
        crate = self.root / ENGINE_CRATE
        crate.write_bytes(payload)
        appendix = self.write_appendix(record_path, record)

        result = self.verify("local-crate", "--record", str(record_path),
                             "--appendix", str(appendix), "--crate", str(crate),
                             "--name", "unmei-engine")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("verify local-crate: ok", result.stdout)
        text = appendix.read_text(encoding="utf-8")
        self.assertIn("### verify local-crate — ok", text)
        self.assertIn(f"expected `{digest}`", text)
        self.assertTrue(text.rstrip().endswith(self.appendices.SIGN_OFF_LINE))

    def test_a_mismatched_crate_is_refused_and_still_recorded(self) -> None:
        record_path, record = self.write_record(digest_of(b"the staged bytes"))
        crate = self.root / ENGINE_CRATE
        crate.write_bytes(b"other bytes")
        appendix = self.write_appendix(record_path, record)

        result = self.verify("local-crate", "--record", str(record_path),
                             "--appendix", str(appendix), "--crate", str(crate),
                             "--name", "unmei-engine")
        self.assertEqual(result.returncode, 1)
        self.assertIn("not the ones the record staged", result.stderr)
        text = appendix.read_text(encoding="utf-8")
        self.assertIn("### verify local-crate — mismatch", text)
        self.assertIn(f"found `{digest_of(b'other bytes')}`", text)

    def test_a_crate_under_another_name_is_a_mismatch(self) -> None:
        payload = b"crate bytes"
        record_path, record = self.write_record(digest_of(payload))
        crate = self.root / "renamed.crate"
        crate.write_bytes(payload)
        appendix = self.write_appendix(record_path, record)
        result = self.verify("local-crate", "--record", str(record_path),
                             "--appendix", str(appendix), "--crate", str(crate),
                             "--name", "unmei-engine")
        self.assertEqual(result.returncode, 1)
        self.assertIn("file name", result.stdout)

    def test_verify_refuses_a_record_without_a_linux_crate(self) -> None:
        partial = copy.deepcopy(facts())
        partial["hashes"] = [entry for entry in partial["hashes"]
                             if entry["job"] != "packaging-linux"]
        record = self.lib.build_record(partial, None, MOMENT)
        record_path = self.root / "candidate-record.json"
        record_path.write_bytes(self.lib.record_bytes(record))
        crate = self.root / ENGINE_CRATE
        crate.write_bytes(b"crate")
        appendix = self.write_appendix(record_path, record, stage="residuals")
        result = self.verify("local-crate", "--record", str(record_path),
                             "--appendix", str(appendix), "--crate", str(crate),
                             "--name", "unmei-engine")
        self.assertEqual(result.returncode, 1)
        self.assertIn("stages no packaging-linux", result.stderr)

    def test_the_sparse_index_cksum_is_checked_at_its_crate_path(self) -> None:
        digest = digest_of(b"crate")
        record_path, record = self.write_record(digest)
        index = self.root / "un" / "me" / "unmei-engine"
        index.parent.mkdir(parents=True)
        index.write_text(json.dumps({"name": "unmei-engine", "vers": VERSION,
                                     "cksum": digest}) + "\n", encoding="utf-8")
        appendix = self.write_appendix(record_path, record)

        ok = self.verify("index-cksum", "--record", str(record_path),
                         "--appendix", str(appendix), "--index-file", str(index),
                         "--name", "unmei-engine")
        self.assertEqual(ok.returncode, 0, ok.stderr)
        self.assertIn("### verify index-cksum — ok", appendix.read_text(encoding="utf-8"))

    def test_a_sparse_index_cksum_mismatch_is_refused(self) -> None:
        record_path, record = self.write_record(digest_of(b"crate"))
        index = self.root / "un" / "me" / "unmei-engine"
        index.parent.mkdir(parents=True)
        index.write_text(json.dumps({"name": "unmei-engine", "vers": VERSION,
                                     "cksum": DIGEST_3}) + "\n", encoding="utf-8")
        appendix = self.write_appendix(record_path, record)
        result = self.verify("index-cksum", "--record", str(record_path),
                             "--appendix", str(appendix), "--index-file", str(index),
                             "--name", "unmei-engine")
        self.assertEqual(result.returncode, 1)
        self.assertIn("cksum", result.stdout)

    def test_a_sparse_index_at_another_path_is_a_mismatch(self) -> None:
        digest = digest_of(b"crate")
        record_path, record = self.write_record(digest)
        index = self.root / "elsewhere" / "unmei-engine"
        index.parent.mkdir(parents=True)
        index.write_text(json.dumps({"vers": VERSION, "cksum": digest}) + "\n",
                         encoding="utf-8")
        appendix = self.write_appendix(record_path, record)
        result = self.verify("index-cksum", "--record", str(record_path),
                             "--appendix", str(appendix), "--index-file", str(index),
                             "--name", "unmei-engine")
        self.assertEqual(result.returncode, 1)
        self.assertIn("sparse-index path: expected un/me/unmei-engine", result.stdout)

    def test_a_sparse_index_without_the_version_is_refused(self) -> None:
        record_path, record = self.write_record(digest_of(b"crate"))
        index = self.root / "un" / "me" / "unmei-engine"
        index.parent.mkdir(parents=True)
        index.write_text(json.dumps({"vers": "0.0.9", "cksum": DIGEST_1}) + "\n",
                         encoding="utf-8")
        appendix = self.write_appendix(record_path, record)
        result = self.verify("index-cksum", "--record", str(record_path),
                             "--appendix", str(appendix), "--index-file", str(index),
                             "--name", "unmei-engine")
        self.assertEqual(result.returncode, 1)
        self.assertIn("lists no " + VERSION, result.stderr)

    def test_the_downloaded_crate_names_where_the_bytes_came_from(self) -> None:
        payload = b"downloaded crate bytes"
        record_path, record = self.write_record(digest_of(payload))
        crate = self.root / ENGINE_CRATE
        crate.write_bytes(payload)
        appendix = self.write_appendix(record_path, record)
        result = self.verify("download-crate", "--record", str(record_path),
                             "--appendix", str(appendix), "--crate", str(crate),
                             "--name", "unmei-engine")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("### verify download-crate — ok",
                      appendix.read_text(encoding="utf-8"))

    def test_release_assets_are_checked_against_every_sum(self) -> None:
        record_path, record = self.write_record(digest_of(b"crate"))
        downloads = self.root / "downloads"
        downloads.mkdir()
        windows = b"windows archive"
        linux = b"linux archive"
        (downloads / PLAYER_WINDOWS).write_bytes(windows)
        (downloads / PLAYER_LINUX).write_bytes(linux)
        sums = self.root / "SHA256SUMS"
        sums.write_text(f"{digest_of(windows)}  {PLAYER_WINDOWS}\n"
                        f"{digest_of(linux)}  {PLAYER_LINUX}\n", encoding="utf-8")
        payload = set_hash(set_hash(facts(), "packaging-windows", PLAYER_WINDOWS,
                                    digest_of(windows)),
                           "packaging-linux", PLAYER_LINUX, digest_of(linux))
        record = self.lib.build_record(payload, None, MOMENT)
        record_path.write_bytes(self.lib.record_bytes(record))
        appendix = self.write_appendix(record_path, record, stage="release-assets")

        result = self.verify("release-assets", "--record", str(record_path),
                             "--appendix", str(appendix), "--dir", str(downloads),
                             "--sums", str(sums))
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("### verify release-assets — ok",
                      appendix.read_text(encoding="utf-8"))

    def test_a_missing_or_wrong_release_asset_is_refused(self) -> None:
        record_path, record = self.write_record(digest_of(b"crate"))
        downloads = self.root / "downloads"
        downloads.mkdir()
        (downloads / PLAYER_WINDOWS).write_bytes(b"tampered")
        sums = self.root / "SHA256SUMS"
        sums.write_text(f"{digest_of(b'windows archive')}  {PLAYER_WINDOWS}\n"
                        f"{digest_of(b'linux archive')}  {PLAYER_LINUX}\n",
                        encoding="utf-8")
        appendix = self.write_appendix(record_path, record, stage="release-assets")
        result = self.verify("release-assets", "--record", str(record_path),
                             "--appendix", str(appendix), "--dir", str(downloads),
                             "--sums", str(sums))
        self.assertEqual(result.returncode, 1)
        self.assertIn("missing from every --dir", result.stdout)
        self.assertIn(f"download of {PLAYER_WINDOWS} in downloads", result.stdout)
        # The record's staged archives do not match this SHA256SUMS either.
        self.assertIn(f"SHA256SUMS covers {PLAYER_WINDOWS} from packaging-windows",
                      result.stdout)

    def test_a_staged_archive_missing_from_the_sums_is_a_mismatch(self) -> None:
        record_path, record = self.write_record(digest_of(b"crate"))
        downloads = self.root / "downloads"
        downloads.mkdir()
        (downloads / PLAYER_WINDOWS).write_bytes(b"windows archive")
        sums = self.root / "SHA256SUMS"
        sums.write_text(f"{digest_of(b'windows archive')}  {PLAYER_WINDOWS}\n",
                        encoding="utf-8")
        appendix = self.write_appendix(record_path, record, stage="release-assets")
        result = self.verify("release-assets", "--record", str(record_path),
                             "--appendix", str(appendix), "--dir", str(downloads),
                             "--sums", str(sums))
        self.assertEqual(result.returncode, 1)
        self.assertIn(f"SHA256SUMS covers {PLAYER_LINUX}", result.stdout)

    def test_verify_refuses_an_appendix_that_cites_another_record(self) -> None:
        payload = b"crate bytes"
        record_path, record = self.write_record(digest_of(payload))
        other = self.root / "other-record.json"
        other.write_bytes(self.lib.record_bytes(
            self.lib.build_record(facts(), None, "2026-02-02T00:00:00Z")))
        appendix = self.write_appendix(other, record, stage="engine-publication")
        crate = self.root / ENGINE_CRATE
        crate.write_bytes(payload)
        result = self.verify("local-crate", "--record", str(record_path),
                             "--appendix", str(appendix), "--crate", str(crate),
                             "--name", "unmei-engine")
        self.assertEqual(result.returncode, 1)
        self.assertIn("does not cite this record's SHA-256", result.stderr)

    def test_verify_refuses_a_file_that_is_not_an_appendix(self) -> None:
        record_path, record = self.write_record(digest_of(b"crate"))
        appendix = self.root / "notes.md"
        appendix.write_text("# Notes\n", encoding="utf-8")
        crate = self.root / ENGINE_CRATE
        crate.write_bytes(b"crate")
        result = self.verify("local-crate", "--record", str(record_path),
                             "--appendix", str(appendix), "--crate", str(crate),
                             "--name", "unmei-engine")
        self.assertEqual(result.returncode, 1)
        self.assertIn("not a candidate-record appendix", result.stderr)


class AppendixTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        load_candidate_record()
        cls.lib = sys.modules["candidate_record_lib"]
        cls.facts = sys.modules["candidate_record_facts"]
        cls.appendices = sys.modules["candidate_record_appendix"]

    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.addCleanup(self.tempdir.cleanup)
        self.root = Path(self.tempdir.name)
        self.record = self.lib.build_record(facts(), None, MOMENT)
        self.record_path = self.root / "candidate-record.json"
        self.record_path.write_bytes(self.lib.record_bytes(self.record))
        self.digest = digest_of(self.record_path.read_bytes())

    def appendix(self, stage, out=None, *extra):
        return subprocess.run(
            [sys.executable, str(SCRIPT), "appendix", "new", stage,
             "--record", str(self.record_path),
             "--out", str(out or self.root / f"{stage}.md"),
             "--tester", "Jesse", "--environment", "Windows 11, RTX 3070",
             "--time", MOMENT, *extra],
            cwd=str(self.root), text=True, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE)

    def test_every_stage_writes_a_header_that_cites_the_record(self) -> None:
        self.assertEqual(set(self.appendices.STAGES),
                         {"windows-walk", "stranger", "residuals",
                          "engine-publication", "cli-publication", "release-assets"})
        for stage in self.appendices.STAGES:
            out = self.root / f"{stage}.md"
            result = self.appendix(stage, out)
            self.assertEqual(result.returncode, 0, result.stderr)
            text = out.read_text(encoding="utf-8")
            self.assertTrue(text.startswith(f"# Appendix: {self.appendices.STAGES[stage]}"))
            for expected in (f"- Record SHA-256: `{self.digest}`",
                             f"- Candidate SHA: `{CANDIDATE}`",
                             f"- Version: `{VERSION}`",
                             "- Rehearsal: `false`",
                             "- Hashes exercised:",
                             "- Tester: Jesse",
                             f"- Time (UTC): {MOMENT}",
                             "- Environment: Windows 11, RTX 3070"):
                self.assertIn(expected, text)
            self.assertTrue(text.rstrip().endswith(self.appendices.SIGN_OFF_LINE))
            self.assertEqual(self.lib.sha256_file(self.record_path), self.digest)

    def test_each_stage_cites_the_hashes_it_exercises(self) -> None:
        engine = self.appendices.exercised_hashes("engine-publication", self.record)
        self.assertEqual([entry["name"] for entry in engine], [ENGINE_CRATE])
        self.assertEqual(engine[0]["job"], "packaging-linux")

        walk = self.appendices.exercised_hashes("windows-walk", self.record)
        self.assertEqual({entry["job"] for entry in walk}, {"packaging-windows"})
        self.assertEqual({entry["kind"] for entry in walk}, {"archive", "executable"})

        stranger = self.appendices.exercised_hashes("stranger", self.record)
        self.assertEqual({entry["kind"] for entry in stranger}, {"archive"})

        assets = self.appendices.exercised_hashes("release-assets", self.record)
        self.assertEqual({entry["name"] for entry in assets},
                         {PLAYER_WINDOWS, PLAYER_LINUX})
        self.assertEqual(len(self.appendices.exercised_hashes("residuals", self.record)),
                         len(self.record["hashes"]))

    def test_a_rehearsal_record_says_so_in_the_header(self) -> None:
        rehearsal = self.lib.build_record(facts(rehearsal=True), None, MOMENT)
        self.record_path.write_bytes(self.lib.record_bytes(rehearsal))
        result = self.appendix("windows-walk")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("- Rehearsal: `true`",
                      (self.root / "windows-walk.md").read_text(encoding="utf-8"))
        self.assertIn("not certification evidence", result.stdout)

    def test_an_appendix_is_never_overwritten(self) -> None:
        out = self.root / "windows-walk.md"
        self.assertEqual(self.appendix("windows-walk", out).returncode, 0)
        before = out.read_bytes()
        second = self.appendix("windows-walk", out)
        self.assertEqual(second.returncode, 1)
        self.assertIn("written once and never edited", second.stderr)
        self.assertEqual(out.read_bytes(), before)

    def test_a_verification_entry_sits_above_the_sign_off(self) -> None:
        text = self.appendices.template("residuals", self.record, self.digest,
                                        "Jesse", MOMENT, "Windows 11")
        entry = self.appendices.entry(
            "release-assets", MOMENT, "release assets",
            [self.lib.Check("download of x", digest_of(b"x"), digest_of(b"y"))])
        mixed = self.appendices.append_entry(text, entry)
        self.assertLess(mixed.index("### verify release-assets — mismatch"),
                        mixed.index(self.appendices.SIGN_OFF_HEADING))
        self.assertTrue(mixed.rstrip().endswith(self.appendices.SIGN_OFF_LINE))
        with self.assertRaises(self.lib.Refusal):
            self.appendices.append_entry("# Appendix: broken\n", entry)

    def test_a_slot_keeps_the_detail_its_job_recorded(self) -> None:
        # The `deny` slot carries the advisory database's revision and the
        # `coverage` slot the counts and the witness; the record has to carry
        # them, not just the verdict.
        supplied = facts(slots={"deny": {"status": "passed", "reason": None,
                                        "detail": {"advisory_database": {
                                            "revision": "d" * 40}}}})
        record = self.lib.build_record(supplied, None, "2026-11-07T00:00:00Z")
        deny = next(slot for slot in record["slots"] if slot["name"] == "deny")
        self.assertEqual(deny["detail"]["advisory_database"]["revision"], "d" * 40)

    def test_a_detail_that_is_not_an_object_is_refused(self) -> None:
        supplied = facts(slots={"deny": {"status": "passed", "reason": None,
                                        "detail": "revision ddd"}})
        with self.assertRaises(Exception) as raised:
            self.lib.build_record(supplied, None, "2026-11-07T00:00:00Z")
        self.assertIn("detail", str(raised.exception))



if __name__ == "__main__":
    unittest.main(verbosity=2)
