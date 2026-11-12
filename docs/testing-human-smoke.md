---
title: Human player smoke checklist and evidence boundary
summary: The item-seven human smoke checklist — safe preflight, fixture recipes, per-case expectations, and the blank future record with its custody rules.
keywords:
  - verification
  - evidence-record
  - player-input
---
# Human player smoke checklist and evidence boundary

**Status: human-confirmed planning contract, Q1–Q11. Not executed.** Part of item seven's testing decisions. This is a checklist and blank record, **not an execution receipt**. No case below has been run by this planning work.

## Scope and inherited acceptance (Q1–Q3)

Item seven owes this updated checklist, a blank evidence template and a focused future handoff. It adds **no fresh human-walk execution obligation** to item-seven completion. It does not deliver product/test changes, fixture infrastructure, packaging, release workflows, a golden-image suite or a spawned-binary replacement for the human walk.

The historical full hand walk keeps its later maintainer confirmation. Do not reopen it, treat its earlier boot-only report as the final status, or reuse its pass for new bytes. The historical Monitor acceptance is also historical feature evidence, not an open acceptance dependency or a fresh result.

[Release certification](release-certification.md) remains authoritative for later items nine/eleven: exact extracted Windows/Linux archives require fresh boot and functional acceptance; Windows also requires real-GPU/rendered/input acceptance. Linux's full rendered/input walk remains accepted **unverified**, never passed. This feature checklist neither relaxes those requirements nor requires every example-only injected condition to be reproduced in a distributed executable. Release records stay with release assets under that contract.

| Evidence kind | What it can establish; what it cannot |
| --- | --- |
| Deterministic headless app/engine tests | Asserted behavior at their actual scope; no window, rendered geometry or physical input proof. |
| Strict shipping schedules | Schedule construction/frames; no renderer startup or binary launch proof. |
| Bounded real boot | Actual process/window initialization to its observed bound; survival is not functional completion or input acceptance. |
| Human feature walk on an identified executable | Its actually observed cases, platform and fixture; not another executable or archive. |
| Controlled rendered examples | Their injected states, drawn surfaces and asserted outcomes; not the shipping executable's composition or exact archive bytes. |
| Exact-archive acceptance | Only the recorded tested bytes and required outcomes under the release contract. Same-source rebuilds are different bytes. |

Preserve existing [testing policy and shipped-path checks](testing.md), which also states how to choose checks for future work. Existing source/tests and historical passes guide the walk; citing them is not executing them again. Runtime vocabulary and behavior remain governed by [the glossary](../GLOSSARY.md), [title/menu](title-and-menu.md), [runtime hardening](runtime-hardening.md), and ADRs [0006](adr/0006-save-slots-on-disk-format-and-version-policy.md), [0008](adr/0008-player-first-failure-posture.md), [0010](adr/0010-credentials-live-in-a-user-level-file.md) and [0011](adr/0011-read-log-as-a-third-persistence-class.md).

## Safe preflight (Q2, Q8)

