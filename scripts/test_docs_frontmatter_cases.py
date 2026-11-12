"""Independently derived expectations for the restricted frontmatter subset.

Authored BEFORE `prototype.py` exists, from the published specification (the
document-metadata contract, section B) plus the YAML 1.2.2 core tag-resolution
table.
Each case cites the rule it exercises. `expect` is what a correct parser must
do; it is not copied from any implementation.

Rejection cases assert the stage ("syntax" before schema), the attributed
field, and nothing about the diagnostic wording.
Accept cases assert the exact decoded mapping.
"""

import textwrap

ALLOWED = {
    "title",
    "summary",
    "keywords",
    "status",
    "related",
}

# The 39 approved terms are loaded from policy.json at run time; this list is
# only used to build fixtures and is asserted against that file.
APPROVED = [
    "content-format", "content-pack", "dialogue-dsl", "decisions", "decision-point",
    "authoring-cli", "pack-validation", "credentials", "terminal-mask",
    "persistence", "save-slots", "settings-file", "read-log", "text-skip", "audio",
    "monitor", "diagnostics", "failure-posture", "presentation-surface", "gallery",
    "timeline", "player-input",
    "ownership", "verification", "evidence-record", "release-certification",
    "shipping-composition", "native-boot", "ci", "external-dependency",
    "issue-tracker", "issue-triage", "agent-process", "agent-tooling",
    "navigation-tooling", "code-comments", "repository-upkeep", "prototype",
    "domain-modeling",
]

EIGHT = APPROVED[:8]


def doc(body: str) -> str:
    return "---\n" + textwrap.dedent(body).lstrip("\n")


