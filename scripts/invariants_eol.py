"""The line-ending invariant: no tracked text file is stored or checked out with CRLF.

`git ls-files --eol` reports two line-ending states for every tracked path, and
both matter here. A CRLF **index** entry means the repository stores the wrong
bytes. A CRLF **worktree** entry means this checkout hands the wrong bytes to
anything that reads files rather than blobs — and `cargo package` does exactly
that, so a drifted checkout changes what a release archive contains.

An attribute value of `-text` (or the `binary` macro) says the file's bytes are
content, not line endings: fixtures compared as raw bytes and preserved
evidence compared by SHA-256 are exempt on purpose (`.gitattributes`).

The index and worktree states are the difference between the accepted tree and
a machine that merely looks right; a rule that read only one could pass a
sabotaged checkout.
"""

from __future__ import annotations

import os
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Sequence

sys.path.insert(0, str(Path(__file__).resolve().parent))

from invariants_core import (  # noqa: E402
    CheckerError,
    RuleOutcome,
    Violation,
    expect_checker_error,
    run,
)

EOL_RULE = "eol"
EOL_INVARIANT = "no tracked text file is stored or checked out with CRLF"

# An attribute value that exempts a file from end-of-line conversion: `-text`
# opts out explicitly, and `binary` is the same statement through a macro. Their
# bytes are content, not line endings (fixtures, preserved evidence).
EOL_EXEMPT_ATTRS = ("-text", "binary")


@dataclass(frozen=True)
class EolEntry:
    """One `git ls-files --eol` record: a path and its two line-ending states."""

    path: str
    index: str
    worktree: str
    attr: str

    @property
    def exempt(self) -> bool:
        return any(flag in self.attr.split() for flag in EOL_EXEMPT_ATTRS)


def parse_eol_entries(raw: bytes) -> list[EolEntry]:
    """Read `git ls-files --eol -z` output into one entry per tracked path.

    Each NUL-terminated record is `<index> <worktree> attr/<attrs>\t<path>`,
    for example `i/lf    w/crlf  attr/text=auto eol=lf\tREADME.md`. A record
    this parser cannot read raises instead of being skipped, so a reshaped
    record cannot turn into a vacuous pass.
    """
    entries: list[EolEntry] = []
    for record in raw.split(b"\0"):
        if not record:
            continue
        header, _, path = os.fsdecode(record).partition("\t")
        fields = header.split()
        if (
            not path
            or len(fields) < 3
            or not fields[0].startswith("i/")
            or not fields[1].startswith("w/")
            or not fields[2].startswith("attr/")
        ):
            raise CheckerError(f"unexpected `git ls-files --eol` record: {header!r}")
        entries.append(
            EolEntry(
                path=path,
                index=fields[0][len("i/") :],
                worktree=fields[1][len("w/") :],
                attr=" ".join(fields[2:])[len("attr/") :],
            )
        )
    return entries


def check_no_crlf(entries: Sequence[EolEntry]) -> RuleOutcome:
    """The rule: no tracked text file stores or checks out CRLF."""
    offenders: list[str] = []
    for entry in entries:
        if entry.exempt:
            continue
        problems = [
            f"{side} {value}"
            for side, value in (("index", entry.index), ("worktree", entry.worktree))
            if value in ("crlf", "mixed")
        ]
        if problems:
            offenders.append(f"{entry.path} ({', '.join(problems)})")
    if not offenders:
        return RuleOutcome(EOL_INVARIANT)
    return RuleOutcome(
        EOL_INVARIANT,
        (
            Violation(
                offenders=tuple(offenders),
                reproduce="git ls-files --eol",
            ),
        ),
    )


def gather_eol_entries(root: Path) -> list[EolEntry]:
    """Every tracked path's index and worktree line endings."""
    entries = parse_eol_entries(run(["git", "ls-files", "--eol", "-z"], root))
    if not entries:
        raise CheckerError(
            "git ls-files --eol listed no tracked paths; refusing to pass the "
            "line-ending invariant vacuously"
        )
    return entries


def check(root: Path) -> RuleOutcome:
    """Run the rule against the tree checked out at `root`."""
    return check_no_crlf(gather_eol_entries(root))


def self_test_cases() -> list[tuple[str, Callable[[], None]]]:
    """Controlled cases: the rule must reject CRLF and accept the exempt shapes."""

    def eol_rule_accepts_lf_entries() -> None:
        entries = parse_eol_entries(
            b"i/lf    w/lf    attr/text=auto eol=lf\tREADME.md\0"
            b"i/none  w/none  attr/text=auto eol=lf\tengine/src/empty.rs\0"
        )
        outcome = check_no_crlf(entries)
        assert not outcome.violations, outcome.violations
        assert outcome.invariant == EOL_INVARIANT, outcome.invariant

    def eol_rule_rejects_a_crlf_blob() -> None:
        entries = parse_eol_entries(
            b"i/crlf  w/lf    attr/text=auto eol=lf\tengine/src/lib.rs\0"
        )
        outcome = check_no_crlf(entries)
        assert outcome.violations, "a CRLF blob must fail the rule"
        assert outcome.violations[0].offenders == (
            "engine/src/lib.rs (index crlf)",
        ), outcome.violations[0].offenders

    def eol_rule_rejects_a_crlf_checkout() -> None:
        entries = parse_eol_entries(
            b"i/lf    w/crlf  attr/text=auto eol=lf\tapp/src/main.rs\0"
        )
        outcome = check_no_crlf(entries)
        assert outcome.violations, "a CRLF checkout must fail the rule"
        assert outcome.violations[0].offenders == (
            "app/src/main.rs (worktree crlf)",
        ), outcome.violations[0].offenders

    def eol_rule_names_both_sides_of_a_mixed_file() -> None:
        entries = parse_eol_entries(
            b"i/mixed w/mixed attr/text=auto eol=lf\tcli/src/main.rs\0"
        )
        outcome = check_no_crlf(entries)
        assert outcome.violations[0].offenders == (
            "cli/src/main.rs (index mixed, worktree mixed)",
        ), outcome.violations[0].offenders

    def eol_rule_exempts_a_text_opt_out() -> None:
        entries = parse_eol_entries(
            b"i/crlf  w/mixed attr/-text            \t"
            b"cli/tests/fixtures/authoring/npc_accept.stdout\0"
        )
        outcome = check_no_crlf(entries)
        assert not outcome.violations, outcome.violations

    def eol_parser_refuses_unreadable_output() -> None:
        expect_checker_error(
            "an eol record with no attribute field",
            lambda: parse_eol_entries(b"i/lf    w/lf\tREADME.md\0"),
        )

    return [
        ("eol rule accepts LF entries", eol_rule_accepts_lf_entries),
        (
            "eol rule rejects a CRLF blob and names the path",
            eol_rule_rejects_a_crlf_blob,
        ),
        (
            "eol rule rejects a CRLF checkout and names the path",
            eol_rule_rejects_a_crlf_checkout,
        ),
        (
            "eol rule names both sides of a mixed file",
            eol_rule_names_both_sides_of_a_mixed_file,
        ),
        (
            "eol rule exempts a -text opt-out",
            eol_rule_exempts_a_text_opt_out,
        ),
        (
            "eol parser refuses unreadable output",
            eol_parser_refuses_unreadable_output,
        ),
    ]
