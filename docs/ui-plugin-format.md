---
title: UI plugin format reference
summary: The UI plugin format and every surface's interface — the slots, controls, lists and copy keys a tree places, with the placeholders an override keeps.
keywords:
  - presentation-surface
  - content-pack
  - pack-validation
---
# UI plugin format reference

A pack restyles its player with a UI plugin: a folder of RON files holding design tokens and one node tree per surface. This page is the reference for that format, and it owns each surface's interface: the slots a tree places, the controls it must place, the lists with their item slots and item controls, and the copy keys the surface may show.

The engine holds the same interface tables in [`engine/src/ui/interface.rs`](../engine/src/ui/interface.rs), and a test compares that page with those tables, so the two cannot disagree. The rules and the reasons behind them are [the UI plugin spec](ui-plugin-spec.md)'s; this page says what to write and what the engine checks.

A plugin that breaks a static rule changes nothing a player sees. The pack keeps the reference look on every surface, still plays, and the check reports the findings.

## 1. The folder

A plugin lives in `ui/`, beside the pack's `pack.ron`.

| Path | Required | Holds |
| --- | --- | --- |
| `ui/plugin.ron` | yes | The manifest: the format version and the plugin's declarations. |
| `ui/tokens.ron` | no | The five token groups. A plugin without it has no tokens. |
| `ui/surfaces/<id>.ron` | no | One surface's node tree. A surface without a tree keeps the reference look. |
| `ui/<any other file>` | no | Font files and the PNG images the manifest declares. |

The ten surface ids are `title`, `title-settings`, `play`, `contents`, `settings`, `gallery`, `still`, `timeline`, `legacy-picker` and `confirm`. A tree file for any other id is `ui.surface.unknown`.

Every path the plugin names is relative and stays inside `ui/`. A path that is empty, absolute, names `..`, or leaves the folder once symlinks are followed is `ui.path.escape`, and no file outside `ui/` is read.

Every image the manifest declares is a PNG: its file name ends `.png` and its bytes are a PNG, the one image format the player decodes. Any other name, or a `.png` file whose bytes are not a PNG, is `ui.image.format`, and the message names the file.

Each cap is an error whose limit is inclusive, so a value at the limit passes:

| Cap | Limit |
| --- | --- |
| Files in `ui/` | 256 |
| One file | 16 MiB |
| Every file together | 64 MiB |
| One shipped image | 4096 × 4096 |
| Nodes in one surface tree | 2000 |
| Depth of one surface tree | 32, the root counted as 1 |
| One copy override | 512 characters |

## 2. The manifest

```ron
(
    format_version: 1,
    name: "Reference",
    numbering: "arabic",
    fonts: [
        ( family: "Inter", weight: 400, file: "fonts/inter-400.ttf",
          license: "OFL-1.1", license_file: "fonts/OFL.txt" ),
    ],
    images: [ (file: "ornament/arch.png") ],
    copy: { "play.deciding": "{npc} weighs the answer…" },
)
```

| Field | Required | Content |
| --- | --- | --- |
| `format_version` | yes | The format version. This build reads `1`, and any other value is `ui.version`. |
| `name` | no | The plugin's display name. |
| `numbering` | no | The style of a choice number. Section 3 lists the values. |
| `fonts` | no | A list of shipped faces: `family`, `weight`, `file`, `license` and `license_file`. |
| `images` | no | A list of shipped PNG art files, each `(file: "…")`. |
| `copy` | no | Copy-key overrides. Section 6 lists the rules. |

The engine embeds five faces, and a pack addresses them by family and weight with no file of its own: `IBM Plex Sans` at 400, 600 and 700, and `Fira Mono` at 400 and 500. Any other face belongs to a font file the plugin ships and declares, and an undeclared or unlicensed face is `ui.font.unknown` or `ui.font.license`.

## 3. Numbering

`numbering` sets how the engine renders a choice's number inside the `choice.number` item slot. The number keys stay `1` to `9`.

| Authored value | Chooses |
| --- | --- |
| `"arabic"` | `1`, `2`, `3` |
| `"roman-upper"` | `I`, `II`, `III` |
| `"roman-lower"` | `i`, `ii`, `iii` |
| `"alpha-upper"` | `A`, `B`, `C` |
| `"alpha-lower"` | `a`, `b`, `c` |

The field is optional and defaults to `"arabic"`. Any other value, and a value that is not a string, is `ui.parse` at the manifest's `numbering` field, and the message names the five styles.

## 4. Tokens

`ui/tokens.ron` holds five groups, each optional: `colors`, `typography`, `rounded`, `spacing` and `components`.

```ron
(
    colors: { "ink": (r: 0.06, g: 0.06, b: 0.08, a: 1.0) },
    typography: {
        "body": (face: "IBM Plex Sans", weight: 400, size_px: 25.0, line_height_px: 38.0),
        "tagline": (face: "Fira Mono", weight: 500, size_px: 12.0, line_height_px: 16.0,
                    tracking_px: 4.1),
    },
    rounded: { "box": 14.0 },
    spacing: { "gutter": 72.0 },
    components: {
        "panel": (
            fill: "{colors.ink}",
            border: (width: 1.0, color: "{colors.ink}"),
            radius: "{rounded.box}",
        ),
    },
)
```