1. Use a unique disposable root outside the tracked tree, e.g. absolute `<scratch>/human-smoke-<run-id>/`, kept out of version control. Keep its pack directories, their **sibling** saves/read log/settings/session log, and redirected config inside it. Do not modify the tracked demo, real user config, real save slots, global environment or broad filesystem permissions. Copy content, not existing demo sidecars. Record a named cleanup inventory.
2. Launch the identified `unmei-play` with an **explicit absolute pack path**. Current source defaults to a working-directory-relative demo; beside-executable discovery belongs to the future packaging slice. Record the executable SHA-256, full source SHA, dirty patch/input identity, build/toolchain/profile/options and effective dev mode. Source identity alone does not establish the executable's provenance.
3. Construct the **child-only** environment before launch: remove inherited `OPENROUTER_API_KEY` and unneeded endpoint overrides; redirect Windows `APPDATA` to an absolute sandbox root, or Linux absolute `XDG_CONFIG_HOME` **and** `HOME` to sandbox roots. Only the deliberate environment-precedence case adds a synthetic key. Do not dump the environment. There is no player `--no-dev` flag: use normal release for dev-off, release `--dev` for dev-on; debug defaults on.
4. Resolve/check the intended credential path before interaction: Windows `<sandbox-APPDATA>/unmei/credentials`; Linux `<sandbox-XDG_CONFIG_HOME>/unmei/credentials` (sandbox HOME also protects fallback). Start with a clean sandbox and confirm the Settings path line stays inside it. Abort if routing is uncertain; do not touch an existing real credential to test isolation.
5. Independently establish outbound network blocking for credential commits, e.g. an operator-controlled isolated offline execution environment. Record the mechanism and verification, not secrets. A fake key does not prevent a request: production commits ping the fixed `/api/v1/key` endpoint, independent of `UNMEI_DECISIONS_URL`. If blocking cannot be verified, do not commit: mark those cells BLOCKED. Never change machine-wide networking speculatively.
6. Use only synthetic keys. Do not capture credential-entry views, raw credential files, raw environment dumps or secret-bearing logs. Even synthetic key text and rendered key fingerprints stay out of retained records: use fixture labels such as `stored-A` and `env-B`. Observe fingerprint-bearing row states live and record only their categorical state and local preservation result in prose; do not retain a screenshot of the fingerprint. This evidence-minimization rule does not change the UI's permitted fingerprint rendering. Inspect logs/images/metadata before retention. Local equality checks may establish credential preservation without retaining a credential copy, digest or value.
7. Default to Auto off, instant text and no held keys for identity/preservation checks. Change those deliberately in the comfort/skip cases. Compare protected slot bytes at stable say-parks with no ordinary crossing, choice resolution or model answer between snapshots. Read knowledge may legitimately grow and dirty-flush; do not demand byte equality of the read log after normal entry/reading.
8. Prepare and validate each disposable fixture before the human observation. Preparation failure is not a player-behavior pass/fail; record BLOCKED and repair setup. The recipes below are **unexecuted preparation instructions**, not new maintained pack files or a generator delivered here.

## Minimal fixture recipes (Q8)

Keep different variants under distinct pack names so their sidecars cannot collide. Record an inventory/hash of their actual content and initial **non-credential** sidecars, plus all edits from the copied source. Do not overwrite source files or reinterpret old save coordinates after editing a scene.

### D — clean copied demo

Copy only `examples/demo-pack/` content. Begin without sibling sidecars. Its opening two say-lines, choice, `reunion` decision and epilogue supply the original story/comfort/save cases. The no-key fallback for `ace.greet` is `cold` (`Ace: Oh. It's you.`). The manifest has **no declared scene order**, so D can show only `This pack has no Timeline.`; all its declared gallery art is present.

### L — reading/Timeline lane

Make another content-only demo copy so existing NPC definitions, initial state and assets remain valid. In its manifest change `entry` to `smoke_entry` and add `scenes: ["smoke_entry", "smoke_target", "smoke_unvisited"]`. Retain other manifest fields. Existing unlisted demo scenes may remain; Timeline must not derive entries from them.

Add these three scene files. Stable labels let a person distinguish a line, a restored checkpoint and a fresh scene entry. Do not edit their source-line layout after read marks are created.

`scenes/smoke_entry.unmei`:

```text
bg school_gate
play music_morning
Narrator: S1 opening
Narrator: S2 second
Narrator: S3 third
Narrator: S4 fourth
Narrator: S5 fifth
Narrator: S6 sixth
choice
  "Carry changed state" -> changed
  "Carry changed state again" -> changed
end
label changed
set affinity_level = "high"
set met_ace = true
sprite ace happy
Narrator: CHANGED {affinity_level}
jump smoke_target
```

`scenes/smoke_target.unmei` — deliberately no leading state/staging changes:

```text
Narrator: TARGET first {affinity_level}
Narrator: TARGET second {affinity_level}
```

`scenes/smoke_unvisited.unmei`:

```text
Narrator: UNVISITED must remain hidden until actually entered
```