CASES = [
    # --- accepted documents -------------------------------------------------
    dict(
        id="A01-published-example",
        area="accept",
        rule="Section B example; status and related are optional and accepted",
        text=doc(
            """
            title: One realization owner per presentation surface
            summary: Defines presentation ownership and state boundaries.
            keywords:
              - presentation-surface
              - ownership
            status: accepted
            related:
              - ../runtime-hardening.md
            ---
            """
        ),
        expect=dict(
            accept=True,
            values=dict(
                title="One realization owner per presentation surface",
                summary="Defines presentation ownership and state boundaries.",
                keywords=["presentation-surface", "ownership"],
                status="accepted",
                related=["../runtime-hardening.md"],
            ),
        ),
    ),
    dict(
        id="A02-minimal-required-only",
        area="accept",
        rule="Only title, summary and keywords are required",
        text=doc(
            """
            title: Minimal document
            summary: Three required fields only.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(
            accept=True,
            values=dict(
                title="Minimal document",
                summary="Three required fields only.",
                keywords=["ci"],
            ),
        ),
    ),
    dict(
        id="A03-crlf",
        area="accept",
        rule="LF and CRLF are accepted",
        text=doc(
            """
            title: CRLF document
            summary: Line endings must not change the result.
            keywords:
              - ci
            ---
            """
        ).replace("\n", "\r\n"),
        expect=dict(
            accept=True,
            values=dict(
                title="CRLF document",
                summary="Line endings must not change the result.",
                keywords=["ci"],
            ),
        ),
    ),
    dict(
        id="A04-blank-lines-and-comments",
        area="accept",
        rule="Blank lines and full-line comments, including indented ones, are allowed",
        text=doc(
            """
            # leading comment
            title: Commented document

            # between fields
              # indented comment
            summary: Comments are not copied to indexes.
            keywords:
              - code-comments
            ---
            """
        ),
        expect=dict(
            accept=True,
            values=dict(
                title="Commented document",
                summary="Comments are not copied to indexes.",
                keywords=["code-comments"],
            ),
        ),
    ),
    dict(
        id="A05-fields-in-any-order",
        area="accept",
        rule="Fields can appear in any order",
        text=doc(
            """
            status: accepted
            related: []
            keywords:
              - ownership
            summary: Order is unrestricted.
            title: Reverse order document
            ---
            """
        ),
        expect=dict(
            accept=True,
            values=dict(
                status="accepted",
                related=[],
                keywords=["ownership"],
                summary="Order is unrestricted.",
                title="Reverse order document",
            ),
        ),
    ),
    dict(
        id="A06-separation-spaces",
        area="accept",
        rule="One or more ASCII spaces separate the value; outside spaces are presentation",
        text=doc(
            """
            title:     Leading separation
            summary: Two  spaces
            keywords:
              - domain-modeling
            ---
            """
        ),
        expect=dict(
            accept=True,
            values=dict(
                title="Leading separation",
                summary="Two  spaces",
                keywords=["domain-modeling"],
            ),
        ),
    ),
    dict(
        id="A07-quoted-escapes",
        area="accept",
        rule='Double quotes permit \\" \\\\ \\/ and \\uXXXX',
        text=doc(
            r"""
            title: "Quote \" backslash \\ slash \/ uni \u0061"
            summary: Escapes decode to one scalar each.
            keywords:
              - domain-modeling
            ---
            """
        ),
        expect=dict(
            accept=True,
            values=dict(
                title='Quote " backslash \\ slash / uni a',
                summary="Escapes decode to one scalar each.",
                keywords=["domain-modeling"],
            ),
        ),
    ),
    dict(
        id="A08-emoji-plain",
        area="accept",
        rule="Literal Unicode including emoji is allowed in plain scalars",
        text=doc(
            """
            title: Shipping checklist 🚀
            summary: Literal emoji stay as written.
            keywords:
              - release-certification
            ---
            """
        ),
        expect=dict(
            accept=True,
            values=dict(
                title="Shipping checklist 🚀",
                summary="Literal emoji stay as written.",
                keywords=["release-certification"],
            ),
        ),
    ),
    dict(
        id="A09-emoji-quoted",
        area="accept",
        rule="Literal Unicode inside double quotes is allowed",
        text=doc(
            """
            title: "菜单 and 🚀"
            summary: Quoted Unicode is not escaped.
            keywords:
              - domain-modeling
            ---
            """
        ),
        expect=dict(
            accept=True,
            values=dict(
                title="菜单 and 🚀",
                summary="Quoted Unicode is not escaped.",
                keywords=["domain-modeling"],
            ),
        ),
    ),
    dict(
        id="A10-escaped-backslash-text",
        area="accept",
        rule="Literal escaped-backslash text such as \\\\ud83d remains allowed",
        text=doc(
            r"""
            title: Show the escape literally
            summary: "Write \\ud83d, not a surrogate."
            keywords:
              - code-comments
            ---
            """
        ),
        expect=dict(
            accept=True,
            values=dict(
                title="Show the escape literally",
                summary="Write \\ud83d, not a surrogate.",
                keywords=["code-comments"],
            ),
        ),
    ),
    dict(
        id="A11-plain-yes",
        area="accept",
        rule="Plain yes is a string in YAML 1.2 core; an internal apostrophe is allowed",
        text=doc(
            """
            title: yes
            summary: It's a string, not a boolean.
            keywords:
              - agent-process
            ---
            """
        ),
        expect=dict(
            accept=True,
            values=dict(
                title="yes",
                summary="It's a string, not a boolean.",
                keywords=["agent-process"],
            ),
        ),
    ),
    dict(
        id="A12-related-empty-list",
        area="accept",
        rule="An empty related list is allowed",
        text=doc(
            """
            title: No related documents
            summary: Empty list is syntactically and semantically valid.
            keywords:
              - repository-upkeep
            related: []
            ---
            """
        ),
        expect=dict(
            accept=True,
            values=dict(
                title="No related documents",
                summary="Empty list is syntactically and semantically valid.",
                keywords=["repository-upkeep"],
                related=[],
            ),
        ),
    ),
    dict(
        id="A13-summary-exactly-160",
        area="accept",
        rule="At most 160 decoded Unicode scalar values; leading/trailing whitespace counted",
        text=doc(
            """
            title: Boundary length
            summary: "%s"
            keywords:
              - verification
            ---
            """
            % (" " + "a" * 158 + " ")
        ),
        expect=dict(
            accept=True,
            values=dict(
                title="Boundary length",
                summary=" " + "a" * 158 + " ",
                keywords=["verification"],
            ),
        ),
    ),
    dict(
        id="A14-eight-keywords",
        area="accept",
        rule="1-8 unique keywords",
        text=doc(
            """
            title: Eight keywords
            summary: The upper bound is accepted.
            keywords:
            %s
            ---
            """
        ) % "\n".join("  - " + term for term in EIGHT),
        expect=dict(
            accept=True,
            values=dict(
                title="Eight keywords",
                summary="The upper bound is accepted.",
                keywords=EIGHT,
            ),
        ),
    ),
    dict(
        id="A15-quoted-hash-and-colon",
        area="accept",
        rule="Quoted values may contain # and :",
        text=doc(
            """
            title: "Cost #1: one owner"
            summary: "Scope: single owner"
            keywords:
              - ownership
            ---
            """
        ),
        expect=dict(
            accept=True,
            values=dict(
                title="Cost #1: one owner",
                summary="Scope: single owner",
                keywords=["ownership"],
            ),
        ),
    ),
    dict(
        id="A16-quoted-numeric-keyword",
        area="accept",
        rule='A numeric-only keyword must be quoted; "12" decodes as a string',
        text=doc(
            """
            title: Quoted numeric
            summary: Quoting keeps the value a string.
            keywords:
              - "12"
            ---
            """
        ),
        expect=dict(
            accept=True,
            values=dict(
                title="Quoted numeric",
                summary="Quoting keeps the value a string.",
                keywords=["12"],
            ),
        ),
        # "12" is not in the closed allowlist; syntax-only acceptance is the
        # point of this case, so the allowlist is relaxed here explicitly.
        relax_allowlist=True,
    ),
    dict(
        id="A17-quoted-status",
        area="accept",
        rule="status is an optional nonempty single-line string",
        text=doc(
            """
            title: Quoted status
            summary: Status values are preserved verbatim.
            keywords:
              - ownership
            status: "accepted"
            ---
            """
        ),
        expect=dict(
            accept=True,
            values=dict(
                title="Quoted status",
                summary="Status values are preserved verbatim.",
                keywords=["ownership"],
                status="accepted",
            ),
        ),
    ),
    # --- rejected: syntax ---------------------------------------------------
    dict(
        id="R01-missing-frontmatter",
        area="syntax",
        rule="Valid leading YAML frontmatter is required",
        text="# Title\n\nBody only.\n",
        expect=dict(accept=False, stage="syntax", field=None),
    ),
    dict(
        id="R02-unterminated-frontmatter",
        area="syntax",
        rule="The leading block must be closed by ---",
        text="---\ntitle: x\nsummary: y\nkeywords:\n  - ci\n",
        expect=dict(accept=False, stage="syntax", field=None),
    ),
    dict(
        id="R03-duplicate-key",
        area="syntax",
        rule="unique-key top-level mapping",
        text=doc(
            """
            title: First
            title: Second
            summary: Duplicate keys are not a mapping.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="title", line=3),
    ),
    dict(
        id="R04-indented-key",
        area="syntax",
        rule="Keys are unindented",
        text=doc(
            """
            title: Indented key
             summary: This line is indented.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="summary"),
    ),
    dict(
        id="R05-nested-map",
        area="syntax",
        rule="Nested maps are rejected",
        text=doc(
            """
            title: Nested
              owner: someone
            summary: Nested mapping under a field.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="title"),
    ),
    dict(
        id="R06-flow-map",
        area="syntax",
        rule="Flow collections other than [] are rejected",
        text=doc(
            """
            title: Flow map
            summary: Flow mapping is not the subset.
            keywords:
              - ci
            related: {a: b}
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="related"),
    ),
    dict(
        id="R07-flow-list",
        area="syntax",
        rule="Block lists only; [] is the sole flow form",
        text=doc(
            """
            title: Flow list
            summary: Flow sequence is not the subset.
            keywords: [ci, ownership]
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="keywords"),
    ),
    dict(
        id="R08-anchor",
        area="syntax",
        rule="Tags, anchors and aliases are rejected",
        text=doc(
            """
            title: &anchor Anchored
            summary: Anchors are not the subset.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="title"),
    ),
    dict(
        id="R09-alias",
        area="syntax",
        rule="Tags, anchors and aliases are rejected",
        text=doc(
            """
            title: *anchor
            summary: Aliases are not the subset.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="title"),
    ),
    dict(
        id="R10-tag",
        area="syntax",
        rule="Tags are rejected",
        text=doc(
            """
            title: !!str Tagged
            summary: Tags are not the subset.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="title"),
    ),
    dict(
        id="R11-block-scalar",
        area="syntax",
        rule="Block scalars are rejected",
        text=doc(
            """
            title: Block scalar
            summary: |
              Two lines are not allowed.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="summary"),
    ),
    dict(
        id="R12-single-quoted",
        area="syntax",
        rule="Single-quoted strings are rejected",
        text=doc(
            """
            title: 'Single quoted'
            summary: Only plain and double-quoted strings.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="title"),
    ),
    dict(
        id="R13-multiline-plain",
        area="syntax",
        rule="Multiline values are rejected",
        text=doc(
            """
            title: First line
              continued here
            summary: Continuation is not allowed.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="title"),
    ),
    dict(
        id="R14-inline-comment",
        area="syntax",
        rule="Inline comments are rejected",
        text=doc(
            """
            title: Trailing note # not allowed
            summary: Inline comments are rejected.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="title"),
    ),
    dict(
        id="R15-tab-indentation",
        area="syntax",
        rule="Tab indentation is rejected",
        text="---\ntitle: Tab indented\nsummary: Reject tabs.\nkeywords:\n\t- ci\n---\n",
        expect=dict(accept=False, stage="syntax", field="keywords"),
    ),
    dict(
        id="R16-tab-in-quoted-value",
        area="syntax",
        rule="Decoded C0 controls are rejected; a literal tab decodes to U+0009",
        text="---\ntitle: \"Tab\tinside\"\nsummary: Literal tab is a control.\nkeywords:\n  - ci\n---\n",
        expect=dict(accept=False, stage="syntax", field="title"),
    ),
    dict(
        id="R17-list-dash-indent",
        area="syntax",
        rule="Block-list dashes have exactly two leading spaces",
        text=doc(
            """
            title: Four space dash
            summary: Dash indentation is fixed.
            keywords:
                - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="keywords"),
    ),
    dict(
        id="R18-bare-boolean",
        area="syntax",
        rule="Bare YAML core booleans are rejected for string fields",
        text=doc(
            """
            title: Bare boolean
            summary: Core resolution is rejected, not converted.
            keywords:
              - ci
            status: true
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="status"),
    ),
    dict(
        id="R19-bare-null",
        area="syntax",
        rule="Bare YAML core nulls are rejected for string fields",
        text=doc(
            """
            title: Bare null
            summary: Null is not a string.
            keywords:
              - ci
            status: ~
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="status"),
    ),
    dict(
        id="R20-bare-int",
        area="syntax",
        rule="Bare YAML core numbers are rejected for string fields",
        text=doc(
            """
            title: Bare int
            summary: Numbers are not strings.
            keywords:
              - ci
            status: 3
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="status"),
    ),
    dict(
        id="R21-bare-float",
        area="syntax",
        rule="Bare YAML core floats are rejected for string fields",
        text=doc(
            """
            title: Bare float
            summary: Floats are not strings.
            keywords:
              - ci
            status: 1.5
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="status"),
    ),
    dict(
        id="R22-bare-hex",
        area="syntax",
        rule="YAML 1.2 core 0x integers are rejected for string fields",
        text=doc(
            """
            title: Bare hex
            summary: Hex integers resolve to int.
            keywords:
              - ci
            status: 0x1f
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="status"),
    ),
    dict(
        id="R23-bare-exponent",
        area="syntax",
        rule="YAML 1.2 core exponent floats are rejected for string fields",
        text=doc(
            """
            title: Bare exponent
            summary: Exponent floats resolve to float.
            keywords:
              - ci
            status: 1e3
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="status"),
    ),
    dict(
        id="R24-unquoted-hash",
        area="syntax",
        rule="Any value containing # must be quoted",
        text=doc(
            """
            title: Ticket#275
            summary: A literal hash requires quotes.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="title"),
    ),
    dict(
        id="R25-unquoted-colon",
        area="syntax",
        rule="Any value containing : must be quoted",
        text=doc(
            """
            title: Scope: one owner
            summary: A literal colon requires quotes.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="title"),
    ),
    dict(
        id="R26-unquoted-backslash",
        area="syntax",
        rule="Any value containing a backslash must be quoted",
        text=doc(
            """
            title: C:\\Users\\owner
            summary: A backslash requires quotes.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="title"),
    ),
    dict(
        id="R27-plain-indicator",
        area="syntax",
        rule="A plain value may not begin with a YAML indicator",
        text=doc(
            """
            title: - dash first
            summary: Indicator start is invalid.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="title"),
    ),
    dict(
        id="R28-surrogate-high",
        area="syntax",
        rule="Every surrogate-valued escape is rejected, paired or not",
        text=doc(
            r"""
            title: "Lone \uD83D high"
            summary: High surrogate escape.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="title"),
    ),
    dict(
        id="R29-surrogate-low",
        area="syntax",
        rule="Every surrogate-valued escape is rejected, paired or not",
        text=doc(
            r"""
            title: "Lone \uDE00 low"
            summary: Low surrogate escape.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="title"),
    ),
    dict(
        id="R30-surrogate-pair",
        area="syntax",
        rule="Surrogate escapes are rejected even in pairs",
        text=doc(
            r"""
            title: "Pair \uD83D\uDE00 here"
            summary: Paired surrogate escapes are rejected.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="title"),
    ),
    dict(
        id="R31-escape-newline",
        area="syntax",
        rule="Only \\\" \\\\ \\/ and \\uXXXX escapes are permitted",
        text=doc(
            r"""
            title: "Line\nbreak"
            summary: \n is not a permitted escape.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="title"),
    ),
    dict(
        id="R32-escape-hex",
        area="syntax",
        rule="Only \\uXXXX; \\x.. is rejected",
        text=doc(
            r"""
            title: "Hex \x41"
            summary: Only four-digit Unicode escapes.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="title"),
    ),
    dict(
        id="R33-escape-long-unicode",
        area="syntax",
        rule="Only \\uXXXX; \\U........ is rejected",
        text=doc(
            r"""
            title: "Rocket \U0001F600"
            summary: Eight-digit escapes are not permitted.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="title"),
    ),
    dict(
        id="R34-decoded-c0",
        area="syntax",
        rule="Decoded C0 controls are rejected",
        text=doc(
            r"""
            title: "Bell \u0007 here"
            summary: C0 control.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="title"),
    ),
    dict(
        id="R35-decoded-c1",
        area="syntax",
        rule="Decoded C1 controls are rejected",
        text=doc(
            r"""
            title: "Next line \u0085 here"
            summary: C1 control.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="title"),
    ),
    dict(
        id="R36-line-separator",
        area="syntax",
        rule="Unicode line and paragraph separators are rejected",
        text=doc(
            r"""
            title: "Line \u2028 separator"
            summary: U+2028 is rejected.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="title"),
    ),
    dict(
        id="R37-paragraph-separator",
        area="syntax",
        rule="Unicode line and paragraph separators are rejected",
        text=doc(
            r"""
            title: "Paragraph \u2029 separator"
            summary: U+2029 is rejected.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="title"),
    ),
    dict(
        id="R38-unterminated-quote",
        area="syntax",
        rule="Double-quoted values must close on the same line",
        text=doc(
            """
            title: "Unterminated
            summary: Missing closing quote.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="title"),
    ),
    dict(
        id="R39-trailing-after-quote",
        area="syntax",
        rule="Nothing may follow the closing quote",
        text=doc(
            """
            title: "Closed" extra
            summary: Trailing content is invalid.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="title"),
    ),
    dict(
        id="R40-empty-value",
        area="syntax",
        rule="An absent value resolves to null, not a string",
        text=doc(
            """
            title: Empty value
            summary:
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="summary"),
    ),
    # --- rejected: schema ---------------------------------------------------
    dict(
        id="S01-unknown-field",
        area="schema",
        rule="Other fields are rejected without a reviewed schema change",
        text=doc(
            """
            title: Unknown field
            summary: Owner is not part of the schema.
            keywords:
              - ci
            owner: someone
            ---
            """
        ),
        expect=dict(accept=False, stage="schema", field="owner", line=6),
    ),
    dict(
        id="S02-missing-title",
        area="schema",
        rule="title is required",
        text=doc(
            """
            summary: No title here.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="schema", field="title"),
    ),
    dict(
        id="S03-missing-summary",
        area="schema",
        rule="summary is required",
        text=doc(
            """
            title: No summary
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="schema", field="summary"),
    ),
    dict(
        id="S04-missing-keywords",
        area="schema",
        rule="keywords is required",
        text=doc(
            """
            title: No keywords
            summary: Keywords are required.
            ---
            """
        ),
        expect=dict(accept=False, stage="schema", field="keywords"),
    ),
    dict(
        id="S05-summary-161",
        area="schema",
        rule="summary is at most 160 decoded Unicode scalar values",
        text=doc(
            """
            title: Too long
            summary: "%s"
            keywords:
              - verification
            ---
            """
            % (" " + "a" * 159 + " ")
        ),
        expect=dict(accept=False, stage="schema", field="summary"),
    ),
    dict(
        id="S06-summary-whitespace-only",
        area="schema",
        rule="Whitespace-only summaries are rejected",
        text=doc(
            """
            title: Blank summary
            summary: "   "
            keywords:
              - verification
            ---
            """
        ),
        expect=dict(accept=False, stage="schema", field="summary"),
    ),
    dict(
        id="S07-keyword-empty-list",
        area="schema",
        rule="keywords requires 1-8 terms; [] is syntactically valid but fails schema",
        text=doc(
            """
            title: No keywords listed
            summary: Empty list fails the schema.
            keywords: []
            ---
            """
        ),
        expect=dict(accept=False, stage="schema", field="keywords"),
    ),
    dict(
        id="S08-keyword-nine",
        area="schema",
        rule="keywords is at most 8 terms",
        text=doc(
            """
            title: Too many keywords
            summary: Nine exceeds the bound.
            keywords:
            %s
            ---
            """
        ) % "\n".join("  - " + term for term in APPROVED[:9]),
        expect=dict(accept=False, stage="schema", field="keywords"),
    ),
    dict(
        id="S09-keyword-duplicate",
        area="schema",
        rule="keywords are unique",
        text=doc(
            """
            title: Duplicate keyword
            summary: Repetition fails the schema.
            keywords:
              - ci
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="schema", field="keywords"),
    ),
    dict(
        id="S10-keyword-unapproved",
        area="schema",
        rule="Keywords come from the approved closed allowlist",
        text=doc(
            """
            title: Unapproved term
            summary: The vocabulary is closed.
            keywords:
              - invented-term
            ---
            """
        ),
        expect=dict(accept=False, stage="schema", field="keywords"),
    ),
    dict(
        id="S11-keyword-uppercase",
        area="schema",
        rule="Reject uppercase without silent rewriting",
        text=doc(
            """
            title: Upper case keyword
            summary: Spelling is canonical.
            keywords:
              - Persistence
            ---
            """
        ),
        expect=dict(accept=False, stage="schema", field="keywords"),
    ),
    dict(
        id="S12-keyword-underscore",
        area="schema",
        rule="Reject underscores without silent rewriting",
        text=doc(
            """
            title: Underscore keyword
            summary: Hyphens only.
            keywords:
              - save_slots
            ---
            """
        ),
        expect=dict(accept=False, stage="schema", field="keywords"),
    ),
    dict(
        id="S13-keyword-space",
        area="schema",
        rule="Reject spaces without silent rewriting",
        text=doc(
            """
            title: Space keyword
            summary: Single terms only.
            keywords:
              - save slots
            ---
            """
        ),
        expect=dict(accept=False, stage="schema", field="keywords"),
    ),
    dict(
        id="S14-keyword-double-hyphen",
        area="schema",
        rule="Terms use single hyphens",
        text=doc(
            """
            title: Double hyphen keyword
            summary: Only single hyphens separate parts.
            keywords:
              - save--slots
            ---
            """
        ),
        expect=dict(accept=False, stage="schema", field="keywords"),
    ),
    dict(
        id="S15-keyword-leading-hyphen",
        area="schema",
        rule="Terms are lowercase letters/digits separated by single hyphens",
        text=doc(
            """
            title: Leading hyphen keyword
            summary: A leading hyphen fails the pattern.
            keywords:
              - -save
            ---
            """
        ),
        expect=dict(accept=False, stage="schema", field="keywords"),
    ),
    dict(
        id="S16-keyword-trailing-hyphen",
        area="schema",
        rule="Terms are lowercase letters/digits separated by single hyphens",
        text=doc(
            """
            title: Trailing hyphen keyword
            summary: A trailing hyphen fails the pattern.
            keywords:
              - save-
            ---
            """
        ),
        expect=dict(accept=False, stage="schema", field="keywords"),
    ),
    dict(
        id="S17-keyword-non-ascii",
        area="schema",
        rule="Reject non-ASCII without silent rewriting",
        text=doc(
            """
            title: Non ascii keyword
            summary: ASCII only.
            keywords:
              - café
            ---
            """
        ),
        expect=dict(accept=False, stage="schema", field="keywords"),
    ),
    dict(
        id="S18-related-absolute",
        area="schema",
        rule="Absolute paths are rejected",
        text=doc(
            """
            title: Absolute related path
            summary: Repository-relative only.
            keywords:
              - ci
            related:
              - /etc/passwd
            ---
            """
        ),
        expect=dict(accept=False, stage="schema", field="related"),
    ),
    dict(
        id="S19-related-url",
        area="schema",
        rule="URLs are rejected (quoted, because the grammar requires quotes for `:`) ",
        text=doc(
            """
            title: URL related path
            summary: Local paths only.
            keywords:
              - ci
            related:
              - "https://example.com/a.md"
            ---
            """
        ),
        expect=dict(accept=False, stage="schema", field="related"),
    ),
    dict(
        id="S20-related-escape",
        area="schema",
        rule="Repository escapes are rejected",
        text=doc(
            """
            title: Escaping related path
            summary: Paths must stay inside the repository.
            keywords:
              - ci
            related:
              - ../../outside.md
            ---
            """
        ),
        expect=dict(accept=False, stage="schema", field="related"),
    ),
    dict(
        id="S21-related-backslash",
        area="schema",
        rule="Related paths are slash-separated (quoted, because a backslash requires quotes)",
        text=doc(
            r"""
            title: Backslash related path
            summary: Slash separators only.
            keywords:
              - ci
            related:
              - "..\\runtime-hardening.md"
            ---
            """
        ),
        expect=dict(accept=False, stage="schema", field="related"),
    ),
    dict(
        id="S22-status-empty",
        area="schema",
        rule="status is a nonempty string",
        text=doc(
            """
            title: Empty status
            summary: Status must be nonempty when present.
            keywords:
              - ownership
            status: ""
            ---
            """
        ),
        expect=dict(accept=False, stage="schema", field="status"),
    ),
    # --- decided after the feasibility run (maintainer answers Q48-Q51) ------
    dict(
        id="D01-space-before-colon",
        area="syntax",
        rule="Q48: the colon must follow the key immediately",
        text=doc(
            """
            title : Space before colon
            summary: A space before the colon is a malformed key.
            keywords:
              - ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="title"),
    ),
    dict(
        id="D02-blank-line-inside-list",
        area="accept",
        rule="Q49: blank lines may appear between block-list items",
        text=doc(
            """
            title: Blank line in list
            summary: Blank lines do not end a block list.
            keywords:
              - ci

              - ownership
            ---
            """
        ),
        expect=dict(
            accept=True,
            values=dict(
                title="Blank line in list",
                summary="Blank lines do not end a block list.",
                keywords=["ci", "ownership"],
            ),
        ),
    ),
    dict(
        id="D03-comment-inside-list",
        area="accept",
        rule="Q49: full-line comments may appear between block-list items",
        text=doc(
            """
            title: Comment in list
            summary: A comment line does not end a block list.
            keywords:
              - ci
              # why the next term applies
              - ownership
            ---
            """
        ),
        expect=dict(
            accept=True,
            values=dict(
                title="Comment in list",
                summary="A comment line does not end a block list.",
                keywords=["ci", "ownership"],
            ),
        ),
    ),
    dict(
        id="D04-dash-without-space",
        area="syntax",
        rule="Q50: every item is `  - ` followed by a value",
        text=doc(
            """
            title: Dash without space
            summary: The dash needs a following space.
            keywords:
              -ci
            ---
            """
        ),
        expect=dict(accept=False, stage="syntax", field="keywords"),
    ),
    dict(
        id="D05-closing-fence-trailing-space",
        area="syntax",
        rule="Q51: fence lines are exactly `---`",
        text="---\ntitle: Closing fence spaces\nsummary: Trailing spaces are rejected.\nkeywords:\n  - ci\n--- \n",
        expect=dict(accept=False, stage="syntax", field=None),
    ),
    dict(
        id="D06-opening-fence-trailing-space",
        area="syntax",
        rule="Q51: fence lines are exactly `---`",
        text="--- \ntitle: Opening fence spaces\nsummary: Trailing spaces are rejected.\nkeywords:\n  - ci\n---\n",
        expect=dict(accept=False, stage="syntax", field=None),
    ),
    dict(
        id="D07-list-separator-spaces",
        area="accept",
        rule="One or more ASCII spaces separate a list item from its dash (Q49/Q50 family)",
        text=doc(
            """
            title: Item separation
            summary: Extra separator spaces are presentation, not value content.
            keywords:
              -   decisions
            ---
            """
        ),
        expect=dict(
            accept=True,
            values=dict(
                title="Item separation",
                summary="Extra separator spaces are presentation, not value content.",
                keywords=["decisions"],
            ),
        ),
    ),
    dict(
        id="D08-literal-line-separator-plain",
        area="syntax",
        rule="Unicode line and paragraph separators are rejected in plain values too",
        text="---\ntitle: Literal separator\nsummary: before\u2028after\nkeywords:\n  - ci\n---\n",
        expect=dict(accept=False, stage="syntax", field="summary"),
    ),
    dict(
        id="D09-literal-c1-plain",
        area="syntax",
        rule="Literal C1 controls are rejected in plain values too",
        text="---\ntitle: bad\u0085title\nsummary: Literal C1 control.\nkeywords:\n  - ci\n---\n",
        expect=dict(accept=False, stage="syntax", field="title"),
    ),
]

# Values that a second, independent decoder (CPython's JSON scanner) must agree
# on. Each entry is (double-quoted source, expected decoded string).
JSON_ORACLE = [
    (r'"Quote \" here"', 'Quote " here'),
    (r'"Backslash \\ here"', "Backslash \\ here"),
    (r'"Slash \/ here"', "Slash / here"),
    (r'"Uni \u0061 here"', "Uni a here"),
    (r'"Rocket 🚀"', "Rocket 🚀"),
    (r'"Quote \" backslash \\ slash \/ uni \u0061"', 'Quote " backslash \\ slash / uni a'),
]

# Q48-Q51 (maintainer decisions, 2026-11-02) resolved every construct this
# experiment previously recorded as undecided; they are now cases D01-D06.
PENDING = []
