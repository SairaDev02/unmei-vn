---
title: Slice save, load and audio definition of done
summary: The vertical slice's bounded save, load and audio scope — the persisted payload, exact-resume semantics and the audio posture.
keywords:
  - persistence
  - save-slots
  - audio
---
# Slice save/load + audio — definition of done

**Status:** settled · **Scope:** the vertical slice.

Bounded versions of the two features charting pulled into the slice. Both are deliberately minimal: this is scope, not polish.

*Superseded for this area by the hardening plan:* [`docs/runtime-hardening.md` § Save slots](runtime-hardening.md) replaces the single fixed file and F5/F9-only scope with three manual slots plus an autosave and an Esc overlay, once built. The persisted payload below is unchanged apart from the added `saved_at` metadata ([ADR 0006](adr/0006-save-slots-on-disk-format-and-version-policy.md)).

### What is persisted — full position save

One save file, RON, containing:

- **State map** — the current `GameState` flat map (schema anchored by `state.ron`).
- **Position** — current scene id + the runner's `pc` index (+ which wait kind it was parked in).
- **Staging snapshot** — current background and active sprites, so loading re-applies visuals without replaying the scene.
- **BGM id** — the id (`play`'d payload) of the currently looping track, or none if stopped.

### Slot + interface

- **One fixed save file** next to the pack; no slot selection.
- **Dev keybinds, no UI:** **F5 = save, F9 = load**. No save/load buttons, no menu. A real save UI is post-slice. *(Historical slice scope; amended 2026-10-11: the built player surface is the Esc overlay, while retained F5/F9 target manual slot 1. F5 overwrites without confirmation. Their planned shared debug/release/`--dev` gate is [Monitor → Dev mode](runtime-hardening.md#dev-mode), not a change to the resume semantics below.)*

### Load semantics — exact resume

Loading reconstructs the `ScriptRunner` at the saved pc and re-applies the snapshot:

- **Say-wait** → re-show the saved line, wait for advance again.
- **Choice-wait** → re-show the same options; player re-picks.
- **Decision-wait** → **re-execute the `decide` line** from the reloaded state: a fresh Decisions call. No frozen in-flight request is ever persisted; re-firing is semantically honest because the alpha API is non-deterministic and live state was captured at save.
- **BGM** → re-`play` the saved loop; no position-within-track (loop restarts from the top).

**Save is always allowed**, in any parked state — no hidden "busy" rules.

## 2. Audio

*Superseded for audio settings by the hardening plan:* [`docs/runtime-hardening.md` § Audio settings](runtime-hardening.md#audio-settings) replaces the "no volume control" scope below with a persisted master/music/SFX mix plus mute. The music posture (every `play` loops, `stop` silences) and the pack-optional `sfx_select` convention are unchanged.

Ecosystem settled by fact: `bevy_kira_audio` 0.26 targets the prototype's Bevy 0.19 exactly — `.looped()` built in, ogg by default, channels available for music vs SFX.

### Definition of done

- **Every DSL `play` loops** (music posture). `stop` silences. Voice is out of the slice.
- **One hardwired choice SFX:** selecting a choice plays the fixed id `sfx_select` from the pack's audio namespace, once. **If the file doesn't exist: silence** — SFX is a pack-optional convention with zero config surface.
- **BGM id in the save**, restored on load (see §1).
- **No volume control** — no keys, no config, no sliders; sounds play at kira defaults. Revisit only if a real scene demands it.

### Demo audio

`examples/demo-pack/` gains real audio (one BGM track exercised by a `play` line, plus `sfx_select.ogg`) as part of the **slice's demo work** — not before. No placeholder audio committed now.

## Cost check (fits the slice bar)

One `Serialize/Deserialize` impl set on structs the build already owns (runner pc + staging + state map), one fixed-file write/read behind two keybinds, and the `bevy_kira_audio` integration plus one optional-asset SFX. No new UI, no DSL changes, no config schema.

---

*Settled 2026-09-26 in the save/load and audio interviews; save/load Q1–Q4, audio Q5–Q8.*
