---
title: UI plugins — build-ready spec
summary: The build-ready spec for UI plugins and variant E as the reference plugin, covering the plugin folder, node kinds, surfaces, motion, caps, rules and preview.
keywords:
  - presentation-surface
  - content-pack
  - pack-validation
  - failure-posture
  - diagnostics
  - agent-tooling
---
# UI plugins — build-ready spec

**Status (2026-11-21).** **Slice 1 built** on the integration branch `spec/102-ui-plugins`; slices 2 and 3 not built. This page turns the accepted [UI plugin requirements](ui-plugins.md) R1–R27, as the [plugin family](plugin-family.md) amended them, into a build-ready spec. It settles every item that [UI plugins §11](ui-plugins.md#11-left-to-the-spec) left to the spec. The decisions below were reviewed with the maintainer on 2026-11-20 and are recorded where they apply. Each section names the requirement it implements. When this page and the requirements disagree, the requirements win and this page is wrong.

**What this spec covers.** The plugin format and its rules, the plugin renderer, the boot fallback, the author flags, and [variant E](title-and-menu.md#default-look) built as the **reference plugin** (R16, R17). Building variant E through the renderer completes the default look's own build. This spec also builds three pieces of shared groundwork from the [readiness audit](plugin-readiness.md#2-shared-groundwork): G1 (the plugin-folder loader), G3 (the interface string table, here the copy table) and G6 (a second asset source for `ui/`). Interface sounds and per-speaker style (plugin family U1–U7) are not part of it. They add to this format later (§16).

**Slices.** The build ships in three slices on one integration branch (*decided 2026-11-20*). Slice 1 changes nothing the player draws. Slices 2 and 3 complete the surfaces, so the branch opens one pull request to `main` after slice 3, because R14 forbids a release with some surfaces drawn by the plugin renderer and others by hand-written code.

| Slice | Contents | Done when |
|---|---|---|
| 1 | The plugin folder (§2), tokens (§3), node kinds (§4), surfaces, slots and copy keys (§5), motion rules (§6), confinement and caps (§7), findings and the plugin outcome (§8), the static rules in `--check` (§1), the reference plugin's files passing them, and `--export-ui` (§1). | §15's slice-1 rules pass. |
| 2 | The renderer and variant E on every surface (§11), the layout rules (§9), the boot fallback and the Notice (§10), the preview's new fixture states (§12), and the docs (§13). The card grid and today's hand-written surface trees are removed. | §15's slice-2 rules pass, and [Default look §7](title-and-menu.md#default-look) passes through the renderer. |
| 3 | The Gothic, Cottagecore and Aero fixture plugins (§13). | §15's slice-3 rules pass. |

---

## 1. Commands (R18, R22, R23)

The launch grammar in [`app/src/launch/mod.rs`](../app/src/launch/mod.rs) gains one form:

```text
unmei-play [PACK_DIR] --export-ui
```

- **`--export-ui`** writes the embedded reference plugin into `PACK_DIR/ui/` and exits 0. If `ui/` exists and is not empty, it writes nothing and exits 2 with a usage error that names the folder. The author deletes or renames `ui/` to start over (*decided 2026-11-20*). It joins the exclusive group of `--check`, `--preview` and Keiko's modes, and `--dev` with it is a usage error. It opens no window and touches no sidecar. A `PACK_DIR` that does not exist is a usage error: export creates no folder but `ui/` and its contents, so a mistyped path makes nothing. *(Amended 2026-11-21, after the slice-1 review.)*
- **`--check`** ([check and preview spec §1](check-preview-spec.md#1-commands-c1)) also runs every plugin rule: the static rules of §2–§8 in the engine and the layout rules of §9 in the player. Its output and exit codes are unchanged.
- **`--preview DIR`** draws through the pack's plugin, or through the reference plugin when the pack has none, and renders the fixture states of §12.

The author CLI is unchanged (R21).

## 2. The plugin folder (R11, R27, G1)

A plugin is the pack's `ui/` folder:

| File | Content |
|---|---|
| `ui/plugin.ron` | The manifest: `format_version`, a display `name`, the `fonts` and `images` declarations (§11), the `numbering` style (§5) and the optional `copy` overrides (§5). |
| `ui/tokens.ron` | The token groups (§3). |
| `ui/surfaces/<surface>.ron` | One tree per restyled surface (§4, §5). A surface with no file keeps the reference plugin's tree (R8). |
| any other file under `ui/` | Fonts, PNG images and license texts, named from the manifest. *(PNG only: amended 2026-11-24, §11.)* |

- **Absent and failed.** A pack with no `ui/` has no plugin, which is not a failure. A `ui/` with no `plugin.ron` is a failed plugin (§10).
- **Format version.** `format_version` starts at **1** and is matched exactly. It is a [format version](../GLOSSARY.md#distribution) beside the save and read-log versions. Any other value fails the plugin with `ui.version`, and the message names the expected version. The reference plugin is always at the player's version. R26 and R27 decide what bumps it.
- **The engine reads it.** The engine's new plugin module parses the folder, resolves tokens and runs every rule that needs no layout. It stays free of Bevy ([Crate boundaries](architecture.md#crate-boundaries-and-invariants)).

## 3. Tokens (R3)

`ui/tokens.ron` holds DESIGN.md's groups:

| Group | A token holds |
|---|---|
| `colors` | An sRGB color with alpha. |
| `typography` | A face (a shipped family or one of the five embedded faces), a weight, a size in px and a line height. |
| `rounded` | A corner radius in px. |
| `spacing` | A length in px. |
| `components` | A named style: a set of node properties (§4), which may itself use tokens. |

Any property may name a token as `{group.name}`. References resolve at load. An unknown token is `ui.token.unknown`, and a reference cycle is `ui.token.cycle`. Plugin authors need no DESIGN.md tooling. The check is the only linter (R3).

## 4. Node kinds (R2, R4)

Format version 1 has six node kinds:

| Kind | What it draws |
|---|---|
| `Box` | A flex container: direction, gap, padding, alignment, width and height, relative or absolute position with offsets, and z-index. It may have children, and may set `typography` and `color` as the text style they inherit (§4, Engine text). *(Amended 2026-11-21, after the slice-1 fix review.)* |
| `Image` | A file in `ui/`, with a fit of `cover`, `contain`, `stretch` or `none`. |
| `Glyph` | Decorative text of at most 8 characters in a typography token: a fleuron, a corner mark, an advance mark. It is ornament and never copy. |
| `Text` | The text of one copy key (§5), in a typography token. |
| `Slot` | One named piece of engine content, or one control (§5). A slot that shows text takes a typography token and a text color. *(Amended 2026-11-21, after the slice-1 review.)* |
| `List` | One engine list (§5), with one item template. |

- **Paint.** `Box`, `Image` and `Glyph` take a fill (a color, or a linear or radial gradient of color stops), a border (width, color and a radius per corner), a list of shadows (offset, blur, spread, color; glow is a shadow with no offset), an opacity and a transform (offset, scale and rotation). Bevy 0.19.1 supplies gradients, box shadows and `UiTransform` rotation.
- **Components.** Any node may name a `components` token, and its own properties override the component's.
- **No literal prose.** Words reach the screen only through copy keys and engine slots. A `Glyph` is the one literal, and its 8-character limit keeps it ornamental (*decided 2026-11-20*). This keeps every visible word in one keyed table, which language packs later translate (plugin family L2).
- **Lists keep engine order (R5).** The engine supplies a `List`'s items in their specified order. The item template places the item's own slots and controls. A plugin cannot reorder, filter or repeat items.
- **Control states.** A control `Slot` and a list item may carry property overrides for the states `focused`, `disabled`, `lit` (Auto and Skip while active) and `current` (the open contents-page section). Hover takes the `focused` style, as variant E does.
- **States inside a list item.** Every node inside a list item may carry state overrides, and they follow the state of the item's control. So a title row's label slot can turn white when the row's control is focused, though the label and the control are separate nodes. A node outside a list item keeps the rule above: only a control slot carries states. *(Amended 2026-11-21, after the slice-1 fix review.)*
- **Engine text.** Every slot or control that shows text (the prose, the speaker, the title rows, a ledger line, a control's label) takes `typography`, a typography token, and `color`, its text color. Its state overrides may change both, so a `lit` label can change tone. A text slot with no `typography` or `color` takes the nearest enclosing `Box` that sets it. *(Amended 2026-11-21, after the slice-1 fix review: the nearest enclosing `Box`, replacing "the surface root's", which named a node that could not set either. `Text` and `Glyph` nodes inherit the same way.)* A node that shows text and finds no `typography` or no `color`, on itself or on an enclosing `Box`, fails with `ui.text.unstyled`, so every piece of text has a face and a tone before the layout rules measure it. Both are additive under R26, so the format version stays 1. Spec rule 34 tests them. *(Amended 2026-11-21, after the slice-1 review.)*

A tree has one root `Box`. A node's path in findings is its place in the tree, for example `root.children[2].children[0]`.

## 5. Surfaces, slots, controls and copy keys (R6–R9, R25)

**Surfaces.** Format version 1 has ten:

| Surface | What it holds |
|---|---|
| `title` | The wordmark, the tagline, the five title rows, Continue's reason line, the legend, the backdrop and its veil. |
| `title-settings` | Settings opened from the title. |
| `play` | The dialogue box, the name tab, the prose, the two markers, the choices and the quick row. |
| `contents` | The Esc menu's contents page, with the Save and Load ledger. |
| `settings` | The Settings view. |
| `gallery` | The Gallery grid. |
| `still` | The Gallery's still view. |
| `timeline` | The Timeline view. |
| `legacy-picker` | The legacy-save target picker. |
| `confirm` | One tree for every confirm kind, with the kind's prompt as a slot. |

The error screen, the Notice and the Monitor are not surfaces (R7). The Backlog becomes an optional surface when it is built.

**The interface tables.** The build writes the UI plugin format reference page, which owns, for each surface:

- its required slots and its optional slots, with what each one shows;
- its required controls, named by their [Keiko control ids](keiko-spec.md#32-control-ids-k10) (R25);
- its lists and each list's item slots;
- its copy keys, each with the placeholders an override must keep (R9).

The tables are derived from what each surface shows today and from variant E. Every interface literal in the player's surface code becomes a copy key, and variant E's text becomes the reference plugin's value for it. Copy keys use the dotted form of the control ids, for example `play.deciding` with `{npc}`, `contents.heading.save` with `{scene}` and `confirm.overwrite.prompt` with `{slot}`. Engine-supplied reasons (an unreadable slot's reason, for example) and pack content (say-lines, option text, the pack name, the tagline) are slots, never copy keys.

**Quick-row controls.** Keiko's control table gains `play.save`, `play.load`, `play.auto`, `play.skip` and `play.menu`. `play.log` is reserved for the Backlog.

**Quick-row labels.** Each quick-row control draws its words from a copy key: `play.quick.save`, `play.quick.load`, `play.quick.auto`, `play.quick.skip` and `play.quick.menu`, none with placeholders. The reference values are Default look §1's `Save`, `Load`, `Auto A`, `Skip Tab` and `Menu Esc`. A plugin may override them like any copy key, but the keys stay `A`, `Tab` and `Esc`. `play.quick.log` waits for the Backlog, as `play.log` does. The copy keys for the help line and the corner cues, `play.help`, `play.cue.auto` and `play.cue.skip`, are removed, because Default look §1 removes those texts. No plugin has shipped, so removing them keeps format version 1. *(Amended 2026-11-24, after the #113 review.)*

**Numbering.** `numbering` in the manifest takes `arabic`, `roman-upper`, `roman-lower`, `alpha-upper` or `alpha-lower`, and sets how choice numbers display. The keys stay `1`–`9`.

**Markers.** The `▼` advance mark and the three-dot deciding marker are `Glyph` or `Image` nodes inside the `play` surface's `advance-marker` and `deciding-marker` slots. The engine decides when each one shows, and the plugin decides how it looks and loops (§6).

**Copy overrides.** The manifest's `copy` maps a copy key to its text. An unknown key is `ui.copy.unknown`. An override that drops a required placeholder, or names one the key does not declare, is `ui.copy.placeholder`.

## 6. Motion (R10)

*Decided 2026-11-20.*

- **A loop** is a list of keyframes, each giving opacity, offset, scale and rotation, with a period of at least 400 ms and an easing of `linear` or `ease-in-out`. It repeats forever from the moment its surface appears.
- **Where.** Loops are allowed on `Box`, `Image` and `Glyph` ornament nodes of the `title` and `play` surfaces, and on the content of the two marker slots. A loop anywhere else is `ui.motion.forbidden`: on a slot, a control, a list item, or any node of `title-settings`, `contents`, `settings`, `gallery`, `still`, `timeline`, `legacy-picker` or `confirm`. Menu surfaces stay static, as plugin family S5's amendment of R10 keeps them.
- **Flashing (WCAG 2.3.1).** A 400 ms floor alone does not stop flashing, because one loop can switch its opacity on and off several times within a period. A **flash** is a pair of opposing opacity changes, each of at least 0.2, between keyframes of one loop. The check walks the period as a ring, from the last keyframe back to the first. Consecutive steps in one direction join into one change, so a fade drawn as many small steps is one change, and a change smaller than 0.2 is dropped. Each pair of opposing changes is one flash, so the count is the number of direction reversals divided by two. Writing a loop's closing keyframe or leaving it implied gives the same count: `[0, 1]` and `[0, 1, 0]` are each one flash per period (*amended 2026-11-21: the slice-1 build counted every reversal as a flash, which counted `[0, 1]` twice*). The check scales the count to one second. A loop with more than 3 flashes per second fails with `ui.motion.flash`. This is the flash cap that [plugin family S7](plugin-family.md#3-stage-plugin) sets for the stage, defined here once so the stage spec can reuse it.
- **Real time.** Loops run on real time, so they keep running under the presentation pause. The contents page is opaque, so no play-frame loop is visible under it.
- **Never gating.** No loop gates input or delays a frame. There are no transitions between surfaces or states.

## 7. Confinement and caps (R13)

- **Confinement.** Every path a plugin names is relative, contains no `..` and resolves inside `ui/` after symlinks are followed. The engine checks each path before it reads the file. A path that fails is `ui.path.escape`. The `ui/` folder itself must resolve inside the pack folder: a `ui/` that is a link out of the pack is `ui.path.escape` before anything inside it is listed. *(Amended 2026-11-21, after the slice-1 review.)*
- **Caps** (*decided 2026-11-20*). Each cap is an error, `ui.cap`, naming the cap and the value found. The folder walk stops as soon as it passes the file cap, so an oversized folder is never listed in full (*amended 2026-11-21*):

  | Cap | Limit |
  |---|---|
  | Files in `ui/` | 256 |
  | One file | 16 MiB |
  | All files | 64 MiB |
  | Image size | 4096 × 4096 |
  | Nodes in one surface tree | 2,000 |
  | Tree depth | 32 |
  | One copy override | 512 characters |

  Raising a cap is additive (R26). Lowering one bumps the format version.
- **No network.** No node kind or property names a URL, and the player opens no connection for a plugin.

## 8. Findings and the plugin outcome (R14, R15, R21)

- **Findings.** Plugin findings use the existing [finding](check-preview-spec.md#2-findings-g2) with `ui.*` rule ids, added to the [rule-id table](check-preview-spec.md#4-rule-ids-c3). The file is the plugin file, such as `ui/surfaces/play.ron`, and the path is the RON path or node path inside it.

  | Id | Severity | Covers |
  |---|---|---|
  | `ui.file.read`, `ui.parse` | error | A plugin file that cannot be read, and invalid RON. |
  | `ui.version` | error | §2. |
  | `ui.path.escape`, `ui.cap` | error | §7. |
  | `ui.token.unknown`, `ui.token.cycle` | error | §3. |
  | `ui.surface.unknown` | error | A tree file for a surface §5 does not list. |
  | `ui.slot.unknown`, `ui.slot.missing`, `ui.slot.duplicate` | error | A slot or control the surface does not have, one it requires that is not placed, and one placed twice. |
  | `ui.copy.unknown`, `ui.copy.placeholder` | error | §5. |
  | `ui.font.license`, `ui.font.unknown` | error | §11. |
  | `ui.motion.forbidden`, `ui.motion.flash` | error | §6. |
  | `ui.layout.outside`, `ui.layout.empty`, `ui.layout.hidden`, `ui.layout.covered` | error | §9. |
  | `ui.contrast`, `ui.text.size` | error | §9. |
  | `ui.contrast.unknown` | warning | §9. |
  | `ui.text.unstyled` | error | §4, Engine text. *(Added 2026-11-21.)* |
  | `ui.image.format` | error | §11, Images are PNG. *(Added 2026-11-24.)* |

- **The plugin outcome, not a load error.** `load_pack` returns the loaded pack with a plugin outcome: absent, loaded, or failed with its findings (*decided 2026-11-20*). A failed plugin never makes `load_pack` fail, because the pack still plays (R14). One loader serves the check, the preview and the boot (R24).
- **In the check.** `--check` reports the plugin's findings in its normal order. Plugin errors count as errors, so a pack with a failed plugin exits 1.
- **Engine API.** `LoadedPack` gains the outcome, and the engine gains public plugin types. This ships in 0.3.0, a 0.x minor release.

## 9. Layout rules (R22)

The player runs these rules at 1280×720 over the same headless layout the renderer uses, for every surface in every fixture state of §12:

- **`ui.layout.outside`, `ui.layout.empty`.** A slot or control's laid-out rectangle lies inside the 1280×720 frame and has non-zero width and height.
- **`ui.layout.hidden`.** Its effective opacity (its own times its ancestors') is above 0.
- **`ui.layout.covered`.** It is covered when its whole rectangle lies inside the rectangle of a node drawn after it whose fill is opaque, with alpha 1 and effective opacity 1. Partial overlap is allowed, so an ornament may overlap a control's edge (*decided 2026-11-20*).
- **`ui.contrast`.** Text color is measured against its effective background: the composite of the fills under the text, down to the first opaque one. For a gradient, the worst stop counts. Body text needs 4.5:1. Large text, at least 24 px or at least 18.66 px in SemiBold or Bold, needs 3:1.
- **`ui.contrast.unknown`.** When the background under text reaches an image or the transparent frame, contrast cannot be computed. That is a warning, and the preview's frames are where an agent judges it (*decided 2026-11-20*).
- **`ui.text.size`.** Text is at least 12 px, the size of variant E's quick row (*decided 2026-11-20*).

The placement rules (`ui.slot.missing`, `ui.slot.duplicate`) are static, so the engine runs them in §8.

## 10. Boot, failure and the Notice (R7, R14)

- **Boot** runs the static rules and the layout rules for the surfaces it builds, through the same code as the check (R24). Any error fails the whole plugin.
- **On failure** the player draws every surface from the reference plugin, never a mix. It writes the findings to `<pack-dir>.log`, and the pack plays.
- **The Notice** shows `This pack's look failed to load. Showing the default look.` on every launch while the plugin fails (*decided 2026-11-20*), so what the player sees is always the current state. The log line starts with the same text and adds the findings.
- **The failure inventory.** [Runtime failure posture](runtime-hardening.md#runtime-failure-posture) gains a row for a failed plugin, with the Notice as its surface.
- **Built-in surfaces.** The error screen, the Notice and the Monitor keep their built-in look and are never drawn through the renderer.

## 11. The renderer and the reference plugin (R16–R20, R24, G6)

- **The renderer** turns a surface's resolved tree into Bevy UI nodes, and binds each slot and list to its surface owner's state. Each surface keeps its one owner ([ADR 0013](adr/0013-presentation-surface-partition.md)): the owner asks the renderer for its tree, then fills slots as it fills hand-built nodes today. Focus, walks, actions and confirms stay in the existing menu and input code, keyed by control id, never by node.
- **Fonts and images.** The player reads plugin files as bytes through the engine's confined paths and adds them to Bevy's asset stores directly. The pack's `assets/` root is unchanged.
- **Images are PNG.** Every image the manifest declares has a `.png` name and PNG bytes, which is the one image format the player decodes and the same format as pack art. Any other name, or a `.png` file whose bytes are not PNG, is `ui.image.format`, and the message names the file. The check and the boot then agree on every image (rule 25). No plugin has shipped yet, so narrowing the accepted images keeps format version 1. *(Amended 2026-11-24, after the #112 review: slice 1 accepted `.jpg`, `.jpeg`, `.webp` and `.bmp` too, which the player silently skipped.)*
- **Fonts are declared.** Each font the plugin ships names a family, a weight, a license and a license-text file in the manifest. A font file with no declaration, or whose license-text file is missing, is `ui.font.license`. A typography token that names a family neither shipped nor embedded is `ui.font.unknown`. The five embedded faces are usable by name without shipping them (R19, R20).
- **The reference plugin.** Variant E is written as plain plugin files in the repository and embedded in the player at build time. It renders [Default look](title-and-menu.md#default-look) §1–§6: the dialogue box, name tab and quick row (without `Log` until the Backlog is built), the centred choices with the question kept, the title's left column and veil, the contents page with its ledger and Exit to title at its foot, and the Monitor drawer at y 212–500. The card grid is removed. The Settings, Gallery and Timeline interiors keep their current views, rewritten as reference-plugin trees. It sets a face and a tone for every text it shows, Default look §6's table included: the choice option text (21 px, sans) and its number badge (mono), and the contents page's section entries (SemiBold) and their `current` state, as well as the slots slice 1 already styled. *(Amended 2026-11-21.)* The reference plugin passes every rule it imposes.
- **Fullscreen.** A plugin lays out at the 1280×720 reference. In borderless fullscreen it is placed exactly as today's surfaces are, with no scaling (*decided 2026-11-20*; [Player comfort](runtime-hardening.md#player-comfort) §5).

## 12. Preview (R23)

[Check and preview §9.2](check-preview-spec.md#92-fixture-states-c4)'s fixture-state table is replaced with variant E's surfaces:

| State | Reached by |
|---|---|
| `title-fresh`, `title-continue` | As in §9.2 today. |
| `title-settings` | Settings from the title. |
| `say`, `narration`, `choice`, `decision`, `scene-end` | As in §9.2 today. |
| `contents-<section>` | `menu` at the first say-line, then each section: `save`, `load`, `timeline`, `gallery`, `settings`. |
| `still` | As in §9.2 today. |
| `legacy-picker` | The legacy picker, in a pack with a legacy save. |
| `confirm-<kind>` | Each confirm kind, as in §9.2 today. |

An unreachable state stays a `preview.state.unreachable` warning. A pack whose plugin fails previews in the reference look and reports the plugin findings as errors with exit 1, and still writes its frames, so an agent sees what the player would show.

## 13. Fixtures, the gate and the docs

- **Fixture packs.** Small packs beside the check fixtures, each with a `ui/` that breaks one family of rules: version, parse, path escape, a cap, tokens, slots, copy placeholders, font licenses, motion, flashing, placement, cover, contrast and text size. One fixture restyles only `play`, to show that left-out surfaces come from the reference plugin.
- **Mood fixtures** (slice 3). Gothic, Cottagecore and Aero are rebuilt from the visual-language prototype (branch `prototype/8-vn-visual-language`, commit 8ff3fde) as fixture plugins in the repository, outside the demo pack. Together they use tokens, gradients, shadows and glow, ornament markup, shipped OFL fonts, alternative numbering, copy overrides, looped motion and rotation, as [UI plugins §10](ui-plugins.md#10-done-when-for-the-build) requires.
- **The gate.** The check gate runs `--check --format json` over each fixture. The reference plugin, the demo pack and each mood pass with no finding. The preview gate renders every reachable state for the demo pack and each mood.
- **Docs** (slice 2). The UI plugin format reference page (§5). The demo pack's `AGENT-GUIDE.md` gains a section on writing a plugin. The Guide's [Play the demo](guide/play-the-demo.md) and the player archive's README describe variant E instead of the card grid. [`GLOSSARY.md`](../GLOSSARY.md) gains **UI plugin**, **Reference plugin**, **Plugin renderer** and **Copy key**. [`testing.md`](testing.md) names the gates.

## 14. Amendments

**On this page's merge (2026-11-20)**, each with a dated pointer here:

- [UI plugins](ui-plugins.md): its status names this spec, and §11 is marked settled.
- [Plugin readiness §4](plugin-readiness.md#4-suggested-spec-order): step 3 is specified here, with G1, G3 and G6.
- [Plugin family S7](plugin-family.md#3-stage-plugin): the flash cap is defined in §6 here, for the stage spec to reuse.

**On the build's merge**, because they describe built behavior: [Default look](title-and-menu.md#default-look) (realized as the reference plugin); [Pack layout](content-format.md#2-pack-layout) (the optional `ui/`); [Type system](title-and-menu.md#type-system) (faces become per-pack tokens); [Runtime failure posture](runtime-hardening.md#runtime-failure-posture) (the plugin row); [Crate boundaries](architecture.md#crate-boundaries-and-invariants) (the engine's plugin module, the renderer and `--export-ui`); [Keiko spec §3.2](keiko-spec.md#32-control-ids-k10) (the quick-row ids); and [check and preview §9.2](check-preview-spec.md#92-fixture-states-c4) (§12's table).

## 15. Validation rules (the build's test list)

**Slice 1**

1. A pack with no `ui/` loads with the outcome absent. A `ui/` with no `plugin.ron` loads the pack with the outcome failed.
2. A plugin with `format_version` 2 fails with `ui.version`, and the message names version 1.
3. A path with `..`, an absolute path and a symlink out of `ui/` each fail with `ui.path.escape`, and no file outside `ui/` is read.
4. Each cap in §7, exceeded by one, fails with `ui.cap`. At the limit, it passes.
5. An unknown token and a token cycle fail with `ui.token.unknown` and `ui.token.cycle`.
6. A tree for an unknown surface, an unknown slot, a required control left out and a control placed twice each fail with their §8 id.
7. A copy override with an unknown key fails with `ui.copy.unknown`. One that drops `{slot}` fails with `ui.copy.placeholder`.
8. A font with no license declaration, or a missing license-text file, fails with `ui.font.license`.
9. A loop on a control, on a list item and on a `contents` ornament each fail with `ui.motion.forbidden`. A loop with a 300 ms period fails too.
10. A 400 ms loop that switches opacity between 0 and 1 three times fails with `ui.motion.flash`. A 2 s glow pulse passes.
11. A `Glyph` of 9 characters fails.
12. Every plugin finding carries a `ui.*` id and a node or RON path, and `--check` exits 1 on any plugin error.
13. The reference plugin's files pass every static rule, and the demo pack passes `--check` with no finding.

**Slice 2**

14. A control placed at x 1300, at zero size, at opacity 0 and under an opaque box fail with `ui.layout.outside`, `ui.layout.empty`, `ui.layout.hidden` and `ui.layout.covered`. An ornament overlapping half a control passes.
15. 14 px grey text on a mid-grey box fails with `ui.contrast`. The same text at 24 px passes at 3:1. Text over an image reports `ui.contrast.unknown` and the check exits 0.
16. 11 px text fails with `ui.text.size`.
17. A pack with a failed plugin boots in the reference look on every surface, shows the Notice on each launch, and its log names the rule. Its Keiko cases pass.
18. A plugin that restyles only `play` draws its own `play` and the reference plugin's other surfaces.
19. Default look §7's geometry and behavior list passes through the renderer, in the headless app tests.
20. Every existing behavior test (keys, walks, confirms, pause, skip and auto) and the Keiko gate pass, except where Default look itself changes behavior.
21. Each quick-row control is reachable by its Keiko id and does what its key does.
22. `numbering: roman-upper` shows `I`, `II` and `III`, and the key `2` still picks the second option.
23. `--export-ui` writes a plugin that passes `--check`. Over a non-empty `ui/` it exits 2 and changes nothing.
24. `--preview` over the demo pack writes every reachable state of §12 as a 1280×720 PNG. Over a pack with a failed plugin, it writes the frames in the reference look and exits 1.
25. The check and the boot accept exactly the same plugins: a fixture that passes the check boots without the Notice.

**Slice 3**

26. Gothic, Cottagecore and Aero each pass `--check` with no finding and preview every reachable state.
27. Together they use every node kind, gradients, shadows, glow, a shipped OFL font, alternative numbering, copy overrides, a loop and a rotation.

**Slice 1, added 2026-11-21** *(numbered after slice 3 so that earlier rule numbers stay stable)*

28. A `ui/` that is a link to a folder outside the pack fails with `ui.path.escape`, and nothing inside its target is read.
29. A `ui/` holding 10,000 files fails with `ui.cap`, and the walk lists no more than 257 of them.
30. At 400 ms, `[0, 1]` and `[0, 1, 0]` each count one flash and pass. At 300 ms they fail only the period floor.
31. A 400 ms loop fading from 1 to 0 and back in steps of 0.1 counts one flash and passes. The same fade done three times in the period fails with `ui.motion.flash`.
32. `--export-ui` with a `PACK_DIR` that does not exist exits 2 and creates nothing.
33. Rules 10 and 13 still hold under the new count: the three-flash loop of rule 10 still fails, and the reference plugin still passes.
34. A text slot with `typography: "{typography.prose}"` and a `color` token parses. An unknown token there fails with `ui.token.unknown`.

**Text styles, added 2026-11-21** *(rules 35, 36 and 38 are static and land on the integration branch before slice 2 starts; rule 37's parsing is static too, and its drawn half lands with #115)*

35. A `Box` with `typography` and `color` styles a text slot inside it that sets neither, and the nearest enclosing `Box` wins over an outer one.
36. A text slot with no `typography` or no `color`, on itself or on any enclosing `Box`, fails with `ui.text.unstyled`. A `Text` node or `Glyph` without them fails the same way.
37. A label slot inside a list item may carry a `focused` override of `color`, and it parses. Drawn, the label takes that color when the item's control is focused.
38. The reference plugin, with faces on the choice option text, the number badge and the section entries, still passes every static rule.

**Images, added 2026-11-24** *(static, and lands on the integration branch before #118)*

39. An image declared as `ornament/arch.jpg`, and a declared `ornament/arch.png` whose bytes are a JPEG, each fail with `ui.image.format`, and the message names the file. A declared PNG passes, and the reference plugin still passes every static rule.

**Quick-row labels, added 2026-11-24** *(with #114)*

40. The five quick-row controls draw `play.quick.save`, `.load`, `.auto`, `.skip` and `.menu`. With an override of `play.quick.auto`, the row shows the override, and `A` still toggles auto. A copy override of `play.help`, `play.cue.auto` or `play.cue.skip` fails with `ui.copy.unknown`.

## 16. Out of scope

- Interface sounds and per-speaker style (plugin family U1–U7). They add to this format later.
- The Backlog surface and its `Log` control. They come as an optional surface and the `play.log` id when the Backlog is built.
- The stage plugin, stage motion, the Reduce motion setting (S7) and keyframe strips in the preview. The stage spec decides whether Reduce motion also holds UI loops.
- Language packs. The copy table is their interface side, but no language is chosen or loaded here.
- Any behavior change beyond what Default look already specifies. Runtime composition, hot reload and live reload. Code plugins. Player-level or user-level plugins. Signing, allowlists and install prompts. A plugin registry.
- Transitions between surfaces or states.
- Styling the error screen, the Notice or the Monitor.
- Scaling or reflowing on displays larger than 1280×720, and windows smaller than it.
- The demo pack's own mood, which waits for its story rewrite.
- A plugin check or export in the author CLI.
- Comparing preview frames with stored frames.