For state-carry discrimination, first visit TARGET with `high`/happy, then New Game resets to `low`/fresh staging while retaining the visited set. Timeline back to TARGET must now show `low` and the **current** staging, not its historical high/happy frame. Repeat after reaching CHANGED if needed to observe the opposite carry. Compare saves around jumps at stable targets, not around the authored crossing which legitimately autosaves.

For reading, use separate clean L copies (or explicitly reset only their named disposable sidecars) for the seen-skip, force-skip and load-knowledge sequences. Never seed read marks by silently changing authored line numbers or assume a force-skipped scene was fully read.

### G — failed gallery art

Make a separate content-only D copy. Remove only its copied `assets/sprites/ace/neutral.png`; replace only its copied `assets/sprites/ace/happy.png` with known undecodable bytes. Keep the healthy background. Both declared sprite entries must remain in the gallery. Missing and undecodable art are distinct fixture inputs; record which is which without treating a loading frame as proof of permanent failure. A blank placeholder alone is insufficient: observe completion/failure diagnostics too.

### B — broken pack

Make another D copy and add `scenes/smoke_broken.unmei` with a say-line followed by `jump smoke_missing_target`, where that target exists nowhere. `engine/src/pack.rs::validate` rejects a jump target that is neither a scene-local label nor a loaded scene, so this is a load-time refusal before the title, not an intentional runtime defensive hold. Do not break the tracked demo. A single actionable load error suffices; the twenty-error cap/overflow fixtures belong to existing controlled example coverage.

### S/K — disposable sidecar and credential faults

- **Unwritable save:** start with a valid manual Slot 1 and record its hash. Create an empty directory at `<pack>.slot1.save.ron.tmp`. `engine/src/sidecar.rs::write_atomic` uses `path.with_extension("ron.tmp")`, so the real UI overwrite fails before replacement while the old slot remains a regular file. Remove that named empty directory after observing failure and retry to establish recovery. This is the required unwritable-save witness, **not** an OS permission-denial claim. Optional actual permission denial must target the disposable **parent**, prove denial under the launch identity and restore original permissions; a Windows folder read-only attribute is insufficient.
- **Unusable autosave:** in a separate fresh D copy with no live session, write known non-RON marker bytes to `<pack>.autosave.save.ron` before launch and record their hash. Expect disabled Continue with its unreadable reason, not a fallback to a manual slot. Compare the autosave hash before any play/autosave trigger; do not overwrite a real checkpoint to prepare this case.
- **Legacy import:** in a dedicated D copy, produce a valid manual save at a stable line, then prepare `<pack>.save.ron` from that same pack/position. For the v1 witness, change its format version to `1` and omit `saved_at`, preserving all other semantic coordinates. Keep original bytes locally for the rename comparison. The initial `.save.ron.imported` path must be absent. Never use a foreign/stale position to stand in for a valid legacy save.
- **Credential unreadable:** in a fresh sandbox make `unmei/credentials` an empty directory, not a file. Reading it must degrade safely; remove only that named empty directory before later credential cases.
- **Credential write failure:** in a fresh sandbox with an existing synthetic credential, make `unmei/credentials.tmp` an empty directory. The credential writer removes stale regular files then uses `create_new`; the directory obstructs it without replacing the stored credential. Expect no ping and no partial credential file. The obstruction is intentional setup and is removed explicitly afterward. Do not reuse the save writer's different suffix here.

## Case checklist: actions and observable expectations (Q4–Q7)

Rows may share a run but every subcheck gets its own result. Use keyboard and pointer where their distinct ownership/hit-testing risk matters, not every equivalent activation permutation. A case that requires two launches records both. Representative screenshots are supporting observations, not substitutes for file equality, sound, timing or input outcomes.

