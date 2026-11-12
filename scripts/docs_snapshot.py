"""Coverage classification and candidate-snapshot readers.

Every tracked Markdown path in the candidate snapshot gets exactly one coverage
class; rules are narrow and listed in the policy, and a path no rule matches is
reported as unclassified rather than skipped.

A snapshot is one candidate tree seen one way: the worktree, the Git index, or
a committed revision. Document text is normalised to LF before parsing, because
a Windows checkout with `core.autocrlf` rewrites line endings that the committed
blob and the generator hold as LF. Generated index bytes are never normalised:
they are pinned to LF by `.gitattributes` and compared exactly.

Symbolic links are part of the tree: the index and a committed tree record one
as mode `120000` whose blob is the link target, and the worktree holds a real
link. A `related` entry is followed through those links, so a link cannot carry
an otherwise lexical path above the repository root.

The policy every command shares is loaded, overlaid and validated in
`docs_policy`; this module classifies a path against whatever policy it is given.
"""

import os
import posixpath
import re
import subprocess
from dataclasses import dataclass
from pathlib import Path

# A skill contract is `SKILL.md` exactly one component below the skills root;
# neighbouring Markdown in the same skill directory is not special-format.
SKILL_RE = re.compile(r"^\.agents/skills/[^/]+/SKILL\.md$")

# A rejected-enhancement record is one `.md` file directly in the rejected-
# enhancements root, one file per rejected concept, written by triage when it
# closes a request.
OUT_OF_SCOPE_RE = re.compile(r"^\.out-of-scope/[^/]+\.md$")

# Coverage inspects every Markdown-shaped file, not only `.md`. A file with one
# of the other suffixes matches no indexed rule and is therefore reported as
# unclassified: a silent skip would hide a document from the policy entirely.
MARKDOWN_SUFFIXES = (".md", ".mdx", ".markdown")

# Git records a symbolic link as this mode; its blob holds the link target.
LINK_MODE = "120000"

# A `related` entry may cross symbolic links, but not many at once.
MAX_LINK_DEPTH = 8

class LinkError(Exception):
    """A `related` entry cannot be followed through the tree's symbolic links."""


class SnapshotError(Exception):
    """The candidate tree cannot be read as specified."""


@dataclass(frozen=True)
class Classification:
    kind: str
    owner: str | None = None


UNCLASSIFIED = Classification("unclassified")


def in_markdown_scope(path):
    """Whether coverage must place `path` in exactly one class."""
    return path.lower().endswith(MARKDOWN_SUFFIXES)


def is_markdown(path):
    """Whether `path` may carry ordinary metadata or be a generated index."""
    return path.lower().endswith(".md")


def coverage_classes(path, policy):
    """Every coverage class `path` matches, in the disposition table's order."""
    index_name = policy["index_name"]
    indexed = policy["indexed_directories"]
    rules = policy["classifications"]
    parent, _, name = path.rpartition("/")
    found = []

    for directory in indexed:
        if path == directory + "/" + index_name:
            found.append(Classification("generated", directory))
    if parent in indexed and is_markdown(name) and name != index_name:
        found.append(Classification("ordinary", parent))
    if path in rules.get("root_guidance", []):
        found.append(Classification("root-guidance"))
    if SKILL_RE.match(path):
        found.append(Classification("skill-format"))
    if path in rules.get("templates", []):
        found.append(Classification("template"))
    if path in rules.get("exploration", []):
        found.append(Classification("exploration"))
    if path in rules.get("crate_readmes", []):
        found.append(Classification("crate-readme"))
    if path in rules.get("archive_readmes", []):
        found.append(Classification("archive-readme"))
    if path in rules.get("asset_notes", []):
        found.append(Classification("asset-note"))
    if path in rules.get("pack_content", []):
        found.append(Classification("pack-content"))
    if OUT_OF_SCOPE_RE.match(path):
        found.append(Classification("out-of-scope"))

    owner = None
    for root in rules.get("evidence_roots", []):
        if path == root or path.startswith(root + "/"):
            # Nested roots are registered on purpose: the deepest one owns the
            # path, so a receipt can be registered beside its parent root.
            if owner is None or len(root) > len(owner):
                owner = root
    if owner is not None:
        found.append(Classification("evidence", owner))
    return found


