---
title: Testing policy for the engine, CLI and player
summary: Which tests each crate needs, how to fake external dependencies, and the validation gates and shipped-path evidence completion requires.
keywords:
  - verification
  - ci
---
# Testing Policy

This page is the test-selection authority for the workspace. It states what to test in `engine/`, `cli/` and `app/`, how to replace an external dependency, and which checks a change must pass before it is called done. It states no behavioral contract of its own: [the contract map](architecture.md#contract-map) names the page that owns each one.

## Purpose

Tests must protect meaningful Unmei behavior, contracts, and regressions. Prefer the smallest deterministic suite that distinguishes correct from incorrect behavior. Do not optimize for test count or coverage percentages.

## Test Selection

Before adding a test, identify the behavior it protects, a plausible failure it detects, and whether existing tests already cover that failure. Do not test every function, branch, or component mechanically.

Prefer independently established expected outcomes. Avoid tautologies, weak assertions, implementation mirroring, redundant cases, and tests that only confirm mocks or language guarantees.

## Engine (`engine/`)

Prioritize observable script execution, state transitions, choice routing, decision resolution, fallback behavior, serialization, and content-pack validation.

- Assert actual runner outcomes and state changes, not merely the presence of parsed commands or labels.
- Test invalid content against specific validation rules and actionable errors.
- Use synthetic packs for isolated failure cases and the demo pack for integration contracts.
- Do not duplicate parser, loader, and runner coverage unless each test protects a distinct failure boundary.
- Preserve compatibility tests when serialized representations are part of an established contract.

## CLI (`cli/`)

Prioritize command results, generated content validity, non-destructive editing, configuration precedence, and failure handling.

- Verify exit status, relevant output, resulting files, and error behavior.
- For dry runs, rejected candidates, and failed generations, verify that protected files remain unchanged.
- For accepted edits, verify the intended modification and preservation of unrelated content.
- Test retry limits and error classification as observable contracts, not incidental internal call sequences.
- Assert prompt or request contents only for required protocol fields, context, constraints, and documented output formats.
- Avoid duplicating equivalent tests at the helper and CLI levels without a distinct integration risk.

## Application (`app/`)

Prioritize player-visible behavior, gameplay progression, persistence, asynchronous decisions, input handling, and failure recovery.

- Use headless Bevy tests to exercise real systems and observable state transitions.
- Verify save/load fidelity, decision lifecycle, cancellation, fallback behavior, and absence of unintended side effects.
- Test UI navigation through input and resulting interaction state.
- Avoid asserting ECS internals, entity structure, component counts, or system execution order unless they constitute explicit contracts.
- Reserve exact dimensions, spacing, colors, typography, and hierarchy assertions for documented presentation requirements.
- Prefer representative UI state transitions over exhaustive permutations without distinct failure risks.
- Keep tests isolated from process-global logging and other shared state.

## External Dependencies

Use deterministic fakes and loopback HTTP servers for provider responses, timeouts, protocol errors, and retry scenarios.

- Mock external boundaries, not the behavior under test.
- Assert response handling and resulting state or files rather than mock calls alone.
- Keep live API tests separate, explicitly ignored by default, and credential-dependent.
- Do not require real API access for routine regression tests.
- Test actual integrations when simulation cannot establish the relevant contract.

## Test Design and Maintenance

- Choose normal, boundary, invalid, and recovery cases according to plausible risks, not a fixed quota.
- Reuse existing fixtures and helpers when they preserve readability.
- Do not create abstraction layers solely to support trivial tests.
- Preserve regression tests for previously confirmed defects.
- Preserve meaningful engine, CLI and Bevy integration coverage.
- Update assertions when requirements legitimately change; never weaken them solely to make tests pass.
- Remove redundant tests only when their distinct regression protection is demonstrably covered elsewhere.
- For important behavior, prefer independently expected regression cases and focused, bounded fault injection when safe. Mutation testing is not part of this policy: no campaign tooling is maintained, so demonstrate sensitivity with those cases rather than with a mutation score.

## Validation

Run focused local checks for changed behavior before pushing to avoid needless
CI reruns, then obtain the applicable workspace-gate results locally or through
the standard CI run. GitHub Actions remains the independent repository gate: automatic
Clippy and full workspace tests in `check.yml` are explicitly allowed at their
existing per-push/PR cadence. A workflow/documentation-only PR does not require
a redundant local full-suite pass; the completed standard GitHub `check` run can
supply its Clippy and workspace-test evidence. The standard CI gate remains:

```sh
cargo fmt --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
```

`--locked` makes Clippy and the tests verify the committed `Cargo.lock`; a
stale lockfile fails the gate instead of being resolved and rewritten under it.
`cargo fmt --check` stays flag-free because it resolves no dependencies.

The `check` job also runs the repository-invariant checker first
(`python3 scripts/check-invariants.py` and its `--self-test` sensitivity proof;
[`CONTRIBUTING.md` § Local repository invariant checks](../CONTRIBUTING.md#local-repository-invariant-checks)):
the engine's Bevy-free dependency graph (resolved with `--locked`, so a stale
lockfile fails the check instead of being rewritten) and the license-sync rule
between `deny.toml` and `packaging/about.toml` on every branch and PR, and the
two tracked-path rules on pushes to `main` and PRs into it.

The `boot` workflow (below) additionally exercises the shipped path; it runs on
demand, not on every push.

**Local execution policy.** Resource-intensive local verification
(instrumented coverage, slow builds, and diagnostics) should run in a clean, suitably provisioned WSL2 Linux-filesystem checkout,
subject to available memory and host stability. **Stop** rather than retry if
WSL2 memory exhaustion terminates the VM. Do not move heavy work back to hosted
CI without a separate decision. Observe memory,
CPU, and disk limits and stop resource-intensive work on instability. This does not
change the standard `check` CI gate or its cadence, nor does it waive selected
behavioral regressions, coverage work, or shipped-path observations.

Use feature-enabled and ignored live tests only when relevant and their prerequisites are available.

Report which behavior was verified, which commands actually ran, their results, and any remaining untested risks. Never claim validation from code inspection alone.

## Shipped Path

Ordinary headless harnesses install a narrower plugin set than the shipping binary. The shared-owner schedule gate below covers schedule construction, not renderer startup or launch sequencing. A green `cargo test --locked --workspace` alone is not evidence that the app starts. The shipped path is backed by an automatic schedule test on every push and an on-demand real-boot workflow.

**Automatic — schedule construction.** `app/tests/shipping_schedule.rs` calls the production `unmei_app::composition::schedule_app` with the one app-lifetime dev-mode value on, then runs frames under `ScheduleBuildSettings { ambiguity_detection: LogLevel::Error }`. A cyclic schedule, or an app-owned unordered conflicting pair, fails the workspace suite (the class of defect a `PostUpdate` ownership-drain cycle belonged to). `WinitPlugin`/`RenderPlugin` are intentionally absent: such a cycle is caught while building the schedule, before anything renders. The few third-party ambiguities the app cannot order (bevy_kira_audio's shared `AudioOutput`, `bevy_ui`'s internal `PostLayout` accessibility trio) are accepted explicitly in `UnmeiPlugin`'s `configure_sets` calls, so the test stays sensitive to app-owned regressions.

**Production owner (2026-10-17).** The real binary calls `unmei_app::composition::shipping_app`; the external gate calls `schedule_app`. Both consume one private `DefaultPlugins`-derived selection/configuration owner in `app/src/composition.rs`, with a coverage-preserving headless delta, source-backed atlas support and update-only gate semantics. RuntimeConfig/log result precede logging; kira precedes the insertion-only startup hook and UnmeiPlugin. Existing ambiguity acceptances, presentation ownership, both audio integrations and CI cadence are unchanged. Delivery-revision commands, fault controls, startup/sink observations, native evidence, fresh Windows human-reviewed frames and explicit unverified residuals are recorded against the delivery revision; that record's acceptance status, not historical prototype closure, determines the native-evidence claim. [Shipping composition](runtime-hardening.md#shipping-composition--proof-gated-shared-owner) owns the design and its evidence history.

**Feasibility proven — historical record (2026-10-15).** The bounded feasibility handoff behind that conditional shared composition was recorded passing on a throwaway prototype branch: the shared `DefaultPlugins`-derived owner built strict three-frame schedules over the real demo pack, the ownership-drain cycle and a distinct app-owned unordered conflict each failed the strict gate when re-injected, and Windows sink/resource witnesses plus bounded debug and release-`--dev` shipping boots on Windows and Linux passed. At that point nothing was extracted: the authoritative list remained the then-curated `shipping_composition`, and **no prototype content was on the shipping branch**. Headless success still does not establish Monitor capture, file sinks, rendering or binary boot. Existing app ambiguity acceptances remained unchanged, including `PresentationSet`'s deliberate self-acceptance; no new suppression, weaker assertion or CI-frequency change was made.

**Native acceptance re-observed on the carried candidate (2026-10-17).** The carried candidate was re-run from scratch: debug and release-`--dev` bounded boots on native Windows and, under a **Linux-CI-only** amendment, native Linux in CI, each panic-free to the bound. The actual binary's rendered borderless fullscreen from the loaded setting, embedded-font resolution, real input path and both audio integrations were observed on Windows (real GPU) and in CI (llvmpipe), with the captured frames, CI log and instrumented reports preserved alongside the delivery record. Linux real-compositor geometry/input/font proof remains an explicit residual (the Windows frames were human-reviewed on the real GPU); this narrows the historical Windows/Linux claim to a revision-bound observation and does not authorize extraction.

**Bounded feasibility outcome — historical feasibility unmet (2026-10-17).** Under the evidence contract in force at that point, the carried candidate was reconciled against the handoff's witness contract at one revision. Schedule sensitivity, actual-binary startup/sink and native boot all passed on their executed scope, but the contract is not completely satisfied: the key-check channel value stays presence-only and the Linux affected rendered cells stay **unmet-by-CI** under the Linux-CI-only amendment, which that handoff says cannot coexist with a fully-proven feasibility. No candidate-2 contract failure was observed; because the evidence does not establish that both allowed designs fail, this is not no-change. **At that point no extraction handoff was published** and the binary and gate were unchanged. [Shipping composition](runtime-hardening.md#shipping-composition--proof-gated-shared-owner) owns the historical record.

**Evidence amendment (2026-10-17).** A revised composition acceptance bar replaced the one above, and [shipping composition](runtime-hardening.md#shipping-composition--proof-gated-shared-owner) records it: Windows retains real-GPU human-reviewed affected fullscreen/input/font acceptance; Linux requires native CI debug/release-`--dev` boot, functional checks and console/session-log diagnostics, with real-compositor fullscreen geometry, physical input and visible embedded-font rendering **accepted unverified residuals, never PASS**. Checker composition is observed installation/phase plus source-verified production `HttpKeyCheck` construction/insertion and independent existing checker-boundary behavior tests, not an invented actual-binary supplied-key/verdict observation. These explicit changes permit the separate production extraction; they do not waive fresh delivery-revision native, startup/sink, strict positive/negative schedule or workspace evidence. The earlier observations remain historical.

**On demand — real boot.** The `boot` workflow (`.github/workflows/boot.yml`) builds the shipping binary and runs it under `xvfb-run` for a bounded window over a temp copy of `examples/demo-pack`: once as the standard debug build, once as a `--dev` release launch, and once as a normal (non-`--dev`) release launch, asserting the process ran to the bound with no panic lines. It needs the Linux build dependencies plus `xvfb`, `xauth`, `libxkbcommon-x11-0`, `libgl1-mesa-dri` and `mesa-vulkan-drivers`. It runs on `workflow_dispatch`, or is called by the candidate coordinator at a candidate commit; there is no schedule and no per-push trigger: the optimized builds dominate the run (and a failing job cannot save the rust-cache).

The rendered `monitor_smoke` walk is deliberately not part of `boot.yml`: it stays the human rendered residual described below, where a person checks the walk on a real compositor and GPU. Its earlier CI red was not an anchor defect. The walk held its synthetic `PgUp` for two frames, so on CI's ~130 ms software-rendered frames the drawer's held-key repeat cadence added a second page **on the frame the walk measured**, while the walk read its reading position from the drawn rows — which only reflect the reader's state after the frame that synced them has ended. The recorded event was therefore one page stale. The walk now holds a paging key for exactly one frame, measures a step one frame after it applies, admits an arrival under the held anchor (checked in the retained history), and compares the drawn top-visible row against the reader's own stored anchor. It takes an emulated per-frame delay from `MONITOR_SMOKE_FRAME_MS`; it passes locally (Windows, real GPU) at the 1280x720 reference, at native fullscreen and at 150 ms and 400 ms per frame.

**Manual residual.** That boot renders on the software rasterizer. The human Monitor rendered walk over every surface (real GPU, screenshots) remains the manual rendered gate. When a change touches UI, layout or rendering and you want evidence the CI job cannot produce, run the affected example locally (`cargo run -p unmei-app --example monitor_smoke`) and record the command and its observed result in the report.

## Release certification (planned)

The human-confirmed [pre-beta release-certification contract](release-certification.md)
composes this policy's existing evidence; it adds no second test-selection policy.
Its implementation belongs to the release work, not to this page. The
current workflows above are unchanged: normal release boot, archive provenance and
candidate coordination are **future requirements**, not checks already performed.

The plan binds an explicit published-main SHA to applicable integration results,
real shipping boots, the exact Windows/Linux player and CLI archives, crate
package evidence and manual maintainer authorization. Full real-GPU/rendered/input
acceptance is required on **Windows**; Linux's missing full rendered walk is an
explicitly accepted residual, not a pass. Both platforms still require actual
extracted-archive boot and functional acceptance. Linux functional evidence may be
manual or automated, but must prove the required operations, not just process
survival. The completed human rendered walk is historical feature acceptance, not
an open dependency or proof for different release artifacts. Missing or
unsuccessful required evidence blocks certification. A successful agent report,
headless schedule or machine-only candidate run cannot replace artifact-bound
acceptance or authorize publication. See the contract for staged crate publication
and its partial-release boundary, durable records, freshness, recovery and focused
future verification.

## Acceptance Rule

A test is justified when it protects a distinct, relevant failure scenario at a suitable level of abstraction. If it cannot plausibly detect incorrect behavior or preserve a documented contract, do not add it.
