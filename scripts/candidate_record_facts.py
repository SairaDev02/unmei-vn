#!/usr/bin/env python3
"""What the candidate record's facts file must contain (`scripts/candidate_record.py`).

One validated shape for the facts file the coordinator's record job writes, so a
typo cannot silently drop a slot, a hash or a residual from the immutable
record.
"""

from __future__ import annotations

import re

from package_lib import Refusal

HASH_KINDS = ("archive", "executable", "crate", "coverage", "sums")
# A machine cannot settle these, so a residual is listed beside the slots and is
# never a pass: the human record in an appendix carries what settled them.
RESIDUALS = (
    ("linux-rendered-walk", "the Linux full-rendered walk"),
    ("smart-app-control", "the Smart App Control sentence"),
    ("publish-environment-oidc", "the `publish.yml` environment and OIDC exchange"),
    ("coverage-app-exclusion", "coverage's app exclusion"),
)
RESIDUAL_STATUSES = ("pending", "inconclusive", "not-run")
SLOT_STATUSES = ("passed", "failed", "pending", "not-run", "inconclusive")
SLOTS = (
    "gate",
    "packaging-windows",
    "packaging-linux",
    "crate-parity",
    "crate-overlay",
    "deny",
    "coverage",
    "linux-shipping-boot",
    "accept-windows",
    "accept-linux",
    "linux-walk",
    "human",
)
FACTS_KEYS = ("candidate", "coordinator", "rehearsal", "environment", "jobs",
              "hashes", "artifacts", "slots")

SHA_RE = re.compile(r"^[0-9a-f]{40}$")
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
ARTIFACT_DIGEST_RE = re.compile(r"^(?:sha256:)?[0-9a-f]{64}$")
CERTIFICATION_KEY = re.compile(r"certif", re.IGNORECASE)


def is_sha(value) -> bool:
    return isinstance(value, str) and bool(SHA_RE.match(value))


def require_sha(value, label):
    if not is_sha(value):
        raise Refusal(f"{label} {value!r} is not 40 lowercase hex characters")
    return value


def require_digest(value, label, pattern=SHA256_RE):
    if not isinstance(value, str) or not pattern.match(value):
        raise Refusal(f"{label} {value!r} is not a SHA-256")
    return value

# --- facts validation ---------------------------------------------------


def require_mapping(value, label):
    if not isinstance(value, dict):
        raise Refusal(f"{label} must be a JSON object")
    return value


def require_text(value, label):
    if not isinstance(value, str) or not value.strip():
        raise Refusal(f"{label} must be non-empty text")
    return value


def require_count(value, label, minimum=0):
    if not isinstance(value, int) or isinstance(value, bool) or value < minimum:
        raise Refusal(f"{label} must be a whole number of at least {minimum}")
    return value


def require_bool(value, label):
    if not isinstance(value, bool):
        raise Refusal(f"{label} must be true or false, not {value!r}")
    return value


def require_entries(value, label):
    if not isinstance(value, list):
        raise Refusal(f"{label} must be a list")
    return value


def hash_key(entry):
    return (entry["job"], entry["name"])


def hash_ref(value, label):
    value = require_mapping(value, f"{label} hash reference")
    return (require_text(value.get("job"), f"{label} hash job"),
            require_text(value.get("name"), f"{label} hash name"))


def require_environment(environment):
    environment = require_mapping(environment, "environment")
    toolchain = require_mapping(environment.get("toolchain"), "environment.toolchain")
    if not toolchain:
        raise Refusal("environment.toolchain records no tool version")
    for key, value in toolchain.items():
        require_text(value, f"environment.toolchain.{key}")
    lock = require_mapping(environment.get("cargo_lock"), "environment.cargo_lock")
    require_digest(lock.get("sha256"), "environment.cargo_lock.sha256")
    require_count(lock.get("bytes"), "environment.cargo_lock.bytes", minimum=1)
    targets = require_entries(environment.get("targets"), "environment.targets")
    if not targets:
        raise Refusal("environment.targets records no target triple")
    for target in targets:
        require_text(target, "environment.targets entry")
    require_text(environment.get("profile"), "environment.profile")
    images = require_mapping(environment.get("runner_images"), "environment.runner_images")
    if not images:
        raise Refusal("environment.runner_images records no runner image")
    for key, value in images.items():
        require_text(value, f"environment.runner_images.{key}")


def require_jobs(jobs):
    jobs = require_entries(jobs, "jobs")
    if not jobs:
        raise Refusal("jobs lists no job for this run attempt")
    seen = set()
    for job in jobs:
        job = require_mapping(job, "jobs entry")
        name = require_text(job.get("name"), "job name")
        if name in seen:
            raise Refusal(f"job {name!r} is listed twice")
        seen.add(name)
        require_count(job.get("run_attempt"), f"job {name} run_attempt", minimum=1)
        require_text(job.get("conclusion"), f"job {name} conclusion")
        for failure in require_entries(job.get("earlier_failures", []),
                                       f"job {name} earlier_failures"):
            require_text(failure, f"job {name} earlier failure")


