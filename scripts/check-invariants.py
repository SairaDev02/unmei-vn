#!/usr/bin/env python3
"""Check the repository's stable architecture invariants.

Run from anywhere; the repository root is derived from this file's location.

    python3 scripts/check-invariants.py                  # all rules, this tree
    python3 scripts/check-invariants.py --rules engine   # engine dependencies
    python3 scripts/check-invariants.py --rules paths    # tracked paths
    python3 scripts/check-invariants.py --rules eol      # line endings
    python3 scripts/check-invariants.py --rules licenses # accepted licenses
    python3 scripts/check-invariants.py --self-test      # prove each rule can fail
    python3 scripts/check-invariants.py --root <dir>     # another checkout

``--root`` points the rules at a tree this file does not live in, so the export
can run them over a checkout of the exported file set and prove that set passes
its own gate.

Invariants
----------
engine
    1. ``unmei-engine`` pulls no Bevy package in any feature or target
       configuration. The rule reads Cargo's resolved dependency graph
       (``cargo tree --locked -p unmei-engine``, normal + build + dev edges)
       under the minimal feature selection and ``--all-features``, each
       resolved for every target. Cargo's features are additive, so those two
       selections bound all the others; no crate is compiled. ``--locked``
       makes a stale ``Cargo.lock`` fail the check instead of resolving, and
       rewriting, the lockfile the gate's later Cargo commands then read.
paths
    2. No tracked file lives under ``app/prototype/``: a prototype is kept off
       ``main`` on its own ``prototype/<issue>-<name>`` branch.
eol
    3. No tracked text file is stored with CRLF, or checked out with it.
       ``git ls-files --eol`` reports each entry's index and worktree endings;
       a file the attributes mark ``-text`` keeps its bytes on purpose
       (fixtures and preserved evidence compared by hash) and is exempt. The
       worktree half matters as much as the index half: ``cargo package``
       reads the working tree, so a checkout that hands it CRLF changes what a
       release archive contains.
licenses
    4. ``deny.toml``'s ``[licenses] allow`` set equals ``packaging/about.toml``'s
       ``accepted`` set. Neither tool reads the other's file and cargo-deny
       offers no include, so the list exists twice and must be kept equal by
       this rule.

The path rule reads the Git index (``git ls-files``), not the worktree, so an
ignored local directory cannot fail it merely for existing; in a CI checkout the
index is the checked-out commit. It describes what lands on ``main``, so CI runs
it for pushes to ``main`` and PRs into it, not on the ``prototype/*`` branches
that carry ``app/prototype/`` content by design (``.github/workflows/check.yml``).
``--rules`` exists for that split. The eol rule reads both sides, because the
checkout itself is what it protects. This script is exported, so it holds the
four invariants ``CONTRIBUTING.md`` documents to a public reader and no rule
that names a private path.

The ordinary run proves the rules accept the current tree. ``--self-test``
feeds each rule a controlled invalid input (and a clean one) and requires the
rule to reject and accept respectively, so a rule that has gone insensitive
fails too. Neither mode builds anything or uses a third-party framework.

Exit codes: 0 every invariant holds, 1 an invariant is violated or a self-test
case fails, 2 the checker itself could not run (Git or Cargo unusable, Cargo
tree output that no longer parses, or ``cargo tree --locked`` refusing a stale
``Cargo.lock``).
"""

from __future__ import annotations

import os
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Sequence

sys.path.insert(0, str(Path(__file__).resolve().parent))

import invariants_eol  # noqa: E402
import license_policy  # noqa: E402
from invariants_core import (  # noqa: E402
    CheckerError,
    RuleOutcome,
    Violation,
    expect_checker_error,
    run,
    run_text,
)

REPO_ROOT = Path(__file__).resolve().parent.parent

ENGINE_CRATE = "unmei-engine"
ENGINE_INVARIANT = (
    f"{ENGINE_CRATE} pulls no Bevy package in any feature or target configuration"
)
ENGINE_RULE = "engine"
PATH_RULES = "paths"
EOL_RULE = invariants_eol.EOL_RULE
LICENSE_RULE = license_policy.RULE
RULE_SELECTIONS = (ENGINE_RULE, PATH_RULES, EOL_RULE, LICENSE_RULE, "all")
USAGE = (
    "usage: check-invariants.py "
    "[--rules engine|paths|eol|licenses|all] [--root <dir>] | [--self-test]"
)

