---
title: Unmei pack format at a glance
summary: "Find the content-format section that owns each part of a pack: manifest, state, scenes, NPCs and assets."
keywords:
  - content-format
  - content-pack
  - dialogue-dsl
  - decision-point
---
# Unmei pack format at a glance

This page lists each part of a pack (see [`GLOSSARY.md` § Content & packs](../../GLOSSARY.md#content--packs)) and links the section of [`docs/content-format.md`](../content-format.md) that defines it. This page gives only a gist of each part. The linked section holds the rules. The `.ron` files are written in RON, described in [`docs/content-format.md` § 1 Structured data: RON](../content-format.md#1-structured-data-ron).

| Part | What it holds | Owning section |
|---|---|---|
| `pack.ron` | The manifest: the pack's `name`, its `entry` scene and the optional `tagline`, `title_backdrop` and ordered `scenes` list | [`docs/content-format.md` § 6 Manifest and state](../content-format.md#6-manifest-and-state) |
| `state.ron` | The declared initial state: every key a scene sets or tests, each with a fixed kind | [`docs/content-format.md` § 6 Manifest and state](../content-format.md#6-manifest-and-state) |
| `scenes/<id>.unmei` | One scene per file, in the line-oriented dialogue DSL: say-lines, commands, choices, labels, jumps, conditionals and decision points | [`docs/content-format.md` § 3 Dialogue DSL](../content-format.md#3-dialogue-dsl-unmei) |
| `npcs/<id>.ron` | One NPC: `id`, `name`, an optional pinned decision `model` and its decision configs | [`docs/content-format.md` § 4 Decision configs and decision points](../content-format.md#4-decision-configs-and-decision-points) |
| `assets/` | `backgrounds/<id>.png`, `sprites/<npc>/<pose>.png` and `audio/<id>.ogg`, named by id in scenes and, for a background, in the manifest's `title_backdrop` | [`docs/content-format.md` § 2 Pack layout](../content-format.md#2-pack-layout) |
| `pack.glossary.md` (optional) | Pack-local terms, which the author CLI sends to the model whenever the file is present | [`docs/content-format.md` § 2 Pack layout](../content-format.md#2-pack-layout) and [`docs/author-cli.md` § 6 Prompt inputs](../author-cli.md#6-prompt-inputs-what-the-model-sees) |
| `AGENT-GUIDE.md` (optional) | Notes for an AI coding agent working on the pack. Read by no loader. It adds no content and changes no validation | [`docs/content-format.md` § 2 Pack layout](../content-format.md#2-pack-layout) |

## How the parts connect

- **Ids come from file names.** A scene id and an NPC id are each the file name without its extension, and the assets an id names are addressed the same way ([`docs/content-format.md` § 2 Pack layout](../content-format.md#2-pack-layout)).
- **Scenes move by label and scene id.** A `jump` goes to a label in the current scene first, then to a scene file ([`docs/content-format.md` § 5 Transitions](../content-format.md#5-transitions)). A choice option's target resolves the same way ([`docs/content-format.md` § 3 Dialogue DSL](../content-format.md#3-dialogue-dsl-unmei)).
- **A decision point joins a scene to an NPC.** The scene line `decide ace.greet -> reply` reaches the `greet` config in `npcs/ace.ron` and lands on a `reply.*` label in that scene. The config's `fallback` covers a failed call ([`docs/content-format.md` § 4 Decision configs and decision points](../content-format.md#4-decision-configs-and-decision-points)).
- **State is the shared vocabulary.** The keys `state.ron` declares are the keys that scenes set and test, and that an NPC config lists in `state_keys` to send ([`docs/content-format.md` § 6 Manifest and state](../content-format.md#6-manifest-and-state)).

## The example pack

`demo-pack` is a complete pack: three scenes (`first_meeting`, `reunion` and `epilogue`), one NPC (`ace`) with a `greet` decision, and stand-in art and audio. The repository holds it at `examples/demo-pack/`, and the CLI archive ships it as `demo-pack/`.

[`examples/demo-pack/AGENT-GUIDE.md`](../../examples/demo-pack/AGENT-GUIDE.md) gives part-by-part notes, including which files you edit by hand and which you replace by name. [`docs/content-format.md` § 7 Example pack](../content-format.md#7-example-pack) links the pack's main files.

## Next

- [`write-your-first-pack.md`](write-your-first-pack.md): copy the demo pack and validate an edit.
- [`author-with-the-cli.md`](author-with-the-cli.md): the `author npc` and `author draft` flows.
- [`install-the-cli.md`](install-the-cli.md): install and configure the CLI.

Guide home: [`README.md`](README.md), with the routes for players, authors and contributors.
