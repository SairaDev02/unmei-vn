---
title: Coverage evidence and reporting contract for item seven
summary: The item-seven coverage-evidence contract — measured tests and source, the subprocess capture witness, the command contract and the retained bundle.
keywords:
  - verification
  - evidence-record
  - ci
  - external-dependency
---
# Coverage evidence and reporting — item seven

**Status: contract settled; one complete baseline executed and reconciled as
historical-only for later inputs.**
Decision owner: this contract. Q1–Q13 settled the contract; Q14 confirmed shared
understanding and authorized its documentation and closure as planning only.
The decision is closed and this page is its surviving statement.

The baseline the contract requires was then executed once. It measured the
item-seven source revision on a GitHub-hosted `ubuntu-24.04` runner: 95.25% of
8145 production-file lines (7758), 92.52% of 816 functions, 94.58% of 13007
regions, over one combined 96-profile run of 514 passing engine/CLI tests. The bundle retains the identity manifest, the
export logs, the subprocess-capture proof, the risk dispositions and the
checksums. The numbers are reported, not enforced: nothing here is a gate.

A later reconciliation with the rest of item seven found that later `main`
commits changed two in-scope measured inputs, so the bundle is historical-only
for current inputs and a fresh complete run
is required before any current-input claim (none is authorized).
The obligation-to-receipt ledger, the exact drift, the pending disposition and
the later-owner handoffs are in that private reconciliation record, which does
not ship.

## Evidence and authorities

The planning session inspected the source at one revision; the baseline was later
measured at another.

