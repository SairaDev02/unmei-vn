---
title: Code comment policy and its review classification
summary: When a code comment earns its place, the REMOVE, SHORTEN, KEEP and REFACTOR outcomes, the protected contracts, the review procedure and the budget gate.
keywords:
  - code-comments
  - repository-upkeep
---
# Code comments

## Policy

A comment earns its place when it tells the reader something they cannot readily infer from the surrounding code or a clear identifier. Prefer clear names, types, small functions, assertions and tests for facts that the code itself can express. Do not add play-by-play narration, decorative section headers, paraphrases of identifiers or commented-out dead code.

Keep comments short and specific. Explain **why** a constraint exists or **why** a surprising choice is intentional. Do not describe **what** the next statement does. Reference the spec, ADR or version-matched dependency source instead of copying a long explanation that will drift. Do not remove a contract only because its reason also appears in the docs: the local warning may be what makes the code safe to change.

When a comment explains avoidable complexity, propose a focused refactor instead of rewording the comment. A comment-review request does **not** authorize a refactor of application code. If a stated constraint is unclear, investigate before you change it. Uncertainty is not evidence that a comment is redundant.

## Classification

| Decision | Use when | Outcome |
| --- | --- | --- |
| `REMOVE` | The comment only narrates obvious code, restates a name, is obsolete or holds dead commented-out code. | Propose deletion, citing the adjacent source. |
| `SHORTEN` | A real reason or invariant is buried in repetitive prose. | Propose a shorter replacement that keeps the reason and the references. |
| `KEEP` | The comment supplies a contract, constraint, context or generated documentation that the code does not show. | State why it matters. Do not change it to meet a comment-count target. |
| `REFACTOR` | The code's shape makes an explanation necessary, and a small structural change might express it instead. | Describe a possible separate change. Do not make it without explicit approval. |

Examples, which illustrate and are not automatic rules:

- `// Increment count` directly before `count += 1` is ordinarily `REMOVE`.
- Several paragraphs on why a resource must be inserted before Bevy's logging setup may be `SHORTEN`, but the startup-ordering contract itself is `KEEP`.
- The reason for `#[cfg_attr(not(feature = "network"), allow(dead_code))]` is worth keeping, even condensed. The attribute alone does not explain the design choice.
- A `///` description on a Clap-derived argument can become the user's `--help` output and is usually `KEEP`.

## Protected context

Before you delete or rewrite a comment, inspect the behavior it describes and the contract that governs it:

- **Rustdoc:** `///` and `//!` describe item and module APIs. Keep useful public contracts, safety requirements, examples and links. Changing one is a documentation change, not a mechanical cleanup.
- **Clap derive:** doc comments on derived commands and arguments can supply `about`, `help` or long help. Treat a change as user-facing, and compare the generated `--help` output if you edit one.
- **Correctness and safety:** keep ordering, lifetime, concurrency, serialization, wire-format, fallback, security, platform and interoperability constraints until you have verified their meaning and how they are enforced. Respect every `unsafe` safety explanation.
- **Lint suppressions:** explain each intentional local `allow` or `expect`, and verify its feature or configuration conditions. Consider an attribute `reason` where it fits. Do not delete a justified suppression only to remove prose.
- **Owned content:** keep license and legal notices, required generator markers, tested fixture content and meaningful examples. This policy does not apply to prose documentation or to authored `.unmei` scene content.

For Unmei behavior, use [domain.md](domain.md), the specs, the ADRs and the tests. For dependency behavior, use [documentation.md](documentation.md) and version-matched upstream references.

## Review and verification

An explicitly requested comment review is **report-only** by default. It covers the requested paths, or the current diff when no paths are named. Each proposed change names the location, the classification, the source context that supports it and any behavior or documentation risk. Do not optimize a comment ratio.

A bulk sweep runs in two passes. The second pass is adversarial: it rechecks every `REMOVE` from the first pass and keeps the comment when the evidence is uncertain. Issue #133 recorded a sample of what such a second pass keeps. Most of it was context the code does not show, such as UI state after a key press, a spec row number or an action's version pin, so keep this pass. The exception is banner rule lines made only of dashes: they are the decorative headers that the policy names, so a sweep removes the rule lines and keeps the title.

Edit comments only after the requester authorizes the change. Deleting an ambiguous contract and refactoring application code each need their own approval. For an edit, follow [`docs/testing.md`](../testing.md) and run only the checks that could detect a plausible regression. A documentation-only change does not need a Cargo test by default. Report exactly what you verified and what you did not.

## Budget gate

[`scripts/no-new-comments.py`](../../scripts/no-new-comments.py) runs in the per-push gate. It fails when a change adds a comment or swaps one comment for another. It compares each file against the per-comment digests in the checked-in [`.comment-baseline.json`](../../.comment-baseline.json), and it names where the content belongs instead: an ADR, an issue, or nowhere. [ADR 0016](../adr/0016-comment-baseline-holds-digests.md) records why the baseline holds digests rather than counts. Rustdoc, Python docstrings and tool directives are exempt, because they carry the API contract, the reading, or an instruction a tool obeys.

The baseline only shrinks. `--tighten` drops the comments the tree no longer holds and refuses a new one, so a sweep lands as a smaller budget rather than a looser one. A file new to the baseline takes `--tighten --admit PATH`. A deliberate `SHORTEN` takes `--tighten --reword PATH`, which accepts the new wording only while that file's count does not rise. The reworded entry then shows in the baseline diff for review. This gate enforces the budget, not the judgement above: a comment the policy protects still belongs in the code.

Run `python3 scripts/no-new-comments.py` before you push any change to a file the gate covers. What you do next depends on the change:

- **The gate fails:** it lists each comment it does not recognize. Move the reason into a name, a type, a test, an ADR or an issue, then delete the comment. If the comment is a `SHORTEN` of an existing one, use `--reword` as above.
- **You deleted comments:** run `--tighten` and commit `.comment-baseline.json` in the same change. Until then, the deleted text is still in the baseline, and it can come back.
- **You added or renamed a file with comments:** the gate fails on it, and `--tighten` refuses it, until you run `--tighten --admit PATH`. Check each comment against the [policy](#policy) first, because `--admit` takes in all of them. For a rename, the old path's entry drops out in the same run. The new baseline entry is where review sees them.

The gate covers `.rs`, `.py`, `.toml`, `.yml`, `.yaml` and `.sh` files, plus the scripts in `.githooks/`. Markdown is out of scope on purpose: an HTML comment in a doc is prose, and [`LANGUAGE.md`](../../LANGUAGE.md) and the docs checks govern prose. [`scripts/test-comments.py`](../../scripts/test-comments.py) proves the gate's behaviour, including the Rust `/* */` path, which no file in the tree currently uses.

## References

- [Rustdoc: how to write documentation](https://doc.rust-lang.org/rustdoc/how-to-write-documentation.html)
- [Clap 4.6.7 derive and doc comments](https://docs.rs/clap/4.6.7/clap/_derive/#doc-comments)
- [Rust Reference: lint reasons](https://doc.rust-lang.org/reference/attributes/diagnostics.html#lint-reasons)
