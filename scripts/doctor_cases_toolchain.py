"""Self-test cases for the Rust toolchain, its components and the pin."""

from __future__ import annotations

from typing import Callable

from doctor_checks import exit_code
from doctor_probe import (
    parse_toolchain_pin_fallback,
    CommandResult,
    Status,
    parse_toolchain_pin,
    pin_agrees,
)
from doctor_selftest import (
    SELF_TEST_BINARIES,
    check_named,
    diagnose_checks,
    expect_doctor_error,
    make_probe,
)


def healthy_environment_has_no_blocking_problem() -> None:
    checks = diagnose_checks(make_probe())
    assert exit_code(checks) == 0, checks
    assert all(
        check.status in (Status.PASS, Status.SKIP) for check in checks
    ), checks


def missing_rust_fails() -> None:
    checks = diagnose_checks(make_probe(SELF_TEST_BINARIES - {"rustc", "cargo"}))
    assert exit_code(checks) == 1, checks
    assert check_named(checks, "rustc").status is Status.FAIL
    assert check_named(checks, "cargo").status is Status.FAIL


def mismatched_toolchain_fails() -> None:
    checks = diagnose_checks(
        make_probe(
            results={
                ("rustc", "-vV"): CommandResult(0, "rustc 1.97.0\nrelease: 1.97.0\n")
            }
        )
    )
    assert exit_code(checks) == 1, checks
    rustc = check_named(checks, "rustc")
    assert rustc.status is Status.FAIL, rustc
    assert "1.98.1" in rustc.message and "1.97.0" in rustc.message, rustc


def uninstalled_pinned_toolchain_fails() -> None:
    checks = diagnose_checks(
        make_probe(
            results={
                ("rustc", "-vV"): CommandResult(
                    1, "", "error: toolchain '1.98.1' is not installed\n"
                )
            }
        )
    )
    assert exit_code(checks) == 1, checks
    assert "rustup toolchain install 1.98.1" in (
        check_named(checks, "rustc").suggestion or ""
    ), checks


def missing_rustfmt_only_warns() -> None:
    checks = diagnose_checks(
        make_probe(
            results={
                ("cargo", "fmt", "--version"): CommandResult(
                    1, "", "error: no such command: fmt\n"
                )
            }
        )
    )
    assert exit_code(checks) == 0, checks
    rustfmt = check_named(checks, "rustfmt")
    assert rustfmt.status is Status.WARN, rustfmt
    assert "rustup component add rustfmt" in (rustfmt.suggestion or ""), rustfmt


def cargo_reported_when_only_rustc_is_missing() -> None:
    checks = diagnose_checks(make_probe(SELF_TEST_BINARIES - {"rustc"}))
    assert exit_code(checks) == 1, checks
    assert check_named(checks, "rustc").status is Status.FAIL, checks
    assert check_named(checks, "cargo").status is Status.PASS, checks


def pin_parser_reads_channel_and_components() -> None:
    pin = parse_toolchain_pin(
        "[toolchain]\n"
        'channel = "1.98.1"\n'
        'profile = "minimal"\n'
        'components = ["rustfmt", "clippy"]\n'
    )
    assert pin.channel == "1.98.1", pin
    assert pin.components == ("rustfmt", "clippy"), pin


def pin_parser_refuses_a_missing_channel() -> None:
    expect_doctor_error(
        "a pin without a channel",
        lambda: parse_toolchain_pin("[toolchain]\nprofile = 'minimal'\n"),
    )


def fallback_pin_parser_matches_the_toml_parser() -> None:
    text = (
        "[toolchain]\n"
        'channel = "1.98.1"\n'
        'profile = "minimal"\n'
        'components = ["rustfmt", "clippy"]\n'
    )
    assert parse_toolchain_pin_fallback(text) == ("1.98.1", ("rustfmt", "clippy"))
    assert parse_toolchain_pin_fallback("[toolchain]\nchannel = \"nightly\"\n") == (
        "nightly",
        (),
    )


def pin_agreement_reads_versions_and_admits_channels() -> None:
    assert pin_agrees("1.98.1", "1.98.1") is True
    assert pin_agrees("1.98", "1.98.1") is True
    assert pin_agrees("1.98.1", "1.97.0") is False
    assert pin_agrees("nightly-2025-01-01", "1.98.1") is None


def cases() -> list[tuple[str, Callable[[], None]]]:
    """(label, case) pairs, in report order."""
    return [
        ("healthy environment has no blocking problem", healthy_environment_has_no_blocking_problem),
        ("missing rustc/cargo fails", missing_rust_fails),
        ("mismatched toolchain fails", mismatched_toolchain_fails),
        ("uninstalled pinned toolchain fails", uninstalled_pinned_toolchain_fails),
        ("missing rustfmt only warns", missing_rustfmt_only_warns),
        ("cargo is still reported when only rustc is missing", cargo_reported_when_only_rustc_is_missing),
        ("pin parser reads channel and components", pin_parser_reads_channel_and_components),
        ("pin parser refuses a missing channel", pin_parser_refuses_a_missing_channel),
        ("fallback pin parser matches the TOML parser", fallback_pin_parser_matches_the_toml_parser),
        ("pin agreement reads versions and admits channels", pin_agreement_reads_versions_and_admits_channels),
    ]