| ID / setup | Human actions | Expected observable outcomes and limits |
| --- | --- | --- |
| T1 — clean D, no key/slots | Launch; wait on title; try disabled Continue and Esc; open/back out of Settings; New Game. | Title is silent, no play frame/advancement; Continue says `No autosave yet` and is inert; Esc stays on title. Title Settings backs to title, has no Resume/Exit-to-title rail controls; New Game starts without a confirm. Boot/title alone creates no read knowledge or slot. |
| T2 — live D, stable say-park | Esc → Settings → Exit to title; cancel once, then confirm; wait; Continue. | One confirm, cancel leaves session intact. Title is silent; Continue names the live scene without a saved time and resumes the same line/reveal/dwell, not a slot. No slot write from the exit/Continue actions. Any asynchronous held-answer detail requires A1 below; do not infer it from this simple park. |
| T3 — distinct autosave/manual parks | Resolve a player choice/cross a scene to create autosave; later save a different park manually. Quit normally; relaunch; Continue. Also use a separate copy with unusable autosave bytes. | Relaunch has no live session; Continue describes and loads autosave, not manual Slot 1. Unusable autosave disables Continue with the engine's reason, bytes preserved. Survival/boot alone cannot prove the restored line. |
| T4 — live session with saves, settings and synthetic credential | Exit to title; New Game → Cancel; repeat → Start; compare protected bytes before any new autosave trigger. | Cancel changes nothing. Start confirms once, returns to fresh entry/state/staging and clears prior skip activation. Manual slots/autosave/preferences/credential survive unchanged; read knowledge survives (R1/R3), though normal entry may add knowledge. New Game itself writes no autosave. Quit/relaunch before a trigger may still Continue the old autosave. |
| P1 — D, no key, Auto initially off | Play through the choice and decision; Esc at a say-park; wait; resume. | Choice is human-selected; offline `ace.greet` lands on authored cold reply and play continues with a categorical fallback log, no fallback Notice. Esc freezes input/progression/reveal/auto dwell; resume restores the park. No-client decision wait may be too brief to observe: A1 owns held/slow evidence. |
| S1 — stable D line | Save Slot 1; advance to a distinct line and save Slot 2; inspect scene/time; load Slot 1 through the overlay. Quit/restart and load a manual slot again. | Slots show usable scene/time metadata; load restores the exact saved line/state/staging rather than merely opening a window. Manual save/load works in normal release through player controls, without F5/F9. Load itself does not autosave. |
| S2 — valid v1 legacy setup | Open overlay; import into a manual slot; reopen overlay; load the imported slot. | Legacy card is offered once, never targets autosave, and retires after successful import. Original bytes survive at `.save.ron.imported`; time stays unknown for v1 without a stamp. Imported slot restores its exact line. No destructive deletion. |
| S3 — exact temp-path obstruction | At a new stable line attempt overwrite of existing Slot 1; observe Notice; hash old slot; remove obstruction; retry save and load it. | Save failure reports loss of persistence and leaves old slot bytes unchanged; play/menu remain usable. Recovery writes and restores the new line. Do not count a read-only folder attribute or successful write as failure evidence. |
| C1 — both Settings doors | Move Master/Music/SFX and Mute; leave Settings; quit/relaunch same pack; inspect and listen with known audio present. Set Fast text, Auto on; play; toggle F11 in play. | Mix values survive relaunch and audible changes are observed where audio hardware is available. Reveal is visible, `AUTO` cue appears; Auto holds at choice/end. F11 switches fullscreen and returns. Record audio unavailable or unobserved geometry honestly, never as a heard/real-GPU pass. Decision-hold observation needs A1. Restore Auto off before identity cases. |
| K1 — isolated no-key config | Open Settings from title and overlay; inspect last Decisions group/path; enter synthetic shape-invalid text; commit; edit/discard via Esc; Copy path; try New Game/Continue as applicable. | No key does not gate play. Shape failure is Invalid, writes nothing and starts no ping. Esc discards editing before backing out of Settings; Copy path copies only sandbox path and transiently reads `Copied`. No credential file is created merely by opening Settings. Keep all entry views out of screenshots. |
| K2 — synthetic stored-A; separate env-B launch | Seed shape-valid stored-A, launch without env override; reopen Settings after a controlled hand-edit. Relaunch with nonempty synthetic env-B, then without it (include empty/whitespace override if checking unset normalization). | Stored value appears Set without standing ping; view-open picks up hand-edit. Env-B renders From environment/read-only, Clear disabled, stored bytes untouched. Removing override restores stored source. Observe precedence via row state and local equality, not key values in logs/screenshots. No live validity claim. |
| K3 — verified offline blocking | Commit a shape-valid synthetic replacement; wait for verdict; cancel Clear once then confirm Clear. | Write precedes check; if visible, Checking disables editing; Unreachable becomes Set/not checked, not Invalid. The row reports the stored source this session; resolver rebuilding is separately attributed boundary-test evidence unless actually witnessed. After Clear, Not set and no Decisions client, so subsequent decision points take their authored offline fallback. A transient Checking frame not seen is unobserved, not inferred. Accepted/Rejected are not established by blocking. |
| K4 — isolated read/write faults | Open unreadable credential setup; then separately attempt a commit with the exact `credentials.tmp` obstruction; remove obstruction and retry under verified blocking. | Read: Not set plus safe log, no Notice, no auto-delete. Write: `Could not save the key` Notice, prior key/row preserved, no partial file and no ping. Recovery can persist under K3. No credential value enters any retained record. |
| G1 — D through both doors | Open Gallery from title, then later from overlay; enter a healthy background and sprite still, browse, close grid/still stepwise. | Correct backdrop/back label for each door; contain-fit image, sprite transparency and readable caption; still → grid → correct origin, then resume only from overlay. No story advance, slot/autosave/settings write or audio start caused by gallery navigation. |
| G2 — G failed art | Open missing and undecodable sprite cards/stills; revisit them after assets settle; inspect safe diagnostics. | Faint solid placeholder frame plus entry label, not missing card/glyph/Notice. Still remains usable. One failure report per path under appropriate asset row, no repeat spam on revisit. Healthy art remains healthy. Do not label a merely loading image permanently failed. |
| R1 — clean L, seen lane | Advance to S2 and remain parked there; do not advance to S3. Return-title/New Game; press Tab at S1. | Seen skip passes prior-read S1/S2, stops on first arrival-unread S3 and ends its cue. S3 may now be normally marked read at its displayed park: stopping does not require it to remain unread forever. Any ordinary player input ends seen skip. |
| R2 — separate clean L, force lane | From S1 hold Ctrl through S2–S6 to choice; do not choose automatically. Release; return-title/New Game; Tab from S1. Repeat at end-of-scene as needed. | Force skip passes unread say-lines but holds at choice/end and never selects/resolves them. On revisit seen skip stops at S2: force-passed lines were not added as read. Scenes actually entered still count visited. Do not infer decision-wait holding from this lane; A1 covers it separately. |
| R3 — separate clean L, saved early park | Save at S1; read S2/S3; reload S1 while seen/force skip had been active; release held controls; Tab anew. | Load restores S1 and clears old activation/arrival status. New activation knows S2/S3 read **after** the checkpoint and stops at S4. Read knowledge is not rolled back by load; New Game retains it too. A held Ctrl needs release and fresh press after replacement. |
| L1 — L initially only entry visited | Open Timeline; inspect TARGET/unvisited placeholders; try locked activation; back out. Then reach TARGET normally, reopen and select visited entry; cancel once, then confirm. | Declared order only; unvisited ids hidden as `???` and inert without Notice. Browsing/cancel preserves play/staging and slots. Confirm restarts visited scene from its first wait, closes overlay, not a save rewind. |
| L2 — L visited TARGET, then fresh low-state New Game | Hold Ctrl while using Timeline to jump to TARGET; compare landing state/staging with current session and its earlier high/happy visit; compare slots around selection before a qualifying event. | TARGET first says low with current staging, not historical high/happy; it is not extra-advanced. Seen skip is off; held Ctrl cannot skip landing until release/repress. Selection writes no autosave/manual slot; normal later authored crossings/choices/model answers keep their own triggers. |
| M1 — normal release, then release `--dev` | In normal mode try F3/F5/F9 at stable play; use ordinary menu save/load. In dev mode use F5/F9 on a disposable manual Slot 1 and F3. | Normal mode has no Monitor or shortcut save/load effect; player controls still work. Dev shortcuts retain manual-slot-1 behavior, and F3 opens Monitor. A debug build with no flag cannot establish dev-off. |
| M2 — dev on representative surfaces | Toggle F3 over title, play, Settings draft, Gallery/still, Timeline/confirm and error screen. Exercise close/Esc, fresh vs held Enter/Space/click, paste and a Settings slider drag. | Monitor opens over each reachable surface; Esc closes only it, preserving underlying menu/confirm. No navigation/commit/paste/click-through/continuing drag underneath; valid draft resumes after release. Held activation must release/repress. On **unpaused play**, Auto/reveal/decision work may continue: Monitor is input ownership, not a pause. Previously armed Ctrl still tracks physical release while Monitor owns input. |
| M3 — dev reading | Exercise wheel/body hit-test, scrollbar, PgUp/PgDn/Home/End, Show everything and Follow latest; hide/reopen while non-following. Observe arrivals/filter/rewrap only when actually available. | Rows/labels remain readable; anchored top event/within-row position does not jump to newest on arrivals/reopen/filter changes, and Follow latest deliberately resumes. Controlled busy history/expiry or locating states require A1 support; absent arrivals are not an anchor-stability pass. |
| B1 — B broken pack | Launch, read errors/path/log pointer; quit by Esc or click and record process exit. | Error screen, never title; actionable load errors and session-log pointer; nonzero startup-error exit. Process survival without reading/quitting this screen does not suffice. Dev F3 reachability on this surface belongs to M2. |
| A1 — separately identified supporting examples/tests | Select the existing controlled Monitor/decision/key-check witnesses needed for otherwise fleeting states. Record exact invocation, binary/profile/fixture, observed assertions, human review and limits. | Controlled slow/held answers can establish Auto/skip decision holds and answer handling under pause/title only when the selected witness actually exercises them. Controlled log arrivals/eviction can establish anchored reading/expiry; deterministic checker tests can establish Accepted/Rejected boundaries. Uncovered shipping-path cells stay explicit, never inherited PASS. |

