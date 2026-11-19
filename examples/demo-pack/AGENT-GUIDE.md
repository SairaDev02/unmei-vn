# Agent guide: working on an Unmei content pack

This file is for an AI coding agent helping an author edit this pack. It points at the files that
matter and the ways to check a change. It does not replace the format documentation: the Guide
carries [Write your first pack](https://github.com/SairaDev02/unmei-vn/blob/v0.2.0/docs/guide/write-your-first-pack.md) and the content format
overview, which [the Guide's home page](https://github.com/SairaDev02/unmei-vn/blob/v0.2.0/docs/guide/README.md) links.

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
| `tests/<id>.keiko` | One Keiko case of steps and assertions, run offline by `unmei-play --take` | Yes |
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

- **Check the whole pack.** `unmei-play <pack-dir> --check` reports every load finding and every
  Keiko case finding in one run, one `<severity>: file:line: message [rule]` line each, then a
  summary. It runs no take, opens no window and never reads or writes the pack's sidecars or the
  user's key. Exit `0` means no error finding, `1` an error finding and `2` a usage error. Add
  `--format json` for one machine-readable document with each finding's severity and rule id.

- **Play the pack.** `unmei-play <pack-dir>` lists every load error on screen and in
  `<pack-dir>.log`, so a broken file is reported with its own `file:line`. A pack that loads plays,
  and without a key every `decide` point takes the author's fallback.

## Test the pack with Keiko

A case is one file `tests/<id>.keiko` in the pack folder: a `keiko 1` line, then any headers
(`decide`, `setting`, `fixture`, `fault`, `dev`, `start`, `state`), then one step per line. A step is a
player intent, a raw input, a clock move, a `frame` or `click` of the drawn screen, or an `expect`
assertion. The player ignores `tests/` during play.

- **Run every case:** `unmei-play <pack-dir> --take` prints one result per case. Name an id after
  `--take` to run one. Exit `0` means every case passed, `1` means a take failed or a case was
  invalid, and `2` means the command line was wrong.
- **Build a case one step at a time:** `--observe <id>` replays a case and prints what the player
  sees, and `--step <id> "<step line>"` replays it, applies the step and appends the step only when
  it passed.
- **Explore every path:** `unmei-play <pack-dir> --explore --budget 10000` takes every choice option
  and every decision landing, then saves each finding as a runnable case under `tests/findings/`. Each
  saved finding fails until the pack is fixed, so keep it as a regression case.
- **Look at the screen:** `frame [name]` writes the current frame as a 1280×720 PNG beside the pack,
  under `<pack-dir>.keiko/frames/<case>/`, and prints the path. The file stays after the run. `click <control>`
  presses a control by its role id and `click <x> <y>` presses a point, both through the player's own
  hit test. They need the software rasterizer, WARP on Windows and Mesa's llvmpipe on Linux, and fail
  with `keiko.render.unavailable` without it.
- **Read it from a script:** add `--format json` for one JSON document per run.
- **Offline unless asked:** a take needs no key. A `decide` header sets the landing a case asserts
  on, and every other decision takes the author's fallback. `--take --live` sends the unset ones to
  NanoGPT and needs `NANOGPT_API_KEY`.

The worked case in this pack is `tests/opening.keiko`, and the Guide page is
[Test your pack with Keiko](https://github.com/SairaDev02/unmei-vn/blob/v0.2.0/docs/guide/test-your-pack.md).
The rules are in [the Keiko spec](https://github.com/SairaDev02/unmei-vn/blob/v0.2.0/docs/keiko-spec.md).

## Preview the pack

A preview draws the pack on the software rasterizer and writes the frames as 1280×720 PNGs, so you
can see a line or a whole scene without playing to it. It opens no window, needs no key, and reads
and writes none of the pack's sidecars or the user's own key. Write the frames to a folder outside
the pack.

- **Draw one line.** `--at SCENE:LINE` writes `<dir>/at-<scene>-<line>.png` at the park that line
  produces:

  ```sh
  unmei-play <pack-dir> --preview preview --at reunion:19
  ```

  The line must be a say-line, a `choice` or a `decide`. Any other line, or a line the picks cannot
  reach, reports `keiko.case.reference` and writes nothing.

- **Draw a scene.** `--scene ID` writes one PNG at every park the default picks meet, from the
  scene's first line until it leaves the scene, named `<dir>/scene-<id>-<line>.png`:

  ```sh
  unmei-play <pack-dir> --preview preview --scene first_meeting
  ```

- **Reach the branch you mean.** `--pick 2` takes the second option at the first choice, and
  `--pick 2,1` the second and then the first. A choice past the end of the list takes option 1.
  `--state FILE` merges a RON map over `state.ron`, so a line only one state reaches can be drawn:

  ```ron
  (affinity_level: "high",)
  ```

  ```sh
  unmei-play <pack-dir> --preview preview --at epilogue:10 --state high.ron
  ```

- **Draw every surface.** With no `--scene` and no `--at`, the preview renders today's fixture
  states, each from a fresh take, as one PNG per state: `title-fresh`, `title-continue`, `say`,
  `narration`, `choice`, `decision`, `scene-end`, `menu-<section>` for each overlay section, `still`,
  and `confirm-<kind>` for each confirmation dialog:

  ```sh
  unmei-play <pack-dir> --preview preview
  ```

  A state this pack cannot reach, such as `say` when nobody but `Narrator` speaks, writes no file
  and is reported as a `preview.state.unreachable` warning naming the state.

- The folder is created when missing, and a folder inside the pack is a usage error. A rerun
  overwrites the files it names and leaves every other file in the folder alone.
- Exit `0` means every frame was written, or only `preview.state.unreachable` warnings occurred, `1`
  a finding or a pack that does not load, and `2` a usage error. A machine with no software
  rasterizer, WARP on Windows and Mesa's llvmpipe on Linux, fails with `keiko.render.unavailable`
  before any frame.

## Rules of thumb

- Scene, NPC and asset ids are filename stems. Renaming a file renames the id everywhere it is
  referenced.
- Every decision config needs a `fallback`, so the pack still plays when the Decisions call fails or
  no key is set.
- Every key a scene `set`s, a condition tests, an NPC config lists in `state_keys` or a say-line
  interpolates must be declared in `state.ron`, with the kind the script assigns.
- Prefer bucketed values (`"low"` / `"high"`) over raw counters in `state.ron`; the pack's own
  `pack.glossary.md` records the terms.
