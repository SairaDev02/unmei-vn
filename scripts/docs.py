"""Entry point: generate, check and report coverage for one candidate snapshot.

`generate` writes the expected bytes of every active directory index and
preflights every destination first, so a symbolic link or junction cannot
redirect a write; `check` compares the expected bytes and refuses a relative
link or anchor the reader-facing documents cannot resolve, and never writes;
`coverage` classifies every tracked Markdown path exactly once; `report` prints
the advisory complexity report against the policy's fixed baseline. The policy
and the document bytes both come from the chosen snapshot, so a staged
check cannot be fooled by a worktree copy; `docs_policy` also loads and
validates that policy for every command here, merging the private overlay the
tree holds when it holds one. An exported tree holds no overlay, so it runs the
exported policy alone. The report's code-file categories and source-size
exclusion policy come from that same snapshot.

`report` imports `docs_report` and the optional `markdown-it-py` parser inside
the command, so the hard checks and the pre-commit closure stay standard library
only and never load a parser.

Exit status: 0 success, 1 findings, 2 tool error (3 for an unavailable parser in
`report`, which CI tolerates). A finding names the path, the rule and the remedy;
a tool error means the tree could not be read as specified.
"""

import argparse
import posixpath
import sys
from pathlib import Path
from typing import NamedTuple

import docs_destinations
import docs_frontmatter
import docs_index
import docs_links
import docs_policy
import docs_snapshot

DEFAULT_POLICY = "scripts/docs-policy.json"
REV_NEEDS_CHECKOUT = "--rev needs --checkout; the worktree and the index have one revision"

COVERAGE_CLASSES = (
    "ordinary", "generated", "root-guidance", "skill-format", "template",
    "exploration", "crate-readme", "archive-readme", "asset-note", "pack-content",
    "out-of-scope", "evidence", "unclassified",
)

MESSAGES = {
    "missing-frontmatter": "no frontmatter block",
    "unterminated-frontmatter": "the frontmatter block is not closed",
    "missing-field": "missing required field '{field}'",
    "unknown-field": "unknown field '{detail}'",
    "duplicate-key": "duplicate field '{detail}'",
    "expected-key": "a frontmatter line is neither a field nor a list item",
    "indented-line": "a frontmatter line is indented without starting a list item",
    "indented-key": "field '{field}' is indented; keys start at column one",
    "tab-character": "a frontmatter line contains a tab",
    "list-without-key": "a list item has no field above it",
    "empty-list-item": "a list item is empty",
    "empty-list-value": "the '{field}' list has no items",
    "empty-value": "a field has no value",
    "flow-collection": "only `[]` may be written as a flow collection",
    "flow-mapping": "nested maps are not part of the subset",
    "block-scalar": "multi-line block scalars are not part of the subset",
    "requires-quotes": "'{field}' contains #, :, a backslash or a double quote",
    "core-type-not-string": "'{field}' resolves to a boolean, null or number, not a string",
    "indicator-start": "'{field}' starts with a YAML indicator character",
    "unsupported-escape": "'{field}' uses an escape the subset does not allow",
    "malformed-unicode-escape": "'{field}' has a malformed \\u escape",
    "surrogate-escape": "'{field}' escapes a surrogate code point",
    "unterminated-quoted-value": "'{field}' has an unterminated double-quoted value",
    "trailing-after-quote": "'{field}' has text after its closing quote",
    "control-character": "'{field}' contains a control character",
    "empty-title": "the title is empty",
    "wrong-type": "'{field}' has the wrong shape",
    "whitespace-only-summary": "the summary is whitespace only",
    "summary-too-long": "the summary is longer than 160 characters",
    "unpaired-surrogate": "the summary holds an unpaired surrogate",
    "empty-keywords": "the keyword list is empty",
    "too-many-keywords": "the keyword list holds more than 8 terms",
    "invalid-keyword": "a keyword is not a string",
    "duplicate-keyword": "keyword '{detail}' is listed twice",
    "keyword-not-approved": "keyword '{detail}' is not in the approved vocabulary",
    "keyword-spelling": "keyword '{detail}' is not lowercase ASCII with single hyphens",
    "empty-status": "the status is empty",
    "invalid-path": "a related entry is not a path",
    "repository-escape": "'{detail}' resolves above the repository root",
    "absolute-path": "'{detail}' is absolute; use a path relative to the document",
    "url-path": "'{detail}' is a URL; only repository-local paths are allowed",
    "backslash-path": "'{detail}' uses backslashes; use forward slashes",
    "empty-segment": "'{detail}' has an empty path segment",
}

