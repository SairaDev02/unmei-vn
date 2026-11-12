#!/usr/bin/env python3
"""Verify comparisons and appendix templates for `scripts/candidate_record.py`.

`verify` compares local or registry bytes with the staged hashes in the record
and appends its entry to the current appendix; `appendix new` starts one stage's
appendix from a template whose fixed header cites the record. Both refuse rather
than guess, and neither certifies.
"""

from __future__ import annotations

import json
from pathlib import Path

from candidate_record_lib import (RECORD_NAME, STAGED_JOB, WINDOWS_JOB, Check,
                                  Refusal, checks_ok, parse_sums, sha256_file,
                                  sparse_index_relpath, staged_crate)

VERIFY_KINDS = ("local-crate", "index-cksum", "download-crate", "release-assets")

STAGES = {
    "windows-walk": "Windows walk",
    "stranger": "First-time stranger run",
    "residuals": "Residual acceptance",
    "engine-publication": "Engine publication",
    "cli-publication": "CLI publication",
    "release-assets": "Release assets",
}

SIGN_OFF_HEADING = "## Sign-off"
SIGN_OFF_LINE = "Signed by: ______________________________  Date: ____________________"
VERIFICATION_HEADING = "## Verification"

BODIES = {
    "windows-walk": """
Real-GPU walk on the maintainer's Windows machine, per `docs/release-runbook.md`.
One row per checklist line, with the screenshot's file name beside it.

| Surface | Step | Result | Screenshot |
| --- | --- | --- | --- |
| | | | |
""",
    "stranger": """
The participant gets only the candidate archives and the public Guide, and the
maintainer observes silently. Record the participant's notes and the maintainer's
observations. A blocked required step is not a pass: it gets an issue, a fix and
a new candidate.

| Step | Reached | Notes |
| --- | --- | --- |
| | | |
""",
    "residuals": """
One section per residual in the record, citing its evidence. A residual is never
a pass and stays listed until the maintainer records what settled it.

""",
    "engine-publication": """
Stage the engine from a clean checkout, verify the bytes, publish, then verify the
`cksum` and the download. Record the token's scope and expiry, the times and every
verification's result.

""",
    "cli-publication": """
The CLI stage follows the engine stage and never precedes it. Record the registry
dry-run, the staged-hash verification, the authorisation, the publish and the
post-publish verification.

""",
    "release-assets": """
Record the draft release, the uploaded set, the download verification, the publish
and the download from the public URLs.

""",
}


def exercised_hashes(stage, record):
    """The staged hashes this stage's bytes are checked against."""
    version = record["candidate"]["version"]
    engine = f"unmei-engine-{version}.crate"
    cli = f"unmei-{version}.crate"
    selectors = {
        "windows-walk": lambda entry: entry["job"] == WINDOWS_JOB
        and entry["kind"] in ("archive", "executable"),
        "stranger": lambda entry: entry["kind"] == "archive",
        "residuals": lambda entry: True,
        "engine-publication": lambda entry: entry["job"] == STAGED_JOB
        and entry["kind"] == "crate" and entry["name"] == engine,
        "cli-publication": lambda entry: entry["job"] == STAGED_JOB
        and entry["kind"] == "crate" and entry["name"] == cli,
        "release-assets": lambda entry: entry["kind"] in ("archive", "coverage", "sums"),
    }
    if stage not in selectors:
        raise Refusal(f"unknown appendix stage {stage!r}; the stages are "
                      + ", ".join(STAGES))
    return [entry for entry in record["hashes"] if selectors[stage](entry)]


def header(stage, record, record_digest, tester, moment, environment):
    lines = [f"# Appendix: {STAGES[stage]}", "",
             f"- Record: `{RECORD_NAME}`",
             f"- Record SHA-256: `{record_digest}`",
             f"- Candidate SHA: `{record['candidate']['sha']}`",
             f"- Version: `{record['candidate']['version']}`",
             f"- Rehearsal: `{'true' if record['rehearsal'] else 'false'}`",
             "- Hashes exercised:"]
    hashes = exercised_hashes(stage, record)
    if hashes:
        lines += [f"  - `{entry['name']}` ({entry['job']}, {entry['kind']}): "
                  f"`sha256 {entry['sha256']}`, {entry['bytes']} bytes"
                  for entry in hashes]
    else:
        lines.append("  - none staged in this record")
    lines += [f"- Tester: {tester}", f"- Time (UTC): {moment}",
              f"- Environment: {environment}"]
    return "\n".join(lines)


