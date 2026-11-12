#!/usr/bin/env python3
"""Logic tests for `scripts/archive_accept.py`.

The archives are built here, so the checks run against a real zip or tar.gz. Only
the executables behind them are fake: `Runner` is replaced by a stub that answers
as the released binaries would, one knob per broken answer.
"""

from __future__ import annotations

import contextlib
import io
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parent))
import archive_accept as accept  # noqa: E402
import package_lib as packaging  # noqa: E402

COMMIT_EPOCH = 1_793_886_831
VERSION = "0.1.0"
PLAYER = f"unmei-play-{VERSION}-x86_64-windows"
CLI = f"unmei-{VERSION}-x86_64-windows"


def result(code=0, out="", err=""):
    return SimpleNamespace(returncode=code, stdout=out, stderr=err)


def player_entries(stem=PLAYER, readme=None):
    return [
        packaging.ArchiveEntry(stem, 0o755, True, b""),
        packaging.ArchiveEntry(f"{stem}/unmei-play.exe", 0o755, False, b"binary"),
        packaging.ArchiveEntry(f"{stem}/README", 0o644, False,
                               (readme or f"# Unmei {VERSION}\n").encode("utf-8")),
        packaging.ArchiveEntry(f"{stem}/LICENSE", 0o644, False, b"license"),
        packaging.ArchiveEntry(f"{stem}/THIRD-PARTY-LICENSES", 0o644, False, b"notices"),
        packaging.ArchiveEntry(f"{stem}/IBMPlexSans-license.txt", 0o644, False, b"form"),
        packaging.ArchiveEntry(f"{stem}/FiraMono-OFL.txt", 0o644, False, b"form"),
        packaging.ArchiveEntry(f"{stem}/FiraMono-LICENSE", 0o644, False, b"form"),
        packaging.ArchiveEntry(f"{stem}/demo-pack", 0o755, True, b""),
        packaging.ArchiveEntry(f"{stem}/demo-pack/pack.ron", 0o644, False, b"(pack)"),
    ]


def cli_entries(stem=CLI, with_pack=True):
    entries = [
        packaging.ArchiveEntry(stem, 0o755, True, b""),
        packaging.ArchiveEntry(f"{stem}/unmei.exe", 0o755, False, b"binary"),
        packaging.ArchiveEntry(f"{stem}/README", 0o644, False, f"# Unmei {VERSION}\n".encode()),
        packaging.ArchiveEntry(f"{stem}/LICENSE", 0o644, False, b"license"),
        packaging.ArchiveEntry(f"{stem}/THIRD-PARTY-LICENSES", 0o644, False, b"notices"),
    ]
    # The packager bundles the demo pack in both archives: the author README's
    # first commands run `--pack demo-pack` from the CLI folder.
    if with_pack:
        entries.append(packaging.ArchiveEntry(f"{stem}/demo-pack", 0o755, True, b""))
        entries.append(packaging.ArchiveEntry(f"{stem}/demo-pack/pack.ron", 0o644, False,
                                              b"(pack)"))
    return entries


class FakeRunner(accept.Runner):
    """Answers as the released binaries do, with one knob per check."""

    def __init__(self, list_output=accept.CLI_LIST_EXPECTED, help_code=0,
                 version_suffix=""):
        self.list_output = list_output
        self.help_code = help_code
        self.version_suffix = version_suffix

    def run(self, argv, cwd, env, timeout):
        if argv[-1] == "--help":
            return result(self.help_code, "Usage: unmei [OPTIONS] <COMMAND>\n")
        if argv[-1] == "-V":
            name = Path(argv[0]).name.removesuffix(".exe")
            return result(0, f"{name} {VERSION}{self.version_suffix}\n")
        return result(0, self.list_output)


class ArchiveTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.addCleanup(self.tempdir.cleanup)
        self.root = Path(self.tempdir.name)

    def write_archive(self, name, entries, into=None):
        path = (into or self.root) / name
        packaging.write_zip(path, entries, COMMIT_EPOCH)
        return path

    def release_dir(self, cli_with_pack=True):
        directory = self.root / "package"
        directory.mkdir()
        player = self.write_archive(PLAYER + ".zip", player_entries(), directory)
        cli = self.write_archive(CLI + ".zip", cli_entries(with_pack=cli_with_pack), directory)
        sums = "".join(f"{accept.sha256_file(path)}  {path.name}\n" for path in (player, cli))
        (directory / "SHA256SUMS").write_text(sums, encoding="utf-8")
        return directory

    # --- archive discovery and extraction ---------------------------------

    def test_each_archive_is_found_by_the_name_packaging_writes(self) -> None:
        directory = self.release_dir()
        self.assertEqual(accept.find_archive(directory, "player", "windows").name,
                         PLAYER + ".zip")
        self.assertEqual(accept.find_archive(directory, "cli", "windows").name,
                         CLI + ".zip")

    def test_a_missing_archive_is_refused(self) -> None:
        directory = self.release_dir()
        accept.find_archive(directory, "cli", "windows").unlink()
        with self.assertRaises(accept.Refusal) as raised:
            accept.find_archive(directory, "cli", "windows")
        self.assertIn("not one", str(raised.exception))

    def test_extraction_requires_one_top_level_folder(self) -> None:
        archive = self.root / (PLAYER + ".zip")
        packaging.write_zip(archive, [
            packaging.ArchiveEntry("one/a", 0o644, False, b"a"),
            packaging.ArchiveEntry("two/b", 0o644, False, b"b"),
        ], COMMIT_EPOCH)
        with self.assertRaises(accept.Refusal) as raised:
            accept.extract(archive, self.root / "out")
        self.assertIn("not one", str(raised.exception))

    # --- the payload ------------------------------------------------------

    def test_the_player_layout_passes_and_names_what_is_missing(self) -> None:
        root = accept.extract(self.write_archive(PLAYER + ".zip", player_entries()),
                              self.root / "p")
        self.assertEqual(accept.layout_problems("player", root, VERSION, ".exe"), [])
        (root / "THIRD-PARTY-LICENSES").unlink()
        (root / "demo-pack" / "pack.ron").unlink()
        problems = accept.layout_problems("player", root, VERSION, ".exe")
        self.assertEqual(len(problems), 2)
        self.assertTrue(any("THIRD-PARTY-LICENSES is missing" in item for item in problems))
        self.assertTrue(any("demo-pack/pack.ron is missing" in item for item in problems))

    def test_the_cli_layout_carries_the_demo_pack_its_readme_uses(self) -> None:
        # The first item-eleven rehearsal refused a correct CLI archive here.
        root = accept.extract(self.write_archive(CLI + ".zip", cli_entries()),
                              self.root / "c")
        self.assertEqual(accept.layout_problems("cli", root, VERSION, ".exe"), [])
        (self.root / "bare").mkdir()
        bare = accept.extract(self.write_archive(CLI + ".zip", cli_entries(with_pack=False),
                                                 self.root / "bare"), self.root / "b")
        problems = accept.layout_problems("cli", bare, VERSION, ".exe")
        self.assertEqual(len(problems), 1)
        self.assertIn("demo-pack/pack.ron is missing", problems[0])

    def test_the_cli_list_sample_reads_the_cli_archive_s_own_demo_pack(self) -> None:
        roots = {"player": self.root / "p", "cli": self.root / "c"}
        argv = dict((name, argv) for name, argv, _ in
                    accept.checks_for(roots, VERSION, ".exe"))["cli-list"]
        self.assertEqual(argv[argv.index("--pack") + 1], str(roots["cli"] / "demo-pack"))

    def test_a_readme_without_the_version_is_a_failure(self) -> None:
        archive = self.write_archive(PLAYER + ".zip", player_entries(readme="# Unmei\n"))
        root = accept.extract(archive, self.root / "p")
        problems = accept.layout_problems("player", root, VERSION, ".exe")
        self.assertTrue(any("does not name" in item for item in problems))

    # --- the executables --------------------------------------------------

    def test_the_environment_carries_no_cargo_or_toolchain(self) -> None:
        env = accept.clean_env(self.root / "home")
        self.assertEqual(env["HOME"], str(self.root / "home"))
        self.assertNotIn("cargo", env["PATH"].lower())
        self.assertNotIn("RUSTUP_TOOLCHAIN", env)

    def test_the_release_accepts_cleanly_and_writes_its_slot(self) -> None:
        directory = self.release_dir()
        out = self.root / "accept-windows-slot.json"
        with contextlib.redirect_stdout(io.StringIO()):
            status = accept.accept_archives(FakeRunner(), directory, "windows", out)
        self.assertEqual(status, 0)
        self.assertEqual(json.loads(out.read_text()),
                         {"slot": "accept-windows", "status": "passed"})

    def test_a_different_list_sample_fails_the_slot_and_names_the_check(self) -> None:
        directory = self.release_dir()
        out = self.root / "slot.json"
        with contextlib.redirect_stderr(io.StringIO()):
            status = accept.accept_archives(FakeRunner(list_output="other rows\n"), directory,
                                   "windows", out)
        self.assertEqual(status, 1)
        slot = json.loads(out.read_text())
        self.assertEqual(slot["status"], "failed")
        self.assertIn("cli-list", slot["reason"])

    def test_the_boot_passes_only_for_a_bounded_launch(self) -> None:
        class BootRunner(accept.Runner):
            def __init__(self, codes, output=""):
                self.codes = iter(codes)
                self.output = output

            def run(self, argv, cwd, env, timeout):
                return result(next(self.codes), self.output)

        player = self.root / "unmei-play"
        player.write_bytes(b"binary")
        self.assertEqual(accept.boot_problems(BootRunner([124, 137]), player), [])
        self.assertEqual(accept.boot_problems(BootRunner([0, 124]), player),
                         ["the normal boot exited 0 instead of running to the 20s bound"])

    def test_a_panicking_boot_is_named(self) -> None:
        class BootRunner(accept.Runner):
            def run(self, argv, cwd, env, timeout):
                return result(124, "thread 'main' panicked at src/main.rs:1")

        player = self.root / "unmei-play"
        player.write_bytes(b"binary")
        self.assertEqual(accept.boot_problems(BootRunner(), player),
                         ["the normal boot panicked", "the --dev boot panicked"])

    def test_the_boot_command_matches_the_reviewed_boot(self) -> None:
        self.assertEqual(
            accept.boot_command("/x/unmei-play", 20, True),
            ["xvfb-run", "-a", "-s", "-screen 0 1920x1080x24", "timeout",
             "--signal=KILL", "20", "/x/unmei-play", "--dev"])

    def test_a_failing_help_or_wrong_version_is_named(self) -> None:
        directory = self.release_dir()
        out = self.root / "slot.json"
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(accept.accept_archives(FakeRunner(help_code=2), directory, "windows", out), 1)
        self.assertIn("--help", json.loads(out.read_text())["reason"])
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(accept.accept_archives(FakeRunner(version_suffix="+dirty"), directory,
                                           "windows", out), 1)
        self.assertIn("player-version", json.loads(out.read_text())["reason"])


