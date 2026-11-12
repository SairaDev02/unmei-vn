"""Loading, overlaying and validating the documentation policy every command shares.

Specification: the documentation contract's coverage and disposition rules. The
exported policy (`scripts/docs-policy.json`) ships as it stands, so it names no
entry only this private tree can satisfy. This tree's extra entries — the agent
and research directories, the planning map, the evidence roots and the pinned
baseline — live in a private overlay beside the export tooling, which the
allowlist excludes. The loader merges the overlay the snapshot holds when it
holds one and validates the result exactly as the commands will use it, so the
exported policy alone is also a complete, valid policy.

The report-only keys stay out of the hard-check set, so a missing advisory key
cannot block a commit or a CI gate.
"""

import json

from docs_snapshot import SnapshotError

# The private overlay: it is excluded from the export with the tooling beside it.
PRIVATE_POLICY_PATH = "export/docs-policy.private.json"

# The policy's explicit classification lists. A path may appear in only one of
# them: two claims would let a later rule silently decide the class.
EXPLICIT_CLASSIFICATIONS = (
    "root_guidance", "templates", "exploration", "crate_readmes", "archive_readmes",
    "asset_notes", "pack_content",
)

# Owned by the hard checks; the report's keys stay out so a missing advisory key
# cannot block a commit or a CI gate.
CORE_POLICY_KEYS = (
    "entry_point", "generate_command", "index_name", "fields", "required",
    "summary_max_scalars", "indexed_directories", "active_directories",
    "status_display_directories", "keyword_allowlist", "classifications",
    "section_links", "excluded_landing_links", "link_check",
)


def evidence_root_problem(policy):
    """A policy error in the registered evidence roots, or None when they are usable."""
    roots = policy["classifications"].get("evidence_roots", [])
    seen = set()
    for root in roots:
        if root.startswith("/") or root.endswith("/") or "//" in root:
            return "evidence root %r is not a normalised repository path" % root
        if root in seen:
            return "evidence root %r is registered twice" % root
        seen.add(root)
    return None


def classification_problem(policy):
    """A path the explicit classification lists claim twice, or None when they are disjoint."""
    listed = {}
    rules = policy["classifications"]
    for key in EXPLICIT_CLASSIFICATIONS:
        for path in rules.get(key, []):
            if path in listed and listed[path] != key:
                return "path %r is classified as both %s and %s" % (path, listed[path], key)
            listed[path] = key
    return None


def link_scope_problem(policy):
    """A link-check scope that is not two lists of normalised paths, or None when usable."""
    scope = policy["link_check"]
    if not isinstance(scope, dict):
        return "link_check is not an object holding 'files' and 'directories'"
    for key in ("files", "directories"):
        entries = scope.get(key)
        if not isinstance(entries, list) or \
                not all(isinstance(entry, str) and entry for entry in entries):
            return "link_check.%s is not a list of repository paths" % key
        for entry in entries:
            if entry.startswith("/") or entry.endswith("/") or "//" in entry:
                return "link_check.%s holds %r, which is not a normalised repository path" \
                       % (key, entry)
    return None


def merge_policy_overlay(policy, overlay):
    """`policy` with the private overlay's extra entries added, and nothing removed.

    The invariant: no overlay at all (`None`) returns `policy` unchanged, so the
    exported policy alone is complete and valid. A present overlay is the same
    shape and holds only extra entries, so a list is unioned in order and a map
    is merged at every level. Merging happens before validation, so a policy is
    validated exactly as the commands will use it.
    """
    if overlay is None:
        return policy
    merged = dict(policy)
    for key, extra in overlay.items():
        current = merged.get(key)
        if isinstance(current, list) and isinstance(extra, list):
            merged[key] = current + [item for item in extra if item not in current]
        elif isinstance(current, dict) and isinstance(extra, dict):
            merged[key] = merge_policy_overlay(current, extra)
        else:
            merged[key] = extra
    return merged


def policy_document(snapshot, path):
    """The JSON policy at `path`, or a SnapshotError naming the fault."""
    try:
        return json.loads(snapshot.text(path))
    except ValueError as exc:
        raise SnapshotError("%s is not valid JSON: %s" % (path, exc)) from None


def overlay_document(snapshot):
    """The private overlay `snapshot` holds, or None when it holds none.

    Only a tracked file counts, so an exported tree and a stray local file both
    read as "no overlay": the exported policy is then used on its own.
    """
    if PRIVATE_POLICY_PATH not in snapshot.tracked():
        return None
    return policy_document(snapshot, PRIVATE_POLICY_PATH)


def load_policy(snapshot, policy_path):
    """The policy `snapshot` holds, merged with its overlay, or a SnapshotError."""
    policy = merge_policy_overlay(policy_document(snapshot, policy_path),
                                  overlay_document(snapshot))
    missing = [key for key in CORE_POLICY_KEYS if key not in policy]
    if missing:
        raise SnapshotError(
            "%s is missing %s" % (policy_path, ", ".join(missing)))
    stray = [name for name in policy["active_directories"]
             if name not in policy["indexed_directories"]]
    if stray:
        raise SnapshotError(
            "%s activates %s, which is not an indexed directory"
            % (policy_path, ", ".join(stray)))
    for problem in (evidence_root_problem(policy), classification_problem(policy),
                    link_scope_problem(policy)):
        if problem:
            raise SnapshotError("%s: %s" % (policy_path, problem))
    return policy