A `{group.name}` reference resolves inside `colors`, `typography`, `rounded` and `spacing`. A reference that names no token is `ui.token.unknown`, and one that closes a chain back onto itself is `ui.token.cycle`.

A `colors` or `typography` token is a literal. A `typography` token is a `face`, a `weight`, a `size_px` and a `line_height_px`, and may add `tracking_px`: the space in pixels added after each letter, 0 when absent and negative to tighten. A `rounded` or `spacing` value may name another token in its own group. A component is a set of node properties; `component: "panel"` applies it, the node's own properties override it, and a component may name another component. A `{components.x}` value reference is `ui.token.unknown`, because a component is named by the `component` property.

## 5. Node trees

A surface's file holds its root node, and the root is a `box`. Every node has a required `kind` and may name a `component`.

| Kind | Holds | Required properties |
| --- | --- | --- |
| `box` | `children` | — |
| `image` | — | `file` |
| `glyph` | — | `glyph` (at most 8 characters), `typography` |
| `text` | — | `text` (a copy key), `typography` |
| `slot` | engine content | `slot` |
| `list` | one `item` template | `list`, `item` |

A complete tree for `still`, whose interface is one list, looks like this:

```ron
(
    kind: "box",
    direction: "column",
    children: [
        (kind: "list", list: "still.entry", item: (kind: "box", children: [
            (kind: "slot", slot: "still.label"),
            (kind: "slot", slot: "still.art"),
        ])),
        (kind: "text", text: "still.legend", typography: "{typography.body}"),
    ],
)
```

A `box` lays its children out: `direction` (`row` or `column`), `gap`, `padding` (a length, or `top`, `right`, `bottom` and `left`), `align_items` (`start`, `center`, `end` or `stretch`), `justify_content` (`start`, `center`, `end` or `space-between`), and `children`. It may also set `typography` and `color`, the text style every text inside it inherits. The nearest enclosing `box` that sets either property wins, so one box styles a whole group of rows.

Any node takes `width`, `height` (a length, `auto` or `fill`), `position` (`relative` or `absolute`), `left`, `top`, `right` and `bottom` for an absolute node, `z_index`, `opacity`, `transform` (`x`, `y`, `scale`, `rotation_deg`) and `loop`.

A `box`, `image` or `glyph` also takes paint: `fill` (a colour, a `{colors.key}` token, or a `linear` or `radial` gradient), `border` (`width` and `color`), `radius` (a number of pixels, a `{rounded.key}` or `{spacing.key}` token, or one per corner) and `shadow` (a list of `x`, `y`, `blur`, `spread` and `color`). A `linear` gradient is `(angle_deg: 0.0, stops: [...])`, where 0 runs left to right and 90 top to bottom, and a stop is `(at: 0.0, color: ...)` with `at` from 0 to 1. A `radial` gradient is `(center: (x: 0.5, y: 0.28), stops: [...])`, where the centre is a fraction of the node's width and height — 0 its left or top edge, 1 its right or bottom edge, and the middle when `center` is absent — and the gradient reaches the node's farthest corner. A `radius` rounds the node, so a filled box that draws no border rounds the same way. A `text` or `glyph` takes `typography` and `fill`.

A length is a number of pixels or a `{spacing.key}` token. A colour is `(r: 0.0, g: 0.0, b: 0.0, a: 1.0)` with channels from 0 to 1, or a `{colors.key}` token. A number may be written as an integer or a float. A name the format knows is a lowercase kebab-case string, for example `"ease-in-out"`; never a RON identifier.

A `slot` names engine content. Its name is one of the surface's slots, or a Keiko control id. A control may carry the four state overrides `focused`, `disabled`, `lit` and `current`, each a set of paint and text properties, and so may a `list`. Every node inside a list item may carry them too, and they follow the state of the item's control, so a row's label can turn white when its control is focused. A node outside a list item carries them only as a control slot or a `list`. A state that is not authored keeps the node's own paint.

A `slot` or control that shows text takes `typography` (a `{typography.key}` token) and `color`, the tone the engine's words are drawn in; the words are the engine's and only their look is the plugin's. One that sets neither takes the nearest enclosing `box` that does. A `text` or `glyph` node inherits the same way and draws its tone from its own `fill`. A node that shows text and finds no face or no tone, on itself or on an enclosing `box`, fails with `ui.text.unstyled`: every word has a look before the layout rules measure it. State overrides may change both, so a `lit` label can change tone while its text stays the engine's.

Two slots carry the plugin's own nodes instead of engine content: `advance-marker` and `deciding-marker` on `play`. A slot other than those two takes no content, and content inside one is `ui.slot.unknown`.

The engine draws a slot it has nothing to show in as absent, and a `box` whose children are all absent draws nothing either: that is how the name tab goes away with a narration line and how a marker appears only when the engine says so. An optional slot your tree places is therefore always in the tree and sometimes not on screen.

An unknown property, a property the node kind does not take, and a `glyph` of more than 8 characters are each `ui.parse` at the field that holds them.

