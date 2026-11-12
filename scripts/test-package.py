#!/usr/bin/env python3
"""Logic tests for `scripts/package.py`.

These tests never run cargo, rustup, node or the network. They load the script
as a module so its pure helpers can be exercised with fake command output and
temporary directories, and they build archives from synthetic bytes. One class
reads the workspace manifests themselves, because the repository a crate names
is a property of the committed tree rather than of a helper.
"""

from __future__ import annotations

import gzip
import importlib.util
import stat
import tarfile
import tempfile
import time
import unittest
import zipfile
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - Python before 3.11
    tomllib = None

SCRIPT = Path(__file__).with_name("package.py")
ROOT = SCRIPT.resolve().parent.parent
COMMIT_EPOCH = 1_793_886_831
# The export replaces this token with the public repository URL, so a committed
# manifest holds either the token or that filled URL.
REPOSITORY_TOKEN = "@@PUBLIC_REPO_URL@@"
# `repository` is the only place a crate names the project. A homepage or a
# documentation URL there would send readers and docs.rs somewhere else.
IDENTITY_FIELDS = ("homepage", "documentation", "authors")


def manifest_identity_findings(manifests: dict[str, str]) -> list[str]:
    """Identity findings for `{manifest path: text}`, in path order."""
    findings = []
    for path in sorted(manifests):
        package = tomllib.loads(manifests[path]).get("package", {})
        repository = package.get("repository")
        if repository is None:
            findings.append(f"{path}: no package.repository")
        elif repository != REPOSITORY_TOKEN and not repository.startswith("https://"):
            findings.append(f"{path}: package.repository names no repository: {repository!r}")
        findings.extend(
            f"{path}: package.{field} is set"
            for field in IDENTITY_FIELDS
            if field in package
        )
    return findings


