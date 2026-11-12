#!/usr/bin/env python3
"""Generated indexes are deterministic projections of document metadata.

The expected bytes are written out from the rendering contract — the index
shapes the documentation-structure specification fixes, and its decision that a
document's metadata renders as inert text — not from the renderer's own output.
"""

import subprocess
import unittest

import docs_test_support as support
from docs_index import Document, render


def doc(path, title, summary, keywords, status=None):
    return Document(path=path, title=title, summary=summary, keywords=keywords, status=status)


class RenderingTests(unittest.TestCase):
    def setUp(self):
        self.policy = support.load_policy()

    def render(self, directory, documents, children=()):
        return render(directory, documents, children, self.policy).decode("utf-8")

    def test_exact_bytes_for_a_directory_without_children_or_landing_links(self):
        text = self.render(
            "docs/guide",
            [
                doc("docs/guide/0001-openrouter-decisions-api.md",
                    "OpenRouter Decisions API",
                    "Records the provider request and response shape.",
                    ["decisions", "external-dependency"]),
                doc("docs/guide/0002-save-slots.md",
                    "Save slots",
                    "Defines the save slot model.",
                    ["save-slots", "persistence"]),
            ],
        )
        expected = (
            support.MARKER + "\n"
            "\n"
            "# Index: docs/guide/\n"
            "\n"
            "Generated discovery index. Edit a document's frontmatter, then run\n"
            "`python scripts/docs.py generate`.\n"
            "\n"
            "- [OpenRouter Decisions API](0001-openrouter-decisions-api.md) — "
            "Records the provider request and response shape. "
            "(`decisions`, `external-dependency`)\n"
            "- [Save slots](0002-save-slots.md) — Defines the save slot model. "
            "(`save-slots`, `persistence`)\n"
        )
        self.assertEqual(text, expected)

    def test_an_empty_directory_renders_only_its_header(self):
        """A directory whose pages have not landed yet, such as `docs/guide`."""
        expected = (
            support.MARKER + "\n"
            "\n"
            "# Index: docs/guide/\n"
            "\n"
            "Generated discovery index. Edit a document's frontmatter, then run\n"
            "`python scripts/docs.py generate`.\n"
        )
        self.assertEqual(self.render("docs/guide", []), expected)

    def test_repeated_rendering_is_byte_identical(self):
        documents = [
            doc("docs/guide/0002-b.md", "B", "Second.", ["ci"]),
            doc("docs/guide/0001-a.md", "A", "First.", ["ci"]),
        ]
        self.assertEqual(self.render("docs/guide", documents),
                         self.render("docs/guide", list(reversed(documents))))

    def test_entry_order_follows_file_name_not_input_order(self):
        documents = [
            doc("docs/guide/0003-c.md", "C", "Third.", ["ci"]),
            doc("docs/guide/0001-a.md", "A", "First.", ["ci"]),
            doc("docs/guide/0002-b.md", "B", "Second.", ["ci"]),
        ]
        text = self.render("docs/guide", documents)
        self.assertLess(text.index("(0001-a.md)"), text.index("(0002-b.md)"))
        self.assertLess(text.index("(0002-b.md)"), text.index("(0003-c.md)"))

    def test_the_index_never_lists_itself(self):
        text = self.render("docs/guide", [
            doc("docs/guide/INDEX.md", "Index", "Self.", ["ci"]),
            doc("docs/guide/0001-a.md", "A", "First.", ["ci"]),
        ])
        self.assertNotIn("Self.", text)
        self.assertNotIn("(INDEX.md)", text)

    def test_document_bodies_are_never_copied(self):
        text = self.render("docs/guide",
                           [doc("docs/guide/0001-a.md", "A", "First.", ["ci"])])
        self.assertIn("First.", text)
        self.assertNotIn("REQUIREMENT", text)
        self.assertNotIn("Body text.", text)

    def test_section_links_render_under_their_document(self):
        text = self.render(
            "docs",
            [
                doc("docs/runtime-hardening.md", "Runtime hardening",
                    "Defines the runtime contracts.", ["persistence"]),
                doc("docs/author-cli.md", "Author CLI",
                    "Defines the authoring command surface.", ["authoring-cli"]),
            ],
            children=["docs/adr", "docs/guide"],
        )
        self.assertIn(
            "- [Runtime hardening](runtime-hardening.md) — Defines the runtime contracts. "
            "(`persistence`)\n"
            "  - [Save slots](runtime-hardening.md#save-slots)\n",
            text,
        )
        self.assertIn("  - [Command surface](author-cli.md#1-command-surface)\n", text)
        self.assertNotIn("Content-format", text)

    def test_child_directories_render_when_indexed(self):
        text = self.render(
            "docs",
            [doc("docs/author-cli.md", "Author CLI", "Defines the command surface.",
                 ["authoring-cli"])],
            children=["docs/adr", "docs/guide"],
        )
        self.assertIn(
            "## Child directories\n"
            "\n"
            "- [`docs/adr/`](adr/INDEX.md)\n"
            "- [`docs/guide/`](guide/INDEX.md)\n",
            text,
        )

    def test_a_directory_without_indexed_children_has_no_child_section(self):
        text = self.render("docs/guide", [doc("docs/guide/0001-a.md", "A", "First.",
                                                 ["ci"])])
        self.assertNotIn("## Child directories", text)

    def test_excluded_landing_links_render_under_related(self):
        text = self.render(
            "docs",
            [doc("docs/author-cli.md", "Author CLI", "Defines the command surface.",
                 ["authoring-cli"])],
            children=["docs/adr"],
        )
        self.assertIn("## Related\n\n- [Glossary](../GLOSSARY.md)\n", text)

    def test_directories_without_configured_landing_links_have_no_related_section(self):
        text = self.render("docs/guide", [doc("docs/guide/0001-a.md", "A", "First.",
                                                 ["ci"])])
        self.assertNotIn("## Related", text)

    def test_status_is_appended_after_the_keywords_only_where_configured(self):
        adr = self.render("docs/adr", [doc(
            "docs/adr/0012-owner.md", "One owner", "Decides the owner.",
            ["ownership"], status="accepted")])
        self.assertIn("- [One owner](0012-owner.md) — Decides the owner. "
                      "(`ownership`) · status: accepted\n", adr)
        research = self.render("docs/guide", [doc(
            "docs/guide/0001-a.md", "A", "First.", ["ci"], status="accepted")])
        self.assertIn("- [A](0001-a.md) — First. (`ci`)\n", research)

    def test_link_targets_are_percent_encoded_and_text_is_escaped(self):
        text = self.render("docs/guide", [doc(
            "docs/guide/0001-a b (c)#d.md", "A [bracket] \\ here",
            "Escapes: 100% and (parens).", ["ci"])])
        self.assertIn("- [A \\[bracket\\] \\\\ here](0001-a%20b%20%28c%29%23d.md) — "
                      "Escapes: 100% and (parens). (`ci`)\n", text)

    def test_inline_markdown_in_a_title_or_summary_renders_inert(self):
        text = self.render("docs/guide", [doc(
            "docs/guide/0001-a.md",
            "Notes *em* _under_ `code` <b>bold</b>",
            "See [release notes](https://example.org), *em*, `code`, <b>bold</b> and a ~tilde~.",
            ["ci"])])
        self.assertIn(
            "- [Notes \\*em\\* \\_under\\_ \\`code\\` \\<b\\>bold\\</b\\>]"
            "(0001-a.md) — See \\[release notes\\](https<!-- -->://example.org), "
            "\\*em\\*, \\`code\\`, \\<b\\>bold\\</b\\> and a \\~tilde\\~. (`ci`)\n",
            text,
        )

    def test_display_text_cannot_be_autolinked(self):
        """A bare URL, `www.` domain and email address stay visible, unlinked text.

        GFM autolinks `scheme://`, `www.` and `local@domain` in a text node, so
        an empty HTML comment splits each one at its connector character without
        changing the visible words.
        """
        text = self.render("docs/adr", [doc(
            "docs/adr/0013-formatting.md",
            "Notes [release](https://example.com) *em* <b>bold</b>",
            "See https://example.com/a?b=1 and user@example.net; "
            "www.example.org/path is bare text.",
            ["ci"],
            status="[Review](https://example.com) <img src=x> **accepted**")])
        self.assertIn(
            "- [Notes \\[release\\](https://example.com) \\*em\\* \\<b\\>bold\\</b\\>]"
            "(0013-formatting.md) — See https<!-- -->://example.com/a?b=1 and "
            "user<!-- -->@example.net; www<!-- -->.example.org/path is bare text. "
            "(`ci`) · status: \\[Review\\](https<!-- -->://example.com) "
            "\\<img src=x\\> \\*\\*accepted\\*\\*\n",
            text,
        )

    def test_text_gfm_does_not_autolink_is_left_byte_identical(self):
        """Ordinary colons and a lone `@` are not autolinks, so nothing is inserted."""
        text = self.render("docs/guide", [doc(
            "docs/guide/0001-a.md", "Ratio 3:1",
            "Note: sign-off @ home at 10:30.", ["ci"])])
        self.assertIn("- [Ratio 3:1](0001-a.md) — Note: sign-off @ home at 10:30. "
                      "(`ci`)\n", text)

    def test_only_the_autolink_connectors_are_marked(self):
        """`mailto:` autolinks through its `@`; `tel:` and a bare `//` do not link."""
        text = self.render("docs/guide", [doc(
            "docs/guide/0001-a.md", "A",
            "Write mailto:user@example.com, not tel:+15550100, and "
            "//example.com/path needs a scheme.", ["ci"])])
        self.assertIn("Write mailto:user<!-- -->@example.com, not tel:+15550100, and "
                      "//example.com/path needs a scheme. (`ci`)\n", text)

    def test_the_autolink_superset_is_deliberately_conservative(self):
        """The break over-marks on purpose: the autolink pattern is the renderer's, not ours.

        GitHub's repository-file renderer leaves `irc://`, `awww.` and a dotless
        `local@domain` as text when probed against it, yet all three are marked
        here: an unmarked connector risks a live link, a marked one is an
        invisible comment.
        """
        text = self.render("docs/guide", [doc(
            "docs/guide/0001-a.md", "A",
            "See irc://example.com, awww.example.com and user@example.", ["ci"])])
        self.assertIn("See irc<!-- -->://example.com, awww<!-- -->.example.com and "
                      "user<!-- -->@example. (`ci`)\n", text)

    def test_keywords_stay_inline_code(self):
        text = self.render("docs/guide", [doc("docs/guide/0001-a.md", "A", "First.",
                                                 ["ci", "prototype"])])
        self.assertIn("(`ci`, `prototype`)", text)

    def test_output_is_lf_only_and_ends_with_one_newline(self):
        raw = render("docs/guide", [doc("docs/guide/0001-a.md", "A", "First.",
                                           ["ci"])], (), self.policy)
        self.assertNotIn(b"\r", raw)
        self.assertTrue(raw.endswith(b"\n"))
        self.assertFalse(raw.endswith(b"\n\n"))


