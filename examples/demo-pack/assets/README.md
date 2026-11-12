# assets/

The demo pack's shipped art and audio. The spec resolves asset references
against this tree:

- `backgrounds/<id>.png` — used by `bg <id>`
- `sprites/<npc_id>/<pose>.png` — used by `sprite <npc_id> <pose>`
- `audio/<sound_id>.ogg` — used by `play <sound_id>` / `stop <sound_id>`

## Demo contents

| File | Referenced by |
|---|---|
| `backgrounds/school_gate.png` | `bg school_gate` (every scene — the same gate at morning and evening), and the manifest's `title_backdrop` (the title's full-window backdrop) |
| `sprites/ace/neutral.png` | `sprite ace neutral` |
| `sprites/ace/happy.png` | `sprite ace happy` |
| `audio/music_morning.ogg` | `play music_morning` |
| `audio/music_evening.ogg` | `play music_evening` |
| `audio/sfx_paper_rustle.ogg` | `play sfx_paper_rustle` |
| `audio/sfx_select.ogg` | hardwired choice SFX (pack-optional convention) |

These are deliberate **stand-ins** — flat-shape art and synthesized tone/loop
audio. They prove the staging/audio path end-to-end; an artist can replace any
file with the same name and nothing else changes.

## Title backdrop contrast

The manifest's `title_backdrop` reuses the backgrounds tree: the title draws
`backgrounds/<id>.png` full-window, scaled to cover and **exactly as authored** —
no veil, tint or overlay over it. Contrast of the wordmark, tagline and foot row
over that art is therefore the **pack's responsibility**, not the runtime's: a
pack that points `title_backdrop` at a busy or bright image should check its
title text stays legible.

## Generating real assets

Match the names above and the engine picks them up with no content or code
change:

- **Backgrounds** — 1280x720 PNG (drawn full-window).
- **Sprites** — PNG with transparency, any size (drawn standing at the bottom
  centre; 256x512 is the stand-in's size).
- **Audio** — OGG Vorbis; `play` loops, `stop` silences. The choice SFX is
  looked up as exactly `audio/sfx_select.ogg` and is silent when absent.
