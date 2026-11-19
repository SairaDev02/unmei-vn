---
title: Plugin family — requirements beyond the UI plugin
summary: Requirements for four more pack plugin kinds (language packs, stage, authoring profile, UI sounds and speaker style) and the shared check and preview.
keywords:
  - content-pack
  - presentation-surface
  - pack-validation
  - authoring-cli
  - dialogue-dsl
  - audio
  - failure-posture
---
# Plugin family — requirements

**Status (2026-11-15).** Requirements accepted 2026-11-15, after one requirements review. **Not specified, not built.** Each kind's build-ready spec is the next phase. [Amendments to the UI plugin requirements](#7-amendments-to-the-ui-plugin-requirements) names each UI plugin requirement this page overrides; those are amended. No other contract changes yet: [Contracts this will amend](#8-contracts-this-will-amend) names each one a spec will change.

**What this is.** [UI plugins](ui-plugins.md) give a pack its own look as data. This page extends the same model to more of the visual novel development experience: translations, the stage behind the dialogue box, interface sound and per-speaker style, a check and preview for the whole pack, and the author CLI's drafting rules. Each is a **plugin kind**: a folder of data in the pack with its own format version.

**Rejected as poor fits.** These were considered and dropped, because each conflicts with a settled decision:

- DSL macros or custom commands from a pack: [ADR 0002](adr/0002-line-oriented-dsl-without-conditionals.md) and [ADR 0004](adr/0004-conditionals-in-the-dialogue-dsl.md) exclude macros, and code plugins are out of scope.
- Decision-provider plugins: [ADR 0009](adr/0009-decision-resolver-seam.md) keeps one concrete client and one narrow seam.
- Importers from other VN formats: they are separate tools and need no plugin interface.

---

## 1. Shared rules

- **F1 — One shape for every kind.** Every plugin kind follows the UI plugin's model unless this page states an exception:
  - author-time and static, loaded once (UI R1);
  - declarative data only, in RON, with no code of any kind (UI R2, R3);
  - chosen by the pack, by the presence of its folder; `pack.ron` gains a field only where this page says so;
  - the same trust, confinement, resource caps and no-network rule as pack data (UI R13);
  - one code path for the check, the preview and the boot (UI R24);
  - its own format version, matched exactly, with additive changes keeping the version (UI R25–R27).
- **F2 — One folder and one version per kind.**

  | Kind | Folder | Read by |
  |---|---|---|
  | UI plugin, with the additions in §4 | `ui/` | player |
  | Stage plugin | `stage/` | player |
  | Language pack | `lang/<tag>/`, one folder per language | player and author CLI |
  | Authoring profile | `author/` | author CLI and `--check` |

- **F3 — Kinds fail alone.** A kind that fails drops to its own default without affecting the others. A broken stage plugin uses the reference stage while the pack's UI plugin still loads, and the reverse.
- **F4 — Stated exceptions.** Three requirements depart from the UI plugin's rules on purpose: partial languages (L6), the player's language choice (L4) and the authoring profile's failure path (A3). Each states why.

## 2. Language packs

- **L1 — Scope: the interface and the content.** A language pack translates the player's interface strings and the pack's content: say-lines, choice text, speaker names, the pack name and the tagline.
- **L2 — The player ships interface translations.** Unmei ships its interface strings in each language it supports, built into the player. A pack's language may override any of them, through the same copy keys and placeholder rules as UI R9. A language may also translate a UI plugin's copy overrides; a key it leaves untranslated uses Unmei's built-in translation of that key, not the plugin's source-language text.
- **L3 — Source language declared, default en-US.** `pack.ron` may name the language its scenes are written in. Without it, the source language is en-US. Any other language, en-US included, may be a translation.
- **L4 — The player picks from the pack's list.** The pack's languages are its source language plus each `lang/<tag>/` folder. A Language setting lists only those, and the choice is stored per pack. This is the first plugin choice the player makes. It is an exception to UI R12, because a language has to suit the reader, not only the author.
- **L5 — Translations keyed by source text.** A content translation lives in `lang/<tag>/` and maps each line, by scene, speaker and source text, to its translation. The dialogue DSL does not change. A translation whose source line was edited or removed is **stale**: it is not shown, and the check reports it.
- **L6 — Partial languages are offered, and marked.** A language may leave entries untranslated. The setting marks it as partial, and each missing or stale entry shows its source-language text. This is an exception to "falls back as a whole" (UI R14), so translations can ship gradually. A language that fails the check outright (for example, an unsupported format version or a malformed file) is not offered, and the reason is logged.
- **L7 — Placeholders kept, values optionally translated.** A translation must keep every `{var}` placeholder of its source line; the check enforces it. A language may map string state values to display text (for example `"high"` to its translation). Unmapped strings, numbers and bools show as the script set them.
- **L8 — Switching at any time.** The player may change the language from the title's Settings or from the Esc menu. The Backlog keeps source keys and shows them in the current language, so its whole history changes with the setting. The read log is unaffected, because it is keyed by the authored source line ([ADR 0011](adr/0011-read-log-as-a-third-persistence-class.md)).
- **L9 — Fallback faces built in.** The player embeds wide-coverage fallback faces (Noto), so every language renders without the pack shipping fonts. This adds tens of MB to the player archive, accepted. A pack or plugin may still ship its own faces under UI R19 and R20.
- **L10 — LLM translation drafting.** The author CLI gains translation drafting on the pattern of `unmei author draft`: a model drafts translations, a validation gate keeps placeholders and the authoring profile's lint rules (A4), and the translator accepts or discards each one.
- **L11 — Decisions are not translated.** NPC instructions and criteria stay in the source language. They are prompts to the Decisions model, not text a player reads.

