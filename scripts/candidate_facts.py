#!/usr/bin/env python3
"""Assemble the candidate record's facts file.

    python3 scripts/candidate_facts.py executables --archive <p> [--archive <p>]
                                                 --out <executables.sha256>
    python3 scripts/candidate_facts.py slot --name <slot> --status <status> \
                                            [--reason <text>] --out <slot.json>
    python3 scripts/candidate_facts.py observations --eligibility <f> \
        --artifacts-api <f> --jobs-api <f> --cargo-lock <f> --out <obs.json>
    python3 scripts/candidate_facts.py previous --staging <dir> --attempt <N>
    python3 scripts/candidate_facts.py release-sums --facts <facts.json> --out <SHA256SUMS>
    python3 scripts/candidate_facts.py build --observations <obs.json> \
        --artifact <job>=<dir> [--artifact <job>=<dir> ...] --staging <dir> \
        --out <facts.json>

`executables` writes the sums file the packaging jobs upload beside their
archives: the executables do not survive as files, so their digests are read
back out of the archives the job just wrote.

`build` turns one coordinator attempt into the facts file `candidate_record.py
record` validates: the record job downloads every artifact of its attempt, and
this hashes each staged file, rechecks every sums file against the bytes beside
it and out of the archives, reads the slot files the other jobs wrote, and
derives the artifacts this attempt uploaded. It refuses rather than records
when a sum does not match, because a mismatched sum is not evidence.

The observations file carries what the running workflow knows: the candidate,
the coordinator, the rehearsal flag, the environment, each job's conclusion, and
the two `gh api` responses (`artifacts_api`, `jobs_api`) this script filters.
"""

from __future__ import annotations

import argparse
import sys
import tarfile
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from candidate_observations import (newest_attempts,  # noqa: E402
                                    observations_command, previous_command)
from package_lib import (Refusal, indented, load_json,  # noqa: E402
                         sha256sums_text, write_json)
from candidate_record_facts import (SLOT_STATUSES, SLOTS,  # noqa: E402
                                    require_facts)
from candidate_record_lib import SUMS_LINE, sha256_bytes, sha256_file  # noqa: E402

ARCHIVE_SUFFIXES = (".zip", ".tar.gz")
EXECUTABLES_SUFFIX = "-executables.sha256"
# The slot each job fills when it wrote no slot file of its own. `crate-parity`
# is derived by the record from the two crate hashes and `human` waits for the
# maintainer, so neither is ever supplied here.
SLOT_JOBS = (
    ("gate", "gate"),
    ("packaging-windows", "packaging-windows"),
    ("packaging-linux", "packaging-linux"),
    ("crate-overlay", "crate-overlay"),
    ("accept-windows", "accept-windows"),
    ("accept-linux", "accept-linux"),
    ("linux-walk", "linux-walk"),
)
# Slots only their own slot file can fill. A green conclusion says the job ran,
# not that the policy, the measurement or the boot passed, so a missing file
# reads `inconclusive` rather than `passed`.
SLOT_FILE_ONLY = (
    ("deny", "deny"),
    ("coverage", "coverage"),
    ("boot", "linux-shipping-boot"),
)
# Which staged files a slot exercised, for the re-run resets. `crate-overlay`,
# both acceptance jobs and the walk all consume the Linux payload because the
# Linux job's files are what publication sends.
SLOT_HASH_JOBS = {
    "packaging-windows": ("packaging-windows",),
    "packaging-linux": ("packaging-linux",),
    "crate-overlay": ("packaging-linux",),
    "accept-windows": ("packaging-windows",),
    "accept-linux": ("packaging-linux",),
    "linux-walk": ("packaging-linux",),
}


def parse_sums(text):
    """`(sha256, name)` for each `sha256sum` line; a stray line is a failure."""
    entries = []
    for line in text.splitlines():
        if not line.strip():
            continue
        match = SUMS_LINE.match(line)
        if match is None:
            raise Refusal(f"not a sha256sum line: {line!r}")
        entries.append((match.group(1), match.group(2)))
    return entries


def sums_text(entries):
    """`sha256sum` text for `(digest, name)` pairs, by the packager's own writer."""
    return sha256sums_text([(name, digest) for digest, name in entries])


