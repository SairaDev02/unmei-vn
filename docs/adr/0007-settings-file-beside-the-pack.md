---
title: A sectioned, unversioned settings file beside the pack
summary: Preferences live in one sectioned, unversioned .settings.ron sibling of the pack that falls back to defaults field by field.
keywords:
  - settings-file
  - persistence
---

# The settings file: beside the pack, sectioned, and deliberately unversioned

Runtime hardening adds the first player preferences the runtime has ever stored. The audio mix is the opening one; player comfort inherits whatever this record settles. Two choices are hard to reverse once players have files on disk: **where a preference file lives**, and **what a missing field or an unreadable file means**. The save system already answered both questions in the opposite direction ([ADR 0006](0006-save-slots-on-disk-format-and-version-policy.md)) — this record explains why preferences answer differently.

**Decision.** Preferences live in `<pack-dir>.settings.ron` — a sibling of the pack directory, named after it, exactly like the slot files. One file per pack, RON, with a **sectioned root** so each hardening area owns a struct rather than inventing a second file:

```ron
(
    audio: (master: 1.0, music: 1.0, sfx: 1.0, muted: false),
)
```

The `play` section is skipped while it is all-default, so an audio-only file carries just `(audio: (...))`. The file has **no version field**: every field carries `#[serde(default)]`, unknown keys are ignored, and out-of-range numbers are clamped. A missing key falls back to that field's default; a file that does not parse at all falls back to *all* defaults with a warning in the log and is left on disk untouched. The type, its path derivation and its atomic temp-and-rename write live in `app/src/settings.rs` — Bevy-adjacent, with `engine/` untouched, because every value in the file is presentation state the headless runner will never read. `*.settings.ron` joins `.gitignore`'s `*.save.ron`.

Why the divergence from saves: a save is player data whose loss is unrecoverable, so the save format pays for a version floor and preserves what it cannot read. A preference is a cheap, reproducible value — the player can restore it by dragging a slider. A version field would buy nothing the defaults do not already buy, and would add a rejection path whose only outcome is "your sliders moved back". Preferences are therefore *permissive on read and lossy on rewrite*: a file written by a newer build parses, and the next write drops the keys this build does not know.

## Considered options

- **A user-level file in the OS config directory** (one mix for all packs). Rejected: charting settled "next to the saves", and the pack-adjacent layout is the project's standing portable, no-installer posture. Consequences accepted: preferences are per-pack, and moving the pack directory leaves them behind.
- **`settings.ron` inside the pack directory.** Rejected: it puts mutable player state inside the portable content directory and needs a pack-relative ignore rule.
- **A `SETTINGS_VERSION` with ADR 0006's additive floor** (accept `<=`, reject newer). Rejected: uniform with saves, but it adds a rejection path that ends in the same place as the plain defaults fallback — and it would treat an unknown-future field as a reason to discard the fields that *are* known.
- **A version field with strict equality.** Rejected for the reason saves rejected it: every additive field would reset the player's preferences.
- **Storing decibels (`-6.0`) or integer percent.** Rejected as the stored unit: decibels make a hand-editable file hostile and hide the `-60.0` silence cliff; percent quantises the file and needs a conversion in both directions. Linear amplitude `0.0..=1.0` is the slider's own domain and the backend-neutral one. (The slider still *displays* percent.)
- **Storing the mix inside `SaveFile`, per slot.** Rejected: a load must not change how loud the game is, and the mix outlives any one slot.
- **A flat root (`music`, `sfx`, …) instead of sections.** Rejected: the player-comfort fields would sit beside audio values with no grouping, and a future `master` would read oddly among comfort fields.
- **Engine-owned `engine/src/settings.rs` mirroring `save.rs`.** Rejected: it would give the headless engine a type it never reads, contradicting the presentation-pause decision's recorded posture that settings are pure `app/` presentation state. The cost is that `unmei-app` gains direct `serde` + `ron` dependencies (already in the lock file via the engine).

## Consequences

- Preferences are **per-pack** and **not portable with the pack directory** — copying or renaming the directory alone leaves the mix behind. Accepted for the demo-scale play; the file is regenerated at defaults on next run.
- A newer build's unknown keys are silently dropped when an older build rewrites the file. This is the price of having no version field; it is recorded here so a future reader does not mistake it for a bug.
- Applying a setting never restarts audio: the mix maps to kira channel volume, which re-applies to sounds already playing.
- The `play` section and its reset control are implemented: text speed, auto mode and fullscreen follow [Runtime hardening — Player comfort](../runtime-hardening.md#player-comfort), from the player-comfort design and its build. The original shape-only promise is fulfilled.
- No engine change, no `SaveFile` change and no `SAVE_VERSION` bump come out of this decision.
- Build-ready detail — the exact schema, the kira mapping, the Settings-view surface, persistence timing, the missing/corrupt posture and the test list — is in [`docs/runtime-hardening.md` § Audio settings](../runtime-hardening.md#audio-settings).

## Amendment (2026-10-13, shared sibling naming and atomic write)

The sentence above placing "its atomic temp-and-rename write" in `app/src/settings.rs` is narrowed. The **settings type, its schema, its path derivation, its permissive read posture and its lossy rewrite all stay in `app/src/settings.rs`** — no format, field or policy changes. What moved into the engine is only the two facts every pack-sibling class must agree on: the sibling **naming rule** and the **atomic write idiom**, now `engine/src/sidecar.rs` (`sibling_path`, `write_atomic`). Three modules held a near-verbatim copy of that write (the save slots, the read log and this settings file), and the app's `failure::pack_sibling_path` was already a second copy of `engine::save::pack_sibling`. ADR 0006's rejection of "handing layout to `app/`" argues the same way: the naming convention belongs in the engine, where it is testable headlessly. The credential file stays outside the sidecar rule ([ADR 0010](0010-credentials-live-in-a-user-level-file.md)) because it needs a private mode the plain write cannot express.
