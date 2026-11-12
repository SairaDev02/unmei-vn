#!/usr/bin/env python3
"""The candidate record model for `scripts/candidate_record.py`.

Everything here is pure: eligibility, the record, the crate-parity rule, the
re-run resets and the immutability invariants.
`candidate_record_facts.py` owns the facts-file shape, the entry point holds the
command line and the Git and file access, and `candidate_record_appendix.py`
holds the verify and appendix side, so `scripts/test-candidate-record.py` can
drive all of it without a run.
"""

from __future__ import annotations

import hashlib
import json
import re
from datetime import datetime, timezone
from pathlib import Path
from typing import NamedTuple

from package_lib import (Refusal, indented, sha256_bytes, sha256_file,
                         workspace_version)
from candidate_record_facts import (RESIDUALS, SLOTS, hash_key, hash_ref,
                                    is_sha, require_digest, require_entries,
                                    require_facts, require_hashes,
                                    require_mapping, require_sha,
                                    require_uncertified)
import export_provenance

STATEMENT = "machine evidence only, not certified"
SCHEMA = 1
MAIN_REF = "refs/heads/main"
RECORD_NAME = "candidate-record.json"
RECORD_SUM_NAME = "candidate-record.json.sha256"

PARITY_SLOT = "crate-parity"
# The Linux job's files are the staged payload: they are what publication sends.
STAGED_JOB = "packaging-linux"
WINDOWS_JOB = "packaging-windows"
SUMS_LINE = re.compile(r"^([0-9a-f]{64}) [ *](.+)$")


class Check(NamedTuple):
    """One hash comparison: `ok` when the bytes are the ones the record staged."""

    label: str
    expected: str
    actual: str

    @property
    def ok(self) -> bool:
        return self.expected == self.actual


def checks_ok(checks) -> bool:
    return all(check.ok for check in checks)


def utc_now() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def version_at(manifest_text):
    """The `[workspace.package] version` of a root `Cargo.toml`, by packaging's rule."""
    return workspace_version(manifest_text)


# --- eligibility --------------------------------------------------------


def shape_refusals(sha, ref):
    """Why this dispatch cannot name a candidate, before any history is read."""
    reasons = []
    if not is_sha(sha):
        reasons.append(f"candidate SHA {sha!r} is not 40 lowercase hex characters")
    if ref != MAIN_REF:
        reasons.append(f"the run was dispatched on {ref!r}; only {MAIN_REF} produces a candidate")
    return reasons


def revision_refusals(coordinator_revision):
    """The coordinator revision is recorded, never mistaken for the candidate."""
    if not is_sha(coordinator_revision):
        return [f"coordinator revision {coordinator_revision!r} is not 40 lowercase hex characters"]
    return []


def history_refusals(sha, commit_exists, is_ancestor_of_main):
    """The commit must be in this clone and reachable from `origin/main`."""
    if not commit_exists:
        return [f"commit {sha} is missing after a full-history fetch"]
    if not is_ancestor_of_main:
        return [f"commit {sha} is not equal to or an ancestor of origin/main"]
    return []


def eligibility_refusals(sha, ref, commit_exists=None, is_ancestor_of_main=None):
    """Every reason this dispatch is not a candidate; empty means eligible."""
    reasons = shape_refusals(sha, ref)
    if reasons:
        return reasons
    return history_refusals(sha, commit_exists, is_ancestor_of_main)


def require_eligible(sha, ref, commit_exists=None, is_ancestor_of_main=None):
    reasons = eligibility_refusals(sha, ref, commit_exists, is_ancestor_of_main)
    if reasons:
        raise Refusal("this dispatch is not a candidate:\n" + indented(reasons))


def eligibility_result(sha, version, coordinator_revision):
    return {
        "eligible": True,
        "candidate_sha": sha,
        "version": version,
        "coordinator_revision": coordinator_revision,
    }


# --- the record ---------------------------------------------------------


def normalized_hashes(hashes):
    return [{"job": entry["job"], "name": entry["name"], "kind": entry["kind"],
             "sha256": entry["sha256"], "bytes": entry["bytes"]}
            for entry in sorted(hashes, key=hash_key)]


def normalized_environment(environment):
    return {
        "toolchain": dict(sorted(environment["toolchain"].items())),
        "cargo_lock": {"sha256": environment["cargo_lock"]["sha256"],
                       "bytes": environment["cargo_lock"]["bytes"]},
        "targets": list(environment["targets"]),
        "profile": environment["profile"],
        "runner_images": dict(sorted(environment["runner_images"].items())),
    }