### Existing controlled witnesses, not a mandatory full replay (Q6–Q7)

The existing command families are:

```sh
cargo run -p unmei-app --example monitor_smoke
cargo run -p unmei-app --example monitor_acceptance
cargo run -p unmei-app --example monitor_acceptance -- --mode hold
cargo run -p unmei-app --example monitor_acceptance -- --mode broken
cargo run -p unmei-app --example monitor_acceptance -- --mode gate
```

These are **future choices, not commands run for this planning decision**. `broken` deliberately exits 1. The default acceptance example has a gated resolver and controlled log fixtures; its other modes do not imply all title/Auto/skip/checker transitions are covered. The examples write generated state/screenshots under `target/monitor-acceptance/`; preserve approved evidence before another run's cleanup/overwrite. Follow their source and the testing policy, including separate ordinary/developer release identity. Do not create a new harness simply to force every manual cell to pass. A1's exact witness selection and any uncovered transition must be explicit in the future execution record.

## Result, freshness and custody rules (Q3, Q9–Q10)

- Per case **and subcheck**, use PASS only for observed expectations; FAIL for contradicted behavior; BLOCKED for missing prerequisites; NOT RUN for omitted execution; INCONCLUSIVE for insufficient observation. Expected injected save/pack errors are PASS only when the expected refusal, preservation and recovery actually occur. A filename, screenshot or process exit alone is not that full observation.
- No numerical pass threshold. No full smoke-acceptance claim while selected required cells remain failed/unobserved. Describe any bounded partial walk as partial, with its explicit cells and supporting witnesses. This does not add execution to item-seven completion or waive later required archive acceptance.
- A changed executable/fixture is a new run identity. Preserve old attempts/failures. After repair, rerun affected cases and directly coupled transitions; reuse other feature evidence only with explicit input applicability. New archives/rebuilds still obey release certification's fresh-byte rules; this feature reuse convention cannot certify them.
- Confirmed release-blocking defects require repair before eventual applicable completion. Lower-impact findings need reason, named follow-up and the maintainer's disposition. Missing observation is neither an automatic product defect nor a pass. The required protections in [behavioral protection](testing-behavioral-protection.md) are not silently deferred by this checklist.
- Default future **non-release** evidence home: a retained record outside the shipped tree, one folder per run id, with README/observation rows and SHA-256 inventory for safe attachments. Capture scratch first, then privacy-review text, pixels, metadata and sensitive paths; exclude credential files/digests, raw environment and credential-entry captures. If redaction is necessary, identify the derivative and what was removed without retaining secret-bearing originals in the published bundle. Redaction must not silently change a screenshot's evidentiary claim.
- Preserve safe original evidence byte-identically and verify links/checksums before deleting its scratch source. Unsafe evidence needs an explicit privacy-safe preservation/disposal decision, not publication by default. Delete disposable setup files by named path and empty directory, never a recursive sweep. Retain pre-existing/unknown-owner evidence. A local uncommitted folder is **local/unpublished**, not durable remote custody; publication needs separate authorization.
- The human signs the observed result, not an agent-generated success label. Final release authorization and exact-archive retention remain entirely under `release-certification.md`.

