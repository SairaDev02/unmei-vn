"""Self-test cases for the probe environment, the Git hooks and the native libraries."""

from __future__ import annotations

import sys
from typing import Callable

from doctor_checks import exit_code
from doctor_probe import TIMED_OUT, CommandResult, Probe, Status, probe_env
from doctor_selftest import (
    REAL_DIR,
    check_named,
    diagnose_checks,
    make_probe,
)


def probes_install_nothing() -> None:
    env = probe_env()
    assert env["RUSTUP_AUTO_INSTALL"] == "0", env


def a_hung_command_times_out_instead_of_hanging_the_doctor() -> None:
    result = Probe().run(
        [sys.executable, "-c", "import time; time.sleep(30)"],
        REAL_DIR,
        timeout=0.2,
    )
    assert result.returncode == TIMED_OUT, result
    assert "timed out" in result.stderr, result


def unset_hooks_warn_with_the_fix() -> None:
    checks = diagnose_checks(
        make_probe(
            results={
                ("git", "config", "--get", "core.hooksPath"): CommandResult(1)
            }
        )
    )
    assert exit_code(checks) == 0, checks
    hooks = check_named(checks, "git hooks")
    assert hooks.status is Status.WARN, hooks
    assert hooks.suggestion == "git config core.hooksPath .githooks", hooks


def alternative_hooks_path_is_named() -> None:
    checks = diagnose_checks(
        make_probe(
            results={
                ("git", "config", "--get", "core.hooksPath"): CommandResult(
                    0, "/tmp/other-hooks\n"
                )
            }
        )
    )
    hooks = check_named(checks, "git hooks")
    assert hooks.status is Status.WARN, hooks
    assert "other-hooks" in hooks.message, hooks


def hooks_path_matching_the_committed_dir_passes() -> None:
    checks = diagnose_checks(make_probe())
    assert check_named(checks, "git hooks").status is Status.PASS, checks


def non_linux_marks_the_library_check_unsupported() -> None:
    checks = diagnose_checks(make_probe(), platform_name="win32")
    assert exit_code(checks) == 0, checks
    deps = check_named(checks, "linux build libs")
    assert deps.status is Status.SKIP, deps
    assert "win32" in deps.message, deps


def linux_missing_library_warns_with_the_package() -> None:
    checks = diagnose_checks(
        make_probe(
            results={("pkg-config", "--exists", "fontconfig"): CommandResult(1)}
        )
    )
    assert exit_code(checks) == 0, checks
    deps = check_named(checks, "linux build libs")
    assert deps.status is Status.WARN, deps
    assert "libfontconfig1-dev" in deps.message, deps
    assert deps.suggestion is not None, deps
    assert "libfontconfig1-dev" in deps.suggestion, deps


def cases() -> list[tuple[str, Callable[[], None]]]:
    """(label, case) pairs, in report order."""
    return [
        ("probes install nothing", probes_install_nothing),
        ("a hung command times out", a_hung_command_times_out_instead_of_hanging_the_doctor),
        ("unset hooks warn with the fix", unset_hooks_warn_with_the_fix),
        ("alternative hooks path is named", alternative_hooks_path_is_named),
        ("hooks path matching the committed dir passes", hooks_path_matching_the_committed_dir_passes),
        ("non-Linux marks the library check unsupported", non_linux_marks_the_library_check_unsupported),
        ("Linux missing library warns with the package", linux_missing_library_warns_with_the_package),
    ]
