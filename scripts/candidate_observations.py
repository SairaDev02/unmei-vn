#!/usr/bin/env python3
"""The candidate observations file for `scripts/candidate_facts.py build`.

The coordinator's record job runs this to turn one attempt's environment, its
eligibility artifact and two `gh api` responses into the observations `build`
reads, so the workflow holds no logic of its own.

    python3 scripts/candidate_facts.py observations --eligibility <f>         --artifacts-api <f> --jobs-api <f> --cargo-lock <f> --out <obs.json>
    python3 scripts/candidate_facts.py previous --staging <dir> --attempt <N>

`previous` prints the newest record an earlier attempt of the same run left, so a
re-run hands `candidate_record.py record --previous` the record it replaces.
"""

from __future__ import annotations

import hashlib
import os
import subprocess
from pathlib import Path

from package_lib import ATTEMPT_MARKER, Refusal, load_json, write_json

# Every job the record states, in the order the coordinator runs them.
CONCLUSION_JOBS = ("eligibility", "gate", "packaging-windows", "packaging-linux",
                   "crate-overlay", "deny", "coverage", "boot",
                   "accept-windows", "accept-linux")
TARGETS = ("x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu")


def conclusion_variable(job):
    return "CONCLUSION_" + job.upper().replace("-", "_")


def observations_from(env, eligibility, artifacts_api, jobs_api, toolchain, lock):
    """The observations file from what the running workflow already knows.

    A conclusion the caller did not pass is refused rather than dropped: a slot
    must never turn `not-run` because an environment variable went missing.
    """
    conclusions = {job: env.get(conclusion_variable(job)) or "" for job in CONCLUSION_JOBS}
    missing = [job for job, value in conclusions.items() if not value]
    if missing:
        raise Refusal("no job conclusion for " + ", ".join(missing))
    return {
        "candidate": {"sha": eligibility["candidate_sha"],
                      "version": eligibility["version"]},
        "coordinator": {"revision": env["GITHUB_SHA"],
                        "run_id": env["GITHUB_RUN_ID"],
                        "run_attempt": int(env["GITHUB_RUN_ATTEMPT"])},
        "rehearsal": env.get("REHEARSAL") == "true",
        "environment": {
            "toolchain": toolchain,
            "cargo_lock": {"sha256": hashlib.sha256(lock).hexdigest(), "bytes": len(lock)},
            "targets": list(TARGETS),
            "profile": "release",
            "runner_images": {"windows": env["RUNNER_WINDOWS"],
                              "linux": env["RUNNER_LINUX"]},
        },
        "conclusions": conclusions,
        "artifacts_api": artifacts_api,
        "jobs_api": jobs_api,
    }


def attempt_of(name):
    """`(base, N)` for a `<base>-attempt<N>` artifact name; `(name, None)` if untagged."""
    base, marker, suffix = name.rpartition(ATTEMPT_MARKER)
    return (base, int(suffix)) if marker and suffix.isdigit() else (name, None)


def newest_attempts(names, attempt):
    """Each artifact's newest upload at or before `attempt`, by name.

    Re-running failed jobs reruns them and their dependents only; the jobs it
    reuses keep the earlier attempt's artifacts, and that evidence still stands.
    """
    chosen = {}
    for name in names:
        base, number = attempt_of(name)
        if number is not None and number > attempt:
            continue
        if base not in chosen or (number or 0) > (attempt_of(chosen[base])[1] or 0):
            chosen[base] = name
    return set(chosen.values())


def earlier_record(staging, attempt):
    """The newest record an earlier attempt of this run left, or None."""
    earlier = []
    for path in sorted(Path(staging).glob("candidate-record-*-attempt*/candidate-record.json")):
        artifact = path.relative_to(staging).parts[0]
        suffix = artifact.rsplit(ATTEMPT_MARKER, 1)[1]
        if suffix.isdigit() and int(suffix) < attempt:
            earlier.append((int(suffix), path))
    return max(earlier)[1] if earlier else None


def toolchain():
    def version(argv):
        return subprocess.run(argv, check=True, text=True,
                              stdout=subprocess.PIPE).stdout.strip()
    return {"rust": version(["rustc", "--version"]),
            "cargo": version(["cargo", "--version"])}


def observations_command(args):
    observations = observations_from(
        os.environ,
        load_json(args.eligibility, "eligibility result"),
        load_json(args.artifacts_api, "artifacts response"),
        load_json(args.jobs_api, "jobs response"),
        toolchain(),
        Path(args.cargo_lock).read_bytes())
    write_json(args.out, observations)
    print(f"{args.out}: candidate {observations['candidate']['sha']} "
          f"({observations['candidate']['version']}), "
          f"attempt {observations['coordinator']['run_attempt']}")
    return 0


def previous_command(args):
    record = earlier_record(Path(args.staging), args.attempt)
    if record is not None:
        print(record)
    return 0