## Blank future record

Copy this into a future run's README. All fields start **unobserved**; do not fill them from this planning decision or historical acceptance.

```markdown
# Human smoke run <run-id>
Status: NOT RUN / partial / completed bounded scope (state which)
Purpose: feature walk / exact-archive appendix (link separate contract)
Tester and explicit sign-off:
Start/end time and timezone:
Source full SHA and dirty/patch identity:
Executable path, SHA-256, provenance/build receipt:
Toolchain, target, profile/options, effective dev mode for each launch:
OS/architecture, GPU/driver/compositor, display size/scale, audio availability:
Exact launch commands and working directory (no key values):
Sandbox config roots, routing check, network isolation verification:
Fixture variants, content checksums, initial non-credential sidecar inventory:
Known exclusions and historical references (not current passes):

| Case/subcheck | Preconditions/input identity | Actions | Expected | Actually observed | Result | Evidence/limits |
| --- | --- | --- | --- | --- | --- | --- |
| <id> | <not prepared> | <not performed> | <from checklist> | unobserved | NOT RUN | <none> |

Supporting test/example records (not shipping-path PASS):
Explicitly unobserved cells, including slow/held decisions/checker verdicts:
Platform/rendered/input limitations; Linux full-rendered residual if applicable:
Findings, impact, disposition owner/rationale/follow-up:
Previous attempts, changes and affected reruns:
Any reused feature observations and input-applicability justification:
Screenshot/log privacy and metadata review:
Attachment inventory and SHA-256 file:
Maintained evidence destination, accessibility/custody verification:
Named scratch cleanup completed/retained with reason:
Human confirmation of the actual bounded observations (not release permission):
```