## 6. Copy overrides

The manifest's `copy` maps a copy key to the text the plugin shows for it. The key's default text and its placeholders are the copy table's, in [`engine/src/copy.rs`](../engine/src/copy.rs).

- An override of a key the table does not hold is `ui.copy.unknown`.
- An override must carry every placeholder its key declares and no other. One that drops a placeholder or names one the key does not declare is `ui.copy.placeholder`. The override may move a placeholder within its sentence.
- An override longer than 512 characters is `ui.cap`, and the message names the length and the cap.

A `text` node names a copy key its own surface may show. A key the table does not hold, and a key another surface owns, are both `ui.copy.unknown`.

## 7. Rule ids

Every finding carries one of these ids and the plugin file, plus the RON field path or the node path it is about (`root.children[2].children[0]`, `copy["play.deciding"]`, `numbering`).

| Id | Covers |
| --- | --- |
| `ui.file.read` | A plugin file that cannot be read, including a folder with no `plugin.ron`. |
| `ui.parse` | Invalid RON, a file that is not UTF-8, a field the format does not accept, and a `numbering` style it does not name. |
| `ui.version` | A `format_version` this build does not read. |
| `ui.path.escape` | A path that is not relative and confined to `ui/`. |
| `ui.cap` | A cap of section 1 exceeded, and an over-long copy override. |
| `ui.token.unknown` | A `{group.name}` reference that names no token. |
| `ui.token.cycle` | A token or component chain that returns to its own start. |
| `ui.surface.unknown` | A tree file for a surface this format version does not have. |
| `ui.slot.unknown` | A slot, control or list the surface does not have, and content inside a slot that holds engine content. |
| `ui.slot.missing` | A required slot, control, item slot or item control the tree does not place. |
| `ui.slot.duplicate` | A required name the tree places twice. |
| `ui.copy.unknown` | An override, or a `text` node, naming a copy key the surface does not have. |
| `ui.copy.placeholder` | An override that drops a declared placeholder or adds an undeclared one. |
| `ui.font.license` | A shipped face with no declaration, or a license file that is missing. |
| `ui.font.unknown` | A face the pack neither embeds nor ships. |
| `ui.motion.forbidden` | A loop outside the places it is allowed, or under the 400 ms period floor. |
| `ui.motion.flash` | More than 3 flashes a second. |
| `ui.text.unstyled` | A text slot, control, `text` node or `glyph` whose `typography` or tone is set neither on itself nor on an enclosing `box`. |
| `ui.image.format` | A declared image without a `.png` name, or a `.png` whose bytes are not PNG. |

## 8. The interface, surface by surface

One table per surface lists what a tree for that surface places. A row whose `Shows` column is words rather than art names a slot or control that needs a face and a tone: section 5 says how a box passes them down, and `ui.text.unstyled` names one that finds neither.

| Kind | Meaning |
| --- | --- |
| `required slot` | A slot the tree must place, holding the engine content the row names. |
| `optional slot` | A slot the tree may place. |
| `list` | A list the tree must place. Its slot is one of the required slots, and the engine fills it with the list's items. |
| `required control` | A control the tree must place, by Keiko control id. A control that a placed list declares in its item template counts as placed. |
| `item slot` | A slot every item of the list places, with the row named under `In list`. |
| `optional item slot` | An item slot an item may also place. |
| `item control` | A control each item places: the engine shows the item's own and lays every control out in the template. |
| `copy key` | A copy key the surface may show. The row gives the placeholders its text carries, and the copy table's default text. A `text` node may name the key and an override may replace its text. |

Two item controls name an indexed family. `gallery.card.<n>` and `timeline.card.<n>` are declared as a family, and the template places one member, for example `gallery.card.1`: the engine numbers the member for each item, starting at 1.

### `title`

The title screen: the wordmark, the tagline, the five rows, the legend, the backdrop.
A tree places the rows as one list, and the five row controls in its item template.

| Kind | Name | In list | Placeholders | Shows |
| --- | --- | --- | --- | --- |
| required slot | `pack.name` | — | — | The pack's name, drawn as the wordmark. |
| required slot | `title.continue-reason` | — | — | Continue's reason line: the scene and saved-at of the save it would resume, or why Continue is off. |
| optional slot | `title.tagline` | — | — | The pack's tagline, uppercased by the engine. |
| optional slot | `title.backdrop` | — | — | The pack's title backdrop art. |
| required control | `title.continue` | — | — | Resumes the session; off when there is nothing to resume. |
| required control | `title.new-game` | — | — | Starts a new game. |
| required control | `title.settings` | — | — | Opens Settings from the title. |
| required control | `title.gallery` | — | — | Opens the Gallery from the title. |
| required control | `title.quit` | — | — | Quits the player. |
| list | `title.rows` | — | — | One row per title control, in walk order. |
| item slot | `title.row.label` | `title.rows` | — | The row label. |
| optional item slot | `title.row.reason` | `title.rows` | — | The row's caption line, when the engine has one. |
| item control | `title.continue` | `title.rows` | — | The row's control; the engine shows the row's own. |
| item control | `title.new-game` | `title.rows` | — | The row's control; the engine shows the row's own. |
| item control | `title.settings` | `title.rows` | — | The row's control; the engine shows the row's own. |
| item control | `title.gallery` | `title.rows` | — | The row's control; the engine shows the row's own. |
| item control | `title.quit` | `title.rows` | — | The row's control; the engine shows the row's own. |
| copy key | `title.row.continue` | — | — | `Continue` |
| copy key | `title.row.new-game` | — | — | `New Game` |
| copy key | `title.row.settings` | — | — | `Settings` |
| copy key | `title.row.gallery` | — | — | `Gallery` |
| copy key | `title.row.quit` | — | — | `Quit` |
| copy key | `title.legend` | — | — | `↑/↓ move · Enter select · click to choose` |
| copy key | `title.continue.empty` | — | — | `No autosave yet` |
| copy key | `title.continue.autosave` | — | `scene`, `saved_at` | `{scene} · {saved_at}` |