def template(stage, record, record_digest, tester, moment, environment):
    """One stage's appendix: fixed header, the stage's prompts, then the sign-off line."""
    parts = [header(stage, record, record_digest, tester, moment, environment),
             BODIES[stage].strip("\n"),
             VERIFICATION_HEADING,
             "`candidate_record.py verify` appends its entries above the sign-off.",
             SIGN_OFF_HEADING,
             SIGN_OFF_LINE]
    return "\n\n".join(parts) + "\n"


def require_header(text, record_digest):
    """An appendix belongs to one record: it must cite that record's SHA-256."""
    if not text.startswith("# Appendix: "):
        raise Refusal("this file is not a candidate-record appendix; start one with "
                      "`appendix new <stage>`")
    if f"- Record SHA-256: `{record_digest}`" not in text:
        raise Refusal(f"the appendix does not cite this record's SHA-256 "
                      f"({record_digest}); it belongs to a different record")


def append_entry(text, entry):
    """Insert a verification entry above the sign-off line, which stays last."""
    marker = text.rfind(SIGN_OFF_HEADING)
    if marker < 0:
        raise Refusal("the appendix has no `## Sign-off` section; verification entries "
                      "go above it")
    head = text[:marker].rstrip("\n")
    return f"{head}\n\n{entry.rstrip()}\n\n{text[marker:]}"


def entry(kind, moment, subject, checks):
    status = "ok" if checks_ok(checks) else "mismatch"
    lines = [f"### verify {kind} — {status}", "",
             f"- Time (UTC): {moment}", f"- Subject: {subject}"]
    lines += [f"- {check.label}: expected `{check.expected}`, found `{check.actual}` — "
              + ("ok" if check.ok else "mismatch") for check in checks]
    return "\n".join(lines) + "\n"


def crate_checks(record, crate, path):
    """A `.crate`'s name and bytes against the staged payload.

    The local package and the registry download answer different questions but
    compare the same way, so they share this body and the entry's kind names where
    the bytes came from.
    """
    staged, name = staged_crate(record, crate)
    return [Check(f"{name} file name", name, Path(path).name),
            Check(f"{name} sha256", staged["sha256"], sha256_file(path))]


def index_cksum(text, version):
    """The `cksum` of one version in a sparse-index document (one JSON per line)."""
    found = []
    for number, line in enumerate(text.splitlines(), start=1):
        if not line.strip():
            continue
        try:
            document = json.loads(line)
        except json.JSONDecodeError as exc:
            raise Refusal(f"the sparse-index file is not newline-delimited JSON "
                          f"(line {number}): {exc}") from exc
        if not isinstance(document, dict):
            raise Refusal(f"the sparse-index line {number} is not a JSON object")
        if document.get("vers") == version:
            found.append(document.get("cksum"))
    if not found:
        raise Refusal(f"the sparse index lists no {version}")
    digests = {str(value).lower() for value in found}
    if len(digests) > 1:
        raise Refusal(f"the sparse index lists {version} with more than one cksum")
    return digests.pop()


def index_cksum_checks(record, crate, index_path, version):
    staged, name = staged_crate(record, crate)
    parts = Path(index_path).parts
    actual_path = "/".join(parts[-3:])
    cksum = index_cksum(index_path.read_text(encoding="utf-8"), version)
    return [Check("sparse-index path", sparse_index_relpath(crate), actual_path),
            Check(f"{name} cksum at {version}", staged["sha256"], cksum)]


def listed_digest(digests, expected):
    if expected in digests:
        return expected
    return ", ".join(sorted(digests)) if digests else "not listed in SHA256SUMS"


def release_asset_checks(record, directories, sums_paths):
    """Every sum's downloaded file, and every staged archive the sums must cover.

    Each packaging job uploads its own `SHA256SUMS`, so any number of sums files
    and download directories may be given; a staged archive must be covered by at
    least one of them.
    """
    if not directories:
        raise Refusal("release-assets needs at least one --dir to read downloads from")
    if not sums_paths:
        raise Refusal("release-assets needs at least one --sums file")
    listed: dict[str, set[str]] = {}
    checks = []
    for sums_path in sums_paths:
        for name, digest in parse_sums(sums_path.read_text(encoding="utf-8")):
            listed.setdefault(name, set()).add(digest)
            found = [directory / name for directory in directories
                     if (directory / name).is_file()]
            if not found:
                checks.append(Check(f"download of {name}", digest, "missing from every --dir"))
                continue
            checks += [Check(f"download of {name} in {path.parent.name}",
                             digest, sha256_file(path)) for path in found]
    for staged in record["hashes"]:
        if staged["kind"] not in ("archive", "coverage"):
            continue
        checks.append(Check(f"SHA256SUMS covers {staged['name']} from {staged['job']}",
                            staged["sha256"],
                            listed_digest(listed.get(staged["name"], set()), staged["sha256"])))
    return checks
