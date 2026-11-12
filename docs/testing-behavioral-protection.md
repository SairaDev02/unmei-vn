---
title: Behavioral protection and test gaps for item seven
summary: The item-seven planning ledger of required behavioral protections — Decisions, CLI diagnostic safety and filesystem handoffs with their evidence.
keywords:
  - verification
  - evidence-record
  - decisions
  - credentials
  - terminal-mask
  - diagnostics
---
# Behavioral protection and test gaps — item seven

Decision owner: this planning ledger, which is the decision's portable published
resolution.

**Status: human-confirmed planning resolution (Q12).** This is a planning ledger,
not implementation, a test-result receipt or item-seven acceptance. The maintainer
confirmed the complete understanding and authorized closing the decision as
planning only. No production/test/CI changes are authorized by this document.

## Evidence boundary

Source inspected at the revision this decision read.
The change from the earlier charting baseline is the trajectory document only;
production and test sources are unchanged between those revisions. Test assertions
read during this session are **source-inspected**, not newly executed passes.
Retained historical observations, when cited, remain bound to their original revision
and scope. No fresh Rust test, build, provider call, coverage or mutation run has
been performed in this decision session.

Tool safety blocked the parent's direct reads of `app/src/credentials.rs`; no
alternate route was attempted. Delegated unit-test observations about that file are
not parent-verified source evidence. Credential storage policy is established by
ADR 0010; directly inspected app integration tests and the real-sink proof are
credited separately below.

Test selection follows [testing](testing.md): credit existing protection,
identify a plausible distinct failure and its owning boundary before proposing a test,
and do not use percentage, test-count, file-size or one-test-per-function quotas.

## Confirmed decisions

### Q1 — Layered Decisions matrix with one connected app witness

Complete required rows at their owning boundary: absent/invalid key, timeout,
HTTP 429, malformed JSON, wrong answer type, missing/unknown action and valid
responses. Every failure must select the configured authored fallback and reach the
next wait; valid responses must route correctly.

Connect real-client results to a real runner for the network matrix's progress checks,
and add one representative real-client-to-headless-app failure/progression witness
for cross-crate wiring. Reuse existing evidence and strengthen suitable assertions;
do not duplicate every matrix row through the app. Keep existing absent-client app
progress protection. All proposed witnesses are offline and deterministic.

A fake returning a pre-resolved fallback proves app lifecycle/progression, not the
real client's fallback selection. Transport/decode, pure fallback, runner progress
and fake-resolver lifecycle remain distinct evidence categories.

### Q2 — Narrow CLI diagnostic safety is required for beta

The maintainer explicitly requires a separate focused CLI diagnostic-safety handoff
before eventual item-seven completion. This is a new CLI policy decision, not a
retroactive claim that app protection repaired the CLI.

Reuse the findings already settled for this decision rather than a second survey:
the authoring CLI is the live instance of the known source-level exposure (it
retains response bodies and carries key-bearing `Debug` values across its provider
seam), and the locked-HTTP dependency analysis settled the dependency-side facts —
a bearer header the typed client marks sensitive, transport errors that can carry
a full URL and connection debug output limited to origin context. This decision
does not change credential storage.

### Q3 — Sanitization also covers model and validation output

The maintainer chose **also sanitize all model/validation output against known
credential values**, rejecting the recommended intentional-content exception.
Do not record terminal model-generated output as exempt from the chosen rule.
This adds sanitization to categorical provider diagnostics, opaque client/config
Debug, sensitive bearer headers, explicit wire-verbosity-off and disabled redirects;
it does not retain raw provider errors behind a last-minute output mask. Structured
support detection may inspect responses transiently but returns fixed safe detail.
Env-only sourcing, the 120-second CLI timeout and retry classifications stay unchanged.

### Q4 — Observe the production Decisions timeout

Require one bounded stalled-loopback test using the real production constructor and
30-second timeout, then assert authored fallback and the next runner wait. Bound
fixture cleanup and completion; do not repeat this slow witness at every layer.
It is offline deterministic regression evidence, not an ignored live-provider test.
Actual suite/CI cost is unmeasured. The shipped 30-second/no-retry contract is unchanged.