REMEDIES = {
    "missing-frontmatter": "start the file with a `---` line, then title, summary and "
                           "keywords, then a closing `---` line",
    "unterminated-frontmatter": "close the frontmatter with a `---` line",
    "missing-field": "add '{field}:' to the frontmatter above the closing fence",
    "unknown-field": "remove '{detail}' from the frontmatter; the schema allows title, "
                     "summary, keywords, status and related",
    "duplicate-key": "remove the repeated '{detail}' field; a field appears once",
    "empty-keywords": "list 1-8 approved keywords as `  - term` lines",
    "too-many-keywords": "keep at most 8 keywords",
    "duplicate-keyword": "remove the duplicated term; keywords are unique",
    "keyword-not-approved": "replace '{detail}' with a term from {policy}",
    "keyword-spelling": "write '{detail}' as lowercase ASCII letters, digits and single hyphens",
    "summary-too-long": "shorten the summary to at most 160 characters",
    "unpaired-surrogate": "remove the unpaired surrogate from the summary",
    "requires-quotes": "wrap '{field}' in double quotes; plain values cannot contain #, :, "
                       r"backslash or a double quote",
    "core-type-not-string": "wrap '{field}' in double quotes; a bare boolean, null or number "
                            "is not a string",
    "repository-escape": "'{detail}' resolves above the repository root; use a path that stays "
                         "inside the repository",
    "related-missing": "point 'related' at a file or directory in the candidate tree, or remove "
                       "the entry",
    "empty-segment": "remove the empty segment from '{detail}'",
    "related-link": "point 'related' at a path that stays inside the repository once its "
                    "symbolic links are followed",
}
DEFAULT_REMEDY = ("correct the frontmatter syntax; the grammar is a restricted "
                  "subset of YAML, not full YAML")


class Problem(NamedTuple):
    path: str
    message: str
    remedy: str

    def render(self):
        return "%s: %s\n  fix: %s" % (self.path, self.message, self.remedy)


def diagnostics_problems(path, result, policy_path):
    problems = []
    for diagnostic in result.errors:
        reason = diagnostic.reason
        problems.append(Problem(
            path,
            "%s (line %d)" % (
                MESSAGES.get(reason, reason).format(
                    field=diagnostic.field, detail=diagnostic.detail or ""),
                diagnostic.line),
            REMEDIES.get(reason, DEFAULT_REMEDY).format(
                field=diagnostic.field, detail=diagnostic.detail or "", policy=policy_path),
        ))
    return problems


def related_problems(path, values, snapshot, directories):
    """`related` must name something the candidate tree already holds."""
    problems = []
    owner_dir = posixpath.dirname(path)
    for target in values.get("related") or []:
        if not isinstance(target, str):
            continue
        try:
            resolved = docs_snapshot.follow_links(snapshot, owner_dir, target)
        except docs_snapshot.LinkError as exc:
            problems.append(Problem(path, "related entry '%s' %s" % (target, exc),
                                    REMEDIES["related-link"]))
            continue
        if resolved in snapshot.tracked() or resolved in directories:
            continue
        problems.append(Problem(path, "related entry '%s' is not in the candidate tree" % target,
                                REMEDIES["related-missing"]))
    return problems


def build_indexes(policy, snapshot, policy_path):
    """The expected bytes of every active index, plus every finding that blocks them."""
    tracked = snapshot.tracked()
    directories = docs_snapshot.represented_directories(tracked)
    active = policy["active_directories"]
    index_name = policy["index_name"]
    documents = {name: [] for name in active}
    problems = []

    for path in snapshot.paths():
        classification = docs_snapshot.classify(path, policy)
        if classification.kind != "ordinary" or classification.owner not in documents:
            continue
        try:
            text = snapshot.text(path)
        except docs_snapshot.SnapshotError as exc:
            problems.append(Problem(path, str(exc), "restore the file or stage its deletion"))
            continue
        result = docs_frontmatter.parse_frontmatter(
            text, policy, owner_dir=classification.owner)
        if not result.ok:
            problems.extend(diagnostics_problems(path, result, policy_path))
            continue
        documents[classification.owner].append(docs_index.Document(
            path=path,
            title=result.values["title"],
            summary=result.values["summary"],
            keywords=result.values["keywords"],
            status=result.values.get("status"),
        ))
        problems.extend(related_problems(path, result.values, snapshot, directories))

    indexes = {}
    for directory in active:
        children = [child for child in policy["indexed_directories"]
                    if child in active and posixpath.dirname(child) == directory]
        indexes[directory + "/" + index_name] = docs_index.render(
            directory, documents[directory], children, policy)
        problems.extend(landing_problems(directory, policy, tracked, directories, policy_path))

    problems.extend(section_link_problems(policy, tracked, policy_path))
    return indexes, problems


