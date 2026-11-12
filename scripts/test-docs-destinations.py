#!/usr/bin/env python3
"""Generation refuses any index destination a link could redirect.

An active index is written at a repository-relative path. If that path, or a
parent component below the resolved repository root, is a symbolic link or a
Windows junction, the write lands in another file. Every active destination is
preflighted before the first write, so a later unsafe destination cannot leave
an earlier index changed.

The fixtures use real filesystem links, read sentinels with `read_bytes()` and
compare `git ls-files --stage` directly: the fixture tree-state helper follows
links, so it cannot be the evidence that a linked destination was untouched.
The parent-component cases run twice, once with a symbolic link and once with a
junction, so each kind is exercised wherever the platform can create it.
"""

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

import docs_test_support as support


def valid(title, summary="A fixture record.", keywords="prototype", extra=""):
    return support.document(title, summary, keywords, extra)


def symlink_or_skip(testcase, link, target, directory=False):
    """A real symbolic link, or a skip carrying the platform's own error text."""
    try:
        os.symlink(target, link, target_is_directory=directory)
    except (OSError, NotImplementedError) as exc:
        testcase.skipTest("this platform cannot create a symbolic link here: %s" % exc)


def junction_or_skip(testcase, link, target):
    """A real Windows junction, or a skip carrying the platform's error text.

    Windows needs a privilege for `os.symlink` (WinError 1314) but not for
    `mklink /J`; elsewhere `cmd` is absent, so the junction case skips there.
    """
    try:
        completed = subprocess.run(
            ["cmd", "/c", "mklink", "/J", str(link), str(target)],
            capture_output=True, text=True)
    except OSError as exc:
        testcase.skipTest("this platform cannot create a junction: %s" % exc)
    if completed.returncode != 0:
        testcase.skipTest("this platform cannot create a junction: %s"
                          % (completed.stderr or completed.stdout).strip())


class DestinationCase(unittest.TestCase):
    def setUp(self):
        # The fixture activates the shipped `docs/adr` group, so the indexes it
        # builds are the ones this tree's own policy indexes.
        self.repo = support.TempRepository(
            self, support.policy(active_directories=["docs/adr"]))
        self.repo.write("README.md", "# Readme\n")
        self.repo.write("docs/adr/0001-a.md", valid("Alpha"))
        self.repo.write("docs/adr/0002-b.md", valid("Beta"))
        self.repo.commit()
        self.assertPasses(self.repo.generate())
        self.repo.commit("index")

    def outside(self):
        """A directory outside the repository, removed before the repository is."""
        holder = tempfile.TemporaryDirectory()
        self.addCleanup(holder.cleanup)
        return Path(holder.name)

    def pointer(self):
        return self.repo.root / "docs/adr/INDEX.md"

    def stale_indexes(self, directory):
        """Overwrite every index under `directory` with a marker the test compares."""
        stale = {}
        for index in sorted(directory.rglob("INDEX.md")):
            name = index.relative_to(directory).as_posix()
            stale[name] = ("stale index at %s\n" % name).encode("utf-8")
            index.write_bytes(stale[name])
        return stale

    def assertStale(self, directory, stale):
        for name, marker in stale.items():
            self.assertEqual((directory / name).read_bytes(), marker,
                             "a write followed the link to %s" % name)

    def assertPasses(self, result):
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def assertRefused(self, result, *paths):
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        for path in paths:
            self.assertIn(path, result.stderr)
        self.assertIn("fix:", result.stderr)
        self.assertIn("nothing was written.", result.stderr)


class LinkFixture:
    """Builds the directory links the parent-component cases create."""

    def make_link(self, link, target):
        raise NotImplementedError

    def replace_with_link(self, relative, target):
        """`relative` stops being a real directory and becomes a link to `target`."""
        real = self.repo.root.joinpath(*relative.split("/"))
        shutil.rmtree(real)
        self.make_link(real, target)

    def archive(self, directory):
        """A copy of `directory` under the repository root, with stale indexes."""
        copy = self.repo.root / (directory + "-archive")
        shutil.copytree(self.repo.root.joinpath(*directory.split("/")), copy)
        return copy, self.stale_indexes(copy)


class SymlinkFixture(LinkFixture):
    def make_link(self, link, target):
        symlink_or_skip(self, link, target, directory=True)


class JunctionFixture(LinkFixture):
    def make_link(self, link, target):
        junction_or_skip(self, link, target)


