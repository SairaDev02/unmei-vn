"""Relative links and anchors in the reader-facing documents.

The scope is deliberately narrow: the Guide pages, `README.md` and
`docs/architecture.md`, which is what a player, an author or a newcomer reads
first, and what changes with a release. Every other document stays out of scope,
so a pre-existing broken link elsewhere cannot fail the gate that guards these.

A link is read from the candidate snapshot, never from the filesystem, so
`--worktree`, `--staged` and `--checkout` judge the same bytes as the rest of the
check. A scoped path the snapshot does not hold is skipped: `docs/architecture.md`
and the Guide pages arrive in later tickets, and a link to a document that is not
there yet is not a fault in the linking page.

Skipped targets: an absolute URL or a protocol-relative one, because CI has no
network; and a `@@…@@` placeholder, which the export fills after this repository
is copied. A `#fragment` is compared against the anchors GitHub derives from the
document's headings, so a renamed heading fails the link that named it.

Only ATX headings (`## Text`) define anchors: that is the form this repository
writes, and a setext heading would have to be added here before a link may
target it.
"""

import posixpath
import re
import urllib.parse

import docs_frontmatter
import docs_snapshot

# A fence is three or more backticks or tildes; a heading or a link inside one is
# example text, not structure.
FENCE_RE = re.compile(r"^ {0,3}(`{3,}|~{3,})")

# `## Text`, indented by at most three spaces. Closing hashes are dropped below.
ATX_RE = re.compile(r"^ {0,3}#{1,6}[ \t]+(.*?)\s*$")
CLOSING_HASHES_RE = re.compile(r"\s+#+\s*$")

# An inline link's destination: wrapped in angle brackets, or up to the next space
# or `)`. The link text may hold nested brackets, and a title may follow.
LINK_RE = re.compile(r"\[[^\]]*\]\(\s*(<[^>]*>|[^)\s]+)(?:\s+(?:\"[^\"]*\"|'[^']*'))?\s*\)")

# Inline Markdown is rendered before an anchor is made, so a link in a heading
# contributes its text and a code span its contents.
HEADING_LINK_RE = re.compile(r"\[([^\]]*)\]\([^)]*\)")

# Why a target is not a repository-relative path, once the skips above are done.
NOT_RELATIVE = {
    "invalid-path": "it is not a path",
    "backslash-path": "it uses backslashes",
    "absolute-path": "it starts at the site root",
    "empty-segment": "it has an empty path segment",
    "repository-escape": "it points above the repository root",
}

UNREADABLE_REMEDY = "restore the file as UTF-8 text, or take it out of the link-check scope"


def fenced_off(text):
    """The lines outside fenced code blocks."""
    fence = None
    for line in text.split("\n"):
        marker = FENCE_RE.match(line)
        if fence is None:
            if marker:
                fence = marker.group(1)
                continue
            yield line
        elif marker and marker.group(1)[0] == fence[0] \
                and len(marker.group(1)) >= len(fence):
            fence = None


def without_code_spans(line):
    """`line` with each inline code span emptied: a link inside backticks is text."""
    pieces = []
    index = 0
    while True:
        start = line.find("`", index)
        if start < 0:
            return "".join(pieces) + line[index:]
        tail = line[start:]
        run = len(tail) - len(tail.lstrip("`"))
        close = line.find("`" * run, start + run)
        if close < 0:
            return "".join(pieces) + line[index:]
        pieces.append(line[index:start])
        index = close + run


def anchor(heading):
    """GitHub's anchor for one heading: lowercase, punctuation dropped, spaces to hyphens.

    Letters and digits are kept in any script, as GitHub keeps them; so are
    spaces, hyphens and underscores. Everything else goes, which is why
    `## 4. Decision configs and decision points` anchors as
    `#4-decision-configs-and-decision-points`.
    """
    text = HEADING_LINK_RE.sub(r"\1", heading).replace("`", "")
    kept = "".join(ch for ch in text.lower() if ch.isalnum() or ch in " -_")
    return kept.replace(" ", "-")


