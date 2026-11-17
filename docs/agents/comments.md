---
title: Code comment policy and its review classification
summary: When a code comment earns its place, the REMOVE, SHORTEN, KEEP and REFACTOR outcomes, the protected contracts and the review procedure.
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

An explicitly requested comment review is **report-only** by default. It covers the requested paths, or the current diff when no paths are named. Each proposed change names the location, the classification, the source context that supports it and any behavior or documentation risk. Do not optimize a comment ratio, and do not add a CI gate or a test suite for comments.

Edit comments only after the requester authorizes the change. Deleting an ambiguous contract and refactoring application code each need their own approval. For an edit, follow [`docs/testing.md`](../testing.md) and run only the checks that could detect a plausible regression. A documentation-only change does not need a Cargo test by default. Report exactly what you verified and what you did not.

## References

- [Rustdoc: how to write documentation](https://doc.rust-lang.org/rustdoc/how-to-write-documentation.html)
- [Clap 4.6.7 derive and doc comments](https://docs.rs/clap/4.6.7/clap/_derive/#doc-comments)
- [Rust Reference: lint reasons](https://doc.rust-lang.org/reference/attributes/diagnostics.html#lint-reasons)