def load_package_module():
    spec = importlib.util.spec_from_file_location("unmei_package_under_test", SCRIPT)
    if spec is None or spec.loader is None:  # pragma: no cover - defensive
        raise ImportError(f"cannot load {SCRIPT}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class PackageTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.pkg = load_package_module()
        cls.win = cls.pkg.target_for("win32", "AMD64")
        cls.linux = cls.pkg.target_for("linux", "x86_64")

    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.addCleanup(self.tempdir.cleanup)
        self.root = Path(self.tempdir.name)

    # --- payload selection ------------------------------------------------

    def test_demo_pack_selection_keeps_tracked_content_only(self) -> None:
        listing = "\n".join(
            [
                "examples/demo-pack/pack.ron",
                "examples/demo-pack/state.ron",
                "examples/demo-pack/scenes/first_meeting.unmei",
                "examples/demo-pack/assets/README.md",
                "examples/demo-pack/AGENT-GUIDE.md",
                "examples/unrelated.txt",
                "cli/Cargo.toml",
            ]
        )
        self.assertEqual(
            self.pkg.demo_pack_files(listing),
            [
                "AGENT-GUIDE.md",
                "assets/README.md",
                "pack.ron",
                "scenes/first_meeting.unmei",
                "state.ron",
            ],
        )

    def test_demo_pack_selection_accepts_nul_separated_git_output(self) -> None:
        listing = "examples/demo-pack/pack.ron\0examples/demo-pack/state.ron\0"
        self.assertEqual(
            self.pkg.demo_pack_files(listing), ["pack.ron", "state.ron"]
        )

    def test_sidecars_logs_temp_and_credentials_never_enter(self) -> None:
        # A tracked sidecar is still refused: the release set excludes it whether
        # or not somebody has run the player inside the repository.
        listing = "\n".join(
            [
                "examples/demo-pack/pack.ron",
                "examples/demo-pack/demo-pack.slot1.save.ron",
                "examples/demo-pack/demo-pack.log",
                "examples/demo-pack/demo-pack.read.ron",
                "examples/demo-pack/demo-pack.settings.ron",
                "examples/demo-pack/scenes/.first_meeting.unmei.swp",
                "examples/demo-pack/notes.tmp",
                "examples/demo-pack/credentials",
                "examples/demo-pack/.env",
                "examples/demo-pack/unmei.author.ron",
            ]
        )
        self.assertEqual(self.pkg.demo_pack_files(listing), ["pack.ron"])

        # The same names present on disk cannot enter either: selection reads the
        # tracked listing, never the working tree.
        pack_dir = self.root / "examples" / "demo-pack"
        pack_dir.mkdir(parents=True)
        for name in (
            "demo-pack.slot1.save.ron",
            "demo-pack.log",
            "credentials",
            "notes.tmp",
        ):
            (pack_dir / name).write_text("secret", encoding="utf-8")
        plan = self.pkg.plan_archive("player", "0.1.0", self.win, ["pack.ron"])
        names = [entry.arcname for entry in plan]
        for leaked in ("demo-pack.log", "credentials", "notes.tmp"):
            self.assertFalse(
                any(name.endswith(leaked) for name in names), f"{leaked} leaked"
            )

    # --- names and layout -------------------------------------------------

    def test_archive_names_follow_the_release_identity(self) -> None:
        self.assertEqual(
            self.pkg.archive_name("player", "0.1.0", self.win),
            "unmei-play-0.1.0-x86_64-windows.zip",
        )
        self.assertEqual(
            self.pkg.archive_name("player", "0.1.0", self.linux),
            "unmei-play-0.1.0-x86_64-linux.tar.gz",
        )
        self.assertEqual(
            self.pkg.archive_name("cli", "0.1.0", self.win),
            "unmei-0.1.0-x86_64-windows.zip",
        )
        self.assertEqual(self.pkg.archive_stem("unmei-0.1.0-x86_64-linux.tar.gz"),
                         "unmei-0.1.0-x86_64-linux")
        self.assertEqual(self.win.triple, "x86_64-pc-windows-msvc")
        self.assertEqual(self.linux.triple, "x86_64-unknown-linux-gnu")

    def test_player_layout_has_one_top_folder_and_the_full_payload(self) -> None:
        demo = [
            "AGENT-GUIDE.md",
            "assets/sprites/ace/neutral.png",
            "pack.ron",
        ]
        plan = self.pkg.plan_archive("player", "0.1.0", self.win, demo)
        names = [entry.arcname for entry in plan]
        stem = "unmei-play-0.1.0-x86_64-windows"
        self.assertEqual(names, sorted(names))
        self.assertEqual(names[0], stem)
        for name in names[1:]:
            self.assertTrue(name.startswith(f"{stem}/"), name)
        for expected in (
            f"{stem}/README",
            f"{stem}/LICENSE",
            f"{stem}/THIRD-PARTY-LICENSES",
            f"{stem}/unmei-play.exe",
            f"{stem}/demo-pack",
            f"{stem}/demo-pack/AGENT-GUIDE.md",
            f"{stem}/demo-pack/assets/sprites/ace/neutral.png",
            f"{stem}/IBMPlexSans-license.txt",
            f"{stem}/FiraMono-OFL.txt",
            f"{stem}/FiraMono-LICENSE",
        ):
            self.assertIn(expected, names)
        self.assertNotIn(f"{stem}/unmei", names)

    def test_cli_layout_uses_its_own_readme_and_no_font_licenses(self) -> None:
        plan = self.pkg.plan_archive("cli", "0.1.0", self.linux, ["pack.ron"])
        names = [entry.arcname for entry in plan]
        stem = "unmei-0.1.0-x86_64-linux"
        self.assertIn(f"{stem}/unmei", names)
        self.assertIn(f"{stem}/demo-pack/pack.ron", names)
        self.assertNotIn(f"{stem}/unmei-play", names)
        for font in self.pkg.FONT_LICENSES:
            self.assertNotIn(f"{stem}/{font}", names)
        readmes = {entry.arcname: entry.origin for entry in plan}
        self.assertEqual(readmes[f"{stem}/README"], "readme")

    def test_modes_are_executable_for_the_binary_and_directories(self) -> None:
        plan = self.pkg.plan_archive("player", "0.1.0", self.win, ["pack.ron"])
        modes = {entry.arcname: entry.mode for entry in plan}
        stem = "unmei-play-0.1.0-x86_64-windows"
        self.assertEqual(modes[stem], 0o755)
        self.assertEqual(modes[f"{stem}/demo-pack"], 0o755)
        self.assertEqual(modes[f"{stem}/unmei-play.exe"], 0o755)
        self.assertEqual(modes[f"{stem}/README"], 0o644)
        self.assertEqual(modes[f"{stem}/demo-pack/pack.ron"], 0o644)

    # --- refusals ---------------------------------------------------------

    def test_dirty_build_inputs_refused_and_override_allowed(self) -> None:
        status = "\n".join(
            [
                " M packaging/player-README.md",
                "?? examples/demo-pack/pack.log",
                " M app/src/main.rs",
                "?? engine/src/x.rs",
                "R  app/assets/fonts/old.txt -> app/assets/fonts/new.txt",
                "?? notes.txt",
                " M Cargo.toml",
            ]
        )
        dirty = self.pkg.dirty_build_input_paths(status)
        self.assertIn("packaging/player-README.md", dirty)
        self.assertIn("examples/demo-pack/pack.log", dirty)
        self.assertIn("app/src/main.rs", dirty)
        self.assertIn("engine/src/x.rs", dirty)
        self.assertIn("app/assets/fonts/new.txt", dirty)
        self.assertIn("Cargo.toml", dirty)
        self.assertNotIn("notes.txt", dirty)

        with self.assertRaises(self.pkg.Refusal) as raised:
            self.pkg.require_clean(dirty)
        message = str(raised.exception)
        for path in ("packaging/player-README.md", "app/src/main.rs", "engine/src/x.rs"):
            self.assertIn(path, message)
        self.assertIn("--allow-dirty", message)

        # A change outside every build input cannot change the archives.
        self.pkg.require_clean(self.pkg.dirty_build_input_paths(" M notes.txt"))

    def test_non_empty_output_directory_is_refused(self) -> None:
        out = self.root / "out"
        out.mkdir()
        self.pkg.require_empty_out(out)
        (out / "stale.zip").write_bytes(b"old")
        with self.assertRaises(self.pkg.Refusal) as raised:
            self.pkg.require_empty_out(out)
        self.assertIn("not empty", str(raised.exception))

    def test_remaining_placeholders_refused_and_override_allowed(self) -> None:
        texts = {
            "packaging/player-README.md": "Unmei @@VERSION@@ at @@PUBLIC_TRACKER_URL@@\n",
            "engine/Cargo.toml": 'repository = "@@PUBLIC_REPO_URL@@"\n',
            "examples/demo-pack/AGENT-GUIDE.md": "see @@GUIDE_URL@@\n",
        }
        findings = self.pkg.placeholder_findings(texts)
        self.assertEqual(
            findings,
            [
                "packaging/player-README.md: @@PUBLIC_TRACKER_URL@@",
                "engine/Cargo.toml: @@PUBLIC_REPO_URL@@",
                "examples/demo-pack/AGENT-GUIDE.md: @@GUIDE_URL@@",
            ],
        )
        with self.assertRaises(self.pkg.Refusal) as raised:
            self.pkg.require_no_placeholders(findings)
        self.assertIn("@@PUBLIC_REPO_URL@@", str(raised.exception))
        self.assertIn("--allow-placeholders", str(raised.exception))

        self.pkg.require_no_placeholders(
            self.pkg.placeholder_findings({"README": "mail me @ someone.example\n"})
        )

    def test_the_placeholder_scan_covers_both_crate_roots(self) -> None:
        # The crate roots carry @@PUBLIC_REPO_URL@@ and @@GUIDE_URL@@ in their
        # rustdoc, and they ship as published crate docs, so a scan that skips
        # them cannot refuse a release whose docs still name a token.
        sources = self.pkg.placeholder_sources([])
        self.assertIn("engine/src/lib.rs", sources)
        self.assertIn("cli/src/lib.rs", sources)

    def test_cargo_about_version_mismatch_prints_the_install_command(self) -> None:
        self.assertEqual(self.pkg.parse_about_version("cargo-about 0.9.3\n"), "0.9.3")
        self.assertIsNone(self.pkg.parse_about_version("cargo about 0.9.3\n"))
        self.pkg.require_about_version("0.9.3")

        command = "cargo install --locked cargo-about@0.9.3 --features cli"
        self.assertEqual(self.pkg.install_command(), command)
        for found in ("0.10.0", None):
            with self.assertRaises(self.pkg.Refusal) as raised:
                self.pkg.require_about_version(found)
            self.assertIn(command, str(raised.exception))

    def test_failed_to_validate_line_is_a_failure_even_on_exit_zero(self) -> None:
        self.pkg.require_no_validate_failures("generated 412 crates\n")
        output = "warning: ring failed to validate the license text\n"
        self.assertEqual(self.pkg.validate_failure_lines(output), [output.strip()])
        with self.assertRaises(self.pkg.Refusal) as raised:
            self.pkg.require_no_validate_failures(output)
        self.assertIn("failed to validate", str(raised.exception))

    def test_crate_version_drift_refused(self) -> None:
        packages = [
            ("unmei-engine", "0.1.0"),
            ("unmei", "0.1.0"),
            ("unmei-app", "0.1.0"),
        ]
        self.pkg.require_versions("0.1.0", packages)
        drift = [("unmei-engine", "0.1.0"), ("unmei", "0.2.0")]
        self.assertEqual(self.pkg.version_mismatches("0.1.0", drift),
                         ["unmei 0.2.0 (workspace 0.1.0)"])
        with self.assertRaises(self.pkg.Refusal) as raised:
            self.pkg.require_versions("0.1.0", drift)
        self.assertIn("unmei 0.2.0", str(raised.exception))

    def test_workspace_version_is_read_from_the_manifest(self) -> None:
        text = (
            '[workspace]\nmembers = ["engine"]\n\n'
            '[workspace.package]\nversion = "0.1.0"\n\n'
            '[profile.release]\nlto = "thin"\n'
        )
        self.assertEqual(self.pkg.workspace_version(text), "0.1.0")
        with self.assertRaises(self.pkg.Refusal):
            self.pkg.workspace_version("[workspace]\nmembers = []\n")

    def test_failed_command_refused_with_its_output(self) -> None:
        self.pkg.require_success("cargo package -p unmei-engine", 0, "Packaged\n")
        with self.assertRaises(self.pkg.Refusal) as raised:
            self.pkg.require_success(
                "cargo package -p unmei-engine", 101, "error: 2 files dirty\n"
            )
        message = str(raised.exception)
        self.assertIn("cargo package -p unmei-engine", message)
        self.assertIn("101", message)
        self.assertIn("2 files dirty", message)

    # --- rehearsal stamp --------------------------------------------------

    def test_rehearsal_stamp_reaches_both_readmes_and_the_run_output(self) -> None:
        self.assertIsNone(self.pkg.rehearsal_stamp(False, False))
        stamp = self.pkg.rehearsal_stamp(True, False)
        self.assertIn("REHEARSAL — not a release", stamp)
        self.assertIn("REHEARSAL — not a release",
                      self.pkg.rehearsal_stamp(False, True))
        self.assertIn("REHEARSAL — not a release",
                      self.pkg.rehearsal_stamp(True, True))

        for source in ("# title @@VERSION@@\n\nbody\n", "no heading\n"):
            stamped = self.pkg.stamp_readme(self.pkg.substitute_version(source, "0.1.0"), stamp)
            self.assertEqual(stamped.splitlines()[0], "# title 0.1.0" if source.startswith("#") else "no heading")
            self.assertIn(stamp, stamped.splitlines()[:3])

        lines = self.pkg.run_header_lines(
            version="0.1.0",
            target=self.win,
            out_dir=self.root,
            stamp=stamp,
        )
        self.assertTrue(any("REHEARSAL — not a release" in line for line in lines))
        clean = self.pkg.run_header_lines(
            version="0.1.0", target=self.win, out_dir=self.root, stamp=None
        )
        self.assertFalse(any("REHEARSAL" in line for line in clean))

        self.assertEqual(
            self.pkg.substitute_version("Unmei @@VERSION@@\n", "0.1.0"),
            "Unmei 0.1.0\n",
        )

    # --- SHA256SUMS -------------------------------------------------------

    def test_sha256sums_format_is_sorted_with_two_spaces_and_lf(self) -> None:
        text = self.pkg.sha256sums_text(
            [("unmei-play.exe", "b" * 64), ("unmei-0.1.0-x86_64-windows.zip", "a" * 64)]
        )
        self.assertEqual(
            text,
            "a" * 64 + "  unmei-0.1.0-x86_64-windows.zip\n"
            + "b" * 64 + "  unmei-play.exe\n",
        )
        self.assertNotIn("\r", text)
        self.assertTrue(text.endswith("\n"))

    # --- crate staging ----------------------------------------------------

    def test_the_package_command_stages_every_publishable_crate(self) -> None:
        self.assertEqual(
            self.pkg.package_args(),
            ["cargo", "package", "--workspace", "--exclude", "unmei-app", "--locked"],
        )
        self.assertEqual(self.pkg.package_args(allow_dirty=True)[-1], "--allow-dirty")
        self.assertNotIn("--list", self.pkg.package_args(allow_dirty=True))

    def test_both_crates_are_read_from_cargos_staging_directory(self) -> None:
        staged = self.root / "package"
        staged.mkdir()
        (staged / "unmei-engine-0.1.0.crate").write_bytes(b"engine")
        (staged / "unmei-0.1.0.crate").write_bytes(b"cli")
        self.assertEqual(
            self.pkg.staged_crates(staged, "0.1.0"),
            [("unmei-engine-0.1.0.crate", b"engine"),
             ("unmei-0.1.0.crate", b"cli")],
        )

    def test_a_missing_crate_is_refused_rather_than_skipped(self) -> None:
        staged = self.root / "package"
        staged.mkdir()
        (staged / "unmei-engine-0.1.0.crate").write_bytes(b"engine")
        with self.assertRaises(self.pkg.Refusal) as raised:
            self.pkg.staged_crates(staged, "0.1.0")
        self.assertIn("unmei-0.1.0.crate", str(raised.exception))

    # --- normalized archive writers --------------------------------------

    def entries(self) -> list:
        return [
            self.pkg.ArchiveEntry("top/exe", 0o755, False, b"binary"),
            self.pkg.ArchiveEntry("top", 0o755, True, b""),
            self.pkg.ArchiveEntry("top/README", 0o644, False, b"read me"),
            self.pkg.ArchiveEntry("top/demo-pack", 0o755, True, b""),
            self.pkg.ArchiveEntry("top/demo-pack/pack.ron", 0o644, False, b"(pack)"),
        ]

    def test_zip_entries_are_normalized(self) -> None:
        path = self.root / "out.zip"
        self.pkg.write_zip(path, self.entries(), COMMIT_EPOCH)
        # The zip DOS timestamp stores two-second granularity, so an odd commit
        # second reads back floored. That is the format's contract, not drift.
        year, month, day, hour, minute, second = time.gmtime(COMMIT_EPOCH)[:6]
        expected_time = (year, month, day, hour, minute, second - second % 2)
        with zipfile.ZipFile(path) as archive:
            infos = archive.infolist()
            self.assertEqual(
                [info.filename for info in infos],
                ["top/", "top/README", "top/demo-pack/", "top/demo-pack/pack.ron",
                 "top/exe"],
            )
            by_name = {info.filename: info for info in infos}
            for info in infos:
                self.assertEqual(info.date_time, expected_time)
                self.assertEqual(info.create_system, 3)
            self.assertEqual(
                by_name["top/exe"].external_attr, (stat.S_IFREG | 0o755) << 16)
            self.assertEqual(
                by_name["top/README"].external_attr, (stat.S_IFREG | 0o644) << 16)
            self.assertEqual(
                by_name["top/"].external_attr, (stat.S_IFDIR | 0o755) << 16)
            self.assertEqual(archive.read("top/README"), b"read me")
            self.assertEqual(archive.read("top/exe"), b"binary")

    def test_tar_entries_are_normalized_and_gzip_header_is_fixed(self) -> None:
        path = self.root / "out.tar.gz"
        self.pkg.write_tar_gz(path, self.entries(), COMMIT_EPOCH)
        expected = self.pkg.normalized_mtime(COMMIT_EPOCH)
        with tarfile.open(path, "r:gz") as archive:
            members = archive.getmembers()
            self.assertEqual(
                [member.name for member in members],
                ["top", "top/README", "top/demo-pack", "top/demo-pack/pack.ron",
                 "top/exe"],
            )
            for member in members:
                self.assertEqual(member.mtime, expected)
                self.assertEqual((member.uid, member.gid), (0, 0))
                self.assertEqual((member.uname, member.gname), ("", ""))
            by_name = {member.name: member for member in members}
            self.assertTrue(by_name["top"].isdir())
            self.assertTrue(by_name["top/demo-pack"].isdir())
            self.assertEqual(by_name["top"].mode, 0o755)
            self.assertEqual(by_name["top/exe"].mode, 0o755)
            self.assertEqual(by_name["top/README"].mode, 0o644)
            self.assertEqual(archive.extractfile("top/README").read(), b"read me")

        raw = path.read_bytes()
        self.assertEqual(raw[:2], b"\x1f\x8b")
        self.assertEqual(raw[3] & 0x08, 0, "gzip FNAME flag set")
        self.assertEqual(int.from_bytes(raw[4:8], "little"), expected & 0xFFFFFFFF)

    def test_zip_minimum_mtime_is_clamped_to_1980(self) -> None:
        self.assertEqual(self.pkg.ZIP_MIN_MTIME, 315_532_800)
        self.assertEqual(self.pkg.normalized_mtime(0), self.pkg.ZIP_MIN_MTIME)
        self.assertEqual(self.pkg.zip_date_time(0), (1980, 1, 1, 0, 0, 0))
        path = self.root / "epoch.zip"
        self.pkg.write_zip(path, self.entries(), 0)
        with zipfile.ZipFile(path) as archive:
            self.assertEqual(archive.infolist()[0].date_time, (1980, 1, 1, 0, 0, 0))

    def test_two_builds_over_the_same_entries_are_byte_identical(self) -> None:
        first_zip = self.root / "first.zip"
        second_zip = self.root / "second.zip"
        first_tar = self.root / "first.tar.gz"
        second_tar = self.root / "second.tar.gz"
        self.pkg.write_zip(first_zip, self.entries(), COMMIT_EPOCH)
        self.pkg.write_zip(second_zip, list(reversed(self.entries())), COMMIT_EPOCH)
        self.pkg.write_tar_gz(first_tar, self.entries(), COMMIT_EPOCH)
        self.pkg.write_tar_gz(second_tar, list(reversed(self.entries())), COMMIT_EPOCH)
        self.assertEqual(first_zip.read_bytes(), second_zip.read_bytes())
        self.assertEqual(first_tar.read_bytes(), second_tar.read_bytes())
        with gzip.open(first_tar, "rb") as handle:
            self.assertTrue(handle.read())


@unittest.skipIf(tomllib is None, "reading the manifests needs tomllib")
class CrateIdentityTests(unittest.TestCase):
    """Every workspace member names one repository and no other identity."""

    def member_manifests(self) -> dict[str, str]:
        workspace = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
        members = workspace["workspace"]["members"]
        return {
            f"{member}/Cargo.toml": (ROOT / member / "Cargo.toml").read_text(
                encoding="utf-8"
            )
            for member in members
        }

    def test_every_member_carries_the_repository_and_no_other_identity(self) -> None:
        manifests = self.member_manifests()
        # The app is a member and was the one without a `repository` line, so a
        # regression that drops it again has to fail here.
        self.assertIn("app/Cargo.toml", manifests)
        self.assertEqual(manifest_identity_findings(manifests), [])

    def test_the_findings_name_a_missing_repository_and_an_extra_field(self) -> None:
        token = f'[package]\nname = "unmei-engine"\nrepository = "{REPOSITORY_TOKEN}"\n'
        filled = '[package]\nname = "unmei"\nrepository = "https://example.test/unmei"\n'
        for accepted in (token, filled):
            self.assertEqual(manifest_identity_findings({"a/Cargo.toml": accepted}), [])

        self.assertEqual(
            manifest_identity_findings({"a/Cargo.toml": '[package]\nname = "unmei"\n'}),
            ["a/Cargo.toml: no package.repository"],
        )
        self.assertEqual(
            manifest_identity_findings(
                {"a/Cargo.toml": token + 'homepage = "https://example.test"\n'}
            ),
            ["a/Cargo.toml: package.homepage is set"],
        )
        self.assertEqual(
            manifest_identity_findings(
                {"a/Cargo.toml": '[package]\nrepository = "own repo"\n'}
            ),
            ['a/Cargo.toml: package.repository names no repository: \'own repo\''],
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)
