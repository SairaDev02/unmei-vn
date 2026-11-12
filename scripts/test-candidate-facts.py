#!/usr/bin/env python3
"""Logic tests for `scripts/candidate_facts.py`.

These tests never run cargo, `gh` or the network. They build synthetic
artifacts, slot files and API responses, so every refusal is exercised without a
coordinator run.
"""

from __future__ import annotations

import contextlib
import hashlib
import json
import os
import io
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import candidate_facts as facts  # noqa: E402
import candidate_observations as observations_module  # noqa: E402
import package_lib as packaging  # noqa: E402

COMMIT_EPOCH = 1_793_886_831
SHA = "a" * 40


def archive_entries(executables, extra=b"read me"):
    return [
        packaging.ArchiveEntry("stem", 0o755, True, b""),
        packaging.ArchiveEntry("stem/README", 0o644, False, extra),
    ] + [packaging.ArchiveEntry(f"stem/{name}", 0o755, False, data)
         for name, data in executables.items()]


def observations(conclusions=None, attempt=1, artifacts=None, jobs=None):
    return {
        "candidate": {"sha": SHA, "version": "0.1.0"},
        "coordinator": {"revision": "b" * 40, "run_id": "77", "run_attempt": attempt},
        "rehearsal": False,
        "environment": {
            "toolchain": {"rust": "1.98.1", "cargo": "1.98.1"},
            "cargo_lock": {"sha256": "c" * 64, "bytes": 4096},
            "targets": ["x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu"],
            "profile": "release",
            "runner_images": {"windows": "windows-2025", "linux": "ubuntu-24.04"},
        },
        "conclusions": conclusions if conclusions is not None else {},
        "artifacts_api": artifacts if artifacts is not None else {"artifacts": []},
        "jobs_api": jobs if jobs is not None else {"jobs": []},
    }


class FactsTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.addCleanup(self.tempdir.cleanup)
        self.root = Path(self.tempdir.name)

    def packaging_dir(self, job, executable, archive_suffix):
        """One packaging job's artifact: crate, archive, SHA256SUMS, executables sums."""
        directory = self.root / job
        directory.mkdir()
        crate = directory / "unmei-engine-0.1.0.crate"
        crate.write_bytes(b"engine crate")
        (directory / "unmei-0.1.0.crate").write_bytes(b"cli crate")
        archive = directory / f"{job}-0.1.0-x86_64{archive_suffix}"
        entries = archive_entries({executable: executable.encode("utf-8") + b"-bytes"})
        if archive_suffix == ".zip":
            packaging.write_zip(archive, entries, COMMIT_EPOCH)
        else:
            packaging.write_tar_gz(archive, entries, COMMIT_EPOCH)
        sums = [(facts.sha256_file(crate), crate.name),
                (facts.sha256_file(directory / "unmei-0.1.0.crate"), "unmei-0.1.0.crate"),
                (facts.sha256_file(archive), archive.name),
                (facts.archive_executables(archive)[0][1], executable)]
        (directory / "SHA256SUMS").write_bytes(facts.sums_text(sums).encode("utf-8"))
        (directory / f"{job}{facts.EXECUTABLES_SUFFIX}").write_bytes(
            facts.sums_text([(sums[-1][0], sums[-1][1])]).encode("utf-8"))
        return directory

    # --- sums files -------------------------------------------------------

    def test_sums_round_trip_sorts_by_name_and_uses_two_spaces(self) -> None:
        text = facts.sums_text([("b" * 64, "unmei-play.exe"), ("a" * 64, "unmei.crate")])
        # By name, as `sha256sums_text` in package_lib sorts it: '-' precedes '.'.
        self.assertEqual(text, "b" * 64 + "  unmei-play.exe\n" + "a" * 64 + "  unmei.crate\n")
        self.assertEqual(facts.parse_sums(text), [("b" * 64, "unmei-play.exe"),
                                                  ("a" * 64, "unmei.crate")])

    def test_a_line_that_is_not_a_sum_is_refused(self) -> None:
        with self.assertRaises(facts.Refusal) as raised:
            facts.parse_sums("deadbeef  unmei.crate\n")
        self.assertIn("not a sha256sum line", str(raised.exception))

    # --- archives ---------------------------------------------------------

    def test_only_executable_members_are_taken_from_either_archive_format(self) -> None:
        for suffix in (".zip", ".tar.gz"):
            archive = self.root / f"payload{suffix}"
            entries = archive_entries({"unmei-play" + (".exe" if suffix == ".zip" else ""):
                                       b"binary"})
            if suffix == ".zip":
                packaging.write_zip(archive, entries, COMMIT_EPOCH)
            else:
                packaging.write_tar_gz(archive, entries, COMMIT_EPOCH)
            found = facts.archive_executables(archive)
            expected = "unmei-play.exe" if suffix == ".zip" else "unmei-play"
            self.assertEqual([name for name, _digest, _size in found], [expected])

    # --- hashes and rechecks ---------------------------------------------

    def test_every_staged_file_and_archived_executable_is_hashed(self) -> None:
        directory = self.packaging_dir("packaging-windows", "unmei-play.exe", ".zip")
        entries = facts.job_hashes("packaging-windows", directory)
        self.assertEqual(
            {(entry["name"], entry["kind"]) for entry in entries},
            {("unmei-engine-0.1.0.crate", "crate"), ("unmei-0.1.0.crate", "crate"),
             ("packaging-windows-0.1.0-x86_64.zip", "archive"),
             ("SHA256SUMS", "sums"),
             ("packaging-windows-executables.sha256", "sums"),
             ("unmei-play.exe", "executable")})
        exe = next(entry for entry in entries if entry["kind"] == "executable")
        self.assertEqual(exe["bytes"], len(b"unmei-play.exe-bytes"))

    def test_a_consistent_artifact_rechecks_clean(self) -> None:
        directory = self.packaging_dir("packaging-linux", "unmei-play", ".tar.gz")
        self.assertEqual(facts.sums_problems("packaging-linux", directory), [])

    def test_a_changed_byte_is_named_by_the_recheck(self) -> None:
        directory = self.packaging_dir("packaging-linux", "unmei-play", ".tar.gz")
        (directory / "unmei-0.1.0.crate").write_bytes(b"tampered")
        problems = facts.sums_problems("packaging-linux", directory)
        self.assertEqual(len(problems), 1)
        self.assertIn("unmei-0.1.0.crate does not match SHA256SUMS", problems[0])

    def test_a_summed_file_that_is_absent_is_named(self) -> None:
        directory = self.packaging_dir("packaging-linux", "unmei-play", ".tar.gz")
        (directory / "unmei-0.1.0.crate").unlink()
        problems = facts.sums_problems("packaging-linux", directory)
        self.assertTrue(any("unmei-0.1.0.crate is summed in SHA256SUMS but absent" in item
                            for item in problems))

    # --- slot files -------------------------------------------------------

    def test_a_slot_file_may_hold_one_slot_or_several(self) -> None:
        single = self.root / "deny-slot.json"
        single.write_text(json.dumps({"slot": "deny", "status": "passed", "reason": None}))
        self.assertEqual(facts.slot_file_entries(single),
                         {"deny": {"status": "passed", "reason": None}})

        several = self.root / "accept-linux-slots.json"
        several.write_text(json.dumps([
            {"slot": "accept-linux", "status": "passed"},
            {"slot": "linux-walk", "status": "failed", "reason": "step `new-game` timed out"},
        ]))
        self.assertEqual(facts.slot_file_entries(several)["linux-walk"]["status"], "failed")

    def test_a_slot_file_keeps_what_the_job_added_as_detail(self) -> None:
        path = self.root / "deny-slot.json"
        path.write_text(json.dumps({
            "slot": "deny", "status": "passed", "reason": "cargo-deny exited 0",
            "tool": {"name": "cargo-deny", "version": "0.20.2"},
            "advisory_database": {"revision": "c" * 40, "committed": "2026-11-06T00:00:00Z"},
        }))
        entry = facts.slot_file_entries(path)["deny"]
        self.assertEqual(entry["status"], "passed")
        self.assertEqual(entry["detail"]["advisory_database"]["revision"], "c" * 40)

    def test_a_green_job_that_wrote_no_slot_file_is_inconclusive(self) -> None:
        # A pass is a verdict, not a conclusion: only the slot file can say it.
        slots = facts.slots_from({"deny": "success", "coverage": "success",
                                  "boot": "success", "gate": "success", "packaging-linux": "success"},
                                 {})
        self.assertEqual(slots["deny"]["status"], "inconclusive")
        self.assertIn("without writing its deny slot", slots["deny"]["reason"])
        self.assertEqual(slots["coverage"]["status"], "inconclusive")
        self.assertEqual(slots["linux-shipping-boot"]["status"], "inconclusive")
        self.assertEqual(slots["gate"]["status"], "passed")
        self.assertEqual(slots["packaging-linux"]["status"], "passed")

    def test_an_unknown_slot_or_status_is_refused(self) -> None:
        path = self.root / "slot.json"
        path.write_text(json.dumps({"slot": "certified", "status": "passed"}))
        with self.assertRaises(facts.Refusal) as raised:
            facts.slot_file_entries(path)
        self.assertIn("which is not one of", str(raised.exception))
        path.write_text(json.dumps({"slot": "gate", "status": "green"}))
        with self.assertRaises(facts.Refusal):
            facts.slot_file_entries(path)

    def test_an_earlier_attempts_slot_files_are_ignored(self) -> None:
        for attempt, status in ((1, "failed"), (2, "passed")):
            artifact = self.root / f"crate-overlay-slot-{SHA}-attempt{attempt}"
            artifact.mkdir()
            (artifact / "crate-overlay-slot.json").write_text(json.dumps(
                {"slot": "crate-overlay", "status": status}))
        self.assertEqual(facts.slot_files(self.root, 2)["crate-overlay"]["status"], "passed")
        self.assertEqual(facts.slot_files(self.root, 1)["crate-overlay"]["status"], "failed")

    def test_a_reused_jobs_earlier_slot_file_still_counts(self) -> None:
        # Re-running failed jobs reuses the jobs that passed, and their
        # artifacts keep the earlier attempt's name (rehearsal run 42465771281).
        for name, slot, status in (
                (f"deny-slot-{SHA}-attempt1", "deny", "passed"),
                (f"accept-linux-slots-{SHA}-attempt2", "accept-linux", "passed"),
                (f"boot-slot-{SHA}-attempt3", "linux-shipping-boot", "failed")):
            (self.root / name).mkdir()
            (self.root / name / "slot.json").write_text(json.dumps(
                {"slot": slot, "status": status}))
        written = facts.slot_files(self.root, 2)
        self.assertEqual(written["deny"]["status"], "passed")
        self.assertEqual(written["accept-linux"]["status"], "passed")
        self.assertNotIn("linux-shipping-boot", written)

    def test_a_written_slot_file_wins_over_the_job_conclusion(self) -> None:
        slots = facts.slots_from({"deny": "success", "gate": "failure"},
                                 {"deny": {"status": "inconclusive", "reason": "no database"}})
        self.assertEqual(slots["deny"]["status"], "inconclusive")
        self.assertEqual(slots["gate"]["status"], "failed")
        self.assertNotIn("crate-parity", slots)
        self.assertNotIn("human", slots)

    def test_job_conclusions_map_to_slot_statuses(self) -> None:
        self.assertEqual(facts.conclusion_slot("success")["status"], "passed")
        self.assertEqual(facts.conclusion_slot("skipped")["status"], "not-run")
        self.assertEqual(facts.conclusion_slot("cancelled")["status"], "not-run")
        self.assertEqual(facts.conclusion_slot("timed_out")["status"], "failed")

    # --- API filters ------------------------------------------------------

    def test_artifacts_are_limited_to_this_attempt_and_its_newest_upload(self) -> None:
        api = {"artifacts": [
            {"id": 1, "name": "packaging-linux-" + SHA + "-attempt1",
             "digest": "sha256:" + "d" * 64, "size_in_bytes": 10},
            {"id": 2, "name": "packaging-linux-" + SHA + "-attempt2",
             "digest": "sha256:" + "e" * 64, "size_in_bytes": 11},
            {"id": 3, "name": "deny-slot-" + SHA + "-attempt1",
             "digest": "sha256:" + "f" * 64, "size_in_bytes": 12},
            {"id": 4, "name": "deny-slot-" + SHA + "-attempt2",
             "digest": "sha256:" + "0" * 64, "size_in_bytes": 13},
            {"id": 5, "name": "boot-slot-" + SHA + "-attempt2",
             "digest": "sha256:" + "1" * 64, "size_in_bytes": 14},
        ]}
        chosen = facts.select_artifacts(api, 2)
        self.assertEqual(sorted(entry["name"] for entry in chosen),
                         ["boot-slot-" + SHA + "-attempt2",
                          "deny-slot-" + SHA + "-attempt2",
                          "packaging-linux-" + SHA + "-attempt2"])
        self.assertEqual(
            next(entry for entry in chosen
                 if entry["name"] == "deny-slot-" + SHA + "-attempt2")["id"], "4")

    def test_an_artifact_only_an_earlier_attempt_uploaded_is_kept(self) -> None:
        api = {"artifacts": [
            {"id": 1, "name": "eligibility-" + SHA + "-attempt1",
             "digest": "sha256:" + "d" * 64, "size_in_bytes": 10},
            {"id": 2, "name": "record-" + SHA + "-attempt3",
             "digest": "sha256:" + "e" * 64, "size_in_bytes": 11},
        ]}
        self.assertEqual([entry["name"] for entry in facts.select_artifacts(api, 2)],
                         ["eligibility-" + SHA + "-attempt1"])

    def test_earlier_failures_are_recorded_per_job(self) -> None:
        api = {"jobs": [
            {"name": "gate", "run_attempt": 1, "conclusion": "failure"},
            {"name": "gate", "run_attempt": 2, "conclusion": "success"},
            {"name": "packaging-linux", "run_attempt": 1, "conclusion": "success"},
        ]}
        self.assertEqual(facts.earlier_failures(api, 2), {"gate": ["attempt 1: failure"]})

    # --- the facts file ---------------------------------------------------

    def test_the_facts_file_carries_every_staged_hash_and_cites_it_from_its_slot(self) -> None:
        windows = self.packaging_dir("packaging-windows", "unmei-play.exe", ".zip")
        linux = self.packaging_dir("packaging-linux", "unmei-play", ".tar.gz")
        obs = observations(conclusions={"packaging-windows": "success",
                                        "packaging-linux": "success",
                                        "accept-linux": "success"},
                           attempt=1,
                           artifacts={"artifacts": [
                               {"id": 5, "name": "packaging-linux-" + SHA + "-attempt1",
                                "digest": "sha256:" + "d" * 64, "size_in_bytes": 10}]})
        built = facts.build_facts(obs, {"packaging-windows": windows,
                                        "packaging-linux": linux}, self.root)
        names = {(entry["job"], entry["name"]) for entry in built["hashes"]}
        self.assertIn(("packaging-windows", "unmei-play.exe"), names)
        self.assertIn(("packaging-linux", "unmei-play"), names)
        self.assertIn(("packaging-linux", "unmei-engine-0.1.0.crate"), names)
        self.assertEqual(built["slots"]["accept-linux"]["status"], "passed")
        self.assertEqual(
            {ref["job"] for ref in built["slots"]["accept-linux"]["hashes"]},
            {"packaging-linux"})
        self.assertEqual(
            {ref["job"] for ref in built["slots"]["packaging-windows"]["hashes"]},
            {"packaging-windows"})
        self.assertEqual(built["artifacts"][0]["name"], "packaging-linux-" + SHA + "-attempt1")

    def test_a_tampered_artifact_refuses_the_whole_facts_file(self) -> None:
        linux = self.packaging_dir("packaging-linux", "unmei-play", ".tar.gz")
        (linux / "SHA256SUMS").write_bytes(
            facts.sums_text([("0" * 64, "unmei-0.1.0.crate")]).encode("utf-8"))
        with self.assertRaises(facts.Refusal) as raised:
            facts.build_facts(observations(), {"packaging-linux": linux}, self.root)
        self.assertIn("does not describe this attempt's bytes", str(raised.exception))

    def test_a_facts_file_is_written_when_no_packaging_job_staged_anything(self) -> None:
        # Both packaging jobs failed, so the record job has no packaging
        # artifact to pass; the facts file must still exist so the record can
        # say why (the first item-eleven rehearsal crashed here instead).
        obs_path = self.root / "observations.json"
        obs_path.write_text(json.dumps(observations(conclusions={
            "packaging-windows": "failure", "packaging-linux": "failure",
            "gate": "success"})), encoding="utf-8")
        out = self.root / "facts.json"
        with contextlib.redirect_stdout(io.StringIO()):
            status = facts.main(["build", "--observations", str(obs_path),
                                 "--staging", str(self.root), "--out", str(out)])
        self.assertEqual(status, 0)
        built = json.loads(out.read_text(encoding="utf-8"))
        self.assertEqual(built["hashes"], [])
        self.assertEqual(built["slots"]["packaging-windows"]["status"], "failed")
        self.assertEqual(built["slots"]["packaging-linux"]["status"], "failed")
        self.assertEqual(built["slots"]["gate"]["status"], "passed")

    # --- observations and the earlier record -------------------------------

    def observation_env(self, **overrides):
        env = {"GITHUB_SHA": "b" * 40, "GITHUB_RUN_ID": "77", "GITHUB_RUN_ATTEMPT": "2",
               "REHEARSAL": "false", "RUNNER_WINDOWS": "windows-2025",
               "RUNNER_LINUX": "ubuntu-24.04"}
        for job in observations_module.CONCLUSION_JOBS:
            env[observations_module.conclusion_variable(job)] = "success"
        env.update(overrides)
        return env

    def test_observations_carry_every_conclusion_and_the_lock_digest(self) -> None:
        lock = b'[package]\nname = "x"\n'
        obs = observations_module.observations_from(
            self.observation_env(REHEARSAL="true"),
            {"candidate_sha": SHA, "version": "0.1.0"}, {"artifacts": []},
            {"jobs": []}, {"rust": "rustc 1.98.1", "cargo": "cargo 1.98.1"}, lock)
        self.assertEqual(obs["candidate"], {"sha": SHA, "version": "0.1.0"})
        self.assertEqual(obs["coordinator"]["run_attempt"], 2)
        self.assertTrue(obs["rehearsal"])
        self.assertEqual(obs["environment"]["cargo_lock"],
                         {"sha256": hashlib.sha256(lock).hexdigest(), "bytes": len(lock)})
        self.assertEqual(len(obs["conclusions"]), len(observations_module.CONCLUSION_JOBS))
        self.assertEqual(obs["environment"]["runner_images"]["linux"], "ubuntu-24.04")

    def test_a_missing_job_conclusion_is_refused(self) -> None:
        env = self.observation_env()
        del env["CONCLUSION_ACCEPT_LINUX"]
        with self.assertRaises(facts.Refusal) as raised:
            observations_module.observations_from(env, {"candidate_sha": SHA, "version": "0.1.0"},
                                    {"artifacts": []}, {"jobs": []},
                                    {"rust": "r", "cargo": "c"}, b"lock")
        self.assertIn("accept-linux", str(raised.exception))

    def test_the_observations_command_runs_through_main(self) -> None:
        # `main` is what the workflow runs, so the subcommand has to be reachable
        # from it and its helpers imported: a missing import is a NameError there.
        eligibility = self.root / "eligibility.json"
        eligibility.write_text(json.dumps({"candidate_sha": SHA, "version": "0.1.0"}))
        for name in ("artifacts.json", "jobs.json"):
            (self.root / name).write_text(json.dumps({}))
        lock = self.root / "Cargo.lock"
        lock.write_bytes(b"lock")
        out = self.root / "observations.json"
        env = self.observation_env()
        saved = {key: os.environ.get(key) for key in env}
        os.environ.update(env)
        self.addCleanup(lambda: [os.environ.pop(key, None) if saved[key] is None
                                 else os.environ.__setitem__(key, saved[key]) for key in env])
        real_toolchain = observations_module.toolchain
        observations_module.toolchain = lambda: {"rust": "rustc 1.98.1", "cargo": "cargo 1.98.1"}
        self.addCleanup(setattr, observations_module, "toolchain", real_toolchain)
        with contextlib.redirect_stdout(io.StringIO()):
            status = facts.main(["observations", "--eligibility", str(eligibility),
                                 "--artifacts-api", str(self.root / "artifacts.json"),
                                 "--jobs-api", str(self.root / "jobs.json"),
                                 "--cargo-lock", str(lock), "--out", str(out)])
        self.assertEqual(status, 0)
        written = json.loads(out.read_text())
        self.assertEqual(written["candidate"]["sha"], SHA)
        self.assertEqual(written["environment"]["cargo_lock"]["bytes"], 4)

    def test_the_earlier_record_is_the_newest_attempt_before_this_one(self) -> None:
        for attempt in (1, 2, 3):
            directory = self.root / f"candidate-record-{SHA}-attempt{attempt}"
            directory.mkdir()
            (directory / "candidate-record.json").write_text("{}")
        self.assertEqual(observations_module.earlier_record(self.root, 3).parent.name,
                         f"candidate-record-{SHA}-attempt2")
        self.assertIsNone(observations_module.earlier_record(self.root, 1))

    def test_the_release_sums_cover_both_jobs_and_skip_the_executables(self) -> None:
        facts_data = {"hashes": [
            {"job": "packaging-windows", "name": "unmei-play-0.1.0-x86_64-windows.zip",
             "kind": "archive", "sha256": "a" * 64, "bytes": 10},
            {"job": "packaging-windows", "name": "unmei-play.exe", "kind": "executable",
             "sha256": "b" * 64, "bytes": 10},
            {"job": "packaging-linux", "name": "unmei-0.1.0-x86_64-linux.tar.gz",
             "kind": "archive", "sha256": "c" * 64, "bytes": 10},
            {"job": "packaging-linux", "name": "unmei-engine-0.1.0.crate", "kind": "crate",
             "sha256": "d" * 64, "bytes": 10},
            {"job": "packaging-linux", "name": "SHA256SUMS", "kind": "sums",
             "sha256": "e" * 64, "bytes": 10},
        ]}
        parsed = facts.parse_sums(facts.release_sums(facts_data))
        self.assertEqual([name for _digest, name in parsed],
                         ["unmei-0.1.0-x86_64-linux.tar.gz", "unmei-engine-0.1.0.crate",
                          "unmei-play-0.1.0-x86_64-windows.zip"])

    def test_a_crate_both_jobs_staged_appears_once_and_a_parity_break_is_refused(self) -> None:
        staged = lambda digest: {"hashes": [
            {"job": "packaging-windows", "name": "unmei-0.1.0.crate", "kind": "crate",
             "sha256": digest, "bytes": 10},
            {"job": "packaging-linux", "name": "unmei-0.1.0.crate", "kind": "crate",
             "sha256": digest, "bytes": 10},
        ]}
        parsed = facts.parse_sums(facts.release_sums(staged("a" * 64)))
        self.assertEqual(parsed, [("a" * 64, "unmei-0.1.0.crate")])
        broken = {"hashes": staged("a" * 64)["hashes"]
                  + [{"job": "packaging-linux", "name": "unmei-0.1.0.crate",
                      "kind": "crate", "sha256": "b" * 64, "bytes": 10}]}
        with self.assertRaises(facts.Refusal) as raised:
            facts.release_sums(broken)
        self.assertIn("two digests", str(raised.exception))

    def test_the_executables_command_writes_the_sums_file(self) -> None:
        directory = self.packaging_dir("packaging-linux", "unmei-play", ".tar.gz")
        out = self.root / "executables.sha256"
        archive = next(directory.glob("*.tar.gz"))
        with contextlib.redirect_stdout(io.StringIO()):
            status = facts.main(["executables", "--archive", str(archive),
                                 "--out", str(out)])
        self.assertEqual(status, 0)
        parsed = facts.parse_sums(out.read_text(encoding="utf-8"))
        self.assertEqual([name for _digest, name in parsed], ["unmei-play"])

    def test_the_slot_writer_refuses_unknown_names_and_statuses(self) -> None:
        out = self.root / "slot.json"
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(facts.main(["slot", "--name", "crate-overlay",
                                         "--status", "passed", "--out", str(out)]), 0)
        self.assertEqual(json.loads(out.read_text()),
                         {"slot": "crate-overlay", "status": "passed", "reason": None})
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(facts.main(["slot", "--name", "certified", "--status",
                                         "passed", "--out", str(out)]), 1)
            self.assertEqual(facts.main(["slot", "--name", "gate", "--status", "green",
                                         "--out", str(out)]), 1)


if __name__ == "__main__":
    unittest.main()