def parity_slot(index, version):
    """`crate-parity` is derived, never taken from the facts: equal hashes or failed."""
    name = f"unmei-engine-{version}.crate"
    windows = index.get((WINDOWS_JOB, name))
    linux = index.get((STAGED_JOB, name))
    refs = [{"job": job, "name": name} for job, entry in
            ((WINDOWS_JOB, windows), (STAGED_JOB, linux)) if entry]
    if windows is None or linux is None:
        return {"name": PARITY_SLOT, "status": "not-run", "hashes": refs,
                "reason": f"a packaging job reported no {name} to compare"}
    if windows["sha256"] != linux["sha256"]:
        return {"name": PARITY_SLOT, "status": "failed", "hashes": refs,
                "reason": f"the Windows and Linux {name} hashes differ: "
                          f"{windows['sha256']} and {linux['sha256']}"}
    return {"name": PARITY_SLOT, "status": "passed", "hashes": refs, "reason": None}


def build_slots(slot_facts, index, version):
    slots = []
    for name in SLOTS:
        if name == PARITY_SLOT:
            slots.append(parity_slot(index, version))
            continue
        supplied = slot_facts.get(name)
        if supplied is None:
            slots.append({"name": name, "status": "not-run", "hashes": [],
                          "reason": "this run attempt reported no result for this slot"})
            continue
        refs = [{"job": job, "name": artifact}
                for job, artifact in (hash_ref(ref, f"slot {name}")
                                      for ref in supplied.get("hashes", []))]
        normal = {"name": name, "status": supplied["status"],
                  "reason": supplied.get("reason"), "hashes": refs}
        if supplied.get("detail") is not None:
            # What the job added beyond the verdict: the advisory database's
            # revision for `deny`, the counts and witness for `coverage`.
            normal["detail"] = supplied["detail"]
        slots.append(normal)
    return slots


def replaced_hashes(prior, index):
    """Hash keys a re-run replaced or dropped between the earlier attempt and this one."""
    if not prior:
        return {}
    replaced = {}
    for entry in prior.get("hashes", []):
        key = hash_key(entry)
        current = index.get(key)
        if current is None or current["sha256"] != entry["sha256"]:
            replaced[key] = current["sha256"] if current else None
    return replaced


def apply_resets(slots, replaced):
    """Every slot that cited a replaced hash returns to pending; parity stays derived."""
    if not replaced:
        return slots
    for slot in slots:
        if slot["name"] == PARITY_SLOT:
            continue
        hit = [(ref["job"], ref["name"]) for ref in slot["hashes"]
               if (ref["job"], ref["name"]) in replaced]
        if not hit:
            continue
        slot["status"] = "pending"
        slot["reason"] = ("a re-run replaced "
                          + ", ".join(f"{job}:{name}" for job, name in hit)
                          + "; this slot returns to pending until it is re-run "
                            "against the new bytes")
    return slots


def inherited_rehearsal(prior, run_id):
    """A re-run of a rehearsal attempt stays a rehearsal: the flag is never cleared."""
    if not prior:
        return False
    return bool(prior.get("rehearsal")) and \
        (prior.get("coordinator") or {}).get("run_id") == run_id


def build_residuals(residual_facts):
    residual_facts = residual_facts or {}
    return [{"id": key, "title": title,
             "status": (residual_facts.get(key) or {}).get("status", "pending"),
             "reason": (residual_facts.get(key) or {}).get("reason")}
            for key, title in RESIDUALS]


def provenance_field(message):
    """The record's `provenance`: the export message's four values, or None.

    The values are the candidate commit's own statement of where the tree came
    from, copied and checked, never inferred from the tree. A message that is
    not an export's leaves it None, so a rehearsal, a merge and a later version
    still produce a record; a message that is an export's but names an unusable
    SHA or digest is refused, because that record would be wrong about the
    export rather than silent about it.
    """
    record = export_provenance.parse(message) if message is not None else None
    if record is None:
        return None
    return {
        "number": record.number,
        "source": require_sha(record.source, "the export message's source commit"),
        "allowlist_digest": require_digest(record.allowlist_digest,
                                           "the export message's allowlist digest"),
        "manifest_digest": require_digest(record.manifest_digest,
                                          "the export message's manifest digest"),
    }


