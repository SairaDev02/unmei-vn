#!/usr/bin/env python3
"""Snapshot readers see the candidate tree, not the developer's worktree.

Each test builds a real temporary Git repository, as
`scripts/test-staged-token-limit.py` does, because the staged snapshot only
exists in Git's index.
"""

import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import docs_snapshot
from docs_snapshot import (GitTree, LinkError, MAX_LINK_DEPTH, SnapshotError, StagedIndex,
                           Worktree, follow_links)


class RepositoryTest(unittest.TestCase):
    def setUp(self):
        self.tempdir = tempfile.TemporaryDirectory()
        self.addCleanup(self.tempdir.cleanup)
        self.root = Path(self.tempdir.name)
        self.env = os.environ.copy()
        for key in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_COMMON_DIR"):
            self.env.pop(key, None)
        self.git("init", "-q")
        self.git("config", "user.name", "Test")
        self.git("config", "user.email", "test@example.org")
        self.git("config", "core.autocrlf", "false")

    def git(self, *args):
        return subprocess.run(["git", *args], cwd=self.root, env=self.env, check=True,
                              capture_output=True, text=True).stdout

    def write(self, name, text, newline="\n"):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(text.replace("\n", newline).encode("utf-8"))

    def commit(self, message="fixture"):
        self.git("add", "-A")
        self.git("commit", "-qm", message)

    def recording_git(self):
        """Record every `git` process the reader starts, then run it for real."""
        calls = []
        real = subprocess.run

        def recording(args, *rest, **kwargs):
            calls.append(list(args))
            return real(args, *rest, **kwargs)

        patcher = mock.patch.object(docs_snapshot.subprocess, "run", recording)
        patcher.start()
        self.addCleanup(patcher.stop)
        return calls


class WorktreeTests(RepositoryTest):
    def test_reads_tracked_markdown_only(self):
        self.write("docs/adr/0001-a.md", "# A\n")
        self.write("docs/adr/data.txt", "ignored\n")
        self.write("notes.md.orig", "ignored\n")
        self.commit()
        self.write("docs/adr/untracked.mdx", "# Untracked\n")
        snapshot = Worktree(self.root)
        self.assertEqual(snapshot.paths(), ["docs/adr/0001-a.md"])
        self.assertEqual(snapshot.label, "worktree")

    def test_reads_the_file_on_disk(self):
        self.write("docs/adr/0001-a.md", "# A\n")
        self.commit()
        self.write("docs/adr/0001-a.md", "# Edited but not staged\n")
        self.assertEqual(Worktree(self.root).read("docs/adr/0001-a.md"),
                         b"# Edited but not staged\n")

    def test_a_crlf_checkout_parses_as_lf_but_reads_as_exact_bytes(self):
        self.write("docs/adr/0001-a.md", "line one\nline two\n")
        self.commit()
        self.write("docs/adr/0001-a.md", "line one\nline two\n", newline="\r\n")
        snapshot = Worktree(self.root)
        self.assertEqual(snapshot.text("docs/adr/0001-a.md"), "line one\nline two\n")
        self.assertIn(b"\r\n", snapshot.read("docs/adr/0001-a.md"))

    def test_a_tracked_file_missing_from_the_worktree_fails_loudly(self):
        self.write("docs/adr/0001-a.md", "# A\n")
        self.commit()
        (self.root / "docs/adr/0001-a.md").unlink()
        with self.assertRaises(SnapshotError) as caught:
            Worktree(self.root).read("docs/adr/0001-a.md")
        self.assertIn("docs/adr/0001-a.md", str(caught.exception))

    def test_paths_outside_the_repository_are_rejected(self):
        self.write("docs/adr/0001-a.md", "# A\n")
        self.commit()
        with self.assertRaises(SnapshotError):
            Worktree(self.root).read("../outside.md")


class StagedIndexTests(RepositoryTest):
    def test_reads_the_staged_blob_not_the_worktree(self):
        self.write("docs/adr/0001-a.md", "first commit\n")
        self.commit()
        self.write("docs/adr/0001-a.md", "staged summary\n")
        self.git("add", "docs/adr/0001-a.md")
        self.write("docs/adr/0001-a.md", "later worktree edit\n")
        snapshot = StagedIndex(self.root)
        self.assertEqual(snapshot.read("docs/adr/0001-a.md"), b"staged summary\n")
        self.assertEqual(Worktree(self.root).read("docs/adr/0001-a.md"),
                         b"later worktree edit\n")
        self.assertEqual(snapshot.label, "index")

    def test_a_partially_staged_file_reads_only_what_is_staged(self):
        self.write("docs/adr/0001-a.md", "one\ntwo\n")
        self.commit()
        self.write("docs/adr/0001-a.md", "one\nCHANGED\n")
        self.git("add", "docs/adr/0001-a.md")
        self.write("docs/adr/0001-a.md", "one\nCHANGED\nAND MORE\n")
        self.assertEqual(StagedIndex(self.root).read("docs/adr/0001-a.md"),
                         b"one\nCHANGED\n")

    def test_a_staged_rename_drops_the_old_path(self):
        self.write("docs/adr/0001-a.md", "# A\n")
        self.commit()
        self.git("mv", "docs/adr/0001-a.md", "docs/adr/0002-b.md")
        self.assertEqual(StagedIndex(self.root).paths(), ["docs/adr/0002-b.md"])

    def test_a_staged_deletion_removes_the_path(self):
        self.write("docs/adr/0001-a.md", "# A\n")
        self.write("docs/adr/0002-b.md", "# B\n")
        self.commit()
        self.git("rm", "-q", "docs/adr/0001-a.md")
        self.assertEqual(StagedIndex(self.root).paths(), ["docs/adr/0002-b.md"])

    def test_the_whole_markdown_corpus_is_read_in_one_batched_process(self):
        for index in range(10):
            self.write("docs/adr/%04d-x.md" % index, "# %d\n" % index)
        self.commit()
        snapshot = StagedIndex(self.root)
        calls = self.recording_git()
        for path in snapshot.paths():
            snapshot.read(path)
        self.assertEqual(calls, [["git", "cat-file", "--batch"]])

    def test_an_unmerged_entry_fails_loudly(self):
        self.write("docs/adr/0001-a.md", "base\n")
        self.commit()
        self.git("checkout", "-q", "-b", "other")
        self.write("docs/adr/0001-a.md", "other\n")
        self.commit("other")
        self.git("checkout", "-q", "-")
        self.write("docs/adr/0001-a.md", "main\n")
        self.commit("main")
        subprocess.run(["git", "merge", "other"], cwd=self.root, env=self.env,
                       capture_output=True)
        with self.assertRaises(SnapshotError) as caught:
            StagedIndex(self.root).paths()
        self.assertIn("unmerged", str(caught.exception))