## 3. Stage plugin

**Baseline.** The stage is fixed in code today ([`app/src/presentation/play/sync.rs`](../app/src/presentation/play/sync.rs)): a background is stretched to 1280×720, and sprites stand 240 px apart in NPC-id order at one height, with no way to place them.

- **S1 — Its own kind, with a reference stage.** A stage plugin lives in `stage/`. The **reference stage**, embedded in the player, is the stage with no plugin and the fallback when the plugin fails (F3).
- **S2 — Positions named by the script, drawn by the plugin.** `sprite <npc_id> <pose>` gains an optional `at <position>`. Positions come from one fixed list (for example `far_left`, `left`, `center`, `right`, `far_right`) that every stage plugin, the reference stage included, must place. Without `at`, the plugin's automatic arrangement applies.
- **S3 — What it may restyle.** Sprite placement, scale and layering for each position, with per-NPC offsets; background fit (cover, contain or stretch) and treatment (tint, vignette, blur); speaker focus, dimming or desaturating sprites that are not speaking; and ambient overlays, looped layers such as rain or petals, chosen per background or by script.
- **S4 — Script cues through one `fx` command.** The DSL gains `fx <name>`, which runs an effect the stage plugin defines (for example a shake, a flash or a zoom). The check flags a name the pack's stage plugin does not define. Under the reference stage, `fx` does nothing visible.
- **S5 — Motion that never blocks input.** Enter and exit effects, `fx` effects and ambient loops may animate. A key press finishes any running effect instantly and advances as today. Auto's timing does not change, and loading a save shows each effect's end state without replaying it. This amends UI R10 for the stage only; menu surfaces stay static.
- **S6 — Skip cuts to end states.** While text skip runs, enter, exit and `fx` effects cut straight to their end state. Ambient loops keep running.
- **S7 — Reduce motion and a flash cap.** A Reduce motion setting cuts every effect to its end state and holds ambient loops still. Independently of the setting, the check fails any effect that flashes more than three times in one second (WCAG 2.3.1).
- **S8 — Matching a speaker to a sprite.** A say-line's speaker matches the sprite of the NPC whose `npcs/<id>.ron` `name` equals it; failing that, a sprite whose NPC id equals it, ignoring case. The match always uses the source speaker text, never a translation (L5). With no match, no sprite is dimmed.

## 4. UI sounds and speaker style

These are additions to the UI plugin's interface, not a new kind.