def landing_problems(directory, policy, tracked, directories, policy_path):
    problems = []
    index_path = directory + "/" + policy["index_name"]
    for link in policy["excluded_landing_links"].get(index_path, []):
        resolved = docs_frontmatter.resolve_related(directory, link["target"])
        if resolved in tracked or resolved in directories:
            continue
        problems.append(Problem(
            index_path,
            "landing link '%s' targets %s, which is not in the candidate tree"
            % (link["label"], resolved),
            "correct the target in %s" % policy_path))
    return problems


def section_link_problems(policy, tracked, policy_path):
    problems = []
    index_name = policy["index_name"]
    for path in sorted(policy["section_links"]):
        directory = posixpath.dirname(path)
        if directory not in policy["active_directories"] or path in tracked:
            continue
        problems.append(Problem(
            path,
            "configured section links target a file that is not in the candidate tree",
            "correct or remove the entry in %s; %s/%s is generated from it"
            % (policy_path, directory, index_name)))
    return problems


def make_snapshot(root, args):
    if args.staged:
        return docs_snapshot.StagedIndex(root)
    if args.checkout:
        return docs_snapshot.GitTree(root, args.rev or "HEAD")
    return docs_snapshot.Worktree(root)


def report(problems, stream=sys.stderr):
    for problem in problems:
        stream.write(problem.render() + "\n")
    if problems:
        stream.write("%d problem%s found; nothing was written.\n"
                     % (len(problems), "" if len(problems) == 1 else "s"))


def write_indexes(root, indexes):
    """Write only the indexes whose bytes differ; return how many changed."""
    written = 0
    for path, content in sorted(indexes.items()):
        target = root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        if target.is_file() and target.read_bytes() == content:
            continue
        target.write_bytes(content)
        written += 1
    return written


def command_generate(root, snapshot, policy, policy_path):
    indexes, problems = build_indexes(policy, snapshot, policy_path)
    if not problems:
        problems = [Problem(*item) for item in docs_destinations.link_problems(root, indexes)]
    if problems:
        report(problems)
        return 1
    written = write_indexes(root, indexes)
    sys.stdout.write("docs: %d index file%s, %d written.\n"
                     % (len(indexes), "" if len(indexes) == 1 else "s", written))
    return 0


def classify_all(snapshot, policy):
    """Per-class counts, the paths no rule matched, and the paths two classes match."""
    counts = {}
    unclassified = []
    ambiguous = []
    for path in snapshot.paths():
        matches = docs_snapshot.coverage_classes(path, policy)
        kind = matches[0].kind if matches else "unclassified"
        counts[kind] = counts.get(kind, 0) + 1
        if not matches:
            unclassified.append(path)
        elif len(matches) > 1:
            ambiguous.append((path, matches))
    return counts, unclassified, ambiguous


def coverage_problems(unclassified, ambiguous, policy_path):
    problems = [Problem(path, "unclassified tracked Markdown path",
                        "add a rule to %s or move the file into an indexed directory"
                        % policy_path)
                for path in unclassified]
    return problems + [
        Problem(path, "matches more than one coverage class: %s" % " and ".join(
            docs_snapshot.problem_for(item) for item in matches),
            "leave exactly one rule matching %s in %s" % (path, policy_path))
        for path, matches in ambiguous]