class RelativeBootPath(unittest.TestCase):
    def test_the_boot_player_path_is_absolute_from_a_relative_extracted_dir(self) -> None:
        # The boots run from another working directory, so a relative path
        # made the first rehearsal's boots exit 127.
        with tempfile.TemporaryDirectory() as tmp:
            player = Path(tmp) / "extracted" / "unmei-play-0.1.0-x86_64-linux" / "unmei-play"
            player.parent.mkdir(parents=True)
            player.write_bytes(b"")
            here = os.getcwd()
            os.chdir(tmp)
            try:
                found = accept.find_player("extracted")
            finally:
                os.chdir(here)
            self.assertTrue(found.is_absolute(), found)


class CommandLineTests(unittest.TestCase):
    """The `main()` entry point, which the workflow actually runs."""

    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.addCleanup(self.tempdir.cleanup)
        self.root = Path(self.tempdir.name)

    def test_a_tampered_archive_and_a_missing_sums_file_are_refused(self) -> None:
        directory = self.root / "package"
        directory.mkdir()
        archive = directory / f"{PLAYER}.zip"
        packaging.write_zip(archive, player_entries(), COMMIT_EPOCH)
        self.assertEqual(accept.sums_problems(directory, [archive.name]),
                         [f"{directory} carries no SHA256SUMS"])
        (directory / "SHA256SUMS").write_text(
            f"{'0' * 64}  {archive.name}\n", encoding="utf-8")
        self.assertEqual(accept.sums_problems(directory, [archive.name]),
                         [f"{archive.name} does not match SHA256SUMS"])

    def test_the_accept_command_runs_through_main(self) -> None:
        directory = self.root / "package"
        directory.mkdir()
        player = directory / f"{PLAYER}.zip"
        cli = directory / f"{CLI}.zip"
        packaging.write_zip(player, player_entries(), COMMIT_EPOCH)
        packaging.write_zip(cli, cli_entries(), COMMIT_EPOCH)
        (directory / "SHA256SUMS").write_text(
            "".join(f"{accept.sha256_file(path)}  {path.name}\n" for path in (player, cli)),
            encoding="utf-8")
        out = self.root / "slot.json"
        original = accept.Runner
        accept.Runner = FakeRunner
        self.addCleanup(setattr, accept, "Runner", original)
        with contextlib.redirect_stdout(io.StringIO()):
            status = accept.main(["accept", "--archive-dir", str(directory),
                                  "--os", "windows", "--out", str(out)])
        self.assertEqual(status, 0)
        self.assertEqual(json.loads(out.read_text())["status"], "passed")


if __name__ == "__main__":
    unittest.main()