- **U1 — Interface sound events.** A UI plugin may give a sound to: picking a choice; moving focus and pressing a control; opening and closing the Esc menu, going back a level and showing a confirm; advancing a say-line; a decision resolving; and a save or a load completing.
- **U2 — Sounds ship in `ui/`; `sfx_select` stays the default.** A plugin's `.ogg` files live in `ui/`, under UI R13's confinement and caps. The reference plugin keeps today's behaviour: picking a choice plays the pack's `assets/audio/sfx_select.ogg` if it exists, and nothing else sounds. Existing packs sound the same.
- **U3 — Interface sounds on the title.** Interface sounds may play on every surface a plugin restyles, the title and its doors included, on the SFX channel under the player's mix and mute. The title's music rule is unchanged: no BGM until play.
- **U4 — Skip mutes advance sounds and blips.** While text skip runs, line-advance sounds and voice blips (U5) do not play. Auto plays them, as manual advance does. Other interface sounds are unaffected.
- **U5 — Per-speaker style.** A UI plugin may vary, per speaker: the name tab's colour, face and ornament; the say-line's colour and face; the reveal pace (U6); and voice blips, a short sound per revealed character or syllable. Every per-speaker colour is held to the check's contrast rule (UI R22).
- **U6 — Pace scales the player's setting.** A speaker's reveal pace is a multiplier on the player's text speed, within a capped range. Instant stays instant for every speaker.
- **U7 — Speaker key.** A per-speaker style is keyed by the same match as S8: an NPC id, or else the raw source speaker text, so a one-off speaker such as "Teacher" can be styled and a translation never breaks the match.

## 5. Pack-wide check and preview

- **C1 — Player flags for the whole pack.** `unmei-play <pack> --check` and `unmei-play <pack> --preview <dir>` check and preview the pack and every plugin kind in it. They replace UI R22's `--ui-check` and R23's `--ui-preview`. The player owns the renderer, so the check, the preview and the boot keep one code path (UI R24). The author CLI keeps its own write-time validation and gains no check command.
- **C2 — What the check covers.**
  - Every rule that fails a pack load today, reported all at once rather than stopping at the first.
  - Every referenced background, sprite and audio file exists and decodes, including those in branches a playthrough may not take.
  - Every plugin kind's rules: UI, stage, language and authoring profile.
  - Flow lint: unreachable labels and scenes, a `decide` with no label for one of its landing keys, and a scene that neither ends nor jumps.
