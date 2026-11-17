---
title: UI plugins — requirements for a pack's own look
summary: The accepted requirements for UI plugins — declarative per-pack looks an author's agent writes, checks and previews, with variant E as the reference plugin.
keywords:
  - presentation-surface
  - content-pack
  - pack-validation
  - failure-posture
  - prototype
---
# UI plugins — requirements

**Status (2026-11-15).** Requirements accepted 2026-11-15, after one requirements review. **Not specified, not built.** The build-ready spec is the next phase. *(Amended 2026-11-15: the [Plugin family](plugin-family.md) requirements override R4, R9, R10, R12, R13, R19, R22, R23 and §12; each carries a dated pointer.)* Nothing here changes a contract yet: where a requirement will amend an existing contract, [Contracts this will amend](#9-contracts-this-will-amend) names it.

**What this is.** A **UI plugin** gives a content pack its own look on the title, the play frame, the Esc menu and every other player-facing surface. An author's coding agent writes it as data, checks it and previews it with the player, then installs it by copying it into the pack. Variant E, the settled [Default look](title-and-menu.md#default-look), becomes the **reference plugin**: the look a pack gets with no plugin, and the fallback when its plugin fails.

**Evidence.** The visual-language prototype (branch `prototype/8-vn-visual-language`, commit 8ff3fde, tracked on issue 8) wrote three demo-pack moods (Gothic, Cottagecore, Aero) as DESIGN.md files and rendered them over variant E. Tokens covered colour, type, radius and spacing, but every mood also needed hand-written gradients, shadows and glow, extra markup (Gothic's lancet arch, Cottagecore's pinned letter, Aero's bubbles), its own small copy ("Ace weighs her answer…"), its own numbering (Roman numerals, letters) and small looping animations. These requirements are sized to carry all of that.

**How this differs from the dropped revision.** An earlier "modular UI components for agents" revision, based on arXiv 2608.25512 (a runtime composition framework with a dynamic component loader and hot module replacement), was dropped as not viable. A UI plugin is the opposite: a fixed artifact written at authoring time, loaded once at boot and never recomposed while the game runs (R1).

---

## 1. What a plugin is

- **R1 — Author-time and static.** A plugin is written and checked before the pack ships. The player loads it once when the pack boots. It is never recomposed, swapped or reloaded while the game runs, and the shipped player has no live reload.
- **R2 — Declarative data only.** A plugin is data: design tokens plus one layout tree per surface it restyles, plus a copy table. The tree is built from node kinds such as boxes, text, images, gradients, borders, shadows and transforms. No code from a plugin ever runs: no WASM, no scripting language, no dynamic library, no custom player build. Packs stay data-only.
- **R3 — RON, with DESIGN.md's vocabulary.** Plugin files are RON, per [ADR 0001](adr/0001-ron-for-structured-content.md). Token groups and names follow DESIGN.md (`colors`, `typography`, `rounded`, `spacing`, `components`, and `{group.name}` references), so an agent can carry a DESIGN.md mood across directly. The Unmei check (R22) is the only linter. Authors need no Node or `designmd` tooling.

## 2. What a plugin may change

- **R4 — Look, layout and copy. Never behaviour.** A plugin may change:
  - tokens: colours, faces, sizes, radii and spacing;
  - depth and ornament: gradients, borders, shadows, glow, and ornament nodes such as arches, pins, bubbles, fleurons and corner marks;
  - glyphs and numbering: the advance mark, the deciding marker, and the style of choice numbers (for example `I`, `II` or `a`, `b`);
  - each surface's geometry (R7);
  - presentation copy (R9).

  It may not change behaviour: which controls exist, what they do, keys, walks and focus order, states and confirms, the pause doctrine, skip and auto, saves, the read log, the Backlog's recording, or any engine contract.

  *(Amended 2026-11-15: [Plugin family](plugin-family.md) U1 and U5–U7 add interface sounds and per-speaker style to what a plugin may change. A speaker's reveal pace is the one timing a plugin sets: it scales the player's text speed and never gates input.)*
- **R5 — Items keep engine order.** Where a slot holds a list (title rows, contents-page sections, ledger rows, cards, Backlog entries), the engine renders the items in their specified order. A plugin styles and places the list, never reorders or drops items, so what the player sees matches the walk.
- **R6 — Every player-facing surface, full reach.** R4 applies, in full, to the title (with its Settings and Gallery title doors), the play frame (box, name tab, prose, choices, quick row, markers), the Esc menu's contents page, and the interiors of its sections: Save and Load, Backlog, Timeline, Gallery with its still view, and Settings, plus the confirm dialogs on those surfaces.
- **R7 — Excluded surfaces.** The startup error screen, the Notice and the dev-only Monitor are always drawn in the built-in look. A broken plugin must never be able to hide a failure.
- **R8 — Whole tree per surface, engine content as slots.** For each surface it restyles, a plugin writes that surface's full node tree. The engine's content and controls appear as named slots the tree places: for example the prose, the speaker, the quick-row controls, the title rows, the ledger and the cards. A surface the plugin does not restyle keeps the reference plugin's tree. Leaving a surface out is not a failure.
- **R9 — Copy through a keyed table that keeps its information.** A plugin may override presentation copy only through a fixed table of string keys: for example the deciding line, headings, legends, section labels, empty states and confirm prompts. Each key declares the placeholders an override must keep (such as `{npc}`, `{scene}` or `{slot}`), so no override can drop what the player needs to know. Engine-supplied reasons, such as why a slot is unreadable, stay verbatim. Pack content (say-lines, option text, the pack name and the tagline) is never presentation copy. The copy table is not a localisation system.

  *(Amended 2026-11-15: the copy keys are now also the interface side of language packs. A language may translate them and a plugin's overrides of them ([Plugin family](plugin-family.md) L2).)*
- **R10 — Motion: looped keyframes on ornament.** A plugin may declare repeating keyframe loops of opacity, offset, scale and rotation, on ornament nodes only. Slots and controls never animate. There are no transitions between surfaces or states, and no timing that could delay or gate input.

  *(Amended 2026-11-15: this still binds UI plugins, and menu surfaces stay static. Motion on the stage behind the play frame belongs to the stage plugin, under [Plugin family](plugin-family.md) S5.)*

## 3. Where a plugin lives and how it is chosen

- **R11 — One plugin per pack, at `ui/`.** A pack has at most one plugin, in the pack's `ui/` folder, described by `ui/plugin.ron`. If the folder exists, the player uses it. `pack.ron` gains no field. Installing a plugin means copying a plugin folder to the pack's `ui/`.
- **R12 — The author chooses.** The pack's plugin is the only plugin. There is no player-level or user-level plugin and no player override.

  *(Amended 2026-11-15: this still holds for the look. The player does choose the language, from the pack's list ([Plugin family](plugin-family.md) L4).)*

## 4. Trust and failure

- **R13 — Same trust as pack data.** A plugin is pack content and gets the same trust as the rest of the pack: no signing, no allowlist, no install prompt. The player protects itself from a hostile or broken plugin through:
  - confinement: every file a plugin names resolves inside its `ui/` folder, with no absolute paths, no `..` and no escape through a symlink;
  - resource caps: on file count, file sizes, image dimensions, node count and tree depth;
  - no network access of any kind.

  *(Amended 2026-11-15: confinement and caps also cover the plugin's sound files ([Plugin family](plugin-family.md) U2).)*
- **R14 — Failure: the whole default look, and a visible Notice.** When the pack's plugin is invalid, has an unsupported format version (R27) or fails while loading, the player drops the whole plugin and shows the reference plugin on every surface. It logs the reason to `<pack-dir>.log`, and shows a Notice that the pack's look failed to load. The player never shows a partial mix of a plugin and the default. The pack still plays.
- **R15 — What fails a plugin.** Any check error (R22) fails the whole plugin at boot. The player runs the same rules as the check, so a plugin that passes the check loads (R24).

## 5. The reference plugin

- **R16 — Variant E is the reference plugin.** Variant E ([Default look](title-and-menu.md#default-look)) is written in the plugin format as plain files in the repository and embedded in the player at build time. It is the look with no `ui/` folder, the fallback under R14, and the source of every surface a plugin leaves out (R8).
- **R17 — E is built once, through the plugin renderer.** Variant E's Bevy build (issue 8's step 2) is done as the reference plugin through the plugin renderer, not as separate hand-written Rust. That build therefore waits on, or merges with, this work.
- **R18 — Exportable starting point.** A player author flag writes a copy of the reference plugin into a pack's `ui/`, as the starting point for an agent. The three mood fixtures (R29) stay in the repository as examples. Neither ships as a folder in the archives.

## 6. Fonts and assets

- **R19 — A plugin ships its own fonts and images.** Fonts and images live inside the plugin's `ui/` folder, under R13's confinement and caps. The player's five embedded faces (IBM Plex Sans Regular, SemiBold and Bold; Fira Mono Regular and Medium, per [Type system](title-and-menu.md#type-system)) are usable by name without shipping them.

  *(Amended 2026-11-15: a plugin may also ship `.ogg` interface sounds and voice blips in `ui/` ([Plugin family](plugin-family.md) U2, U5). The player also embeds fallback faces for other scripts ([Plugin family](plugin-family.md) L9).)*
- **R20 — Font licences are declared and checked.** `ui/plugin.ron` names a licence and a licence-text file for every font it ships, and may do so for images. The check fails a font that has no declaration, or whose licence-text file is missing. The check does not judge whether a licence allows redistribution. The licence texts ship inside the pack.

## 7. Developing and checking a plugin

- **R21 — The rules live in two crates, run by the player.** The engine (Bevy-free, per [Crate boundaries](architecture.md#crate-boundaries-and-invariants)) owns the plugin format, parsing and every rule that needs no layout: structure, references, slots and copy keys, placeholders, versions, confinement, caps and licences. The player owns the rules that need layout, and rendering. The CLI is unchanged.
- **R22 — The check.** A player author flag (for example `unmei-play <pack> --ui-check`) runs every rule and reports each error with its file and the path inside it. These are errors:
  - every required slot and control is placed exactly once;
  - each one lies inside the 1280×720 frame, is not zero-size, is not fully transparent, and is not covered by an opaque node;
  - text contrast is at least WCAG AA: 4.5:1 for body text and 3:1 for large text;
  - text is at least a minimum size;
  - all of R9, R13, R20 and R27.

  *(Amended 2026-11-15: the flag is `unmei-play <pack> --check`, which checks the whole pack and every plugin kind, adds warnings and offers `--format json` ([Plugin family](plugin-family.md) C1–C3).)*
- **R23 — The preview.** A player author flag (for example `--ui-preview <dir>`) renders every surface the plugin can reach in fixed fixture states to 1280×720 PNGs, through the player's own renderer. The states cover the title's states, a say-line, narration, a choice-wait, a decision-wait, scene end, each contents-page section, the still view and each confirm. An agent checks its work by looking at the PNGs.

  *(Amended 2026-11-15: the flag is `--preview <dir>`, which also renders script lines (`--at`, `--scene`) and motion as keyframe strips ([Plugin family](plugin-family.md) C1, C4–C7).)*
- **R24 — One code path.** The check, the preview and the boot path use the same parsing, rules and renderer, so what passes the check is exactly what loads, and what the preview shows is exactly what the player draws.

## 8. Versioning

- **R25 — One interface.** The plugin interface is the set of node kinds, token groups, surfaces, slot and control names, copy keys and their required placeholders.

  *(Amended 2026-11-17: the control names are the Keiko control ids, which [Keiko spec §3.2](keiko-spec.md#32-control-ids-k10) owns, so a case and a plugin name a control the same way.)*
- **R26 — Additive changes keep the version.** A new optional slot, token, copy key or node kind does not change the format version.
- **R27 — A format version, matched exactly.** `ui/plugin.ron` carries a [format version](../GLOSSARY.md#distribution) integer, as save slots and the read log do. A player reads only its own version. Any other version takes R14's failure path, and the check names the expected version. Removing or renaming anything in the interface, or adding a required slot to a surface, bumps the version. The reference plugin is always at the player's version.

## 9. Contracts this will amend

These are not amended yet. The spec phase amends each one in place, with a dated pointer:

- [Default look](title-and-menu.md#default-look): it stays the owner of variant E's look, but it is realised as the reference plugin (R16, R17), not as fixed code.
- [Pack layout](content-format.md#2-pack-layout): the pack gains the optional `ui/` folder (R11).
- [Type system](title-and-menu.md#type-system): faces become per-pack tokens, with the five embedded faces as the reference plugin's (R19).
- [Runtime failure posture](runtime-hardening.md#runtime-failure-posture): its inventory gains the plugin failure row (R14).
- [Crate boundaries](architecture.md#crate-boundaries-and-invariants): the engine gains the plugin format and its static rules, and the player gains the author flags (R21–R23).
- `GLOSSARY.md`: it gains **UI plugin** and **Reference plugin**.

## 10. Done when (for the build)

- The reference plugin renders variant E on every surface and passes [Default look](title-and-menu.md#default-look) §7's tests through the plugin renderer.
- All three prototype moods (Gothic, Cottagecore, Aero) are rebuilt as test-fixture plugins in the repository, outside the demo pack. Each passes the check and is previewed on every surface. Together they exercise tokens, gradients, shadows and glow, ornament markup, shipped OFL fonts, alternative numbering, copy overrides, looped motion and rotation (Cottagecore's tilts use Bevy 0.19.1's `UiTransform` rotation, which is present in the vendored sources).
- A pack with an invalid plugin plays in the reference look and shows the Notice, and its log names the reason.
- A plugin with a wrong format version takes the same path, and the check names the expected version.

## 11. Left to the spec

These details follow from the requirements and are settled in the spec, not here:

- the node kinds and their properties; the slot, control and copy-key lists per surface, and each key's required placeholders;
- the cap values, the minimum text size, and how "covered by an opaque node" is measured;
- the Notice's copy, and whether it shows on every launch;
- the exact author flag names and output layout, and what the export flag does when `ui/` already exists;
- how a plugin's 1280×720 layout behaves in borderless fullscreen on larger displays (today's layouts are fixed at the reference size; see [Player comfort](runtime-hardening.md#player-comfort) §5);
- whether ornament loops keep running during the presentation pause;
- where the fixture plugins live in the repository.

## 12. Out of scope

Behaviour changes of any kind; runtime composition, hot reload and live reload; code plugins (WASM, scripting, dynamic libraries, custom player builds); player-level or user-level plugins and overrides; signing, allowlists and install prompts; a plugin registry or distribution channel; transitions between surfaces or states; localisation; styling the error screen, the Notice or Monitor; the demo pack's own mood, which waits for the demo pack's story rewrite and can then become its first real plugin; gamepad support and input rebinding; window sizes below 1280×720.

*(Amended 2026-11-15: localisation is now in scope, as language packs ([Plugin family](plugin-family.md) §2).)*