### Q5–Q6 — Known credentials and terminal/file boundary

Snapshot both non-empty recognized environment values (`UNMEI_API_KEY` and
`OPENROUTER_API_KEY`) for the command, including the non-selected value. Replace
exact literal occurrences throughout CLI stdout/stderr, including previews, dry runs,
validation feedback and best-output reports, with a fixed marker. Handle overlapping
values without partial leaks; do not disclose the recognized-value list. Preserve
ordinary output otherwise. No claim covers unknown secrets, fragments, arbitrary
encodings or paraphrases.

This is a **terminal-output rule only**. Do not silently rewrite generated pack files,
provider prompts or correction payloads. Generated and author-supplied content is not
certified secret-free and must be reviewed before sharing; a sanitized preview does
not establish that the file is safe. No generated-candidate secret-refusal rule or
credential-storage change is introduced.

### Q7 — Three additional distinct protections

Require focused protection for both-key CLI precedence, malformed author-config
refusal and the engine read-log decoder-preservation boundary. These protect existing
contracts; they are not freshly reproduced product defects or a reason to duplicate
higher-level suites.

### Q8 — Both filesystem evidence gaps are required, not deferred

The maintainer rejected the proposed deferral: legacy-import final-rename failure
and interrupted atomic replacement protection are both required before eventual
item-seven completion. Existing per-owner failure tests still receive credit.

### Q9 — Deterministic filesystem error handling, not crash durability

Narrowly inject I/O failure after partial temp write and at replacement in the actual
owning writer; assert failure, old target bytes and the owner's documented temp
postconditions, then successful whole-file retry. Credit existing phase-error tests;
do not unify the two writers or add a broad/public filesystem test framework.

The shared engine sidecar writer already tests write failure and complete-temp
retention on rename failure (`engine/src/sidecar.rs:122–152`). It discards partial
temp after write error and deliberately retains complete new bytes after rename
error. The CLI content writer is separate (`cli/src/splice.rs:91–101`): on rename
error it removes the temp and preserves the target; its successful replacement is
already tested. Do not infer an additional CLI partial-temp cleanup guarantee from
the shared writer's policy. Protect existing-target bytes at the missing interrupted
write/replace boundary, rather than merely duplicating directory-target cases.

For legacy import, block the `.imported` destination so rename fails after successful
slot write. Require failure, the correctly written target slot, byte-identical still-
importable legacy source and untouched other slots. No rollback is invented.

Deterministic failpoints suffice: no process-kill harness, power-loss durability,
fsync protocol or new OS atomicity guarantee is required.

### Q10 — Classification, impact and handoff boundary

A missing test is a plausible documented failure lacking adequate assertions at its
owner, not proof of broken behavior. A product defect needs a wrong observable
outcome or a direct source-level violation of a confirmed contract, with the witness
or trace retained. A fixture/setup/build failure is neither a pass nor automatically
a product defect. No newly reproduced runtime defect was established here; the known
CLI exposure is an unimplemented newly adopted policy requiring repair and protection.

Confirmed data loss/destructive edits, supported-path credential disclosure, wrong
authored routing, unbounded stall, broken save refusal/preservation or loss of ordinary
input/recovery are release-blocking. Smaller diagnostic/usability/evidence findings
may be explicitly deferred only with impact, rationale and follow-up. **All selected
protections in this ledger remain required**, including both filesystem gaps; that
scope requirement does not imply that their untested paths currently contain defects.
A new behavioral failure requires a focused repair and regression, never a weakened
assertion. Closing this decision as planning only approves planning, not a build
or item-seven delivery.

### Q11 — Durable policy record

The maintainer requested [ADR 0014](adr/0014-cli-diagnostics-and-terminal-credential-safety.md),
accepted but not implemented. This makes the new CLI policy and its deliberate
terminal/file distinction explicit without reopening ADR 0010's storage decision.

## Decisions evidence ledger (source-inspected; confirmed dispositions)