### `title-settings`

Settings opened from the title: the pack backdrop under a panel that draws the shared Settings body and the door's own back control.

| Kind | Name | In list | Placeholders | Shows |
| --- | --- | --- | --- | --- |
| required slot | `pack.name` | — | — | The pack's name. |
| required slot | `title.backdrop` | — | — | The pack's title backdrop art, behind the panel. |
| required slot | `settings.key-state` | — | — | The key row's state line: not set, set, checking, invalid, or from the environment. |
| required slot | `settings.key-path` | — | — | Where the credential file is stored. |
| optional slot | `settings.key-placeholder` | — | — | The key field's placeholder, shown while the field is empty and no key is stored. |
| optional slot | `settings.copy-path-label` | — | — | The Copy path control's label, which becomes its copied confirmation for a moment. |
| required control | `settings.back` | — | — | Closes Settings and returns to the title. |
| required control | `settings.master` | — | — | The master volume slider. |
| required control | `settings.music` | — | — | The music volume slider. |
| required control | `settings.sfx` | — | — | The sound-effect volume slider. |
| required control | `settings.mute` | — | — | Mute on or off. |
| required control | `settings.reset-audio` | — | — | Resets the audio settings, after a confirm. |
| required control | `settings.text-speed` | — | — | The text-speed step. |
| required control | `settings.auto` | — | — | Auto on or off. |
| required control | `settings.fullscreen` | — | — | Fullscreen on or off. |
| required control | `settings.reset-play` | — | — | Resets the play settings, after a confirm. |
| required control | `settings.key` | — | — | The Decisions key row. |
| required control | `settings.copy-path` | — | — | Copies the credential file's path. |
| required control | `settings.clear-key` | — | — | Deletes the stored key, after a confirm. |
| list | `settings.audio-rows` | — | — | One row per audio stop, in audio order. |
| item slot | `settings.audio.label` | `settings.audio-rows` | — | The stop's name. |
| item slot | `settings.audio.value` | `settings.audio-rows` | — | The stop's value: a percentage, or on and off. |
| item control | `settings.master` | `settings.audio-rows` | — | The row's slider; the engine shows the row's own. |
| item control | `settings.music` | `settings.audio-rows` | — | The row's slider; the engine shows the row's own. |
| item control | `settings.sfx` | `settings.audio-rows` | — | The row's slider; the engine shows the row's own. |
| copy key | `settings.heading` | — | — | `Settings` |
| copy key | `settings.group.audio` | — | — | `Audio` |
| copy key | `settings.group.play` | — | — | `Play` |
| copy key | `settings.group.decisions` | — | — | `Decisions` |
| copy key | `settings.control.master` | — | — | `Master` |
| copy key | `settings.control.music` | — | — | `Music` |
| copy key | `settings.control.sfx` | — | — | `SFX` |
| copy key | `settings.control.mute` | — | — | `Mute` |
| copy key | `settings.control.reset-audio` | — | — | `Reset audio` |
| copy key | `settings.control.text-speed` | — | — | `Text speed` |
| copy key | `settings.control.auto` | — | — | `Auto` |
| copy key | `settings.control.fullscreen` | — | — | `Fullscreen` |
| copy key | `settings.control.reset-play` | — | — | `Reset play` |
| copy key | `settings.control.copy-path` | — | — | `Copy path` |
| copy key | `settings.control.copy-path.copied` | — | — | `Copied` |
| copy key | `settings.control.clear-key` | — | — | `Clear key` |
| copy key | `settings.value.on` | — | — | `On` |
| copy key | `settings.value.off` | — | — | `Off` |
| copy key | `settings.key.not-set` | — | — | `Not set` |
| copy key | `settings.key.set` | — | `fingerprint` | `Set — {fingerprint}` |
| copy key | `settings.key.set-unchecked` | — | `fingerprint` | `Set — {fingerprint} · not checked (see the log)` |
| copy key | `settings.key.checking` | — | — | `Checking…` |
| copy key | `settings.key.invalid-shape` | — | — | `Invalid — does not look like a key` |
| copy key | `settings.key.invalid-rejected` | — | — | `Invalid — rejected by NanoGPT` |
| copy key | `settings.key.invalid-openrouter` | — | — | `Invalid — an OpenRouter key; enter a NanoGPT key` |
| copy key | `settings.key.from-environment` | — | `fingerprint` | `From environment — {fingerprint}` |
| copy key | `settings.key.from-environment-openrouter` | — | — | `From environment — Invalid, an OpenRouter key` |
| copy key | `settings.key.env-note` | — | — | `NANOGPT_API_KEY overrides the stored key. Unset it and relaunch to use the stored key.` |
| copy key | `settings.key.placeholder` | — | — | `Paste or type a NanoGPT key` |
| copy key | `settings.key.path` | — | `path` | `Stored in {path}` |
| copy key | `settings.back.slots` | — | — | `← Back to slots` |
| copy key | `settings.back.title` | — | — | `← Back to title` |
| copy key | `settings.text-speed.instant` | — | — | `Instant` |
| copy key | `settings.text-speed.fast` | — | — | `Fast` |
| copy key | `settings.text-speed.normal` | — | — | `Normal` |
| copy key | `settings.text-speed.slow` | — | — | `Slow` |
| copy key | `settings.text-speed.slowest` | — | — | `Slowest` |

