---
title: Pre-beta release certification contract
summary: The portable release-certification contract for 0.1.0 — candidate identity, release set, blocking evidence, artifact custody and staged publication.
keywords:
  - release-certification
  - ci
  - verification
  - evidence-record
  - native-boot
---
# Pre-beta release certification

**Status: human-confirmed design, not implemented.** Interview Q1–Q15, with the final Linux-executor clarification Q16. This document is the portable release-certification contract for 0.1.0; it does not certify a candidate or authorize a release. Implementation belongs to existing pre-beta **CI/CD and beta releases (item eleven)**, after the existing packaging and documentation slices. No earlier dependency has been demonstrated. There is no new milestone, release-infrastructure track or dependency on the production composition extraction.

## Current boundary and evidence

The inspection distinguished the published `main` from the local `main` (20 commits ahead). The workflows, app launch, shipping-schedule test, README and CONTRIBUTING were identical between those revisions. Later local outcomes are not evidence for the published revision.

- [check.yml](../.github/workflows/check.yml) runs on branch pushes and PRs into `main`: actionlint/ShellCheck, repository invariants and their sensitivity self-test, formatting, locked workspace Clippy and tests. The shipping-schedule test is part of that suite, not a second suite to run redundantly.
- [shipping_schedule.rs](../app/tests/shipping_schedule.rs) runs strict headless schedules and frames over the demo pack with developer affordances enabled. It excludes the renderer/window backend and does not prove binary startup or rendering.
- [boot.yml](../.github/workflows/boot.yml) is manual-only Linux/Xvfb debug and release-`--dev` boot. It passes an explicit temporary pack path and checks bounded process survival/no panic. It neither exercises normal release mode nor packages or accepts a distribution.
- [launch/mod.rs](../app/src/launch/mod.rs) plays `demo-pack` in the folder holding the executable when no `PACK_DIR` is given, in every build and never from the working directory, and reports the release version through `-V`/`--version` ([runtime hardening § Launch grammar](runtime-hardening.md#launch-grammar-q1)). Certification still owns packaging and archive acceptance: this launch contract builds, publishes and accepts nothing.
- The accepted shipping-composition decision and the bounded feasibility outcome do not make production extraction a certification prerequisite. Keep the current independent schedule, boot and rendered witnesses.
- GitHub inspection: `gh api` against the repository's `branches/main` returned `protected: false`, required-status-check enforcement `off`; the branch-protection endpoint returned HTTP 403, “Upgrade to GitHub Pro or make this repository public to enable this feature.” Running CI is **not enforced merge approval**. No setting was changed.
- The completed trial of the agent checkpoint did not establish a benefit sufficient to adopt it. An agent completion report is not a release authority. Its removal is local history beyond the inspected published revision.

This contract and the dependency-policy amendment below sit in the pre-beta plan. The requirements below remain self-contained; dated planning snapshots do not override this contract. [ADR 0010](adr/0010-credentials-live-in-a-user-level-file.md) already establishes Windows/Linux scope and the downloadable-player posture; [Type system](title-and-menu.md#type-system) records the packaged-font license obligation.

## Approach and cost (Q1–Q2)

| Approach | Operational cost and failure posture | Decision |
| --- | --- | --- |
| Documented manual candidate procedure using existing workflows | Least implementation; humans must assemble/check source identity, results and artifact hashes. Checklist omissions remain possible. | Valid minimal alternative, not selected. |
| Small manually dispatched candidate coordinator | Adds orchestration, evidence/status handling and artifact retention; generates facts it owns while reusing existing checks. Human acceptance/publication stay separate. | **Selected.** No generic framework, evidence database, attestation service or always-on environment. |
| Release branch with stabilization | Same evidence obligations plus divergence/backport/branch lifecycle. A branch alone prevents no invalid release. | Not justified for the first beta; ordinary development-branch repairs instead. |
| Existing checks and judgment only | No setup, but normal-release boot, artifact provenance and evidence-completeness gaps remain. | Rejected for this release contract. |

The coordinator does not publish crates, create/publish draft releases, or require release-write credentials. The maintainer explicitly authorizes the irreversible stages and publishes manually. Under current GitHub controls this is a **maintainer-controlled procedure**, not a platform-enforced gate; the same account can bypass its own procedure. No second reviewer, mandatory Pi component or repository-plan change is required.

## Candidate identity and freshness (Q3, Q8–Q9, Q12)

1. Supply a **full commit SHA already published and reachable from `main`**. Validate that eligibility at dispatch, then freeze the SHA. Every source-consuming check/build/package job checks out that SHA, never a moving branch. Record the coordinator/workflow-policy revision separately; orchestration identity must not be confused with checked-out product source.
2. A later movement of `main` does not change the candidate. The eventual release tag must resolve to the frozen SHA. Source repairs, version bumps, lockfile changes, pack edits and packaging-script changes all require a new candidate and fresh certification. Do not edit a frozen checkout and call it the same candidate.
3. Reuse complete successful **main-push `check` results for the exact SHA** only when their required jobs/steps, run/attempt details and policy applicability are available. A PR merge-context SHA, branch badge or different revision is insufficient. Review material workflow/toolchain/security changes since that run; stale or incomplete policy evidence cannot be inherited. There is no arbitrary age-only expiry.
4. If reuse is unavailable/inapplicable, run the **existing check implementation** against the candidate. Item eleven must provide that composition path, not copy suites into a second verification policy or dispatch a moving ref and assume it checked the candidate.
5. Build each archive once and test **those bytes**. Hash archives and extracted executables with SHA-256, retain them as candidate workflow artifacts, then upload the same archives manually. An independently rebuilt binary from the same source is not the tested binary.
6. Record infrastructure-only retries as distinct attempts with reasons and prior failures retained. A new build environment, package payload or artifact set invalidates affected artifact acceptance and requires review of the remaining evidence's applicability. An earlier approval cannot cover silently replaced artifacts. If staged artifacts expire or are unavailable, rebuild and reaccept; an old green run cannot recover their identity.

## Public-repository amendment (decided 2026-11-03, amended 2026-11-10)

The maintainer decided on 2026-11-03 that public beta ships from a **separately reviewed public repository with fresh Git history**. This private repository stays private and is never the release source. This amendment changes where the candidate lives:

- **"Published `main`" means the public repository's `main`.** The frozen candidate in step 1 above is a full SHA reachable from the public repository's `main`. Every requirement in this contract applies to that SHA and that repository: exact-SHA `check` results, built-once archives, the dependency-policy verdict, the tag and staged publication. Private-repository SHAs and runs are not candidate evidence.
- **The export is recorded as provenance, not as certification.** Each export commit's message names the private source commit it was built from, a digest of the reviewed allowlist and a digest of the exported file manifest. The candidate record copies those three values into its top-level `provenance` object, and that object is `null` when the candidate commit is not an export commit. The evidence that the public tree is that export is a reproduced tree hash: the export is deterministic, so a rebuild from the source commit yields the public commit's tree hash. A private-repository pass cannot stand in for the public candidate's evidence.
- **The export is curated and free of dangling references.** The reviewed allowlist is a set of include and exclude path patterns over the source commit's tracked files. A tracked file that matches neither set, or both, makes the export refuse, and an untracked file never ships. It carries the product crates and the demo pack, `packaging/`, every tracked file under `scripts/`, `.githooks/`, the six exported workflows (`boot`, `candidate`, `check`, `coverage`, `deny`, `publish`) with the other `.github/` files, the top-level `docs/` pages with `docs/adr/` and `docs/guide/`, and the root files, which include `LANGUAGE.md` and the build files `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `deny.toml`, `.gitattributes` and `.gitignore`. It excludes agent guidance, research records, evidence bundles, the planning map, the rejected-request triage records and the export tooling itself. A private-reference check refuses any exported file that names a private issue, pull request or commit, a private path or name, or a link to a file the export does not carry. The public tracker starts empty.
- **Every export and every publication needs explicit maintainer approval.** That includes creating the public repository, each export into it and every release action.

This is a decided, planned amendment. It does not show that the public repository exists or that anything has been published from it. Item twelve delivers the reviewed allowlist, the private-reference check, the export tool with its prepare report, its single-commit push and the tree comparison that proves the match, the settings script that creates the public repository and reads its settings back, the private runbook and a proven export: the built tree committed alone in a fresh local repository, passing its own gate. Item eleven implements running the coordinator and certification in the public repository. The rest of this contract is unchanged.

## Dependency-policy amendment (planned)

The later human-confirmed CI lint decision adds a **fresh candidate-time cargo-deny advisories/licenses/sources verdict** to item eleven's future required evidence, after supported-graph baseline review and explicit maintainer approval of concrete license/source policy and exceptions. Bind the scan to the frozen candidate SHA, record policy/tool and advisory-database revision/fetch identity, and refresh the database: an inherited exact-SHA integration pass is not a current advisory verdict. A blocking policy finding or failed/unavailable refresh cannot yield positive certification; local offline results are provisional, with no silent stale-cache fallback. Reuse the same dependency policy as the planned push/PR and weekly/manual job, without another Rust build/test suite. This is a planned-only amendment, not evidence that dependency policy or certification is implemented; the CI lint decision does not authorize implementation, publication, a second coordinator or a new release authority. Its focused handoff owns activation proof and operational follow-up. The original certification decisions below remain otherwise unchanged.

## Coverage-reporting amendment (baseline executed)

The human-confirmed [coverage evidence contract](testing-coverage-evidence.md)
required item seven's complete manual Ubuntu engine/CLI baseline. The baseline
has now been executed: the measured revision on an `ubuntu-24.04` runner, one
combined engine + CLI profile set, with the retained bundle in the private
evidence set. Item eleven still owns later Ubuntu
CI automation and per-release **candidate-source advisory** coverage reporting,
not a percentage gate or duplicate uninstrumented regression suite. A release
record states its own candidate's coverage status and cites that candidate's
report; the earlier bundle is not carried here, so it cannot be cited. The first
public candidate's coverage report is the public baseline, and candidate bundles
are retained with the release assets. Missing/failed
advisory coverage stays visible but is not itself a new
certification blocker. Required regression failures retain their existing blocking
status. Source coverage does not establish exact archive-byte acceptance, Windows
coverage, app rendering or live-provider behavior. This amendment authorizes no
workflow change or execution and leaves the original certification requirements
unchanged. Item seven's acceptance-status reconciliation, including the
historical-only applicability of that baseline to later inputs, is recorded in the
private reconciliation record; it changes no certification requirement.

## Human-smoke cross-reference (planned)

The human-confirmed [player smoke checklist and blank evidence record](testing-human-smoke.md)
delivers item seven's checklist preparation and focused execution handoff, not a
fresh-walk execution obligation or a current feature acceptance. It preserves
that decision's later maintainer-confirmed historical pass and separately attributes
shipping-binary observations, controlled examples/tests and unobserved cells.
Later items nine/eleven still owe this contract's fresh exact Windows/Linux
archive boot and functional acceptance, Windows real-GPU/rendered/input proof,
and the explicitly accepted unverified Linux full-rendered residual. Historical
feature evidence, headless schedules and bounded process survival cannot replace
those proofs. No additional gate, workflow, packaging work or change to release
authority is authorized by the checklist decision.

## Item eleven plan amendment (planned, 2026-11-07)

The item-eleven plan settled how this contract is implemented, and its handoff
assigns the work. The changes below amend this contract. Everything else in it stands. This is planned only: none of it is built yet.

- **Gate.** The coordinator always reruns `check.yml` at the candidate SHA through `workflow_call`. Reusing an earlier main-push result (step 3) stays permitted but is not used. An earlier run is linked as context only.
- **Linux build host.** Linux archives are built on `ubuntu-24.04`, so the Linux floor is **glibc 2.39**, because `ubuntu-22.04` retires on 2027-04-17.
- **Linux shipping boot.** `boot.yml` adds the normal (non-`--dev`) release boot and is called by the coordinator, which fills its own slot.
- **Linux functional acceptance.** It comes from a deterministic automated walk of the exact archive on a hosted `ubuntu-24.04` runner, as Q16 allows. The walk uses Xvfb with `xdotool windowfocus` and asserts from logs, sidecars and exit codes, not from process survival. The Linux full-rendered walk stays an accepted unverified residual.
- **Windows.** The hosted runner checks the archive's integrity and its command-line behaviour only. The archived player's boots and its functional and rendered acceptance on Windows remain the maintainer's real-GPU walk.
- **Record.** The machine record is immutable for each run attempt. It holds named evidence slots, each `passed`, `failed`, `pending`, `not-run` or `inconclusive`, and a separate `residuals` list. It never states `certified`. Later evidence is an appendix that cites the record's SHA-256.
- **Tag timing.** The `v<version>` tag is pushed at the frozen SHA before the human runs, because shipped Guide links are tag-pinned. Before any publication, a failed candidate may be re-tagged at the same version. Once anything is published, the tag never moves.
- **Publication after 0.1.0.** An approval-gated `publish.yml`, never the coordinator, may publish later versions through crates.io trusted publishing. 0.1.0 itself is published by hand with a short-lived scoped token, and the coordinator holds no credential.
- **Dependency verdict.** It comes from `deny.yml` called at the candidate SHA. A failed advisory-database fetch, or a run without a summary, is `inconclusive`, never a pass.
- **Coverage.** Candidate coverage comes from `coverage.yml`, called at the candidate SHA. It stays advisory.

## First-time acceptance fallback (decided 2026-11-08)

The default stays as planned: a human stranger run on the candidate, after the export and the tag, before any publication. The stranger is someone who has never seen Unmei and has no Rust. This amendment adds a fallback for when no participant can be recruited.

- **Who decides.** Only the maintainer, explicitly, at release time. The choice and its reason, for example "no participant recruited", go in the release attempt log. Nothing selects the fallback automatically.
- **Evidence that replaces the stranger run.** Two things together:
  - the maintainer's Windows walk also covers the first-time operations, using only the archive README and the public Guide: the player launches, finishes the offline demo, saves and loads, and finds the API-key Settings; the author completes the documented sample operation without Cargo. The first-time documentation acceptance row above already allows the Windows walk to supply this evidence;
  - item ten's sealed agent dry run, recorded in the private evidence set. That run used rehearsal archives, not the candidate's bytes, and the stranger appendix says so.
- **Where it is recorded.** The stranger appendix records that the fallback was used and cites both pieces of evidence. The release notes name the human first-time run as an open item. The record's `residuals` list is unchanged: the deferral lives in the appendix, the attempt log and the release notes.
- **When the open item closes.** It has no fixed version. It closes when a human stranger run passes on a later candidate, or when a later decision accepts the maintainer's planned agent test harness as first-time evidence. Until then, each release's notes keep naming it, and each release uses the default or this fallback the same way.
- **What the fallback does not change.** A blocked required step found by the maintainer's walk still means an issue, a fix and a new candidate. Every other blocking row stands.

## Compatibility during 0.x (decided 2026-11-10)

During 0.x, Unmei makes no format-stability promise. The pack format, the save format, the settings file and the authoring config (`unmei.author.ron`) can change in any 0.x release. Two refusals hold in every release: a build refuses a save slot whose format version is newer than it supports ([`runtime-hardening.md` § Save slots, 3. Version policy](runtime-hardening.md#3-version-policy)), and the pack loader refuses a pack that declares a field it does not know ([`content-format.md` § 6 Manifest and state](content-format.md#6-manifest-and-state)). A reader follows the docs of the release they use.

## Release set and blocking evidence (Q4, Q6–Q7, Q10)

One release-level record covers **four x64 archives** (Windows player, Windows CLI, Linux player, Linux CLI) and **two crates.io packages** (`unmei-engine`, then `unmei`). macOS is outside 0.1.0. These are the already-planned release products, not implemented distributions or an expansion of feature scope.

The planned player archive contains `unmei-play`, `demo-pack/`, a short player README and required license texts, including both embedded-font license texts. The planned CLI archive contains `unmei` and short author instructions pointing to the documentation on-ramp. *(Amended 2026-11-05, item nine's packaging contract.)* The archives are `unmei-play-<ver>-x86_64-windows.zip`, `unmei-play-<ver>-x86_64-linux.tar.gz`, `unmei-<ver>-x86_64-windows.zip` and `unmei-<ver>-x86_64-linux.tar.gz`, each with one top-level folder. Each carries `LICENSE` and a `THIRD-PARTY-LICENSES` file for its binary and target. The CLI archive also carries a copy of `demo-pack/`. The player archive's license texts include the vendored Bevy `FiraMono-LICENSE`. `scripts/package.py` builds them per host OS and writes `SHA256SUMS`. **Blocking prerequisite:** the existing packaging/documentation slices must first implement these products and usable launch/default-pack behavior, including beside-executable pack lookup. Until then, item eleven cannot certify the distributions. Do not package developer credentials or launch-generated sidecars as fixtures.

| Evidence | Release requirement and limits |
| --- | --- |
| Existing Ubuntu integration | actionlint with configured ShellCheck; invariant self-test and applicable engine/path checks; fmt; locked workspace Clippy/tests, including strict shipping schedules. Preserve sensitivity, branch/PR coverage and existing CI cadence. No new Windows per-push suite. |
| Linux shipping boot | Retain debug and release-`--dev` real boots; add **normal non-`--dev` release** boot separately. A software-rendered boot does not establish real-GPU acceptance. |
| Actual extracted player, both OSes | Normal release **and `--dev`** boots using the archived executable, not `cargo run` or a different build. Existing Linux debug/developer boots do not substitute for these artifacts. Windows debug is not an additional release requirement. |
| Archive functional acceptance, both OSes | Extract exact player/CLI archives to fresh directories; operate without a Rust toolchain and from a different working directory. Verify default pack lookup beside the player executable, complete the offline demo, save/restart/load and a deterministic documented CLI authoring/validation operation. Record supported environment/runtime prerequisites. An Ubuntu source check is not Windows evidence. |
| Full rendered/input acceptance, **Windows** | Real compositor/GPU, exact distributed player: title, play/choices/offline fallback, menu/Settings, save/restart/load, Gallery/Timeline, normal-mode absence of developer affordances, and Monitor/input-ownership paths with `--dev`. Record bounded steps, display/hardware and representative screenshots. Existing affected-surface examples remain supporting evidence, not substitutes for the archived binary. |
| Linux rendered residual | **No required full real-GPU/surface/input walk.** Explicitly record it as unverified and accepted by the maintainer, never passed. Linux archive boot and functional acceptance remain mandatory. Certification makes a narrower rendered claim, not full cross-platform visual assurance. |
| First-time documentation acceptance | Give a player only the extracted player archive/README, and an author the CLI archive/documentation on-ramp; verify the intended operation without maintainer guidance or Cargo. Record blocked steps as issues and their disposition; an unresolved blocker of a required operation cannot count as a pass. The Windows primary walkthrough may supply this evidence. |
| Crate package acceptance | Stage and validate intended package payloads with frozen source/locked dependencies, establish actual publish-payload identity, and perform publish dry-runs respecting registry dependency order. See staged publication below. |
| Advisory | Live-provider/nightly tests and performance/observability data do not automatically block certification. Existing intentionally ignored live tests remain ignored. A normal release does not require a secret/live-provider call to pass routine regression evidence. |

**Linux functional executor (Q16):** either a human or a deterministic automated walk, including software-rendered execution, may supply the required functional outcomes. Record the actor/runner, environment, artifact hashes, steps and observed assertions/results. Bounded process survival/no-panic output alone is insufficient; automation must actually establish every required operation. This does not turn software-rendered functional evidence into a full real-GPU/input pass or require a new harness in this ticket.

**Monitor witness ownership:** the Monitor rendered walk is completed historical feature acceptance, not an open dependency to wait for or a reopened work item by this contract. Its fixtures and evidence support the existing policy; its earlier SHA cannot certify different release artifacts. The candidate's Windows artifact-bound walk supplies the release-specific rendered evidence, without requiring every fixture-only case to be reproduced in the distributed executable.

**Refusal rule:** a failed, skipped, cancelled, missing, unavailable or inconclusive **required** check means **not certified**, never an implicit pass or waiver. Repair and recertify, rerun with justified evidence, or wait. The explicitly scoped Linux full-rendered residual is not an unavailable required check. Retain [testing.md](testing.md) as the sole test-selection authority; this contract composes evidence, not another test policy.

## Record and artifact custody (Q5, Q9, Q13)

Generate a small machine-fact record and SHA-256 file; append a human acceptance/sign-off record. No generated automated-success label may imply manual acceptance or permission to publish. Required information:

- candidate SHA, release version/tag and eligibility; workflow/policy revision; run/attempt identities, URLs, checked-out source, required results and any reused evidence/applicability decision;
- actual Rust toolchain/build identity, committed `Cargo.lock` identity, target OS/architecture, build profile/options and relevant environment;
- archive/executable and crate package names, versions and hashes; which exact bytes each boot/manual result exercised; artifact custody and replacement/retry history;
- tester, time, OS/hardware/display, commands/steps, observed outcomes and durable screenshot/evidence attachments or links; explicit Linux full-rendered residual, other residuals and their disposition;
- stage approvals and final explicit maintainer authorization, publication outcomes, tag resolution, downloadable-asset hash checks and registry package identity/checksum.

Temporary Actions artifacts support acceptance only. At completion, retain the machine record, checksums and human appendix with the **release assets**, including enough check-result detail to interpret them after CI logs expire. Release notes state the source SHA/tag, check-run links, narrowed rendered-platform claim and explicit maintainer sign-off. Keep secrets out of attachments. A separate evidence store or a source commit merely to record its own candidate's results is unnecessary.

Before publishing release assets, verify versions/names, tag-to-SHA identity, complete required evidence and the uploaded archives' correspondence to tested hashes. Recheck downloadable bytes afterward. A mismatch blocks completion and, if something is already published, is a visible incident—not a reason to overwrite quietly or move the tag.

## Staged publication, not an atomic transaction (Q11, Q15)

Distinguish **observed CI success**, **partial candidate evidence**, **authorization of a bounded publication stage**, and **complete release certification/publication**. The coordinator may finish successfully while human evidence is pending; its success is only the machine subset.

1. Complete archive/manual acceptance and all available package checks before any irreversible upload. No already failed required check may be bypassed. Retain intended engine/CLI package payloads and their source/hash identity.
2. The maintainer explicitly authorizes publishing `unmei-engine` first. Before upload establish that the actual Cargo publish payload corresponds to the staged candidate package; after upload verify registry identity/checksum and retain evidence. Item eleven must demonstrate this mechanism; **this design has not proven it**. If identity cannot be established, stop rather than claim provenance.
3. If CLI registry-dependent validation/dry-run needs the newly published engine version, name it **pending** until that prerequisite exists. A local path-dependency check alone cannot establish registry publishability. Engine-stage authorization is **not full release certification** and does not turn the pending CLI check into a pass. Run the newly available checks, then obtain separate explicit authorization to publish `unmei` with the same package-provenance obligations.
4. Only after every required component/evidence item succeeds, with the declared Linux rendered residual accepted, may the maintainer record full certification and complete GitHub release publication using the retained archives. Verify the final package/assets/tag identities and record completion separately from intent.

Crates.io publication can **partially succeed** and a version cannot simply be replaced. If engine publishes but CLI or later asset publication fails, stop and record a partial release. Do not claim the whole release shipped, retag to conceal the failure, or reuse a version for different bytes. Unchanged certified inputs can support a justified infrastructure retry; repairs need a fresh candidate and explicit version/recovery decision. No automatic rollback, yanking or autonomous publication is implied.

## Failure recovery and post-beta boundary (Q12, Q14)

Source repairs follow ordinary development branches, land on `main`, then produce a new candidate SHA. Agents may propose patches or collect evidence, never replace certified artifacts, authorize publication or bypass a failed gate. No automatic repair loop is required. After two meaningful failures of the same problem, stop speculative attempts, identify the unsupported assumption and obtain source/environment evidence or report the blocker.

For beta, reject relaxed pre-merge verification and autonomous publication/bypass. Defer post-merge automatic repair, richer CI risk/observability and agent-checkpoint integration. No new source-control system, post-merge-only CI or generic release framework is justified here.

Use a small manual log, not a telemetry project. Review after the first **three release attempts**, or earlier when:

- one escaped regression or incorrect certification is attributable to missing/stale evidence;
- two attempts repeat a coordination/transcription failure, or one attempt exceeds **two hours of evidence-coordination work**, excluding real product fixes;
- CI queue wait exceeds **15 minutes in at least 5 of 10 observed runs**;
- if actually measured, agents spend **over 20% of elapsed work time waiting for CI in 5 of 10 observed implementation sessions**.

Retain samples/denominators and investigate the demonstrated problem. A trigger authorizes review, not automatic adoption of repair loops, weaker checks or a new CI cadence. Revisit platform enforcement if the repository plan/visibility changes; no settings change belongs to this design ticket.

## Item eleven handoff: focused future verification

These are **future requirements, not commands or tests performed by this contract**. Choose minimal deterministic checks and real-environment witnesses through [testing.md](testing.md).

1. A clean eligible SHA produces an identity-bound machine record; every job uses that SHA and records workflow/policy identity. Local-only or mutable-only input cannot produce certification. Main movement cannot change the candidate or tag target.
2. Exact-SHA complete main-push evidence is reusable; wrong-SHA/PR-merge, unavailable, incomplete or inapplicable-policy evidence is rejected or rerun through the existing gate. Branch/PR coverage and failure sensitivity remain unchanged.
3. Failed/skipped/cancelled/unavailable/inconclusive required evidence cannot yield a positive complete result; machine-only completion cannot imply manual approval. Preserve expected live-test ignores and the explicit Linux rendered residual.
4. A later source/lock/version/packaging edit requires a new candidate. Same-SHA rebuild, changed environment/payload or expired artifact forces fresh affected acceptance; stale human approval cannot follow replacement bytes.
5. Normal release boot is distinct from `--dev`. Both target OSes exercise exact extracted artifacts without Cargo, from another working directory, including default-pack lookup, offline/save/restart/load and CLI behavior. Windows supplies real-GPU/rendered/input and first-time-user evidence; Linux's full-rendered limitation stays visible. Linux manual/automated functional evidence must assert actual required outcomes, not merely process survival.
6. Archive and executable hashes follow the tested bytes into uploaded/downloaded assets. The final record remains understandable beyond Actions retention; missing records or tag/hash mismatch block completion.
7. Prove staged `.crate` payload versus actual Cargo-upload/registry identity, publish dry-run and dependency-order behavior using version-matched Cargo documentation/source. Make engine-only partial publication and pending CLI checks visible; never infer an atomic release or treat a local path check as registry validation.
8. Exercise retries/failure records and the manual publication boundary without granting the candidate coordinator write/publication authority or requiring a Pi extension. No automatic repair, settings changes or real publication is needed merely to test refusal behavior.

This contract delivers the human decision, comparison and portable documentation only. No workflow, runtime, test, repository setting, release, commit or push is delivered by it; implementation remains in item eleven. Generic release terminology adds no domain-glossary entries, and this intentionally removable workflow does not warrant an ADR.