| Contract / plausible failure | Existing evidence and its limit | Gap / confirmed disposition |
| --- | --- | --- |
| State filtering and valid `choice` answer; wrong action routes play | `engine/src/decision.rs:512–564` asserts filtered request/wire shape, real-shaped response and successful `none`; `engine/tests/decision_resolver.rs:110–128` asserts real HTTP client/trait success and model-pin handling. | Credit existing protection; connect applicable real-client matrix outcomes to independently expected runner waits. |
| Wrong answer type / missing question / unknown action must not become a model answer | `engine/src/decision.rs:566–610` asserts missing answer, wrong/unknown type and action outside criteria. | These are pure decoder/validator checks, not HTTP-to-runner progress. Complete the network matrix without relabelling these as absent coverage. |
| Any client failure selects authored fallback | `engine/src/decision.rs:630–638` checks `Http` -> `cold` and reason/model; typed fallback at `:749–756` checks bool/index landing keys. `engine/tests/decision_resolver.rs:130–136` asserts real HTTP 500 -> `cold` and fallback state. | Preserve pure and real-client evidence separately. Matrix needs exact authored landing and next wait, not merely `is_fallback()`. |
| Decode / unknown action / refused connection remain categorical through real client and actual sinks | `app/tests/credential_safety.rs:400–463,505–518` drives real client failures; `:615–712` requires expected events in console/session-log/Monitor and zero loss/eviction/truncation, then rejects reflected dummy material. | Substantial client/fallback/sink protection already exists. It does not drive a live app runner or assert the authored landing in each error case. Reuse, do not rewrite as a new sink suite. |
| No configured client still advances fiction | `app/src/tests.rs:154–178`, `decision_without_a_client_takes_the_author_fallback`, checks Ace's authored cold say-line and cleared pending task. | Credit as existing absent-client-to-next-wait evidence. Credential sourcing is a separate boundary. |
| Typed valid answers map to landing keys, not state or score scalar | `engine/src/decision.rs:697–731,772–872` checks noul threshold, score argmax/ties/out-of-legend entries and unusable-answer fallback. ADR 0005 deliberately defines bare noul threshold, including out-of-range numeric values. | Credit existing typed runner protection; do not invent stricter probability validation or duplicate every invalid response for all three question types. |
| Stalled endpoint must not hang play indefinitely | `engine/src/decision/client.rs:39,70` configures 30 seconds; `:198–200` maps transport failure to categorical `Http`. | Configuration source is not an executed timeout witness. Q4 requires one production-duration stalled-loopback witness; no shipped timeout/retry change. |
| Invalid key / rate-limit status must fall back | `engine/src/decision/client.rs:202–209` maps all non-success statuses to `Status`; inspected real-client delegation fixture is HTTP 500. Settings `verdict_from_status` tests cover 401/403/429 for key checking, a different request/behavior. | Add/credit actual Decisions-call 401/403 and 429 fixtures; key-check verdict tests cannot stand in for Decisions fallback and progress. |

### Required matrix — exact outcome obligations

For network rows, exercise the actual `DecisionsClient` fallback/decode path with a
loopback endpoint and feed its result to a real `ScriptRunner` over a fixture whose
expected landing say-line/state is independently authored. Assert both the exact
landing key and next wait; `is_fallback()` alone is insufficient. Use the resolver
entry point where practical and retain the existing explicit trait-delegation test.
No client/fallback selection is mocked by supplying a pre-resolved answer.