class DestinationFileTests(DestinationCase):
    """An active index that is itself a link must not be written."""

    def test_an_outside_sentinel_survives_a_linked_index(self):
        sentinel = self.outside() / "sentinel.md"
        sentinel.write_bytes(b"outside sentinel\n")
        self.pointer().unlink()
        symlink_or_skip(self, self.pointer(), sentinel)
        staged = self.repo.git("ls-files", "--stage")

        self.assertRefused(self.repo.generate(), "docs/adr/INDEX.md")

        self.assertEqual(sentinel.read_bytes(), b"outside sentinel\n")
        self.assertEqual(self.repo.git("ls-files", "--stage"), staged)

    def test_an_inside_tracked_file_survives_a_linked_index(self):
        target = self.repo.root / "docs/adr/0002-b.md"
        before = target.read_bytes()
        self.pointer().unlink()
        symlink_or_skip(self, self.pointer(), "0002-b.md")
        staged = self.repo.git("ls-files", "--stage")

        self.assertRefused(self.repo.generate(), "docs/adr/INDEX.md")

        self.assertEqual(target.read_bytes(), before)
        self.assertEqual(self.repo.git("ls-files", "--stage"), staged)

    def test_a_dangling_linked_index_is_rejected(self):
        absent = self.outside() / "absent.md"
        self.pointer().unlink()
        symlink_or_skip(self, self.pointer(), absent)

        self.assertRefused(self.repo.generate(), "docs/adr/INDEX.md")

        self.assertTrue(self.pointer().is_symlink(), "the link was replaced")
        self.assertFalse(absent.exists(), "the write created the link target")


class LinkedParentCases(LinkFixture):
    """A parent component below the resolved root that is a link must be refused."""

    def test_a_linked_parent_inside_the_repository_is_rejected(self):
        archive, stale = self.archive("docs/adr")
        self.replace_with_link("docs/adr", archive)
        staged = self.repo.git("ls-files", "--stage")

        result = self.repo.generate()

        self.assertRefused(result, "docs/adr/INDEX.md")
        self.assertIn("real directory", result.stderr)
        self.assertStale(archive, stale)
        self.assertEqual(self.repo.git("ls-files", "--stage"), staged)

    def test_a_staged_and_committed_generation_does_not_write_through_a_linked_parent(self):
        outside = self.outside() / "adr"
        shutil.copytree(self.repo.root / "docs/adr", outside)
        stale = self.stale_indexes(outside)
        self.replace_with_link("docs/adr", outside)
        staged = self.repo.git("ls-files", "--stage")

        for args in (("--staged",), ("--checkout",)):
            with self.subTest(selector=args):
                self.assertRefused(self.repo.generate(*args), "docs/adr/INDEX.md")
                self.assertStale(outside, stale)
        self.assertEqual(self.repo.git("ls-files", "--stage"), staged)


class SymlinkedParentTests(SymlinkFixture, LinkedParentCases, DestinationCase):
    pass


class JunctionParentTests(JunctionFixture, LinkedParentCases, DestinationCase):
    pass


class BatchCases(LinkFixture):
    """The whole destination set is checked before the first write."""

    def activate_two_directories(self):
        two = support.policy(active_directories=["docs", "docs/adr"],
                             indexed_directories=["docs", "docs/adr"],
                             section_links={}, excluded_landing_links={})
        self.repo.write_policy(two)
        self.repo.commit("two active directories")
        self.assertPasses(self.repo.generate())
        self.repo.commit("both indexes")

    def test_an_earlier_safe_index_is_not_written_when_a_later_destination_is_linked(self):
        self.activate_two_directories()
        self.repo.write("docs/INDEX.md", "an older index\n")
        archive, stale = self.archive("docs/adr")
        self.replace_with_link("docs/adr", archive)
        staged = self.repo.git("ls-files", "--stage")
        status = self.repo.git("status", "--porcelain")

        self.assertRefused(self.repo.generate(), "docs/adr/INDEX.md")

        self.assertEqual(self.repo.read("docs/INDEX.md"), b"an older index\n")
        self.assertStale(archive, stale)
        self.assertEqual(self.repo.git("ls-files", "--stage"), staged)
        self.assertEqual(self.repo.git("status", "--porcelain"), status)

    def test_every_offending_destination_is_reported(self):
        self.activate_two_directories()
        archive, stale = self.archive("docs")
        self.replace_with_link("docs", archive)

        self.assertRefused(self.repo.generate(), "docs/INDEX.md", "docs/adr/INDEX.md")

        self.assertEqual(sorted(stale), ["INDEX.md", "adr/INDEX.md"])
        self.assertStale(archive, stale)


class SymlinkedBatchTests(SymlinkFixture, BatchCases, DestinationCase):
    pass


class JunctionBatchTests(JunctionFixture, BatchCases, DestinationCase):
    pass


if __name__ == "__main__":
    unittest.main()