def classify(path, policy):
    """The coverage class generation uses: the first match when a path matches two."""
    matches = coverage_classes(path, policy)
    return matches[0] if matches else UNCLASSIFIED


def problem_for(classification):
    """How a diagnostic names a class: the kind, and the owner that matched it."""
    if classification.owner is None:
        return classification.kind
    return "%s in '%s'" % (classification.kind, classification.owner)


def normalise_newlines(text):
    return text.replace("\r\n", "\n").replace("\r", "\n")


def git(root, *args):
    """Run one Git command, turning a failure into an actionable snapshot error."""
    return _git(root, args)


def _git(root, args, stdin=None):
    completed = subprocess.run(
        ["git", *args], cwd=str(root), input=stdin,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    if completed.returncode != 0:
        detail = completed.stderr.decode("utf-8", "replace").strip().splitlines()
        raise SnapshotError("git %s failed: %s" % (
            " ".join(args), detail[0] if detail else "no output"))
    return completed.stdout


class Snapshot:
    """A candidate tree: its tracked paths, plus the bytes at each path.

    `tracked()` lists every tracked path, because a `related` entry may point at
    a file that is not Markdown. `paths()` is the Markdown subset coverage
    classifies, as the policy names it.
    """

    label = "snapshot"

    def tracked(self):
        raise NotImplementedError

    def paths(self):
        return [path for path in self.tracked() if in_markdown_scope(path)]

    def read(self, path):
        raise SnapshotError("%s is not in the %s snapshot" % (path, self.label))

    def link_target(self, path):
        """The target `path` records when it is a symbolic link, else None."""
        return None

    def text(self, path):
        raw = self.read(path)
        try:
            return normalise_newlines(raw.decode("utf-8"))
        except UnicodeDecodeError as exc:
            raise SnapshotError("%s is not valid UTF-8 in the %s snapshot: %s"
                                % (path, self.label, exc)) from exc


def _read_blobs(root, shas):
    """Read several blobs in one `git cat-file --batch` call.

    One process per document cost 14x more on the prototype's realistic tree, so
    the whole Markdown corpus of a snapshot is fetched in a single call and
    looked up by object id afterwards.
    """
    if not shas:
        return {}
    request = ("\n".join(shas) + "\n").encode("ascii")
    out = _git(root, ["cat-file", "--batch"], stdin=request)
    blobs = {}
    position = 0
    for sha in shas:
        newline = out.index(b"\n", position)
        header = out[position:newline].decode("ascii", "replace")
        if header.endswith(" missing"):
            raise SnapshotError("blob %s is missing from the object database" % sha)
        size = int(header.rsplit(" ", 1)[1])
        start = newline + 1
        blobs[sha] = out[start:start + size]
        position = start + size + 1
    return blobs


class Worktree(Snapshot):
    """Tracked files as they are on disk; untracked files are never consulted."""

    label = "worktree"

    def __init__(self, root):
        self.root = Path(root).resolve()

    def tracked(self):
        out = git(self.root, "ls-files", "-z").decode("utf-8")
        return sorted(path for path in out.split("\0") if path)

    def read(self, path):
        target = (self.root / path).resolve()
        try:
            target.relative_to(self.root)
        except ValueError:
            raise SnapshotError("%s resolves outside the repository" % path) from None
        try:
            return target.read_bytes()
        except FileNotFoundError:
            raise SnapshotError(
                "%s is tracked but missing from the worktree" % path) from None

    def link_target(self, path):
        entry = self.root / path
        if not entry.is_symlink():
            return None
        return os.readlink(entry)


class BlobSnapshot(Snapshot):
    """A snapshot read from Git objects: each path records a mode and a blob id."""

    def __init__(self, root):
        self.root = Path(root).resolve()
        self.entries = {}
        self.modes = {}
        self.blobs = None

    def tracked(self):
        return sorted(self.entries)

    def missing(self, path):
        """The error for a path this snapshot does not hold."""
        raise NotImplementedError

    def read(self, path):
        return self._blob(path, self.missing(path))

    def link_target(self, path):
        if self.modes.get(path) != LINK_MODE:
            return None
        target = self._blob(path, self.missing(path))
        try:
            return target.decode("utf-8")
        except UnicodeDecodeError as exc:
            raise SnapshotError("%s is a symlink whose target is not UTF-8: %s"
                                % (path, exc)) from exc

    def _blob(self, path, missing):
        sha = self.entries.get(path)
        if sha is None:
            raise SnapshotError(missing)
        if self.blobs is None:
            # One `git cat-file --batch` covers the whole Markdown corpus; reading
            # each document in its own process costs an order of magnitude more.
            wanted = {digest for name, digest in self.entries.items() if is_markdown(name)}
            self.blobs = _read_blobs(self.root, sorted(wanted))
        if sha not in self.blobs:
            self.blobs.update(_read_blobs(self.root, [sha]))
        return self.blobs[sha]


class StagedIndex(BlobSnapshot):
    """The Git index: what the next commit will contain."""

    label = "index"

    def __init__(self, root):
        super().__init__(root)
        unmerged = []
        for record in git(self.root, "ls-files", "--stage", "-z").decode("utf-8").split("\0"):
            if not record:
                continue
            meta, _, path = record.partition("\t")
            mode, sha, stage = meta.split(" ")
            if stage != "0":
                unmerged.append(path)
                continue
            self.entries[path] = sha
            self.modes[path] = mode
        if unmerged:
            raise SnapshotError(
                "the index has unmerged entries, so no candidate snapshot exists: %s"
                % ", ".join(sorted(unmerged)))

    def missing(self, path):
        return "%s is not staged" % path


class GitTree(BlobSnapshot):
    """A committed revision's tree."""

    def __init__(self, root, revision):
        super().__init__(root)
        self.label = revision
        for record in git(self.root, "ls-tree", "-r", "-z", revision).decode("utf-8").split("\0"):
            if not record:
                continue
            meta, _, path = record.partition("\t")
            mode, kind, sha = meta.split(" ")
            if kind == "blob":
                self.entries[path] = sha
                self.modes[path] = mode

    def missing(self, path):
        return "%s is not in %s" % (path, self.label)


def follow_links(snapshot, owner_dir, path):
    """The path a `related` entry names after its symbolic links are followed.

    `path` has already passed the lexical checks in
    `docs_frontmatter.related_path_reason`. The snapshot supplies each link's
    recorded target, so the index and a committed tree are followed from their
    mode `120000` blobs rather than from the developer's checkout.
    """
    resolved = [part for part in owner_dir.split("/") if part]
    pending = [part for part in path.split("/") if part not in ("", ".")]
    followed = set()
    via = None
    while pending:
        segment = pending.pop(0)
        if segment == "..":
            if not resolved:
                if via is None:
                    raise LinkError("%r resolves above the repository root" % path)
                raise LinkError("%r resolves above the repository root after following "
                                "the symlink %r" % (path, via))
            resolved.pop()
            continue
        resolved.append(segment)
        here = "/".join(resolved)
        target = snapshot.link_target(here)
        if target is None:
            continue
        if here in followed:
            raise LinkError("%r revisits the symlink %r, so it never terminates"
                            % (path, here))
        if len(followed) >= MAX_LINK_DEPTH:
            raise LinkError("%r follows more than %d symlinks" % (path, MAX_LINK_DEPTH))
        followed.add(here)
        if target.startswith("/") or "\\" in target:
            raise LinkError("%r follows symlink %r to %r, which is not a repository path"
                            % (path, here, target))
        # A relative link target is relative to the directory holding the link.
        resolved.pop()
        pending[:0] = [part for part in target.split("/") if part not in ("", ".")]
        via = here
    return "/".join(resolved)


def represented_directories(paths):
    """Every directory a candidate path lives in or below, ancestors included."""
    directories = set()
    for path in paths:
        parent = posixpath.dirname(path)
        while parent and parent not in directories:
            directories.add(parent)
            parent = posixpath.dirname(parent)
    return directories


def repository_root(path):
    """The working-tree root of the repository containing `path`."""
    return Path(git(path, "rev-parse", "--show-toplevel").decode("utf-8").strip()).resolve()