| Scenario | Existing credit | Required completion evidence |
| --- | --- | --- |
| Absent key/client | Existing no-client app test reaches authored `cold` say-line; credential sourcing is distinct. | Credit that test plus the relevant absent-key credential-selection assertion; do not send an empty bearer merely to simulate absence. No duplicate network row. |
| Invalid key | Settings key-check status tests are a different request. | Real Decisions 401 (403 is the same rejection-status class) -> categorical status reason, authored fallback, next runner wait, no retry. Dummy key only. |
| Timeout | Production constructor visibly sets 30 seconds; refused-port transport fixtures do not observe expiry. | One held-open/stalled loopback request with actual production constructor expires, falls back and reaches next runner wait. Bounded fixture/test cleanup and no retry; do not duplicate at app layer or change shipped timeout. |
| HTTP 429 | Generic non-success branch is already exercised by HTTP 500/redirect status tests. | A rate-limit response takes fallback and progresses without retry/backoff; assert categorical 429 reason. This guards a plausible later special-case change, not an untested generic status parser. |
| Malformed JSON | Unknown discriminator/credential-reflecting decode tests exist. | Syntactically invalid JSON from a 200 response -> categorical Decode, configured fallback and next wait. Credit existing decode-safety assertions separately. |
| Wrong answer type | Pure type-mismatch tests exist. | Valid JSON with a known but wrong question type -> WrongType, authored fallback and next wait. |
| Missing answer/action | Pure missing-question assertion exists; malformed typed payloads exercise decode separately. | Distinguish missing requested answer from an answer lacking required choice data; both must fall back and progress, with the appropriate safe category. |
| Unknown action | Pure validation and actual client/sink reflection fixtures exist. | Preserve that evidence; connect an out-of-criteria answer to exact authored fallback/next wait rather than treating it as a usable answer. |
| Valid responses | Pure choice/noul/score and real-client choice/score evidence exists; typed runner routing exists separately. | Use representative valid wire shapes for choice, noul and score; assert expected landing and next wait, no fallback. Credit existing threshold/tie/legend tests rather than repeating each boundary at every layer. |

One representative HTTP 500 real-client-to-headless-app witness closes the app join:
exercise actual spawn/poll/application, assert the authored fallback say-line, no
pending/held result left behind, no defensive hold/Notice from ordinary fallback,
and no fallback-only autosave churn. Existing fake lifecycle/hold/cancel tests stay
as the evidence for those distinct timing rules; the app does not repeat the matrix.

## Wider contract/risk ledger

All evidence in this table is **source-inspected**, not executed this session.
"Credit" means meaningful assertions exist for the named plausible failure, not
that every failure in that area is covered. Selections and completion requirements
follow Q1–Q11 and the final Q12 shared-understanding confirmation.

