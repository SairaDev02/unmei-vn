"""The doctor's prerequisite checks, in report order.

Each ``check_*`` function returns one ``Check``; ``checks_for`` assembles them
from an already-read pin (the seam ``--self-test`` uses) and ``diagnose`` reads
the pin first. Nothing here mutates the machine or the repository.
"""

from __future__ import annotations

from pathlib import Path
from typing import Sequence

from doctor_probe import (
    TOOLCHAIN_FILE,
    Check,
    Probe,
    Status,
    ToolchainPin,
    first_line,
    parse_rustc_release,
    pin_agrees,
    read_toolchain_pin,
)

HOOKS_DIR = ".githooks"


# Components the pin asks for, and the probe that shows one is installed.
# Neither command compiles the workspace.
COMPONENT_COMMANDS: dict[str, tuple[tuple[str, ...], str]] = {
    "rustfmt": (("cargo", "fmt", "--version"), "cargo fmt --check"),
    "clippy": (("cargo", "clippy", "--version"), "cargo clippy"),
}

# Debian package -> pkg-config module, matching .github/workflows/check.yml.
LINUX_BUILD_PACKAGES: tuple[tuple[str, str], ...] = (
    ("libasound2-dev", "alsa"),
    ("libudev-dev", "libudev"),
    ("libwayland-dev", "wayland-client"),
    ("libxkbcommon-dev", "xkbcommon"),
    ("libfontconfig1-dev", "fontconfig"),
)


def check_rust_toolchain(
    probe: Probe, root: Path, pin: ToolchainPin
) -> list[Check]:
    """Rule: the pinned rustc and cargo are installed and agree with the pin."""
    checks: list[Check] = []
    install_hint = "install Rust with rustup: https://rustup.rs/"

    if probe.which("rustc") is None:
        checks.append(Check("rustc", Status.FAIL, "rustc is not on PATH", install_hint))
    else:
        version = probe.run(["rustc", "-vV"], root)
        if version.returncode != 0:
            detail = first_line(version.stderr) or first_line(version.stdout) or "no output"
            checks.append(
                Check(
                    "rustc",
                    Status.FAIL,
                    f"the pinned toolchain is not usable: {detail}",
                    f"rustup toolchain install {pin.channel}",
                )
            )
        else:
            release = parse_rustc_release(version.stdout)
            agrees = pin_agrees(pin.channel, release)
            if agrees is False:
                checks.append(
                    Check(
                        "rustc",
                        Status.FAIL,
                        f"rustc {release} does not match the "
                        f"{TOOLCHAIN_FILE} pin ({pin.channel})",
                        f"rustup toolchain install {pin.channel}",
                    )
                )
            elif agrees is None:
                checks.append(
                    Check(
                        "rustc",
                        Status.WARN,
                        f"rustc {release}; a channel pin ({pin.channel}) cannot be "
                        "verified from the compiler alone",
                        "rustup show active-toolchain",
                    )
                )
            else:
                checks.append(
                    Check(
                        "rustc",
                        Status.PASS,
                        f"rustc {release} matches the {TOOLCHAIN_FILE} pin",
                    )
                )

    if probe.which("cargo") is None:
        checks.append(Check("cargo", Status.FAIL, "cargo is not on PATH", install_hint))
    else:
        cargo = probe.run(["cargo", "--version"], root)
        if cargo.returncode == 0:
            checks.append(
                Check("cargo", Status.PASS, first_line(cargo.stdout) or "cargo available")
            )
        else:
            detail = first_line(cargo.stderr) or first_line(cargo.stdout) or "no output"
            checks.append(
                Check(
                    "cargo",
                    Status.FAIL,
                    f"cargo did not run under the pinned toolchain: {detail}",
                    f"rustup toolchain install {pin.channel}",
                )
            )
    return checks


