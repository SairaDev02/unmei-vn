"""Shared types and process helpers for the repository invariant rules.

`scripts/check-invariants.py` is the CLI and the self-test harness; a rule
group that carries enough detail lives in its own sibling module, so no single
authored file grows past the staged-token limit (`scripts/check-staged-token-limit.py`).
"""

from __future__ import annotations

import subprocess
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable, Sequence


class CheckerError(RuntimeError):
    """The checker could not run, so it cannot report a pass."""


@dataclass(frozen=True)
class Violation:
    """The offending dependencies or paths, with a command that shows them."""

    offenders: Sequence[str]
    reproduce: str


@dataclass(frozen=True)
class RuleOutcome:
    """One invariant's verdict: its summary plus every violation found."""

    invariant: str
    violations: Sequence[Violation] = field(default_factory=tuple)


def run(command: Sequence[str], root: Path) -> bytes:
    """Run `command` in `root`, or raise CheckerError with its stderr."""
    try:
        completed = subprocess.run(
            list(command),
            cwd=root,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except OSError as exc:
        raise CheckerError(f"cannot run {command[0]!r}: {exc}") from exc
    if completed.returncode != 0:
        stderr = completed.stderr.decode("utf-8", "replace").strip()
        raise CheckerError(
            f"{' '.join(command)} failed (exit {completed.returncode}): {stderr}"
        )
    return completed.stdout


def run_text(command: Sequence[str], root: Path) -> str:
    """`run`, decoded as text."""
    return run(command, root).decode("utf-8", "replace")


def expect_checker_error(what: str, action: Callable[[], object]) -> None:
    """Assert the checker refuses `action`, rather than reading it as a pass."""
    try:
        action()
    except CheckerError:
        return
    raise AssertionError(f"{what} must raise CheckerError")