Reuse the portable tooling findings gathered for this contract rather than
repeating that survey.
Command forms were checked against the pinned
[v0.9.1 README](https://github.com/taiki-e/cargo-llvm-cov/blob/v0.9.1/README.md)
and [report help](https://github.com/taiki-e/cargo-llvm-cov/blob/v0.9.1/docs/cargo-llvm-cov-report.txt).
The executed run confirmed the pin and found one documented-command
inaccuracy: `cargo-llvm-cov 0.9.1`'s `report` subcommand rejects `--features`,
so only the package selectors reach the exports. The retained run records the
observed error rather than claiming the documented candidate ran verbatim.

[Testing](testing.md) owns test selection;
[behavioral protection](testing-behavioral-protection.md) owns the selected
protections. Coverage does not replace either. Release evidence is governed by
[release certification](release-certification.md).

## Delivery boundary and freshness (Q1, Q7, Q13)

Item seven's actual **complete manual Ubuntu x64 baseline** — measuring the
final item-seven engine/CLI source, tests and locked inputs after selected
changes — is the one recorded above. The freshness rule that
guarded it still governs any successor: earlier runs remain historical, changes
to measured inputs require a fresh complete run, and a run is collected after
the relevant work settles, not automatically after every commit.

Use a clean product checkout and full measured SHA. Dirty exploratory runs are
non-baseline diagnostics. Later evidence-only documentation commits are distinct
from the measured revision. For unrelated later documentation/app-only changes,
explicitly establish unchanged measured-input applicability; never claim the
later SHA was instrumented. Candidate reports name their own candidate SHA.

Start with exact candidate `cargo-llvm-cov 0.9.1`, repository Rust/Cargo `1.98.1`,
matching `llvm-tools-preview`, committed `Cargo.lock` and `--locked` resolution.
Verify installation/integrity and actual runtime compatibility before adoption;
an alternative tool pin needs explicit maintainer approval. The toolchain's
minimal component list does not currently include `llvm-tools-preview`.
This contract authorizes no installation or toolchain edit.

Record actual Ubuntu distribution/version and x64 target, not merely a moving
runner label. Do not copy CI's debug-info-zero environment unexamined: verify
coverage-specific debug/instrumentation settings and record them. Different
platform/tool/scope policies are not equivalent comparison baselines.

## Measured tests and source (Q6, Q9, Q11–Q12)

| Included | Excluded or separate, with reason |
| --- | --- |
| Packages `unmei-engine` and `unmei`; explicit `unmei-engine/network`, other default features retained | No `--workspace`: it also selects the app. No automatic `--all-features` scope expansion. |
| Default test profile; library/binary unit tests and integration tests | No release-profile matrix, examples or benches: not the chosen routine test scope. No doctest coverage: pinned tool requires nightly for it. Existing ordinary regression policy is unchanged. |
| Non-ignored offline tests, including loopback Decisions/authoring fixtures using dummy keys | No real keys/provider calls, `--ignored` or `--include-ignored`. Engine live Decisions test remains ignored; app live test is outside package scope. Live scheduling belongs elsewhere. |
| Report allowlist `engine/src/**` and `cli/src/**`, checked against actual report paths | Default tool source-path exclusions retain separate test/example/bench contributions to production-file hits but omit those files from the denominator. Exclude app, dependency/toolchain/vendor, generated/build and scratch source. |
| Inline test modules as emitted inside allowed production files | No new source-range analyzer, manual count editing or blanket exclusion of low/credential-adjacent production source. |

The CLI dependency already enables engine `network`; name it explicitly anyway.
An engine-only default-feature report omits network-gated code entirely and is
not interchangeable with this baseline. `engine/tests/decision_resolver.rs` is
network-gated but non-ignored and uses loopback, not a live provider.

Label the metric **production-file coverage, including inline test code**,
not pure production-code coverage. Keep pinned default source-path filters and
record all additional filters, actual omitted paths and reasons. Check all
retained views against the allowlist. Compiled unexecuted source remains zero
where emitted; platform/feature-disabled or wholly unrepresented source gets an
explicit inventory/limitation, never assumed coverage. No claim covers Windows
code absent from the Ubuntu build.

Derive aggregate and engine-source/CLI-source views from the **same combined
run**. Show line/function/region covered and total counts with percentages;
label unavailable metrics unavailable, not zero or inferred. Sum applicable
counts, never average file/package percentages. Do not attribute hits to which
package's tests caused them. No separate package-only reruns, branch/MC/DC
requirement or numerical gate. A hit does not prove a meaningful assertion.

## Required subprocess capture witness (Q10)

Before the baseline, use an isolated existing assert_cmd test to establish:

1. Which instrumented `unmei` executable was actually spawned.
2. Child profile contribution and successful merge, not just inherited env.
3. A hit in child-only CLI startup code that an in-process library test cannot supply.

`cli/tests/cli_test.rs` obtains `unmei` through `Command::cargo_bin`; its helper
sets dummy keys but does not scrub profile variables. That is source evidence,
not observed capture. `help_shows_contract_surface` is a small existing startup
witness; `dry_run_writes_nothing` additionally drives loopback authoring and
validation. Choose the smallest existing witness that proves capture, retaining
its exact command, executable identity, profile/merge observations and startup-hit
receipt. Environment inheritance or a successful child exit alone is insufficient.

Use distinct witness coverage state, then clear it before the full baseline.
Do not add a broad harness/product test merely for measurement proof. If capture
fails, repair and reverify; baseline is incomplete until capture works. Never
label an uninstrumented child covered by assumption.

## Reproducible command contract (Q3, Q9–Q12)

The following was the **unexecuted command candidate** this contract settled on;
the baseline run then executed it from the repository root in an isolated clean
checkout after provisioning/verification and witness cleanup. Two facts are now
observed rather than assumed: `report` rejects `--features` in 0.9.1 (so the
exports carry only the package selectors), and `--html --output-dir X` writes
under `X/html/`. The retained bundle keeps the exact workflow and verifier that
ran. The executed run also passed `--locked` to the initial `clean` and drove the
whole sequence with `-vv` to
record the measured crates' rustc invocations; see the workflow copy for the
verbatim commands, which differ from the block below only in those additions.

```bash
set -euo pipefail
mkdir -p coverage/item7
scope=(--locked -p unmei-engine -p unmei --features unmei-engine/network)
ignore='(^|/)(app|\.pi-tmp)/|(^|/)build\.rs$'

cargo +1.98.1 llvm-cov clean --workspace
cargo +1.98.1 llvm-cov "${scope[@]}" --lib --bins --tests --no-report \
  2>&1 | tee coverage/item7/tests.log

cargo +1.98.1 llvm-cov report "${scope[@]}" --failure-mode any \
  --ignore-filename-regex "$ignore" > coverage/item7/summary.txt
cargo +1.98.1 llvm-cov report "${scope[@]}" --failure-mode any \
  --ignore-filename-regex "$ignore" --json --output-path coverage/item7/coverage.json
cargo +1.98.1 llvm-cov report "${scope[@]}" --failure-mode any \
  --ignore-filename-regex "$ignore" --html --output-dir coverage/item7/html
```

`clean --workspace` clears measurement artifacts in this isolated checkout; it is
not permission to select workspace tests. `--no-report` retains state, making the
explicit initial clean important. `report` exports without another test run.
The ignore expression supplements defaults; it is not a native positive-allowlist
switch. Verify emitted filenames are strictly within `engine/src` and `cli/src`;
if extra paths appear, record and correct explicit exclusions and regenerate all
views from the same intact profiles before declaring completion. Verify output
layout on the actual tool, and retain tool/report logs and each export status.

Start with clean dedicated coverage state; do not merge witness, live, previous
revision/host, cancelled/partial or selected passing fragments into the baseline.
Use the same package/feature/filter configuration for every export and verify
summary/HTML/JSON agree. Never use `--ignore-run-fail`, permissive profile-merge
failure mode, `--fail-under-*` or uncovered-count gates.

## Outcomes and risk review (Q3–Q4)

A completed baseline needs successful intended tests, verified instrumentation and
child capture, a complete valid profile set, every required consistent report and
a reviewed accessible retained bundle. Test success alone is not coverage success.
Instrumentation/merge failures, missing reports, cancellation or failed intended
tests mean failed/incomplete evidence. Keep reasons/partial diagnostics and each
retry as distinct attempts; repair/rerun before claiming the deliverable complete.
Existing ordinary test results retain only their own scope.

Review uncovered areas against a contract and plausible behavioral failure,
crediting existing protection. Record a justified action or explicit deferral with
impact/rationale/follow-up only for meaningful gaps. Low coverage alone creates
no automatic test, defect, refactor or release blocker. A demonstrated wrong
outcome follows the behavioral-protection contract's existing impact
classification; no assertion is weakened
to improve coverage. Its selected protections remain required regardless of
reported numbers.

## Retained bundle and privacy (Q2, Q5, Q7)

Initial maintained home: the private evidence set, which now exists. The completed
measurement sits in `<measured-revision>-push<n>-complete/` and the two incomplete
attempts stay under `attempts/<revision>-push<n>-incomplete/`, with all of them
reachable from this contract.

Retain a short human summary/risk disposition, browsable HTML, full LLVM JSON,
identity/command manifest, test/tool logs, compact subprocess witness proof and
checksums of retained artifacts. No redundant LCOV/Cobertura export, upload
service or routine commitment of raw build/profile trees. The compact witness
must preserve enough observation to distinguish actual child capture from an
inference after temporary profiles disappear.

Manifest requirements:

- measured full SHA, clean-state observation, lockfile hash and evidence-retention
  revision separately; time, operator/runner and attempt identity/status;
- actual `rustc -vV`, Cargo, cargo-llvm-cov and matching LLVM tool versions;
  Ubuntu/architecture/target and profile identity;
- exact commands, working directory, packages/targets/features, filters and actual
  exclusions; safe relevant environment settings, never a full environment dump;
- intended/executed/ignored inventory and disabled/unrepresented source limits;
  subprocess capture reference, each test/export/merge status and failure history;
- aggregate/source-owned counts and denominators, bundle paths/checksums,
  privacy review and risk-led findings/dispositions.

Inspect actual source text, absolute manifest paths, HTML/JSON metadata and logs
before sharing. Use dummy keys only; path remapping alone does not sanitize JSON
metadata. Unsafe mandatory artifacts prevent a publishable completed bundle until
resolved. If redaction is needed, disclose its extent and preserve interpretability
and checksums for the retained version; never silently alter measurement counts
or claim an unsafe/missing report was retained. Disposable scratch and expiring
Actions artifacts are not the baseline's durable home.

## Item-eleven and release handoff (Q8)

Item eleven owns Ubuntu CI automation and per-release **candidate-source advisory
reporting** using this contract, not another duplicate uninstrumented test suite,
percentage gate or release authority. Exact workflow composition/cadence is later
implementation work. Preserve existing required regression gates. Automating a
command later does not turn an unrun item-seven baseline into success now.

A release record states its own candidate's coverage status and cites that
candidate's report, never an earlier bundle the repository does not carry: the
first public candidate's coverage report is the public baseline. Candidate
bundles belong with release assets; temporary CI artifacts are staging. Missing
or failed advisory coverage remains visible but is not by itself a new
certification blocker. A required regression failure remains blocking under the
existing policy, even if also observed during coverage collection.

Coverage is source-execution evidence, not proof for exact archived executables,
Windows, app rendering, meaningful assertion strength or live-provider behavior.
Do not promote a historical baseline or advisory job success into those claims.

## Focused verification and resolution

The baseline run executed what this section previously listed as future work:
tool integrity/runtime compatibility and locked inputs, the intended
feature/target/test inventory, isolated child capture, one clean full run,
report consistency/allowlist/arithmetic, refusal to treat failed or incomplete
attempts as evidence, privacy review and durable bundle access. Its three
attempts, two of them incomplete and retained, are recorded in the bundle
index. Choose the smallest meaningful check on later work by the rules in
[testing](testing.md).

No new domain-glossary term: this is general testing/evidence vocabulary. No ADR:
the removable reporting setup does not meet the hard-to-reverse trade-off bar.
Sibling human smoke is closed as planning only; its checklist remains
preparation-only and item seven owes no fresh walk. Item eleven still owns Ubuntu
CI automation and candidate-time advisory reporting, and the private reconciliation
record holds the item-seven acceptance status.
