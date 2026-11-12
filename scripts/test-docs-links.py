#!/usr/bin/env python3
"""Reader-facing documents must not link to a path or anchor that is absent.

Specification: the link-check scope the documentation contract decides. The scope
is `docs/guide/*.md`, `README.md` and `docs/architecture.md`; every other
document stays out of scope, so a broken link elsewhere cannot start failing the
gate.
Absolute URLs and `@@…@@` placeholders are skipped: CI has no network, and the
export fills the placeholders after this repository is copied.

Each case drives `scripts/docs.py check` against a throwaway repository, so it
asserts the exit status and the diagnostic a reader gets, not the checker's
internals. The link check belongs to `check`: `generate` must not run it.
"""

import unittest

import docs_test_support as support

# `## Crates` is an anchor; the `#` lines inside the fenced block are shell
# comments, so `#not-a-heading` must not resolve.
ARCHITECTURE = (
    "# Architecture\n"
    "\n"
    "## Crates\n"
    "\n"
    "The engine stays free of Bevy.\n"
    "\n"
    "```sh\n"
    "# Not a heading\n"
    "cargo run -p unmei-app -- examples/demo-pack\n"
    "```\n"
)

GUIDE_PAGE = support.document(
    "Play the demo", "Choosing an archive and starting the demo.", "content-pack"
) + "\nThe [pack layout](../content-format.md#1-command-surface) is the contract.\n"

README = (
    "# Readme\n"
    "\n"
    "[Content format](docs/content-format.md#1-command-surface)\n"
    "[Glossary](GLOSSARY.md)\n"
    "[Demo pack](examples/demo-pack/)\n"
    "[Top](#readme)\n"
)


class LinkCase(unittest.TestCase):
    """A fixture tree whose reader-facing documents all link correctly."""

    def setUp(self):
        # The fixture activates the shipped `docs/adr` group: its index is fresh
        # and outside the link-check scope, so the cases below judge links only.
        self.repo = support.TempRepository(
            self, support.policy(active_directories=["docs/adr"]))
        self.repo.write("GLOSSARY.md", "# Glossary\n\n## Guide\n")
        self.repo.write("docs/adr/0001-a.md",
                        support.document("Alpha", "A fixture record.", "prototype"))
        self.repo.write("docs/content-format.md", "# Content format\n\n## 1. Command surface\n")
        self.repo.write("docs/architecture.md", ARCHITECTURE)
        self.repo.write("docs/guide/play-the-demo.md", GUIDE_PAGE)
        self.repo.write("examples/demo-pack/pack.ron", "(scenes: [])\n")
        self.repo.write("README.md", README)
        self.repo.commit()
        self.assertPasses(self.repo.generate())
        self.repo.commit("index")

    def assertPasses(self, result):
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn("fix:", result.stderr)

    def assertFails(self, result, *fragments):
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        for fragment in fragments:
            self.assertIn(fragment, result.stderr)
        self.assertIn("fix:", result.stderr)


class AcceptedLinkTests(LinkCase):
    def test_a_file_a_file_anchor_a_directory_and_a_same_page_anchor_resolve(self):
        """`#1-command-surface` is GitHub's anchor for `## 1. Command surface`."""
        self.assertPasses(self.repo.check())

    def test_an_absolute_url_and_an_export_token_are_skipped(self):
        self.repo.write("README.md",
                        "# Readme\n"
                        "\n"
                        "[Releases](https://example.org/no-such-page.md)\n"
                        "[Guide](@@GUIDE_URL@@/play-the-demo.md)\n")
        self.assertPasses(self.repo.check())

    def test_a_link_inside_an_inline_code_span_is_text(self):
        self.repo.write("README.md",
                        "# Readme\n\nWrite `[text](docs/no-such-page.md)` for a link.\n")
        self.assertPasses(self.repo.check())

    def test_a_document_outside_the_three_scopes_is_not_checked(self):
        self.repo.write("docs/content-format.md",
                        "# Content format\n"
                        "\n"
                        "## 1. Command surface\n"
                        "\n"
                        "[A retired page](retired-page.md)\n")
        self.assertPasses(self.repo.check())

    def test_a_scoped_path_absent_from_the_snapshot_is_skipped(self):
        """`docs/architecture.md` and the Guide pages land in later tickets."""
        self.repo.git("rm", "-q", "docs/architecture.md", "docs/guide/play-the-demo.md")
        self.assertPasses(self.repo.check())


class RejectedLinkTests(LinkCase):
    def test_a_missing_target_file_fails(self):
        self.repo.write("README.md", README + "[Gone](docs/no-such-page.md)\n")
        self.assertFails(self.repo.check(), "README.md", "docs/no-such-page.md")

    def test_a_missing_anchor_fails(self):
        self.repo.write("README.md", README + "[Gone](docs/architecture.md#no-such-section)\n")
        self.assertFails(self.repo.check(), "README.md",
                         "docs/architecture.md#no-such-section")

    def test_a_heading_inside_a_fenced_code_block_is_not_an_anchor(self):
        self.repo.write("README.md", README + "[Crates](docs/architecture.md#crates)\n"
                        "[Not a heading](docs/architecture.md#not-a-heading)\n")
        result = self.repo.check()
        self.assertFails(result, "README.md", "docs/architecture.md#not-a-heading")
        self.assertNotIn("#crates", result.stderr)

    def test_a_link_that_escapes_the_repository_root_fails(self):
        self.repo.write("README.md", README + "[Up](../../../etc/passwd)\n")
        self.assertFails(self.repo.check(), "README.md",
                         "../../../etc/passwd", "above the repository root")

    def test_a_scoped_page_that_is_not_utf8_fails(self):
        (self.repo.root / "docs/guide/broken.md").write_bytes(b"# Broken\n\n\xff\n")
        self.repo.git("add", "docs/guide/broken.md")
        self.assertFails(self.repo.check(), "docs/guide/broken.md", "UTF-8")


class SnapshotAndCommandTests(LinkCase):
    def test_the_check_reads_the_snapshot_not_the_checkout(self):
        self.repo.write("README.md", README + "[Gone](docs/no-such-page.md)\n")
        self.assertFails(self.repo.check(), "docs/no-such-page.md")
        self.assertPasses(self.repo.check("--staged"))

    def test_generate_does_not_run_the_link_check(self):
        self.repo.write("README.md", README + "[Gone](docs/no-such-page.md)\n")
        self.assertPasses(self.repo.generate())


if __name__ == "__main__":
    unittest.main()