def require_hashes(hashes):
    hashes = require_entries(hashes, "hashes")
    seen = set()
    for entry in hashes:
        entry = require_mapping(entry, "hashes entry")
        job = require_text(entry.get("job"), "hash job")
        name = require_text(entry.get("name"), "hash name")
        kind = entry.get("kind")
        if kind not in HASH_KINDS:
            raise Refusal(f"{job}:{name} kind {kind!r} is not one of {', '.join(HASH_KINDS)}")
        require_digest(entry.get("sha256"), f"{job}:{name} sha256")
        require_count(entry.get("bytes"), f"{job}:{name} bytes")
        if (job, name) in seen:
            raise Refusal(f"{job} lists {name} twice; each artifact has one hash")
        seen.add((job, name))


def require_artifacts(artifacts):
    seen = set()
    for artifact in require_entries(artifacts, "artifacts"):
        artifact = require_mapping(artifact, "artifacts entry")
        name = require_text(artifact.get("name"), "artifact name")
        if name in seen:
            raise Refusal(f"artifact {name!r} is listed twice")
        seen.add(name)
        require_text(str(artifact.get("id")), f"artifact {name} id")
        require_digest(artifact.get("digest"), f"artifact {name} digest",
                       pattern=ARTIFACT_DIGEST_RE)
        require_count(artifact.get("bytes"), f"artifact {name} bytes")


def require_slot_facts(slots):
    slots = require_mapping(slots, "slots")
    unknown = [name for name in slots if name not in SLOTS]
    if unknown:
        raise Refusal(f"unknown slot(s) {', '.join(unknown)}; the twelve slots are "
                      + ", ".join(SLOTS))
    for name, slot in slots.items():
        slot = require_mapping(slot, f"slot {name}")
        status = slot.get("status")
        if status not in SLOT_STATUSES:
            raise Refusal(f"slot {name} status {status!r} is not one of "
                          + ", ".join(SLOT_STATUSES))
        if slot.get("reason") is not None:
            require_text(slot["reason"], f"slot {name} reason")
        if slot.get("detail") is not None:
            require_mapping(slot["detail"], f"slot {name} detail")
        for ref in require_entries(slot.get("hashes", []), f"slot {name} hashes"):
            hash_ref(ref, f"slot {name}")


def require_residual_facts(residuals):
    if residuals is None:
        return {}
    residuals = require_mapping(residuals, "residuals")
    known = dict(RESIDUALS)
    unknown = [key for key in residuals if key not in known]
    if unknown:
        raise Refusal(f"unknown residual(s) {', '.join(unknown)}; the residuals are "
                      + ", ".join(known))
    for key, entry in residuals.items():
        entry = require_mapping(entry, f"residual {key}")
        status = entry.get("status", "pending")
        if status not in RESIDUAL_STATUSES:
            raise Refusal(f"residual {key} cannot be {status!r}; a residual is never a "
                          f"pass and stays {', '.join(RESIDUAL_STATUSES)}")
        if entry.get("reason") is not None:
            require_text(entry["reason"], f"residual {key} reason")
    return residuals


def certification_paths(value, prefix="record"):
    """Every key that would claim a certification, wherever it hides."""
    found = []
    if isinstance(value, dict):
        for key, item in value.items():
            path = f"{prefix}.{key}"
            if isinstance(key, str) and CERTIFICATION_KEY.search(key):
                found.append(path)
            found.extend(certification_paths(item, path))
    elif isinstance(value, list):
        for position, item in enumerate(value):
            found.extend(certification_paths(item, f"{prefix}[{position}]"))
    return found


def require_uncertified(value, prefix):
    found = certification_paths(value, prefix)
    if found:
        raise Refusal("the record never carries a certification field; remove "
                      + ", ".join(found))


def require_facts(facts):
    facts = require_mapping(facts, "the facts file")
    missing = [key for key in FACTS_KEYS if key not in facts]
    if missing:
        raise Refusal("the facts file is missing " + ", ".join(missing))
    candidate = require_mapping(facts["candidate"], "candidate")
    require_sha(candidate.get("sha"), "candidate SHA")
    require_text(candidate.get("version"), "candidate version")
    coordinator = require_mapping(facts["coordinator"], "coordinator")
    require_sha(coordinator.get("revision"), "coordinator revision")
    require_text(str(coordinator.get("run_id")), "run id")
    require_count(coordinator.get("run_attempt"), "run attempt", minimum=1)
    require_bool(facts["rehearsal"], "rehearsal")
    require_environment(facts["environment"])
    require_jobs(facts["jobs"])
    require_hashes(facts["hashes"])
    require_artifacts(facts["artifacts"])
    require_slot_facts(facts["slots"])
    require_residual_facts(facts.get("residuals"))
    return facts