# The two feature selections that bound the rest: Cargo's features are
# additive, so enabling one only ever adds dependencies. Each selection is
# resolved for every target, so a target-specific edge cannot hide behind the
# runner's own platform.
FEATURE_CONFIGS: tuple[tuple[str, ...], ...] = (
    ("--no-default-features",),
    ("--all-features",),
)

# `bevy`, and the ecosystem crates named after it (`bevy_ecs`, `bevy_kira_audio`,
# `bevy-inspector-egui`): all of them put the Bevy runtime world in the engine.
BEVY_PACKAGE = re.compile(r"^bevy(?:$|[-_])")


@dataclass(frozen=True)
class EngineTree:
    """The packages `cargo tree --locked -p unmei-engine` resolved for one selection."""

    flags: tuple[str, ...]
    packages: Sequence[str]

    @property
    def label(self) -> str:
        return " ".join(self.flags)


@dataclass(frozen=True)
class PathInvariant:
    prefix: str
    invariant: str


PATH_INVARIANTS: tuple[PathInvariant, ...] = (
    PathInvariant(
        prefix="app/prototype/",
        invariant=(
            "no tracked file under app/prototype/ "
            "(a prototype stays on its prototype/<issue>-<name> branch)"
        ),
    ),
)


def cargo_tree_command(flags: Sequence[str]) -> list[str]:
    """The `cargo tree` invocation the engine rule resolves, as a list."""
    return [
        "cargo",
        "tree",
        # Resolve strictly against the committed `Cargo.lock`. This rule runs
        # before the locked clippy and test steps; unlocked, it would repair a
        # stale lockfile and hide the breakage from them.
        "--locked",
        "-p",
        ENGINE_CRATE,
        "--edges",
        "normal,build,dev",
        "--prefix",
        "none",
        "--format",
        "{p}",
        "--color",
        "never",
        "--target",
        "all",
        *flags,
    ]


def parse_cargo_tree(stdout: str) -> list[str]:
    """Package names from `cargo tree --prefix none --format "{p}"`.

    Each line is `<name> v<semver>` plus a dedupe marker, a package kind or a
    path (`serde v1.0.229 (*)`, `serde_derive v1.0.229 (proc-macro)`,
    `unmei-engine v0.1.0 (/repo/engine)`). A line without a version is Cargo
    output this checker does not understand: raise instead of silently reading a
    partial graph and reporting a pass.
    """
    names: list[str] = []
    for line in stdout.splitlines():
        line = line.strip()
        if not line:
            continue
        name, _, rest = line.partition(" ")
        if not rest.startswith("v"):
            raise CheckerError(
                f"unexpected `cargo tree` line, cannot read the engine graph: {line!r}"
            )
        names.append(name)
    return names


def check_engine_bevy_free(trees: Sequence[EngineTree]) -> RuleOutcome:
    """Rule 1: no Bevy package is reachable from `unmei-engine`."""
    violations: list[Violation] = []
    for tree in trees:
        if ENGINE_CRATE not in tree.packages:
            raise CheckerError(
                f"`cargo tree` output for {tree.label} does not list {ENGINE_CRATE}; "
                "cannot decide the Bevy-free invariant"
            )
        offending = sorted({name for name in tree.packages if BEVY_PACKAGE.match(name)})
        if offending:
            violations.append(
                Violation(
                    offenders=tuple(f"{name} ({tree.label})" for name in offending),
                    reproduce=" ".join(cargo_tree_command(tree.flags)),
                )
            )
    return RuleOutcome(ENGINE_INVARIANT, tuple(violations))


def check_no_tracked_paths(
    tracked: Sequence[str], invariant: PathInvariant
) -> RuleOutcome:
    """Rule 2: no tracked file under the invariant's path prefix."""
    offenders = tuple(path for path in tracked if path.startswith(invariant.prefix))
    if not offenders:
        return RuleOutcome(invariant.invariant)
    return RuleOutcome(
        invariant.invariant,
        (
            Violation(
                offenders=offenders,
                reproduce=f"git ls-files -z -- {invariant.prefix}",
            ),
        ),
    )


def license_outcome(mismatches: Sequence[license_policy.Mismatch]) -> RuleOutcome:
    """Rule 4: both files accept the same licenses, as a set."""
    if not mismatches:
        return RuleOutcome(license_policy.INVARIANT)
    violations = tuple(
        Violation(
            offenders=tuple(
                f"{license} (only in {mismatch.source})" for license in mismatch.licenses
            ),
            reproduce=license_policy.REPRODUCE,
        )
        for mismatch in mismatches
    )
    return RuleOutcome(license_policy.INVARIANT, violations)


