---
title: Pack-wide check and preview — build-ready spec
summary: The build-ready spec for unmei-play --check and --preview, covering findings, all-at-once loading, rule ids, asset and flow rules and the preview.
keywords:
  - pack-validation
  - diagnostics
  - content-pack
  - presentation-surface
  - verification
  - agent-tooling
---
# Pack-wide check and preview — build-ready spec

**Status (2026-11-20).** Both slices are **built**, on the integration branch `spec/91-check-preview`. This page turns the accepted [plugin family requirements](plugin-family.md#5-pack-wide-check-and-preview) C1–C7 into a build-ready spec, as the [Keiko spec](keiko-spec.md) §11 and §12 amended them. The decisions below were settled with the maintainer on 2026-11-19 and are recorded where they apply. Each section names the requirement it implements. When this page and the requirements disagree, the requirements win and this page is wrong.

**What this spec covers.** Today's pack content only. No plugin kind exists yet, so the check runs no plugin rules and the preview renders today's surfaces. Each later kind's spec adds its rules and surfaces through the shapes defined here (C2's third bullet). The plugin-folder loader, G1 in the [readiness audit](plugin-readiness.md#2-shared-groundwork), is deferred to the first plugin-kind spec, which has a real folder to load (*decided 2026-11-19*). This spec builds G2, the diagnostics model.

**Slices.** The build ships in two slices. Each slice is complete and mergeable on its own (*decided 2026-11-19*).

| Slice | Contents | Done when |
|---|---|---|
| 1 | Commands (§1), findings (§2), all-at-once loading (§3), rule ids (§4), assets (§5), flow lint (§6), cases (§7), output (§8), the check's gate and docs (§11). | §13's slice-1 rules pass. |
| 2 | The preview (§9) and its gate and docs (§11). | §13's slice-2 rules pass. |

---

## 1. Commands (C1)

The launch grammar in [`app/src/launch/mod.rs`](../app/src/launch/mod.rs) gains these forms:

```text
unmei-play [PACK_DIR] --check [--format text|json]
unmei-play [PACK_DIR] --preview DIR [--scene ID | --at SCENE:LINE] [--state FILE] [--pick N,N,...] [--format text|json]
```

- **One mode per invocation.** `--check` and `--preview` join Keiko's group of modes ([Keiko spec §1](keiko-spec.md#1-commands-k3-k27)). Two modes together are a usage error.
- **Modifiers.** `--format` is valid with both. `--scene`, `--at`, `--state` and `--pick` are valid only with `--preview`, and `--scene` and `--at` exclude each other. `--dev` is a usage error alongside either mode.
- **`PACK_DIR`.** It resolves exactly as for play, including the default `demo-pack` beside the executable ([`app/src/launch/pack.rs`](../app/src/launch/pack.rs)).
- **`DIR`.** The preview's output folder. It is created when missing. A `DIR` inside the pack folder is a usage error, so a preview never writes into the pack it reads.
- **No window, no sidecars.** Neither mode opens a window, and neither reads or writes the pack's sidecars (slots, read log, settings, log) or the user's credential file. The preview's takes run in Keiko's sandbox ([Keiko spec §4.2](keiko-spec.md#42-the-sandbox-k17)).

The author CLI keeps its write-time validation and gains no check command (C1).

## 2. Findings (G2)

*Decided 2026-11-19: Keiko's finding shape becomes the one diagnostic shape of the engine, with a severity, and replaces `LoadError` and `Warning`.*

- **One type.** `Finding` moves from `unmei_engine::keiko` to a new engine module, `unmei_engine::finding`. `unmei_engine::keiko` re-exports it, so Keiko's code and its JSON keep their shape. It gains one field:

  | Field | Content |
  |---|---|
  | `file` | The pack-relative file, such as `scenes/reunion.unmei`, `npcs/ace.ron` or `tests/opening.keiko`. |
  | `path` | Where inside the file: `line <n>` for a line, a dotted RON path such as `decisions.greet.fallback` when the rule knows one, or an empty string for the file as a whole. |
  | `rule` | One id from §4, or a Keiko id. |
  | `severity` | `error` or `warning`. |
  | `message` | The human-readable detail. |

- **`load_pack` returns findings.** Its signature becomes `load_pack(root) -> Result<LoadedPack, Vec<Finding>>`, and `LoadedPack.warnings` becomes `Vec<Finding>`. Every finding in the `Err` list has severity `error`, and every finding in `warnings` has severity `warning`. `LoadError` and `Warning` are removed. This changes the public API of the published `unmei-engine` crate, which a 0.x minor release (0.3.0) allows.
- **The parser speaks findings too.** `Parser::parse` returns `Result<Scene, Finding>`, with `path` set to `line <n>`.
- **The text people already read is unchanged.** A finding displays as `file:line: message` when its path is `line <n>`, as `file: path: message` for another path, and as `file: message` for an empty path. That keeps every message the error screen, the session log and the author CLI's corrections show today byte-identical. The CLI's NPC staging filter matches loader content diagnostics as before ([author-cli §9](author-cli.md#9-staged-pack-validation--decided-design-2026-10-14)).
- **Keiko's findings are errors.** A Keiko finding built by `Finding::new` or `Finding::at_line` has severity `error`. No Keiko JSON output gains the `severity` field. The check's output adds it (§8).

## 3. Loading reports everything (C2, first bullet)

*Decided 2026-11-19: loading keeps going past a broken manifest or state file, and each scene file still reports its first syntax error.*

- **No early return.** `load_pack` no longer returns when `pack.ron` or `state.ron` cannot be read or decoded ([`engine/src/pack/mod.rs`](../engine/src/pack/mod.rs)). It records the finding and goes on to read and parse every scene and NPC file.
- **Rules that need a missing input are skipped, not guessed.** Without a decoded `state.ron`, the rules that read the state vocabulary do not run: declared keys, kinds, `{var}` references, conditions and `state_keys`. Without a decoded `pack.ron`, the manifest rules do not run: the entry scene, the `scenes` list, the tagline and the backdrop id. Every other rule runs.
- **One syntax error per scene file.** The parser still stops at a file's first syntax error, because a broken block (`if`, `choice`) makes later errors in the same file unreliable. A scene that fails to parse is absent from the semantic pass, so a jump to it reports nothing more.
- **Deterministic order.** Findings are sorted by file, then by line (a finding with no line first), then by rule id, then by message. *(Amended 2026-11-20: path and severity are tie-breakers after those four keys, so a pack whose findings agree on all four still has one order.)*
- **Boot uses the same function.** The player's boot calls the same `load_pack`, so its error screen now lists every load error the check reports, instead of only the manifest or state error when one of those fails. It does not read a Keiko case, so a case finding appears only in the check. This is the one visible change to normal play in slice 1. *(Amended 2026-11-20.)*

## 4. Rule ids (C3)

Every finding the loader, the check and the preview produce carries one id from this table. The build adds the ids as constants in `unmei_engine::finding::rules`, beside Keiko's. Each of today's error and warning sites gets the id of its family. Adding an id is additive.

| Id | Severity | Covers |
|---|---|---|
| `pack.file.read` | error | A pack file or folder that cannot be read (`pack.ron`, `state.ron`, `scenes/`, `npcs/`, a scene or NPC file). |
| `pack.file.ron` | error | Invalid RON in `pack.ron`, `state.ron` or an NPC file, including a non-finite number in `state.ron`. |
| `pack.manifest` | error | The entry scene, the `scenes` list, the tagline's characters and the backdrop id. |
| `pack.manifest.unlisted` | warning | A loaded scene missing from a non-empty `scenes` list. |
| `pack.manifest.tagline-length` | warning | A tagline longer than the title column fits. |
| `pack.scene.syntax` | error | Every parser error: commands, arguments, blocks, labels, literals and `{var}` syntax. |
| `pack.scene.target` | error | A `jump` or choice target that is neither a label in the scene nor a scene file. |
| `pack.scene.cycle` | error | A jump cycle with no say-line, `choice`, `decide` or `if` on it. |
| `pack.state` | error | An undeclared state key, a kind mismatch, an ordering operator on a non-number and a bare non-bool condition. |
| `pack.npc` | error | An NPC id that does not match its file stem. |
| `pack.decision` | error | An unknown NPC or question, a missing landing label, a fallback outside the landing space and an undeclared `state_keys` entry. |
| `pack.decision.no-none` | warning | A `choice` decision with no `none` action. |
| `pack.asset.missing` | error | A referenced asset file that does not exist (§5). |
| `pack.asset.decode` | error | A referenced asset file the player cannot decode (§5). |
| `flow.unreachable-scene` | warning | §6. |
| `flow.unreachable-label` | warning | §6. |
| `flow.no-exit` | warning | §6. |
| `preview.state.unreachable` | warning | A preview fixture state this pack cannot reach (§9.2). |
| `ui.file.read` | error | A plugin file that cannot be read, including a `ui/` folder with no `plugin.ron` ([UI plugin spec §2](ui-plugin-spec.md#2-the-plugin-folder-r11-r27-g1)). |
| `ui.parse` | error | Invalid RON, or a file that is not UTF-8, in a plugin file ([UI plugin spec §8](ui-plugin-spec.md#8-findings-and-the-plugin-outcome-r14-r15-r21)). |
| `ui.version` | error | A plugin `format_version` this build does not read, which is any value but 1 ([UI plugin spec §2](ui-plugin-spec.md#2-the-plugin-folder-r11-r27-g1)). |
| `ui.path.escape` | error | A path a plugin names that is not relative and confined to `ui/`: it is empty, absolute, names `..`, or is a symlink out of the folder ([UI plugin spec §7](ui-plugin-spec.md#7-confinement-and-caps-r13)). |
| `ui.cap` | error | A plugin cap exceeded: the files in `ui/`, one file's size, the folder's total or a shipped image's dimensions ([UI plugin spec §7](ui-plugin-spec.md#7-confinement-and-caps-r13)). |

The `ui.*` ids are the UI plugin's, and each plugin ticket adds its family's ids here as it builds them ([UI plugin spec §8](ui-plugin-spec.md#8-findings-and-the-plugin-outcome-r14-r15-r21)).

A `.keiko` case reports Keiko's own ids ([Keiko spec §10.2](keiko-spec.md#102-rule-ids)) as errors (§7).

## 5. Assets (C2, second bullet)

*Decided 2026-11-19: split like [UI R21](ui-plugins.md#7-developing-and-checking-a-plugin): the engine lists and checks existence, the player decodes. Normal boot keeps today's silent degradation.*

- **The engine lists every reference.** A new read-only accessor on a loaded pack yields every asset the pack's files reference, with the file and line of each reference: each `bg <id>` and the manifest's `title_backdrop` (`assets/backgrounds/<id>.png`), each `sprite <npc> <pose>` (`assets/sprites/<npc>/<pose>.png`) and each `play <id>` (`assets/audio/<id>.ogg`). It covers every line of every scene, including branches and arms no playthrough takes ([content format §3](content-format.md#3-dialogue-dsl-unmei)). `stop <id>` names a sound `play` already names, so it adds no reference.
- **Missing files are an engine rule.** The check reports one `pack.asset.missing` finding per distinct missing path, at its first reference in finding order (§3), and the message counts every reference. It needs no new engine dependency. *(Amended 2026-11-20: the message reports the total, as rule 10 states.)*
- **Decoding is a player rule.** For each path that exists, the check loads the file through the player's own asset loaders, the same image and audio loaders play uses ([`app/src/presentation/assets.rs`](../app/src/presentation/assets.rs)), in a headless app with no window and no audio device. A load that fails is one `pack.asset.decode` finding at the path's first reference. What passes the check therefore decodes in play.
- **Unreferenced files are ignored.** A file in `assets/` that no pack file names is not checked, matching the gallery's rule that declared references are the only ones ([content format §2](content-format.md#2-pack-layout)). The optional `sfx_select` sound is not referenced by any pack file, so it is not checked.
- **Boot does not change.** Play still loads assets lazily, and a missing or broken file stays the silent degradation of the [failure inventory](runtime-hardening.md#runtime-failure-posture) rows 3–5. Only `--check` runs §5.

## 6. Flow lint (C2, fourth bullet)

The check builds a control-flow graph over the loaded pack and reports three warnings. The graph is **state-blind**: both arms of every `if` are reachable, every option of every `choice` is reachable, and every landing key of every `decide` is reachable, whatever the state and the decision would be.

- **Nodes and edges.** A node is a line position. Each line flows to the next line, except a `jump`, which flows only to its target, and a `choice` or `decide`, which flows to each of its targets. A target resolves exactly as at runtime: a local label first, then a scene file's first line ([content format §5](content-format.md#5-transitions)).
- **`flow.unreachable-scene`.** A scene no path from the manifest's entry scene reaches. One finding per scene, on the scene file with an empty path. The Timeline does not count as a path, because it only restarts scenes the player has already reached ([Timeline](title-and-menu.md#timeline)).
- **`flow.unreachable-label`.** A label in a reachable scene that no path reaches. One finding per label, at its line. A label inside an unreachable scene is not reported again.
- **`flow.no-exit`.** A reachable scene from whose first line no path reaches the end of the file or a jump to another scene, so play can never leave it. One finding per scene, with an empty path.
- **Already an error.** C2 also lists "a `decide` with no label for one of its landing keys". The loader already rejects that as a `pack.decision` error ([content format §4](content-format.md#4-decision-configs-and-decision-points)), and existing behavior is kept, so flow lint adds no rule for it.

Keiko's exploration lists unreached labels as coverage ([Keiko spec §9.3](keiko-spec.md#93-coverage-k25)). Its view is dynamic and limited by a budget. Flow lint is static and complete. Each reports independently.

## 7. Keiko cases

As the [Keiko spec §11](keiko-spec.md#11-check-and-packaging) already settled, the check runs the engine's Keiko validation over every case under `tests/`, `tests/findings/` included, in byte order of their relative paths. Each case finding is an error. The check runs no take.

When the pack fails to load, the check runs only the case parser: the version line, the syntax and the header order. The reference checks need the loaded pack's facts, so they run only when the pack loads. *(Amended 2026-11-19.)*

## 8. Output and exit codes (C3)

- **Text** is the default: one line per finding, `<severity>: <display> [<rule>]`, using §2's display form, then a summary line, `check: <e> errors, <w> warnings`.
- **JSON.** `--format json` prints one document to stdout: `{ "findings": [{ "file", "path", "rule", "severity", "message" }], "errors": <e>, "warnings": <w> }`, findings in §3's order.
- **When the pack fails to load.** If `load_pack` fails, the check reports its findings and the case-parser findings of §7. It runs no asset rule (§5) and no flow lint (§6), because each needs the loaded pack, which a failed load does not return. *(Amended 2026-11-19: this bullet first said the existence rule and every case rule still run. Neither can: `load_pack` returns no partial pack, and Keiko's reference checks need the loaded one.)* *(Amended 2026-11-20: `load_pack`'s warnings are not reported either. A failed load's warnings describe files a broken pack happens to agree on, and the error screen has never shown them, so the check keeps them out and reports only the errors.)*
- **Exit codes.**

  | Code | Meaning |
  |---|---|
  | 0 | No error finding. Warnings may be present. |
  | 1 | At least one error finding. For the preview, also a render failure (§9.4). |
  | 2 | Usage error. |

## 9. The preview (C4, C5, C7) — slice 2

The preview is a generated Keiko case run through Keiko's take engine, the one replay the [Keiko spec §12](keiko-spec.md#12-the-shared-replay-k29) defines. [`app/src/keiko/preview.rs`](../app/src/keiko/preview.rs) already builds that case from `--scene`, `--at`, `--state` and `--pick`. Each frame is drawn by the render path of [Keiko spec §13](keiko-spec.md#13-slice-2-frames-and-position-clicks-k14-k9): the player's own composition at 1280×720 on wgpu's fallback adapter, with no window and no GPU (C7).

### 9.1 What it renders

- **`--at SCENE:LINE`** writes one frame, `DIR/at-<scene>-<line>.png`, at the park that line produces. The line must be a say-line, a `choice` or a `decide`. Any other line is a `keiko.case.reference` finding, and so is a line the default picks cannot reach.
- **`--scene ID`** writes a frame at every park the replay meets from the scene's first line until it leaves the scene, `DIR/scene-<id>-<line>.png`, taking the default picks and fallbacks (C5).
- **`--preview DIR` alone** writes the fixture states of §9.2.
- **`--state FILE`** and **`--pick N,N,...`** override the initial state and the picks, in the formats the Keiko spec §12 defines. They apply to every mode above. *(Amended 2026-11-20: a `--state` file that cannot be read or decoded is a `keiko.case.reference` finding naming the file, because it is the request's own argument rather than part of the pack.)*

A rerun overwrites the files it names and leaves every other file in `DIR` alone.

*(Amended 2026-11-20.)* A scene id, whether `--scene`'s or the one in `--at SCENE:LINE`, is written as one bare token in the generated case, so an id containing whitespace is a usage error: exit 2, no frame. A `--scene` walk stops after 10,000 parks and reports `keiko.defensive`, because a scene that never ends is an authoring defect and a preview cannot render that many frames.

### 9.2 Fixture states (C4)

*Decided 2026-11-19: the preview renders today's surfaces now. The UI plugin spec replaces this table with variant E's surfaces.*

Each state is reached by Keiko steps over the pack under preview, from a fresh take, and written as `DIR/<state>.png`:

| State | Reached by |
|---|---|
| `title-fresh` | Boot, with no slots. |
| `title-continue` | Boot with a save in slot 1, so `Continue` is enabled. *(Amended 2026-11-20: `Continue` is enabled by the autosave or a live session, never by a manual slot, so a fresh take reaches this state by playing to a park, pressing `Esc`, then `Tab`×5 and `Exit to title`, and confirming `Yes`.)* |
| `say` | `new-game`, then advancing to the first say-line with a speaker, fully revealed. |
| `narration` | The first say-line spoken by `Narrator`. |
| `choice` | The first `choice` park. |
| `decision` | The first `decide` park, held pending with `decision-hold`. |
| `scene-end` | The first scene end the default picks reach. |
| `menu-<section>` | `menu` at the first say-line, then each overlay section: `save`, `load`, `settings`, `gallery`, `timeline`. |
| `still` | The gallery's first card opened in the still view. |
| `confirm-<kind>` | Each confirmation dialog: `overwrite`, `load`, `reset-audio`, `reset-play`, `new-game`, `quit`, `exit-to-title`, `clear-key`, `timeline-jump`. The Timeline's one confirm is raised by activating a card of a scene this take has read, so it needs a pack whose manifest declares a scene order. *(Amended 2026-11-20: the first list named eight kinds and `ConfirmKind` has nine.)* |

A state the pack cannot reach, such as `narration` in a pack with no narrator, or a confirm whose precondition a take cannot set up, writes no file. It is reported as a `preview.state.unreachable` warning naming the state. The preview exits 0 when only such warnings occur.

### 9.3 The pack must load

The preview first runs `load_pack`. If the pack fails to load, the preview prints those findings in §8's format, writes no frame and exits 1. It does not run the rest of the check.

### 9.4 No renderer

A machine with no fallback adapter fails the preview with `keiko.render.unavailable` and exit 1, before any frame. The check (§1–§8) needs no renderer.

### 9.5 Motion (C6)

No pack can produce motion today: Unmei has no stage plugin and no transitions. C6's keyframe strips are therefore deferred to the stage plugin spec, which adds the motion they show.

## 10. One code path (UI R24)

- The check and the boot call the same `load_pack`, so a pack that passes the check's load rules loads in play.
- The check's extra rules (§5, §6, §7) run after the load and never change what the boot accepts.
- The preview draws through the player's own composition. Its frames may differ slightly from GPU frames. The native Monitor walk stays the rendered gate (C7).

## 11. The gate and the docs

**Slice 1**

- **Fixture packs.** `app/tests/check/<name>/` holds small packs that each break one family of rules: a broken `pack.ron` beside a broken scene (§3), a missing and an undecodable asset, an unreachable scene, an unreachable label and a scene with no exit (§6), and an invalid case (§7).
- **The gate.** `app/tests/check_gate.rs` runs the built `unmei-play` (`CARGO_BIN_EXE_unmei-play`) with `--check --format json` over the demo pack, which must exit 0 with no finding, and over each fixture pack, which must exit with the expected code and findings. The `check` workflow's existing test step runs it.
- **Docs.** The demo pack's `AGENT-GUIDE.md` gains a check section. The Guide's [Write your first pack](guide/write-your-first-pack.md) gains a step that runs `--check`. [`GLOSSARY.md`](../GLOSSARY.md) widens **Finding** to any problem the loader, the check, the preview or Keiko reports, with a severity, and adds **Check** and **Flow lint**. [`testing.md`](testing.md) names the check gate.

**Slice 2**

- **The gate.** `app/tests/preview_gate.rs` runs `--preview` over the demo pack on the software rasterizer, as Keiko's slice-2 tests do, and asserts that every §9.2 state the demo pack can reach is written as a 1280×720 PNG.
- **Docs.** The demo pack's `AGENT-GUIDE.md` gains a preview section. The Guide's [Test your pack](guide/test-your-pack.md) gains the preview.

## 12. Amendments

Each of these is amended in place on 2026-11-19, with a dated pointer to the section here:

- [Plugin family C2](plugin-family.md#5-pack-wide-check-and-preview): a `decide` with no label for a landing key stays a load error (§6), and C6 is deferred to the stage plugin spec (§9.5).
- [Plugin readiness §4](plugin-readiness.md#4-suggested-spec-order): step 1 builds G2. G1 moves to the first plugin-kind spec.
- [Pack layout](content-format.md#2-pack-layout): assets are still not checked at load, but `--check` checks them (§5).
- [Crate boundaries](architecture.md#crate-boundaries-and-invariants): the engine's `finding` module and asset reference list, and `unmei-play`'s `--check` and `--preview` modes.

## 13. Validation rules (the build's test list)

**Slice 1**

1. The grammar accepts each §1 form, and rejects with exit 2: `--check` with a Keiko mode, `--check` with `--preview`, `--dev` with either, `--scene` or `--at` without `--preview`, `--scene` with `--at`, and a preview `DIR` inside the pack.
2. `--check` creates no window and never writes or reads the pack's sidecars or the user's credential file. A test points `APPDATA`/`XDG_CONFIG_HOME` at a canary folder and asserts it is untouched.
3. A pack with an invalid `pack.ron` and a scene with a syntax error reports both, and skips the manifest rules without inventing findings for them.
4. A pack with an invalid `state.ron` still reports a `jump` to a missing target in a scene.
5. Two syntax errors in one scene file report only the first.
6. Every error and warning site in the loader and the parser carries an id from §4. A test loads one fixture per id and asserts the id.
7. The display form of every loader error the error screen showed before the change is byte-identical after it, across the existing loader and parser test messages.
8. The author CLI's staged validation and its corrections text are unchanged: the existing CLI tests pass without edits to their expected messages.
9. The asset reference list holds every `bg`, `sprite`, `play` and `title_backdrop` reference, including those inside `if` arms and after a `choice`.
10. A missing sprite referenced three times reports one `pack.asset.missing` at its first reference, and the message says three.
11. A truncated PNG and a text file named `.ogg` each report `pack.asset.decode`. A valid asset reports nothing.
12. A file in `assets/` that nothing references reports nothing.
13. A scene reachable only through a decision landing is reachable. A scene nothing jumps to reports `flow.unreachable-scene`.
14. A label behind a `jump` that skips it reports `flow.unreachable-label`. A label reached by falling through does not.
15. A scene whose every path loops back through a choice reports `flow.no-exit`. A scene with one option that leaves does not.
16. An invalid case under `tests/` reports its Keiko finding as an error, and the check exits 1.
17. Findings come out in §3's order, and the same pack gives byte-identical JSON twice.
18. A pack with only warnings exits 0. A pack with an error exits 1.
19. The demo pack passes `--check` with no finding.
20. The boot's error screen lists every load error `--check` reports for a pack that fails to load. *(Amended 2026-11-20: `--check` also reports the case-parser findings of §7 there, and the boot reads no case.)*

**Slice 2**

21. `--at reunion:<line of a say-line>` writes one 1280×720 PNG at `DIR/at-reunion-<line>.png`. A line that is not a park reports `keiko.case.reference` and writes nothing.
22. `--scene first_meeting` writes one PNG per park the default picks meet in that scene.
23. `--pick 2` reaches the second option's branch. In `epilogue`, `--state` setting `affinity_level` to `"high"` reaches the say-line in the `if` arm that the initial state skips.
24. `--preview DIR` alone over the demo pack writes every §9.2 state the demo pack can reach, and reports each other state as `preview.state.unreachable`.
25. A pack that fails to load writes no frame and exits 1 with its findings.
26. With no fallback adapter, the preview fails with `keiko.render.unavailable` and exit 1.
27. A rerun overwrites the same files and leaves an unrelated file in `DIR` untouched.

**Slice 1, added 2026-11-19** *(numbered after slice 2 so that earlier rule numbers stay stable)*

28. A pack that fails to load, with a missing asset, an unreachable scene, one case with a bad version line and one case that names an unknown scene, reports the load findings and the `keiko.case.version` finding only: no `keiko.case.reference`, asset or flow finding.

## 14. Out of scope

- G1, the plugin-folder loader, and every plugin kind's rules and surfaces. Each kind's spec adds them.
- Motion keyframe strips (C6), deferred to the stage plugin spec.
- Recovery inside the scene parser after a syntax error.
- Decoding assets at boot, or any change to play's asset degradation.
- A check command in the author CLI.
- Comparing preview frames with stored frames.
- A preview flag for a language or for Reduce motion. The language-pack and stage specs add them ([plugin family §9](plugin-family.md#9-left-to-the-spec)).