def archive_members(path):
    """`(member name, bytes, mode)` for every file in a `.zip` or `.tar.gz`."""
    if path.name.endswith(".zip"):
        with zipfile.ZipFile(path) as archive:
            return [(info.filename, archive.read(info.filename), info.external_attr >> 16)
                    for info in archive.infolist() if not info.is_dir()]
    with tarfile.open(path, "r:gz") as archive:
        return [(member.name, archive.extractfile(member).read(), member.mode)
                for member in archive.getmembers() if member.isfile()]


def archive_executables(path):
    """`(file name, sha256, bytes)` for the executable members of one archive.

    Packaging fixes 0755 for the executables and 0644 for everything else, so an
    executable bit is what marks them and no payload list has to be repeated.
    """
    return [(Path(name).name, sha256_bytes(data), len(data))
            for name, data, mode in archive_members(path) if mode & 0o100]


def archive_paths(directory):
    return [path for path in sorted(Path(directory).iterdir())
            if path.is_file() and path.name.endswith(ARCHIVE_SUFFIXES)]


def hash_entry(job, name, kind, digest, size):
    return {"job": job, "name": name, "kind": kind, "sha256": digest, "bytes": size}


def job_hashes(job, directory):
    """Every staged file of one packaging job, plus the executables inside its archives."""
    entries = []
    for path in sorted(Path(directory).iterdir()):
        if not path.is_file():
            continue
        data = path.read_bytes()
        if path.name.endswith(".crate"):
            entries.append(hash_entry(job, path.name, "crate", sha256_bytes(data), len(data)))
        elif path.name == "SHA256SUMS" or path.name.endswith(EXECUTABLES_SUFFIX):
            entries.append(hash_entry(job, path.name, "sums", sha256_bytes(data), len(data)))
        elif path.name.endswith(ARCHIVE_SUFFIXES):
            entries.append(hash_entry(job, path.name, "archive", sha256_bytes(data), len(data)))
            for name, digest, size in archive_executables(path):
                entries.append(hash_entry(job, name, "executable", digest, size))
    return sorted(entries, key=lambda entry: (entry["kind"], entry["name"]))


def available_digests(directory):
    """What the artifact holds, by file name: its files, and the executables in its archives."""
    digests = {path.name: sha256_file(path) for path in Path(directory).iterdir()
               if path.is_file()}
    for archive in archive_paths(directory):
        for name, digest, _size in archive_executables(archive):
            digests[name] = digest
    return digests


def sums_problems(job, directory):
    """Each sum line that does not describe the bytes this artifact carried."""
    digests = available_digests(directory)
    problems = []
    for path in sorted(Path(directory).iterdir()):
        if not path.is_file():
            continue
        if path.name != "SHA256SUMS" and not path.name.endswith(EXECUTABLES_SUFFIX):
            continue
        for digest, name in parse_sums(path.read_text(encoding="utf-8")):
            if name not in digests:
                problems.append(f"{job}: {name} is summed in {path.name} but absent")
            elif digests[name] != digest:
                problems.append(f"{job}: {name} does not match {path.name}")
    return problems


def slot_file_entries(path):
    data = load_json(path, "slot file")
    items = data if isinstance(data, list) else [data]
    entries = {}
    for item in items:
        if not isinstance(item, dict):
            raise Refusal(f"{path} holds a slot entry that is not an object")
        name = item.get("slot")
        status = item.get("status")
        if name not in SLOTS:
            raise Refusal(f"{path} names slot {name!r}, which is not one of "
                          + ", ".join(SLOTS))
        if status not in SLOT_STATUSES:
            raise Refusal(f"{path} gives slot {name} status {status!r}, which is not one of "
                          + ", ".join(SLOT_STATUSES))
        reason = item.get("reason")
        if reason is not None and not str(reason).strip():
            raise Refusal(f"{path} gives slot {name} an empty reason")
        entry = {"status": status, "reason": reason}
        # Everything the job added beyond the verdict is the slot's detail: the
        # advisory database's revision for `deny`, the counts and witness for
        # `coverage`. The record keeps it, because the deny and coverage slots
        # own that detail and a verdict alone would lose it.
        detail = {key: value for key, value in item.items()
                  if key not in ("slot", "status", "reason")}
        if detail:
            entry["detail"] = detail
        entries[name] = entry
    return entries


def slot_files(directory, attempt):
    """Every slot the newest attempt of each job wrote, from its `*slot*.json` files.

    A replaced job's older slot file is skipped; a reused job's still counts.
    """
    paths = sorted(Path(directory).rglob("*slot*.json"))
    keep = newest_attempts({path.relative_to(directory).parts[0] for path in paths}, attempt)
    slots = {}
    for path in paths:
        if path.relative_to(directory).parts[0] not in keep:
            continue
        slots.update(slot_file_entries(path))
    return slots