def gather_tracked_paths(root: Path) -> list[str]:
    """Every tracked path, repository-relative and NUL-separated.

    The Git index is what is versioned; the worktree may hold ignored scratch
    that must not fail these rules.
    """
    raw = run(["git", "ls-files", "-z"], root)
    return [os.fsdecode(path) for path in raw.split(b"\x00") if path]


def check(root: Path, rules: str = "all") -> list[RuleOutcome]:
    """Run the selected invariants against the tree checked out at `root`."""
    outcomes: list[RuleOutcome] = []

    if rules in ("all", ENGINE_RULE):
        trees = []
        for flags in FEATURE_CONFIGS:
            stdout = run_text(cargo_tree_command(flags), root)
            trees.append(EngineTree(flags, parse_cargo_tree(stdout)))
        outcomes.append(check_engine_bevy_free(trees))

    if rules in ("all", PATH_RULES):
        tracked = gather_tracked_paths(root)
        if not tracked:
            raise CheckerError(
                "git ls-files listed no tracked paths; refusing to pass the path "
                "invariants vacuously"
            )
        outcomes.extend(
            check_no_tracked_paths(tracked, invariant) for invariant in PATH_INVARIANTS
        )

    if rules in ("all", EOL_RULE):
        outcomes.append(invariants_eol.check(root))

    if rules in ("all", LICENSE_RULE):
        try:
            outcomes.append(
                license_outcome(
                    license_policy.differences(*license_policy.read_lists(root))
                )
            )
        except license_policy.LicensePolicyError as exc:
            raise CheckerError(str(exc)) from exc

    return outcomes


def exit_code(outcomes: Sequence[RuleOutcome]) -> int:
    """0 when every rule held, 1 when any rule was violated."""
    return 1 if any(outcome.violations for outcome in outcomes) else 0


def report(outcomes: Sequence[RuleOutcome], stream=sys.stdout) -> int:
    """Print one line per rule; return the process exit code."""
    for outcome in outcomes:
        if outcome.violations:
            print(f"FAIL: {outcome.invariant}", file=stream)
            for violation in outcome.violations:
                print(f"  offending: {', '.join(violation.offenders)}", file=stream)
                print(f"  reproduce: {violation.reproduce}", file=stream)
        else:
            print(f"ok: {outcome.invariant}", file=stream)
    code = exit_code(outcomes)
    noun = _invariants(len(outcomes))
    if code:
        violated = sum(1 for outcome in outcomes if outcome.violations)
        print(f"\n{violated} of {len(outcomes)} {noun} violated", file=stream)
    else:
        print(f"\n{len(outcomes)} {noun} checked, no violations", file=stream)
    return code


def _invariants(count: int) -> str:
    return "invariant" if count == 1 else "invariants"


