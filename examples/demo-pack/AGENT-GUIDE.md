# Agent guide: working on an Unmei content pack

This file is for an AI coding agent helping an author edit this pack. It points at the files that
matter and the two ways to check a change. It does not replace the format documentation: the Guide
carries [Write your first pack](https://github.com/SairaDev02/unmei-vn/blob/v0.1.0/docs/guide/write-your-first-pack.md) and the content format
overview, which [the Guide's home page](https://github.com/SairaDev02/unmei-vn/blob/v0.1.0/docs/guide/README.md) links.

## Pack layout

| Path | What it is | Edit by hand? |
|---|---|---|
| `pack.ron` | Manifest: `name`, `entry` scene, and the optional `tagline`, `title_backdrop` and ordered `scenes` list | Yes |
| `state.ron` | The state keys scenes and decisions may read and set, with their declared kinds | Yes |
| `scenes/<id>.unmei` | One scene per file, in the line-oriented DSL; the scene id is the filename stem | Yes, or `unmei author draft` for prose |
| `npcs/<id>.ron` | One NPC: identity plus decision configs | Prefer `unmei author npc` |
| `assets/backgrounds/<id>.png` | A background, referenced as `bg <id>` or from the manifest's `title_backdrop` | Replace the file; keep the name |
| `assets/sprites/<npc>/<pose>.png` | A sprite pose, referenced as `sprite <npc> <pose>` | Replace the file; keep the name |
| `assets/audio/<id>.ogg` | Music and sound, referenced as `play <id>` and `stop <id>` | Replace the file; keep the name |
| `pack.glossary.md` | Optional pack-local terms, which the CLI sends to the model | Yes |
| `AGENT-GUIDE.md` | This file: notes for an agent, never loaded as content | No |

## Never touch

- The `demo-pack.*` files **beside** the pack folder — saves, settings, reading progress and the
  session log. They are player state, not content, and a running player rewrites them.
- The API key. The CLI reads it from an environment variable; the player keeps its own copy in the
  user's config folder. Never write a key into the pack, into `unmei.author.ron`, or into a commit.

## Check your work

- **List a scene's prose runs.** The command loads and checks the whole pack first, then prints the
  runs, and writes nothing:

  ```sh
  unmei author draft <scene> --list --pack <pack-dir>
  ```

- **Play the pack.** `unmei-play <pack-dir>` lists every load error on screen and in
  `<pack-dir>.log`, so a broken file is reported with its own `file:line`. A pack that loads plays,
  and without a key every `decide` point takes the author's fallback.

## Rules of thumb

- Scene, NPC and asset ids are filename stems. Renaming a file renames the id everywhere it is
  referenced.
- Every decision config needs a `fallback`, so the pack still plays when the Decisions call fails or
  no key is set.
- Every key a scene `set`s, a condition tests, an NPC config lists in `state_keys` or a say-line
  interpolates must be declared in `state.ron`, with the kind the script assigns.
- Prefer bucketed values (`"low"` / `"high"`) over raw counters in `state.ron`; the pack's own
  `pack.glossary.md` records the terms.