def build_record(facts, prior=None, recorded_at=None, message=None):
    """The immutable record for one run attempt: validated facts plus what the script enforces.

    `message` is the candidate commit's message, which supplies `provenance`.
    """
    require_uncertified(facts, "facts")
    require_facts(facts)
    candidate = facts["candidate"]
    coordinator = facts["coordinator"]
    hashes = normalized_hashes(facts["hashes"])
    index = {hash_key(entry): entry for entry in hashes}
    slots = build_slots(facts["slots"], index, candidate["version"])
    slots = apply_resets(slots, replaced_hashes(prior, index))
    record = {
        "schema": SCHEMA,
        "statement": STATEMENT,
        "recorded_at": recorded_at or utc_now(),
        "rehearsal": facts["rehearsal"] or inherited_rehearsal(prior, str(coordinator["run_id"])),
        "candidate": {"sha": candidate["sha"], "version": candidate["version"]},
        "provenance": provenance_field(message),
        "coordinator": {"revision": coordinator["revision"],
                        "run_id": str(coordinator["run_id"]),
                        "run_attempt": coordinator["run_attempt"]},
        "environment": normalized_environment(facts["environment"]),
        "jobs": [{"name": job["name"], "run_attempt": job["run_attempt"],
                  "conclusion": job["conclusion"],
                  "earlier_failures": list(job.get("earlier_failures", []))}
                 for job in facts["jobs"]],
        "hashes": hashes,
        "artifacts": [{"name": artifact["name"], "id": str(artifact["id"]),
                       "digest": artifact["digest"], "bytes": artifact["bytes"]}
                      for artifact in facts["artifacts"]],
        "slots": slots,
        "residuals": build_residuals(facts.get("residuals")),
    }
    require_uncertified(record, "record")
    return record


def is_candidate_evidence(record):
    """A rehearsal record is never candidate evidence, whatever its slots say."""
    return record.get("statement") == STATEMENT and not record.get("rehearsal")


def record_bytes(record):
    return (json.dumps(record, indent=2, ensure_ascii=False) + "\n").encode("utf-8")


def record_sum_text(digest):
    return f"{digest}  {RECORD_NAME}\n"


def load_record(path):
    path = Path(path)
    try:
        record = json.loads(path.read_text(encoding="utf-8"))
    except OSError as exc:
        raise Refusal(f"cannot read the record {path}: {exc}") from exc
    except json.JSONDecodeError as exc:
        raise Refusal(f"{path} is not JSON: {exc}") from exc
    require_record(record, path)
    return record


def require_record(record, path="the record"):
    record = require_mapping(record, str(path))
    if record.get("statement") != STATEMENT:
        raise Refusal(f"{path} is not a candidate record written by this script; "
                      f"its statement is not {STATEMENT!r}")
    require_sha((record.get("candidate") or {}).get("sha"), f"{path} candidate SHA")
    names = [slot.get("name") for slot in require_entries(record.get("slots"), f"{path} slots")]
    if names != list(SLOTS):
        raise Refusal(f"{path} does not carry the twelve slots in order")
    require_hashes(record.get("hashes"))
    require_entries(record.get("residuals"), f"{path} residuals")
    return record


def slot_of(record, name):
    return next(slot for slot in record["slots"] if slot["name"] == name)


# --- staged payload and SHA256SUMS -------------------------------------


def staged_crate(record, crate):
    """The Linux job's `.crate` entry for `crate`, which is the staged payload."""
    version = record["candidate"]["version"]
    name = f"{crate}-{version}.crate"
    entries = [entry for entry in record["hashes"] if entry["name"] == name]
    staged = [entry for entry in entries if entry["job"] == STAGED_JOB]
    if not staged:
        raise Refusal(f"the record stages no {STAGED_JOB} {name}; there is nothing to "
                      "verify")
    digests = {entry["sha256"] for entry in staged}
    if len(digests) > 1:
        raise Refusal(f"the record stages {name} with more than one hash")
    return staged[0], name


def sparse_index_relpath(crate):
    """crates.io's sparse-index layout for a crate name."""
    if len(crate) == 1:
        return f"1/{crate}"
    if len(crate) == 2:
        return f"2/{crate}"
    if len(crate) == 3:
        return f"3/{crate[0]}/{crate}"
    return f"{crate[:2]}/{crate[2:4]}/{crate}"


def parse_sums(text):
    """`SHA256SUMS` as its `(name, sha256)` pairs, in file order."""
    entries = []
    seen = set()
    for number, line in enumerate(text.splitlines(), start=1):
        if not line.strip():
            continue
        match = SUMS_LINE.match(line)
        if not match:
            raise Refusal(f"SHA256SUMS line {number} is not `<sha256>  <name>`: {line!r}")
        name = match.group(2).strip()
        if name in seen:
            raise Refusal(f"SHA256SUMS lists {name} twice")
        seen.add(name)
        entries.append((name, match.group(1)))
    if not entries:
        raise Refusal("SHA256SUMS is empty")
    return entries