class LineEndingTests(unittest.TestCase):
    def test_the_repository_pins_every_generated_index_to_lf(self):
        """Git's own attribute resolution, not a re-parse of `.gitattributes`."""
        paths = [directory + "/INDEX.md"
                 for directory in support.load_policy()["indexed_directories"]]
        out = subprocess.run(
            ["git", "check-attr", "text", "eol", "--", *paths],
            cwd=support.REPO_ROOT, check=True, capture_output=True, text=True).stdout
        attributes = {}
        for line in out.splitlines():
            path, attribute, value = line.split(": ", 2)
            attributes.setdefault(path, {})[attribute] = value
        for path in paths:
            with self.subTest(path=path):
                self.assertEqual(attributes[path]["text"], "set")
                self.assertEqual(attributes[path]["eol"], "lf")

    def test_the_index_pins_and_the_policy_cannot_drift(self):
        """One explicit pin per indexed directory: no glob hides a missing pin."""
        root = support.REPO_ROOT
        directories = support.load_policy()["indexed_directories"]
        expected = {directory + "/INDEX.md" for directory in directories}
        sources = [root / ".gitattributes"]
        sources += [root / directory / ".gitattributes" for directory in directories]
        pinned = {}
        for source in (source for source in sources if source.is_file()):
            for line in source.read_text(encoding="utf-8").splitlines():
                fields = line.split()
                if not fields or fields[0].startswith("#"):
                    continue
                name = fields[0]
                if name.endswith("/INDEX.md"):
                    pinned[name] = fields[1:]
                elif name == "INDEX.md" and source.parent != root:
                    # A pin in a directory's own `.gitattributes` is relative to
                    # that file, so it names the index of its own directory.
                    directory = source.parent.relative_to(root).as_posix()
                    pinned[directory + "/INDEX.md"] = fields[1:]
        self.assertEqual(set(pinned), expected)
        for path, attributes in pinned.items():
            with self.subTest(path=path):
                self.assertEqual(attributes, ["text", "eol=lf"])


if __name__ == "__main__":
    unittest.main()
