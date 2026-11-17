---
title: Agent failure recovery when a fix stops converging
summary: When to stop speculative edits, and how to reproduce, locate, check contracts, revise the hypothesis, verify and report an unresolved blocker.
keywords:
  - agent-process
  - verification
---
# Agent failure recovery

Read this page when a problem meets a condition in [When to escalate](#when-to-escalate). It governs how a coding agent debugs. It does not describe retry or fallback behavior in the shipped player.

## When to escalate

Enter recovery mode when:

- two meaningful attempts leave the same error or observable failure;
- an approach brings back an earlier failure or contradicts verified behavior;
- the behavior of an external API, crate, CLI option or feature is uncertain;
- compiler, test and runtime observations appear to contradict each other;
- repeated edits have not narrowed the possible causes.

A changed error with a narrower cause is progress. Rerunning an unchanged command, or trying a cosmetic variant of one fix, is not a new hypothesis. When a condition holds, stop speculative edits and name the assumption you have not verified.

## Recovery procedure

1. **Reproduce:** record the smallest failing command, the diagnostics, the environment, the expected behavior and the actual behavior.
2. **Locate:** search the source for the failing symbol, message or task, then read the code the search finds. Name any limit of a search you could not confirm.
3. **Check Unmei contracts:** read `GLOSSARY.md`, the relevant ADRs in `docs/adr/` and the spec for the feature. Treat settled decisions as constraints, not as hypotheses to reverse silently.
4. **Check versions and features:** read `rust-toolchain.toml`, `Cargo.lock`, the package manifests and the relevant Cargo features. Separate a local defect from a mismatch with a third-party API.
5. **Consult the authority:** use [documentation.md](documentation.md) to find version-matched official docs. Inspect the cached upstream crate source and release notes when needed. If an external contract may have drifted, check its current upstream documentation instead of assuming an old finding still holds.
6. **Revise the hypothesis:** name the assumption the evidence disproved, cite the evidence and choose the smallest correcting change. If the evidence is inconclusive, design an experiment that tells the causes apart instead of editing by intuition.
7. **Verify:** run a focused test that can tell the old failure from success, then the workspace gates that apply under [`docs/testing.md`](../testing.md).
8. **Stop and report:** if the blocker remains, report it as [Reporting an unresolved blocker](#reporting-an-unresolved-blocker) describes. Do not loop.

## Where to start for each area

### Engine (`engine/`)

Start with [`docs/content-format.md`](../content-format.md), the relevant ADRs and the engine module the failure points at: the parser, the pack loader, the runner, saves or `engine/src/decision/`. Verify observable execution, content validation, serialization compatibility and the authored fallback with focused engine tests.

For RON or Serde problems, inspect the locked crate versions and their source. For a Decisions wire problem, compare `engine/src/decision/client.rs` with [`docs/nanogpt.md`](../nanogpt.md) and the current official NanoGPT reference it links.

**Do not import Bevy into the engine.** Its runtime-independent boundary is intentional. **Do not add runtime Decisions retries:** [ADR 0008](../adr/0008-player-first-failure-posture.md) requires one call, a 30-second timeout and the authored fallback on failure.

### CLI (`cli/`)

Start with [`docs/author-cli.md`](../author-cli.md), [`docs/author-cli-drafting.md`](../author-cli-drafting.md), the affected command or provider code and `cli/tests/`. For option parsing, read the version-matched Clap docs. For a network protocol problem, read the provider's official reference.

Verify the exit status, the diagnostics, the generated output and that unrelated files stay unchanged. The CLI's authoring retry and error handling is separate from the player's no-retry Decisions fallback.

### Bevy player (`app/`)

For ECS schedules, lifecycle, UI, keyboard and clipboard input and rendering, use the **Bevy 0.19.1** docs and the locked source, not examples from another Bevy release.

For persistence and failure posture, read [ADR 0006](../adr/0006-save-slots-on-disk-format-and-version-policy.md), [ADR 0008](../adr/0008-player-first-failure-posture.md) and [`docs/runtime-hardening.md`](../runtime-hardening.md). For audio, read the compatible `bevy_kira_audio` release. Keep headless ECS tests apart from the shipping binary's schedule construction, its real boot and rendered evidence. [`docs/testing.md`](../testing.md) defines each gate.

## Verification

Start with a focused test for the target or feature configuration the failure involves:

```sh
cargo test --locked -p unmei-engine
cargo test --locked -p unmei
cargo test --locked -p unmei-app
```

Before you report completion, run the workspace checks that apply:

```sh
cargo fmt --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
```

For a scheduling failure, read `app/tests/shipping_schedule.rs`. For a boot or rendering regression, read [`docs/testing.md` § Shipped Path](../testing.md#shipped-path). Live external API tests depend on credentials and stay ignored by default.

## Reporting an unresolved blocker

Give the smallest reproducer and the exact observed failure. Summarize each distinct hypothesis you tried and its result. Link the repository contract, the official documentation or the upstream source you consulted. State the unverified assumption and the evidence needed next. A reproducible, honestly scoped blocker is better than speculative edits.