def command_check(root, snapshot, policy, policy_path):
    indexes, problems = build_indexes(policy, snapshot, policy_path)
    _counts, unclassified, ambiguous = classify_all(snapshot, policy)
    problems.extend(coverage_problems(unclassified, ambiguous, policy_path))
    for path, expected in sorted(indexes.items()):
        try:
            actual = snapshot.read(path)
        except docs_snapshot.SnapshotError:
            problems.append(Problem(
                path, "index is missing from the %s snapshot" % snapshot.label,
                index_remedy(policy, path)))
            continue
        if actual != expected:
            problems.append(Problem(
                path, "index is stale for the %s snapshot" % snapshot.label,
                index_remedy(policy, path)))
    findings, links = docs_links.link_problems(snapshot, policy)
    problems.extend(Problem(*finding) for finding in findings)
    if problems:
        report(problems)
        return 1
    sys.stdout.write("docs: %d tracked Markdown paths classified, %d index file%s fresh "
                     "(%s snapshot).\n"
                     % (len(snapshot.paths()), len(indexes),
                        "" if len(indexes) == 1 else "s", snapshot.label))
    sys.stdout.write("docs: %d relative link%s in the reader-facing documents resolve "
                     "(%s snapshot).\n" % (links, "" if links == 1 else "s", snapshot.label))
    return 0


def index_remedy(policy, path):
    return "%s && git add %s" % (policy["generate_command"], path)


def command_coverage(root, snapshot, policy, policy_path):
    counts, unclassified, ambiguous = classify_all(snapshot, policy)
    sys.stdout.write("%d tracked Markdown paths\n" % len(snapshot.paths()))
    for kind in sorted(COVERAGE_CLASSES):
        sys.stdout.write("  %-16s %d\n" % (kind, counts.get(kind, 0)))
    for problem in coverage_problems(unclassified, ambiguous, policy_path):
        sys.stderr.write(problem.render() + "\n")
    return 1 if unclassified or ambiguous else 0


COMMANDS = {
    "generate": command_generate,
    "check": command_check,
    "coverage": command_coverage,
}

# `report` is not in COMMANDS: it needs the summary-file argument and imports the
# optional parser, so it dispatches separately and never joins the hook closure.
REPORT = "report"


def parse_args(argv):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("command", choices=sorted(list(COMMANDS) + [REPORT]))
    parser.add_argument("--root", default=None,
                        help="repository root (default: the repository holding the cwd)")
    parser.add_argument("--policy", default=DEFAULT_POLICY,
                        help="policy path relative to the repository root")
    parser.add_argument("--summary-file", default=None,
                        help="append the report to this file (the CI step summary)")
    source = parser.add_mutually_exclusive_group()
    source.add_argument("--worktree", action="store_true",
                        help="read the files on disk (the default)")
    source.add_argument("--staged", action="store_true", help="read the Git index")
    source.add_argument("--checkout", action="store_true",
                        help="read a committed tree, HEAD unless --rev names another")
    parser.add_argument("--rev", default=None, help="revision for --checkout")
    return parser.parse_args(argv)


def tool_error(message):
    sys.stderr.write("docs: %s\n" % message)
    return 2


def prepare(args):
    """The root, snapshot and policy every command reads from the selected tree."""
    root = Path(args.root).resolve() if args.root else \
        docs_snapshot.repository_root(Path.cwd())
    snapshot = make_snapshot(root, args)
    return root, snapshot, docs_policy.load_policy(snapshot, args.policy)


def command_report(args):
    """The advisory report, whose own setup failures say it did not run.

    `docs_report` is imported here so the hard checks and the pre-commit closure
    never load it; a report that fails before `run` reports through the same
    diagnostic the report itself uses, including the requested summary.
    """
    import docs_report
    if args.rev and not args.checkout:
        return docs_report.unavailable(REV_NEEDS_CHECKOUT, args.summary_file)
    try:
        _root, snapshot, policy = prepare(args)
    except (docs_snapshot.SnapshotError, OSError, ValueError) as exc:
        return docs_report.unavailable(str(exc), args.summary_file)
    return docs_report.run(snapshot, policy, args.policy, args.summary_file)


def main(argv):
    args = parse_args(argv)
    if args.command == REPORT:
        return command_report(args)
    if args.rev and not args.checkout:
        return tool_error(REV_NEEDS_CHECKOUT)
    try:
        root, snapshot, policy = prepare(args)
    except (docs_snapshot.SnapshotError, OSError, ValueError) as exc:
        return tool_error(str(exc))
    try:
        return COMMANDS[args.command](root, snapshot, policy, args.policy)
    except (docs_snapshot.SnapshotError, OSError) as exc:
        return tool_error(str(exc))


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