### `play`

The play frame: the dialogue box, the name tab, the prose, the two markers, the choices and the quick row.

| Kind | Name | In list | Placeholders | Shows |
| --- | --- | --- | --- | --- |
| required slot | `prose` | — | — | The say-line's prose, revealed at the pack's text speed. |
| required slot | `advance-marker` | — | — | Ornament carrier: the plugin's own advance mark. The engine decides when it shows. |
| required slot | `deciding-marker` | — | — | Ornament carrier: the plugin's own deciding mark. The engine decides when it shows. |
| required slot | `quick-row` | — | — | Where the quick row runs. The five quick-row controls sit in the same box. |
| optional slot | `speaker` | — | — | The say-line's speaker, for the name tab. Narration has none. |
| optional slot | `deciding` | — | — | The deciding line, shown while a decision resolves. |
| optional slot | `scene-end` | — | — | The end-of-scene line. |
| required control | `play.save` | — | — | Opens the contents page on Save. |
| required control | `play.load` | — | — | Opens the contents page on Load. |
| required control | `play.auto` | — | — | Turns auto on or off; lit while auto runs. |
| required control | `play.skip` | — | — | Turns skip on or off; lit while skip is effective. |
| required control | `play.menu` | — | — | Opens the contents page. |
| list | `choices` | — | — | One button per option, in script order. |
| item slot | `choice.number` | `choices` | — | The option's number, in the manifest's numbering style. |
| item slot | `choice.text` | `choices` | — | The option's text. |
| copy key | `play.quick.save` | — | — | `Save` |
| copy key | `play.quick.load` | — | — | `Load` |
| copy key | `play.quick.auto` | — | — | `Auto A` |
| copy key | `play.quick.skip` | — | — | `Skip Tab` |
| copy key | `play.quick.menu` | — | — | `Menu Esc` |
| copy key | `play.deciding` | — | `npc` | `{npc} is deciding…` |
| copy key | `play.end-of-scene` | — | — | `— end of scene —` |

### `contents`

The contents page: the section column, the ledger, Resume and Exit to title.

