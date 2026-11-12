"""The environment the doctor reads, and the toolchain pin it compares against.

The seam exists so ``--self-test`` can prove each check's classification
without touching the real machine: a fake ``Probe`` supplies the binaries and
command results. Standard library only.
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import Sequence

TOOLCHAIN_FILE = "rust-toolchain.toml"


try:  # Python 3.11+; the fallback keeps older interpreters working.
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - depends on the interpreter
    tomllib = None  # type: ignore[assignment]


class DoctorError(RuntimeError):
    """The doctor could not gather the facts a check needs, so it cannot report."""


class Status(Enum):
    PASS = "PASS"
    WARN = "WARN"
    FAIL = "FAIL"
    SKIP = "SKIP"


@dataclass(frozen=True)
class Check:
    """One prerequisite's verdict, with the command that would fix it."""

    name: str
    status: Status
    message: str
    suggestion: str | None = None


@dataclass(frozen=True)
class CommandResult:
    returncode: int
    stdout: str = ""
    stderr: str = ""


@dataclass(frozen=True)
class ToolchainPin:
    """The ``[toolchain]`` selection ``rust-toolchain.toml`` commits to."""

    channel: str
    components: tuple[str, ...] = ()


# Return code a probe reports for a command that hung. Real processes never
# exit with it, so a timeout stays distinguishable from a tool's own failure.
TIMED_OUT = -1000


def probe_env() -> dict[str, str]:
    """The environment every probe runs under: nothing installed or downloaded."""
    return dict(os.environ, RUSTUP_AUTO_INSTALL="0")


class Probe:
    """The environment the doctor reads. A fake overrides it in ``--self-test``."""

    def which(self, name: str) -> str | None:
        return shutil.which(name)

    def run(
        self, command: Sequence[str], cwd: Path, timeout: float | None = None
    ) -> CommandResult:
        # Resolve the program through the same lookup the checks use, because
        # on Windows a bare name may be a ``.CMD`` shim that only the full path
        # can launch.
        program = self.which(command[0]) or command[0]
        try:
            completed = subprocess.run(
                [program, *command[1:]],
                cwd=cwd,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                env=probe_env(),
                timeout=timeout,
            )
        except subprocess.TimeoutExpired:
            return CommandResult(TIMED_OUT, "", f"timed out after {timeout:g}s")
        except OSError as exc:
            raise DoctorError(f"cannot run {command[0]!r}: {exc}") from exc
        return CommandResult(
            completed.returncode,
            completed.stdout.decode("utf-8", "replace"),
            completed.stderr.decode("utf-8", "replace"),
        )


_CHANNEL_RE = re.compile(r'^\s*channel\s*=\s*"([^"]+)"', re.MULTILINE)
_COMPONENTS_RE = re.compile(r"^\s*components\s*=\s*\[([^\]]*)\]", re.MULTILINE)
_QUOTED_RE = re.compile(r'"([^"]*)"')


def parse_toolchain_pin(text: str) -> ToolchainPin:
    """Read ``channel`` and ``components`` from a ``rust-toolchain.toml`` body."""
    if tomllib is not None:
        try:
            data = tomllib.loads(text)
        except tomllib.TOMLDecodeError as exc:
            raise DoctorError(f"{TOOLCHAIN_FILE} is not valid TOML: {exc}") from exc
        section = data.get("toolchain")
        if not isinstance(section, dict):
            raise DoctorError(f"{TOOLCHAIN_FILE} has no [toolchain] table")
        channel = str(section.get("channel", "")).strip()
        components = tuple(str(item) for item in section.get("components", ()))
    else:  # pragma: no cover - only on a Python without tomllib
        channel, components = parse_toolchain_pin_fallback(text)
    if not channel:
        raise DoctorError(f"{TOOLCHAIN_FILE} has no [toolchain].channel pin")
    return ToolchainPin(channel, components)


def parse_toolchain_pin_fallback(text: str) -> tuple[str, tuple[str, ...]]:
    """Read the two values the doctor needs without a TOML parser.

    The repository's own pin is a two-key table; a line scanner is enough to
    keep the doctor runnable on Python versions that predate ``tomllib``.
    """
    channel_match = _CHANNEL_RE.search(text)
    channel = channel_match.group(1).strip() if channel_match else ""
    components_match = _COMPONENTS_RE.search(text)
    components = (
        tuple(_QUOTED_RE.findall(components_match.group(1)))
        if components_match
        else ()
    )
    return channel, components


def read_toolchain_pin(root: Path) -> ToolchainPin:
    path = root / TOOLCHAIN_FILE
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as exc:
        raise DoctorError(f"cannot read {path}: {exc}") from exc
    return parse_toolchain_pin(text)


_RELEASE_RE = re.compile(r"^release:\s*(\S+)\s*$", re.MULTILINE)
_VERSION_RE = re.compile(r"^\d+(?:\.\d+)*$")


def first_line(text: str) -> str:
    for line in text.splitlines():
        line = line.strip()
        if line:
            return line
    return ""


def truncate(text: str, limit: int = 160) -> str:
    """One line is enough for a report; keep a runaway summary out of it."""
    return text if len(text) <= limit else text[: limit - 3] + "..."


def parse_rustc_release(verbose: str) -> str:
    """The ``release:`` value from ``rustc -vV``; raise if it is absent."""
    match = _RELEASE_RE.search(verbose)
    if not match:
        raise DoctorError("`rustc -vV` output has no `release:` line")
    return match.group(1)


def pin_agrees(channel: str, release: str) -> bool | None:
    """Whether ``release`` matches the pin; ``None`` when it cannot be compared.

    A plain version pin (``1.98.1``) is compared with the running compiler. A
    named channel (``stable``, ``nightly-2025-01-01``) cannot be checked from
    the compiler alone, so the caller reports it as unverified rather than a
    failure.
    """
    if _VERSION_RE.match(channel):
        return release == channel or release.startswith(channel + ".")
    return None