class GitTreeTests(RepositoryTest):
    def test_reads_the_committed_blob(self):
        self.write("docs/adr/0001-a.md", "committed\n")
        self.commit()
        self.write("docs/adr/0001-a.md", "worktree\n")
        snapshot = GitTree(self.root, "HEAD")
        self.assertEqual(snapshot.read("docs/adr/0001-a.md"), b"committed\n")
        self.assertEqual(snapshot.label, "HEAD")

    def test_a_path_absent_from_the_tree_is_absent(self):
        self.write("docs/adr/0001-a.md", "committed\n")
        self.commit()
        self.assertEqual(GitTree(self.root, "HEAD").paths(), ["docs/adr/0001-a.md"])
        with self.assertRaises(SnapshotError):
            GitTree(self.root, "HEAD").read("docs/adr/0002-b.md")

    def test_the_whole_markdown_corpus_is_read_in_one_batched_process(self):
        for index in range(10):
            self.write("docs/adr/%04d-x.md" % index, "# %d\n" % index)
        self.commit()
        snapshot = GitTree(self.root, "HEAD")
        calls = self.recording_git()
        for path in snapshot.paths():
            snapshot.read(path)
        self.assertEqual(calls, [["git", "cat-file", "--batch"]])


class BlobReaderTests(unittest.TestCase):
    def test_both_git_snapshots_share_one_blob_reader(self):
        """The index and a committed tree differ in their entries, not in reading them."""
        self.assertIs(StagedIndex._blob, GitTree._blob)
        self.assertIs(StagedIndex.read, GitTree.read)


class LinkTests(RepositoryTest):
    """A symlink is tree content: mode `120000` in Git, a real link on disk."""

    def stage_link(self, name, target):
        """Record a symlink in the index, which needs no platform symlink support."""
        holder = self.root / "link-target.txt"
        holder.write_bytes(target.encode("utf-8"))
        blob = self.git("hash-object", "-w", "link-target.txt").strip()
        self.git("update-index", "--add", "--cacheinfo", "120000,%s,%s" % (blob, name))
        holder.unlink()

    def test_the_index_reports_a_link_target_from_its_blob(self):
        self.write("docs/adr/0001-a.md", "# A\n")
        self.stage_link("docs/adr/alias", "0001-a.md")
        snapshot = StagedIndex(self.root)
        self.assertEqual(snapshot.link_target("docs/adr/alias"), "0001-a.md")
        self.assertIsNone(snapshot.link_target("docs/adr/0001-a.md"))

    def test_a_committed_tree_reports_a_link_target(self):
        self.write("docs/adr/0001-a.md", "# A\n")
        self.git("add", "docs/adr/0001-a.md")
        self.stage_link("docs/adr/alias", "0001-a.md")
        self.git("commit", "-qm", "symlink")
        snapshot = GitTree(self.root, "HEAD")
        self.assertIn("docs/adr/alias", snapshot.tracked())
        self.assertEqual(snapshot.link_target("docs/adr/alias"), "0001-a.md")

    def test_a_worktree_symlink_reports_its_target(self):
        self.write("docs/adr/0001-a.md", "# A\n")
        self.commit()
        try:
            os.symlink("0001-a.md", self.root / "docs/adr/alias")
        except (OSError, NotImplementedError) as exc:
            self.skipTest("this platform cannot create a symlink: %s" % exc)
        self.assertEqual(Worktree(self.root).link_target("docs/adr/alias"),
                         "0001-a.md")
        self.assertIsNone(Worktree(self.root).link_target("docs/adr/0001-a.md"))


class FollowLinkTests(unittest.TestCase):
    """`related` resolution follows links, and stops a cycle or an over-deep chain."""

    class Links:
        def __init__(self, links):
            self.links = links

        def link_target(self, path):
            return self.links.get(path)

    def test_a_link_inside_the_repository_resolves(self):
        self.assertEqual(follow_links(self.Links({"docs/a": "sub/b.md"}), "docs", "a"),
                         "docs/sub/b.md")

    def test_a_cycle_is_reported_instead_of_looping(self):
        with self.assertRaises(LinkError) as caught:
            follow_links(self.Links({"docs/a": "b", "docs/b": "a"}), "docs", "a")
        self.assertIn("never terminates", str(caught.exception))

    def test_a_chain_deeper_than_the_cap_is_reported(self):
        links = {"docs/l%d" % index: "l%d" % (index + 1)
                 for index in range(MAX_LINK_DEPTH + 1)}
        with self.assertRaises(LinkError) as caught:
            follow_links(self.Links(links), "docs", "l0")
        self.assertIn("follows more than %d symlinks" % MAX_LINK_DEPTH,
                      str(caught.exception))


if __name__ == "__main__":
    unittest.main()