| Kind | Name | In list | Placeholders | Shows |
| --- | --- | --- | --- | --- |
| optional slot | `pack.name` | — | — | The pack's name. |
| optional slot | `contents.scene` | — | — | The current scene's id. |
| required control | `overlay.resume` | — | — | Closes the page and resumes reading. |
| required control | `overlay.exit-to-title` | — | — | Returns to the title, after a confirm. |
| required control | `overlay.save-tab` | — | — | Shows the Save section. |
| required control | `overlay.load-tab` | — | — | Shows the Load section. |
| required control | `overlay.timeline` | — | — | Shows the Timeline. |
| required control | `overlay.gallery` | — | — | Shows the Gallery. |
| required control | `overlay.settings` | — | — | Shows Settings. |
| list | `contents.sections` | — | — | One row per section, in the walk order. |
| item slot | `contents.section.label` | `contents.sections` | — | The section's name. |
| item control | `overlay.save-tab` | `contents.sections` | — | The row's control; the engine shows the row's own. |
| item control | `overlay.load-tab` | `contents.sections` | — | The row's control; the engine shows the row's own. |
| item control | `overlay.timeline` | `contents.sections` | — | The row's control; the engine shows the row's own. |
| item control | `overlay.gallery` | `contents.sections` | — | The row's control; the engine shows the row's own. |
| item control | `overlay.settings` | `contents.sections` | — | The row's control; the engine shows the row's own. |
| list | `contents.ledger` | — | — | One row per save slot in slot order, with the legacy entry last in save mode. |
| item slot | `contents.ledger.label` | `contents.ledger` | — | The slot's label. |
| item slot | `contents.ledger.detail` | `contents.ledger` | — | The scene the save holds, the reason it cannot be read, or the empty mark. |
| item slot | `contents.ledger.saved-at` | `contents.ledger` | — | When the save was written. |
| item slot | `contents.ledger.action` | `contents.ledger` | — | What activating the row does: save here, overwrite, load, or load only. |
| item control | `overlay.slot.1` | `contents.ledger` | — | The row's control; the engine shows the row's own. |
| item control | `overlay.slot.2` | `contents.ledger` | — | The row's control; the engine shows the row's own. |
| item control | `overlay.slot.3` | `contents.ledger` | — | The row's control; the engine shows the row's own. |
| item control | `overlay.slot.autosave` | `contents.ledger` | — | The row's control; the engine shows the row's own. |
| item control | `overlay.legacy` | `contents.ledger` | — | The row's control; the engine shows the row's own. |
| copy key | `contents.rail.scene` | — | `scene` | `scene: {scene}` |
| copy key | `contents.rail.resume` | — | — | `▶ Resume  (Esc)` |
| copy key | `contents.rail.exit-to-title` | — | — | `Exit to title` |
| copy key | `contents.tab.save` | — | — | `Save` |
| copy key | `contents.tab.load` | — | — | `Load` |
| copy key | `contents.tab.timeline` | — | — | `Timeline` |
| copy key | `contents.tab.gallery` | — | — | `Gallery` |
| copy key | `contents.tab.settings` | — | — | `⚙ Settings` |
| copy key | `contents.card.legacy-import` | — | — | `Legacy save — Import…` |
| copy key | `contents.slot.1` | — | — | `Slot 1` |
| copy key | `contents.slot.2` | — | — | `Slot 2` |
| copy key | `contents.slot.3` | — | — | `Slot 3` |
| copy key | `contents.slot.autosave` | — | — | `Autosave` |
| copy key | `contents.slot.manual` | — | — | `Slot` |
| copy key | `contents.slot.kind.manual` | — | — | `manual` |
| copy key | `contents.slot.kind.autosave` | — | — | `game-written` |
| copy key | `contents.slot.status.empty` | — | — | `Empty` |
| copy key | `contents.slot.status.ok` | — | — | `Ok` |
| copy key | `contents.slot.status.unreadable` | — | — | `Unreadable` |
| copy key | `contents.slot.none` | — | — | `—` |
| copy key | `contents.slot.saved-at.none` | — | — | `no save yet` |
| copy key | `contents.action.save-here` | — | — | `Save here` |
| copy key | `contents.action.overwrite` | — | — | `Overwrite` |
| copy key | `contents.action.load` | — | — | `Load` |
| copy key | `contents.action.load-only` | — | — | `Load-only` |

### `settings`

Settings opened from the contents page: the shared Settings body.

| Kind | Name | In list | Placeholders | Shows |
| --- | --- | --- | --- | --- |
| required slot | `settings.heading` | — | — | The view's heading. |
| required slot | `settings.key-state` | — | — | The key row's state line: not set, set, checking, invalid, or from the environment. |
| required slot | `settings.key-path` | — | — | Where the credential file is stored. |
| optional slot | `settings.key-placeholder` | — | — | The key field's placeholder, shown while the field is empty and no key is stored. |
| optional slot | `settings.copy-path-label` | — | — | The Copy path control's label, which becomes its copied confirmation for a moment. |
| required control | `settings.back` | — | — | The way out of the view, back to the contents page. |
| required control | `settings.master` | — | — | The master volume slider. |
| required control | `settings.music` | — | — | The music volume slider. |
| required control | `settings.sfx` | — | — | The sound-effect volume slider. |
| required control | `settings.mute` | — | — | Mute on or off. |
| required control | `settings.reset-audio` | — | — | Resets the audio settings, after a confirm. |
| required control | `settings.text-speed` | — | — | The text-speed step. |
| required control | `settings.auto` | — | — | Auto on or off. |
| required control | `settings.fullscreen` | — | — | Fullscreen on or off. |
| required control | `settings.reset-play` | — | — | Resets the play settings, after a confirm. |
| required control | `settings.key` | — | — | The Decisions key row. |
| required control | `settings.copy-path` | — | — | Copies the credential file's path. |
| required control | `settings.clear-key` | — | — | Deletes the stored key, after a confirm. |
| list | `settings.audio-rows` | — | — | One row per audio stop, in audio order. |
| item slot | `settings.audio.label` | `settings.audio-rows` | — | The stop's name. |
| item slot | `settings.audio.value` | `settings.audio-rows` | — | The stop's value: a percentage, or on and off. |
| item control | `settings.master` | `settings.audio-rows` | — | The row's slider; the engine shows the row's own. |
| item control | `settings.music` | `settings.audio-rows` | — | The row's slider; the engine shows the row's own. |
| item control | `settings.sfx` | `settings.audio-rows` | — | The row's slider; the engine shows the row's own. |
| copy key | `settings.heading` | — | — | `Settings` |
| copy key | `settings.group.audio` | — | — | `Audio` |
| copy key | `settings.group.play` | — | — | `Play` |
| copy key | `settings.group.decisions` | — | — | `Decisions` |
| copy key | `settings.control.master` | — | — | `Master` |
| copy key | `settings.control.music` | — | — | `Music` |
| copy key | `settings.control.sfx` | — | — | `SFX` |
| copy key | `settings.control.mute` | — | — | `Mute` |
| copy key | `settings.control.reset-audio` | — | — | `Reset audio` |
| copy key | `settings.control.text-speed` | — | — | `Text speed` |
| copy key | `settings.control.auto` | — | — | `Auto` |
| copy key | `settings.control.fullscreen` | — | — | `Fullscreen` |
| copy key | `settings.control.reset-play` | — | — | `Reset play` |
| copy key | `settings.control.copy-path` | — | — | `Copy path` |
| copy key | `settings.control.copy-path.copied` | — | — | `Copied` |
| copy key | `settings.control.clear-key` | — | — | `Clear key` |
| copy key | `settings.value.on` | — | — | `On` |
| copy key | `settings.value.off` | — | — | `Off` |
| copy key | `settings.key.not-set` | — | — | `Not set` |
| copy key | `settings.key.set` | — | `fingerprint` | `Set — {fingerprint}` |
| copy key | `settings.key.set-unchecked` | — | `fingerprint` | `Set — {fingerprint} · not checked (see the log)` |
| copy key | `settings.key.checking` | — | — | `Checking…` |
| copy key | `settings.key.invalid-shape` | — | — | `Invalid — does not look like a key` |
| copy key | `settings.key.invalid-rejected` | — | — | `Invalid — rejected by NanoGPT` |
| copy key | `settings.key.invalid-openrouter` | — | — | `Invalid — an OpenRouter key; enter a NanoGPT key` |
| copy key | `settings.key.from-environment` | — | `fingerprint` | `From environment — {fingerprint}` |
| copy key | `settings.key.from-environment-openrouter` | — | — | `From environment — Invalid, an OpenRouter key` |
| copy key | `settings.key.env-note` | — | — | `NANOGPT_API_KEY overrides the stored key. Unset it and relaunch to use the stored key.` |
| copy key | `settings.key.placeholder` | — | — | `Paste or type a NanoGPT key` |
| copy key | `settings.key.path` | — | `path` | `Stored in {path}` |
| copy key | `settings.back.slots` | — | — | `← Back to slots` |
| copy key | `settings.back.title` | — | — | `← Back to title` |
| copy key | `settings.text-speed.instant` | — | — | `Instant` |
| copy key | `settings.text-speed.fast` | — | — | `Fast` |
| copy key | `settings.text-speed.normal` | — | — | `Normal` |
| copy key | `settings.text-speed.slow` | — | — | `Slow` |
| copy key | `settings.text-speed.slowest` | — | — | `Slowest` |