def _self_test_cases() -> list[tuple[str, Callable[[], None]]]:
    """Controlled cases: each rule must reject an invalid input, accept a clean one."""

    cleantree = EngineTree(
        flags=("--all-features",),
        packages=["unmei-engine", "ron", "serde", "tokio"],
    )

    def engine_rule_accepts_bevy_free_graph() -> None:
        outcome = check_engine_bevy_free([cleantree])
        assert not outcome.violations, outcome.violations
        assert outcome.invariant == ENGINE_INVARIANT, outcome.invariant

    def engine_rule_rejects_every_bevy_name_shape() -> None:
        tree = EngineTree(
            flags=("--all-features",),
            packages=[
                "unmei-engine",
                "ron",
                "bevy",
                "bevy_ecs",
                "bevy_kira_audio",
                "bevy-inspector-egui",
                "bevyfoo",
                "unbevy",
            ],
        )
        outcome = check_engine_bevy_free([tree])
        assert exit_code([outcome]) == 1, "a Bevy package must fail the rule"
        assert len(outcome.violations) == 1, outcome.violations
        violation = outcome.violations[0]
        offenders = sorted(name.split(" ")[0] for name in violation.offenders)
        assert offenders == [
            "bevy",
            "bevy-inspector-egui",
            "bevy_ecs",
            "bevy_kira_audio",
        ], offenders
        assert tree.label in violation.offenders[0], violation.offenders[0]
        assert "cargo tree" in violation.reproduce

    def engine_rule_refuses_a_tree_without_the_engine() -> None:
        expect_checker_error(
            "a tree missing unmei-engine",
            lambda: check_engine_bevy_free(
                [EngineTree(("--no-default-features",), ["ron"])]
            ),
        )

    def cargo_tree_parser_reads_package_lines() -> None:
        stdout = (
            "unmei-engine v0.1.0 (C:/repo/engine)\n"
            "ron v0.10.1\n"
            "serde v1.0.229 (*)\n"
            "serde_derive v1.0.229 (proc-macro)\n"
            "\n"
        )
        assert parse_cargo_tree(stdout) == [
            "unmei-engine",
            "ron",
            "serde",
            "serde_derive",
        ]

    def cargo_tree_parser_refuses_unreadable_lines() -> None:
        expect_checker_error(
            "an unreadable cargo tree line",
            lambda: parse_cargo_tree('ron feature "default"\n'),
        )

    def path_rule_accepts_clean_paths() -> None:
        clean = [
            "Cargo.toml",
            "app/src/main.rs",
            "app/prototypes/notes.md",
            "app/prototype",
        ]
        for invariant in PATH_INVARIANTS:
            outcome = check_no_tracked_paths(clean, invariant)
            assert not outcome.violations, (invariant.prefix, outcome.violations)

    def prototype_rule_rejects_a_tracked_prototype() -> None:
        tracked = ["Cargo.toml", "app/prototype/shots/menu.png"]
        outcome = check_no_tracked_paths(tracked, PATH_INVARIANTS[0])
        assert outcome.violations[0].offenders == ("app/prototype/shots/menu.png",)
        assert exit_code([outcome]) == 1

    def license_rule_reports_a_mismatch_as_a_violation() -> None:
        mismatches = license_policy.differences(
            frozenset({"Apache-2.0", "OpenSSL"}), frozenset({"Apache-2.0"})
        )
        outcome = license_outcome(mismatches)
        assert exit_code([outcome]) == 1, outcome
        assert outcome.invariant == license_policy.INVARIANT, outcome.invariant
        assert outcome.violations[0].offenders == (
            f"OpenSSL (only in {license_policy.ABOUT_TOML})",
        ), outcome.violations[0].offenders

    return [
        *license_policy.self_test_cases(),
        ("engine rule accepts a Bevy-free graph", engine_rule_accepts_bevy_free_graph),
        (
            "engine rule rejects every Bevy name shape and passes near-misses",
            engine_rule_rejects_every_bevy_name_shape,
        ),
        (
            "engine rule refuses a tree without unmei-engine",
            engine_rule_refuses_a_tree_without_the_engine,
        ),
        ("cargo tree parser reads package lines", cargo_tree_parser_reads_package_lines),
        (
            "cargo tree parser refuses unreadable lines",
            cargo_tree_parser_refuses_unreadable_lines,
        ),
        ("path rule accepts clean paths", path_rule_accepts_clean_paths),
        (
            "prototype rule rejects a tracked prototype and names it",
            prototype_rule_rejects_a_tracked_prototype,
        ),
        *invariants_eol.self_test_cases(),
    ]


def run_self_test() -> int:
    """Prove each rule's sensitivity; return the process exit code."""
    failures = 0
    cases = _self_test_cases()
    for name, case in cases:
        try:
            case()
        except Exception as exc:  # a broken case must fail the self-test too
            failures += 1
            print(f"self-test FAIL: {name}\n  {type(exc).__name__}: {exc}")
        else:
            print(f"self-test ok: {name}")
    if failures:
        print(f"\n{failures} of {len(cases)} self-test cases failed.")
        return 1
    print(f"\nAll {len(cases)} self-test cases passed.")
    return 0


def parse_arguments(argv: Sequence[str]) -> tuple[str, Path] | None:
    """The selected rules and tree, or None when the arguments are not understood."""
    rules = "all"
    root = REPO_ROOT
    remaining = list(argv)
    while remaining:
        flag = remaining.pop(0)
        if flag == "--rules" and remaining and remaining[0] in RULE_SELECTIONS:
            rules = remaining.pop(0)
        elif flag == "--root" and remaining:
            root = Path(remaining.pop(0)).resolve()
        else:
            return None
    return rules, root


def main(argv: Sequence[str]) -> int:
    args = list(argv)
    if args in (["-h"], ["--help"]):
        print(__doc__.strip())
        return 0
    if args == ["--self-test"]:
        return run_self_test()

    parsed = parse_arguments(args)
    if parsed is None:
        print(USAGE, file=sys.stderr)
        return 2
    rules, root = parsed

    try:
        outcomes = check(root, rules)
    except CheckerError as exc:
        print(f"error: checker could not run: {exc}", file=sys.stderr)
        return 2
    return report(outcomes)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