def check_components(probe: Probe, root: Path, pin: ToolchainPin) -> list[Check]:
    """Rule: the pinned rustfmt and clippy components are installed.

    Missing components warn rather than fail: the checkout can still build and
    test, and the CI gate, not this report, decides a contribution.
    """
    if probe.which("cargo") is None:
        return []
    wanted = [name for name in pin.components if name in COMPONENT_COMMANDS]
    for name in ("rustfmt", "clippy"):
        if name not in wanted:
            wanted.append(name)

    checks: list[Check] = []
    for name in wanted:
        command, purpose = COMPONENT_COMMANDS[name]
        result = probe.run(command, root)
        if result.returncode == 0:
            checks.append(
                Check(name, Status.PASS, first_line(result.stdout) or f"{name} available")
            )
        else:
            detail = first_line(result.stderr) or first_line(result.stdout) or "not installed"
            checks.append(
                Check(
                    name,
                    Status.WARN,
                    f"{name} unavailable ({detail}); the {purpose} gate would fail",
                    f"rustup component add {name}",
                )
            )
    return checks


def check_git_hooks(probe: Probe, root: Path) -> Check:
    """Rule: ``core.hooksPath`` resolves to the committed ``.githooks`` directory."""
    if probe.which("git") is None:
        return Check(
            "git hooks",
            Status.WARN,
            "git is not on PATH, so core.hooksPath cannot be inspected",
            "install Git: https://git-scm.com/downloads",
        )

    config = probe.run(["git", "config", "--get", "core.hooksPath"], root)
    configured = config.stdout.strip()
    if config.returncode != 0 or not configured:
        return Check(
            "git hooks",
            Status.WARN,
            "core.hooksPath is not set, so the committed pre-commit hook never runs",
            f"git config core.hooksPath {HOOKS_DIR}",
        )

    expected = (root / HOOKS_DIR).resolve()
    resolved = Path(configured)
    if not resolved.is_absolute():
        resolved = root / resolved
    if resolved.resolve() == expected:
        return Check("git hooks", Status.PASS, f"core.hooksPath -> {configured}")
    return Check(
        "git hooks",
        Status.WARN,
        f"core.hooksPath is {configured}, not the committed {HOOKS_DIR} directory",
        f"git config core.hooksPath {HOOKS_DIR}",
    )


def check_linux_build_dependencies(probe: Probe, root: Path, platform_name: str) -> Check:
    """Rule: on Linux, the native build libraries CI installs are present.

    Non-Linux platforms report the check as not applicable, so a Windows or
    macOS checkout is never failed for a Linux-only prerequisite.
    """
    if not platform_name.startswith("linux"):
        return Check(
            "linux build libs",
            Status.SKIP,
            f"Linux-only check; not applicable on {platform_name}",
        )

    if probe.which("pkg-config") is None:
        return Check(
            "linux build libs",
            Status.WARN,
            "pkg-config is unavailable, so the native build libraries cannot be verified",
            "install pkg-config (Debian/Ubuntu: "
            "`sudo apt-get install -y pkg-config`)",
        )

    missing = [
        package
        for package, module in LINUX_BUILD_PACKAGES
        if probe.run(["pkg-config", "--exists", module], root).returncode != 0
    ]
    if not missing:
        return Check(
            "linux build libs",
            Status.PASS,
            "ALSA, udev, Wayland, xkbcommon and fontconfig are present",
        )
    return Check(
        "linux build libs",
        Status.WARN,
        "missing native build libraries: " + ", ".join(missing),
        "sudo apt-get install -y " + " ".join(missing),
    )


def checks_for(
    probe: Probe, root: Path, platform_name: str, pin: ToolchainPin
) -> list[Check]:
    """Every check, given an already-read pin (the seam ``--self-test`` uses)."""
    checks = check_rust_toolchain(probe, root, pin)
    checks += check_components(probe, root, pin)
    checks.append(check_git_hooks(probe, root))
    checks.append(check_linux_build_dependencies(probe, root, platform_name))
    return checks


def diagnose(probe: Probe, root: Path, platform_name: str) -> list[Check]:
    return checks_for(probe, root, platform_name, read_toolchain_pin(root))


def exit_code(checks: Sequence[Check]) -> int:
    """0 when nothing is blocking, 1 when any blocking check failed."""
    return 1 if any(check.status is Status.FAIL for check in checks) else 0
