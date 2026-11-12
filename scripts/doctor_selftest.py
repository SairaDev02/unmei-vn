"""The doctor's controlled environment: fakes, canned results and case helpers.

``--self-test`` builds a ``FakeProbe`` answering from these canned results, so
every classification is proven without running a real tool.
"""

from __future__ import annotations

from pathlib import Path
from typing import Callable, Sequence

from doctor_checks import checks_for
from doctor_probe import Check, CommandResult, DoctorError, Probe, ToolchainPin

# A real directory a probe may run in (the canned root does not exist).
REAL_DIR = Path(__file__).resolve().parent


SELF_TEST_BINARIES = frozenset({"rustc", "cargo", "git", "pkg-config"})
SELF_TEST_PIN = ToolchainPin("1.98.1", ("rustfmt", "clippy"))

HEALTHY_RESULTS: dict[tuple[str, ...], CommandResult] = {
    ("rustc", "-vV"): CommandResult(
        0,
        "rustc 1.98.1 (abc 2026-09-01)\n"
        "binary: rustc\n"
        "host: x86_64-unknown-linux-gnu\n"
        "release: 1.98.1\n",
    ),
    ("cargo", "--version"): CommandResult(0, "cargo 1.98.1 (abc 2026-09-01)\n"),
    ("cargo", "fmt", "--version"): CommandResult(0, "rustfmt 1.9.0-stable\n"),
    ("cargo", "clippy", "--version"): CommandResult(0, "clippy 0.1.98\n"),
    ("git", "config", "--get", "core.hooksPath"): CommandResult(0, ".githooks\n"),
    ("pkg-config", "--exists", "alsa"): CommandResult(0),
    ("pkg-config", "--exists", "libudev"): CommandResult(0),
    ("pkg-config", "--exists", "wayland-client"): CommandResult(0),
    ("pkg-config", "--exists", "xkbcommon"): CommandResult(0),
    ("pkg-config", "--exists", "fontconfig"): CommandResult(0),
}


class FakeProbe(Probe):
    """A probe backed by a fixed binary set and canned command results."""

    def __init__(
        self,
        binaries: frozenset[str],
        results: dict[tuple[str, ...], CommandResult],
    ) -> None:
        self._binaries = binaries
        self._results = results

    def which(self, name: str) -> str | None:
        return f"/fake/bin/{name}" if name in self._binaries else None

    def run(
        self, command: Sequence[str], cwd: Path, timeout: float | None = None
    ) -> CommandResult:
        key = tuple(command)
        if key not in self._results:
            raise DoctorError(f"self-test has no fake result for {key!r}")
        return self._results[key]


def make_probe(
    binaries: frozenset[str] = SELF_TEST_BINARIES,
    results: dict[tuple[str, ...], CommandResult] | None = None,
) -> FakeProbe:
    merged = dict(HEALTHY_RESULTS)
    if results:
        merged.update(results)
    return FakeProbe(binaries, merged)


SELF_TEST_ROOT = Path("/fake/unmei")


def check_named(checks: Sequence[Check], name: str) -> Check:
    return next(check for check in checks if check.name == name)


def diagnose_checks(probe: Probe, platform_name: str = "linux") -> list[Check]:
    return checks_for(probe, SELF_TEST_ROOT, platform_name, SELF_TEST_PIN)


def expect_doctor_error(what: str, action: Callable[[], object]) -> None:
    """Assert the doctor refuses ``action`` instead of reporting a pass."""
    try:
        action()
    except DoctorError:
        return
    raise AssertionError(f"{what} must raise DoctorError")