- **C3 — Errors and warnings, as text or JSON.** An error fails the check with a non-zero exit. A warning passes and is reported; partial and stale translations, flow lint and authoring-profile lint on hand-written lines are warnings. Output is readable text by default. `--format json` gives each finding's file, the path inside it, a rule id and the message.
- **C4 — What the preview renders.** UI R23's fixture states for every surface, plus `--at <scene>:<line>`, the real frame at that say-line, choice or decide, and `--scene <id>`, every park in a scene.
- **C5 — Reaching a line offline.** The preview replays from the top of the scene with the declared initial state. Each choice takes its first option and each decision lands on its fallback, with no API call and no key needed. `--state <file>` and `--pick` override these.

  *(Amended 2026-11-17: C4's `--at` and `--scene` and C5's replay are Keiko's one replay, and the `--state` and `--pick` formats are defined there ([Keiko spec §12](keiko-spec.md#12-the-shared-replay-k29)).)*
- **C6 — Motion as keyframe strips.** A park with motion renders its start, middle and end as separate PNGs.

  *(Amended 2026-11-19: the [check and preview spec](check-preview-spec.md) specifies C1–C5 and C7. C6 moves to the stage plugin spec, because nothing produces motion before it (§9.5). C2's "a `decide` with no label for one of its landing keys" is already a load error, and stays one, not a flow-lint warning (§6).)*
- **C7 — No GPU needed.** The preview runs on the software rasterizer CI already boots on, so an agent in a container can use it. Its frames may differ slightly from GPU frames; the native Monitor walk stays the rendered gate.

## 6. Authoring profile

- **A1 — Its own kind in `author/`.** `author/profile.ron` holds the pack's drafting rules, with its own format version. `pack.glossary.md` stays the home for terms and `unmei.author.ron` the home for the provider and model; neither changes.
- **A2 — What it holds.** A style guide for the whole pack (voice, register, tense, point of view and prose rules), sent to the model on every draft; each speaker's voice, with optional sample lines; lint rules (A4); and translation guidance per language (term translations, honorifics, register), used by L10.
- **A3 — An invalid profile stops the CLI.** When the profile is invalid or at an unsupported format version, `unmei author` stops with the error and its file and path instead of drafting without the pack's rules, and `--check` reports it as an error. The player ignores `author/`. This is an exception to "falls back as a whole": that rule protects players, and here an author is present to fix the file.
- **A4 — Lint rules: rejections when drafting, warnings when checking.** In `unmei author draft` and translation drafting, a line that breaks a lint rule is a rejection fed back for a retry, as the existing validation gate does. In `--check`, the same rules report hand-written lines as warnings, so an author's deliberate exception never fails the pack.
- **A5 — Fixed rule kinds plus regex.** A lint rule is one of a fixed set of parameterised kinds (maximum line length, banned words or phrases, allowed speakers, punctuation style) or a regular expression, evaluated by a linear-time engine so no pattern can hang the check. A rule may be limited to some speakers or scenes.
- **A6 — Prose and translations only.** The profile shapes `unmei author draft` and translation drafting. `unmei author npc` keeps using the glossary only, because decision configs are prompts to a model, not text a player reads.

## 7. Amendments to the UI plugin requirements

This page overrides the following [UI plugin](ui-plugins.md) requirements. Each was amended in place on 2026-11-15, with a dated pointer to the requirement here that overrides it.

- **R4** (never behaviour): allows a speaker's reveal pace (U6) and interface sounds (U1).
- **R9** ("not a localisation system"): copy keys become the interface side of language packs (L2).
- **R10** (no transitions): stage motion is allowed under S5; menu surfaces stay static.
- **R12** (the author chooses): the player chooses the language (L4).
- **R13** and **R19**: confinement and caps extend to audio files (U2).
- **R22** and **R23**: `--ui-check` and `--ui-preview` become `--check` and `--preview` (C1).
- **§12** (out of scope): localisation moves into scope.

## 8. Contracts this will amend

These are not amended yet. Each spec amends its contracts in place, with a dated pointer.

- [Pack layout](content-format.md#2-pack-layout): the optional `stage/`, `lang/` and `author/` folders (F2).
- [Manifest and state](content-format.md#6-manifest-and-state): the optional source language (L3).
- [Dialogue DSL](content-format.md#3-dialogue-dsl-unmei): `at <position>` on `sprite` (S2) and `fx <name>` (S4). The gallery's entry scan keeps one entry per NPC and pose, whatever the position.
- [Save slots](adr/0006-save-slots-on-disk-format-and-version-policy.md): the staging snapshot gains each sprite's position, so its format version likely changes.
- [Settings file](adr/0007-settings-file-beside-the-pack.md): Language (L4) and Reduce motion (S7).
- [Backlog](title-and-menu.md#backlog): it stores source keys and shows the current language (L8).
- [Type system](title-and-menu.md#type-system): the embedded fallback faces (L9).
- The audio contract in [slice-save-audio.md](slice-save-audio.md) and the title's "the title is silent" rule ([Title](title-and-menu.md#title)): `sfx_select` is re-described as the reference plugin's default (U2), and interface sounds may play on the title (U3).
- [Drafting contract](author-cli-drafting.md): context assembly gains the profile (A2), and the gate gains a lint layer (A4). Translation drafting (L10) needs its own contract.
- [Crate boundaries](architecture.md#crate-boundaries-and-invariants): the engine owns every kind's format and static rules, the lint rules included, because both the CLI and the player run them; the regex dependency joins the engine. The player gains `--check` and `--preview` (C1). *(Amended 2026-11-17: `--check` also reports the findings of every `.keiko` case under `tests/`, through the engine's Keiko validation ([Keiko spec §11](keiko-spec.md#11-check-and-packaging)).)*
- `packaging/about.toml` and the third-party licence list: the fallback faces' licence (L9).
- `GLOSSARY.md`: it gains **plugin kind**, **stage plugin**, **reference stage**, **language pack** and **authoring profile**.

## 9. Left to the spec

- The file formats of each kind, its caps and its fixed lists: positions, sound events, lint rule kinds.
- How "partial" appears in the Language setting, and whether it shows a completeness figure.
- Whether a slow speaker's pace (U6) should also delay auto, which today waits for the reveal to finish before its 2.0 s dwell.
- That per-speaker pace and voice blips have no effect at the default text speed, Instant, and whether the default should change for packs that use them.
- The `--state` file format and the `--pick` syntax, and the error when the default picks cannot reach a target line.
- The preview's flags for a language and for Reduce motion.
- Whether the stale-translation report is part of `--check` only or also a CLI command.
- Where voice notes belong when the glossary already holds some (the demo glossary describes how Ace speaks); the profile is the intended home.
- The order the kinds are specified and built in.

## 10. Out of scope

Everything UI §12 excludes, apart from localisation (§7); DSL macros and custom commands; decision-provider plugins; importers; translating NPC decision configs; a player-level choice of stage or UI plugin; video or animated preview output.
