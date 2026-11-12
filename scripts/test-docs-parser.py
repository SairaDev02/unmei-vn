#!/usr/bin/env python3
"""The approved 88-case corpus must pass against the production parser.

The expectations are not written here: they are the corpus copied to
`test_docs_frontmatter_cases.py` from the evidence bundle that froze it under the
document-metadata contract. Each case names the rule it exercises, so a failure
points at a documented rule rather than a guess. The pinned copy is this suite's only input,
so the suite runs in a tree that holds only the exported files; the private suite
holds the comparison with the evidence bundle.
"""

import unittest

import docs_test_support as support
import test_docs_frontmatter_cases as corpus
from docs_frontmatter import parse_frontmatter

# The corpus was authored against two owning-directory depths: everything sits in
# `docs/adr/`, except S20, which is a document directly in `docs/` so that
# `../../outside.md` escapes the repository root.
OWNER = {"S20-related-escape": "docs"}


def run_case(case):
    policy = support.load_policy()
    if case.get("relax_allowlist"):
        # The frozen corpus's syntax-only cases quote a term the closed vocabulary
        # does not hold. The corpus file is pinned by SHA-256 and cannot carry the
        # flag into the parser, so the policy for that case admits the terms its
        # own expectation accepts and every other rule still applies.
        policy = dict(policy, keyword_allowlist=list(policy["keyword_allowlist"])
                      + list(case["expect"]["values"]["keywords"]))
    return parse_frontmatter(
        case["text"], policy, owner_dir=OWNER.get(case["id"], "docs/adr"))


class CorpusTests(unittest.TestCase):
    def test_the_corpus_holds_88_cases_and_none_pending(self):
        self.assertEqual(len(corpus.CASES), 88)
        self.assertEqual(corpus.PENDING, [])

    def test_every_case_meets_its_expectation(self):
        for case in corpus.CASES:
            with self.subTest(case=case["id"]):
                result = run_case(case)
                expected = case["expect"]
                if expected["accept"]:
                    self.assertEqual(result.errors, [], case["rule"])
                    self.assertEqual(result.values, expected["values"], case["rule"])
                    continue
                same_stage = [e for e in result.errors if e.stage == expected["stage"]]
                self.assertTrue(same_stage, "%s: %s" % (case["rule"], result.errors))
                self.assertIn(expected["field"], {e.field for e in same_stage}, case["rule"])
                if expected.get("line") is not None:
                    self.assertIn(expected["line"], {e.line for e in same_stage}, case["rule"])


class EmptyListTests(unittest.TestCase):
    """A bare list key names the field it left empty."""

    def test_a_bare_keywords_line_names_the_field(self):
        result = parse_frontmatter(
            "---\ntitle: Bare\nsummary: A bare list key.\nkeywords:\n---\n",
            support.load_policy(), owner_dir="docs/adr")
        self.assertEqual([(error.stage, error.field, error.reason) for error in result.errors],
                         [("syntax", "keywords", "empty-list-value")])

    def test_an_empty_scalar_value_is_still_an_empty_value(self):
        """`summary: ` with nothing after the colon keeps the scalar diagnostic."""
        result = parse_frontmatter(
            "---\ntitle: Bare\nsummary: \nkeywords:\n  - ci\n---\n",
            support.load_policy(), owner_dir="docs/adr")
        self.assertEqual([(error.field, error.reason) for error in result.errors],
                         [("summary", "empty-value")])


class PolicyVocabularyTests(unittest.TestCase):
    def test_the_policy_keyword_allowlist_is_the_approved_vocabulary(self):
        self.assertEqual(support.load_policy()["keyword_allowlist"], corpus.APPROVED)

    def test_the_policy_fields_and_required_keys_match_the_schema(self):
        policy = support.load_policy()
        self.assertEqual(set(policy["fields"]), corpus.ALLOWED)
        self.assertEqual(policy["required"], ["title", "summary", "keywords"])
        self.assertEqual(policy["summary_max_scalars"], 160)


if __name__ == "__main__":
    unittest.main()
