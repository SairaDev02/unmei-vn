#!/usr/bin/env python3
"""Report whether this machine can build, test and lint Unmei.

Run from anywhere; the repository root is derived from this file's location
rather than the current working directory.

    python3 scripts/doctor.py              # report, exit 0 or 1
    python3 scripts/doctor.py --self-test  # prove the checks classify correctly

The checks, the environment seam and the self-test cases live in sibling
modules (``doctor_probe``, ``doctor_checks``, ``doctor_selftest``,
``doctor_cases_*``) so each file stays inside the repository's 5,000-token
budget; this file stays the only entry point.

The doctor is **read-only** in the repository: it installs nothing, updates no
cache, changes no Git configuration, writes no tracked file, and runs no
rebuild, init or query command. Every command it runs is a version probe or a
read-only configuration query, so it is safe to run before or inside any other
work. Probes run with ``RUSTUP_AUTO_INSTALL=0``, so a missing pinned toolchain
is reported and never downloaded; nothing inside the repository changes.

Checks
------
rustc, cargo
    ``rust-toolchain.toml`` is the source of truth. ``rustc -vV`` is run from
    the repository root, so rustup resolves the same toolchain a build would,
    and its ``release`` is compared with the pinned ``channel``. A missing
    binary or a mismatched release is a blocking failure.
rustfmt, clippy
    The components the pin asks for, probed with ``cargo fmt --version`` and
    ``cargo clippy --version`` (neither compiles the workspace). Missing
    components are warnings: a valid Rust checkout still passes.
git hooks
    Whether ``core.hooksPath`` resolves to the committed ``.githooks``
    directory, naming any other configured path. The Git config is only read.
linux build libs
    Best-effort check of the native libraries CI installs
    (``.github/workflows/check.yml``) through ``pkg-config``. Linux only;
    other platforms report the check as not applicable.

Exit codes
----------
0   No blocking problem. Warnings and skipped checks do not fail the run.
1   A blocking prerequisite is missing or mismatched (rustc, cargo, pin).
2   The doctor itself could not run (unreadable pin, unusable probe).
"""

from __future__ import annotations

import sys
from pathlib import Path
from typing import Callable, Sequence

from doctor_checks import diagnose, exit_code
from doctor_probe import Check, DoctorError, Probe, Status

REPO_ROOT = Path(__file__).resolve().parent.parent
USAGE = "usage: doctor.py [--self-test] | [-h|--help]"


def report(checks: Sequence[Check], root: Path, stream=sys.stdout) -> int:
    """Print one line per check plus its fix; return the process exit code."""
    width = max(len(check.name) for check in checks)
    print(f"Unmei environment doctor (repo root: {root})", file=stream)
    print(file=stream)
    for check in checks:
        print(
            f"{check.status.value:<4}  {check.name:<{width}}  {check.message}",
            file=stream,
        )
        if check.suggestion and check.status in (Status.WARN, Status.FAIL):
            print(f"{'':<8}{check.suggestion}", file=stream)

    passed = sum(1 for check in checks if check.status is Status.PASS)
    warnings = sum(1 for check in checks if check.status is Status.WARN)
    failures = sum(1 for check in checks if check.status is Status.FAIL)
    skipped = sum(1 for check in checks if check.status is Status.SKIP)

    print(file=stream)
    print(
        f"{passed} passed, {warnings} warning(s), {skipped} skipped, "
        f"{failures} blocking failure(s)",
        file=stream,
    )
    if failures:
        print("Fix the FAIL lines above before building or testing.", file=stream)
    return exit_code(checks)


def _self_test_cases() -> list[tuple[str, Callable[[], None]]]:
    """Controlled cases: each situation must classify PASS/WARN/FAIL/SKIP."""
    # Imported here so the case modules can import the fakes they run against.
    from doctor_cases_environment import cases as environment_cases
    from doctor_cases_toolchain import cases as toolchain_cases

    return toolchain_cases() + environment_cases()


def run_self_test() -> int:
    """Prove each check's classification; return the process exit code."""
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


def main(argv: Sequence[str]) -> int:
    args = list(argv)
    if args in (["-h"], ["--help"]):
        print(__doc__.strip())
        return 0
    if args == ["--self-test"]:
        return run_self_test()
    if args:
        print(USAGE, file=sys.stderr)
        return 2

    try:
        checks = diagnose(Probe(), REPO_ROOT, sys.platform)
    except DoctorError as exc:
        print(f"error: doctor could not run: {exc}", file=sys.stderr)
        return 2
    return report(checks, REPO_ROOT)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