| Contract | Plausible failure | Existing test/assertion | Gap and confirmed disposition |
| --- | --- | --- | --- |
| Loader rejects incomplete decision landing labels | A permitted answer cannot land at runtime | `engine/tests/pack_tests.rs:603`, `missing_decide_label_is_error`, requires the missing `reply.none` diagnostic; typed closure tests at `:409,:516`. | Credit; no mechanical parser/loader/runner duplication. |
| Runner follows authored control flow | A conditional stages the skipped arm, or a decision lands on a wrong say-line | `engine/src/runner.rs`, `bare_bool_key_is_eq_true_and_a_false_key_skips_the_arm`, checks both staged commands and say text; `fallback_action_lands_on_fallback_label` checks `cold` then Ace's authored say-line. Typed decision tests assert boolean/index routing. | Credit existing outcome-level tests; network matrix adds a different connection, not a rewritten runner suite. |
| Restore validates the actual destination before mutation | A stale/foreign/edited save corrupts the active runner | `engine/src/save.rs:832–922` checks exact refusal categories/reasons, all position fields unchanged, revalidation after read, different pack with same scene id, and unchanged disk bytes. | Credit. Existing structural acceptance deliberately excludes stronger state-value-kind/fingerprint validity; not a newly discovered defect or new testing requirement. |
| Unreadable slots survive refusal and remain overwritable | Failed load deletes data or makes recovery impossible | `engine/src/save.rs`, `semantic_failures_classify_unreadable_and_stay_overwritable`, and version/garbage-slot tests assert preserved bytes and successful overwrite. | Credit. No save-format/version/policy change. |
| Read-log unusable-file and version posture | Bad knowledge enables skip, or reading destroys an unusable file | `engine/src/read_log.rs:258–323` checks missing/no reason, undecodable and IO refusal, strict older/newer mismatch, preserved bytes/target and later dirty replacement; `:341–373` checks roundtrip/stable bytes. | Credit. Require one focused preservation test for schema-valid unknown IDs/line values at engine decode; app owns pack filtering. |
| Accepted CLI edit changes only its intended target | Regenerating one question destroys unrelated content/comments | `cli/tests/cli_test.rs:224–260` checks comment preservation, legacy choice form and successful `load_pack`; `cli/tests/splice_test.rs`, `apply_candidate_accepts_an_unusual_ordinary_id_and_writes_only_its_file`, checks other-file preservation. | Credit; replaced-block comments follow documented replacement semantics. |
| Rejected/dry-run CLI work is non-destructive | Failed generation installs content or truncates the target | `cli/tests/cli_test.rs:263`, dry-run before/after bytes; `:410–447`, exhausted replies fail and preserve bytes; `:454–501`, preparation failure has no success preview/write and exactly one request. | Credit, including real CLI process tests. |
| Content retry versus terminal provider/preparation failure | A terminal error spends more attempts or reaches a later sentinel success | `cli/tests/gen_test.rs:300–333` checks terminal provider classification and two calls after one rejection; existing retry/runner tests pin exhaustion, preparation and correction replacement. | Credit. Scene-drafting preparation classification is an already documented separate policy, not silently repaired here. |
| CLI provider configuration is honored | Wrong account selected or malformed config silently ignored | `cli/tests/provider_test.rs:35–108` checks file/flag/default/missing-key/OpenRouter-fallback paths; `cli/src/provider.rs:89–125` implements parse refusal and UNMEI-first key precedence. | Require distinguishable both-key precedence and malformed author-file refusal; neither is a reproduced product defect. |
| Input ownership blocks actions and queued edits for the claimed frame | Monitor typing advances fiction, saves, or replays after release | `app/src/tests.rs:15935–15994` asserts unchanged field/wait/no slot under claim, restored editing/advance without replay, and frozen menu focus. Further existing cases cover paste, drag, startup-error quit and held-input quarantine. | Credit; do not add a Cartesian UI permutation suite. |
| Async answer holding/cancellation/exactly-once persistence | Pausing applies an answer early, loading reuses an old answer, fallback churns checkpoint | `app/src/tests.rs:1237–1409` observes future drop, unchanged park while held, resumed authored say-line, exactly-once model autosave, no fallback autosave, and load discard/refire. | Credit lifecycle as fake-resolver evidence. One approved real-client app witness covers the missing joined boundary. |
| Settings are separate from saves and retry failed flushes | Loading rewrites preferences; failure discards an in-session change | `app/src/tests.rs:5711–5750` asserts unchanged settings bytes across no-op/menu/load; `:6160–6192` checks dirty/value retained, Notice and one failure event, then successful retry. | Credit. Do not conflate preference, save and read-log policies. |
| Credential values do not travel in normal shareable runtime diagnostics | Response reflects key into console/log/Monitor | `app/tests/credential_safety.rs:615–724` requires expected events and zero loss/eviction/truncation, rejects reflected material at all exercised sinks, and proves no redirect destination request. | Credit the actual supported sinks and locked configuration, not arbitrary emitters or painted-screen pixels. Parent direct read of credential unit-test file was blocked; no claim based on it alone. |
| Shipping composition builds strict executable schedules | App-owned cycle/conflict exists only in the broad plugin composition | `app/tests/shipping_schedule.rs:30–78` calls production `schedule_app`, retains Error ambiguity detection, runs three frames, asserts demo Runtime/no StartupFailure and documented plugin delta. | Credit schedule evidence only. Real native boot, rendered input/fonts/geometry and extracted release archives remain distinct existing owners, not passes inferred from this test. |

### Actual sharing surfaces

