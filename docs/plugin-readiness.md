---
title: Plugin readiness — how far the code is from the plugin family
summary: A read-only audit of the code each plugin kind touches, the groundwork the kinds share, the risks, and a suggested spec order.
keywords:
  - content-pack
  - presentation-surface
  - pack-validation
  - authoring-cli
  - failure-posture
---
# Plugin readiness — audit

**Status (2026-11-15).** A read-only audit, done before any spec. It records findings from the code at `314ae45`; nothing here is a requirement or a contract. The requirements are [UI plugins](ui-plugins.md) and [Plugin family](plugin-family.md). The spec order stays the maintainer's decision; §4 only suggests one.

**Ratings.** Each touch point is one of:

- **Seam** — a clear place to attach the change already exists.
- **Refactor** — the code exists but needs reshaping first.
- **New** — nothing exists to build on.

---

## 1. Per kind

### UI plugin (UI R1–R29)

| Touch point | Rating | Evidence |
|---|---|---|
| Layout as data | **New** | Every surface builds its Bevy tree in Rust. `app/src/presentation/` and `app/src/menu/` are about 6,900 non-test lines, with sizes, colours and text set inline. |
| Tokens | **Refactor** | `app/src/presentation/style.rs` holds 7 colour constants, but 35 more `Color::srgb` calls sit in other app files. |
| Variant E, the reference plugin | **New** | [Default look](title-and-menu.md#default-look) is **partly built**: the play frame, the title and the Esc menu's contents page are drawn from the reference plugin, while the three sub-view interiors still build their own trees. The [Backlog](title-and-menu.md#backlog) is not built. By R17, building the UI plugin also means building variant E. |
| One owner per surface | **Seam** | [ADR 0013](adr/0013-presentation-surface-partition.md) gives each surface one owner, so each can move to the plugin renderer separately. The plugin still has to cover every surface before release, because R14 forbids a partial mix. |
| Copy keys (R9) | **Refactor** | The engine's copy table owns every interface string the drawn surfaces show; the hand-built interiors still write theirs as literals, for example the Load confirm in `overlay/dialogs.rs`. |
| Plugin fonts and images | **Refactor** | The Bevy asset root is the pack's `assets/` folder (`style.rs`, `main.rs`), and the five faces are loaded with `include_bytes!`. `ui/` sits outside that root, so it needs a second asset source or direct byte loading. |
| Excluded surfaces (R7) | **Seam** | The error screen, the Notice and the Monitor already live in `app/src/failure/` and `app/src/monitor/`, apart from the surfaces a plugin restyles. |

### UI sounds and speaker style (U1–U7)

| Touch point | Rating | Evidence |
|---|---|---|
| `sfx_select` default | **Seam** | It is one hard-coded path in `app/src/input/play.rs:119`, so it becomes the reference plugin's single sound event. |
| Sounds on the title | **Refactor** | `sync_audio` in `play/sync.rs` stops the BGM channel while the title is up. SFX are a separate channel, so U3 is a rule change there, not a new mechanism. |
| Reveal pace | **Seam** | The reveal clock is in `app/src/presentation/play/reveal.rs`, and skip is in `app/src/core/skip.rs`. |
| Everything else | **New** | This builds on the UI plugin's renderer and cannot start before it. |

### Stage plugin (S1–S8)

| Touch point | Rating | Evidence |
|---|---|---|
| Placement | **Refactor** | `sync_staging_visuals` (`play/sync.rs`) stretches the background to 1280×720 and places sprites 240 px apart at y −120. This matches the plugin-family baseline. |
| Sprite order and position | **Refactor** | `StagingState.sprites` is a `BTreeMap<npc_id, pose>` (`engine/src/save/mod.rs:59`), so order is alphabetical and it has no position. Adding `at` changes `Line::Sprite`, `StageCommand::Sprite`, the parser and the saved snapshot. |
| Save format | **Refactor** | `SAVE_VERSION` is 2. Storing positions in saves changes the save format ([ADR 0006](adr/0006-save-slots-on-disk-format-and-version-policy.md)). |
| Background fit | **Seam** | `cover_size` and `contain_size` already exist in `style.rs`. |
| Motion, `fx` and ambient loops | **New** | The play surface has no animation system, only the reveal clock. |
| Speaker match (S8) | **Seam** | `Npc` has `id` and `name` (`engine/src/content/mod.rs:26`), which is what S8 needs. |

### Language packs (L1–L11)

| Touch point | Rating | Evidence |
|---|---|---|
| Source text at display time | **Refactor** | The runner interpolates `{var}` before it parks: `Wait::Say { text: interpolate(..) }` (`engine/src/runner/mod.rs:173`). A translation is keyed by source text (L5), so presentation needs the uninterpolated line and its scene. `Wait` is serialized into saves, so changing its shape touches the save format too. |
| Choice text | **Refactor** | `Wait::Choice` carries display text as written. Translating it needs the same source key. |
| Read log (L8) | **Seam** | It is keyed by scene and authored line number (`engine/src/read_log.rs`), not by text, so translation leaves it unaffected, as L8 assumes. |
| Backlog source keys (L8) | **Seam** | The Backlog is not built yet, so it can store source keys from the start. |
| Interface strings (L2) | **Refactor** | This is the same missing string table as UI R9. |
| Language setting (L4) | **Seam** | The settings file applies defaults field by field ([ADR 0007](adr/0007-settings-file-beside-the-pack.md)), so a Language field is an additive change. |
| Fallback faces (L9) | **New** | Only the five Latin faces are embedded. |
| Translation drafting (L10) | **Seam** | The pattern exists in `unmei author draft`. |

### Pack-wide check and preview (C1–C7)

| Touch point | Rating | Evidence |
|---|---|---|
| Flags | **Seam** | `app/src/launch/mod.rs` has a hand-written parser that returns a `LaunchRequest` enum. Today it knows only `--dev`, `--help` and `--version`, so `--check` and `--preview` become new variants. |
| Report everything at once (C2) | **Refactor** | `validate` collects errors, but `load_pack` returns early when `pack.ron` or `state.ron` can't be read or decoded, and after scene parse errors (`engine/src/pack/mod.rs:127–194`). |
| Rule ids and in-file paths (C3) | **Refactor** | `LoadError` has a file, a line and a message, with no rule id and no path inside the file. `Warning` has no line. Adding rule ids touches about 20 error sites in `validate.rs` and the parser. |
| Asset existence and decode (C2) | **New** | The loader states that assets are not checked. Decoding images and audio needs decoders, and the engine must stay free of Bevy. Where decoding lives (an engine dependency, or the player) is a spec decision. |
| Flow lint (C2) | **New** | No reachability analysis exists. |
| Rendering frames to PNG | **Seam** | `app/examples/monitor_acceptance` already captures screenshots, and CI runs the player under Xvfb with Mesa's software rasterizer (`.github/workflows/boot.yml`). |
| Reaching a line offline (C5) | **Seam** | `ScriptRunner` is headless, and each decision has a declared fallback to land on. |

### Authoring profile (A1–A6)

| Touch point | Rating | Evidence |
|---|---|---|
| Context assembly | **Seam** | `cli/src/gen/draft_prompt.rs` already reads `pack.glossary.md` into the prompt context, so the profile joins it there. |
| Gate lint layer | **Seam** | `cli/src/staging/validation.rs` holds the validation gate, and a lint layer slots in beside it. |
| Regex | **New** | No crate depends on `regex`. Adding it to the engine passes `scripts/check-invariants.py`, which bans only Bevy. |
| Player impact | none | The player ignores `author/`. |

## 2. Shared groundwork

These pieces serve more than one kind. Building them once, early, avoids designing them again per kind.

- **G1 — A plugin-folder loader in the engine:** RON decoding with errors that give the file and the path inside it, confinement (no absolute paths, no `..`, no symlink escape), caps, and an exact format-version match. Every kind needs it (F1).
- **G2 — A diagnostics model** with a rule id, the file, the path inside it, a line and a severity, collected rather than stopping at the first error. `--check` needs it, every kind reports through it, and `LoadError` and `Warning` move onto it.
- **G3 — An interface string table.** Copy keys with their required placeholders serve both UI R9 and language packs (L2).
- **G4 — One speaker-match function** in the engine, shared by stage focus (S8) and speaker style (U7).
- **G5 — One save-format change.** Sprite positions (S2) and, possibly, source keys in `Wait` (L5) both change what a save holds. Planning them together avoids two version bumps.
- **G6 — A second asset source** for `ui/` and `stage/`, outside the pack's `assets/` root.

## 3. Risks

- **R-A: the UI plugin is the largest piece by far.** It includes the unbuilt variant E and replaces about 6,900 lines of surface code. Specifying it first could hold up every kind that depends on it.
- **R-B: "No GPU" still needs a display.** CI renders under Xvfb. C7 asks for a preview that runs in a container, which works if the container has Xvfb and Mesa. A render path with no window at all is unproven.
- **R-C: image and audio decoding in the check.** Putting decoders in the engine adds dependencies beside its Bevy-free rule. Putting them in the player splits the check across both crates.
- **R-D: archive size.** Noto fallback faces add tens of MB (L9, accepted). Release certification and packaging may have size expectations to update.
- **R-E: save compatibility.** G5 affects every player who already has saves. ADR 0006's version floor decides what happens to their existing saves.

## 4. Suggested spec order

*(Amended 2026-11-17: Keiko is specified before any step below ([Keiko K30](keiko.md#8-relation-to-other-work), [Keiko spec](keiko-spec.md)). Step 1's preview reuses Keiko's replay.)*

1. **Check and preview, with G1 and G2.** It needs no plugin kind, authors and agents can use it at once, and every later kind reports through it (UI R24). The preview's fixture states start with today's surfaces. *(Amended 2026-11-19: specified in the [check and preview spec](check-preview-spec.md) with G2 only. G1 moves to the first plugin-kind spec, which has a folder to load.)*
2. **Authoring profile.** It is small, touches only the CLI, and is independent of the player. It can run alongside step 1.
3. **UI plugin with variant E (R17), including G3 and G6.** This is the largest piece, and it waits for step 1's diagnostics. *(Amended 2026-11-20: specified in the [UI plugin spec](ui-plugin-spec.md), which also builds G1.)*
4. **Stage plugin, with G4 and G5.** It doesn't depend on the UI renderer, so it can run alongside step 3. Its motion previews need step 1.
5. **UI sounds and speaker style.** These are additions to step 3's interface.
6. **Language packs.** They depend on G3, the Backlog from step 3, and G5's save decision, so they go last.

## 5. Mismatches between the docs and the code

None found within the audited scope. The plugin-family stage baseline, the read-log key, the `sfx_select` convention and the unchecked assets all match the code.
