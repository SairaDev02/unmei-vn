#!/usr/bin/env python3
"""The exported policy is complete, and this repository adds its entries in an overlay.

The exported `scripts/docs-policy.json` ships as it stands, so it must hold no
entry only this private tree can satisfy. This repository's extra entries live in
an overlay beside the export tooling, which the allowlist excludes. These cases
prove both halves: the merge adds the overlay's entries and never removes one, an
absent overlay leaves the exported policy untouched, and the tooling reads the
overlay from whichever snapshot it was told to read.
"""

import json
import unittest

import docs_policy
import docs_test_support as support

# Fixture entries, not this repository's: the merge is what these cases prove.
BASE = {
    "indexed_directories": ["docs"],
    "classifications": {"root_guidance": ["README.md"]},
}
OVERLAY = {
    "indexed_directories": ["docs/notes"],
    "classifications": {
        "root_guidance": ["HANDOFF.md"],
        "evidence_roots": ["docs/receipts/x"],
    },
}


class MergeTests(unittest.TestCase):
    def test_an_absent_overlay_returns_the_policy_unchanged(self):
        self.assertEqual(docs_policy.merge_policy_overlay(BASE, None), BASE)

    def test_a_list_entry_is_unioned_in_order_without_duplicates(self):
        merged = docs_policy.merge_policy_overlay(BASE, OVERLAY)
        self.assertEqual(merged["indexed_directories"], ["docs", "docs/notes"])
        again = docs_policy.merge_policy_overlay(
            merged, {"indexed_directories": ["docs", "docs/notes"]})
        self.assertEqual(again["indexed_directories"], ["docs", "docs/notes"])

    def test_a_map_entry_merges_key_by_key(self):
        merged = docs_policy.merge_policy_overlay(BASE, OVERLAY)
        self.assertEqual(merged["classifications"]["root_guidance"],
                         ["README.md", "HANDOFF.md"])
        self.assertEqual(merged["classifications"]["evidence_roots"], ["docs/receipts/x"])

    def test_the_merge_never_removes_an_entry(self):
        merged = docs_policy.merge_policy_overlay(BASE, OVERLAY)
        for key, value in BASE.items():
            with self.subTest(key=key):
                self.assertIn(key, merged)
        self.assertIn("README.md", merged["classifications"]["root_guidance"])

    def test_the_merge_does_not_change_the_documents_it_was_given(self):
        before = json.loads(json.dumps(BASE)), json.loads(json.dumps(OVERLAY))
        docs_policy.merge_policy_overlay(BASE, OVERLAY)
        self.assertEqual((BASE, OVERLAY), before)


class SnapshotTests(unittest.TestCase):
    """The overlay is read from the chosen snapshot, so a committed one is honoured."""

    # The base policy indexes nothing, so only the overlay can classify the
    # fixture document: the fixture passes exactly when the merge happens.
    FIXTURE_OVERLAY = {
        "indexed_directories": ["docs/notes"],
        "active_directories": ["docs/notes"],
    }

    def repository(self, overlay):
        base = support.policy(indexed_directories=[], active_directories=[])
        repo = support.TempRepository(self, policy_document=base)
        repo.write("docs/notes/0001-a.md",
                   support.document("Alpha", "A private record.", "prototype"))
        if overlay is not None:
            repo.write(docs_policy.PRIVATE_POLICY_PATH, json.dumps(overlay, indent=2) + "\n")
        repo.commit()
        return repo

    def test_the_worktree_policy_merges_the_overlay_beside_it(self):
        repo = self.repository(self.FIXTURE_OVERLAY)
        self.assertEqual(repo.check().returncode, 1, "without an index the check fails")
        self.assertEqual(repo.generate().returncode, 0)
        result = repo.check()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_a_committed_overlay_is_read_from_the_checkout_snapshot(self):
        repo = self.repository(self.FIXTURE_OVERLAY)
        self.assertEqual(repo.generate().returncode, 0)
        repo.commit("index")
        result = repo.check("--checkout")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_a_tree_without_the_overlay_is_still_a_valid_policy(self):
        repo = self.repository(None)
        result = repo.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn("docs/notes/0001-a.md", result.stderr)
        self.assertIn("unclassified", result.stderr)
        self.assertNotIn(docs_policy.PRIVATE_POLICY_PATH, result.stderr)


if __name__ == "__main__":
    unittest.main()