def conclusion_slot(conclusion):
    """A job that wrote no slot file still has one: its own conclusion."""
    if conclusion == "success":
        return {"status": "passed", "reason": None}
    if conclusion in ("skipped", "cancelled"):
        return {"status": "not-run", "reason": f"the job did not run ({conclusion})"}
    return {"status": "failed", "reason": f"the job finished {conclusion}"}


def slots_from(conclusions, written):
    slots = {}
    for job, slot in SLOT_JOBS:
        if slot not in written and job in conclusions:
            slots[slot] = conclusion_slot(conclusions[job])
    for job, slot in SLOT_FILE_ONLY:
        if slot in written:
            continue
        conclusion = conclusions.get(job)
        if conclusion == "success":
            slots[slot] = {"status": "inconclusive",
                           "reason": f"the {job} job finished without writing its "
                                     f"{slot} slot"}
        elif conclusion is not None:
            slots[slot] = conclusion_slot(conclusion)
    for name, entry in written.items():
        slots[name] = entry
    return slots


def with_hash_refs(slot, name, hashes):
    jobs = SLOT_HASH_JOBS.get(name, ())
    refs = [{"job": entry["job"], "name": entry["name"]}
            for entry in hashes if entry["job"] in jobs]
    return {**slot, "hashes": refs} if refs else slot


def select_artifacts(artifacts_api, attempt):
    """Each artifact's newest attempt up to this one; the newest upload id wins."""
    items = artifacts_api.get("artifacts", [])
    keep = newest_attempts({item.get("name") or "" for item in items}, attempt)
    chosen = {}
    for item in items:
        name = item.get("name") or ""
        if name not in keep:
            continue
        entry = {"name": name, "id": str(item.get("id")),
                 "digest": item.get("digest"), "bytes": item.get("size_in_bytes")}
        if name in chosen and int(chosen[name]["id"]) >= int(entry["id"]):
            continue
        chosen[name] = entry
    return [chosen[name] for name in sorted(chosen)]


def earlier_failures(jobs_api, attempt):
    """`attempt N: conclusion` for each earlier attempt of each job, by job name."""
    found = {}
    for job in jobs_api.get("jobs", []):
        job_attempt = job.get("run_attempt") or attempt
        conclusion = job.get("conclusion")
        if job_attempt >= attempt or conclusion in (None, "success"):
            continue
        found.setdefault(job.get("name"), []).append(f"attempt {job_attempt}: {conclusion}")
    return found


def job_entries(observations):
    attempt = observations["coordinator"]["run_attempt"]
    earlier = earlier_failures(observations.get("jobs_api") or {}, attempt)
    return [{"name": name, "run_attempt": attempt, "conclusion": conclusion,
             "earlier_failures": earlier.get(name, [])}
            for name, conclusion in observations["conclusions"].items()]


def build_facts(observations, artifact_dirs, staging):
    """The facts file, refused when a sum or a slot contradicts the artifacts."""
    hashes = []
    problems = []
    for job, directory in artifact_dirs.items():
        hashes.extend(job_hashes(job, directory))
        problems.extend(sums_problems(job, directory))
    if problems:
        raise Refusal("a staged sum does not describe this attempt's bytes:\n"
                      + indented(problems))
    slots = slots_from(observations["conclusions"],
                       slot_files(staging, observations["coordinator"]["run_attempt"]))
    attempt = observations["coordinator"]["run_attempt"]
    facts = {
        "candidate": observations["candidate"],
        "coordinator": observations["coordinator"],
        "rehearsal": observations["rehearsal"],
        "environment": observations["environment"],
        "jobs": job_entries(observations),
        "hashes": hashes,
        "artifacts": select_artifacts(observations.get("artifacts_api") or {}, attempt),
        "slots": {name: with_hash_refs(entry, name, hashes)
                  for name, entry in slots.items()},
        "residuals": observations.get("residuals") or {},
    }
    return require_facts(facts)


def slot_command(args):
    """Write one slot for a job whose steps decide it, so the record reads a status."""
    if args.name not in SLOTS:
        raise Refusal(f"{args.name!r} is not one of the twelve slots")
    if args.status not in SLOT_STATUSES:
        raise Refusal(f"{args.status!r} is not one of " + ", ".join(SLOT_STATUSES))
    write_json(args.out, {"slot": args.name, "status": args.status,
                          "reason": args.reason or None})
    print(f"slot {args.name}: {args.status}" + (f" ({args.reason})" if args.reason else ""))
    return 0


