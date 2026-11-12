"""Advisory complexity measurement: the five metric families in the frozen baseline.

The definitions are a port of the reference tool that produced the pinned
baseline the report compares against, because a different reading of a silent
definition would fabricate a delta out of a definition change. Where this module
and that tool could disagree, the tool wins and `docs_report` compares against
the baseline it recorded.

This module is standard library only, except the Markdown parser, which is
imported inside `measure_document`: the pre-commit hook and the other `check`
commands never load `docs_metrics` at all, so the optional parser stays an
instalment of the advisory report alone. Nothing here decides a warning, renders
a finding or reads the policy; `docs_report` owns those.

The source-size audit ships beside this module and is loaded in-process for its
classifier, so a code-file category means here what it means at the size gate;
the exclusion policy is read from the snapshot being measured, never from
another snapshot or from the local copy.
"""

import json
import posixpath
import re
import runpy
from pathlib import Path

import docs_snapshot

# The reference strips a leading frontmatter block before every Markdown
# measurement, and accepts trailing spaces on the closing fence. Kept verbatim so
# the same bytes measure the same; `docs_frontmatter` is the validator, not this.
FRONTMATTER_RE = re.compile(r"\A---\n(.*?\n)?---[ \t]*\n", re.S)
ASCII_WHITESPACE = " \t\n\r\f\v"
TOKEN_RE = re.compile(r"[^%s]+" % re.escape(ASCII_WHITESPACE))

# Containers whose paragraphs are not narrative, and the wider set that also
# keeps a heading out of the section hierarchy. The reference reads "outside
# lists, including blockquotes" strictly: a paragraph in a blockquote nested in a
# list item is excluded.
LIST_CONTAINERS = {"bullet_list_open", "ordered_list_open", "list_item_open"}
CONTAINERS = LIST_CONTAINERS | {"blockquote_open"}

PREAMBLE = "preamble"

# The reference measures tracked `.rs` and `.py` files plus the one standalone
# shell script. `.sh` files exist only in retained evidence, outside the authored
# tree, so including them would move the pinned totals without any source growth.
SOURCE_SUFFIXES = {".rs": "rust", ".py": "python"}
STANDALONE_SHELL = ".githooks/pre-commit"
# The audit tool ships beside this module, so a fixture repository can be measured
# with the real classifier even though it holds no tooling of its own. Its `main()`
# is `__main__`-guarded, so loading it here defines the classifier without running
# a whole-tree audit.
AUDIT_TOOL = Path(__file__).resolve().parent / "audit-source-size.py"


class MeasurementError(Exception):
    """A measurement could not be taken as specified."""


# The adopted advisory families, shared by the report and its text so neither has
# to restate a name. The policy owns the numbers; this table owns the vocabulary.
# Tuple shape: policy metric key, human family label, measured unit, record key.
# The other three families are measurement only: the report prints their counts
# and baseline deltas, and no code path gives them a reference value.
FAMILIES = (
    ("document_lines", "document length", "body lines", "lines"),
    ("document_units", "document length", "whitespace units", "tokens"),
    ("heading_density", "heading density", "headings", "headings"),
)
ADOPTED = tuple(family[0] for family in FAMILIES)
LABELS = {metric: "%s: %s" % (family, unit) for metric, family, unit, _key in FAMILIES}
FIELDS = {metric: key for metric, _family, _unit, key in FAMILIES}

# Only production and example sources are candidates for a size signal; tests and
# generated/historical sources are measured and reported without one.
CODE_CATEGORIES = ("production", "example")


def parser_version():
    """The optional parser's version; raises ImportError when it is not installed."""
    import markdown_it

    return markdown_it.__version__


def _parser():
    """CommonMark plus tables, with raw HTML kept as HTML rather than prose.

    `html=True` is the reference's setting; it changes whether an HTML block
    counts as a paragraph, so the baseline is only reproducible with it.
    """
    from markdown_it import MarkdownIt

    return MarkdownIt("commonmark", {"html": True}).enable("table")


def split_frontmatter(text):
    """Return (frontmatter, body): frontmatter is stripped before every measurement."""
    normalised = text.replace("\r\n", "\n")
    match = FRONTMATTER_RE.match(normalised)
    if not match:
        return "", normalised
    return match.group(0), normalised[match.end():]


def body_stats(body):
    """Physical body lines and ASCII-whitespace-separated tokens.

    Blanks, fences, tables and code are counted; a final newline does not add an
    empty line. A token is a maximal run of non-ASCII-whitespace characters, so
    U+00A0 and its neighbours stay inside a token.
    """
    lines = body.split("\n")
    if lines and lines[-1] == "":
        lines.pop()
    return len(lines), len(TOKEN_RE.findall(body))


def measure_document(text):
    """The baseline-shaped measurement of one ordinary document."""
    _frontmatter, body = split_frontmatter(text)
    lines, tokens = body_stats(body)
    headings, sections, preamble_paragraphs, first_section = _structure(body)

    if first_section is None:
        # A headingless document always has its sole preamble, even when empty.
        sections = [{"identity": PREAMBLE, "paragraphs": preamble_paragraphs}]
    elif first_section > 0:
        # Any parsed content before the first section heading is a preamble,
        # including a list-, table- or code-only one with no narrative
        # paragraphs; a heading as the first parsed content records none.
        sections.insert(0, {"identity": PREAMBLE, "paragraphs": preamble_paragraphs})

    identities = [json.dumps(section["identity"], ensure_ascii=False)
                  for section in sections if section["identity"] != PREAMBLE]
    ambiguous = sorted({identity for identity in identities if identities.count(identity) > 1})
    return {
        "lines": lines,
        "tokens": tokens,
        "headings": len(headings),
        "sections": sections,
        "max_section_paragraphs": max((section["paragraphs"] for section in sections), default=0),
        "ambiguous_identities": ambiguous,
    }


