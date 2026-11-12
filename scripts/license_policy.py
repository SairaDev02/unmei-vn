"""The two accepted-license lists, and whether they still agree.

`packaging/about.toml` is what cargo-about reads to build the shipped notices;
`deny.toml` is what cargo-deny reads to gate a dependency change. Neither tool
reads the other's file and cargo-deny offers no include, so the list exists
twice and only this module keeps the copies equal. `check-invariants.py --rules licenses`
is the rule; this is the reading and the comparison behind it.

It sits beside the checker rather than inside it because that file is close to
the repository's 20,000-byte authored-file budget
(`scripts/check-staged-token-limit.py`), not because the rule is separable from
the rest of the checker.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

try:  # Python 3.11+; the fallback below keeps older interpreters working.
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - depends on the interpreter
    tomllib = None  # type: ignore[assignment]

RULE = "licenses"
ABOUT_TOML = "packaging/about.toml"
DENY_TOML = "deny.toml"
ABOUT_KEY = "accepted"
DENY_SECTION = "licenses"
DENY_KEY = "allow"
INVARIANT = f"{DENY_TOML} accepts exactly the licenses in {ABOUT_TOML}"
REPRODUCE = f"python3 scripts/check-invariants.py --rules {RULE}"


class LicensePolicyError(RuntimeError):
    """A list cannot be read, so the rule cannot report a pass."""


@dataclass(frozen=True)
class Mismatch:
    """The licenses one file keeps that the other does not."""

    source: str
    licenses: tuple[str, ...]


_QUOTED_RE = re.compile(r'"([^"]*)"')


def _quoted_list(text: str, key: str) -> list[str] | None:
    """Read one list of quoted identifiers without a TOML parser.

    Only interpreters that predate ``tomllib`` reach this. Both files hold these
    lists as one key per line, and the key must start its line, so
    ``allow-registry`` is never read as ``allow``. A missing key returns
    ``None`` for the caller to refuse.
    """
    match = re.search(rf'^{re.escape(key)}\s*=\s*\[(.*?)\]', text, re.S | re.M)
    if match is None:
        return None
    return _QUOTED_RE.findall(match.group(1))


def parse_license_list(
    text: str, key: str, table: str | None, name: str
) -> frozenset[str]:
    """The license identifiers under ``key``, or a LicensePolicyError.

    A missing key, an empty list or a non-string entry raises instead of
    returning a partial set: comparing nothing against nothing would pass every
    tree.
    """
    if tomllib is not None:
        try:
            data = tomllib.loads(text)
        except tomllib.TOMLDecodeError as exc:
            raise LicensePolicyError(f"{name} is not valid TOML: {exc}") from exc
        if table is not None:
            section = data.get(table)
            if not isinstance(section, dict):
                raise LicensePolicyError(f"{name} has no [{table}] table")
            data = section
        values: object = data.get(key)
    else:  # pragma: no cover - only on a Python without tomllib
        values = _quoted_list(text, key)
    if not isinstance(values, list) or not values:
        raise LicensePolicyError(f"{name} has no non-empty {key} list")
    entries: list[str] = []
    for value in values:
        if not isinstance(value, str):
            raise LicensePolicyError(
                f"{name}: {key} holds a non-string entry: {value!r}"
            )
        entries.append(value)
    return frozenset(entries)


def read_lists(root: Path) -> tuple[frozenset[str], frozenset[str]]:
    """The (cargo-about, cargo-deny) license sets committed under ``root``."""

    def read(relative: str, key: str, table: str | None) -> frozenset[str]:
        try:
            text = (root / relative).read_text(encoding="utf-8")
        except OSError as exc:
            raise LicensePolicyError(f"cannot read {relative}: {exc}") from exc
        return parse_license_list(text, key, table, relative)

    return (
        read(ABOUT_TOML, ABOUT_KEY, None),
        read(DENY_TOML, DENY_KEY, DENY_SECTION),
    )


def differences(about: frozenset[str], deny: frozenset[str]) -> tuple[Mismatch, ...]:
    """Every licence the two sets do not share; empty means they agree."""
    return tuple(
        Mismatch(source, tuple(sorted(licenses)))
        for source, licenses in ((ABOUT_TOML, about - deny), (DENY_TOML, deny - about))
        if licenses
    )


def self_test_cases() -> list[tuple[str, Callable[[], None]]]:
    """Sensitivity cases ``check-invariants.py --self-test`` runs for this rule."""

    about_snippet = (
        "# Accepted licenses, in priority order.\n"
        'accepted = [\n    "Apache-2.0",\n    "MIT",\n    "OpenSSL",\n]\n'
    )
    deny_snippet = (
        "[licenses]\n"
        'allow = [\n    "MIT",\n    "Apache-2.0",\n    "OpenSSL",\n]\n'
        "include-dev = true\n"
        'allow-registry-near-miss = ["notme"]\n'
    )

    def reads_both_file_shapes() -> None:
        expected = frozenset({"Apache-2.0", "MIT", "OpenSSL"})
        assert (
            parse_license_list(about_snippet, ABOUT_KEY, None, ABOUT_TOML) == expected
        )
        assert (
            parse_license_list(deny_snippet, DENY_KEY, DENY_SECTION, DENY_TOML)
            == expected
        )
        assert _quoted_list(deny_snippet, DENY_KEY) == [
            "MIT",
            "Apache-2.0",
            "OpenSSL",
        ]

    def accepts_the_same_set_in_any_order() -> None:
        assert not differences(
            frozenset({"Apache-2.0", "MIT"}), frozenset({"MIT", "Apache-2.0"})
        )

    def rejects_a_license_one_file_keeps() -> None:
        assert differences(
            frozenset({"Apache-2.0", "OpenSSL"}), frozenset({"Apache-2.0"})
        ) == (Mismatch(ABOUT_TOML, ("OpenSSL",)),)

    def rejects_a_license_the_other_file_lacks() -> None:
        assert differences(
            frozenset({"Apache-2.0"}), frozenset({"Apache-2.0", "GPL-3.0-or-later"})
        ) == (Mismatch(DENY_TOML, ("GPL-3.0-or-later",)),)

    def refuses_anything_it_cannot_compare() -> None:
        unreadable = {
            "a file without the key": lambda: parse_license_list(
                deny_snippet, ABOUT_KEY, None, ABOUT_TOML
            ),
            "an empty list": lambda: parse_license_list(
                "accepted = []\n", ABOUT_KEY, None, ABOUT_TOML
            ),
            "a non-string entry": lambda: parse_license_list(
                'accepted = [\n    "MIT",\n    4,\n]\n',
                ABOUT_KEY,
                None,
                ABOUT_TOML,
            ),
            "a missing table": lambda: parse_license_list(
                about_snippet, DENY_KEY, DENY_SECTION, DENY_TOML
            ),
        }
        for what, action in unreadable.items():
            try:
                action()
            except LicensePolicyError:
                continue
            raise AssertionError(f"{what} must raise LicensePolicyError")

    return [
        ("license lists read both file shapes", reads_both_file_shapes),
        (
            "license rule accepts the same set in any order",
            accepts_the_same_set_in_any_order,
        ),
        (
            "license rule rejects a license one file keeps",
            rejects_a_license_one_file_keeps,
        ),
        (
            "license rule rejects a license the other file lacks",
            rejects_a_license_the_other_file_lacks,
        ),
        (
            "license reader refuses anything it cannot compare",
            refuses_anything_it_cannot_compare,
        ),
    ]