RELEASE_SUMS_KINDS = ("archive", "crate")


def release_sums(facts_data):
    """The release set's sums: each staged archive once, and each crate once.

    One file, because a release carries one `SHA256SUMS`. Both packaging jobs
    stage both crates, so the same crate name arrives from each of them: equal
    digests collapse to one line, and two different digests are refused rather
    than silently picked between — that is a parity failure the record reports.
    """
    digests = {}
    for entry in facts_data["hashes"]:
        if entry["kind"] not in RELEASE_SUMS_KINDS:
            continue
        seen = digests.setdefault(entry["name"], entry["sha256"])
        if seen != entry["sha256"]:
            raise Refusal(f"{entry['name']} has two digests, {seen} and {entry['sha256']}; "
                          "the two packaging jobs did not stage the same bytes")
    return sums_text([(digest, name) for name, digest in sorted(digests.items())])


def release_sums_command(args):
    text = release_sums(load_json(args.facts, "facts file"))
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(text.encode("utf-8"))
    print(f"{out}: {len(text.splitlines())} sums")
    return 0


def executables(args):
    entries = []
    for archive in args.archive:
        path = Path(archive)
        if not path.is_file():
            raise Refusal(f"no archive at {path}")
        found = archive_executables(path)
        if not found:
            raise Refusal(f"{path} carries no executable")
        entries.extend((digest, name) for name, digest, _size in found)
    text = sums_text(entries)
    if args.out:
        Path(args.out).write_bytes(text.encode("utf-8"))
    print(text, end="")
    return 0


def build(args):
    pairs = {}
    for item in args.artifact:
        job, _, directory = item.partition("=")
        if not job or not directory:
            raise Refusal(f"--artifact needs <job>=<dir>, not {item!r}")
        if not Path(directory).is_dir():
            raise Refusal(f"{job}: no artifact directory at {directory}")
        pairs[job] = Path(directory)
    facts = build_facts(load_json(args.observations, "observations file"), pairs,
                        Path(args.staging))
    write_json(args.out, facts)
    print(f"{args.out}: {len(facts['hashes'])} hashes, {len(facts['artifacts'])} artifacts")
    for slot in facts["slots"]:
        print(f"  slot {slot}")
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(
        description="Assemble the candidate record's facts file.")
    commands = parser.add_subparsers(dest="command", required=True)

    exe = commands.add_parser("executables", help="sums for the executables inside archives")
    exe.add_argument("--archive", action="append", required=True)
    exe.add_argument("--out")
    exe.set_defaults(func=executables)

    writer = commands.add_parser("slot", help="write one slot file for a job's steps")
    writer.add_argument("--name", required=True)
    writer.add_argument("--status", required=True)
    writer.add_argument("--reason", default="")
    writer.add_argument("--out", required=True)
    writer.set_defaults(func=slot_command)

    maker = commands.add_parser("observations", help="write the observations file")
    maker.add_argument("--eligibility", required=True)
    maker.add_argument("--artifacts-api", required=True)
    maker.add_argument("--jobs-api", required=True)
    maker.add_argument("--cargo-lock", required=True)
    maker.add_argument("--out", required=True)
    maker.set_defaults(func=observations_command)

    finder = commands.add_parser("previous", help="print an earlier attempt's record path")
    finder.add_argument("--staging", required=True)
    finder.add_argument("--attempt", required=True, type=int)
    finder.set_defaults(func=previous_command)

    releaser = commands.add_parser("release-sums", help="merge the release set's sums")
    releaser.add_argument("--facts", required=True)
    releaser.add_argument("--out", required=True)
    releaser.set_defaults(func=release_sums_command)

    builder = commands.add_parser("build", help="write the facts file for one attempt")
    builder.add_argument("--observations", required=True)
    # Optional: a failed packaging job stages nothing, and the record must still
    # be written so its packaging slot can say `failed`.
    builder.add_argument("--artifact", action="append", default=[],
                         metavar="JOB=DIR")
    builder.add_argument("--staging", required=True,
                         help="every artifact of this attempt, for the slot files")
    builder.add_argument("--out", required=True)
    builder.set_defaults(func=build)

    args = parser.parse_args(argv)
    try:
        return args.func(args)
    except Refusal as refusal:
        print(f"refused: {refusal}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