def _structure(body):
    """Headings, top-level sections, preamble paragraphs and the first section's index."""
    tokens = _parser().parse(body)
    headings = []
    sections = []
    preamble_paragraphs = 0
    first_section = None
    stack = []                     # open list/blockquote containers
    heading_stack = []
    current = None

    for index, token in enumerate(tokens):
        if token.type in CONTAINERS:
            stack.append(token.type)
            continue
        if token.nesting == -1 and token.type.endswith("_close"):
            opened = token.type[:-6] + "_open"
            if opened in stack:
                while stack and stack[-1] != opened:
                    stack.pop()
                if stack:
                    stack.pop()
            continue
        if token.type == "heading_open":
            inline = tokens[index + 1]
            heading = (int(token.tag[1]), inline.content if inline.type == "inline" else "")
            if stack:
                # Inside a list or blockquote: density only, never a section.
                headings.append(heading)
                continue
            if first_section is None:
                first_section = index
            if current is not None:
                sections.append(current)
            while heading_stack and heading_stack[-1][0] >= heading[0]:
                heading_stack.pop()
            heading_stack.append(heading)
            current = {"identity": [list(step) for step in heading_stack], "paragraphs": 0}
            headings.append(heading)
            continue
        if token.type == "paragraph_open" and not any(x in LIST_CONTAINERS for x in stack):
            if current is None:
                preamble_paragraphs += 1
            else:
                current["paragraphs"] += 1

    if current is not None:
        sections.append(current)
    return headings, sections, preamble_paragraphs, first_section


def directory_density(documents, directories):
    """Direct ordinary documents per indexed directory."""
    return {name: sum(1 for path in documents if posixpath.dirname(path) == name)
            for name in directories}


def source_kind(path):
    """The measured kind of a tracked authored source file, or None."""
    if path == STANDALONE_SHELL:
        return "shell"
    return SOURCE_SUFFIXES.get(posixpath.splitext(path)[1])


def physical_lines(data):
    """Physical lines in a blob, including blanks, comments and inline tests."""
    return data.count(b"\n") + (0 if data.endswith(b"\n") else 1)


def measure_code_files(snapshot):
    """Physical lines per tracked authored source file, split by audit category.

    Paths, bytes and the source-size exclusion policy all come from `snapshot`,
    so a staged path is categorized by the same authoritative rules as its
    committed equivalent and no other snapshot's policy can leak in.
    """
    paths = [path for path in snapshot.tracked() if source_kind(path)]
    if not paths:
        return {"files": [], "total": 0, "total_lines": 0, "by_category": {}}
    categories = audit_categories(snapshot, paths)
    files = []
    for path in paths:
        data = snapshot.read(path)
        files.append({
            "path": path,
            "kind": source_kind(path),
            "category": categories[path],
            "bytes": len(data),
            "lines": physical_lines(data),
        })
    files.sort(key=lambda item: item["path"])
    by_category = {}
    for category in sorted({item["category"] for item in files}):
        family = [item for item in files if item["category"] == category]
        by_category[category] = {"files": len(family),
                                 "lines": sum(item["lines"] for item in family)}
    return {
        "files": files,
        "total": len(files),
        "total_lines": sum(item["lines"] for item in files),
        "by_category": by_category,
    }


def audit_categories(snapshot, paths):
    """The category `scripts/audit-source-size.py` records for each of `paths`.

    The classifier is loaded in-process from the audit tool rather than
    reimplemented, so the report and the source-size gate cannot grow competing
    rules for what a path is.
    """
    tool = runpy.run_path(str(AUDIT_TOOL))
    return tool["classify_paths"](paths, snapshot_checker(snapshot, tool))


def snapshot_checker(snapshot, tool):
    """The exclusion predicate `snapshot` records, or a MeasurementError naming it.

    There is deliberately no fallback to another snapshot or to the local copy:
    either would classify one tree's paths with a different tree's exclusions.
    The policy's repository path comes from the audit tool, which owns it.
    """
    checker_path = tool["CHECKER_REPO_PATH"]
    origin = "%s:%s" % (snapshot.label, checker_path)
    try:
        source = snapshot.text(checker_path)
    except docs_snapshot.SnapshotError as exc:
        raise MeasurementError("the %s snapshot cannot supply the source-size exclusion "
                               "policy: %s" % (snapshot.label, exc)) from exc
    try:
        checker = tool["compile_checker"](source, origin)
    except ValueError as exc:
        raise MeasurementError("the %s snapshot cannot supply the source-size exclusion "
                               "policy: %s" % (snapshot.label, exc)) from exc
    if not callable(getattr(checker, "excluded", None)):
        raise MeasurementError("the %s snapshot cannot supply the source-size exclusion "
                               "policy: %s defines no 'excluded' predicate"
                               % (snapshot.label, checker_path))
    return checker
