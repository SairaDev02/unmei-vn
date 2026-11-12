#!/usr/bin/env python3
"""Candidate eligibility, the immutable record, byte verification and appendices.

    python3 scripts/candidate_record.py eligibility --sha <40-hex> [--json-out <file>]
    python3 scripts/candidate_record.py record --facts <facts.json> --out <dir>
                                             --message <candidate message>
                                             [--previous <earlier record.json>]
    python3 scripts/candidate_record.py verify <kind> --record <record.json>
                                             --appendix <file> [kind options]
    python3 scripts/candidate_record.py appendix new <stage> --record <record.json>
                                             --out <file> --tester <name>
                                             --environment <text>

Standard library only. The record is written once per run attempt and never
edited, and this script has no way to write `certified`: a rehearsal record can
never be read as candidate evidence.

`eligibility` refuses a SHA that is not 40 lowercase hex characters, a commit that
is missing after a full-history fetch or is not equal to or an ancestor of
`origin/main`, a dispatch that was not on `refs/heads/main`, and a coordinator
revision that is not a SHA. It reads `[workspace.package] version` at the SHA and
keeps the coordinator revision (`GITHUB_SHA`) apart from the candidate.

`record` reads one facts file. The coordinator's record job re-downloads
everything before it writes it, and this script re-checks every fact:

    {
      "candidate": {"sha": "<40 hex>", "version": "0.1.0"},
      "coordinator": {"revision": "<GITHUB_SHA>", "run_id": "12345",
                      "run_attempt": 2},
      "rehearsal": true,
      "environment": {
        "toolchain": {"rust": "1.91.0", "cargo": "1.91.0"},
        "cargo_lock": {"sha256": "<64 hex>", "bytes": 123},
        "targets": ["x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu"],
        "profile": "release",
        "runner_images": {"windows": "windows-2025", "linux": "ubuntu-24.04"}},
      "jobs": [{"name": "packaging-windows", "run_attempt": 2,
                "conclusion": "success", "earlier_failures": ["attempt 1: …"]}],
      "hashes": [{"job": "packaging-linux", "name": "unmei-engine-0.1.0.crate",
                  "kind": "crate", "sha256": "<64 hex>", "bytes": 123}],
      "artifacts": [{"name": "linux-package", "id": "1234",
                     "digest": "sha256:<64 hex>", "bytes": 123}],
      "slots": {"gate": {"status": "passed", "reason": null,
                         "hashes": [{"job": "packaging-linux", "name": "…"}]}},
      "residuals": {"linux-rendered-walk": {"status": "pending", "reason": null}}
    }

`record` also reads the candidate commit's message, which `--message` names as
`git log -1 --format=%B` writes it. When the message is an export's provenance
record, the record copies its four values into a top-level `provenance` object,
because that message is the only statement of which private source the tree came
from. Any other commit leaves it `null` and the record is still written, so a
rehearsal, a merge and a later version all have records.

`hashes[].kind` is `archive`, `executable`, `crate`, `coverage` or `sums`, and a
slot's `hashes` name the staged entries it exercised. A slot the facts file omits
is `not-run`. `crate-parity` is never supplied: it is `passed` only when the
Windows and Linux `.crate` hashes are equal, and the Linux job's files are the
staged payload. When `--previous` names the earlier attempt's record, a re-run
that replaced a hash returns every slot that cited it to `pending`, and a re-run
of a rehearsal attempt stays a rehearsal.

`verify` compares bytes with the staged hashes and appends its entry to the
appendix (above the sign-off line, which stays last):

    local-crate     --crate <path> --name <crate>   a local `.crate`
    index-cksum     --index-file <path> --name <crate>
                                                    the sparse-index `cksum`
    download-crate  --crate <path> --name <crate>   a downloaded `.crate`
    release-assets  --dir <dir> --sums <SHA256SUMS> downloaded release assets

`--dir` and `--sums` may each be repeated. Any mismatch refuses (exit 1) and is
still recorded in the appendix, because a failed verification is evidence.

`appendix new <stage>` writes one stage's appendix for `windows-walk`, `stranger`,
`residuals`, `engine-publication`, `cli-publication` or `release-assets`. Its
fixed header cites the record's SHA-256, the candidate SHA and the hashes the
stage exercises, and it ends with the sign-off line the maintainer writes.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

# Load the sibling modules beside this script, both as
# `python3 scripts/candidate_record.py` and when the test suite loads this file
# as a module from another directory.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from candidate_record_appendix import (STAGES, VERIFY_KINDS, append_entry,
                                       crate_checks, entry, index_cksum_checks,
                                       release_asset_checks, require_header,
                                       template)  # noqa: E402
from candidate_record_lib import (MAIN_REF, RECORD_NAME, RECORD_SUM_NAME,
                                  Refusal, build_record, eligibility_result,
                                  history_refusals, indented, load_record,
                                  record_bytes, record_sum_text,
                                  revision_refusals, sha256_bytes, sha256_file,
                                  shape_refusals, utc_now, version_at,
                                  checks_ok)  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parent.parent


def git(repo, *args):
    return subprocess.run(["git", *args], cwd=str(repo), text=True,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def tail(output, limit=12):
    lines = [line for line in output.strip().splitlines() if line.strip()]
    return "\n".join(lines[-limit:])


def fetch_main(repo):
    result = git(repo, "fetch", "--prune", "--no-tags", "origin",
                 f"+refs/heads/main:refs/remotes/origin/main")
    if result.returncode != 0:
        raise Refusal("git fetch of origin/main failed, so ancestry cannot be proven:\n"
                      + tail(result.stderr or result.stdout))


def history_facts(repo, sha, fetch):
    """Whether the commit is here at all, and whether `origin/main` contains it."""
    if fetch:
        fetch_main(repo)
    else:
        print("note: --no-fetch reads the existing origin/main; the coordinator "
              "fetches it", file=sys.stderr)
    present = git(repo, "rev-parse", "--verify", "--quiet", f"{sha}^{{commit}}")
    if present.returncode != 0:
        return {"commit_exists": False, "is_ancestor_of_main": False}
    relation = git(repo, "merge-base", "--is-ancestor", sha, "origin/main")
    if relation.returncode not in (0, 1):
        raise Refusal("git merge-base --is-ancestor failed against origin/main:\n"
                      + tail(relation.stderr or relation.stdout))
    return {"commit_exists": True, "is_ancestor_of_main": relation.returncode == 0}


def manifest_at(repo, sha):
    result = git(repo, "cat-file", "-p", f"{sha}:Cargo.toml")
    if result.returncode != 0:
        raise Refusal(f"cannot read Cargo.toml at {sha}:\n"
                      + tail(result.stderr or result.stdout))
    return result.stdout


def write_once(path, data, what):
    """A record and an appendix are written once: a later run must not edit either."""
    path = Path(path)
    if path.exists():
        raise Refusal(f"{path} already exists; {what} is written once and never edited")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return path


def require_arg(value, message):
    if value in (None, ""):
        raise Refusal(message)
    return value


def load_facts(path):
    try:
        return json.loads(Path(path).read_text(encoding="utf-8"))
    except OSError as exc:
        raise Refusal(f"cannot read the facts file {path}: {exc}") from exc
    except json.JSONDecodeError as exc:
        raise Refusal(f"the facts file {path} is not JSON: {exc}") from exc


def load_message(path):
    try:
        return Path(path).read_text(encoding="utf-8")
    except OSError as exc:
        raise Refusal(f"cannot read the candidate commit's message {path}: {exc}") from exc


def run_eligibility(args) -> int:
    sha = args.sha
    ref = args.ref or ""
    revision = args.coordinator_revision or ""
    reasons = shape_refusals(sha, ref) + revision_refusals(revision)
    if reasons:
        raise Refusal("this dispatch is not a candidate:\n" + indented(reasons))
    facts = history_facts(Path(args.repo).resolve(), sha, fetch=not args.no_fetch)
    reasons = history_refusals(sha, facts["commit_exists"], facts["is_ancestor_of_main"])
    if reasons:
        raise Refusal("this dispatch is not a candidate:\n" + indented(reasons))

    version = version_at(manifest_at(Path(args.repo).resolve(), sha))
    result = eligibility_result(sha, version, revision)
    print(f"candidate SHA: {sha}")
    print(f"version: {version} (from [workspace.package] at the candidate SHA)")
    print(f"coordinator revision: {revision} (recorded separately, not the candidate)")
    print(f"dispatched on {ref}")
    print("eligible: yes")
    print(json.dumps(result, indent=2))
    if args.json_out:
        out = Path(args.json_out)
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_bytes((json.dumps(result, indent=2) + "\n").encode("utf-8"))
        print(f"wrote {out}")
    return 0


def run_record(args) -> int:
    prior = load_record(args.previous) if args.previous else None
    record = build_record(load_facts(args.facts), prior, args.recorded_at,
                          load_message(args.message))
    data = record_bytes(record)
    digest = sha256_bytes(data)
    out = Path(args.out)
    write_once(out / RECORD_NAME, data, "a candidate record")
    write_once(out / RECORD_SUM_NAME, record_sum_text(digest).encode("utf-8"),
               "a candidate record")

    print(f"{RECORD_NAME}: sha256 {digest}")
    print(f"candidate: {record['candidate']['sha']} ({record['candidate']['version']})")
    provenance = record["provenance"]
    if provenance is None:
        print("provenance: null (the candidate commit is not an export commit)")
    else:
        print(f"provenance: export {provenance['number']}, source "
              f"{provenance['source']}, allowlist {provenance['allowlist_digest']}, "
              f"manifest {provenance['manifest_digest']}")
    print(f"coordinator: revision {record['coordinator']['revision']}, "
          f"run {record['coordinator']['run_id']} attempt "
          f"{record['coordinator']['run_attempt']}")
    print("slots:")
    for slot in record["slots"]:
        print(f"  {slot['name']:<22} {slot['status']:<13} {slot['reason'] or ''}".rstrip())
    print("residuals (never a pass):")
    for residual in record["residuals"]:
        print(f"  {residual['id']:<26} {residual['status']:<12} {residual['title']}")
    print(f"statement: {record['statement']}")
    if record["rehearsal"]:
        print("note: stamped rehearsal; this record is not candidate evidence")
    return 0


def verify_checks(args, record):
    if args.kind in ("local-crate", "download-crate"):
        require_arg(args.crate, f"verify {args.kind} needs --crate <path>")
        require_arg(args.name, f"verify {args.kind} needs --name <crate>")
        return (crate_checks(record, args.name, Path(args.crate)),
                f"{args.kind} of {args.name} at {args.crate}")
    if args.kind == "index-cksum":
        require_arg(args.index_file, "verify index-cksum needs --index-file <path>")
        require_arg(args.name, "verify index-cksum needs --name <crate>")
        return (index_cksum_checks(record, args.name, Path(args.index_file),
                                   record["candidate"]["version"]),
                f"sparse index at {args.index_file}")
    return (release_asset_checks(record, [Path(item) for item in args.dir or []],
                                 [Path(item) for item in args.sums or []]),
            "release assets against " + ", ".join(str(item) for item in args.sums or []))


def run_verify(args) -> int:
    record = load_record(args.record)
    digest = sha256_file(args.record)
    appendix = Path(args.appendix)
    try:
        text = appendix.read_text(encoding="utf-8")
    except OSError as exc:
        raise Refusal(f"cannot read the appendix {appendix}: {exc}") from exc
    require_header(text, digest)
    checks, subject = verify_checks(args, record)
    moment = args.time or utc_now()

    for check in checks:
        mark = "ok" if check.ok else "mismatch"
        print(f"  {mark:<8} {check.label}: expected {check.expected}, found {check.actual}")
    appendix.write_bytes(
        append_entry(text, entry(args.kind, moment, subject, checks)).encode("utf-8"))
    print(f"appended the {args.kind} entry to {appendix}")
    if not checks_ok(checks):
        print(f"verify {args.kind}: mismatch — these bytes are not the ones the "
              "record staged", file=sys.stderr)
        return 1
    print(f"verify {args.kind}: ok")
    return 0


def run_appendix(args) -> int:
    record = load_record(args.record)
    digest = sha256_file(args.record)
    moment = args.time or utc_now()
    text = template(args.stage, record, digest, args.tester, moment, args.environment)
    path = write_once(Path(args.out), text.encode("utf-8"), "an appendix")
    print(f"wrote {path}")
    print(f"stage: {args.stage} ({STAGES[args.stage]})")
    print(f"record SHA-256: {digest}")
    print(f"candidate SHA: {record['candidate']['sha']}")
    if record["rehearsal"]:
        print("note: this record is a rehearsal; its appendix is not certification evidence")
    return 0


def build_parser():
    parser = argparse.ArgumentParser(
        description="Decide candidate eligibility, write the machine record, verify "
                    "bytes and start stage appendices. It never certifies.")
    commands = parser.add_subparsers(dest="command", required=True)

    eligible = commands.add_parser(
        "eligibility", help="refuse a dispatch that cannot produce a candidate")
    eligible.add_argument("--sha", required=True, help="the dispatched candidate SHA")
    eligible.add_argument("--ref", default=os.environ.get("GITHUB_REF", ""),
                          help="the ref the run was dispatched on (default: GITHUB_REF)")
    eligible.add_argument("--coordinator-revision",
                          default=os.environ.get("GITHUB_SHA", ""),
                          help="the coordinator revision (default: GITHUB_SHA)")
    eligible.add_argument("--repo", default=str(REPO_ROOT),
                          help="the checkout to read (default: this repository)")
    eligible.add_argument("--no-fetch", action="store_true",
                          help="skip the origin/main fetch and read the existing one")
    eligible.add_argument("--json-out", help="also write the result as JSON here")

    record = commands.add_parser(
        "record", help="write candidate-record.json and its SHA-256 once")
    record.add_argument("--facts", required=True, help="the facts file this attempt gathered")
    record.add_argument("--message", required=True,
                        help="the candidate commit's message (`git log -1 --format=%%B`)")
    record.add_argument("--out", required=True, help="the directory to write into")
    record.add_argument("--previous", help="the earlier attempt's record, for re-run resets")
    record.add_argument("--recorded-at", help="the recording time (default: now, UTC)")

    verify = commands.add_parser(
        "verify", help="compare bytes with the staged hashes and append the result")
    verify.add_argument("kind", choices=VERIFY_KINDS)
    verify.add_argument("--record", required=True, help="the record holding the staged hashes")
    verify.add_argument("--appendix", required=True, help="the appendix to append to")
    verify.add_argument("--name", help="the crate name (`unmei-engine` or `unmei`)")
    verify.add_argument("--crate", help="the local or downloaded `.crate` to hash")
    verify.add_argument("--index-file", help="the sparse-index document for the crate")
    verify.add_argument("--dir", action="append",
                        help="a directory of downloaded release assets (repeatable)")
    verify.add_argument("--sums", action="append",
                        help="a SHA256SUMS file to check the downloads against (repeatable)")
    verify.add_argument("--time", help="the verification time (default: now, UTC)")

    appendix = commands.add_parser("appendix", help="start one stage's appendix")
    appendix.add_argument("action", choices=("new",))
    appendix.add_argument("stage", choices=tuple(STAGES))
    appendix.add_argument("--record", required=True, help="the record the appendix cites")
    appendix.add_argument("--out", required=True, help="the appendix file to write")
    appendix.add_argument("--tester", required=True, help="who runs this stage")
    appendix.add_argument("--environment", required=True,
                          help="the machine, OS, GPU and driver it runs on")
    appendix.add_argument("--time", help="the start time (default: now, UTC)")
    return parser


def main(argv=None) -> int:
    args = build_parser().parse_args(argv)
    try:
        if args.command == "eligibility":
            return run_eligibility(args)
        if args.command == "record":
            return run_record(args)
        if args.command == "verify":
            return run_verify(args)
        return run_appendix(args)
    except Refusal as refusal:
        print(f"refused: {refusal}", file=sys.stderr)
        return 1
    except OSError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