The bug-report template (`.github/ISSUE_TEMPLATE/bug_report.yml`) asks for logs and
screenshots. There is no implemented app/CLI bug-report export operation in the
bounded source audit. Shareable runtime diagnostics include the pack-sibling session
log, console transcripts and Monitor screenshots; do not invent an exporter.
The error screen is a startup pack-load surface, not a Decisions transport surface:
`app/src/failure.rs:1019–1052` tests its twenty-error cap and actual sibling-log pointer.
Key-row screenshots need their own masking/input evidence; real-sink tests do not
prove every painted screen. Arbitrary author-supplied pack error text is not certified
secret-free by the Decisions transport proof.

The historical shipping/startup/native observations remain in the private evidence
set and their revision-bound owning records. No native execution or rendered acceptance is refreshed
here. In particular, debug/release-with-`--dev` observations are not a normal release
player-mode boot observation; normal release/archive acceptance belongs to the
existing [release-certification contract](release-certification.md), not a new behavioral-protection suite.

### Documentation finding

The glossary's Autosave definition incorrectly included every decision resolution.
It is aligned with existing `runtime-hardening.md` Save slots §4, ADR 0011 and
`a_fallback_resolved_decision_writes_no_autosave` / `a_held_fallback_writes_no_autosave`:
a model answer refreshes Autosave; an authored fallback alone does not. A scene
crossing reached after fallback can still independently trigger Autosave. This is a
vocabulary correction, not a change in behavior or a new general testing term.

## Focused implementation handoffs (not implementation authorization)

All four handoffs are delivered; the delivery commits, observed receipts and
present-source checks are mapped in the private reconciliation record.
Each note below names the receipt that closed it.

1. **Decisions fallback/progress:** complete the offline real-client/runner matrix,
   including the single production-timeout witness, and one representative connected
   headless-app fallback witness. Preserve existing fake lifecycle and sink proofs.
   **Delivered (2026-10-19):** `engine/tests/decision_matrix.rs` drives the real
   client through
   the resolver seam into a real runner for every failure and valid row, including
   the observed 30-second production timeout; `app/tests/real_fallback.rs` is the
   one joined headless-app HTTP 500 witness. Fake lifecycle, exactly-once autosave
   and real-sink proofs remain separately scoped.
2. **CLI safety:** implement ADR 0014, including categorical retained error values,
   Debug/client/header/redirect posture and command-level masking of both recognized
   env values across stdout/stderr. At planning time the shipped code did not
   satisfy this new policy; both halves are delivered below.
   Keep generated-file/prompt behavior unchanged and document the sharing residual.
   **Provider half delivered (2026-10-18):**
   retained `ProviderError` values are fixed categories, the credential-bearing
   client/config `Debug` is opaque, the bearer header stays sensitive, wire verbosity
   is explicitly off and redirects are not followed.
   **Terminal half delivered (2026-10-18):**
   both non-empty recognized values are snapshotted before argument parsing and every
   exact literal occurrence is masked with `[redacted]` across stdout/stderr — Clap's
   argument/help rendering, the final error line, `--list`, previews, dry runs,
   exhaustion/best-output reports, interactive prompts and the `env_logger` sink, not
   only the runner's `Terminal`. A malformed `RUST_LOG` is parsed without printing
   and reported through the masked logger sink instead of `env_filter`'s own
   un-maskable stderr warning. A value split across formatting/write chunks is masked
   whole: all writers on one stream share that stream's buffer (the `env_logger` sink
   shares stderr) and both buffers are finalized before exit; a split across the two
   streams is outside the rule. Exact-literal limits and the unchanged
   generated-file/prompt boundary are documented in ADR 0014.
3. **Focused existing-contract regressions:** both-key precedence, malformed author
   file refused before provider/write, and schema-valid read-log collections retained
   by the engine decoder. Reuse existing boundaries/fixtures; no parallel duplicate
   end-to-end suite. The first two landed with the configuration-selection work.
   **Read-log half delivered (2026-10-19):**
   `engine/src/read_log.rs::load_read_log` decodes without consulting a pack, and
   `schema_valid_unknown_scene_ids_and_source_lines_survive_decode` pins that at the
   decoder boundary — a hand-authored RON file with ids and source lines from another
   pack, including the line `0` the app refuses, loads with no unusable-file reason
   and its exact `read`/`visited` collections. The app's single launch-time
   `filter_against_pack` is still the only place pack-invalid entries are dropped
   (`launch_filtering_drops_unusable_entries_without_writing` still owns that
   behaviour); nothing new treats unknown data as read or visited in play, and the
   strict version policy, byte preservation, dirty replacement and player-knowledge
   behavior are unchanged. Commands and observed output for that test are in the
   issue's closing comment.
