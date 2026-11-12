#!/usr/bin/env python3
"""Every tracked Markdown path gets exactly one coverage class.

The rules are the narrow, listed ones in the coverage disposition table. Each case
here fails if a blanket directory rule is substituted for an explicit list, or
if a Markdown-shaped file is skipped instead of reported. The rules are exercised
against a fixture policy, and coverage of this repository's own tree is asserted
by the private suite that reads this repository's overlay, as are the two classes
whose own paths are private: a skill contract and a rejected-enhancement record.
"""

import unittest

import docs_test_support as support
from docs_policy import classification_problem, evidence_root_problem
from docs_snapshot import classify, coverage_classes, in_markdown_scope


class ClassificationTests(unittest.TestCase):
    def setUp(self):
        self.policy = support.load_policy()

    def assertClass(self, path, kind, owner=None):
        result = classify(path, self.policy)
        self.assertEqual(result.kind, kind, path)
        if owner is not None:
            self.assertEqual(result.owner, owner, path)

    def test_generated_index_belongs_to_its_directory(self):
        for directory in self.policy["indexed_directories"]:
            self.assertClass(directory + "/INDEX.md", "generated", directory)

    def test_direct_document_of_an_indexed_directory_is_ordinary(self):
        self.assertClass("docs/adr/0012-menu-and-live-session-transition-ownership.md",
                         "ordinary", "docs/adr")

    def test_a_document_one_level_below_an_indexed_directory_is_not_ordinary(self):
        self.assertClass("docs/adr/notes/0012-x.md", "unclassified")

    def test_an_index_name_outside_an_indexed_directory_is_not_generated(self):
        self.assertClass("app/INDEX.md", "unclassified")

    def test_root_guidance_is_only_the_listed_files(self):
        for name in ("README.md", "CONTRIBUTING.md", "SECURITY.md", "GLOSSARY.md",
                     "LANGUAGE.md"):
            self.assertClass(name, "root-guidance")
        self.assertClass("ARCHITECTURE.md", "unclassified")

    def test_local_only_and_removed_root_files_are_not_root_guidance(self):
        # A scratch note stays local and git-ignored; CODE_OF_CONDUCT.md was
        # removed. Neither is a tracked document, so tracking either must fail
        # coverage.
        for name in ("NOTES.md", "CODE_OF_CONDUCT.md"):
            self.assertClass(name, "unclassified")

    def test_only_the_listed_template_is_a_template(self):
        self.assertClass(".github/PULL_REQUEST_TEMPLATE.md", "template")
        self.assertClass(".github/ISSUE_TEMPLATE.md", "unclassified")

    def test_exploration_records_are_listed_individually(self):
        """A record outside the configured list is not an exploration record."""
        changed = support.policy()
        changed["classifications"]["exploration"] = ["map/Principles.md"]
        self.assertEqual(classify("map/Principles.md", changed).kind, "exploration")
        self.assertEqual(classify("map/Backlog.md", changed).kind, "unclassified")

    def test_asset_notes_and_pack_content_are_listed_individually(self):
        self.assertClass("app/assets/fonts/README.md", "asset-note")
        self.assertClass("app/assets/fonts/LICENSE.md", "unclassified")
        self.assertClass("examples/demo-pack/pack.glossary.md", "pack-content")
        self.assertClass("examples/demo-pack/assets/README.md", "pack-content")

    def test_crate_readmes_are_listed_individually(self):
        self.assertClass("engine/README.md", "crate-readme")
        self.assertClass("cli/README.md", "crate-readme")
        self.assertClass("app/README.md", "unclassified")
        self.assertClass("engine/docs/README.md", "unclassified")

    def test_archive_readmes_are_listed_individually(self):
        self.assertClass("packaging/player-README.md", "archive-readme")
        self.assertClass("packaging/author-README.md", "archive-readme")
        self.assertClass("packaging/LICENSE.md", "unclassified")
        self.assertClass("packaging/notes/README.md", "unclassified")

    def test_evidence_inherits_from_a_registered_root(self):
        changed = support.policy()
        changed["classifications"]["evidence_roots"] = ["records/receipt"]
        owned = classify("records/receipt/README.md", changed)
        self.assertEqual((owned.kind, owned.owner), ("evidence", "records/receipt"))
        self.assertEqual(classify("records/other/README.md", changed).kind,
                         "unclassified")

    def test_evidence_without_a_registered_root_is_unclassified(self):
        changed = support.policy()
        changed["classifications"]["evidence_roots"] = ["records/known"]
        self.assertEqual(classify("records/unknown/README.md", changed).kind,
                         "unclassified")
        self.assertEqual(classify("records/README.md", changed).kind, "unclassified")

    def test_the_deepest_registered_root_owns_the_path(self):
        changed = support.policy()
        changed["classifications"]["evidence_roots"] = ["records/x",
                                                       "records/x/deep"]
        owned = classify("records/x/deep/NOTES.md", changed)
        self.assertEqual(owned.kind, "evidence")
        self.assertEqual(owned.owner, "records/x/deep")

    def test_duplicate_evidence_roots_are_a_policy_error(self):
        broken = support.policy()
        broken["classifications"]["evidence_roots"] = ["records/x", "records/x"]
        self.assertIn("records/x", evidence_root_problem(broken))

    def test_an_unnormalised_evidence_root_is_a_policy_error(self):
        for root in ("records/x/", "/records/x", "records//x"):
            with self.subTest(root=root):
                broken = support.policy()
                broken["classifications"]["evidence_roots"] = [root]
                self.assertIsNotNone(evidence_root_problem(broken))
        self.assertIsNone(evidence_root_problem(self.policy))


class AmbiguityTests(unittest.TestCase):
    """Exactly one classification per path: a second class is reported, never ignored."""

    def setUp(self):
        self.policy = support.load_policy()

    def test_a_path_in_two_explicit_lists_is_a_policy_error(self):
        broken = support.policy()
        broken["classifications"]["exploration"] = ["examples/demo-pack/pack.glossary.md"]
        problem = classification_problem(broken)
        self.assertIn("examples/demo-pack/pack.glossary.md", problem)
        self.assertIn("exploration", problem)
        self.assertIn("pack_content", problem)
        self.assertIsNone(classification_problem(self.policy))

    def test_every_matching_class_is_gathered(self):
        changed = support.policy()
        changed["classifications"]["templates"] = ["docs/adr/0012-a.md"]
        kinds = [item.kind for item in coverage_classes("docs/adr/0012-a.md", changed)]
        self.assertEqual(kinds, ["ordinary", "template"])
        self.assertEqual([item.kind for item in
                          coverage_classes("docs/adr/0012-a.md", self.policy)],
                         ["ordinary"])


class MarkdownScopeTests(unittest.TestCase):
    def setUp(self):
        self.policy = support.load_policy()

    def test_other_markdown_extensions_are_in_scope(self):
        for path in ("notes/a.mdx", "notes/a.markdown", "docs/adr/0012-a.md"):
            self.assertTrue(in_markdown_scope(path), path)

    def test_a_markdown_shaped_extension_is_reported_not_skipped(self):
        self.assertEqual(classify("notes/a.mdx", self.policy).kind, "unclassified")
        self.assertEqual(classify("docs/adr/0012-a.mdx", self.policy).kind, "unclassified")

    def test_a_non_markdown_file_is_out_of_scope(self):
        for path in ("docs/adr/data.txt", "scripts/docs.py", "GLOSSARY"):
            self.assertFalse(in_markdown_scope(path), path)


if __name__ == "__main__":
    unittest.main()