## Focused future handoff

1. Prepare the documented disposable variants and safe child-local launch environment, validate fixture bytes and establish network/config isolation. No new generator, workflow or maintained pack is required by this decision.
2. When a feature walk is requested, select its bounded scope through the testing policy; execute human actions on the identified executable and record each observed/unobserved subcheck. Use existing controlled witnesses only for their real scope; no secret/live-provider requirement for routine acceptance.
3. Retain privacy-reviewed attributable observations at the chosen maintained home, resolve findings contextually and rerun affected transitions after repairs. Do not delete evidence before verified custody or label local-only material published.
4. Later packaging/certification work uses the relevant steps as an appendix while independently meeting exact archive identity, Windows real-GPU/input, both-platform functional acceptance and the accepted Linux rendered residual. No new certification mechanism or waiver is created here.

Q11 confirms shared understanding and authorizes this local document, the narrow release-contract cross-reference and the portable closure of this decision only. No implementation, fixture execution, Cargo/provider run, new build issue, commit or push is authorized. Local documents remain uncommitted/unpublished until separately authorized; the issue resolution preserves the portable contract independently of them.

No domain concept changed: generic evidence terms stay out of `GLOSSARY.md`, and this reversible checklist procedure does not warrant a new ADR. This decision resolves the human planning boundary only, not item-seven delivery or any executed feature/release acceptance.