4. **Filesystem failure protection:** missing partial-write/replacement witnesses at
   the actual owning writers plus final-rename legacy-import failure. Keep the
   existing distinct temp policies and successful retry/preservation behavior.
   **Delivered (2026-10-19):** deterministic, test-only one-shot fault rigs in the
   shared sidecar writer and the CLI content writer observe partial-temp write and
   rename
   failure against an existing target, each owner keeping its own documented temp
   policy and whole-file retry; `engine/tests/save_legacy.rs` obstructs the retired
   destination after a successful slot write. No rollback, crash-durability or
   writer-unification claim is introduced.

### Safety acceptance details

Use dummy credentials only. Require both presence of intended safe output/events
and absence of known credential values: clean empty output alone is not proof.
For CLI, cover reflected status/transport/decode/structured-degradation detail and
representative success/preview/dry-run/exhaustion/interactive validation output on
both terminal streams. Check safe Debug values and valid no-follow redirects against
a distinct live loopback destination, not a malformed Location that cannot be followed.
Preserve terminal exit-status, retry and non-destructive file behavior while updating
the intentionally changed raw-provider-body expectation.

The broad terminal rule is not implemented solely inside the runner: `cli/src/main.rs`
snapshots both values before parsing, renders `Cli::try_parse` errors and the final
error itself, points `env_logger` at a masked sink and hands the runner masked
streams. Argument/setup/final-error rendering and supported enabled
dependency-log formatting must not bypass known-value masking, including before
provider resolution. Tests must exercise actual command wiring as well as focused
masking behavior. Preserve legitimate output and interactive prompt/flush usability;
format/write chunk boundaries must not leak a known literal that is one emitted
value. This does not promise control over arbitrary external emitters, OS terminal
echo of input, deliberately bypassing code or unknown secrets. No raw payloads may
be retained merely because the final stdout/stderr writer masks them.

### Validation required from the later build

Run the smallest owning checks and report exact commands, revision/features, observed
outcomes, fixture failures and unexecuted residuals. Applicable command candidates:

- `cargo test --locked -p unmei-engine --features network` for the client/runner matrix;
- `cargo test --locked -p unmei` for CLI boundaries;
- `cargo test --locked -p unmei-app` for the connected app witness;
- `cargo test --locked -p unmei-app --test shipping_schedule` for shared schedule evidence;
- `cargo fmt --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`,
  `cargo test --locked --workspace`, and the applicable invariant checks before merge.

These were future checks when the ledger was written, **not commands run
successfully by this decision**. The later build ran them and recorded the observed
outputs in its delivery receipts; the private reconciliation record
maps each receipt to its owning paths and records that they are unchanged at the
reconciled revision. Keep ignored live-provider tests distinct. Headless/schedule
success is not a boot, render or artifact pass.

## Resolution and residuals

Q12 confirmed the assembled ledger and authorized closing it as planning only.
Its portable resolution is published and the decision is closed.
No build, new issue decomposition, commit, push or production/test/CI change is
authorized here. The documents remain local and uncommitted.
No selected behavioral protection is deferred. Remaining accepted boundaries are:
source-inspection rather than fresh execution; the parent's blocked credential-file
read; exact-literal terminal masking rather than general secret detection; unchanged,
not-certified-secret-free generated content/prompts; deterministic I/O error handling
rather than crash/power-loss durability; and existing native/render/artifact residuals
under their own owners.

Coverage/reporting and the updated human smoke checklist remain the sibling
[coverage](testing-coverage-evidence.md) and [human smoke](testing-human-smoke.md)
decisions; they are not resolved here.