### `gallery`

The Gallery grid, one card per entry the pack declares art for.

| Kind | Name | In list | Placeholders | Shows |
| --- | --- | --- | --- | --- |
| optional slot | `gallery.empty` | — | — | The line shown when the pack has no art. |
| list | `gallery.cards` | — | — | One card per gallery entry. |
| item slot | `gallery.card.label` | `gallery.cards` | — | The entry's label. |
| item slot | `gallery.card.art` | `gallery.cards` | — | The entry's art. |
| item control | `gallery.card.<n>` | `gallery.cards` | — | The card's control. The template places one member; the engine numbers it per item, 1-based. |
| copy key | `gallery.heading` | — | — | `Gallery` |
| copy key | `gallery.empty` | — | — | `No images in this pack` |
| copy key | `gallery.legend` | — | — | `↑/↓/←/→ move · Enter view · click to open` |
| copy key | `gallery.back.slots` | — | — | `← Back to slots` |
| copy key | `gallery.back.title` | — | — | `← Back to title` |

### `still`

The Gallery's still view: one entry, browsed left and right.

| Kind | Name | In list | Placeholders | Shows |
| --- | --- | --- | --- | --- |
| list | `still.entry` | — | — | The entry being viewed. |
| item slot | `still.label` | `still.entry` | — | The entry's label. |
| item slot | `still.art` | `still.entry` | — | The entry's art. |
| copy key | `still.legend` | — | — | `←/→ browse · ↑/↓ or Esc or click to close` |

### `timeline`

The Timeline view: the pack's declared scenes, locked until visited.

| Kind | Name | In list | Placeholders | Shows |
| --- | --- | --- | --- | --- |
| required slot | `timeline.explanation` | — | — | The line explaining what a jump does. |
| optional slot | `timeline.empty` | — | — | The line shown when the pack declares no scenes. |
| list | `timeline.cards` | — | — | One card per declared scene. |
| item slot | `timeline.card.label` | `timeline.cards` | — | The scene's id, or the locked mark while it is unvisited. |
| item control | `timeline.card.<n>` | `timeline.cards` | — | The card's control, off until the scene is visited. The template places one member; the engine numbers it per item, 1-based. |
| copy key | `timeline.heading` | — | — | `Timeline` |
| copy key | `timeline.back` | — | — | `← Back to slots` |
| copy key | `timeline.explanation` | — | — | `Choose a visited scene to start it from the beginning with your current state.` |
| copy key | `timeline.empty` | — | — | `This pack has no Timeline.` |
| copy key | `timeline.locked` | — | — | `???` |
| copy key | `timeline.legend` | — | — | `↑/↓/←/→ move · Enter/Space select · Esc back · click to choose` |

