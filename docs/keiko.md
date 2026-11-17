---
title: Keiko — requirements for end-to-end testing from the player's side
summary: Requirements for Keiko, the framework an author's or maintainer's agent uses to play a pack headlessly, check what the player perceives and explore every path.
keywords:
  - verification
  - content-pack
  - player-input
  - decisions
  - agent-tooling
  - ci
---
# Keiko — requirements

**Status (2026-11-17).** Requirements reviewed 2026-11-15 in one requirements review (four rounds, 26 questions) and accepted 2026-11-15. **Specified 2026-11-17 in the [build-ready spec](keiko-spec.md), not built.** The spec amended each contract in [Contracts this will amend](#9-contracts-this-will-amend) with a dated pointer ([spec §16](keiko-spec.md#16-amendments)).

**What this is.** Unmei's tests today stop short of the player. The headless app tests assert ECS-level state transitions. The Monitor examples drive a real window with a real GPU. The [human smoke checklist](testing-human-smoke.md) is the only end-to-end pass over menus, saves, skip, the Timeline and failures, and a person runs it by hand. An author has nothing beyond playing their own pack. **Keiko** (稽古, rehearsal) plays a pack the way a player does and checks what that player would perceive. A coding agent can drive it from a shell without a person present. Authors test their own packs with it. Maintainers test Unmei with it through the demo pack and fixture packs.

**Prior art.** Ren'Py's [automated testing](https://www.renpy.org/doc/html/testcases.html) puts `testcase` blocks in the game: steps such as `advance`, `click "Save"` and `assert`, text selectors, and pixel-compared screenshots. [Ink-Tester](https://github.com/wildwinter/Ink-Tester) plays an ink story thousands of times with random choices and reports which lines were reached. [inkcheck](https://glama.ai/mcp/servers/chaoz23/inkcheck) explores an ink story's reachable states within limits and reports an exact path to each problem. [bevy_brp_mcp](https://docs.rs/bevy_brp_mcp) lets an agent drive a running Bevy app over the Bevy Remote Protocol. Keiko takes Ren'Py's step vocabulary and inkcheck's exhaustive, reproducible exploration. It drops pixel comparison and live remote control (§10).

---

## 1. Shape

- **K1 — One framework, the pack as the unit.** Keiko serves pack authors and Unmei's maintainers with one tool and one case format. The unit under test is always a content pack. A maintainer's engine check is a case over the demo pack or a fixture pack.
- **K2 — The headless shipping player.** Keiko drives the shipping player plugin with no window and no GPU, so it runs in CI and in a container. It never drives the engine runner alone, because menus, saves through the overlay, skip and input ownership exist only in the player.
- **K3 — Hosted by `unmei-play`.** The player owns Bevy and the renderer, and already hosts `--check` and `--preview` ([plugin family C1](plugin-family.md#5-pack-wide-check-and-preview)). Keiko joins them there: one code path, and no Bevy in the author CLI.
- **K4 — A virtual clock.** Time in a take moves only when a step moves it. Reveal, auto's 2.0 s dwell, the skip cadence and a Notice's lifetime are testable with no real waiting, and a take gives the same result on every machine.
- **K5 — Three uses, one step vocabulary.** A **take** executes stored cases in batch (§2). **Stepping** lets an agent apply one step at a time (§5). **Exploration** walks the pack's paths by itself (§6). All three use the same steps and the same observations.

## 2. Cases and steps

- **K6 — Cases live in the pack.** A case is `tests/<id>.keiko` in the pack folder, one case per file. The player ignores `tests/` during play.
- **K7 — A line-oriented format.** A case is a sequence of steps, one step per line, like a scene in the [dialogue DSL](content-format.md#3-dialogue-dsl-unmei). RON stays the format for structured data ([ADR 0001](adr/0001-ron-for-structured-content.md)), but a step list reads and diffs better as lines.
- **K8 — Its own format version.** A case carries a format version, matched exactly. An additive change keeps the version, as for the plugin kinds ([UI plugins R25–R27](ui-plugins.md#8-versioning)). A case at an unknown version fails with a clear error and is never guessed at.
- **K9 — Intent steps and raw input.** A step may state player intent: advance, pick an option, open the menu, save to slot 2, open the Timeline. It may also give raw input: press, hold or release a key, or click. Both reach the player only as real input messages, never as direct state changes, so every input rule holds, including input ownership and held-input quarantine ([Monitor](runtime-hardening.md#monitor)).
- **K10 — Targets by role.** A step names a control by its role: a choice option by its index or its source text, and a menu control by its copy key. It never names a control by its displayed text alone, so a UI plugin's copy overrides and a language pack leave cases working.

## 3. What a step observes

- **K11 — What the player perceives.** After each step Keiko reports an **observation**: the current surface (title, play, menu view, Gallery, still, Timeline, confirm, error screen); the text on screen (speaker, say-line, choice options, Notice, menu labels and whether each is enabled); and the staging (background, sprites, music). A step may assert on any of it.
- **K12 — The author's own data.** A step may also assert on what the pack declares and the player stores: the state map, the current scene and label, the read log, the save slots, and the session log by target and text. A silent degradation, such as a missing sprite, is visible only in the log, so an author needs that assertion too.
- **K13 — Never Bevy internals.** No step sees entities, components, system order or any other ECS detail. A refactor that keeps the player's behaviour keeps every case passing.
- **K14 — Frames for looking, never for comparing.** A step may ask for the current frame as a PNG, rendered through the preview's path on the software rasterizer ([plugin family C7](plugin-family.md#5-pack-wide-check-and-preview)). An agent looks at it. Keiko never compares frames pixel by pixel.

## 4. Decisions, sandbox and faults

- **K15 — Decisions set per case.** A case may set the landing key of any decision point. Keiko answers through the [decision resolver seam](adr/0009-decision-resolver-seam.md). A decision the case leaves unset takes the author's fallback. A take needs no key and no network.
- **K16 — Live decisions on request.** A separate mode sends the decisions a case leaves unset to the Decisions model. It needs a key, and Keiko marks its results as nondeterministic. Ordinary takes never call the model.
- **K17 — Every take in a sandbox.** Each take runs on a throwaway copy of the pack, with its own sidecars and a sandboxed credential location. Keiko never reads or writes the author's real saves, settings, read log, session log or key.
- **K18 — Declared starting fixtures.** A case may declare the sidecars it starts with, such as a given save slot, read log, settings file or credential state. Without a declaration a take starts with none.
- **K19 — Named faults from a fixed list.** A case may declare faults from a fixed list, such as an obstructed sidecar write, a missing or undecodable asset, an unreadable credential file, a held or slow decision answer, a key check that cannot reach NanoGPT *(amended 2026-11-16: it was OpenRouter; [ADR 0015](adr/0015-nanogpt-is-the-one-provider.md))*, or a broken pack. Each is applied through the sandbox or through a seam the player already has (the resolver, the key check). The shipping player gains no fault hook.
- **K20 — Dev mode.** A case may start the player in [dev mode](runtime-hardening.md#dev-mode), to reach the Monitor and the F5/F9 shortcuts.

## 5. Stepping

- **K21 — Stateless replay.** Coding agents mostly run one shell command at a time, so stepping keeps no process alive between steps. Each stepping command replays the case file from the start, applies one new step, appends it to the file and prints the observation. Because a take is deterministic (K4, K15), the replay lands exactly where the last command left off.
- **K22 — A stepped file is a case.** What an agent stepped through is already a stored case, and a later take replays it unchanged.

## 6. Exploration

- **K23 — Every branch, within a budget.** Exploration branches on every choice option and on every landing key of every decision point, not only the fallback. A combination of scene, line and state map already seen counts as visited. Exploration stops at a declared budget.
- **K24 — Findings.** A **finding** is a panic; a [defensive failure](../GLOSSARY.md) or a Notice; a warning or error log line the pack causes, such as a missing asset; or a park no player input can leave. Taking a fallback is never a finding.
- **K25 — Coverage is not a finding.** Exploration reports which lines and scenes it reached. An unreached line or scene is coverage information, because a budget can end exploration early.
- **K26 — Findings become cases.** Exploration saves each finding as a ready-to-run case holding the exact steps that reproduce it. An agent replays it, fixes the pack and keeps the case as a regression test.

## 7. Output and onboarding

- **K27 — Text or JSON.** Every Keiko command prints readable text by default. With `--format json` it prints observations and findings as JSON. Each finding uses the shape of the check's findings: file, path inside it, rule id and message ([plugin family C3](plugin-family.md#5-pack-wide-check-and-preview)).
- **K28 — Discoverable from the pack.** A pack's `AGENT-GUIDE.md` gains a Keiko section, and the Guide gains a Keiko page. An agent needs no extra install to find and use Keiko.

## 8. Relation to other work

- **K29 — One replay, shared with the preview.** Keiko owns the replay that reaches a line offline. The preview's `--at`, `--scene`, `--pick` and `--state` ([plugin family C4 and C5](plugin-family.md#5-pack-wide-check-and-preview)) become one consumer of it, and their formats move into Keiko's spec. There is no second replay to drift.
- **K30 — Specified first.** Keiko's spec and build come before the plugin family's and the UI plugin's specs. The plugin builds then reuse its replay for the preview and have end-to-end cases from their start.
- **K31 — Not release evidence.** Keiko's results are not [release-certification](release-certification.md) evidence. Whether Keiko results can close the first-time acceptance item that the stranger run holds open is a separate maintainer decision, taken once Keiko exists.

  *(Amended 2026-11-17: overridden. Once Keiko is built, its takes on the candidate's extracted archives are release evidence and replace every human walk, the stranger run included. Real-GPU rendering, heard audio and physical input become accepted unverified residuals ([Keiko replaces the human walks](release-certification.md#keiko-replaces-the-human-walks-decided-2026-11-17)).)*

### Done when (for the build)

- Every [smoke checklist](testing-human-smoke.md) case that needs no real GPU, audio device or physical input is a Keiko case over a fixture pack. The human checklist keeps only the rendered, audio and physical-input checks. *(Amended 2026-11-17: the human checklist is retired instead, and those checks become accepted unverified residuals (K31).)*
- The `check` workflow runs the demo pack's and the fixture packs' cases, plus a bounded exploration of the demo pack, on every push.
- The demo pack's `AGENT-GUIDE.md` and the Guide describe Keiko (K28).

## 9. Contracts this will amend

These are not amended yet. The spec amends each in place, with a dated pointer.

- [Plugin family C4 and C5](plugin-family.md#5-pack-wide-check-and-preview): the preview's replay is Keiko's, and its `--pick` and `--state` formats move to Keiko's spec (K29).
- [Pack layout](content-format.md#2-pack-layout): the optional `tests/` folder (K6).
- [Crate boundaries](architecture.md#crate-boundaries-and-invariants): `unmei-play` gains the Keiko commands (K3).
- [Testing policy](testing.md): Keiko as the player-level end-to-end layer, and its `check` gate (Done when).
- [Human smoke checklist](testing-human-smoke.md): it shrinks to the residuals Keiko cannot reach (Done when).
- `.github/workflows/check.yml`: the new Keiko gate.
- `examples/demo-pack/AGENT-GUIDE.md` and the Guide: the Keiko section and page (K28).
- `GLOSSARY.md`: it gains the terms below.

**Terms for the glossary.** These avoid **Run** (a block of say-lines), *playthrough* (avoided under **Live session**), *session* (**Live session**, session log) and *view* (**Settings view**, **Still view**).

| Term | Meaning | Avoid |
|---|---|---|
| **Keiko** | Unmei's end-to-end testing framework: it plays a pack the way a player does and checks what the player perceives. | test harness, test runner |
| **Case** | One `tests/<id>.keiko` file: a sequence of steps, with any declared fixtures, faults and decision landings. | test case file, script, scenario |
| **Step** | One line of a case: a player intent, a raw input, a clock move or an assertion. | command (a DSL term), action (a decision term) |
| **Take** | One execution of one case, start to finish, in its own sandbox. | run, test run, playthrough |
| **Observation** | What the player perceives after a step, plus the author's data a step may assert on. | snapshot, view, state dump |
| **Stepping** | Applying one step at a time to a case, each command replaying it from the start. | session, interactive mode, REPL |
| **Exploration** | Keiko walking every choice option and landing key of a pack within a budget. | fuzzing, random walk, crawl |
| **Finding** | A problem exploration reports, saved as a case that reproduces it. | failure, bug, error (each means something narrower) |

## 10. Left to the spec

*(Answered 2026-11-17: each item below is settled in the [build-ready spec](keiko-spec.md), where its decision is recorded.)*

- The step syntax, the full fixed fault list and the JSON shape of an observation.
- The command and flag names under `unmei-play`.
- How the sandbox keeps sidecars apart without copying a pack's large assets.
- How a take waits for assets to finish loading under the virtual clock.
- How live mode treats a decision whose landing key the case set.
- The exploration budget's units and defaults.
- How a case chooses a language once language packs exist.
- Whether `--check` validates `.keiko` files, and whether packaging leaves `tests/` out of a shipped pack.

## 11. Out of scope

Pixel-compared screenshots; performance measurement; proof that audio is heard, rendering on a real GPU, or physical input (the human walk keeps these; *amended 2026-11-17: no walk keeps them, they are accepted unverified residuals, K31*); live remote control of a running player over the Bevy Remote Protocol or MCP; an agent skill shipped for one vendor; Keiko results as release evidence (K31; *amended 2026-11-17: now in scope*).