def anchors(text):
    """Every anchor `text` defines, including GitHub's `-1` suffix for a repeat."""
    counts = {}
    found = set()
    for line in fenced_off(text):
        match = ATX_RE.match(line)
        if not match:
            continue
        slug = anchor(CLOSING_HASHES_RE.sub("", match.group(1)))
        seen = counts.get(slug, 0)
        counts[slug] = seen + 1
        found.add(slug if seen == 0 else "%s-%d" % (slug, seen))
    return found


def links(text):
    """The destination of every inline link outside a fence or an inline code span."""
    for line in fenced_off(text):
        for match in LINK_RE.finditer(without_code_spans(line)):
            target = match.group(1)
            if target.startswith("<") and target.endswith(">"):
                target = target[1:-1]
            yield target


def skipped(target):
    """Whether `target` is not a repository link: absent, external, or a placeholder."""
    return (not target or "@@" in target or target.startswith("//")
            or docs_frontmatter.SCHEME_RE.match(target) is not None)


def scoped_paths(policy, tracked):
    """The reader-facing documents the snapshot holds, in report order."""
    scope = policy["link_check"]
    wanted = {path for path in scope["files"] if path in tracked}
    for directory in scope["directories"]:
        prefix = directory + "/"
        wanted.update(path for path in tracked
                      if path.startswith(prefix) and path.endswith(".md")
                      and "/" not in path[len(prefix):])
    return sorted(wanted)


class LinkCheck:
    """One candidate snapshot's link check, reading each target document once."""

    def __init__(self, snapshot, policy):
        self.snapshot = snapshot
        self.policy = policy
        self.tracked = set(snapshot.tracked())
        self.directories = docs_snapshot.represented_directories(self.tracked)
        self.cached_anchors = {}

    def run(self):
        """Every finding as `(path, message, remedy)`, and how many links were checked."""
        findings = []
        checked = 0
        for path in scoped_paths(self.policy, self.tracked):
            try:
                text = self.snapshot.text(path)
            except docs_snapshot.SnapshotError as exc:
                findings.append((path, str(exc), UNREADABLE_REMEDY))
                continue
            for target in links(text):
                if skipped(target):
                    continue
                checked += 1
                finding = self.finding(path, target)
                if finding:
                    findings.append(finding)
        return findings, checked

    def finding(self, path, target):
        """The finding one link earns, or None when its path and anchor both resolve."""
        location, _, fragment = target.partition("#")
        location = urllib.parse.unquote(location)
        fragment = urllib.parse.unquote(fragment)
        if not location:
            return self.anchor_finding(path, path, target, fragment) if fragment else None

        reason = docs_frontmatter.related_path_reason(location, posixpath.dirname(path))
        if reason:
            return (path,
                    "link target '%s' is not a repository-relative path: %s"
                    % (target, NOT_RELATIVE.get(reason, reason)),
                    "point the link at a path relative to %s" % path)

        resolved = docs_frontmatter.resolve_related(posixpath.dirname(path), location)
        if resolved in self.directories:
            if not fragment:
                return None
            return (path,
                    "link target '%s' names a directory, so it has no anchor" % target,
                    "drop the '#%s' fragment, or link the document that holds it" % fragment)
        if resolved not in self.tracked:
            return (path,
                    "link target '%s' is not in the candidate tree" % target,
                    "correct the path, or add the file it names")
        if not fragment or not docs_snapshot.is_markdown(resolved):
            return None
        return self.anchor_finding(path, resolved, target, fragment)

    def anchor_finding(self, path, resolved, target, fragment):
        """The finding when `fragment` is not one of `resolved`'s anchors, else None."""
        try:
            known = self.anchors_of(resolved)
        except docs_snapshot.SnapshotError as exc:
            return (resolved, str(exc), UNREADABLE_REMEDY)
        if fragment in known:
            return None
        return (path,
                "link target '%s' names an anchor that %s does not define"
                % (target, resolved),
                "point the fragment at a heading in %s, or add that heading" % resolved)

    def anchors_of(self, path):
        if path not in self.cached_anchors:
            self.cached_anchors[path] = anchors(self.snapshot.text(path))
        return self.cached_anchors[path]


def link_problems(snapshot, policy):
    """Every reader-facing link finding for `snapshot`, and how many links were checked."""
    return LinkCheck(snapshot, policy).run()