### `legacy-picker`

The legacy-save target picker.

| Kind | Name | In list | Placeholders | Shows |
| --- | --- | --- | --- | --- |
| required slot | `legacy.heading` | — | — | The picker's heading. |
| required control | `legacy.cancel` | — | — | Closes the picker without importing. |
| list | `legacy.targets` | — | — | One row per manual slot. |
| item slot | `legacy.target.label` | `legacy.targets` | — | The slot's label, marked when importing would overwrite it. |
| item slot | `legacy.target.detail` | `legacy.targets` | — | The slot's state and the scene it holds. |
| item control | `legacy.target.1` | `legacy.targets` | — | The row's control; the engine shows the row's own. |
| item control | `legacy.target.2` | `legacy.targets` | — | The row's control; the engine shows the row's own. |
| item control | `legacy.target.3` | `legacy.targets` | — | The row's control; the engine shows the row's own. |
| copy key | `legacy-picker.heading` | — | — | `Import legacy save` |
| copy key | `legacy-picker.prompt` | — | — | `Choose a slot. Occupied slots are overwritten.` |
| copy key | `legacy-picker.cancel` | — | — | `Cancel` |
| copy key | `legacy-picker.target.overwrite` | — | `slot` | `{slot}  (overwrite)` |

### `confirm`

One tree for every confirm kind. The kind's prompt, detail and accept label are engine content, so one tree serves all of them.

| Kind | Name | In list | Placeholders | Shows |
| --- | --- | --- | --- | --- |
| required slot | `confirm.prompt` | — | — | The kind's question, drawn from its `title` copy key. |
| required slot | `confirm.accept-label` | — | — | The accept control's label, which differs by kind. |
| required slot | `confirm.yes` | — | — | The accept control. |
| required slot | `confirm.no` | — | — | The cancel control. |
| optional slot | `confirm.detail` | — | — | The kind's consequence, drawn from its `prompt` copy key, or `detail` for the Timeline jump. |
| required control | `confirm.yes` | — | — | Accepts the confirm. |
| required control | `confirm.no` | — | — | Cancels the confirm. |
| copy key | `confirm.overwrite.title` | — | — | `Overwrite` |
| copy key | `confirm.overwrite.prompt` | — | `slot` | `Overwrite {slot}? Its current save is replaced.` |
| copy key | `confirm.load.title` | — | — | `Load` |
| copy key | `confirm.load.prompt` | — | `slot` | `Load {slot}? Unsaved progress is lost and any decision in flight is cancelled.` |
| copy key | `confirm.reset-audio.title` | — | — | `Reset audio` |
| copy key | `confirm.reset-audio.prompt` | — | — | `Reset Master, Music and SFX to 100% and turn mute off? The file is not deleted.` |
| copy key | `confirm.reset-play.title` | — | — | `Reset play` |
| copy key | `confirm.reset-play.prompt` | — | — | `Reset text speed to Instant, turn auto off and return to windowed? The audio mix is untouched and the file is not deleted.` |
| copy key | `confirm.new-game.title` | — | — | `Start a new game?` |
| copy key | `confirm.new-game.prompt` | — | — | `The session in progress is discarded. Your saved slots and the autosave are untouched.` |
| copy key | `confirm.quit.title` | — | — | `Quit?` |
| copy key | `confirm.quit.prompt` | — | — | `Unsaved progress is lost and any decision in flight is cancelled.` |
| copy key | `confirm.exit-to-title.title` | — | — | `Exit to the title?` |
| copy key | `confirm.exit-to-title.prompt` | — | — | `Your session is kept — Continue resumes it where you left off.` |
| copy key | `confirm.clear-key.title` | — | — | `Clear key?` |
| copy key | `confirm.clear-key.prompt` | — | `path` | `The stored key is deleted from {path}. You can paste it again at any time.` |
| copy key | `confirm.clear-key.none` | — | — | `—` |
| copy key | `confirm.timeline-jump.title` | — | `scene` | `Jump to {scene}?` |
| copy key | `confirm.timeline-jump.detail` | — | — | `Unsaved progress is lost and any decision in flight is cancelled. The scene restarts from the beginning. Current game state is retained.` |
| copy key | `confirm.button.yes` | — | — | `Yes` |
| copy key | `confirm.button.start` | — | — | `Start` |
| copy key | `confirm.button.quit` | — | — | `Quit` |
| copy key | `confirm.button.exit` | — | — | `Exit` |
| copy key | `confirm.button.jump` | — | — | `Jump` |
| copy key | `confirm.button.cancel` | — | — | `Cancel` |

## 9. Names the format reserves

- `play.log` is reserved for the Backlog's control, which is not built. It is not a control id, so a tree that places it is `ui.slot.unknown`.
- Three clickable controls in the player have no Keiko id: the Gallery's back control, the Timeline's back control, and the view that closes the still. A plugin cannot place a control it cannot name, so the tables above leave them out and those three clicks stay the player's own.
- The error screen, the Notice and the developer Monitor are not surfaces. A plugin does not restyle them.
