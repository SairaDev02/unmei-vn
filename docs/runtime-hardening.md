---
title: Runtime hardening — save, menu, audio, diagnostics and presentation contracts
summary: The player-runtime contracts — save slots, menu overlay, audio settings, player comfort, failure posture, read log, Monitor, text skip and presentation.
keywords:
  - save-slots
  - settings-file
  - audio
  - read-log
  - text-skip
  - monitor
  - failure-posture
  - presentation-surface
---
# Runtime hardening — build-ready spec

**Status (2026-10-16).** This document is the destination of the harden-the-runtime map, written as a plan and now largely built: **Save slots**, the **Menu overlay**, **Audio settings**, **Player comfort**, the **Runtime failure posture** and the **Provider seam** shipped on 2026-10-14; the **Read log** and **Text skip** shipped in the change that also built the [Timeline](title-and-menu.md#timeline); the **Monitor** shipped with its own map, after its mandatory source-safety prerequisite and across the following interaction, layout and acceptance work. All three architecture-deepening sections carried by the architecture map now have production implementations: [Presentation surfaces](#presentation-surfaces--decided-partition) was extracted, with presentation-state ownership tightened afterwards; [Menu and live-session transitions](#menu-and-live-session-transitions--ownership-design) was extracted, and [Reading anchor](#reading-anchor--decided-seam-2026-10-13) too. Sections are appended by the map's work items as they resolve; each carries its own status marker, and amendments remain dated and in place.

**Built (2026-10-15)**: [Save restoration — one semantic validity owner](#save-restoration--one-semantic-validity-owner-2026-10-14), decided on the save map. It deepens the existing saved-file restoration interface; direct `SaveFile::restore` now applies the shared semantic gate against the destination runner's actual pack before mutation.

**Feasibility accepted under the explicitly amended evidence bar (2026-10-17); separate extraction authorized after that acceptance:** [Shipping composition — proof-gated shared owner](#shipping-composition--proof-gated-shared-owner), selected during the save map's design work. The [fully-proven outcome](#bounded-feasibility-outcome--fully-proven-2026-10-15) and the [feasibility-unmet outcome](#bounded-feasibility-outcome--feasibility-unmet-2026-10-17) remain historical. The [maintainer-approved evidence-reconciliation amendment](#maintainer-approved-f-evidence-reconciliation-2026-10-17) explicitly changes the acceptance contract; it does not invent missing observations. The production owner is now implemented in `app/src/composition.rs` and consumed by the real binary and strict external gate; the extraction's evidence record explicitly states the status of fresh independent delivery validation.

Sections so far:

- **Save slots** — settled 2026-09-28.
- **Menu overlay** — settled 2026-09-28; prototype-validated, layout B, the card grid. Its layout is superseded 2026-11-14 by the contents page of [Default look](title-and-menu.md#default-look).
- **Audio settings** — settled 2026-09-28.
- **Player comfort** — settled.
- **Runtime failure posture** — settled.
- **Provider seam** — settled.
- **Read log** — settled.
- **Text skip** — settled.
- **Monitor** — settled with its own map; dev mode, the log stream behind its mandatory source-safety prerequisite, the interaction model confirmed by the maintainer, and the layout (prototype-validated; variant A, the dialogue-reserved bottom drawer).

---

## Save slots

Settled 2026-09-28. Design record: [ADR 0006](adr/0006-save-slots-on-disk-format-and-version-policy.md). Bevy survey (2026-09-28): in Bevy 0.19 no serialization surface can write a plain serde struct without reflect derives or ECS residency, so the save path stays Bevy-free. **Built (2026-10-14)** — shipped.

### 1. Files on disk

Four sibling files beside the pack directory, in `Slot::ALL` order:

| Slot | Path | Written by |
| --- | --- | --- |
| `Manual(1)` | `<pack-dir>.slot1.save.ron` | the player |
| `Manual(2)` | `<pack-dir>.slot2.save.ron` | the player |
| `Manual(3)` | `<pack-dir>.slot3.save.ron` | the player |
| `Autosave` | `<pack-dir>.autosave.save.ron` | the game |

- RON, one `SaveFile` per file; siblings of the pack, so the pack directory stays pristine and portable — the `docs/slice-save-audio.md` §1 posture, kept.
- Each write is atomic (temp sibling + rename), exactly as `write_save` does today, and the temp name derives from the slot path so two slots never share one.
- 1-indexed, and the extension stays `.save.ron`, so a slot file is recognisable beside the pack.

### 2. Engine API (all in `engine/src/save.rs`, Bevy-free)

```rust
pub const SAVE_VERSION: u32 = 2;

/// Manual accepts 1..=3; `Slot::ALL` is the canonical list and menu order.
pub enum Slot { Manual(u8), Autosave }
impl Slot { pub const ALL: [Slot; 4]; }

pub fn save_path_for(pack: &Pack, slot: Slot) -> PathBuf;
/// The slice's single `<pack-dir>.save.ron` — the legacy file (see §5).
pub fn legacy_save_path_for(pack: &Pack) -> PathBuf;

pub fn list_slots(pack: &Pack) -> [SlotInfo; 4];
pub fn read_slot(pack: &Pack, slot: Slot) -> Result<SaveFile, SaveError>;
pub fn write_slot(pack: &Pack, slot: Slot, save: &SaveFile) -> Result<(), SaveError>;

pub struct SlotInfo { pub slot: Slot, pub status: SlotStatus }
pub enum SlotStatus {
    Empty,
    Ok { scene_id: String, saved_at: Option<u64> },
    Unreadable { reason: String }, // SaveError's display text
}
```

- `SaveFile` gains `#[serde(default)] pub saved_at: Option<u64>` — Unix epoch **seconds**.
- `SaveFile::capture` stays pure and leaves `saved_at: None`; the **app stamps** the current epoch on the captured value immediately before every `write_slot` (manual and autosave alike), so the engine's capture path stays deterministic under test.
- `RunOutcome` gains `pub entered_scene: Option<String>`, set only when a `jump` lands in a **different** scene file. A local-label jump, a same-scene target, and a jump whose target is the current scene's own id all leave it `None`.
- `write_slot` / `read_slot` delegate to the existing path-taking `write_save` / `read_save`; the slot functions only add naming and classification.

### 3. Version policy

- Accept `1 <= version <= SAVE_VERSION`; reject `version > SAVE_VERSION` (`SaveError::Version`); `version == 0` is unreadable.
- Convention: **every field added to `SaveFile` after v1 carries `#[serde(default)]`**, so older files keep parsing. A v1 file loads with `saved_at: None`.
- A slot is `Unreadable` when its bytes fail to decode, its version is newer than this build, its IO fails, or its `scene_id` is absent from the loaded pack. The [runtime failure posture](#runtime-failure-posture) §7 tightened this with cheap **semantic checks**: a slot is also `Unreadable` when `pc` is outside its scene's lines, when `pc == 0` and a wait is parked, when the state map holds a key `state.ron` does not declare, or when the parked wait's kind does not match the line at `pc - 1` — because `restore_position` validates only the scene id, so a decodable slot from an earlier revision of the pack, or one copied in from another, would otherwise restore a position that no longer describes the script. No new field, no version bump. `list_slots` classifies it up front through `read_slot`. Classification and restoration share this rule for the same unchanged saved value against the same loaded pack, not across later file edits or another destination pack. Direct `SaveFile::restore` applies the shared gate against the destination runner's actual pack before mutation (the [restoration amendment](#save-restoration--one-semantic-validity-owner-2026-10-14) below owns that design). Current production disk-slot loads already pass through `read_slot`.
- An unreadable slot is **never auto-deleted**, refuses to load with a message, and can still be overwritten by a new save.

### 4. Slot semantics and autosave

- All four slots hold the same `SaveFile` — state map, position (scene + `pc` + parked wait), staging snapshot, BGM id. They differ only in **who writes them**.
- The three manual slots are the only save and import targets; the **autosave is load-only**.
- The game writes the autosave when either:
  - a scene **crossing** happened (`RunOutcome.entered_scene.is_some()`), or
  - a player choice resolved, or a decision resolved **from a model answer** (after the follow-up `run()` has applied).
- A decision that resolved via its **author fallback** (API failure, timeout, unusable answer) advances the script but does **not** write the autosave — a transient outage must not churn the checkpoint, and the next crossing or choice resolve captures the position.
- The app applies the outcome's `staged` commands to its `StagingState` **before** capturing, so the autosave's snapshot matches what is on screen.
- **The boot-time entry scene is naturally exempt.** The first `run()` at setup crosses no scene, so `entered_scene` is `None` and a launch writes nothing. Launching and quitting — or even advancing a few say-lines and quitting — can never clobber the checkpoint.
- **Loading never autosaves.** A load restores a position that already lives in a slot, so the act of loading writes no autosave; the next autosave is the next crossing or resolve.
- An autosave write failure is non-fatal: the game continues, and the player is told with a **Notice** ([§ Runtime failure posture](#runtime-failure-posture) row 15) rather than left unaware that the checkpoint did not land. A failed manual save notices the same way (row 14). The general failure posture lives in [§ Runtime failure posture](#runtime-failure-posture).

### 5. Legacy single save

- Detect the slice's `<pack-dir>.save.ron`.
- If it decodes, has an accepted version (`1..=SAVE_VERSION`), **and** its scene exists in the pack, the overlay offers a one-time **Legacy save** entry. Eligibility deliberately does **not** include the slot's saved-position semantic checks: an eligible legacy file may import into a slot that then classifies as unreadable. This clarifies the earlier ambiguous “parses under §3” wording without changing eligibility, and the [restoration decision](#save-restoration--one-semantic-validity-owner-2026-10-14) confirms it. Its placement is the overlay's call.
- Selecting it: choose a manual slot (1–3), write the legacy `SaveFile` there (keeping its original `saved_at`, usually `None`), then rename the legacy file to `<pack-dir>.save.ron.imported`. The rename is what retires the entry — no marker file, and nothing is deleted.
- If the legacy file fails to parse, is newer than this build, or names a scene the pack lacks: **hide the entry and leave the file untouched**. It is a pre-slots dev artifact, not player data — deliberately unlike an unreadable slot, which is shown ready to overwrite.
- The import overwrites the chosen manual slot; any confirmation is the overlay's concern.

### 6. Overlay contract (for the menu overlay)

`list_slots` returns `[SlotInfo; 4]` in `Slot::ALL` order — `Manual(1)`, `Manual(2)`, `Manual(3)`, `Autosave` — which is everything the overlay needs to render four rows with no further engine calls:

- `Empty` → an empty row; a save action is offered on manual rows, load is disabled.
- `Ok { scene_id, saved_at }` → the row shows the scene id and the app-formatted local time (`—` when `saved_at` is `None`, e.g. an imported v1 save); load is enabled. Formatting happens app-side from the epoch integer, so the engine takes no date dependency.
- `Unreadable { reason }` → the row shows the reason; load is disabled; overwrite (manual rows only) is still allowed.
- Plus the optional one-time **Legacy save** entry from §5.

Exact layout, ordering, keybinds and pause semantics are the menu-overlay prototype's.

### 7. Validation rules (the build's test list)

- Each `Slot` maps to its distinct sibling path, and `Slot::ALL` is a stable order.
- `list_slots` on a save-free pack → four `Empty`.
- Round trip: `write_slot(Manual(2))` → `list_slots()[1]` is `Ok` with the scene id and the stamped time; the other three slots are untouched.
- A v1 RON file (no `saved_at`) → `Ok { saved_at: None }`.
- `version = SAVE_VERSION + 1` → `Unreadable`; truncated/garbage bytes → `Unreadable`; a `scene_id` absent from the pack → `Unreadable`.
- A slot whose `pc` is past the scene's last line, whose `pc == 0` carries a parked wait, whose state map holds a key `state.ron` does not declare, or whose parked wait does not match the line at `pc - 1` → `Unreadable`, preserved, and still overwritable (the [runtime failure posture's](#runtime-failure-posture) semantic checks).
- `list_slots` and `read_slot` agree for unchanged files against the same loaded pack. For a decodable saved value, version/position acceptance must also agree with restoration into a runner holding that same pack; an earlier `Ok` is not authorization after the value, file or destination changes. Direct restoration applies the same shared rule against the destination runner's actual pack ([restoration amendment](#save-restoration--one-semantic-validity-owner-2026-10-14)).
- An unreadable slot is preserved on disk and can be overwritten by `write_slot`.
- `RunOutcome.entered_scene`: `Some` on a cross-scene jump; `None` on a local-label jump and on the initial run.
- A boot `run()` writes no slot; a crossing, a choice resolve and a model-answered decision resolve each write the autosave; a fallback-resolved decision and a load write nothing.
- Legacy import writes the target slot and renames the file; a second pass finds neither the entry nor an importable file; an unreadable legacy file is neither shown nor touched.

### Save restoration — one semantic validity owner (2026-10-14)

Settled 2026-10-14 as a child of the save map: three design rounds (Q1–Q6) and explicit complete-design confirmation (Q7). **Built (2026-10-15)**: the interface below is now the production body of `SaveFile::restore`. The design interview itself was plan-only and ran no production implementation or Rust validation; its source baseline was the then-current local `HEAD`, where `read_slot` invoked private `validate_position` while `SaveFile::restore` checked version and delegated to raw runner repositioning. Production disk-slot loads already used `read_slot`; this was a restoration-interface guarantee gap, not a demonstrated ordinary disk-load failure.

#### Interface, depth and implementation ownership

Deepen the existing Bevy-free `engine/src/save.rs`; no new module or public surface. Keep the consuming interface:

```rust
pub fn restore(self, runner: &mut ScriptRunner) -> Result<StagingState, SaveError>
```

The build replaces its standalone version check with the existing private rule, before any mutation:

```rust
// The production body: the gate runs against the destination runner's pack
// before `restore_position` mutates anything.
validate_position(runner.pack(), &self)?;
runner.restore_position(self.scene_id, self.pc, self.state, self.wait)?;
Ok(self.staging)
```

`read_slot(pack, slot)` continues to invoke the same private validator, and `list_slots` continues to classify through `read_slot`. Version and saved-file acceptance stay save-owned; the runner does not acquire persistent-save policy. The in-process dependencies are the concrete `Pack`, `ScriptRunner`, state and wait; slot IO keeps the existing real filesystem operations. No trait, adapter framework, public validate-first protocol or helper-only test door is introduced.

The leverage is an unavoidable check at every `SaveFile::restore`, with no caller-side validation recipe to remember. The implementation stays local to the existing rule and consuming operation; a behavior-preserving extraction is unnecessary because the shared rule already exists.

#### Destination, refusal and compatibility contract

- **Check the actual destination every time.** Derive the pack from `runner.pack()`, never from an origin, earlier classification or caller-supplied second pack. A save accepted against pack A may fail against runner B even with the same scene id. `SaveFile` remains publicly constructible and mutable, so a value altered after a successful read must also be checked again.
- **Read-only validation before mutation.** Every refusal leaves the destination's scene, `pc`, state and parked wait unchanged and returns no staging snapshot. The existing raw reposition operation's mutation-free unknown-scene refusal remains intact. The engine does not own app staging, cancellation or replay; the app assigns returned staging only on success.
- **Same consuming ownership.** Failure still consumes `SaveFile`; no rejected-save return protocol or clone-on-restore is added. A caller needing retry retains or re-reads its own value. Success returns the exact saved staging and preserves existing resume semantics: say/choice waits are restored, and the app re-fires a restored decision wait.
- **Same rule, order and errors.** Keep `validate_position` unchanged: accepted version → existing scene → `pc <= scene.lines.len()` → declared state keys → no wait at `pc == 0` → wait kind matching the line at `pc - 1`. Keep `SaveError::Version` (including zero), `SaveError::Runner(RunnerError::UnknownScene)` and semantic reasons on `SaveError::Decode`, with existing wording and check order. There is no stable choice of first offending key among multiple undeclared keys in a hash map.
- **Do not infer stronger validity.** `pc == 0` with no wait and `pc == scene length` remain permitted when the other checks pass. No missing-key completeness, state-value-kind, finite-number, exact wait-payload, fingerprint or pack-identity check is added. A foreign or edited save that still passes these structural checks remains acceptable.
- **Qualified agreement.** Classification/read and restoration use the same acceptance rule for an unchanged value and the same loaded pack. Classification is not durable authorization across later file edits, changes to public fields or a different destination; IO/decode refusal is not a claim about an independently available in-memory value.

Accepted costs are repeated read/restore validation, consuming refusal and runtime rather than type-level enforcement. These buy the guarantee without changing caller signatures, serialization, versions or persistent layout.

#### Alternatives and independent paths

The design-it-twice comparison considered (A) the existing method with private shared checking, (B) an opaque checked-save or scoped destination capability and (C) a save-owned public `restore_into(runner, save)` with method delegation. B can be sound by binding to a destination or revalidating, but adds a type/lifetime/use protocol without a concrete caller need; a check against A alone cannot authorize B. C is sound with delegation, but adds a second public operation without a stronger guarantee than the existing destination-taking method. Choose A; a new module is not mandatory to earn depth.

Raw `ScriptRunner::restore_position` remains a separate operation with its current contract. Timeline keeps scene-from-top repositioning with current state/staging; no save validation is imposed on it. Pack-free `from_ron`/`read_save`, encoding and `write_save`/`write_slot` gain no validation policy. Legacy eligibility remains decode + accepted version + existing scene; `import_legacy` still writes the manual slot before renaming the source, preserves its stamp and rejects autosave targets. An eligible but semantically invalid legacy file can still become an unreadable imported slot. Nothing is auto-deleted.

#### Composition with menu/live-session ownership

Compose with [ADR 0012](adr/0012-menu-and-live-session-transition-ownership.md), not absorb it. The restore seam — `apply_loaded_save` at the time of this decision, later extracted to `SessionTransitions::restore_loaded` — calls `SaveFile::restore`; success assigns staging and performs the existing app-owned cancellation/reset/replay recipe. Refusal performs no successful-replacement cleanup and retains the existing row-17 reporting. Menu outcomes, failure texts/levels, read-log knowledge and presentation ownership stay with their accepted owners.

This engine handoff is independent of the separately sequenced false-success/remaining-dwell repairs and extraction in the menu/live-session design: it may land before or after them. Their repair prerequisites remain binding for their extraction; whichever implementation lands later must preserve the engine call and independent app witnesses. Do not invent a planning or implementation blocker between these slices, bundle their repairs here or claim engine tests prove menu behavior.

#### Validation contract for the later build

Tests cross the production restoration interface, not `validate_position` directly. Prefer extending existing witnesses over duplicate tests or a blanket rewrite.

- **Successful restoration:** retain current/v1 compatibility, say/choice/decision waits, state carry-forward and conditional resume; assert the exact returned staging snapshot and valid `pc` boundaries (zero without a wait, and scene length). Do not strengthen matching to exact authored wait payloads.
- **Independent direct refusals:** construct saves with each existing semantic defect (past-end `pc`, wait at zero, undeclared state key, mismatched wait kind), then call `restore` without relying on slot-read refusal. Assert the expected error category/reason and preservation of all four observable runner fields. Extend `restore_rejects_an_unknown_scene` and version refusal coverage, including zero and newer versions, with the same preservation assertions.
- **Stale acceptance regressions:** mutate a save's public position after `read_slot` succeeds, then require direct restore refusal. Separately accept a value against pack A and attempt restoration into pack B with the same scene id but incompatible content; require refusal and unchanged B. No fingerprint or transferable token is needed.
- **Agreement and disk behavior:** check same-pack, unchanged-value version/position agreement through `list_slots`, `read_slot` and direct restore. Retain the independent slot classification, unreadable-file preservation and manual overwrite witnesses; do not replace their IO assertions with helper-only validation tests.
- **Deliberately separate paths:** add one narrow regression that a decode/version/scene-eligible but semantically invalid legacy value remains importable, is written/renamed and then classifies unreadable. Retain existing legacy source preservation, write-before-rename failure, stamp and autosave-target coverage. Preserve raw runner and real app Timeline witnesses rather than rewriting them to use saved-file restoration.
- **App witnesses remain independent:** retain `a_load_refused_at_restore_time_notices_row_17`, `a_load_refused_under_the_overlay_notices_row_17`, `a_load_clears_the_held_and_pending_decision_state`, real future-drop/held-answer/decision-refire coverage and the menu/live-session session/Timeline validation contract. This engine slice neither migrates its two one-shot closures nor adds a broad app rewrite. Recheck any hand-built saved positions against the existing rule instead of weakening it to accommodate fixtures.
- **Commands:** start with `cargo test --locked -p unmei-engine`. Run applicable app regression witnesses (for example `cargo test --locked -p unmei-app a_load_refused` and `cargo test --locked -p unmei-app a_load_clears_the_held_and_pending_decision_state`) without claiming they replace the wider integration suite. Before merge run `cargo fmt --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace`, `python3 scripts/check-invariants.py`, its `--self-test`. Follow the repository's testing guidance in [the testing page](testing.md); no provider, boot or rendered acceptance is inferred from engine/headless tests.

#### Smallest implementation handoff and record

Implemented in the 2026-10-15 build, the implementation work carried by this design and validation list when the save map was decomposed. The build reverified source/borrow sequencing at its HEAD, established the missing direct-restoration regressions (observed failing against the old version-only `restore`), replaced the version-only check with the private destination validation, and updated the Rust documentation of `restore`, `read_slot` and the shared validator (including the same-pack/unchanged-value qualifications). The validator's rule body, raw runner implementation and production app policy are unchanged. There is no behavior-preserving extraction step to manufacture; any later file split is separately justified work.

The dated amendment is the owning record. No ADR or glossary change: no new domain concept or hard-to-reverse trade-off is introduced. This section clarifies §3/§5/§7 prose, and the build shipped the gate. Explicit exclusions: new modules/public operations/types/traits, raw-runner tightening, legacy eligibility changes, write/decode validation, new validity rules, format/version/path changes, slot deletion, app contract repairs/navigation/presentation changes, broad test rewrites and journey-page updates. Remaining work is closing the save map; structurally valid foreign/edited saves remain an accepted limitation, not a newly discovered defect.

### Out of scope for this section

Overlay visuals and interaction ([menu overlay](#menu-overlay)); audio and comfort settings ([audio settings](#audio-settings), [player comfort](#player-comfort)); the general failure posture ([runtime failure posture](#runtime-failure-posture)); slot deletion UI; cloud/portable save sharing.

---

## Menu overlay

Settled 2026-09-28. Prototype-validated: the clickable stub lived on a throwaway prototype branch at `app/prototype/menu-overlay.html` (out of `main`, per the prototype practice), with three layouts over one interaction model. The **card grid** (variant B) won; the other two remain on that branch as the primary source. **Built** — shipped 2026-10-14, with the Gallery view added 2026-10-05 and the Timeline view added 2026-10-08. The faces every surface here renders with are [Type system](title-and-menu.md#type-system)'s, not re-decided in this section.

### 1. Shape

*(Amended 2026-11-14: the layout below — rail, top bar, card grid and the dimmed freeze — is superseded by the contents page of [Default look](title-and-menu.md#default-look) §4. The overlay's contents, actions and pause doctrine stand.)*

One overlay, opened with **Esc**, over a dimmed, blurred freeze of the game. It is the single pause surface for the hardening effort: save/load **and** settings live here, and the gallery joins them ([Gallery](title-and-menu.md#gallery)).

```
┌──────────────┬───────────────────────────────────────────────┐
│ unmei demo   │  [ Save | Load ] Timeline Gallery ⚙ Settings  │
│ scene: …     │  ┌────────────┐ ┌────────────┐                │
│              │  │ Slot 1     │ │ Slot 2     │                │
│              │  │ reunion    │ │ — empty —  │                │
│              │  │ 12 Oct …   │ │ Save here  │                │
│              │  └────────────┘ └────────────┘                │
│              │  ┌────────────┐ ┌────────────┐                │
│              │  │ Slot 3     │ │ Autosave   │                │
│              │  │ Unreadable │ │ first_…    │                │
│              │  │ Overwrite  │ │ Load-only  │                │
│              │  └────────────┘ └────────────┘                │
│  ▶ Resume    │  ┌── Legacy save — Import… ───────────────┐   │
└──────────────┴──┴────────────────────────────────────────┴───┘
```

- **Left rail** — pack name, the current scene id, and a **Resume** button (Esc).
- **Top bar** — a **Save / Load** segmented toggle, **Timeline**, **Gallery** and **Settings** controls: Timeline sits immediately before Gallery, Gallery immediately before Settings. The entry controls render only while the slot grid is up, and `Tab` cycles Save → Load → Timeline → Gallery → Settings → Save in that view (Save → Load in the sub-views). *(Amended 2026-10-05 and 2026-10-08: Timeline adds its overlay-only door.)*
- **Grid** — four cards in `Slot::ALL` order (`Slot 1`–`Slot 3`, `Autosave`).
- **Legacy card** — full-width, dashed, below the grid, **save mode only** (see §4).
- **Settings view** — replaces the grid in the same overlay; the header swaps to `Settings` with a back-to-slots control.
- **Gallery view** *(added 2026-10-05)*: replaces the grid the same way, headed `Gallery` with the door-aware back control (`← Back to slots` / `← Back to title`); the surface's behavior is [Gallery](title-and-menu.md#gallery)'s.
- **Timeline view** *(added 2026-10-08)*: a fifth `MenuView` beside `Title | Slots | Settings | Gallery`; replaces the grid, headed `Timeline` with `← Back to slots` and Resume. It has no title door; behaviour and entries are [Timeline](title-and-menu.md#timeline)'s. The door stays enabled for packs with no declared scene order and shows that section's empty state.

### 2. Card contents

Each card shows exactly what `list_slots` yields ([Save slots](#save-slots) §6) — no playtime or thumbnail, which the engine does not carry:

| Field | Source |
| --- | --- |
| Slot label + kind | `Slot::ALL` order; the autosave is marked game-written |
| Status chip | `Ok` / `Empty` / `Unreadable` |
| Scene id | `SlotInfo::Ok { scene_id }`; unreadable → the reason; empty → `—` |
| Saved-at | app-formatted local time from `saved_at`; `—` when `None`; empty → “no save yet” |
| Action | contextual — see §3 |

### 3. Selectable when

- **Empty** (manual) → `Save here`, immediate; load disabled.
- **Ok** (manual) → `Overwrite` in save mode, `Load` in load mode.
- **Unreadable** (manual) → `Overwrite` only; load disabled and the reason shown; the file is never auto-deleted.
- **Autosave** → `Load` in load mode; in save mode the action is a disabled `Load-only`.
- **Overwrite** and **Load** each ask once. Save-over-empty, and the legacy slot-pick, add no second prompt.
- Metadata is read once per open via `list_slots`; the overlay makes no further engine calls until an action is taken, then re-reads `list_slots` after a save or import to refresh the cards.

### 4. Legacy entry

The one-time **Legacy save** card appears at the bottom of the grid in **save mode only**, and only while the legacy file is importable ([Save slots](#save-slots) §5). `Import…` opens a slot picker for `Manual(1..=3)`; occupied targets are labelled `(overwrite)`. Choosing one runs ADR 0006's settled flow — write the slot, rename the file aside — and retires the card. This section fixes only **where the affordance lives**; the flow is not re-decided here.

### 5. Settings

Both settings groups live in the overlay's **Settings view**:

- **Audio** — Master, Music, SFX, Mute ([audio settings](#audio-settings)).
- **Play** — Text speed, Auto, Fullscreen; skip is not a setting — its two in-play controls are [Text skip](#text-skip)'s *(amended 2026-10-08)*.

Placement is this section's decision; the **contents** are not. Ranges, defaults, the settings file, persistence and the reveal/auto semantics are settled by [audio settings](#audio-settings) and [player comfort](#player-comfort), which inherit this surface rather than inventing a second one. Settings are pure `app/` presentation state (fullscreen is Bevy window config) — nothing crosses into `engine/`.

*(Amended 2026-10-08: Timeline is a separate overlay view, not a Settings group or control; neither Settings door opens it. The Settings contents and walk are unchanged.)*

### 6. Interaction model

**Esc is a presentation pause.** It never advances the script, never cancels an in-flight Decisions request, and never drops a player choice. Concretely:

- **Opens from any parked state** — say-wait, choice-wait, decision-wait, an in-flight request, end of scene. Opening changes neither the runner's position nor the request.
- **One key, four moments.** Esc cancels an open confirmation first; then backs out of a sub-view one level — Settings or Timeline to the slot grid, the gallery's still to its grid and the gallery's grid to its door ([Gallery](title-and-menu.md#gallery) §3; [Timeline](title-and-menu.md#timeline) §1, added 2026-10-08) — then closes the overlay (Resume). It also closes while a request is in flight and while an answer is held.
- **A completed answer is held, not applied, while the overlay is open.** The in-flight request keeps running; when it answers, the app buffers the resolution and the script does **not** advance until resume. On resume the buffered answer is applied and the runner parks at the next wait — the world never moves under the player.
- **Destructive position replacement cancels the old request and discards a held answer.** A load restores the slot, and a parked decision **re-runs the `decide` line** from restored state ([Save slots](#save-slots) §4). New Game and a confirmed Timeline jump also cancel/discard; browsing Timeline never does. *(Amended 2026-10-08: this supersedes “Loading is the one action that cancels the request”; [Title](title-and-menu.md#title) §3 and [Timeline](title-and-menu.md#timeline) §4 own their replacement rules.)*
- **Manual save is allowed in any parked state**, including with a request in flight: it captures the parked decision-wait, which re-fires on load. The autosave is unaffected by the overlay — crossings and resolves keep writing it.
- **The overlay itself never writes the autosave**, except through a held answer: a held **model** answer applied on resume counts as a model-answered resolve and writes the autosave exactly once, while a held **fallback** resolution writes nothing ([Save slots](#save-slots) §4). Opening or closing with no answer writes nothing.

**Input.** *(Amended 2026-10-11: the following underlying menu inputs are inert while [Monitor → Interaction](#interaction) owns input, on its transition frames and for quarantined holds afterward. Esc dismisses Monitor first, without cancelling/backing out/resuming this menu; the existing presentation pause remains in force.)* *(Amended 2026-11-14: the grid walk is replaced by [Default look](title-and-menu.md#default-look) §4's ledger walk.)* Mouse and keyboard are equal citizens: click any card or control; `↑`/`↓`/`←`/`→` move focus across the 2×2 grid, `Tab` cycles the top-bar controls, `Enter`/`Space` activates, `Esc` backs out, digits `1`–`3` focus a manual slot. Focus is always visible. Gamepad and rebinding are **not** covered; they are out of scope for this section.

The **Settings view deviates deliberately**: there, `↑`/`↓` move between controls and `←`/`→` adjust the focused slider, so the grid's focus rule does not also have to be a slider's value rule. [Audio settings](#audio-settings) §5 fixes that model for the audio group; [player comfort](#player-comfort) inherits it.

### 7. Validation rules (the build's test list)

- Esc opens from each parked wait kind and leaves `runner.wait()` and `pc` unchanged.
- Opening then closing with no action leaves the position byte-identical.
- A model answer arriving while the overlay is open is held: `pc`/wait unchanged until close; on close the answer is applied and the runner parks at the next wait.
- Resuming with a held **model** answer writes the autosave exactly once; resuming with a held **fallback** writes none; opening/closing with no answer writes none.
- Load cancels the pending task and clears any held answer; a decision-wait slot re-fires the `decide` line.
- Save-over-empty writes immediately; save-over-occupied/unreadable confirms, and cancelling leaves the file unchanged.
- The autosave card offers no save action in save mode.
- An unreadable slot refuses to load and can still be overwritten.
- The legacy card appears only in save mode and only while the legacy file is importable; importing targets a manual slot and retires it.
- Settings changes are app-side only — no engine call.

### Amendments

**2026-10-01.** The **title screen** ([Title](title-and-menu.md#title)) is a second pause surface in front of this one: a pre-play boot gate with the same freeze doctrine and the same `menu_closed` predicate, and `Esc` is inert while it is up. §1's “the single pause surface for the hardening effort” stands corrected by this note; nothing else in this section's rules changes.

**2026-10-05 (gallery).** The top bar gains its `Gallery` control and §1 now names the gallery view alongside Settings; the surface's behavior is [Gallery](title-and-menu.md#gallery). §6's second `Esc` moment now reads “one level” and names the gallery's back-out — the count stays four. §5's group list is untouched: the gallery is not reachable from Settings.

**2026-10-08 (Timeline).** This amendment adds the fifth view and overlay-only top-bar door before Gallery (§1), explicitly leaves Settings unchanged and without a Timeline door (§5), and adds Timeline's one-level Esc back-out and confirmed-jump cancellation rule (§6). The slot-grid Tab cycle gains Timeline; sub-views retain Save → Load. The surface and its validation rules live in [Timeline](title-and-menu.md#timeline).

**2026-11-14 (default look).** §1's layout and §6's grid walk are superseded by the contents page of [Default look](title-and-menu.md#default-look) §4, and `Exit to title` moves from the Settings rail to the page's foot. The contents, actions, confirms and pause doctrine are unchanged.

**2026-10-08 (text skip).** §5's Play row drops “skip is out”: skip is not a preference and gains no Settings row — its `Tab` toggle and held `Ctrl` are [Text skip](#text-skip)'s. Nothing else in this section changes.

**2026-10-14 (status).** The settled banner above now reads **Built** — the section shipped on 2026-10-14. No rule in this section changed.

### Out of scope for this section

The comfort settings themselves ([player comfort](#player-comfort); audio settings are settled [above](#audio-settings)); the general failure posture ([runtime failure posture](#runtime-failure-posture)); quit-to-desktop; slot deletion; gamepad support and input rebinding; cloud/portable saves.

---

## Audio settings

Settled 2026-09-28. Design record: [ADR 0007](adr/0007-settings-file-beside-the-pack.md). **Built (2026-10-14)** — shipped. This section also fixes the **settings-file pattern** that [player comfort](#player-comfort) inherits, as the map's settings-surface decision requires.

### 1. File and schema

- **`<pack-dir>.settings.ron`** — a sibling of the pack directory, named after it, exactly like the slot files ([Save slots](#save-slots) §1). One file per pack. `*.settings.ron` joins `.gitignore`'s `*.save.ron`.
- RON, with `#[serde(default)]` on every field and unknown keys ignored. There is **no version field** (§3).
- A **sectioned root**, so each settings ticket owns a struct instead of inventing a second file:

```ron
(
    audio: (
        master: 1.0,
        music: 1.0,
        sfx: 1.0,
        muted: false,
    ),
)
```

- The `play` section is declared now but carries `#[serde(default, skip_serializing_if = ...)]`, so an audio-only file is just `(audio: (...))` and [player comfort](#player-comfort) starts emitting the key when it adds its first non-default field.
- Owned by **`app/src/settings.rs`**: the `Settings` types, path derivation from `pack_root`, load/save, and the read posture. The write idiom and the sibling naming rule are shared with the other sidecars ([`engine/src/sidecar.rs`](../engine/src/sidecar.rs)); nothing about a preference the headless runner never reads crosses into `engine/` ([menu overlay](#menu-overlay) §5). `unmei-app` gains direct `serde` + `ron` dependencies (already in the lock file via the engine).
- Read at startup from the app's `pack_root` (`main.rs` canonicalises the positional `PACK_DIR` best-effort; amended 2026-10-11), and applied to the audio channels before the first scene plays; the pack load is unaffected by a missing or unreadable settings file.

### 2. Volumes, ranges and defaults

| Control | Field | Kind | Range | Default |
| --- | --- | --- | --- | --- |
| Master | `audio.master` | `f32` | `0.0..=1.0` | `1.0` |
| Music | `audio.music` | `f32` | `0.0..=1.0` | `1.0` |
| SFX | `audio.sfx` | `f32` | `0.0..=1.0` | `1.0` |
| Mute | `audio.muted` | `bool` | — | `false` |

- Values are **linear amplitudes**, not decibels and not percent; the UI displays percent and the file stores the amplitude (§5).
- Defaults are today's kira identity, so a first launch sounds exactly as the slice does now. A real audio pass is a separate, still-deferred trajectory and may re-tune them.
- `sfx` covers the whole `Sfx` channel — today the pack-optional `sfx_select` convention, and any future one-shot.
- **Loudness is a preference of the player-pack pair.** It lives in this file and never in `SaveFile`, so loading a slot never changes the mix.

### 3. No version field

- Unlike `SaveFile` ([ADR 0006](adr/0006-save-slots-on-disk-format-and-version-policy.md)), the settings file carries no `version`. Every field is `#[serde(default)]` and unknown keys are ignored, so files from any build parse: a key written by a newer build is read and then *dropped by the next write*.
- A **missing key** falls back to that field's default while its siblings keep their stored values. A file that does not parse at all (malformed RON, a value of the wrong type) has no partial recovery: **all** fields fall back to defaults, with a warning in the log, and the file is left on disk untouched.
- Rationale: a save is unrecoverable player data and pays for a version floor; a preference is a cheap, reproducible value the player restores by dragging a slider. A version field's only outcome here would be "your sliders moved back".
- Accepted cost, recorded in ADR 0007: an older build rewriting a newer build's file loses the unknown keys.

### 4. Mapping to kira

- Effective amplitude per channel: `if muted { 0.0 } else { master * channel }` — master is arithmetic over the two app channels, not a routed parent.
- Amplitude → volume: `Decibels(20.0 * a.log10())`, with `a == 0.0` (and any non-finite value) special-cased to `Decibels::SILENCE`, because `log10(0)` is `-inf`.
- Applied with `AudioChannel::set_volume` on the existing `Bgm` and `Sfx` channels. That API re-applies the level to sounds already playing *and* remembers it for the next `play`, so a change is audible immediately and needs no stop/play — the loop restarts only on a load, as today.
- Un-tweened, the call rides kira's default 10 ms linear ramp: no click, and no explicit fade is specified.
- kira's `MainTrack` is a **sibling** channel to `Bgm`/`Sfx`, and the app routes nothing through it. Master therefore does not govern the default `Audio` resource; anything ever played there would bypass the mix. Recorded as a constraint, not fixed here.

### 5. Settings surface (for the menu overlay's Settings view)

- **Audio group, in order:** Master, Music, SFX sliders and a Mute toggle — loudest/most general first, mute last.
- Sliders read **0–100% in 5% steps**: one step per key press, mouse drag snapped to the same grid, so both input paths share one value space. The file always stores the linear value.
- **Keyboard, Settings view only:** `↑`/`↓` move between controls, `←`/`→` adjust the focused slider, `Tab` cycles the top bar, `Enter`/`Space` toggles mute, `Esc` backs out. This deliberately replaces the card grid's `←`/`→` focus rule inside this view; the grid itself keeps [menu overlay](#menu-overlay) §6's model.
- **While muted** the sliders stay visible and editable and the toggle reads ON; muting never rewrites a stored level, and unmuting restores exactly what was there.
- **Reset to defaults:** one control in the group, resetting the `audio` section only — `master`/`music`/`sfx` → `1.0`, `muted` → `false`. It asks once before applying (the overlay's confirm pattern, [menu overlay](#menu-overlay) §3), then applies and persists like any other change. It never deletes the file. [Player comfort](#player-comfort)'s group gets its own reset rather than reaching into this one.

### 6. Persistence

- Values apply **live** as a control moves; the file is written once when the Settings view is left, when the overlay closes, and on app exit if a change is still pending. One atomic write per flush (temp sibling + rename, as `write_save` does).
- Opening the Settings view or moving a slider writes nothing. A change never left behind — the app is killed before the flush — is lost; accepted for a preference file.
- Merely loading a file never rewrites it: an untouched session leaves the bytes (including any unknown keys) exactly as they were.

### 7. Missing, corrupt, out-of-range, unwritable

- **Missing** → defaults in memory; nothing is written until the first change, so a pristine checkout never gains a file.
- **Unparseable** → warning in the log, all-defaults in memory, file untouched on disk until a later change overwrites it. No backup copy and no notice in the UI — unlike an unreadable save slot, which is player data worth preserving loudly.
- **Out-of-range** (e.g. `music: 3.5`, `-1.0`) → clamped into `0.0..=1.0`; **non-finite** (`nan`, `inf`) → the field's default.
- **Unwritable** (read-only pack directory, IO error) → in-session values kept, no retry, and the player is told with a **Notice** ([§ Runtime failure posture](#runtime-failure-posture) row 20); the next successful flush rewrites the file. The log line carries the path and the IO error.

### 8. Validation rules (the build's test list)

- `settings_path_for(pack_root)` is the pack directory's sibling `<pack-dir>.settings.ron`, never a path inside the pack directory.
- A pack with no settings file: defaults in memory, no file created by loading it, and none created by merely opening the Settings view.
- Round trip: change all four controls, leave the view → one file holding the four `audio` values and **no `play` key**; reload in a fresh app → the same values.
- A file missing one key → that field's default while its siblings keep their stored values.
- Malformed RON, a wrong-typed value, and a non-bool `muted` → all defaults plus a logged warning, with the file bytes unmodified.
- Out-of-range values clamp; non-finite values take the default.
- Unknown keys are tolerated on read and dropped by the next write (asserted deliberately, per ADR 0007).
- Mapping: `master = 0.5, music = 0.5` with `muted = false` → 25% amplitude (≈ −12 dB); `music = 0.0` → silence; `muted = true` → silence whatever the stored levels hold.
- Mute preserves stored levels: mute → leave → unmute → the sliders are where they were and only `muted` changed in the file.
- Reset resets the `audio` section only, confirms once, and persists on leave; cancelling the confirm changes nothing.
- A volume change while BGM loops does not stop or restart the track, and the new level is what the next `play` uses.
- Loading a slot changes no channel volume and leaves the settings file byte-identical.
- A failed write keeps the in-session values, notices once ([§ Runtime failure posture](#runtime-failure-posture) row 20), and the next change attempts one write.
- The app-exit flush writes a pending change.

### Out of scope for this section

The voice channel and generalised DSL `sfx` events — ruled out in charting, not reopened; [player comfort](#player-comfort)'s `play` fields and that group's own reset; the general failure posture ([runtime failure posture](#runtime-failure-posture)); output-device selection, sample rate and device hot-plug; loudness normalisation or per-asset gain; mixes persisted in `SaveFile` or per slot; a mixer reachable outside the Esc overlay; cloud/portable preference sync.
---

## Player comfort

Settled 2026-09-28. **Built (2026-10-14)** — shipped. Inherits the settings-file pattern fixed by [audio settings](#audio-settings) and the menu surface fixed by [menu overlay](#menu-overlay) rather than inventing a second one; no design record of its own.

### 1. What ships

| Control | Verdict | Shape |
| --- | --- | --- |
| **Text speed** | in | a reveal whose pace is a player setting; **default Instant** = today's whole-line render |
| **Auto mode** | in | a toggle; advances say-lines after a fixed dwell; **off by default** |
| **Skip** | **in** *(amended 2026-10-08)* | two in-play controls, neither a preference: `Tab` toggles seen text, a held `Ctrl` forces — [Text skip](#text-skip) |
| **Fullscreen** | in | a toggle, persisted; **windowed by default** |

**Non-goals, recorded so the surface cannot creep:** font-size / legibility controls, key rebinding, gamepad support (already map fog), authored per-line timing in the DSL, remembered window size/position, and text-box styling. These four rows are the whole comfort surface; the bar is *a friend can play the demo pack comfortably*, not a full player build.

### 2. The `play` section

```ron
play: (
    text_speed: 0.0,   // seconds per character; 0.0 = Instant
    auto: false,
    fullscreen: false,
)
```

- The same file as [audio settings](#audio-settings) §1 — the pack-adjacent `<pack-dir>.settings.ron` — and the same rules: `#[serde(default)]` on every field, **the whole `play` section skipped while all default** (an audio-only file has no `play` key; the first non-default comfort value starts emitting it), unknown keys tolerated on read and dropped by the next write, **no version field**, one atomic temp-sibling-plus-rename write.
- Persistence follows [audio settings](#audio-settings) §6 exactly: applied live as a control moves, written once on leaving the Settings view, on overlay close, and on app exit if a change is still pending; merely reading a file never rewrites it.
- Missing, malformed, out-of-range and unwritable behave as [audio settings](#audio-settings) §7: a missing key falls back to its default while its siblings keep their values; malformed RON or a wrong-typed value means **all** defaults plus a logged warning with the bytes untouched; out-of-range clamps; non-finite takes the default; an unwritable file keeps the in-session values and notices at the flush ([§ Runtime failure posture](#runtime-failure-posture) row 20) if it still cannot be written. Unreadable comfort values never block the pack load.
- `text_speed` clamps to `0.0..=0.08` (the ladder's ends, §3).
- Owned by `app/src/settings.rs`; the `audio` group in the same file is untouched by any comfort change and vice versa.
- **Read once at startup, in `main.rs`, before `App::new()`** — `settings.rs` still owns the types, the path and load/save; `main.rs` calls its load, uses `play.fullscreen` when constructing the `Window`, and hands the loaded `Settings` to the plugin (which applies the audio levels before the first scene, per [audio settings](#audio-settings) §1). Nothing is re-read later. A load that wants to warn (a malformed value, an unwritable file) happens before `DefaultPlugins` installs `LogPlugin`, so the warning rides on the loaded `Settings` and is emitted by the plugin's startup system — [audio settings](#audio-settings) §7's logging still happens, one system later.

### 3. Text speed and the reveal

| Step | `text_speed` (s/char) | ≈ chars/second |
| --- | --- | --- |
| Instant *(default)* | `0.0` | whole line at once |
| Fast | `0.012` | 83 |
| Normal | `0.02` | 50 |
| Slow | `0.04` | 25 |
| Slowest | `0.08` | 12.5 |

- The file stores the **physical delay in seconds per character**, as the audio group stores amplitudes; the menu walks the named ladder. `0.0` is not "as fast as possible" but *no reveal at all*: the line is whole on the first frame, byte-identical to today's rendering.
- **Reveal scope: the say-line text only.** The `Speaker: ` prefix is complete from the first frame (the name plate is never half-drawn); `Narrator:` narration reveals like any say-line, since narration *is* a say-line; the `<npc_id> is deciding…` placeholder and the end-of-scene marker never reveal. Say text reaches the app already interpolated (`{var}` is resolved in the runner) and carries no markup, so slicing it is safe.
- **The reveal advances by Unicode scalar value** (`chars()`), so a line splits on real character boundaries, never mid-codepoint. A grapheme cluster (an emoji, a combining mark) can still split mid-cluster — accepted, since the alternative is a grapheme-segmentation dependency and the demo bar's text is Latin prose.
- **Advance contract** (unchanged for everything else): a press while a line is revealing **completes the line instantly** and is consumed — it never both completes and advances, so a player mashing the key cannot skip past a line. The next press advances. A left-click anywhere behaves exactly like the key. Choice-waits, decision-waits and end-of-scene are unaffected by the reveal.
  *(Amended 2026-10-08: the one exception is a skip step, which completes a reveal **and** advances in a single step — skip is not a press; see [Text skip](#text-skip) §2. The press contract above is unchanged for every press.)*
- **Re-park:** a slot loaded onto a say-wait re-reveals from the first character — the same code path as a fresh park, and the reveal reading of the slice's load rule "re-show the saved line, wait for advance again" ([`slice-save-audio.md`](slice-save-audio.md#load-semantics--exact-resume)).
- **Presentation pause:** the reveal clock is frozen while the Esc overlay is open. [Menu overlay](#menu-overlay) §6 pauses the presentation without touching the runner; this section reads the comfort timers as part of that presentation — a part-revealed line resumes where it stopped and never finishes under the overlay.
- **Recorded, unchanged quirk:** the UI sync compares whole `Wait::Say { speaker, text }` values, so two *consecutive identical* say-lines are indistinguishable to it and the second does not re-reveal. Pre-existing behaviour, not touched here.

### 4. Auto mode

- **Off by default**; `play.auto: true` turns it on, and the state is a preference, not save data — it survives a load and a choice pick.
- **Dwell: 2.0 s after the line is fully revealed** (immediately, when the line is Instant). It is a fixed constant — not scaled by line length, and not authored in the pack.
- A **manual advance** while the dwell is running restarts a full 2.0 s; a player clicking through is never raced into the next line. Completing a reveal with a press is **not** an advance: it ends the reveal, which is exactly when the dwell starts, so the line then gets its full 2.0 s.
- **Auto holds** at a choice-wait, a decision-wait and end-of-scene. It only ever calls the say-advance path: it never picks a choice, never resolves or cancels a decision, and never advances while a Decisions request is in flight.
- It pauses with the presentation: with the overlay open, no dwell fires and the timer does not accumulate. Closing resumes it.
- *(Amended 2026-11-14: the corner cue is replaced by the lit `Auto A` label of [Default look](title-and-menu.md#default-look) §1's quick row.)* **In-play:** `A` toggles auto. While it is on, a dim uppercase **`AUTO`** cue sits at the dialogue band's bottom-right (`bottom: 40px`, `right: 48px`, ≈50 % alpha) — absolutely positioned in an otherwise empty corner, so turning auto on moves no text. Neither `A` nor `F11` is an advance or a choice input: Space/Enter/left-click and 1–9 keep those, and both keys are inert at a choice-wait.
- The cue is the only in-play comfort indicator; fullscreen gets none.

### 5. Fullscreen

- One field, two states: `WindowMode::Windowed` (the `play.fullscreen: false` default, today's 1280×720 window) and `WindowMode::BorderlessFullscreen(MonitorSelection::Primary)`.
- **Exclusive fullscreen is not used** (`WindowMode::Fullscreen(_, VideoModeSelection)`): it changes video mode and refresh rate for a comfort toggle's worth of benefit.
- When the startup read says `true`, the window is **created** fullscreen — no windowed first frame.
- `F11` and the Settings view's switch toggle the same field; the switch reflects a hotkey toggle immediately, without reopening the overlay.
- The windowed size stays the fixed 1280×720 from `main.rs`; remembering size/position is a non-goal (§1).
- **The windowed minimum is 1280×720** *(added 2026-10-04)*: the window's `resize_constraints` floor is the reference size, so the OS refuses a smaller windowed resize and the layouts below 1280×720 are unsupported rather than responsive. Two recorded residuals: borderless fullscreen on a display smaller than 1280×720 still clips, and a compositor may ignore the constraint. Sub-1280 reflow or scaling is deliberately not built.

### 6. Menu and input surface

- The **Play group** lives in [Menu overlay](#menu-overlay) §5's Settings view, **after** the audio group, in the order **Text speed**, **Auto**, **Fullscreen**, plus its own **Reset**.
- Text speed is a stepped control walking §3's ladder — one step per `←`/`→` press, click snapping to the same steps, the focused row showing the step name. Auto and Fullscreen are switches.
- Keyboard follows [audio settings](#audio-settings) §5's in-Settings model (`↑`/`↓` between controls, `←`/`→` to adjust, `Tab` across the top bar, `Enter`/`Space` to toggle, `Esc` back out) — the deliberate deviation from the card grid's `←`/`→` focus rule, inherited rather than re-decided.
- **Reset** resets the `play` section only (`text_speed` → `0.0`, `auto` → `false`, `fullscreen` → `false`), confirms once, persists on leave, and never touches `audio` or deletes the file.
- **In-play:** `A` toggles auto, `F11` toggles fullscreen. Both are ignored while the overlay is open **or Monitor owns the input/transition frame** ([Interaction](#interaction), amended 2026-10-11) — the overlay owns input there ([Menu overlay](#menu-overlay) §6) — and both show their state through the existing surfaces (the `AUTO` cue; the Settings switch).
- A hotkey toggle **marks the settings dirty and rides [audio settings](#audio-settings) §6's flush points**; there is no write on key press, and the app-exit flush keeps a toggle made just before quitting.
- *(Amended 2026-11-14: the help line is retired; the quick row of [Default look](title-and-menu.md#default-look) §1 carries the keys.)* The always-visible in-play help line is updated to carry them and to drop the F5/F9 text the overlay replaces — e.g. `Space/Enter/click advance · Tab skip · Ctrl skip all · 1-9 choose · A auto · F11 fullscreen · Esc menu`. Exact punctuation is the build's; the skip tokens are [Text skip](#text-skip) §7's *(amended 2026-10-08)*. *(Amended 2026-10-11: F5/F9 remain developer-only under [Dev mode](#dev-mode); they stay absent from this player legend even when enabled.)*

### 7. No engine surface

The whole section is `app/` presentation state: the runner already hands the app the whole line in `Wait::Say { speaker, text }` and then parks until the app calls `advance_say`, so a reveal is only *how* the app renders a line it already holds and a dwell is only *when* it presses advance. **`engine/` and the dialogue DSL are untouched** — no `Wait` change, no parser or content change, no save-format change — and moving either timing into `engine/` is the one change the boundary forbids. Every validation rule below is therefore an `app/` test and `engine/` gains none.

### 8. Deferrals

- **Skip is still out of scope.** Its rule for what skipping does at a `decide` point — where the answer takes seconds and cannot be fast-forwarded without changing what the player is exposed to — belongs to [Text skip](#text-skip). It is not worth carrying for a three-scene demo; it stays recorded as an open question for the packaging / player-build work rather than lost, where replaying a finished game becomes something a player would actually do.
  *(Amended 2026-10-08: the read-tracking half of this deferral is retired — the read log is specified in [Read log](#read-log), keyed by `(scene_id, source line)` rather than `(scene_id, pc)`. The skip half above is retired too: [Text skip](#text-skip) is the spec, and the `Space` control the plan's position named is superseded by `Tab`.)*
- The non-goals in §1 are deliberately **not** deferrals with triggers: nothing about hardening the runtime for a friend makes them reachable, and they belong to the packaging / player-build trajectory if anywhere.

### 9. Validation rules (the build's test list)

- Settings round trip: change all three controls, leave the Settings view → one file holding a `play` section with the three values and an untouched `audio` section; load it in a fresh app → the same values.
- A file with no `play` key → all three defaults in memory; reading it (and merely opening the Settings view) creates no file; a pristine pack never gains one.
- A `play` section holding only `auto: true` → the other two at their defaults, and the next write emits all three.
- Out-of-range and non-finite: `text_speed: 3.5` → `0.08`; `-1.0` → `0.0`; `nan`/`inf` → `0.0`. A wrong-typed `auto` (e.g. a number) → all defaults plus a logged warning, bytes unmodified.
- Fullscreen boot: `fullscreen: true` → the window exists as `BorderlessFullscreen(Primary)` on the first frame (no windowed frame); `false`, a missing key and an unreadable file → 1280×720 windowed.
- `F11` flips the mode and the Settings switch reflects it without the overlay being reopened; `A` flips auto and the `AUTO` cue appears/disappears in the same frame.
- A hotkey toggle writes nothing until the next flush point; toggling `A` then quitting persists it (app-exit flush), and toggling `A` twice restores the file bytes.
- Reveal: at `text_speed = 0.0` a parked say-line is whole on the first frame and shows no partial state; at `0.02` a 50-char line reveals over ≈1 s of simulated `Time` with the `Speaker: ` prefix complete throughout; revealing never splits a multi-byte character.
- Reveal completion: a press mid-reveal shows the whole line and does **not** advance; the next press advances; a left-click mid-reveal behaves identically.
- A say-wait restored from a slot re-reveals from the first character.
- With the overlay open, a running reveal does not progress and no auto dwell fires; closing resumes both from where they stopped ([Menu overlay](#menu-overlay) §7's pause rules hold).
- Auto: with `auto: true` and Instant text, a line advances 2.0 s after it parks; with a 1 s reveal, 2.0 s after the reveal completes; a manual advance during the dwell restarts the full 2.0 s.
- Auto holds at a choice-wait, a decision-wait and end-of-scene, and never calls `choose`, `resolve_decision` or the Decisions client.
- Auto stays on across a choice pick and across a load.
- The Play Reset resets `play` only — the `audio` bytes are identical afterwards — confirms once, and cancelling changes nothing.
- No engine test is added or changed by this section.

### Out of scope for this section

Skip's own rules, which [Text skip](#text-skip) owns; the read log ([Read log](#read-log)); the non-goals in §1; the audio group ([audio settings](#audio-settings)); the overlay's layout, card rules and pause semantics ([menu overlay](#menu-overlay)); the general failure posture ([runtime failure posture](#runtime-failure-posture)); quit-to-desktop; window-manager and output-device specifics; per-line authored pacing in the DSL.

---

## Runtime failure posture

Settled 2026-09-28. Design record: [ADR 0008](adr/0008-player-first-failure-posture.md). **Built** — shipped 2026-10-14.

### 1. Postures

Charting named the runtime's failure posture in prose; this section fixes it as five classes. A failure is classified by **what the player gets and what they can do next**, not by what went wrong — the class *is* the surface and the recovery.

| Posture | Surface | Recovery | Verdict |
| --- | --- | --- | --- |
| **Quit-with-message** | the startup error screen (§4) | fix the pack | **quit** — nothing else is possible |
| **Continue-with-message** | the Notice (§3), or the surface that already owns the message (a slot card) | the action that failed can be retried | continue |
| **Continue-silently** | the log and `<pack-dir>.log` only (§6; row 21's `stderr` warning is the one carve-out) | the next staging command, the next decision point, the next write | continue |
| **Continue-with-fallback** | the log only; the script takes its authored `fallback` branch | the next decision point calls again | continue |
| **Defensive** | the Notice (§3) and an `error!` | `Esc` opens the menu, the only affordance the hold allows | continue, at the menu |
| *not a failure* | — | — | continue |

**Refuse-and-continue** is this section's name for the shape **Continue-with-message** takes when an action was attempted and is refused: the player is told, the action does not happen, and play continues — rows 14–18 and 20, whose message is the Notice (row 16's is the slot card it already owns).

Two rules follow from the set. **Nothing fails without a log line** (§6). And **"game continues" always means the runner reaches the next wait**: a failure never leaves a parked wait without an affordance — the defensive hold (rows 11–13 below) is the one case where the affordance is the menu, and it says so.

### 2. The inventory

Every failure class a player can hit, with its surface, recovery path and verdict; the **Surface** column is where the Notice (§3) appears. Every row is reachable in a shipped build except the three marked **defensive**, which the loader's own validation makes unreachable and which are specified anyway — the cost of being wrong is a frozen screen. Row 1 is reachable only with a malformed pack (that is what it means), and row 22 is not a failure at all.

| # | Failure class | Posture | Surface | Recovery path | Verdict |
| --- | --- | --- | --- | --- | --- |
| 1 | Pack directory missing, manifest unreadable, or any parse/validation error at startup | Quit-with-message | startup error screen: pack path + every `file:line: message` it holds | fix the pack | Quit |
| 2 | Pack warning (`LoadedPack.warnings`, e.g. a `choice` config with no `none`) | Continue-silently | log `warn!` | none — the pack plays | Continue |
| 3 | Background PNG missing **or undecodable** (`backgrounds/<id>.png`) | Continue-silently | nothing drawn; log `warn!` (Bevy also logs the failed load) | the next `bg` replaces it; the author fixes the pack | Continue |
| 4 | NPC sprite/pose PNG missing or undecodable (`sprites/<npc_id>/<pose>.png`) | Continue-silently | that sprite absent, the others stay; log `warn!` | the next `sprite`/`hide`; the author fixes the pack | Continue |
| 5 | BGM missing or undecodable (`audio/<id>.ogg`) | Continue-silently | silence — the channel is stopped and nothing plays; log `warn!` | the next `play`; the author fixes the pack | Continue |
| 6 | `sfx_select.ogg` missing or undecodable | Continue-silently | nothing; log `warn!` | none — the SFX is pack-optional by convention | Continue |
| 7 | No audio device, or any kira setup failure | Continue-silently | silence; log `warn!` (`bevy_kira_audio` also warns itself) | none — play continues | Continue |
| 8 | Decision transport failure: 30 s timeout, HTTP error, non-2xx status | Continue-with-fallback | log `warn!` naming the reason | the authored `fallback` branch plays; the next `decide` tries again | Continue |
| 9 | Decision answer unusable: decode failure, unknown action, wrong type, missing answer, unusable legend index | Continue-with-fallback | log `warn!` naming the reason | as row 8 | Continue |
| 10 | Decision config absent: no API key, or an NPC with no pinned model | Continue-with-fallback | log `warn!` — offline is a supported mode, not a fault | every decision point falls back by design | Continue |
| 11 | A `decide` point with no matching NPC/question — **defensive** | Defensive | `error!` + Notice | Esc → menu; the hold is visible, not silent | Continue |
| 12 | A decision answer whose label does not exist — **defensive** | Defensive | `error!` + Notice | as row 11 | Continue |
| 13 | `advance_say` / `choose` / `resolve_decision` returns a `RunnerError` (not parked, bad index) — **defensive** | Defensive | `error!` + Notice | as row 11 | Continue |
| 14 | Manual save write fails (encode error, IO error, read-only pack directory) | Continue-with-message | Notice; the slot file and the card are left as they were | retry from the overlay; fix permissions or space | Continue |
| 15 | Autosave write fails (scene crossing, choice resolve, model-answered decision) | Continue-with-message | Notice — [Save slots](#save-slots) §4 carries this | the next crossing or resolve retries; manual save still works | Continue |
| 16 | Slot unreadable at open: decode failure, newer version, IO error, `scene_id` absent from the pack, or a failed semantic check ([Save slots](#save-slots) §3) | Continue-with-message | the card's reason ([save slots](#save-slots), [menu overlay](#menu-overlay)) and load disabled | overwrite it, or fix the file | Continue |
| 17 | Load refused at read/restore time (the slot changed under the overlay, or `restore` rejects it) | Continue-with-message | [menu overlay](#menu-overlay)'s refusal message + Notice | pick another slot | Continue |
| 18 | Legacy import fails to write its target slot | Continue-with-message | Notice; the Legacy card stays | retry the import | Continue |
| 19 | Settings file malformed or wrong-typed | Continue-silently | log `warn!`, the bytes untouched ([Audio settings](#audio-settings) §7) | a later change rewrites it | Continue |
| 20 | Settings flush fails (the pack-adjacent file cannot be written) | Continue-with-message | Notice — [Audio settings](#audio-settings) §7 carries this | a later flush retries | Continue |
| 21 | `<pack-dir>.log` cannot be created or written | Continue-silently | one `stderr` warning at startup naming the path and the error | the next write attempts again; diagnostics only | Continue |
| 22 | End of script — no parked wait | **not a failure** | the existing `— end of scene —` marker | menu; quit | Continue |
| 23 | Credential write fails (the user-level `credentials` file cannot be written) | Continue-with-message | Notice — [Decisions settings](title-and-menu.md#decisions-settings) §5 carries this; the key row is left as it was | retry the commit; fix permissions or space | Continue |
| 24 | Credential read fails: the file is unreadable, its content fails the shape check, or the config directory cannot be derived | Continue-silently | log `warn!`; the bytes untouched ([Decisions settings](title-and-menu.md#decisions-settings) §5) | the next launch tries again (a Settings open joins once the editing ticket adds the read-on-open) | Continue |
| 25 | Title backdrop PNG missing **or undecodable** (`backgrounds/<id>.png`, the manifest's `title_backdrop`) | Continue-silently | nothing drawn, the neutral gradient stands; log `warn!` (Bevy also logs the failed load) | the title still shows; the author fixes the pack | Continue |
| 26 | Read log unusable or unwritable (`<pack-dir>.read.ron`) *(added 2026-10-08)* | Continue-silently | log `warn!`; nothing shown ([Read log](#read-log) §7) | the next flush retries; an unreadable file means everything unread | Continue |

### 3. The Notice

A single transient line for the rows whose **Surface** column says Notice — the postures' one player-facing surface.

- **App-side presentation state only**: a `Notice` resource holding at most one `(text, remaining)` pair, rendered by a presentation system. It never advances the runner, never cancels an in-flight Decisions request, and never writes the autosave.
- *(Amended 2026-11-14: the slot stands; the help line and the corner cue it names are retired by [Default look](title-and-menu.md#default-look) §1.)* **Placement**: top-right (`top: 12px`, `right: 48px`), right-aligned — the one empty corner, since the help line is top-left, the dialogue band is the bottom, and the `AUTO` cue ([Player comfort](#player-comfort) §4) is bottom-right. Plain text in a dim tint; no icon, no colour coding and no animation is specified beyond §3's lifetime.
- **Lifetime**: roughly 5 s of **unpaused** time. The timer freezes while the [menu overlay](#menu-overlay) is open and resumes on close, exactly like the text-speed reveal ([Player comfort](#player-comfort) §3), so a line is never consumed while the world is frozen.
- **One slot**: a second failure replaces the first. A failure fires **once per occurrence** — a repeated failed autosave re-notices at each failed write rather than going quiet, so what the player reads is always the current state.- **It never consumes input**: no key and no click is intercepted, and the Notice is not dismissed by a press. `Esc` still performs exactly [menu overlay](#menu-overlay) §6's four moments — the Notice is deliberately not a fifth.
- **Text**: one short string per failure (§5). What the player reads is a prefix of what the log records.

### 4. The startup error screen

A pack that does not load is the only failure with nothing to recover, so its message has to be readable where the player is: the window, not a console a file-manager launch never shows.

- `setup_pack` **no longer exits**. It records the failed load (the canonicalised `pack_root` plus the whole `Vec<LoadError>`) instead of printing to `stderr` and calling `std::process::exit`, and a startup system draws the screen.
- **Contents**: the pack path, then up to **20** error lines — `LoadError`'s `Display` already carries the pack-relative `file:line: message` — then `… and N more — see <pack-dir>.log`. A 1280×720 window with no scrolling UI is why the list is capped; the complete list is on disk.
- **Quit**: `Esc`, `Enter`/`Space` or a click quits, with a non-zero exit code so terminal invocations keep their current semantics. **Exception while Monitor owns input/transition frames or held input is quarantined**: these inputs must not quit; Esc only dismisses Monitor, and a subsequent release/fresh quit input restores this behavior ([Interaction](#interaction); amended 2026-10-11). There is no pack-selection UI and no retry: a broken pack is fixed, not picked around.
- **Ordering**: `<pack-dir>.log` is opened (and truncated) **before** the pack load, so the screen's pointer is always valid and a broken pack's full error list is recoverable even though the screen shows at most 20.
- **Warnings are not errors**: `LoadedPack.warnings` never reaches the screen (row 2).
- **Unchanged**: the `unmei` CLI keeps printing every load error and exiting non-zero. The runtime's screen is a second surface over the same `Vec<LoadError>`, not a replacement for the CLI's.

### 5. The failure funnel

Settled while this failure posture was designed: the engine is already typed and Bevy-free, so the discipline lives on the app side of the seam.

- **One module, `app/src/failure.rs`**, owns the vocabulary: a `Failure` value tagged with §1's posture, one `report(failure)` entry point, and the `Notice` from §3.
- **Call sites construct a `Failure`** where an engine error is caught; `report` derives the target, the level (§6) and whether a Notice appears. **No call site chooses its own surface**, so §2's table is the only place that choice lives and a table-driven test can assert all of it. The ~10 scattered `log::error!`/`log::warn!` sites and `main.rs`'s "Decisions disabled…" line collapse into it.
- **Async failures keep the same rule.** A missing or undecodable asset cannot be caught at its `asset_server.load` call, so the load site (`sync_staging_visuals`, `sync_audio`, `choice_input`) names its own row and path to an `AssetWatch`; one presentation system reports each settled `LoadState::Failed` through `report`, once per path. Row 7 is the one row with an origin outside this workspace: kira's setup failure lives in `bevy_kira_audio`'s private `AudioOutput`, so the app's log layer observes the crate's own setup warning and funnels it — the crate keeps logging it too.
- **One string per failure** — the row's short player-facing text, used by both its surface (the Notice, or the slot card for row 16) and its log line:

| Row(s) | Short string |
| --- | --- |
| 3–7, 25 (silent degradation) | `couldn't load a background` · `couldn't load a sprite` · `couldn't load the music` · `couldn't load the choice sound` · `couldn't start audio` · `couldn't load the title backdrop` — log-only, never shown |
| 26 (read log) | `couldn't use the read log` — log-only, never shown |
| 11–13 (defensive hold) | `the story can't continue — Esc for the menu` |
| 14 (manual save) | `couldn't save — the slot is unchanged` |
| 15 (autosave) | `couldn't autosave` |
| 16 (unreadable slot) | the card's reason itself ([save slots](#save-slots), [menu overlay](#menu-overlay)) — this row logs its string **Notice-less**: the card is the surface |
| 17 (load refused) | `couldn't load that save` |
| 18 (legacy import) | `couldn't import the legacy save` |
| 20 (settings flush) | `settings not saved` |
| 23 (credential write) | `Could not save the key` |

  The log line is that string followed by the machine detail — the path, the IO error, the engine error's `Display` — so the notice text is a prefix of every log entry for the same failure.
- **Boundary discipline.** Every fallible engine operation keeps returning `Result<_, typed error>`, and `engine/` stays Bevy-free: `LoadError` (with its pack-relative location), `Warning`, `RunnerError`, `SaveError` and `DecisionError` are the seam as they stand. **No unified engine error enum is introduced** — `load_pack`'s `Vec<LoadError>` does not fit one, and the four types already say what the app needs. App play paths contain **no `unwrap`, `expect`, `panic!` or `std::process::exit`**; the only exit is the error screen's quit. The engine's two static-config `expect`s (`reqwest::Client` construction and the shared Tokio runtime build, both in `engine/src/decision/client.rs`) are named exceptions that stay: neither can fail.

### 6. Logging and `<pack-dir>.log`

- **Target**: every failure logs exactly once, under `unmei::failure`. Row 7 keeps `bevy_kira_audio`'s own warning on the stream as well (the app mirrors it rather than replacing it), so the session log holds both lines while `unmei::failure` holds exactly one. The existing non-failure `info!` lines (which decision answered, via which model, at what cost; saves and loads; and the three progress lines the amendment below adds) keep their current targets.
- **Level**: every row has exactly one level, so the table-driven test can assert it. `error!` for row 1, the defensive rows 11–13, and every refusal in rows 14–18, 20 and 23 (a persistence action the player asked for did not happen, or the file it needed is unreadable); `warn!` for the silent degradations in rows 2–10, 19, 24, 25 and 26, and for row 21 (whose warning goes to `stderr` as well, since the file it wanted to write is the one that failed); row 22 logs nothing.
- **File**: `<pack-dir>.log` — a sibling of the pack, named after it, exactly like the slot and settings files; `*.log` is already in `.gitignore`'s Logs block, so nothing joins it.
- **Truncated at startup**, so the file is one session's record. It carries **the same stream as the console (stderr) under the same filter** — including the info lines a bug report needs for context — written by the same subscriber so the two sinks cannot disagree, and **flushed per line** so a crash or a hard kill still leaves the tail on disk. The exact subscriber wiring is the build's choice; the requirement is both sinks, one writer, no duplicated line.
- **Opened before the pack load** (§4).
- **Unwritable or uncreatable** (read-only or full pack directory): one `stderr` warning at startup naming the path and the IO error, play continues, the next write attempts again, and **no Notice** — diagnostics are not player data and nothing the player did is lost, so the Notice stays out of it. This is the one posture where a log failure is quieter than its subject.
- **No rotation and no size cap**: truncating per launch is what keeps the policy trivial. Retention is fog.

### 7. Validation rules (the build's test list)

**Engine** (`engine/src/save.rs`):

- `list_slots` classifies a slot `Unreadable` when `pc` exceeds the scene's line count, when `pc == 0` and a wait is parked, when the state map holds a key `state.ron` does not declare, or when the parked wait's kind does not match the line at `pc - 1`. ([Save slots](#save-slots) §3 and §7 carry these.) The checks are executable exactly where classification happens: `list_slots` / `read_slot` already take `&Pack`, so the scene's lines and the declared state keys are in hand — which is what lets the overlay classify up front and never offer a load guaranteed to fail.
- A v2 slot whose `pc` has drifted past a shortened scene → `Unreadable`; an untouched round-trip slot → `Ok`.
- `list_slots` and `read_slot` agree on every case: no slot that lists as `Ok` fails to restore, and none that lists as `Unreadable` restores.
- A slot copied in from another pack that names an existing scene but carries undeclared state → `Unreadable`, and the file is preserved.

**App:**

- A table-driven test walks every row of §2: each failure constructs its `Failure`, produces exactly one line under `unmei::failure` at the tabled level, and emits a Notice exactly for the rows §2 marks with one.
- Rows 2–10, 19, 21, 24, 25 and 26 (the silent degradations) write **no** Notice — asserted, so the rule cannot drift.
- A missing or undecodable background, sprite or track logs one `warn!` (rows 3–5), shows no Notice, and leaves the sibling assets in the staging snapshot alone; a valid sibling still loads beside a failed one; the runner still reaches its next wait. A missing `sfx_select.ogg` logs row 6 once; a captured kira setup failure logs row 7 once and never notices.
- Notice: one slot, replacing; the timer expires after the ~5 s of unpaused time; it does not expire while the [menu overlay](#menu-overlay) is open and resumes on close; it consumes no key or click; `Esc` still performs exactly [menu overlay](#menu-overlay) §6's four moments.
- `<pack-dir>.log` is truncated and reopened per launch, receives the lines the console (stderr) receives under the same filter, and a bad-pack run writes every `LoadError` even though the screen shows at most 20.
- An unwritable `<pack-dir>.log` warns once on `stderr`, writes no Notice, and the pack still plays.
- A broken pack no longer exits `setup_pack`: the window opens, the screen shows the pack path and the error lines, and `Esc` or a click quits non-zero. Assert both sides of §4's cap: a pack with 20 or fewer errors shows every one and no `… and N more` line, and a pack with more shows 20 plus the pointer — while `<pack-dir>.log` holds them all.
- Inducing a defensive failure (a `resolve_decision` that cannot land) leaves the runner parked, shows the hold Notice, and `Esc` opens the overlay.
- Autosave failure notices at each failed write and stays silent on a write that succeeds.
- Manual save failure leaves the target slot file byte-identical and the overlay's card unchanged.
- A settings flush failure notices, keeps the in-session values, and a later flush retries.

### Amendments

This section supersedes text that was settled before it, and that text has been amended **in place** rather than left to contradict the inventory: [Save slots](#save-slots) §3's classification rule and §7's test list carry the semantic checks, §4's autosave-failure line now points here, [Audio settings](#audio-settings) §7 and §8 carry the Notice for a failed flush, and [ADR 0006](adr/0006-save-slots-on-disk-format-and-version-policy.md) carries a dated amendment note. **[Save slots](#save-slots) §3 is the canonical statement of the unreadable rule**; its restatements in §7, §2's row 16 and ADR 0006 must move with it, and any change to the rule is one pass over all four. The five glossary terms this section introduces — Notice, Defensive failure, Silent degradation, Unreadable slot, Error screen — are in [`GLOSSARY.md`](../GLOSSARY.md#failures).

**2026-10-01 (hotfix).** The session log is `<pack-dir>.log` throughout this section — the pack-sibling derivation the build shipped, and the name the error screen now derives from the same path helper that writes the file instead of formatting a fixed string. The earlier working name `unmei.log` referred to this same file and has been corrected in place; nothing in the posture, the classes or the routing above changes.

**2026-10-02 (credential write).** [Decisions settings](title-and-menu.md#decisions-settings) §5's credential rows join §2 in place: the failed **write** is **row 23**, a Continue-with-message row whose short string is `Could not save the key` (the one row whose copy is sentence-cased); the **read** failures (unreadable, shape-invalid content, an underivable directory) are **row 24**, a Continue-silently row that logs its detail at `warn!` and leaves the bytes untouched.

**2026-10-04 (title backdrop).** The title screen's optional pack art adds **row 25** to §2 in place: a declared-but-missing or undecodable `title_backdrop` file is a Continue-silently degradation, driven by the same `AssetWatch`/`report` funnel as the play assets, with the short string `couldn't load the title backdrop`. The title keeps drawing the neutral gradient and play is unaffected ([Title layout](title-and-menu.md#2-backdrop)).

**2026-10-08 (read log).** [Read log](#read-log) §7 adds **row 26** to §2 in place: the read log's IO — a *present* `<pack-dir>.read.ron` that will not decode, carries another version or cannot be read, or a flush that cannot be written — is a Continue-silently degradation with the short string `couldn't use the read log`. A *missing* file is the defined first-run state and logs nothing; the file is never deleted; a failed flush leaves its dirty flag set so the next flush moment retries. `FailureRow::ALL` grows to 26 and the table-driven and silent-rows tests move with it.

**2026-10-11 (console sink correction).** §6 and §7 now say **console/stderr**, not stdout: installed `bevy_log 0.19.1`'s default formatted console writer is `std::io::stderr` (`src/lib.rs:353`), and `app/src/failure.rs::file_layer` shares its upstream filter. This is a factual wording/test-contract correction, not a sink, filtering, truncation or flush change. Terminal help's stdout contract is unrelated and unchanged. [Monitor → Log stream](#log-stream) adds only an in-memory viewer; its mandatory source-safety prerequisite is the explicit narrow exception to preserving existing diagnostic payloads, not their targets, levels or failure routing.

**2026-10-14 (status).** The settled banner above now reads **Built** — the section shipped on 2026-10-14. No posture, class or routing changed.

**2026-11-07 (progress lines).** §6's non-failure lines gain three player-visible progress lines that let an automated walk follow a session, each at `info!` under the ordinary target of the module that writes it, so the console (stderr) and `<pack-dir>.log` sinks carry them like every other line: `unmei: title ready` when the title becomes the surface for the first time in a launch, written at startup before the first frame draws it — a later `Exit to title` says nothing, and the boot park behind the title is not a play entry; `unmei: entered scene <id>` on each scene entry, meaning a New Game's entry scene, a restored save's scene, a Timeline scene restart, and every `jump` that crosses into another scene file; and `unmei: saved to <path>` for the Esc overlay's Save as well as the F5 developer shortcut, written by the one manual-save path both use, while the autosave stays silent. No new flag, input path, level or target is added, and nothing above changes.

### Out of scope for this section

The `unmei author` CLI's error surfaces (they keep printing every error and exiting non-zero); the provider seam and the Decisions backend ([provider seam](#provider-seam)); log rotation, size caps and cross-session retention; crash reporting and telemetry; i18n of the Notice and error-screen strings; GPU and window-creation failures, which abort inside Bevy before this crate's code runs; packaging and player builds; and the rest the map already ruled out: CI, many-NPC batching, voice and generalised `sfx`, full VN features.

---

## Provider seam

Settled 2026-09-28. Design record: [ADR 0009](adr/0009-decision-resolver-seam.md). Evidence: the Decisions API drift check of 2026-09-27 rated drift risk LOW — the vendor's [changelog](https://openrouter.ai/docs/changelog) and its Alpha SDK surface still matched the recorded wire contract — and [`decisions-scale.md`](decisions-scale.md) §4 rejected caching. **Built (2026-10-14)** — shipped.

### 1. The verdict, split

The evidence points two ways at once, so the verdict is two verdicts.

- **The vendor abstraction abstains.** `engine/` keeps its single hardwired OpenRouter client. Nothing abstracts the wire contract — `DecisionRequest`, the typed `Answer` enum, `Decision` and `DecisionError` stay concrete — and no second implementation is written. What already covers the realistic breaks: `UNMEI_DECISIONS_URL` (the endpoint override), the per-NPC pinned model id, and the pure `resolve_or_fallback`, which keeps the authored branch playable offline.
- **One narrow injection point is cut now**, because three rules already settled elsewhere in this document cannot be driven in-process without it: [menu overlay](#menu-overlay) §6's answer held while the Esc overlay is open, §6's load-cancels-the-request, and [runtime failure posture](#runtime-failure-posture)'s defensive hold with its Notice. Before the seam, `app/` held a concrete `Option<DecisionsClient>`: every deterministic in-process test passed `None`, which routes straight to the author fallback, and the only test that passed a real client was the `#[ignore]`d live network test — so no reproducible app test could drive a model answer, a held answer, or a cancelled request.

The asymmetry is **testability, not portability**: research 0001 §7 found nothing arguing for restructuring the client before an actual break, and nothing that makes an untestable decision pipeline acceptable. The reasoning and the rejected alternatives are in [ADR 0009](adr/0009-decision-resolver-seam.md).

### 2. The trait

```rust
/// Resolve one parked question, author fallback already applied.
pub trait DecisionResolver: Send + Sync + 'static {
    fn resolve<'a>(
        &'a self,
        model: &'a str,
        question_id: &'a str,
        config: &'a DecisionConfig,
        state: &'a HashMap<String, StateValue>,
    ) -> Pin<Box<dyn Future<Output = ResolvedDecision> + Send + 'a>>;
}
```

- **This is the canonical statement of the signature, and it is a design sketch.** The exact lifetime/`Send`/`'static` bounds interact with how the app spawns the task and with the held-answer and cancellation rules, so the build validated the shape at first compile instead of treating it as fixed. What is settled here is *what* it abstracts and *where* it lives — not the spelling of one line.
- **It abstracts the resolved landing, not the transport.** One call per parked question, the author fallback already applied, returning `ResolvedDecision` — `action`, `model`, `confidence` and `fallback_reason`. That is the whole of what the app consumes, and the reason it needs: [Save slots](#save-slots) §4 distinguishes a model-answered resolve (writes the autosave) from a fallback resolve (writes nothing) by `fallback_reason` alone.
- **It mirrors `decide_with_fallback`**, so the app's call site is the call it makes today.
- **Home: `engine/src/decision.rs`** — the always-compiled, Bevy-free wire module, next to `ResolvedDecision` and `resolve_or_fallback`, reachable from any consumer whether or not the `network` feature is on. `DecisionsClient` implements it in `engine/src/decision/client.rs`, behind the existing `network` feature; both are re-exported at the crate root beside `DecisionsClient`.
- The client's inherent `decide` and `decide_with_fallback` stay, and the trait implementation delegates to `decide_with_fallback` — one fallback path, with the raw `decide` still available to the ignored live test.
- **No new dependency.** The trait returns a hand-boxed future; `async-trait` is not added for a one-method trait.

### 3. What it is not

- **Not a vendor abstraction.** No second implementation, no wire-level seam, no unifying of request/response/error types. The trait exists for **test injection** — which is why it is surprising enough to need an ADR — and not because OpenRouter might move.
- **`DecisionBackend` stays declined.** A *provider-agnostic* `DecisionBackend` was recorded as declined twice, revisitable only if the Decisions API breaks. This trait is narrower and differently motivated, so it takes a different name and the docs do not read as reopening that ruling.
- **`AuthorChat` stays separate.** `cli/src/provider.rs`'s authoring seam is a different job: offline CLI authoring rather than runtime play, a different endpoint (`{base_url}/v1/chat/completions` vs `/api/alpha/decisions`), a different auth env (`UNMEI_API_KEY` then `OPENROUTER_API_KEY` vs `OPENROUTER_API_KEY` alone), a different wire shape, structured output with a batched retry loop, and no in-fiction fallback — its failure is a CLI message, not a script branch. It also lives in `cli/`, which `engine/` cannot depend on. Revisit only if a second real consumer of either appears. Its own contract is [`author-cli.md`](author-cli.md) §4 and [`author-cli-drafting.md`](author-cli-drafting.md) §7.
- **The in-fiction author fallback is not a backend.** `Fallback::Action`/`Bool`/`Index` is a config value the pure `resolve_or_fallback` already applies; it has no address, no request and no failure mode of its own. The ticket's question — whether the author fallback is a second backend worth unifying — answers *no*.
- **No mock ships in `engine/`.** The fakes are test-local in `app/`'s tests, following `cli/tests/`'s precedent (four `AuthorChat` fakes, none shipped in `cli/src/`).

### 4. How it lands in the app

- `RuntimeConfig.decisions: Option<Arc<dyn DecisionResolver>>`. `None` keeps its meaning: every decision resolves immediately to its author fallback with the reason `no Decisions client` — the offline posture, unchanged. (The string deliberately does not name the environment variable, so no runtime log line carries it; the short form is the one already specified for the resolver.)
- The two **immediate** fallbacks stay app-side, because both are app-owned facts: no client (`None`), and an NPC with no pinned model. `start_decision` still returns them as `DecisionStart::Immediate`.
- `PendingDecision` keeps `Option<Task<ResolvedDecision>>`; only the spawn's call target changes, from the concrete client to the trait object. Dropping the task still cancels the request, which is what [menu overlay](#menu-overlay) §6's load rule relies on.
- `main.rs` keeps building `DecisionsClient::from_env()`, and the app keeps depending on `engine/`'s `network` feature. The seam is for injection, not for slimming the dependency graph.
- `app/tests/live_decision.rs` needs nothing beyond wrapping the concrete client in an `Arc`, and is otherwise unchanged — it stays the only real-network coverage, alongside `engine/tests/decision_live.rs`. (§6 confirmed it at first compile.)
- The existing `None`-path tests in `app/src/lib.rs` are untouched.

### 5. Build ordering

**The seam was the build effort's first step.** It is a prerequisite for the in-process tests of [menu overlay](#menu-overlay) §6 (the held answer, the load-cancels-the-request) and [runtime failure posture](#runtime-failure-posture) (the defensive hold), none of which can be written against a concrete client. The section's resolution lists it as such.

**Two different seams, not one.** [Menu overlay](#menu-overlay)'s build note asks for "a small app-side seam" to buffer a completed resolution and defer advancement while the overlay is open. That buffer is app-side and stays app-side — it is not this section's subject. This section fixes the *other* seam: the resolver injection point, whose trait lives in `engine/` and whose only use is in `app/`.

### 6. Validation rules (the build's test list)

The fake must be **jointly controllable**: able to withhold completion until the test releases it, and to observe that a dropped task was dropped. Without both, a test re-proves only what `None` already proves.

**Engine:**

- `DecisionsClient` implements `DecisionResolver`, and `resolve(...)` returns exactly what `decide_with_fallback(...)` does for the same answer and for the same error — the delegation is asserted, not assumed.
- The trait is object-safe and `Send + Sync`: `Arc<dyn DecisionResolver>` compiles, and so does a build with the `network` feature off — the trait is always compiled, the implementation is not.

**App** (each driven by a fake backend, with no network and no key):

- A completed **model** answer lands the runner on the answer's label, carries no `fallback_reason`, and writes the autosave once ([Save slots](#save-slots) §4).
- A **fallback** resolution lands on the author's branch, carries the reason, and writes **no** autosave.
- An answer completing while the Esc overlay is open is **held**: `pc` and the parked wait are unchanged, and on close the answer is applied and the runner parks at the next wait.
- A **load** while a request is in flight drops the task and discards any held answer; a restored decision-wait re-fires the `decide` line ([Menu overlay](#menu-overlay) §6).
- A resolution whose label does not exist leaves the runner parked, logs `error!` and shows the hold Notice, with `Esc` opening the overlay ([Runtime failure posture](#runtime-failure-posture) rows 11–12).
- `None` still takes the author fallback — the existing test stands, unchanged, as the offline posture's guard.
- `app/tests/live_decision.rs` and the `None`-path tests compile and pass with only the `Arc` wrap — the two expectations §4 states, confirmed rather than assumed.

### 7. The recorded trigger

The vendor abstraction is revisited when **any one** of these becomes true:

- the alpha endpoint **breaks**, or a contract change lands that the endpoint override and the pinned model cannot absorb;
- a **second provider** is wanted for the same job — another transport for the same model (TypeSafe's System One, per research 0001 §3) or a different decision-model vendor;
- an **offline mode** is wanted that goes beyond the author fallback — a local model, or a recorded-answer mode;
- the many-NPC **batching** story flips, per [`decisions-scale.md`](decisions-scale.md) §1.

**A cache is not a trigger.** It is rejected outright in [`decisions-scale.md`](decisions-scale.md) §4 — replaying a stored answer presents a stale judgment as a current one — so "offline/cache mode" is deliberately split: offline is a trigger, caching is closed. The trigger is restated in [ADR 0009](adr/0009-decision-resolver-seam.md) so the abstention cannot be lost if this section is later superseded; `NEXT-STEPS.md`'s provider-seam mentions (that file was deleted 2026-10-08) pointed at it, and this section and the ADR are their surviving home.

### Amendment (2026-10-14, status, provider seam built)

The settled banner above now reads **Built** — the seam shipped on 2026-10-14. §1's "Today `app/` holds a concrete `Option<DecisionsClient>`", §2's "the build validates the shape at first compile", §4's "Expected, not observed" and §5's "The seam is the build effort's first step" described the pre-build baseline and are corrected to the past tense in place: `DecisionResolver` is in `engine/`'s always-compiled `decision` module, `DecisionsClient` implements it behind `network`, `RuntimeConfig.decisions` holds `Option<Arc<dyn DecisionResolver>>`, and §6's test list is built against the jointly controllable `FakeResolver` in `app/src/tests.rs`. The verdict, the trait and the revisit triggers are unchanged.

### Out of scope for this section

Implementing the seam or any part of §4 (build effort); the held-answer buffer itself, which is [menu overlay](#menu-overlay)'s build note; any second provider, offline mode or batching implementation — §7 names when they are reconsidered, and batching's own constraints live in [`decisions-scale.md`](decisions-scale.md); caching, already rejected there; unifying `AuthorChat`; a wire-level abstraction of request/response/error types; the `unmei author` CLI's provider, settled in [`author-cli.md`](author-cli.md) §4.

---

## Read log

Settled 2026-10-08. Design record: [ADR 0011](adr/0011-read-log-as-a-third-persistence-class.md). **Built (2026-10-14)** — shipped.

### 1. File and key

- `<pack-dir>.read.ron` — a sibling of the pack directory, named after it, exactly like the slot, settings and log files. One file per pack, RON, never inside the pack directory, so the pack stays pristine and portable (the [`slice-save-audio.md`](slice-save-audio.md) §1 posture, kept).
- The file holds two collections: the **say-lines** the player has read, and the **scenes** they have visited ([`GLOSSARY.md`](../GLOSSARY.md#reading): Read log, Visited scene).
- A line is keyed by **`(scene id, authored source line)`** — the scene's filename stem plus the 1-based source line the say-line was authored on, which the parser records for every `Line` and `Scene::source_line` exposes. At a say-park the runner's `pc` is the say-line's `lines` index plus one ([`runner/mod.rs`](../engine/src/runner/mod.rs) increments before parking), so the app's key is `source_line(pc - 1)`.
- **The key is deliberately not `pc`.** `pc` is a position in the parsed `lines` vector whose say-park value carries the increment-before-park offset; the authored source line is the coordinate the author wrote and the one a human reads in the file (`"reunion": [12, 13]`). A save's position keeps using `pc` ([Save slots](#save-slots) §2); the read log and a save describe the same park in different vocabularies on purpose (ADR 0011).
- One entry per authored line, **however many visits**: a `{var}` say-line whose interpolated text differs between visits is one key, and the UI sync's consecutive-identical-lines quirk is untouched ([Player comfort](#player-comfort) §3).
- The visited set holds scene ids.
- The file is **player knowledge**: not a save and not a preference. One per pack, never per slot; New Game deliberately does not reset it ([`title-and-menu.md`](title-and-menu.md) § Title §3); nothing in it is required to play, and losing it costs at most a re-read.

### 2. Engine API (all in `engine/src/read_log.rs`, Bevy-free)

```rust
pub const READ_LOG_VERSION: u32 = 1;

/// The whole file, as the app holds it in memory.
pub struct ReadLog {
    pub version: u32,
    /// Scene id -> the authored 1-based source lines of the say-lines seen.
    pub read: BTreeMap<String, BTreeSet<usize>>,
    /// The scene ids the player has played in.
    pub visited: BTreeSet<String>,
}

impl ReadLog {
    pub fn is_read(&self, scene_id: &str, source_line: usize) -> bool;
    /// True when the line was newly inserted — the app's dirty trigger.
    pub fn mark_read(&mut self, scene_id: &str, source_line: usize) -> bool;
    pub fn is_visited(&self, scene_id: &str) -> bool;
    /// True when the scene was newly inserted.
    pub fn mark_visited(&mut self, scene_id: &str) -> bool;
}

pub fn read_log_path_for(pack: &Pack) -> PathBuf;
/// A missing file is the empty log with no reason; a present file that fails to
/// decode, carries another version, or cannot be read is the empty log plus the
/// reason the caller logs (§7).
pub fn load_read_log(path: &Path) -> (ReadLog, Option<String>);
pub fn write_read_log(path: &Path, log: &ReadLog) -> Result<(), ReadLogError>;
```

- `ReadLog::default()` is the empty log, stamped `version: READ_LOG_VERSION`; `ReadLogError` carries `Io { path, message }` and `Encode(String)`, mirroring `SaveError` ([Save slots](#save-slots) §2). `load_read_log` never returns an error, because every read failure is the empty log plus a reason.
- The engine owns the naming and the IO exactly as `save.rs` owns the slots': the shared `sidecar::sibling_path` helper backs both derivations, so they cannot drift; `sidecar::write_atomic` backs the one atomic write. The **app** owns everything about *when* — the resource, the arrival status, the dirty flag and the flush moments (§4, §6). The `unmei` CLI does not read or write the file.

### 3. Version policy

- `version: u32` is written first, `READ_LOG_VERSION` is the current value, and the policy is **strict equality** — a file loads only when `version == READ_LOG_VERSION`.
- **No additive floor**, deliberately unlike [Save slots](#save-slots) §3. The floor exists there because a checkpoint is player data whose loss is unrecoverable; here the payload is knowledge, and a partially-understood older key set risks the one failure the log exists to prevent — skipping text the player never saw. A version bump therefore re-locks every line and every scene rather than migrating them (ADR 0011 records the rejected floor).
- An **absent, undecodable, wrongly-versioned (older *or* newer) or IO-unreadable** file means **everything unread**: the empty log, never an error, never a [Notice](#runtime-failure-posture), and the file is never deleted, renamed or truncated. "Never deleted" bounds *reading*: the bytes are left exactly as they are at load time.
- The first **dirty flush** writes the current format over whatever was there — including a newer-versioned file, whose build is not the one running. That write is an ordinary flush of this session's marks (§6), not a separate upgrade or discard step.
- **Unknown or renamed entries are ignored** and dropped by the next write: a scene id the pack lacks and a source line that no longer identifies a say-line never reach the app's usable in-memory log, and the write serializes that log rather than merging with the bytes on disk. *(Clarified 2026-10-08: `load_read_log(path)` decodes without pack access; the app filters its collections against the loaded pack once at launch, before evaluating arrival status or visits. Retain only loaded scene ids and their authored say-line source numbers; filtering alone does not dirty the log or write a file.)*
- A file is created only by a flush that has something to write: launching a pack and sitting on the title marks nothing, so a pack that is never played never gains a `.read.ron`.

### 4. What counts as read

- A say-line is **read the moment play parks on it**: the park is the render, and the line is on screen from the frame the park lands. The mark lands once per park **transition** — the frame the runner lands on the say-line — not on every frame the park lasts.
- Only `Wait::Say` parks count. A choice-wait, a decision-wait (answer in flight or answered) and the end-of-scene marker mark nothing.
- **The boot park behind the title does not count.** The runner is already parked at the entry scene's first wait while the title is up, but that is not play ([`title-and-menu.md`](title-and-menu.md) § Title §1): the boot park marks no line and visits no scene.
- **Arrival status** — the skip gate's view. On the transition into a park the app evaluates `was_read(scene_id, source_line)` **before** marking, and keeps that boolean with the park while it lasts; the seen-text gate reads the arrival status, so a line is skippable only when it was read *before this visit*. Without this rule, marking at the park would make every line read the instant skip reached it and the stop-at-the-first-unread-line rule could never fire.
- **Force-skip's own answer belongs to the skip section**: whether a force-skip writes marks at all is its call, not this section's. This section fixes only the general rule and the arrival status the gate consumes.
  *(Amended 2026-10-08: the answer is **no** — a force-skip advance is exempt from the mark, so text blown past stays unread; its parks still evaluate the arrival status, and the scenes it crosses are still marked visited. The exemption attaches to the advance, not the line: a later manual park marks normally. See [Text skip](#text-skip) §8.)*
- **Recorded consequences** of mark-at-park (the rejected alternative was mark-on-advance, ADR 0011): a line quit mid-reveal, or on screen for a single frame, counts as read and is skippable next session; a line restored onto by a load counts on the frame it is shown; New Game's first line counts when play is entered.

### 5. What counts as visited

- A scene is **visited when it becomes the live play scene**: play is entered on it (New Game, Continue resuming a live session, Continue loading the autosave), it is crossed into by a `jump` that lands in a different scene file (`RunOutcome.entered_scene.is_some()`), or it is restored by a load.
- The boot park behind the title is not play, so the entry scene is not visited by launching the app or looking at the title; it becomes visited the moment play is entered. A player who starts a new game and immediately quits has visited the entry scene.
- A load marks the **restored** scene visited even when the slot came from elsewhere (an imported or copied save): visited means "the player has played here", and the restored scene is where they are playing.
- A **Timeline jump** is a jump like any other and needs no special rule: it lands the runner in a scene, and that scene becomes the live play scene. Only visited scenes are selectable, so the mark is a no-op in practice; the Timeline section ([Timeline](title-and-menu.md#timeline)) consumes this rule rather than re-deciding it.
- The set is **monotone**: nothing removes a scene from it, and New Game does not reset it.

### 6. Lifecycle and flush points

- **Read once per launch**, after the pack loads; never re-read. A pack that fails to load reaches the error screen and neither reads nor writes the file ([Runtime failure posture](#runtime-failure-posture) §4). The in-memory log is the session's only source of truth; the file is an output.
- **One flush, one write**: the whole log, atomically — a temp sibling plus a rename, exactly `write_save`'s shape. Never once per line, never a partial write, never a merge.
- **Flush moments, defined in their own right** (the autosave's moments inform the list; the autosave is not its owner):
  - **play entry into a scene** — New Game, Continue resuming, Continue loading, and every confirmed Timeline landing, including a same-scene restart; *(Amended 2026-10-08: this makes Timeline's scene-entry opportunity explicit; dirty-gated after parks settle, independent of autosave. Opening/walking/cancelling adds none. [Timeline](title-and-menu.md#timeline) §5.)*
  - a scene **crossing**;
  - after a **player choice resolves**;
  - after a **decision resolves** — from a model answer ***or* its author fallback**;
  - **app exit**, beside the settings file's exit flush ([Audio settings](#audio-settings) §6).
- **Not flush moments**: a same-scene label jump, opening or closing the overlay, `Exit to title`, a defensive hold, the error screen. A dirty log simply waits for the next moment.
  *(Amended 2026-10-08: skip adds no flush moment to this list — a running seen skip creates no new park marks (it only passes already-read lines) and force skip marks nothing; see [Text skip](#text-skip) §8.)*
- **Writes are dirty-gated.** `mark_read` and `mark_visited` return true only when they insert something; the app flushes only with a set flag and clears it on success. A re-visit of an already-read line, a resume of a live session and a second flush with nothing new all write nothing.
- The flush runs **after the frame's parks have settled** — the autosave's write point is the model ([Save slots](#save-slots) §4) — so the marks of the frame that crossed or resolved are in the file.
- A **failed write keeps the flag set**: the previous bytes stay, the next flush moment retries, and §7's row logs each attempt. A crash between flushes loses at most the marks since the last one — the current scene's progress — and force-skip is the escape hatch for replaying them.
- **The fallback divergence is deliberate**: [Save slots](#save-slots) §4 skips the autosave on an author fallback so a transient outage never churns the checkpoint; read marks are player knowledge, so they flush regardless. A fallback-answered decision writes the read log and still writes no autosave.

### 7. Failure posture

- **The read log never fails the player.** Its IO joins [Runtime failure posture](#runtime-failure-posture)'s inventory *(amended 2026-10-08)* as **row 26**, **Continue-silently**: exactly one `warn!` under `unmei::failure` per occurrence, no Notice, no hold, no player-facing surface, play continues.
  - **Read**: a *present* file that does not decode, carries another version, or cannot be read logs once with the path and the reason, and yields the empty log. A *missing* file is the defined first-run state and logs nothing.
  - **Write**: each failed flush logs once with the path and the IO error; the dirty flag survives, so the next flush moment retries.
- **Nothing is deleted on any failure**: after a decode failure, a version mismatch or a failed write, the file is still at the same path, byte-identical until the next successful flush.
- The amendment's mechanics move together: §2's inventory table gains the row; `FailureRow::ALL` grows to 26; the row's posture is `ContinueSilently`, its level `warn!`, its short string `couldn't use the read log`, and its Notice the `None` the table derives; the table-driven and silent-rows tests include it.
- The module is the only new engine surface: the runner, the parser, the DSL, `SaveFile`/`SAVE_VERSION`, the settings file and the pack layout are untouched. The read log is not a slot, never reaches `SaveFile`, and never blocks a load.

### 8. Validation rules (the build's test list)

**Engine** (`engine/src/read_log.rs`):

- `read_log_path_for(pack)` is `<parent>/<pack-name>.read.ron` for a pack in any directory, sharing the sibling derivation with the slot and log files.
- A key round-trips: a say-line parked at `lines` index `k` records `line_numbers[k]`; the runner's `pc` is `k + 1`, and the app derives the key from `pc - 1`.
- Two visits to the same parked line — including one whose `{var}` interpolation differs — leave one entry.
- A missing file → the empty log with no reason and nothing logged; an undecodable file, `version != READ_LOG_VERSION` (older *and* newer), and an IO read error → the empty log with a reason, and the bytes on disk byte-identical afterwards.
- `version: 2` and `version: 0` → the empty log; a later `write_read_log` writes `(version: 1, …)` over the file.
- The path-only decoder preserves structurally valid collections; the app's launch filtering against the loaded pack removes absent scene ids, line `0`, out-of-range source lines and source lines that identify commands rather than say-lines. They cannot unlock a card or pass seen skip and are absent from the next ordinary dirty write; filtering alone writes nothing.
- Round trip: marks in two scenes and two visited scenes → write → load in a fresh process → identical `read` and `visited`.
- The bytes are stable: the map and the lists are sorted on write, and writing an unchanged log twice produces identical bytes.
- `ReadLog::default()` serializes with `version: READ_LOG_VERSION` and both collections empty.

**App** (the lifecycle; no network, no assets):

- The log is read once per launch after the pack loads; entering play, re-entering the title and loading a slot never re-read it; a pack the loader rejects neither reads nor writes the file.
- A first visit to a say-line marks it and reports arrival-unread in the same step; a second visit to the same line reports arrival-read.
- The boot park behind the title marks nothing and visits nothing; on entering play the current park marks and the entry scene becomes visited.
- A choice-wait, a decision-wait and an end-of-scene park mark nothing; a load's re-park marks the restored say-line.
- New Game marks the entry scene visited; Continue-resume changes nothing and, being clean, writes nothing; a cross-scene jump marks the target; a same-scene label jump marks no scene.
- A load marks the restored scene visited, including a slot copied in beside a different pack.
- Flush moments: play entry, a crossing, a choice resolve, a model-answered decision, a fallback-answered decision and app exit each write at most once, and only when dirty; a same-scene jump, overlay open/close, `Exit to title` and a Notice write nothing.
- A fallback-answered decision flushes the read log and writes no autosave — the deliberate divergence from [Save slots](#save-slots) §4, asserted against the same frame.
- A failed flush logs once at `warn!` under `unmei::failure`, shows no Notice and engages no hold, leaves the previous bytes, and keeps the dirty flag; the next flush moment retries and succeeds when the path is writable.
- The file is still present and unrenamed after a decode failure, a version mismatch and a failed flush.
- New Game preserves every existing read mark and visited scene, then applies ordinary play-entry visits, displayed-park marks and the dirty-gated flush. With a clean log and already-known entry marks, collections and file bytes stay identical and no write occurs; with unknown entry marks, only those ordinary marks are added and flushed. A pre-existing dirty log is not discarded or assumed write-free.
- `FailureRow::ALL` contains 26 rows; the new row logs exactly once at `warn!`, shows no Notice and engages no hold, and the silent-rows test includes it.

### Amendment (2026-10-08, build-spec synthesis)

The change that built text skip, the read log and the Timeline clarified the New Game validation raised while the Timeline was designed: “identical before and after the click; the click itself writes nothing” becomes **retain existing knowledge plus ordinary entry marking and dirty flushing**, with no-write only for a clean, already-known entry. This aligns the test with §4–§6, not a new lifecycle decision. §3/§8 also locate pack-aware filtering at the app's launch boundary, leaving the specified path-only decoder intact.

### Amendment (2026-10-14, status, read log built)

The settled banner above now reads **Built** — the read log shipped on 2026-10-14. No lifecycle, format or failure-posture rule changed.

### Out of scope for this section

Skip's own rules — what starts and stops it, its dwell, its cue, and force skip's mark exemption — which [Text skip](#text-skip) owns; the Timeline's surface, gating, jump and confirm, which the Timeline section owns ([Timeline](title-and-menu.md#timeline)) — this section fixes only what *visited* means; the manifest `scenes` list; any change to the runner, the `.unmei` DSL, `SaveFile`/`SAVE_VERSION`, the settings file or the pack layout; per-entry deletion or edit tooling for the log; a read-log UI; sync, sharing or per-slot read state; anything the `unmei` CLI reads.

---

## Monitor

Destination of the Monitor map. Dev mode, log stream, interaction and layout are settled below, and the shipped state of each slice is recorded in the dated status notes in this section: the mandatory source-safety prerequisite and the five slices that followed it, up to the assembled rendered walk. The spec text below is the settled contract; every slice has since shipped, and the dated status notes say what. **Built (2026-10-14)** — dev mode, the log stream, the drawer and its reader, and the rendered acceptance walk.

**Implementation status (2026-10-11).** The prerequisite and the first five slices have landed: credential-safe diagnostics, the app-input ownership seam, shared dev mode and launch grammar, bounded hidden capture with the real-sink credential proof, the safe F3 drawer and the older-line reader. The [Log stream](#log-stream) capture/retention, the visible filter/projection/status contracts, the complete [Interaction](#interaction) reader (wheel/scrollbar/paging, stable anchors, filter fallback, expiry acknowledgement) and the [Layout](#layout) drawer geometry are implemented and measured on a real Bevy render (see each status note). The assembled rendered acceptance walk has landed (see the Layout status note); **human confirmation of that walk was recorded on 2026-10-12** and the browser prototype remains reference only.

### Dev mode

*Settled 2026-10-11*, rounds Q1–Q12 and the final shared-understanding check confirmed by the human. This subsection specifies the launch/configuration gate only.

**Implementation status (2026-10-11).** The launch/config gate and the F5/F9 build gate described here have landed: `RuntimeConfig::new`/`with_dev_mode`, the binary's `--dev`/help/usage grammar, and the shortcut run condition all exist and are tested. The Monitor-only **capture layer and retained history** have also landed (see the Log stream status note), and the **F3 drawer now exists**: when the gate is on, `RuntimeCorePlugin` installs the drawer state, the held-input quarantine and the projection/UI systems, and nothing at all when it is off. The help and README now document the live F3 drawer and its older-line reader.

#### Enablement and lifetime (Q2–Q3)

- **One effective value for Monitor and F5/F9**: debug builds default on, release builds default off, and the binary's `--dev` forces on. This is enablement, not panel visibility. Player save/load, Continue, legacy import, autosave, existing log sinks and failure routing are not gated by it.
- **Build default (Q9)** means `cfg!(debug_assertions)`, not optimization level or presence of debug symbols. Standard Cargo debug/test defaults on and release off; custom profiles follow their debug-assertions setting.
- **Library construction (Q2/Q8)**: `RuntimeConfig::new(pack_root, decisions)` uses that build default; an explicit construction-time override (e.g. `with_dev_mode(bool)`) allows either state in any build. RuntimeConfig carries one effective value; Monitor wiring and the shortcut gate consume it, never independently recomputing a feature-specific default. Both RuntimeCorePlugin and UnmeiPlugin honor the supplied value. Only the binary parses `--dev`; the library reads no process arguments. Exact internal resource/helper spelling remains implementation freedom.
- Resolve the effective value **before installing LogPlugin and the runtime plugins**. All three developer surfaces consume the same construction value, fixed for the app lifetime. It is never persisted and cannot be changed in-session; loads, New Game, title/menu transitions and hiding the enabled Monitor do not alter it.

| Standard build | Binary without `--dev` | Binary with `--dev` |
| --- | --- | --- |
| Debug/test (`debug_assertions` on) | On | On |
| Release (`debug_assertions` off) | Off | On |

Explicit library on/off overrides the default in either build. This override is a construction seam, not a `--no-dev` flag.

#### Launch grammar (Q1)

- Retain one optional positional `PACK_DIR`; no path means `demo-pack` in the folder holding the executable — in every build, never the working directory, and with no working-directory fallback when the executable cannot be located (that case is the no-executable error screen, which opens no session log). `--dev` alone uses that same default. Path canonicalization remains best-effort, with the original path retained on failure.
- Recognize `--dev` before or after the positional path until `--` ends option parsing. Thus `unmei-play --dev examples/demo-pack` and `unmei-play examples/demo-pack --dev` agree. After `--`, tokens are positional: `unmei-play -- --dev` names a pack and does not force dev mode.
- Do not add runtime `--pack`; the offline authoring CLI's `--pack` is a separate contract.
- **Repetition (Q5)**: any number of exact `--dev` tokens before `--` is idempotent, including in debug builds: on, with no redundant-flag warning.
- **Invalid launch (Q6)**: unknown leading-dash options before `--` (including `--dev=false`, `--pack` and `--no-dev`), an empty path token, or more than one positional path produce a concise diagnostic plus usage on **stderr**, exit **2**, and no startup side effects. Keep pack paths as OS strings; a non-Unicode path is not invalid merely for its encoding.
- Parse/validate before credential or settings reads, resolver creation, session-log opening/truncation, App creation or window creation. A syntactically valid missing/broken pack path is **not** a usage error: retain best-effort canonicalization and the existing in-window pack-load error contract.

#### Discoverability (Q4)

- Explain the build defaults, `--dev` and F5/F9 in clearly developer-only launch documentation/help. Warn explicitly that **F5 immediately overwrites manual slot 1 without confirmation**; F9 loads that slot. Document F3 to toggle Monitor and F3/Esc/Close to dismiss it, with exclusive input ownership and no new presentation pause ([Interaction](#interaction), amended 2026-10-11).
- Keep ordinary player help, the title legend and Settings free of developer controls, even when dev mode is enabled. No new enabled-only in-window hint.
- **Terminal help (Q7)**: support `--help` and `-h`. A valid help invocation prints usage, the beside-the-executable demo default, `--` escaping and a marked developer section to **stdout**, exits **0**, and has no startup side effects. The developer section states the build/force-on policy and the F5 overwrite/F9 load behavior. `-V`/`--version` prints `unmei-play <version>` from the crate's `CARGO_PKG_VERSION` to stdout, exits **0** and shares the same side-effect-free rule and the same whole-grammar validation. **Errors beat help (Q11)**: validate the entire argument grammar first. An invalid invocation still returns stderr/exit 2 even when help was requested. Otherwise any recognized `-h`/`--help` prints help, which beats a version request in either order, so `-V --help` prints help; repeats are idempotent and valid flag/path combinations are accepted. After `--`, both spellings are positional path tokens. A valid nonexistent pack path with help is not accessed or checked.

#### Disabled surfaces (Q10/Q12)

- F5/F9 are a **silent no-op before shortcut work**: no slot inspection/read/write, runner capture/restore, request cancellation, held-answer discard, skip/auto/read-log changes, shortcut log or Notice. No shortcut press is deferred until a gated view closes. Ordinary player save/load, Continue, import, autosave and all pre-existing non-shortcut logs remain active.
- Do not install the **Monitor-only** capture layer/channel, retained-log buffer, panel or input systems when off. Existing console/file sinks and the kira observer remain installed, with the same filtering, truncation, flush and failure contracts.
- When on, install Monitor capture before pack-load/startup events, independent of whether the panel is shown. Hidden-state retention and which surfaces can display it belong to the log-stream/interaction tickets. The feature's code remains available in release: `--dev` must work, not encounter a debug-only compilation boundary. RuntimeCorePlugin remains headless-capable; this gate does not add rendering or process-global logger initialization to a MinimalPlugins harness.

#### Enabled F5/F9: preserve the existing shortcut contract

- F5 captures/stamps/writes **Manual(1)** immediately, including overwriting an occupied/unreadable slot **without confirmation**. It does not cancel a decision or stop skip. F9 loads only Manual(1); a missing slot is a clean no-op. Preserve unreadable-slot/restore/write failure routing and atomic-write behavior.
- Preserve the existing guards: a loaded Runtime and the title/menu closed. No shortcut work on the title, its subviews, an open overlay or the startup error screen. **Also inert while Monitor owns input, on its opening/closing frames and for unreleased quarantined input afterward** ([Interaction](#interaction); amended 2026-10-11). A defensive hold does not disable save/load recovery; any parked state, including an in-flight decision, retains the existing shortcut behavior.
- Successful F9 retains pending-request cancellation, held-answer discard, defensive-hold release, auto/skip reset, read-log replacement handling and visual/audio/UI replay. A restored decision-wait can re-fire normally. Missing/refused loads do not perform those replacement resets; settings and autosave policy are unchanged.
- Preserve simultaneous-input semantics: F5 executes before F9 when both are pressed (a successful save is what the load then reads; a failed save does not suppress the load). From play, same-frame F5/F9 runs before Esc opens the overlay **when Monitor does not own/claim that input frame**; F3 transitions take precedence over both ([Interaction](#interaction); amended 2026-10-11). A frame that closes an already-open overlay does not replay those keys into play. These are preservation rules, not new interaction choices for the Monitor.

#### Validation rules (the later build's acceptance list)

These are specified checks, **not tests run by this planning ticket**. Reuse meaningful existing save/load, decision-cancellation and skip tests rather than duplicating them.

- Verify normal debug/test on, normal release off and release `--dev` on against the table, with Monitor capture and F5/F9 sharing the same effective value. Cover a custom debug-assertions setting rather than inferring mode from optimization/debug symbols.
- Normal RuntimeConfig construction follows the build default. Explicit on and off work in both profiles; headless and rendered plugins honor the supplied value, with no library process-argument read or per-feature recomputation. Tests that require shortcuts explicitly enable them; disabled regressions explicitly disable them, so a release test run protects the same behavior.
- Launch cases: no args and `--dev` alone use the cwd-relative demo default; a pack path alone is retained; `--dev pack` and `pack --dev` agree; repeated `--dev` is accepted silently; `--` alone uses the default; `-- --dev` and `-- -h` are pack paths; `--dev -- --dev` forces on while naming that pack. Preserve OS-string paths and best-effort canonicalization.
- Reject a representative unknown option, `--dev=false`, runtime `--pack`, `--no-dev`, an empty pack token and two paths, including extra path tokens after `--`. Assert diagnostic/usage on stderr, exit 2, no window and no startup work; a pre-existing session log and sidecar bytes remain untouched.
- `-h`/`--help`, repeated help and a valid `pack --dev --help` print the required stdout content and exit 0 without accessing pack content or starting the app. A nonexistent pack plus help succeeds; `--help --bogus` and `-h pack-a pack-b` remain usage errors. Assert that help and errors do not truncate an existing session log or touch player sidecars.
- A valid broken/missing pack still follows the existing startup error-screen/nonzero-exit contract, not the argument-error path.
- Off: F5 cannot create/overwrite a slot; F9 with a valid existing slot cannot restore position or cancel/discard a pending/held decision, clear a hold or reset skip/auto/read-log state; no shortcut log/Notice. Isolate the key's effects from ordinary same-frame script/persistence work. Player overlay save/load, Continue, import and autosave still work.
- Off: no Monitor-only capture/channel/retained buffer/panel/input installation; normal logging and kira observation still work. On in debug **and forced release**: capture is installed in time for startup/pack diagnostics; showing/hiding the panel never changes dev mode. Retention/visibility details remain the sibling contracts' responsibility.
- On: retain the manual-slot-1 save/load round trip, immediate F5 overwrite, missing/unreadable/refused-load and failed-write behavior, decision cancellation/re-fire, hold recovery, skip discontinuity and replay contracts above. Check same-frame F5+F9 and F5/F9+Esc, and no replay on menu/title closure. Keep shortcuts inert on title/menu/error surfaces.
- Player help/title legend/Settings never gain dev controls. Terminal developer help/docs expose the policy and explicit F5 overwrite warning. Loading, New Game and title/menu/Monitor-visibility transitions do not change the effective gate or persist it.

#### Amendments and trace

**2026-10-11.** Audio settings §1 corrects the runtime's mistaken `--pack` wording to positional PACK_DIR, best-effort canonicalized. Player comfort §6 and Text skip's controls/validation clarify that F5/F9 are developer-only, with the player legend unchanged. README separates player controls from the planned developer launch contract; the historical slice links this gate rather than restating the old F5/F9-only interface.

**2026-10-11.** The launch/config gate and F5/F9 build gate are implemented; the spec text above is unchanged and now records shipped behavior. The single value lives on `RuntimeConfig` (`new` defaults from `cfg!(debug_assertions)`, `with_dev_mode` overrides), is inserted before `LogPlugin`/runtime plugins so the later capture hook can read it, and gates the shortcut system with one run condition. Monitor capture/retention/panel were the owning feature slices at that date; the help surface and README then stated that hidden capture had landed while F3/Monitor was not yet a usable viewer. Both claims were superseded by the Log stream and Layout status notes above and by the assembled rendered walk.

Decision trace: **Q1** positional grammar/default/escape; **Q2/Q8** library default/construction/override; **Q3** app-lifetime gate before plugins; **Q4** developer-only discoverability; **Q5** idempotent force-on; **Q6** strict usage errors without startup; **Q7/Q11** help surface/error precedence; **Q9** debug-assertions definition; **Q10/Q12** disabled shortcut/Monitor boundaries. The enabled shortcut preservation rules derive from the dev-mode requirement and the inspected input/scheduling code, not a new slot design.

#### Out of scope

Panel toggle key, reach, input ownership, initial visibility, log filtering/retention, geometry; a no-dev launch flag; general CLI/developer tooling; slot-system redesign, player save/load changes and feature code. No ADR: these are reversible launch/wiring choices within the map's already-settled boundary, not a qualifying hard-to-reverse architectural trade-off. *Amended 2026-10-11: the launch/config and F5/F9 gates now exist (see the implementation-status note above); Monitor capture/retention/panel and its input ownership remain out of this slice.*

### Log stream

**Implementation status (2026-10-11).** Hidden capture has landed in `app/src/monitor/`: the dev-gated tracing layer composes through `LogPlugin::custom_layer` in `app/src/failure/mod.rs::app_log_layer`, the bounded pending queue and 1,000-line retained history are `MonitorCapture`/`MonitorHistory`, and `RuntimeCorePlugin` ingests up to 128 lines every app update whether or not a panel or `Runtime` exists. The escaped 8,192-byte line bound, the evicted/lost/truncated counters and the real-sink credential proof are in place. **The F3 drawer now consumes that history** through `app/src/drawer/`: a bounded progressive projection over the one shared history (exact case-sensitive `unmei::` targets by default, everything on demand), the settled status footer and the complete older-line reader. The reader's [Interaction](#interaction) scrolling, anchors and expiry ride the same pass; fixing the reader surfaced one latent projection defect from this slice, recorded in the [Interaction](#interaction) status note: the arrivals band was marked examined up to infinity, so a second batch of arrivals above an established band was skipped. The reader's own pass now bounds that band at both ends.

*Settled 2026-10-11*: human-confirmed rounds Q1–Q14, shared-understanding check Q15, and the new redirect-safety branch Q16 exposed by independent review. **Built** — see the implementation-status note above; the checks below were the later build's acceptance list, **not tests run by the design work itself**. Mandatory source-safety build prerequisite: the credential-safe diagnostics gate specified below.

#### Capture, filtering and event identity (Q1, Q3–Q5, Q14)

- Consume [Dev mode](#dev-mode)'s one app-lifetime gate. Off means no Monitor-only capture/channel/history/presentation. When on, compose the additional capture layer through `LogPlugin::custom_layer`, alongside the existing file layer and kira observer in [`app_log_layer`](../app/src/failure/mod.rs); do not replace them or initialize a second global logger.
- Capture starts when that subscriber is installed, before startup/pack diagnostics, regardless of panel visibility or disk-log availability. Pre-subscriber events and window/process failures before logging exists cannot be recovered. The retained stream includes startup, title/menu periods and defensive holds; the interaction ticket decides where the panel may be opened.
- **Preserve the existing upstream filter.** Installed `bevy_log 0.19.1/src/lib.rs:311–315,406–420` places the custom layer behind its global filter, starts from INFO/default directives and merges `RUST_LOG`. Display controls never change that filter or resurrect suppressed events.
- Retain **all upstream-admitted events** in one bounded history. The initial/default display is the exact, case-sensitive predicate `target.starts_with("unmei::")`; show-everything removes only that display predicate. Toggle changes reveal the corresponding surviving history, without clearing it, changing capture or creating a second buffer. Dependency events consume shared capacity even when hidden by the default view.
- Successful save/load events in [`app/src/input/mod.rs`](../app/src/input/mod.rs), and key-check warnings in [`app/src/core/key_check.rs`](../app/src/core/key_check.rs), use implicit module targets and therefore require show-everything. Preserve those targets. `unmei::decision` and `unmei::failure` are explicit and qualify for the default view. Do not imply that the default view contains every app event.
- Capture **events only**, not synthetic span new/enter/exit/close lines. Include named fields recorded on the event, not inherited span fields, source locations or thread identities.
- One source event becomes at most one Monitor line; do not add authored-flow events or deduplicate distinct events. Kira's upstream setup warning and the funnel's failure mirror remain two events; the engine decision warning and app failure warning likewise remain distinct, potentially emitted at different times around a presentation pause. Monitor creates no additional copy of either in the existing sinks.
- Order surviving lines by **successful queue admission** — the insertion of a formatted line into the completed queue, not the earlier formatter reservation. Serial events from one producer keep their order; concurrent producers have no promised causal or callback-start order. Assign the monotonic admission identity and elapsed time at that insertion and never sort by timestamp. Handoff staging stays charged against pending capacity until retained history owns the payload (see the [2026-10-12 amendment](#amendments-trace-and-out-of-scope)).
- Ingest on every app update while enabled, hidden or presentation-paused, independently of runner/menu/hold gates. Loads, New Game and title/menu transitions clear neither history nor counters. When the app loop stops updating, producers may still capture into bounded pending slots, but history cannot advance; overload policy still applies. Exit discards Monitor history and pending payload without waiting for a final drain. No cross-launch retention, new sidecar or persisted Monitor preference.

#### The Monitor line (Q2–Q3, Q7, Q15)

- One diagnostic event is one **logical Monitor line**, irrespective of UI soft wrapping. A prefix shows monotonic elapsed time since capture installation with millisecond precision, LEVEL and the exact target; then the message, exactly once and only when present, followed by the other named event fields in recorded order. Render message once, not again as an ordinary field. A message-less event still shows its fields. Exact separators are implementation freedom; none may introduce physical line breaks or ANSI control sequences.
- Escape literal backslashes, CR/LF/tab, other control characters, Unicode line separators and bidi-control characters across prefix/message/field text. Distinguish a real newline from literal backslash-n; preserve ordinary Unicode. An embedded newline or terminal escape must not manufacture another row or reverse prefix text.
- Bound the **complete escaped UTF-8 output to 8,192 bytes**, including metadata, fields and a visible `[truncated]` suffix. Reserve suffix space, cut only on complete Unicode/escape boundaries and truncate the tail rather than reject an oversized event. An oversized message-less field or target obeys the same full-line limit. Filtering uses original event metadata, never its possibly shortened rendered target.
- Use a bounded writer during field formatting, stop visiting further fields after the limit, and never allocate a complete oversized intermediate string just to truncate it later. Payload formatting occurs inside the tracing callback because event field references do not outlive it. Stored-output limits do **not** impose a deadline on arbitrary `Debug` code or bound memory that such code independently allocates; the unchanged console/file formatting costs remain separate.

#### Authoritative limits, overload and frame work (Q6–Q10, Q13)

| Budget | Fixed contract |
| --- | --- |
| Retained history | **1,000 Monitor lines**, one authoritative cap shared by both display views and consumed unchanged by interaction/layout |
| One complete line | **8,192 UTF-8 bytes**, including prefix, escapes and truncation suffix |
| Pending capture | **256 slots**, one bounded line per slot; includes in-flight formatter reservations and handoff staging until ownership transfers to history |
| Ingestion | At most **128 events per app update**, visible or hidden, never an unlimited catch-up drain |
| Presentation text work | At most **128 line-sized text materializations/replacements per app update**, separately from ingestion; none while hidden |
| Metadata projection | At most **one scan of the 1,000 capped retained entries per app update** |

- Ingesting beyond the retained cap evicts the oldest retained line. No per-target, per-level, visible-only or second-layout cap; soft wraps and local status are not retained log lines. The disk log's uncapped/truncate-per-launch policy is unchanged.
- Reserve pending capacity **before formatting**. Concurrent producers cannot each create an unaccounted 8 KiB Monitor buffer. Full pending capacity evicts the oldest **completed** pending line if it can immediately free a slot; in-flight reservations cannot be evicted. If every slot is reserved, or an admission lock is contended, discard the incoming event instead. No severity lane, blocking wait for space, capacity retry/spin or producer backpressure added by Monitor.
- Preserve order among surviving queued events after any pending eviction. Failed admission creates no line. Reservations must be released on formatting failure, abort or panic unwinding; avoid holding a queue lock while formatting user values.
- Raw text maxima are **7.8125 MiB retained + 2 MiB pending**. Ingestion touches at most **1 MiB of line payload/update**; presentation text work has a separate **1 MiB/update** allowance. These are operation/payload budgets, not measured FPS or millisecond guarantees. Metadata, allocation bookkeeping and separately bounded presentation storage are additional; do not advertise these figures as total process memory.
- Presentation caches reference the shared history and are bounded by that same 1,000-line cap; they are not independent retained history. Bound any staging/copies too: do not create an unbounded/full-snapshot queue when toggles, evictions or refreshes occur. Do not reformat retained event values, concatenate the whole log every frame or rebuild unchanged text.
- A filter change/large refresh is progressive under the presentation budget, with panel-local **`updating view`** status while incomplete. Apply the current predicate, do not present old-filter work as the new view, and cancel obsolete work on a newer selection. Ingestion continues; a busy producer must not force indefinite restart of projection work. Geometry, scroll and follow-tail are sibling decisions; real Bevy text/layout/GPU cost must be checked by the build.
- Three **saturating app-lifetime counters**, unaffected by hiding, filtering, loads or New Game: retained-history evictions (once per line aged out), pending/admission losses (once per discarded event, including completed pending evictions and unavailable/reentrant/formatting-error admission), and successfully admitted truncated events (once per truncated line whose insertion into the completed queue succeeded). A formatted but unsuccessfully inserted truncated candidate is a loss, not a truncation; a truncated line later evicted can contribute to both counters. Ordinary display filtering is not a loss. Count all targets, not just the current view.
- Counters and updating/unavailable state are **panel-local status**, not synthetic retained rows, tracing events, Notices or new failure inventory entries. Layout chooses placement, not meaning. Monitor must never recursively diagnose its own overload or failure through the subscriber.
- Monitor-owned failures fail open for the app: release reservations, count losses where possible, preserve existing history, and expose local unavailable state if capture cannot continue after a poisoned/unavailable queue or disconnected receiver. Never panic on such bookkeeping failures or alter console/file/kira observation. Suppress reentrant Monitor capture with loss accounting; do not recursively format its own diagnostics. No `catch_unwind` or promise to neutralize a panic/infinite computation in arbitrary emitter `Debug`; reservation cleanup works if unwinding occurs, with the emitter's existing panic behavior unchanged.

#### Disk/audio independence and credential-safety prerequisite (Q11–Q12, Q15–Q16)

- Failure to open `<pack-dir>.log` does not disable enabled Monitor capture or the kira observer: the optional file layer, mandatory observer and enabled-only capture are composed independently. Preserve the one console/stderr startup warning, no Notice, and the existing file filtering, truncation and flush contracts. Changing Monitor views/loss accounting never writes to those sinks.
- **No API key may enter any supported sink.** Do not inherit unsafe data as an accepted residual or hide it only in Monitor while console/file remain exposed. Existing source hardening is required before the new viewer ships: **credential-safe diagnostics is a mandatory implementation prerequisite**, not implementation delivered by this section. The later Monitor build handoff must declare its native blocking edge unless the source hardening is already in place; sibling interaction/layout planning is not blocked by its implementation.
- Evidence: the HTTP diagnostic credential-surface inspection distinguishes the actual raw-body-to-fallback-to-two-log-events path from the merely possible client `Debug` exposure. Neither is an observed runtime key leak. Key-check output is already fixed/safe; preserve its wording, with the explicit no-redirect exception below. The inspection is not a completed safety proof.
- The narrow prerequisite redacts `DecisionsClient::Debug`'s key, endpoint and HTTP-client details; uses categorical HTTP/task/decode/invalid-answer diagnostics and status codes, without raw status bodies, URLs, uncontrolled transport/decoder strings or response-derived invalid-action text in emitted or client-returned diagnostics. Do not retain unsafe raw error payloads merely behind a safer `Display`. Keep useful authored context and exact failure-row classification.
- **Trusted network-client provenance:** when the response model exactly equals the requested authored pin, `Decision.model` and successful `ResolvedDecision.model` carry that pin; otherwise they carry the literal **`unverified response model`** (wrapped in `Some` for a successful resolution). Fallback model remains `None`. The existing engine info event explicitly labels the requested pin as **requested**, never silently substitutes it as proof of the responding build. No new event or resolver field is required for that context.
- The network client returns **no server-echoed score legend** as diagnostic provenance (`Decision.legend` is absent). If legend detail is logged, use the authored legend with an explicit **authored** label. Keep numeric cost/score/confidence and validated landing/fallback keys. The pure decoder's landing-selection semantics do not change; document the difference between raw decoding and safe network-client diagnostic provenance.
- Mark authorization headers sensitive on both Decisions and key-check requests, preferably with public `bearer_auth`, and retain/force **connection verbosity off**. Locked reqwest marks bearer headers sensitive, but its opt-in raw-wire TRACE bypasses typed-header redaction. No HTTP-target filter ceiling or global subscriber/redaction refactor: preserve the existing upstream filter. Dependency/HTTP feature/tracing/verbosity changes require renewed safety validation.
- **Redirect-safety exception (Q16):** disable redirects on both clients with `reqwest::redirect::Policy::none()`. Never emit or return uncontrolled Location/redirect-target text or raw 3xx bodies as diagnostics. A Decisions 3xx is a safe categorical status failure taking the authored fallback; a key-check 3xx maps directly to the existing fixed `Unreachable` verdict. No follow-up request/connection, retry or Referer propagation. This deliberately stops endpoints that formerly succeeded only through transparent redirect following: the configured Decisions URL must name the final endpoint. The fixed key-check URL remains unchanged. Removing Authorization on cross-host redirects would not prevent a server-reflected credential in the next connection's logged hostname; wire verbosity off alone was insufficient for this active path (established by inspecting the locked `reqwest` and `http` crate sources, not by a runtime observation).
- Preserve source targets, levels, warning multiplicity, resolver seam, timeout/no-retry, authored fallback and Notice/error-screen posture. Preserve key-check verdict mapping/wording except for the explicit no-redirect change. Safe diagnostic payload/provenance and redirect-policy changes are the **explicit narrow exceptions** to verbatim/current-transport preservation. Update the intentionally affected provenance/error docs and tests: `engine/tests/decision_resolver.rs` currently expects `fake/model` for requested `m`, which becomes a mismatch-marker case; add a matching-pin case. Keep `app/src/failure.rs::decision_rows_follow_the_engine_error_variants`' Display-to-row coverage and row routing intact.
- Validate the supported app/locked HTTP configurations, not arbitrary external emitters deliberately logging secrets. CLI authoring diagnostics are a separate source path, excluded here. No real keys in fixtures, screenshots, issues or prototype artefacts. Failure of the prerequisite's supported-path safety checks blocks shipping, not an acceptable leak with a follow-up note.

#### Deterministic observable validation list (later build)

Follow [the testing page](testing.md); use local subscribers/dummy fixtures and meaningful existing coverage, not process-global logger races or live keys. These checks are contracts to implement, not results observed by the design work.

- Filter fixture: emit explicit `unmei::failure`/`unmei::decision`, implicit app-module, dependency, `unmei` (no trailing `::`) and differently cased targets. Default includes only the exact literal prefix; show-everything reveals surviving earlier admitted context. A globally suppressed event never appears in either view; toggles leave history/order/counters/upstream filter unchanged. Dependency noise counts against the same cap while hidden by the display filter.
- A message-less event includes recorded fields; a multiline/CR/tab/backslash/ANSI/Unicode-separator/bidi fixture produces one logical line with safe, distinguishable escapes and ordinary Unicode preserved. Assert elapsed install-origin/admission timestamp, level and target using an injected clock rather than sleeps; no span-lifecycle or inherited-span lines. A huge target/message/field and multibyte boundary stay at or below 8,192 bytes including `[truncated]`, with valid Unicode and complete escapes. Guard against full intermediate formatting and further-field visits after the cap.
- Inject 1,001 retained events with numbered IDs: exactly 1,000 survive, oldest absent, newest last, one history eviction. Filter switches do not obtain another 1,000 lines. An accepted truncated event increments truncation once; subsequent eviction/loss is counted separately. Counters saturate instead of wrapping.
- Deterministically withhold consumer updates: fill 256 completed pending slots, admit one more, and verify oldest pending removal/recent survival and one loss. Hold all 256 formatter reservations and assert the next producer is immediately rejected without formatting. Exercise contention, formatting-error/reentrant/disconnected/poisoned paths, reservation cleanup during unwinding and independent existing sinks. No unbounded temporary producer payload, Monitor warning/Notice or recursive event.
- With at least 129 pending events, one update ingests at most 128 and leaves the rest; no catch-up on a long update. The same holds hidden, on title/menu and in a defensive hold. Before any updates, startup events can queue. Stopping updates still bounds pending; resuming applies normal budgets. Load/New Game/title preserve history/counters; exit requires no drain or persistence.
- Synchronize multiple producers with controlled barriers: successful admission order is stable, surviving per-producer order is preserved, each event is admitted at most once, and loss counters account for the forced overload. Do not assert an arbitrary total cross-thread causal order or use timing-sensitive sleeps.
- Instrument visible text work: at most 128 line-sized materializations/replacements and one capped metadata scan per update; no hidden text work, unchanged-frame rebuild or retained-field reformatting. Filter changes show bounded marked progress; rapid newer selections cancel obsolete work without false filter attribution, extra history/staging growth or refresh starvation. Verify caps under continuous arrivals/evictions. Real rendered walking later checks text/layout usability, not a promised hard FPS threshold.
- With file open and unavailable, Monitor and kira observation remain independent. Assert one copy per source event in capture and unchanged console/file multiplicity, including kira's two-source warning/mirror and the decision two-source warnings. Overload/filter changes emit no extra existing-sink events; the unwritable-log warning and Notice behavior remain unchanged.
- **Ship gate (credential-safe diagnostics):** dummy client/error Debug/Display, response-body/decoder/invalid-action/model/legend reflection, credential-bearing endpoint errors, safe auth-header construction and permissive TRACE loopback tests inspect both source targets plus console-format/file/candidate-Monitor capture output. Include 301/302/307/308 Location fixtures reflecting dummy credentials in authority and path/query: no second request/connection, emitted Referer, raw Location/redirect target/body or credential in returned diagnostics or any captured sink; Decisions takes fallback and key-check returns `Unreachable` without follow/retry. Matching pin succeeds; mismatch/reflected key becomes the fixed marker; fallback model stays `None`; raw legend is absent and authored legend labelled. Retain direct/trait delegation, pure landing, row routing, fallback/timeout and key-check tests. No production key/network calls.

#### Amendments, trace and out of scope

**2026-10-11.** Runtime failure posture §6 Logging and §7's log test wording are corrected in place from stdout to **console/stderr**, with a dated factual note; no sink behavior changes. [`GLOSSARY.md`](../GLOSSARY.md#failures) clarifies Monitor's launch-wide diagnostic scope and defines Monitor line. The genuinely new HTTP diagnostic boundary — whether a dependency's own diagnostics can bypass safe application messages — was settled by inspecting the locked `reqwest`, `http`, `hyper` and `hyper-util` crate sources; it is the build prerequisite this section records.

**2026-10-12.** Maintainer-approved clarification, recorded here with its date: the repair-planning interview after the Monitor implementation audit approved the completed-line-insertion meaning below. The repair resolves the log-stream admission ambiguity that audit recorded and bounds handoff staging. **Completed-line insertion is queue admission:** a formatter reservation only charges pending capacity; the monotonic admission identity and elapsed millisecond time are assigned when the formatted line is inserted into the completed queue under its lock, and the `truncated` counter follows a *successful* insertion (a truncated candidate that loses the insertion lock, or any other discarded admission, is one `lost`, never a `truncated`). Retained order is therefore completed-insertion order, newest last; serial per-producer order is preserved and no cross-producer causal or callback-start order is promised. Handoff staging counts against the 256-slot pending cap until retained history owns each payload: the drained batch stays charged, releases one slot per line only after [`MonitorHistory::push`](../app/src/monitor/mod.rs), and a dropped batch counts each leftover `lost`. The duplicate raw escaped body is eliminated — the grid's last column is now a byte-range slice of the one stored line — so the **2 MiB pending** and **7.8125 MiB retained** raw-text ceilings are not silently doubled. The original target metadata remains stored separately and is not part of the raw-text ceiling. The fixed-width elapsed field saturates its displayed prefix at ten digits (about 115 days of continuous uptime) while `elapsed_ms` keeps the exact value, so the fixed width keeps the line's byte budget and body offset stable. The 2026-10-11 body, its budgets and every parent limit are otherwise unchanged; this is a clarification, not a scope amendment.

**2026-10-12.** Dated clarification of the [Monitor line](#the-monitor-line-q2q3-q7-q15) rendered by the capture layer, resolving the metadata and explicit-message findings the Monitor implementation audit recorded. **Message placement:** a present `message` field renders exactly once, immediately after the target, before every other named field; an explicitly named `message` declared after another field therefore still leads the body, and the remaining fields keep recorded order. **Target preservation and bridge identity:** an ordinary tracing event keeps its own metadata target. A field that merely shares a `log`-bridge name (`log.target`, `log.module_path`, `log.file`, `log.line`) is an ordinary named field — retained in recorded order, never used to rename the target or hidden. Only a genuine `log`-bridge record, identified by its own bridge callsite (not by field names), has its original target recovered from `log.target` and only its own scaffolding fields omitted. Recovery reads the recovered target as a string through the bridge's normalized metadata (one copy of the target itself, which filtering stores exactly), so it never runs an unbounded `Debug`/`format!` pass over the event's field values. The exact original target remains the filtering/projection input and is stored separately from the escaped line text; the `unmei::` default predicate is never satisfied by a forged field. This is a correction to the rendered-line contract, not a scope amendment.

**2026-10-12.** Dated clarification of the [Log stream](#log-stream) projection and the [Interaction](#interaction) reader display, resolving the projection and reader findings that audit recorded. **One contiguous examined band:** the projection keeps a single `[frontier, ceiling]` band instead of an old block plus a separately tracked arrivals band. The opening pass establishes the band newest-first; later passes extend it **upward** from its last examined arrival (oldest-of-the-new first) and **downward** into older history, so an arrival batch larger than the pass budget resumes exactly where the pass stopped and no arrival between two batches can be mistaken for reviewed. The band still advances by at most the 128-entry metadata budget per update, and a settled band skips the history walk entirely. **Evicted references are pruned while hidden:** [`project_monitor`](../app/src/drawer/mod.rs) drops projection rows retention no longer owns before its visibility check, so a hidden full projection cannot pin evicted payloads even though it performs no hidden text work. **A pending refresh shows no tail:** while a restarted pass is still locating a non-following anchor, the reader window yields nothing and the body shows a local `locating your position…` placeholder under `updating view`, instead of presenting the newest matching rows as the older reader's location. These are corrections to the projection/reader contracts, not scope amendments.

**2026-10-12.** Dated clarification of the [Interaction](#interaction) reader anchoring/navigation and the [Layout](#layout) target column, resolving the anchoring, navigation and target-column findings that audit recorded, with the maintainer's approved meanings recorded here. **Actual laid-out row metrics:** the reading offset and clamp are measured against the anchor row's real laid-out extent, derived per column (the target's fixed 224px and the message/fields column's remaining width) rather than a guessed whole-line height, so a nominal one-line anchor can no longer be clipped by an over-estimated offset; older/newer movement from a fractional offset consumes that offset once instead of double-counting it. **Home during an incomplete refresh:** Home selects the **oldest currently displayed matching event** and keeps that selected event as older matches are prepended; it does not silently retarget to the whole retained history. **Whole escaped target:** a nontruncated event renders its full escaped target, which wraps inside the fixed 150–224px target column; the ordinary display ellipsis is removed. To make the analytical row metric match the laid-out extent, every nontruncated column wraps at any character (the terminal-like `LineBreak`) and each row's laid-out height is set from that metric; wide CJK/emoji glyphs count two cells. A genuine byte-cap truncation still collapses to the preserved warm line with its visible `[truncated]` suffix. **Newer-first rendered oracle:** the rendered acceptance walk derives independent expectations from the parent's newer-first fallback order, exercises a matches-on-both-sides case (first newer) and an earlier-only case (nearest earlier), and observes the actual top-visible event and its within-row offset rather than a nearby fully visible row within a loose pixel tolerance. These are corrections to the reader/layout contracts, not scope amendments. **Human rendered confirmation:** the maintainer's rendered confirmation of the visible target/wrap change was recorded on 2026-10-12.

**2026-10-12.** Dated **evidence correction** of the [real Monitor sink](#log-stream) credential-safety proof, resolving the audit's redirect-evidence finding. No runtime behavior, sink, provider contract or source-hardening rule changes — the audit observed no supported-path leak; the earlier fixture simply could not prove the no-follow it claimed. **Valid reflected redirects:** the 301/302/307/308 fixtures now construct syntactically valid absolute loopback `Location` URLs reflecting the dummy key in **authority (userinfo), path and query**, parse each with a real URL parser and pin the parsed host and port to the intended destination (a doubled-scheme string must not masquerade as a non-follow). **Distinct destination listener:** the destination is a separate live listener; the proof first shows that listener records a request, then asserts it receives **zero** connections/requests through completion, while the origin records exactly one request carrying the sensitive bearer header and no `Referer`. No follow-up connection, forwarded `Authorization`, emitted `Referer` or retry is therefore observed **at the destination**, not inferred from the origin listener's own count. **Positive retained-event proof:** every fixture case registers the categorical `unmei::decision`/`unmei::failure` event it must have produced, and the test asserts each reached the **actual ingested Monitor retained history**, the console-format layer and the file layer with the capture's `lost`/`evicted`/`truncated` counters at zero — a clean absence check is not an empty or dropped capture. Supported diagnostic values (`fallback_reason`, `Debug` and the failure Notice) are asserted clean of the credential and any redirect query; requested-pin trust, the `unverified response model` marker, absent fallback model and authored numeric/landing semantics are unchanged. The engine-level redirect fixture is corrected the same way. **Key-check seam:** this remains the **direct `send_key_check`** proof only; the production key-check URL is fixed and not overridable, so `HttpKeyCheck` has no app-subscriber loopback seam, and no endpoint-injection feature is added to manufacture one.

**2026-10-12.** Dated **repair and revalidation** of the [Interaction](#interaction) input-ownership/draft-suspension boundaries and the no-new-pause clocks, resolving the audit's input-ownership finding. **Repaired defect:** the suspended-edit restore used to decide whether input was still held from the frame's [`InputQuarantine`](../app/src/input/mod.rs) set, but `track_quarantine`'s drain runs **later in the same `PreUpdate` chain**. On the frame a held activation key/button was released, the restore therefore still saw the non-empty set and skipped, then the quarantine drained and the ordinary focused-editing sync (`sync_key_focus`) cleared the valid draft in `Update` before the next frame could restore it. A key draft with a caret and selection was discarded rather than restored after release (pre-repair witness: `draft "" focus None editing false`). The closed-frame restore in [`panel_input`](../app/src/drawer/mod.rs) now decides from the **live** physical state ([`InputQuarantine::has_live_hold`](../app/src/input/mod.rs)), so it restores the field on the release frame before the sync can clear it; a key/button genuinely still down still defers the restore, and the closing frame still keeps its claim. Invalidated/removed/read-only/`Checking`/environment fields clear on the same schedule, and no timed path is frozen. **Revalidation evidence:** the production-widget harness pins a nontrivial caret and tail selection preserved through real Monitor `Tab`/`Enter` focus movement and dismissal, deferred until the held activation key releases; a departed field is not restored onto the live replacement field, a changed but still-editable row keeps the valid draft, and a read-only/`Checking`/environment row clears its stale draft; a committed key's ping, a Notice countdown and an engaged seen skip keep their ordinary clocks under Monitor; and the rendered `monitor_acceptance` walk adds a same-frame F3 + real cursor hit-test press on the rendered New Game row that sets the widget's own `Interaction` to `Pressed` yet cannot start play, plus rendered caret/selection preservation across the draft suspension and close. These are corrections to the input/draft contract and its evidence, not scope amendments.

**2026-10-12.** Dated **production-join evidence** for the [Log stream](#log-stream) sink composition and the [Layout](#layout) startup surface, resolving the remaining audit items that no headless or injected-resource stand-in had witnessed. No runtime behavior, routing, survivor rule or sink multiplicity changes. **Actual failed open:** the unavailable-session-log witness now places a directory at `<pack-dir>.log`, so the entry point's real [`SessionLog::open`](../app/src/failure/mod.rs) returns the `Err` it turns into `LogOpenError` (the earlier test injected the resource). Through the production `LogPlugin` + [`app_log_layer`](../app/src/failure/mod.rs) composition this proves: a healthy pack still builds a `Runtime` and plays console-only, the log path is unchanged (no file was created), the mandatory kira observer and the dev-gated `MonitorCapture`/`MonitorHistory` are both installed, exactly one row-21 warning reaches the shared subscriber and no Notice appears. **Kira observer-to-funnel:** the upstream `bevy_kira_audio::audio_output` setup warning is driven once through the real layer installed by `app_log_layer` (only the source is emitted, never both source and funnel by hand) and then reported by `report_audio_setup_failure`; the source event and the distinct `unmei::failure` row-7 event each survive in the ingested Monitor history exactly once, in source-before-funnel order, with no Notice. **Genuine-capture continuity:** 30 ordinary and one oversized event admitted through the production capture layer — not a seeded history — keep their retained lines and saturating `truncated`/`evicted`/`lost` counters across entering play, hide/reopen, the manual slot round trip and a real player New Game, so the launch-local lifetime is proven on real admissions. **In-window capped startup reading:** the broken-pack acceptance fixture now carries an escaped target (a bidi control and a literal backslash-n) in addition to the 25 numbered unresolvable jumps, so the startup screen's 20-line cap is exceeded while the entire aggregate fits the 8,192-byte logical-line cap. The rendered `--mode broken` walk opens Monitor over the error screen and reads every surviving message off the drawn rows using real `Home`/`PageDown`/`End` reader controls, confirms the escaped target rendered escaped, confirms the aggregate is not truncated, and still restores the app's own non-zero startup exit. The oversized/`[truncated]` case remains witnessed by the walk mode's own oversized fixture. The standalone `unmei-play` binary's window cannot be driven by another process's input, so the runtime F3/F5/F9 consequences of the effective dev policy stay evidenced by the rendered gate walk (shipping `UnmeiPlugin` + shipping `launch::parse`), with the entry point's help/error surfaces covered by the hermetic subprocess suite.

**2026-10-14 (status).** The settled banner above now reads **Built** — the capture layer and the drawer, with its reader, shipped (see the implementation-status note above). No capture, retention or projection contract changed.

Trace: Q1 shared history/projection; Q2 logical line; Q3 metadata/events; Q4 admission order; Q5 background ingestion; Q6 authoritative cap; Q7 byte cap; Q8 pending/update budgets; Q9 overload; Q10 counters; Q11 source safety; Q12 trusted provenance; Q13 presentation work; Q14 lifetime; Q15 exact edge mechanics and publication approval; Q16 no-redirect source-safety exception and amended publication approval. No additional ADR for reversible sizing/wiring choices; the explicit source trust-boundary rationale lives here and in the credential-safety prerequisite.

Out of scope: feature code and safety implementation in this decision resolution; toggle key, initial visibility/reach, opening-pause policy, control/input focus, scrollbar/follow-tail, panel geometry, search, export and command entry, persistent preferences, disk-log rotation/caps, CLI authoring hardening, provider/DSL/save redesign and measured GPU/FPS guarantees. Interaction/layout consume the one cap, projection/lifetime and progressive-status contracts above; they do not invent replacement limits or stop capture.

### Interaction

*Settled 2026-10-11*: human-confirmed Q1–Q22 and complete-contract/publication approval Q23. **Built** — see the implementation-status note below; this settlement implemented no feature code itself. This subsection consumes the settled dev gate and log-stream contract above; layout is decided in [Layout](#layout).

**Implementation status (2026-10-11).** The interaction contract has landed in `app/src/drawer/` and `app/src/input/`: F3 claims app input at a `PreUpdate` resolution point after physical input/UI focus and before focused dispatch and every `Update` action, keeps the claim through the closing frame, and quarantines every key/button held across the transition until release and a fresh press (`app_input_ready`). The suspended Settings draft, drag cancellation on open, window-focus-loss cancellation and the three-control focus cycle are implemented and driven through the production widget/pointer paths in `app/src/tests.rs`. **The older-line reader has landed**: `reader_input` runs after the toggle/close decision (so Esc/Close win the frame), hit-tests the wheel to the log body's own rect, drags a real scrollbar thumb/track, pages with fresh-plus-held PgUp/PgDn and fresh Home/End, ignores conflicting key/wheel intents whole, and cancels its drag on close and window focus loss. A non-following position is a retained admission identity plus a within-row offset measured against the row's actual laid-out extent; `resolve_reader` keeps it for a matching survivor, falls back to the first newer then the nearest preceding match, clamps an evicted anchor to the oldest surviving match and sets the panel-local `older lines expired`, and only End/Follow latest/a deliberate bottom arrival resume following. One latent projection defect surfaced by this reader is fixed in the same file: the arrivals band was stored as `[new_frontier, ∞)`, which marked every later arrival above it as already examined; it is now the bounded `[new_frontier, new_top]` band with a sorted insert, so continuous arrivals keep projecting. One integration fix rode the earlier slice: `EditableTextSystems` is part of `UiSystems::Content`, so the input-ownership seam's drain ordering had to move into `Content` (no `.after(FocusChangeEvents)`), which removed a real `PostUpdate` schedule cycle the full `DefaultPlugins` app hit at startup.

#### Observation, ownership and reach (Q1–Q4)

- **No new presentation pause.** Opening Monitor observes the live runtime: it does not set or clear the title/menu pause, cancel a Decisions request, replace the runner position or release a defensive hold. Underlying menu/title/hold rules still apply. With ordinary play otherwise live, reveal, auto advance and decision completion continue; closing may reveal a scene that has moved on. Decisions completed over an existing title/menu pause remain held until that pause ends, not until Monitor closes. Existing audio, credential checking, Notice lifetime, read-log and autosave rules remain governed by their ordinary runtime conditions.
- **Monitor owns app input across the window while open**, independently of the no-new-pause choice. Inspection must not also advance dialogue, choose an option, activate/navigate an underlying menu, type/commit an API key or quit the startup-error screen. There are **no app-hotkey exceptions**: F5/F9 and F11 are inert too (Q7). OS window close/Alt-F4 remains available.
- **Reach every runtime surface** when dev mode is enabled: ordinary play, player-choice and decision waits, title, menu/confirmation, Settings including key editing, Gallery/grid/still, Timeline, startup error and defensive hold. Opening changes neither the underlying surface nor its selection/confirmation. Availability cannot depend on a loaded Runtime or on the menu being closed. OS/process/window failures before an app surface exists are not recoverable through Monitor.
- **Initially closed on every enabled launch**, opened explicitly; no automatic opening on failures. Disabled dev mode still installs no Monitor plumbing. Hidden startup capture remains the log-stream contract, not dependent on visibility.

#### Controls, focus and transition precedence (Q5–Q11)

- **F3** is the fixed developer-only toggle, avoiding F1/F2 render-debug and existing app bindings. Close with F3, Esc or the Monitor's Close button. Esc dismisses only Monitor: it does not navigate/back/cancel the underlying surface or quit a startup error. Outside clicks are inert; no implicit outside-click close. Keep OS close behavior unchanged.
- **Transition input wins for the whole frame** (Q10): resolve Monitor input ownership before every underlying keyboard/mouse/widget path, including startup-error quit and Settings text/pointer observers. Opening with F3+F5/Space/click must not also save/advance/choose/quit; closing with Esc+Enter or a Close click must not act underneath. This is input isolation, not a one-frame suspension of ordinary timers, script driving or Decision polling.
- **Settings edit suspension** (Q8): preserve the uncommitted draft/caret without committing, logging, persisting or discarding merely because Monitor temporarily owns focus. Block typing/paste and the ordinary outside-press/focus-loss discard handlers while suspended. Restore editing on close only if the underlying field still exists and is currently editable; otherwise clear the invalidated secret draft and use the current view's safe focus, without recreating a departed view or forcing a Checking/environment row into editing (Q20). Valid restoration preserves caret and selection and triggers no credential read/write/check. Restore the underlying surface/selection/confirmation rather than treating Monitor as a menu navigation.
- **Existing skip stays live** (Q9): Monitor controls neither end seen skip nor toggle auto. A force hold armed beforehand keeps running while aggregate Ctrl remains down and the ordinary menu/hold gates permit it. Releases are mirrored even under Monitor; Ctrl first pressed there never arms force skip, including on closure. Opening/closing is not a position discontinuity. Natural skip stopping and actual load/New Game/title→play/Timeline resets remain unchanged.
- Operate the labelled **Show everything** checkbox by pointer or keyboard: Tab/Shift+Tab navigates Monitor controls; Enter/Space activates the focused control. No new mnemonic/search/command shortcut. It only selects the settled display predicate; progressive refresh and `updating view` remain the log-stream contract (Q11).

#### Reading, lifetime and window focus (Q12–Q14)

- Wheel/trackpad scroll is hit-tested to the log body; provide a draggable scrollbar. PgUp/PgDn and Home/End work anywhere in Monitor; Home goes to the oldest displayed line, End to the newest. During an incomplete refresh Home selects the oldest **currently displayed** matching event and keeps it as older matches are prepended (it does not retarget to the whole retained history). Outside-panel wheel and clicks are swallowed, not passed to the game, Settings sliders or Gallery/Timeline. Exact geometry belongs to [Layout](#layout).
- Initially **follow latest**. Scrolling away disables following; new messages never pull an older-lines reader to the tail. Reaching the bottom or the explicit **Follow latest** control resumes following. Stable event anchors and filter fallbacks below apply; there is no replacement history cap.
- Visibility, display filter and reader position/follow choice last **this launch**, across hiding/reopening, New Game/load/title/menu/Timeline and externally triggered transitions. Underlying actions retain their own session mutation/cancellation contracts; Monitor does not block or undo them merely by existing. Fullscreen/resize recomputes layout without resetting reader choices; F11 is not a Monitor hotkey exception. Next launch starts closed, exact `unmei::` view, following latest. No persistence/sidecar (Q13).
- Window focus loss preserves visibility/filter/reader choices, cancels active pointer drags and clears transient Monitor activation input; no activation is replayed on regain. It adds no new gameplay pause. Existing force-release behavior still applies; auto/seen skip/Decision behavior keeps its ordinary semantics (Q14).

#### Stable reading anchors (Q15–Q16, Q22)

- While follow is off, anchor to the **top visible retained event plus within-row reading offset** measured against that row's actual laid-out extent, not an unstable display index or pixel count. New arrivals do not move that anchor. Preserve it across hiding, transitions and layout/soft-wrap changes; clamp the offset when the same event rewraps. No parallel text history or evicted-line resurrection.
- If retention evicts the anchor, clamp to the **oldest surviving event matching the current display predicate**, keep follow off and set panel-local **`older lines expired`** status. Repeated evictions keep it set. Clear it on deliberate log navigation (wheel/scrollbar/PgUp/PgDn/Home/End), filter change or Follow latest, not on hide/reopen or a timer. It is not a tracing event, retained line, Notice or new failure class.
- Filter changes preserve the follow choice. Following readers follow the newest matching event as the bounded projection becomes available. Non-following readers retain the anchor if it matches; otherwise use the **first surviving matching newer event**, else the **nearest preceding match**. No matches means an empty view, not a filter reset or loss of history. An empty view gaining matching lines does not silently enable follow.
- An automatic anchor fallback/clamp at the bottom does **not** enable following. Only deliberate scroll-to-bottom, End or Follow latest does. During progressive filter refresh keep the current predicate, mark `updating view`, cancel obsolete projection work and preserve the selected anchor/follow intent without exceeding the log-stream budgets. Anchor comparisons use retained admission order, not timestamps; exact internal identity/offset representation remains implementation freedom.

#### Fresh input and bounded focus (Q17–Q21)

- First opening focuses **Show everything**. Tab/Shift+Tab wraps within **Show everything → Follow latest → Close**; reopen remembers the last valid Monitor control. Pointer activation focuses that control; log/background clicks cannot focus an underlying widget. PgUp/PgDn/Home/End always address the log, independent of control focus. Arrows and unrelated keys have no underlying effect.
- F3, Esc, checkbox/button activation and Home/End act **once per fresh non-repeat press**. PgUp/PgDn may use OS repeat for paging. F3 has priority: **closed + F3 + Esc opens once; open + F3 + Esc closes once**. Otherwise Esc/Close beats filter/scroll/control activation for that frame. Ignore simultaneous conflicting navigation/scroll commands rather than applying them unpredictably. Do not interpret holding a toggle or activation key as repeated activation.
- Quarantine controls pressed/held across Monitor ownership/transition frames until release and a **fresh press**: no delayed advance/choice/save/load/menu navigation/text insertion or commit/paste/drag/force-skip arming after close. Do not replay buffered keyboard, text, clipboard or widget/pointer interactions. Closing-frame isolation alone is insufficient. Restore a suspended text field's keyboard focus only after activation keys/buttons release; repeat must not commit an Enter-held draft or insert held Space. Restore other underlying focus/selection only if still valid, otherwise use the current view's safe focus.
- The **already-armed pre-Monitor aggregate Ctrl hold is the one held-input exception**: it stays live while physically down, subject to the existing gates, and release clears it even with Monitor open. A Ctrl first pressed inside Monitor or on a claimed transition frame cannot arm on close or by OS repeat. Keep physical release tracking separate from new-action ownership; never solve isolation by freezing the whole input resource or ordinary script/timer pipeline.
- Opening Monitor cancels an already-active **underlying Settings pointer drag**, preserving the last value applied before the transition frame, with no rollback, extra adjustment or Monitor-triggered flush. It cannot resume on close: release and a fresh pointer press are required. Monitor scrollbar drags are cancelled on close or OS focus loss; held pointer state cannot become an underlying drag (Q21).
- Focus loss preserves view state but cancels drags/transient activation; regain/repeat is not a fresh activation. The release/fresh-input rule must protect Monitor and suspended underlying widgets even where platform-generated focus input differs. No new general focus-based game pause or promise that a physically held Ctrl auto-resumes on regain: Bevy 0.19 turns focus loss into synthesized key releases, so a key cannot stay stuck, but a press made while a gate is closed is still `pressed` on the frame play resumes.

#### Transition summary

| State/transition | Observable Monitor contract |
| --- | --- |
| Live say/choice/decision wait | Own input, not runtime progress. Reveal/auto/armed skip follow their existing rules; a Decision completes normally and may move the scene. A choice is never picked by Monitor input. |
| Title/menu/Settings/Gallery/still/Timeline/confirmation | Preserve the underlying surface, selection and pause. Decision answers remain held by that pause; closing Monitor alone does not apply them or navigate back. Suspended key editing uses the rules above. |
| Startup error / defensive hold | Available without a loaded Runtime; block error-screen quit input while open/transitioning. Closing does not quit or release a hold. After release/fresh input, existing quit or menu/load recovery remains available. |
| New Game / load / Timeline jump / title transitions | Preserve Monitor visibility/filter/anchor/follow. Actual underlying transition retains its ordinary cancellation, held-answer discard, clocks/skip reset, read-log and replay behavior; no stale field/focus resurrection. History/counters remain launch-wide. |
| Resize/fullscreen / OS focus loss | Reflow without resetting reader choices; preserve stable anchor. F11 stays blocked under Monitor. Focus loss cancels drags and stale activation but not visibility, reader state or ordinary timed runtime behavior. |

#### Observable validation list (later build)

These are **specified acceptance checks, not runtime tests run by the interaction design**. Use the [testing guidance](testing.md), deterministic resolver/input fixtures and a real Bevy rendered walk; no network/key required. Protect observable isolation through the raw-input, focused-widget and pointer-observer paths, not just a mock focus value.

- Enabled debug and forced-release launches start closed; F3 opens on every surface listed above, including no-Runtime startup failure. Disabled dev mode installs no toggle/panel/capture, and F3 does no Monitor work. No automatic failure opening or player-help/Settings developer hints.
- Drive a controllable Decision through Monitor over live play: completion applies/parks normally, retaining existing model/fallback autosave and failure routing. Repeat over menu/title: resolution is held, Monitor close alone leaves it held, underlying resume applies once. Opening/closing never cancels/discards/re-fires a request, changes state/staging or releases a defensive hold by itself; isolate this claim from normal live progress.
- Age reveal, auto dwell, Notice and seen/armed-force skip while Monitor is open: they retain ordinary play/menu/title/hold behavior, with no Monitor freeze or catch-up clock. Verify audio and credential-check polling continue under their existing contracts, and live scene changes remain visible behind the viewer where layout permits.
- From a fully revealed say-line, player choice, menu confirm, Settings, Gallery still, Timeline and startup error, inject Space/Enter/digits/arrows/A/Tab/Ctrl/F5/F9/F11/clicks/wheel. Only Monitor controls act: no advance/choice/back/confirm/slider/key mutation/slot read-write/quit or Notice caused by blocked input. Underlying timed work is not falsely asserted absent.
- Overlap: F3+F5/F9/Space/Enter/digit/click/Esc from closed takes precedence; closed F3+Esc opens once, open F3+Esc closes once. Esc+Enter/Space/checkbox click and Close+choice/error-screen click only dismiss. Next fresh Esc has the original underlying back/menu/quit behavior; preserve ordinary F5-before-F9/F5-F9-before-Esc ordering when Monitor does not claim input.
- Production-shaped held/repeat fixture: hold/repeat F3 or Enter/Space through open/close; no oscillation, multiple checkbox/Close activation or underlying repeat insertion/commit. Paging repeat works; conflicting scroll/navigation is ignored. Held mouse cannot click through or resume a drag. Pre-armed Ctrl continues and its release is mirrored; Ctrl first pressed inside/opening/closing does not arm afterward until release/fresh press. OS focus-loss/regain/repeat cannot manufacture a fresh activation.
- Edit a **dummy** API-key draft with caret/selection, open Monitor, type/paste/click/close and wait for activation release: unchanged draft restores only to a still-editable field, no credential read/write/check or secret diagnostic. Changed/removed/read-only/Checking field discards the invalidated draft and uses safe current focus. Inspect production focus-discard/outside-press paths as well as core state; never use a real key in tests/screenshots.
- Open during a Settings slider drag: last previously applied value remains, no further adjustment/flush from Monitor; close while still held does not resume it. New drag requires release/fresh press. OS focus loss cancels Monitor scrollbar drag; regain does not activate/quit/type underneath.
- Tab/Shift+Tab stays in the three-control cycle; first/reopen/pointer focus and valid/invalid underlying restoration follow the contract. Show everything works by pointer and Enter/Space, preserves shared history/filter semantics and marked bounded refresh; background pointer/wheel cannot reach underlying controls.
- Reader fixture with numbered admitted events: follow tracks newest; wheel/PgUp/Home away freezes the anchor across arrivals. Wheel hit-tests only the log body; scrollbar/PgDn/End/Follow latest work; deliberate bottom arrival resumes follow. Evict the anchored event: oldest matching survivor, follow off, local expiry indication until specified reader acknowledgement. Hide/reopen and repeated eviction do not clear it.
- Filter fixture: surviving anchor, first newer match, no newer/nearest earlier, empty view and later matching arrival. Preserve follow choice, never enable it on automatic bottom fallback, never show old-predicate rows during refresh, and cancel obsolete refresh under continuing arrivals within the log stream's budgets. Resize/soft-wrap retains the same event and clamps within-line offset without tail jumps.
- Hide/reopen, New Game/load/title/menu/Timeline transitions and fullscreen preserve Monitor reader state/history/counters; underlying transitions still perform their documented session resets. Window focus loss preserves viewer state, clears stale activation and adds no new gameplay pause. App exit/relaunch resets visibility/filter/follow/focus/expiry; no Monitor sidecar/preferences.
- Real rendered walk at the supported reference size/fullscreen validates topmost input hit-testing over menu/text widgets/error screen, long wrapped event reading, anchor stability, keyboard/pointer scrolling, focus restoration and progressive refresh. Geometry/type/color approval belongs to [Layout](#layout); prototype proportions are not proof that the Bevy paths work.

#### Amendments, trace and out of scope

**2026-10-11.** Dev-mode discoverability now names F3; enabled F5/F9 guards and same-frame order explicitly defer to Monitor ownership. Menu/input, comfort hotkeys, startup quit and Text skip wording below/in place gains the topmost-input exception, without changing the underlying pause or failure posture. [Title/menu docs](title-and-menu.md) qualify their input rules and distinguish suspended key editing from ordinary focus-loss discard. README then recorded the developer controls as planned, explicitly not current implementation; they have since landed (see the Layout status note). [Glossary](../GLOSSARY.md#failures) sharpens Monitor as an everywhere-reachable non-pausing viewer.

**2026-10-14 (status).** The settled banner above now reads **Built** — the interaction contract and its older-line reader landed (see this subsection's implementation-status note). No interaction rule changed.

Trace: Q1 live observation; Q2 exclusive input; Q3 reach; Q4 initially closed; Q5 F3; Q6 close affordances; Q7 no hotkey exceptions; Q8 draft suspension; Q9 pre-armed Ctrl; Q10 full-frame input precedence; Q11 filter control; Q12 scroll/follow; Q13 launch-local lifetime; Q14 focus loss; Q15 event/rewrap/eviction anchor; Q16 filter fallback; Q17 release/fresh quarantine; Q18 repeat/close overlap; Q19 bounded focus; Q20 invalidated draft; Q21 existing drag cancellation; Q22 expiry acknowledgement; Q23 complete-contract approval and local docs-only resolution/publication. No new ADR: these are reversible viewer interaction choices under the established architecture, not a qualifying hard-to-reverse trade-off.

Out of scope: feature code; log-stream/capture/retention/source-hardening redesign; dev parsing or F5/F9 build-gate implementation; precise geometry/type/color (settled by [Layout](#layout), amended 2026-10-11); search, export and commands, persistent preferences, new player controls/failure routing, general input/menu bug fixing, key rebinding/gamepad or new general window-focus pause. Monitor-specific masking/suspension is required; pre-existing unrelated menu overlap/Checking behavior is not silently redefined.

### Reading anchor — decided seam (2026-10-13)

*Settled 2026-10-13*, part of the architecture-deepening map: human-confirmed Q1–Q6 through three interview rounds, with complete-design and docs-only publication approval Q7. **Implemented 2026-10-15:** the reader now lives in `app/src/monitor_reader/` as the plain private `Reader` inside the existing `MonitorPanel`, owning navigation, reconciliation and the one shared row-metric derivation behind a borrowed frame and a read-only view; the settled design below is the living contract. The reading behaviour remains the binding interaction resolution and [Stable reading anchors](#stable-reading-anchors-q15q16-q22) contract above; this section decides its implementation/test seam, not new behaviour.

#### Ownership and storage

- One app-private, Bevy-free **reader module**, proposed home `app/src/monitor_reader.rs`, owns reader navigation and reconciliation together: logical position, follow choice, expiry acknowledgement and locating state. Extracting only the private `wrap_lines`, `row_height_units`, `bottom_anchor`, `clamp_row_offset`, `walk_older` and `walk_newer` helpers while leaving those rules distributed in the drawer was rejected; keeping the coherent module private inside `drawer.rs` was also not selected (Q1).
- A plain `Reader` value lives inside the existing `MonitorPanel` resource; its anchor/follow/expiry/locating fields are private, changed only through reader operations. Logical ownership moves without a new resource, registration, schedule phase or duplicated state (Q2).
- `drawer.rs` retains visibility, filter selection, focus and held-input quarantine; real input precedence, wheel hit testing, page-repeat timing, scrollbar drag lifetime/focus-loss cancellation; capture/projection integration; text/widget/Node synchronization, status wording and pixel application. It calls the reader instead of manipulating reader fields. Capture, retention, projection/admission, their budgets and widget-sync redesign remain outside this extraction.
- The reader borrows the current projection; it neither retains rows/payloads nor owns another history, filter pass or capture stream. `MonitorProjection` keeps progressive work, filtering, pruning and work accounting. Existing named systems and their ordering remain declarative: `reader_input` follows panel transition resolution; projection precedes reader reconciliation and the existing UI/row/control/status syncs. Do not fuse schedules or change input/timer ownership to obtain a smaller parameter list.

#### Interface and shared metrics

The interface is **concrete synchronous reader methods plus a read-only reader view**, not a World lookup, trait framework, effect/executor round trip or action queue (Q4). Illustrative names are not an exact method-count requirement:

- A short-lived borrowed frame supplies ordered projected `MonitorLine` rows, current layout numbers, the oldest retained identity, whether projection is updating, and whether its examined band already proves the oldest matching survivor. All facts come from the same current projection/history view; callers do not invent completeness or duplicate reconciliation. Preserve the distinction between age completeness and a fully settled filter pass.
- Operations cover reconciliation, older/newer motion in row units, Home, End/Follow latest, normalized scrollbar placement, and filter/hidden notifications needed by the existing lifetime rules. The drawer translates eligible real input into these operations. Deliberate navigation and automatic reconciliation remain distinct; the reader, not each caller, decides acknowledgement and return to following. Hidden notifications preserve anchor/follow/expiry, with existing transient locating handling; no hidden projection or text work is introduced.
- The read-only view supplies logical anchor/follow/expiry/locating state, the visible row range, within-row shift, short-tail spacer and normalized thumb size/top. The reader owns these pure calculations; the drawer materializes the range and converts the results to the existing pixel/Node representation. No retained text copy or extra full-history/formatting pass is introduced. Preserve the current display-row cap, viewport units and scrollbar mapping rather than introducing height-weighted travel or other observable changes.
- **One reader-owned pure metric derivation** covers cell wrapping, per-column row extent and conversion to the row units used by walks/clamps and the scrollbar (Q3). The drawer supplies current column capacities, line pitch/padding and viewport extent, including the existing headless fallback, from its geometry/font/window policy. The same derived extents drive imposed row height, surviving-row rewrap, shift/spacer, clamp and reader navigation: never separate reader and render formulas.
- **Frozen metric basis:** current `row_text_lines`/`row_height_units` use analytical display-cell counts and column capacities; `sync_panel_rows` imposes those derived heights. They are not feedback from Bevy's glyph layout. Preserve existing formula, rounding, constants and offset epsilon during extraction. Rendered measurements remain independent evidence that the stored event is actually top-visible and its offset is inside the drawn row; do not redefine that oracle to equal the formula or silently replace the formula with glyph feedback.

**Composition with the presentation-surface partition.** The reading anchor resolves first. Reader-owned metric derivation is fixed; any later surface partition owns layout/font/window inputs and rendering, and consumes this same derivation instead of introducing a second formula or owning reader state. The partition's brief excludes the drawer's own widget syncs; composition does not enlarge either settlement into their redesign. [ADR 0012](adr/0012-menu-and-live-session-transition-ownership.md)'s session-recipe versus presentation-state rule remains untouched; this extraction does not move session resets or applied settings state.

#### Test surface and preserved rendered evidence

Tests cross the **same production reader interface** as the drawer (Q5). Construct fractional positions with navigation/normalized scrollbar placement and clamp scenarios by establishing a position in a taller row before rewrap; do not add an unchecked public setter solely to reproduce an old test seed. Remove `set_anchor_for_test` and reader-only test-state doors. If a formerly seeded scenario cannot be migrated without losing distinct regression protection, surface that obstacle rather than silently retaining the setter, deleting the witness or weakening its assertion.

Replace genuinely redundant algebra-only app cases with deterministic reader-interface cases, not blanket removal of the reader integration block. Keep or rewrite distinct witnesses for body-only wheel hit testing, held paging/conflicting intent/close precedence, scrollbar activation and cancellation, projection budgets, imposed row/shift agreement and launch-local position/expiry through session transitions. Existing private drawer/projection accessors unrelated to reader state are not a cleanup target.

Keep the small public panel-observation facade needed by external examples: `MonitorPanel::anchor()` delegates to `Reader`, and `MonitorAnchor` retains admission identity/offset meaning and the existing public path; `MonitorRow` retains the drawn row's public identity (Q6). The reader module/methods/full view remain app-private (`pub(crate)` where necessary). Production renderer/status reads replace reader-only dead-code/test accessors; no public mutation or test-seeding surface is added.

Both `monitor_acceptance` and `monitor_smoke` keep comparing stored identity against the independently observed **drawn top-visible row**, with fractional offset strictly inside its drawn extent. Retain `monitor_acceptance`'s one-frame synthetic page-key hold, measurement one frame after application, real retained-arrival witness and slow-frame pacing checks. A green pure reader suite is not a replacement for these witnesses, or permission to alter the rendered expectations.

#### Validation required of the implementation handoff

These are **future checks, not tests run by the reading-anchor design**. Follow the [testing guidance](testing.md); use minimal deterministic fixtures, independently expected results and no network/key.

- Direct reader-interface cases pin fractional older-walk consumption exactly once (e.g. single-line `42 + 0.5`, three units older → `39 + 0.5`), newer motion and deliberate bottom arrival, automatic bottom fallback staying non-following, multi-column wrapping/row-unit derivation, offset clamp on rewrap and a fitting tail. Protect observable operations/results, not six helpers mechanically.
- Reconciliation cases pin surviving identity, eviction to the oldest matching survivor only when age completeness proves it, local expiry acknowledgement versus hiding/repeated eviction, same/first-newer/nearest-earlier filter fallbacks, empty view followed by matching arrivals without auto-follow, and incomplete projection locating without presenting the matching tail as the reader's position. Keep Home's oldest-currently-projected event stable while older matches are prepended.
- Preserve the distinct real-system/input/projection/row-layout/session witnesses listed above and the shipping-schedule witness. Re-point the existing systems at the real module; do not mock the behavior being extracted. Confirm no extra retained payloads, hidden projection/text work or breach of the settled scan/text budgets.
- Run the focused suite (`cargo test --locked -p unmei-app --lib monitor_reader`, if that is the chosen module name), then `cargo fmt --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings` and `cargo test --locked --workspace`, including `app/tests/shipping_schedule.rs`. Report actual results rather than assuming an unchanged schedule is valid.
- Run the preserved rendered walks: `cargo run -p unmei-app --example monitor_acceptance`, then the same command with `-- --mode hold`, `-- --mode broken` (its intentional exit 1) and `-- --mode gate`; and `cargo run -p unmei-app --example monitor_smoke`. Repeat smoke with `MONITOR_SMOKE_FRAME_MS=150` and `MONITOR_SMOKE_FRAME_MS=400`. Retain supported reference/wide/fullscreen, wrapped-row, filter/expiry and cross-surface checks, including existing dev-gate coverage; headless success alone does not establish drawn geometry or shipping startup.

#### Scope, verification risks and trace

No new behavioural defect was established by this planning inspection. The analytical-versus-glyph basis above is an explicit verification risk, not a repaired defect. Preserve current metric inputs (including the documented 8px scrollbar gap in `ROW_CHROME_PX`); if implementation/rendered evidence establishes a mismatch, record it and seek an explicit contract decision before changing geometry or reader behaviour. The `anchor == None`, empty-current-projection branch during a progressive refresh is likewise a scenario to verify, not an established false-empty defect or permission to change the settled empty-view rule. Unrelated elapsed-column overflow speculation was not substantiated and is not a reader repair requirement. Any genuine defect found later must be carried explicitly in the implementation handoff, never fixed silently under this extraction.

Trace: Q1 separate coherent reader module; Q2 plain embedded state; Q3 reader-owned shared metrics; Q4 concrete synchronous methods/read-only view; Q5 real-interface tests and scoped test-door removal; Q6 public anchor facade/internal reader visibility; Q7 complete-design confirmation and docs-only resolution. No glossary change: **Monitor reading anchor** remains the same domain concept. No ADR: this reversible app-local seam does not add a new hard-to-reverse ownership contract beyond the settled Monitor rules. **Reconciled 2026-10-17:** the separate implementation handoff landed on 2026-10-15; this trace's planning-time "handoff due at map close" status no longer applies. No capture/admission/retention contract, interaction behaviour, geometry/typography, session recipe, production code, test code or journey artefact changes in this resolution.

### Layout

*Settled 2026-10-11*: human-confirmed verdict through the interview form after the browser prototype walk. Primary source: a throwaway prototype branch holding `app/prototype/monitor.html`, its `pre-*` shots and the losing B/C alternatives; never merged to main. **Built** — the drawer it specifies shipped, with its reader and the rendered acceptance walk; this settlement implemented no feature code itself. This subsection consumes the settled dev gate, log-stream and interaction contracts and decides **geometry, typography, tone and chrome placement only**. Browser evidence is reference; the real Bevy rendered surface was validated by the later build's acceptance walk.

**Implementation status (2026-10-11).** The approved variant-A drawer is built in `app/src/drawer/` from fixed px values, and `app/examples/monitor_smoke.rs` measures it on a real Bevy window/renderer at both reference sizes. Observed at 1280×720: panel `x 24 y 212 w 1232 h 360`, 24px side insets, 148px bottom reservation, header/body/footer `52/293/13` inside the 1px panel border, row columns `72/46/224`, single-line row 20.0px for the 12px/1.45 Fira Mono line. Observed at 1920×1080: panel `x 24 y 572 w 1872 h 360` with the same height, insets, reservation and type — the fixed-px contract with no width cap. The reader's chrome rides inside the same body: a flexible viewport column with a top gap spacer and a rows group shifted by the within-row offset, and a fixed 10px scrollbar track with a draggable thumb; the body's earlier 8px vertical padding was removed so the documented ≈15 single-line rows fill the full 293px content box. The screenshot-based human walk and the remaining surfaces/reflow checks stay with the rendered acceptance walk; the browser prototype is not the proof.

**Walk-found fix (2026-10-11, from the rendered acceptance walk).** The human walk over the live drawer caught two horizontal-overflow defects the single-row fixtures had hidden: a long message ran past the panel instead of wrapping, and a long module path drew over the message column. Both are fixed. The message column now shrinks to the space left in the row and wraps (a row grows past one line); the log body takes exactly the space between the header and the footer, so a taller wrapped row clips inside it and can never squeeze the header. A target longer than the fixed 224px column is shortened to its escaped tail under a `…`, cut on whole escape sequences, while the raw target still drives projection and filtering. `monitor_smoke` now emits a 296-character message and a long target alongside the short fixture, and asserts on the text pipeline's own laid-out glyph size: the message's glyphs fit inside its column and the long target's fit inside 224px; the short fixture stays one line.

**Implementation status (2026-10-11).** The assembled rendered acceptance walk has landed as `app/examples/monitor_acceptance/` (a cargo example directory), and the assembled product's rendered acceptance has landed; **human confirmation of that walk was recorded on 2026-10-12** — the manual live drawer pass exercised every action successfully. The walk boots the shipping `UnmeiPlugin` on a real window and GPU over a generated fixture pack (`target/monitor-acceptance/pack`, no credentials and no network: the one `decide` point answers through a gated resolver the walk releases itself) and drives it with real input messages. Three modes:

```sh
cargo run -p unmei-app --example monitor_acceptance                  # surfaces, reader, styles (293 checks)
cargo run -p unmei-app --example monitor_acceptance -- --mode hold   # defensive hold (18 checks)
cargo run -p unmei-app --example monitor_acceptance -- --mode broken # startup error (23 checks; exits 1)
cargo run -p unmei-app --example monitor_acceptance -- --mode gate   # the dev gate at this build's default
```

**Evidence preservation (2026-10-16).** The 17 rescued PNGs are preserved with their source provenance, sizes and SHA-256 checksums. This is byte-preservation of historical artifacts, not a new acceptance run or verification of current HEAD. The local originals remain retained until the maintainer commits and verifies the backup; the record notes the unresolved capture-revision and remote-verification limits.

Screenshots and the fixture pack land in the gitignored `target/monitor-acceptance/`. The walk measures the approved geometry at 1280x720 (panel `x 24 y 212 w 1232 h 360`, header/body/footer `52/293/13`, columns `72/46/224`, 10px gaps, 1px row padding, single-line rows measuring 20.0px against the settled ~19.4px line unit (real font line-box rounding), no scrim), at a 1920x1080 window (panel `w 1872`, the D10 width contract) and on a real borderless-fullscreen frame at this machine's native 2560x1440 (panel `w 2512`, fixed px). It asserts the INFO/WARN/ERROR tones, the newest-**retained** marker (absent while that event is filtered out, present under Show everything), the collapsed `[truncated]` row, the escaped/Unicode/message-less/long-target/long-wrapped fixtures and the 1,000-line cap with its marked `updating view` refresh; walks the reader (wheel, PgUp, real scrollbar drag, Home/End, stable anchors across a filter change, expiry acknowledgement); opens Monitor over the title, a say-line, the player-choice and decision waits, the overlay, Settings (a typed dummy draft, the environment and `Checking…` rows), the Gallery grid and still, the Timeline and a live-session confirmation, checking the underlying surface is unmoved each time and that Space/Enter/digits under Monitor never act; walks a defensive hold and the startup error screen without a `Runtime`; and measures the recorded startup-error overlap (error list `y 52..557` against the drawer top `212`, with the whole 26-error list preserved (25 unresolvable jumps plus the missing `npcs/` directory) and readable from `broken.log`, and carried as one escaped `unmei::failure` row in Monitor itself). `--dev` resolves through the shipping `launch::parse`, so a release run exercises the binary's own force-on mapping; `--mode gate` leaves the build default in place and asserts F3/F5 accordingly, and the `release-assert` profile (release optimization, debug assertions on) covers the custom-assertions row. The existing `app/tests/credential_safety.rs` proof still runs the **real** capture layer and retained history under permissive TRACE (both source targets, console-format, file and Monitor outputs, 301/302/307/308 reflected-Location cases, no follow-up request), now on the final dependency/features/verbosity state.

**Typed fallback reason (2026-10-13).** The walk's gated resolver now reports the engine's own `DecisionError::Task` when the gate is dropped or already taken, so the walk's fallback log line reads `decision task did not complete` where it previously carried the fixture's own string. No walk step asserts that text and no other walk expectation changes; a human walker at the drawer should expect that line. The reason crosses the resolver seam as the error itself, so the app classifies the row by variant instead of matching the `Display` text.

**Evidence correction (2026-10-11).** The earlier `monitor_smoke` wide-frame step poked `window.mode` directly, which the app's `sync_window_mode` reverted on the next frame from the stored `play.fullscreen` value; only the 1920x1080 resolution change stuck, so its "fullscreen" frame was really a windowed 1920x1080 one. The step now goes borderless through the shipping Settings toggle, and the smoke's wide frame is a real fullscreen one (native 2560x1440 here, panel `w 2512`); the 1920x1080 width contract (panel `w 1872`) is measured explicitly by the acceptance walk. No geometry claim changes.

**Walk-found fix (2026-10-11, rendered walk).** The rendered walk caught a second arrivals-band defect, worse than the first: the projection's membership test for the arrivals band compared against the *live* `new_top`, which the same pass was extending as it examined the newest arrival. Any update that saw two or more new arrivals at once therefore examined the newest one, marked every older arrival in the same batch as "already reviewed" and silently dropped it from the projection — the drawer advanced about one row per update while the rest of the batch never appeared, until a filter change restarted the pass. The membership test now uses the band as the **previous** pass left it (`seen_new_frontier`/`seen_new_top`), so a whole batch is examined in one bounded pass; `app/src/tests.rs::a_third_arrival_batch_above_an_established_band_is_projected` pins it (red before the fix: `shown` 106 of 130; green after: 130).

**2026-10-13.** Dated **evidence correction** of the rendered `monitor_smoke` fullscreen-rewrap anchoring check, resolving the rendered example's red step in an Xvfb boot run of a verification prototype branch. **No reader behavior changes.** The deliberately-read anchor event is preserved across the borderless-fullscreen rewrap: the same retained admission identity, its within-row offset still inside the row's laid-out extent (the resolver re-clamps that offset whenever a rewrap shrinks the row — the fixture's one-line `event 19` keeps its fractional offset, and `check_anchor_top` proves the drawn offset lies inside the drawn row), the drawn top-visible row still that event, and the panel status still `reading older lines` with neither `older lines expired` nor `updating view`. The CI failure was the **walk's own measurement**, not a reader defect: its synthetic `PgUp` held the key for two frames, so on CI's ~130 ms software-rendered frames the drawer's held-key repeat cadence (`PAGE_REPEAT_SECONDS`, 0.12 s) added a second deliberate page **on the frame the walk measured**, while the walk recorded its reading position from the *drawn* rows — which only reflect the reader's state after the frame that synced them has ended, because row materialization is deferred to that frame's sync point. The recorded pre-rewrap event was therefore one page stale and the post-rewrap comparison failed with the reader anchoring correctly. The CI signature (`event 19` → `event 6`) reproduces here on Windows at 150 ms per frame, the pacing alone; the extra CI-only `DeviceNotAvailable` diagnostic row and Xvfb's fullscreen path are therefore recorded as **not required to explain that signature**, not as proven absent, since no Linux/Xvfb re-run of the walk was made. The walk now (a) holds each paging key for exactly one frame, so the held-key cadence has no frame in which it could add an unaccounted page, (b) measures a step one frame after it applies, (c) admits a real arrival under the held anchor — checked in the retained history, so the row-set change really happened — and requires the same event to stay the drawn top-visible row, and (d) compares the reader's own stored anchor (`MonitorPanel::anchor`) with the drawn top-visible row's public admission identity (`MonitorRow`) at every recorded reading step, across that arrival and across the rewrap. The walk takes an emulated per-frame delay from `MONITOR_SMOKE_FRAME_MS` (CI's cadence ≈ 130): it passes here at the 1280x720 reference, at native fullscreen, and at 150 ms and 400 ms per frame.

#### The panel (Q1–Q2, verdict A)

- *(Amended 2026-11-14: the drawer moves to y 212–500 with a 220px reservation and an 11-row log body, clearing the taller dialogue box of [Default look](title-and-menu.md#default-look) §5.)* **Bottom drawer, dialogue-reserved.** At the 1280×720 reference the panel spans **x 24–1256 (1232 wide)** and **y 212–572 (360 tall)** — a 24px side inset and a **148px bottom reservation** that keeps the live dialogue band (bottom 40), the cue slot (bottom 40 / right 48), the help line (top 12 / left 16) and the player Notice (top 12 / right 48) visible and unmoved while the panel is open. Opening, closing, resizing, filtering or scrolling the panel never reflows, shifts or dims those elements, and the drawer adds no reservation of its own to them.
- **Chrome**: a **header** row (≈52px) carrying the `Monitor` label and the three settled controls on one line — **Show everything** checkbox, **Follow latest**, **Close** — then the **log body** (≈293px, about 15 single-line rows), then a one-line **status footer** (≈13px). The three-control Tab cycle, first-open focus, pointer/keyboard activation and all focus rules remain [Interaction](#interaction)'s.
- **Status line** (panel-local; the log-stream and interaction sections' wording, not re-invented): the active view (`showing unmei:: only` / `showing everything`), the readout `retained · shown` plus the three saturating counters `evicted · lost · truncated`, the marked `updating view` and `older lines expired` flags, the reader state (`following latest` / `reading older lines`) and a short `F3/Esc close · Tab cycles controls` hint. No counter or flag is a retained line, a tracing event, a Notice or a new failure class.
- **Losing alternatives, kept only as the prototype's comparison**: B, a right column (460px wide, full height under the Notice) which covers the cue slot and the right of the dialogue band and wraps long lines harder; C, a centred 880px column over a full-window scrim, which hides the live scene behind the menu overlay's dimmer. **Neither is built**, and Monitor is not another MenuView by assumption — the verdict is the drawer.
- The drawer is a **dev-only in-window surface**: no player-facing chrome, legend or Settings row, and its geometry is not a new player layout.

#### The rows (Q3–Q6, verdict)

- **One line per event, newest last**, in **Fira Mono Medium 12px / 1.45** — the type system's machine-value face for the whole line including the message; a log line is a machine value, not prose. Soft wrap never creates another retained row.
- **Row grid** at the reference: elapsed (72px, right-aligned, faint) · `LEVEL` (46px, toned) · exact target (150–224px, dim) · message then named fields (`1fr`, full text tone); 10px column gaps, 1px vertical row padding, ≈19.4px per single-line row. The three fixed columns hold their width: the message column takes the rest of the row and wraps, so a long message grows its row rather than running past the panel; a target longer than its column renders whole and wraps inside that column (no ordinary display ellipsis) instead of drawing over the message. A line over the 8,192-byte cap collapses the grid, flows as one warm-toned line and ends in a visible `[truncated]` suffix.
- **Level tones** (severity, reusing the app's warm vocabulary): INFO neutral `rgba(204,204,217,.62)`, WARN warm `#dba166`, ERROR red `#e07a6a`. The **newest retained line** carries a 2px warm inset bar `#e69e5c` plus a 4.5% warm wash `rgba(230,158,92,.045)`; no other row is decorated.
- Escaping, the byte cap with its suffix, message-less field rendering and the exact `unmei::` display predicate are [Log stream](#log-stream)'s and are rendered here, not re-chosen.

#### Fullscreen and scaling (Q7)

- **px anchoring.** Insets, panel height, row type and the 148px reservation stay fixed at fullscreen; the drawer spans the wider frame (1872px at 1920×1080) and the message column grows. No proportional scaling and no width cap on the drawer. Resize/fullscreen preserves reader state per [Interaction](#interaction).
- The losing B/C caps (460/880) are recorded only as alternatives; the build does not carry them.

#### Validation rules (the later build's acceptance list)

These are specified checks, **not tests run by the layout design**. Geometry is validated on the real Bevy rendered surface, not the browser prototype.

- At 1280×720 the panel rect, the header/log/status split and the row grid measure as specified, and the panel intersects none of the dialogue band, cue slot, help line or Notice. The prototype's overlap check is the reference for that assertion; the rendered walk is the proof.
- The three controls render in the header in the settled order, stay reachable through the [Interaction](#interaction) Tab cycle, and the status line follows the log body; no Monitor control or status text re-enters the player legend, title or Settings.
- Rows measure 12px/1.45 Fira Mono Medium with the elapsed/LEVEL/target/message+fields order. A multiline/CR/tab/ANSI/bidi fixture produces one logical row with visible escapes and no raw control character in the rendered text.
- The three level tones and the newest-line marker render exactly as specified; an empty view shows the empty state; a truncated line stays at or below 8,192 bytes with its `[truncated]` suffix.
- Long-target, oversized-message, message-less-field and 1,000-line overflow fixtures render and scroll inside the panel without moving the underlying HUD; `updating view`, `older lines expired` and the counters keep their [Log stream](#log-stream)/[Interaction](#interaction) meaning and wording.
- Fullscreen/resize keeps px anchoring, the 148px reservation and reader state; the log widens and the underlying HUD stays put.
- The viewer renders over play, title, menu/Settings, Gallery/Timeline and the startup error screen without changing those surfaces; F3/Esc/Close, input isolation and focus restoration follow [Interaction](#interaction). At 1280×720 the drawer covers the error screen's lower region — the `Esc · Enter · Space · click to quit` footer and, once the list reaches its 20-line `MAX_SCREEN_LINES` cap, the tail of the error list. The same errors reach the log as one escaped `unmei::failure` line, so the rendered walk must prove the dev can read a capped error list from the log or from the region above the drawer. That overlap is a presentation finding, not a settled error-screen geometry.
- **Reference only**: the browser prototype validates proportions, wrapping, scrollbar hit-testing and the state fixtures. Real Bevy text metrics, layout/GPU cost, cursor and hit-testing, and fullscreen behaviour remain build-time checks.

#### Amendments and trace

**2026-10-11.** The § Monitor intro and this document's *Sections so far* bullet record layout as settled; [Interaction](#interaction)'s out-of-scope line no longer defers geometry/type/color. No other section changes: Monitor adds no player surface.

**2026-10-14 (status).** The settled banner above now reads **Built** — the drawer's geometry, typography and tone shipped with the drawer and the rendered walk (see this subsection's implementation-status note). No geometry rule changed.

Trace (two interview forms, seven answers; the numbers are this subsection's decision areas, not the form order): Q1 structure (A, the dialogue-reserved bottom drawer); Q2 the drawer's numbers (reserve the dialogue band, 360px); Q3 line typography (whole line in Fira Mono Medium 12px/1.45); Q4 message face (mono); Q5 level tone (severity colours); Q6 the newest-line marker (keep); Q7 fullscreen (px anchoring). The pre-verdict fixtures, the losing variants and the human verdict remain on the prototype branch; this section records the settlement. No new ADR: reversible viewer geometry under the established architecture.

Out of scope: feature code; the log-stream/capture/retention/source-hardening contracts; dev parsing or the F5/F9 gate; interaction/input/focus rules; a proportional or fullscreen-scaled design; search, export and commands, persistent preferences, player-facing Monitor chrome, and any change to the dialogue/cue/help/Notice geometry.

---

## Text skip

Settled 2026-10-08. **Built** — shipped 2026-10-14. Consumes [Read log](#read-log)'s mark rule and arrival status and [Player comfort](#player-comfort) §2–§4's reveal, dwell and settings machinery; implements none of them twice. The held-key behaviour rests on Bevy 0.19's synthesized focus-loss key releases.

### 1. Shape

**Skip is auto mode with a different dwell and a gate.** It is one per-frame machine in `app/` — the `AutoDwell` shape ([Player comfort](#player-comfort) §4): gated by the same `menu_closed` + `not_held` conditions, counting `time.delta_secs()`, and **its only action is `advance_say`**.

- **Two controls, neither a preference.** `Tab` (read nowhere else in play) toggles the **seen skip**; a held `Ctrl` (unused anywhere else in the app) is the **force skip**. Neither is a `play` setting, neither survives the session, and the Settings view gains no row (corrected in place in [Player comfort](#player-comfort) §1's table and [Menu overlay](#menu-overlay) §5).
- **Space, Enter and left-click are unchanged** by this section: they advance, and a press mid-reveal still only completes the line. *(The 2026-10-01 plan's “Space skips read text only” is superseded: the key is `Tab`.)*
- **Layering.** Ctrl is an override, not a second toggle: while Ctrl is held the force skip runs and `Tab` is inert; on release the seen toggle resumes if it was on. Nothing changes state underneath a force hold, and the cue is one `SKIP` throughout (§7).
- **The one-thing list.** Skip never chooses, never resolves or cancels a decision, never calls a resolver, never arms the autosave, never writes the settings file and never touches staging. The only trace it can leave in the world is the read log's own marks — a park mark where a line is genuinely read, a visited mark where a crossing lands (§8).

### 2. The skip step

- **One step = complete, dwell, advance.** A step completes any in-progress reveal instantly (the line is whole), holds it for `SKIP_DWELL_SECONDS = 0.05` — about 3 frames at 60 fps, ~20 lines/s — then advances. A const beside `AUTO_DWELL_SECONDS`: not scaled by line length, not authored, not a setting.
- **Skip never waits for a reveal and never doubles up.** Unlike a press, which mid-reveal only completes and is consumed ([Player comfort](#player-comfort) §3), skip's step does both: the press contract is the player-action protection, and **skip is not a press**. One advance per step; a stall (a frame hitch, a freeze) is not caught up afterwards.
- **Commands are transparent.** A step advances a say-line; the commands and jumps between parks (`bg`, `sprite`, `play`, `jump`, `set`) execute exactly as on a manual advance, inside the same step. A crossing is not a stop: a `jump` after a skipped line crosses, and the new scene's first park is evaluated by the same rules (§3).
- **The setting does not change.** `play.text_speed` is untouched; under skip every line is whole for its dwell whatever the ladder holds.

### 3. Seen skip (`Tab`)

- **It engages only where it can act.** `Tab` turns the mode on only while the runner is parked on a say-line whose **arrival status** is read ([Read log](#read-log) §4); mid-reveal counts, because a step completes the reveal. On an unread say-line, at any other park and at end-of-scene, `Tab` is **inert** — no mode, no cue flash.
- **It ends at anything it cannot pass.** The toggle ends at **any park that is not a say-line** — a choice-wait, a decision-wait, the end-of-scene marker, and any future wait kind, since the test is `Wait::Say` and not a list — and at the first **arrival-unread say-line**. It lands on that line, which is shown from that frame at the player's text speed; the arrival status was read before the mark, so the line counts as unseen even though its mark lands that frame.
- **Cancels.** Any player input ends it **when the input belongs to play**, not when [Monitor](#interaction) owns it: Monitor controls/Esc/close do not cancel seen skip or toggle auto; an already-armed force hold and release tracking stay live, while fresh Ctrl/Tab there cannot arm skip. Opening/closing Monitor is not a pause or discontinuity *(amended 2026-10-11)*: `Space`/`Enter`/left-click, `1`–`9` and a choice button, and `Esc`. Each press then performs **only its own action** — a cancelling press never also spends a skip step, and a `Space` on a revealing line still just completes the line. `A` ends it and toggles auto (the mode handoff); `F11` and an enabled developer `F5` leave it running; enabled developer `F9` is a load (§5). *(Amended 2026-10-11: F5/F9 require [Dev mode](#dev-mode); when disabled neither shortcut affects skip.)*
- **`Esc` is observed before the gate closes.** The cancel is read by the in-play input system, which is chained before `menu::menu_input` ([Menu overlay](#menu-overlay) §6); on the frame that opens the overlay the toggle ends and the overlay opens — the toggle is never left running behind it.
- **A discontinuity clears it.** A slot load, a New Game and a Timeline jump ([Timeline](title-and-menu.md#timeline)) end the toggle and disarm force skip (§5). New Game does not reset the read log ([Read log](#read-log) §1), so `Tab` can engage again on re-read text.
- **The mode is not a preference.** It is off at launch, off on entering play and gone on exit; there is no `play.skip` and nothing to persist.

### 4. Force skip (`Ctrl`)

- **Held = force, no gate.** While `Ctrl` is held the machine skips every say-line, read or not, at §2's pace. It starts wherever a say-line is parked (mid-reveal counts), holds at any park it cannot pass — a choice-wait, a decision-wait, the end-of-scene marker — and never picks or resolves. Because the key is physical it **resumes on the next say-line if it is still held**, including after a choice or a decision lands.
- **Player input does not end a hold.** `1`–`9` and the choice buttons still choose while Ctrl is held, and skip resumes after the park; only releasing the key (or §5's discontinuity) stops it. This is the deliberate difference from the toggle's cancel rule: a hold is a physical act, not a mode with a memory.
- **It writes no marks.** A force-skip advance is **exempt from mark-at-park** ([Read log](#read-log) §4, amended): text blown past stays unread, so a later seen skip still stops there. The exemption attaches to the force-skip advance, not to the line or the scene — a later manual park on the same line marks normally.
- **Arrival status and visited are unchanged.** The app still evaluates `was_read` at each force-skip park, so the log's view of the line is untouched; only the mark is withheld. A scene it crosses into becomes the live play scene and is **marked visited** ([Read log](#read-log) §5), so it is Timeline-selectable like any other.

### 5. Gates, freezes and discontinuities

- **The freeze set is auto's.** Both modes run under `menu_closed` and `not_held` ([Player comfort](#player-comfort) §4); there is no new focus gate. Focus loss is handled one layer down — the engine forces key releases — so a held Ctrl stops on Alt-Tab, and a running toggle advances only read text and ends by itself at the first unread line.
- **The overlay owns input while it is open** ([Player comfort](#player-comfort) §6's `A`/`F11` pattern): neither `Tab` nor a Ctrl press is read under it, and no toggle can be running behind it because the `Esc` that opened it ended the toggle (§3).
- **A freeze honours a hold; a discontinuity disarms.** Closing the Esc overlay with Ctrl still held resumes the force skip — a presentation pause is not a discontinuity, and a hold observed in play stays armed across it. Title→play, a slot load (dev-enabled `F9` or the player overlay's Load, which is not dev-gated; amended 2026-10-11), a New Game and a Timeline jump **disarm both modes**: the toggle clears, and a held Ctrl must be released and pressed again **in play** before it runs. This is Bevy 0.19's forced-release fact applied to the app's own freezes: `bevy_winit` turns focus loss into synthesized key releases, so a key already down when a gate opens is not a fresh press.
- **The title is inert**, as for every other in-play key ([Title](title-and-menu.md#title) §1): no `Tab`, no cue, and play begins with both modes off. The defensive hold pauses both modes like the overlay.
- **Play session only.** Both modes are play-session state: entering the title, the error screen or app exit leaves nothing behind.

### 6. Auto mode

- **Skip wins while it runs.** Auto's dwell is suppressed — it neither counts nor fires — and the `AUTO` cue is hidden (§7); `play.auto` is untouched. When skip ends, auto's dwell **restarts** and auto resumes if the preference is on. After a seen skip stops at unread text, a still-on auto advances that line 2.0 s after its reveal, exactly as [Player comfort](#player-comfort) §4's rule would.
- **`A` is the explicit handoff.** Pressing `A` ends the toggle and toggles auto (§3); under a held Ctrl it toggles the preference and the hold continues.

### 7. Cue and labels

- *(Amended 2026-11-14: this bullet, the help-line bullet, the approved cue layout and the dialogue reservation below are superseded by [Default look](title-and-menu.md#default-look) §1: the lit `Skip Tab` label is the indicator. When it lights is unchanged.)* **One cue, `SKIP`.** A dim uppercase `SKIP` sits exactly where `AUTO` does — the dialogue band's bottom-right (`bottom: 40px`, `right: 48px`), ≈50 % alpha, mono ([Type system](title-and-menu.md#type-system)) — and shows whenever either mode is engaged: the `Tab` toggle while it runs, and a held `Ctrl` including while it waits at a choice, a decision or scene end. It is hidden on title surfaces exactly like `AUTO` ([Title](title-and-menu.md#title) §1), `AUTO` is not shown while skip runs, and the cue follows the **effective** state, so a disarmed hold shows nothing.
- **No Settings row.** Skip is not a preference: the Play group stays Text speed, Auto, Fullscreen plus its reset, and `play` gains no field.
- **The help line carries the keys.** Tokens: `Space/Enter/click advance · Tab skip · Ctrl skip all · 1-9 choose · A auto · F11 fullscreen · Esc menu`; exact punctuation is the build's ([Player comfort](#player-comfort) §6, amended). The title's legend is unchanged.
- **Approved cue layout** *(folded 2026-10-08 from the prototype that decided the cue and the Timeline layout, whose prototype page is the primary source)*: `SKIP` is **20px Fira Mono Medium**, warm `rgba(230,158,92,.5)`; `AUTO` is the same size/face in neutral `rgba(217,217,230,.5)`. They share the existing right/bottom slot above and are mutually exclusive during effective skip. The prototype's original “beside AUTO” is superseded; its adjacent 120px option is comparison-only.
- **Reserve dialogue space in every cue state**: fixed **right inset 160px**, left 48px, bottom 40px, 24px prose and mono speaker prefix, whether both cues are absent or either is shown. No text shift on cue changes. The help stays top 12px/left 16px in 14px mono. Instant cuts, no motion and no full-window skip surface. The shaded prototype sample is staging, not a mandated new dialogue-box skin. These are the approved reference proportions at 1280×720; Bevy collision/scroll validation is still required.

### 8. The read-log seam

- **The mark rule gains one exemption** *(dated amendment, [Read log](#read-log) §4)*: a **force-skip advance** does not mark the line it passes; every other park — a seen skip's arrival, a manual advance, a load's re-park — marks normally. The exemption is on the advance, not the line: a later manual park marks it.
- **The arrival status is derived at each park transition** — from the log, **before** that frame's mark lands ([Read log](#read-log) §4) — and held for that park's duration only. It is never carried across a park change, a load, a New Game or a Timeline jump: a load onto a read line lets `Tab` engage, a load onto an unread line does not. That is the invariant that makes §3's gate and §5's disarm rule safe.
- **No new flush moment.** A running seen skip adds no *park* marks — every line it passes was already read — so it never dirties the log while it runs; force skip adds none at all. The stop line (the ordinary mark-at-park) and a crossing's visited mark are the ordinary marks, and they ride the existing moments ([Read log](#read-log) §6, noted). A crash mid-skip loses at most the same window it does today, and force skip remains the replay escape hatch.
- **An absent or discarded log** ([Read log](#read-log) §3) initializes the launch's in-memory log empty: `Tab` is inert on first arrivals to unread say-lines, but ordinary marks can make later visits during that same launch arrival-read and eligible. Force skip is unaffected. *(Clarified 2026-10-08: replaces “everything unread for the whole launch”, the overbroad assertion flagged while the Timeline was designed; read-once does not freeze knowledge.)* The log's own row 26 carries its IO ([Read log](#read-log) §7); skip adds no failure row.

### 9. Failure posture

- **No new inventory row.** A skip advance reuses `AutoDwell`'s path exactly: an `advance_say` error goes through `failure::report_runner_call` → row 13's defensive hold and Notice ([Runtime failure posture](#runtime-failure-posture) §2), and the `not_held` gate pauses both modes until the menu recovery. The read log's IO is row 26 already.
- **App-side only.** `engine/` and the dialogue DSL are untouched — no `Wait` change, no parser or content change, no save-format or settings-file change — the [Player comfort](#player-comfort) §7 boundary, kept. Every rule above is an `app/` test and `engine/` gains none.

### 10. Validation rules (the build's test list)

- `SKIP_DWELL_SECONDS` is `0.05`, unscaled and not a `play` field; an Instant-text line advances one step per 0.05 s of simulated `Time`, and a 1 s reveal is completed and advanced in a single step.
- A cancelling press never double-steps: with the toggle running, a `Space` on a revealing line completes the line and no step fires that frame; on a whole line it advances exactly once.
- Commands between parks execute inside the skipped step (a `bg`/`sprite` between two skipped say-lines lands), and a `jump` crosses: the new scene's first park is evaluated normally — skip continues on a read line, ends on an unread one.
- The toggle engages only on an arrival-read say-line — first visit `Tab` inert (no cue), second visit `Tab` engages, mid-reveal counts — and ends at the first arrival-unread say-line (which then reveals at the player's `text_speed`), at a choice-wait, at a decision-wait and at end-of-scene, dropping the cue each time.
- `Space`, `Enter`, left-click, `1`–`9`, a choice button and `Esc` each end the toggle and do only their own action; `F11` and dev-enabled `F5` leave it running; `A` ends it and toggles auto. Dev-disabled F5/F9 affect no skip state *(amended 2026-10-11)*.
- `Esc` ends the toggle on the frame that opens the overlay (observed before `menu_input`), and closing the overlay does not resume it.
- A held Ctrl skips unread lines; the line it passes stays arrival-unread on a later visit while its scene is visited; a later manual park on the same line marks it.
- Ctrl holds at a choice/decision/end park and resumes on the next say-line while still held; a digit still chooses under the hold, and the force skip resumes after the park lands.
- Ctrl held across the overlay's close resumes on the next say-line; a Ctrl held across title→play, a load, a New Game or a Timeline jump does not run until released and pressed again in play; the cue follows the effective state and is absent for a disarmed hold.
- Skip suppresses auto: with `play.auto` true a skipped line advances on the skip dwell, not the 2.0 s dwell, and on skip's end auto's dwell restarts from full; no skip control changes `play.auto` or writes the settings file.
- With the overlay open neither mode advances, neither cue changes, and no dwell accumulates; closing resumes per §5.
- The `SKIP` cue shows for either mode and not otherwise, `AUTO` is hidden while skip runs, and both are hidden on title surfaces.
- The help-line constant carries `Tab skip` and `Ctrl skip all` (and still no `F5`/`F9`); the Settings view has no skip row and the settings file gains no key.
- New Game: both modes off and the runner at the entry scene; existing knowledge is retained, while ordinary entry/park marks and dirty flushing still apply. A clean already-known entry leaves the log bytes identical; an unknown entry adds its normal knowledge.
- Empty log at launch: first-arrival unread lines cannot engage `Tab`; ordinary marks permit a later same-launch visit to engage it. Force arrivals add no read marks and force skip remains available. Play begins with both modes off.
- At 1280×720, off/seen/force/auto cue states use §7's exact size, tones and exclusive position, leave dialogue geometry unchanged, and intersect neither dialogue nor help; long prose wraps within the fixed reserved width.
- Held-key tests drive `ButtonInput` through the `InputPlugin` harness (`KeyboardInput` messages) so `pressed` survives gated frames; `press_and_update`/`reset_all` cannot express a hold.

### Amendments

This section corrected other text in place, each with its own dated note:

- **2026-10-08, build-spec synthesis**: §7 folds the approved prototype's cue tone/size and fixed dialogue reservation into its owning contract; §10 gains visual validation. §8/§10 replace the flagged “whole launch” and New Game “byte-identical” shorthand with initial-empty/later-revisit and knowledge-retained/ordinary-entry assertions. Normative arrival-before-mark and flush rules are unchanged.

- [Player comfort](#player-comfort) §1's table (`Skip` is in), §3's advance contract (the skip step), §6 (the help line's tokens), §8 (the deferral retired) and the section's *Out of scope* line.
- [Menu overlay](#menu-overlay) §5's Play row and its Amendments.
- [Read log](#read-log) §4 (the force-skip exemption), §6 (no new flush moment) and its *Out of scope* line.
- [Title](title-and-menu.md#title) §1's inert-hotkey sentence (the skip keys join the list).
- The document's *Sections so far* bullet; [ADR 0011](adr/0011-read-log-as-a-third-persistence-class.md); [`GLOSSARY.md`](../GLOSSARY.md); and `NEXT-STEPS.md`'s retired deferral (that file was deleted 2026-10-08).
- **2026-10-14**: the settled banner above now reads **Built** — the section shipped on 2026-10-14. No rule changed.

### Out of scope for this section

The Timeline's surface, gating, jump and confirm ([Timeline](title-and-menu.md#timeline)); the read log's format, key and lifecycle ([Read log](#read-log)); the Timeline card geometry ([Timeline Layout](title-and-menu.md#layout-1)); new motion beyond §7's instant cuts; a skip preference of any kind (speed, default-on, a Settings row); skip in the `unmei author` CLI; gamepad support and input rebinding; per-line authored pacing in the DSL; anything `engine/`-side.

---

## Menu and live-session transitions — ownership design

Settled 2026-10-13: four design rounds (Q1–Q9) and explicit shared-understanding confirmation (Q10). This is the menu-transition candidate the architecture-deepening map chose, not interface alternative B below. **Implemented 2026-10-15:** the recipes live in `app/src/session.rs` as `SessionTransitions` behind the one `SessionInput` Bevy bridge, and the menu, the Continue paths and F9 call them; the settled design below is the living contract. Existing menu, title, Gallery, Timeline, save-slot, failure and presentation-pause behavior remains binding. Design record: [ADR 0012](adr/0012-menu-and-live-session-transition-ownership.md). The glossary's **Live session** already names the concept; this extraction does not introduce another kind of session.

### Settled ownership and timing (round 1)

- **Two modules, two responsibilities.** The menu owns navigation, confirmation, focus and surface changes. A separate app-owned module owns coordinated live-session entry/replacement recipes, shared by the menu and developer shortcuts. It is not an app-wide controller: settings, credentials, widgets and rendering are not absorbed into it. Resource representation and the operation interface are settled in round 3 below.
- **Synchronous application.** Accepted actions apply their state changes in the input frame; no queued transition phase or extra frame of latency. Bevy input adapters and named systems remain declaratively ordered in `app/src/lib.rs`: input ownership/pause/dev gates, decision/key-check polling, `skip_advance → drive_script → auto_dwell`, then `observe_parks → write_pending_read_log → write_pending_autosave`. The physical force-key release mirror remains ungated by the menu pause.
- **Composition with the presentation-surface partition.** These choices did not themselves settle the presentation partition. The requirement for the second resolution to describe applied presentation resources and replay/reset access was subsequently fulfilled by [the settled partition composition](#composition-with-the-settled-ownership), with no second owner of the same state. Both extractions landed on 2026-10-15.

### Settled load policy and contract-repair separation (round 2)

- **One load operation with a typed origin.** Named menu/Continue versus developer-shortcut origins select the existing policy, not caller-supplied failure rows or Notice booleans. The shared operation owns the missing-file probe where applicable, read → restore → successful cleanup, and origin/stage-to-row mapping. It returns an explicit loaded/missing/refused outcome; a menu may close or enter play only on loaded. Engine save IO/validation stays engine-owned, and `failure.rs` remains the sole owner of texts, levels, Notice and defensive-hold posture.
- **Distinct failure meanings remain.** F9 missing slot is a no-op. F9 read refusal is row 16, the engine's reason with path detail, Notice-less; menu/Continue read refusal and every restore refusal are row 17, `couldn't load that save`, with a Notice. No row is unified or invented.
- **Two source/contract discrepancies require separate contract-repair work, not incidental extraction fixes.** `menu.rs`'s live Continue calls `enter_play`, which resets auto dwell through `core::reset_for_replacement`, although [Title §2](title-and-menu.md#2-the-titles-states-and-continue) requires its remaining dwell to resume. The newer skip contract still requires title-to-play disarming; preserving dwell must not remove that disarm. Separately, `load_slot` currently closes after the void `apply_loaded_save` call even if a defensive restore refusal was reported, contrary to the stay-open contract. Normal `read_slot` → restore uses the same pack synchronously, so that restore refusal is not ordinarily reachable; metadata-to-activation disk changes instead fail the read and already stay open. Direct invalid-save tests establish the defensive helper path, not an ordinary filesystem race.
  *(Repaired 2026-10-15): both discrepancies are fixed — live Continue now preserves the remaining auto dwell while still disarming skip on title-to-play entry, and a defensive restore refusal no longer completes the load (the surface stays up, with the old runner/staging and pending/held session intact). The extraction this section planned landed on 2026-10-15.*
  *(Witnessed 2026-10-15): the forced restore-stage completion witness the extraction left as a residual is restored at the production seam. `SessionTransitions::read` returns the already-read save (`ReadOutcome`) and the menu's `complete_load` — the one completion the Continue and overlay load callers share — restores it and closes/enters play only on `Loaded`. A genuinely refused restore keeps the title up with no launch-entry marker and keeps the overlay on its slot grid, with the old runner/staging and pending/held session intact and no replay. The read-refusal and successful-load witnesses stay separate.*
- **Sequencing obligation.** At the map's close, create separately scoped contract-repair and extraction handoffs. The repairs must establish the binding behavior and focused regressions before the behavior-preserving extraction builds on it. Neither observed discrepancy is promoted to a new player contract. This design implements neither repair. *(Fulfilled 2026-10-15: the repair handoff landed first and natively blocked the extraction handoff.)*

### Settled interface and test scope (round 3)

- **Choose direct synchronous orchestration through restricted borrowed capabilities (alternative C).** A recipe-only `app/src/session.rs` module owns entry/replacement implementation, with ordinary short-lived borrowed operation access rather than a new owned ECS aggregate. The menu calls operations and reacts immediately to success/refusal; it does not emit a menu effect list or use a command interpreter. Existing underlying resources remain separately stored and registered.
- **The 26-parameter call becomes a restricted operation interface**, illustratively `menu::transition(action, &mut menu, &mut context)`. The context exposes concrete session, settings/key and exit operations plus bounded read-only facts, never arbitrary mutable resource access. `SessionTransitions` exposes load, New Game, live Continue entry and Timeline replacement; its fields and cleanup helpers are private. A module-owned Bevy `SystemParam` bridge acquires/reborrows resources once; ordinary recipe implementations use references, not `World`, `Commands` or a service locator. Names/lifetimes are sketches to validate at implementation compile, not permission to revert to public reset fields.
- **Depth acceptance bar.** Menu action handling and the F9 shortcut body must not perform pending/held task cancellation, comfort-clock/skip reset or presentation replay-flag manipulation. Those coordinated mutations live behind session operations. The bridge may name the resources to acquire them; that is not permission for the caller to re-perform the recipe. Broad acquisition and constructor wiring remain real costs, concentrated in one bridge rather than repeated at each call.
- **Other menu destinations keep their owners.** Settings mutations/flushes and credential/key-check/clipboard/editable-field actions call their dedicated concrete handlers; no such dependency enters the session recipes. Menu navigation, confirmation and focus remain menu-owned; app exit uses the existing writer. Full widget actions still require a Bevy adapter and integration tests. Shared Notice/hold access is acquired once and narrowly reborrowed, not aliased in simultaneously live capability objects.
- **Alternatives considered.** A consolidated owned `LiveSession` resource offers stronger physical state locality but demands broad resource/public-`Runtime` migration and coarser Bevy borrowing/change ticks. A pure menu decide/execute/complete round trip offers independent policy testing but adds operation/completion vocabulary and a two-pass protocol. C provides the shared recipe seam with less migration and no new execution protocol; it fails if it merely disguises the existing argument bag.
- **Replace two unnecessary one-shot closures, not the integration suite.** In `app/src/tests.rs`, migrate `a_load_refused_at_restore_time_notices_row_17` (definition 763, call 784) and `a_load_clears_the_held_and_pending_decision_state` (definition 1350, call 1365) to the ordinary session restore interface. The verified baseline is **47** `.run_system_once(` expressions, not the issue's 49: 38 in 22 tests and 9 in 9 helpers. Retain the other 45 sites in this slice, including adjacent startup/auto tests and real presentation/input machinery. Add or relocate only minimal direct-seam cases for distinct missing protection: typed-origin/stage mapping, transition-specific cleanup and refusal preservation. The second migrated test completes its pending task before restoration; keep actual future-drop integration to prove in-flight cancellation.

### Settled remaining ownership (round 4)

- **One private launch-entry marker stays in `MenuState`.** It is the menu's monotonic fact that play was entered this launch, not ownership of runner/task/clock state. Boot initializes it false. Successful boot New Game or cold Continue/autosave entry sets it true; live Continue and later New Game retain true. Overlay load and Timeline are already live-session routes and retain true; ordinary Resume/Exit to title do not clear it, and refusal cannot set it. The inspected code has one write site (`menu.rs:2218`, inside `enter_play`). F9 is already play-only through the existing dev-mode + `menu_closed` + `app_input_ready` run-condition chain (`lib.rs:300–321`) and establishes no second entry route. All Continue/New Game/Quit/rail branches use the one shared predicate; no mirror is added to `Runtime` and no new pause gate appears.
- **Timeline passes a scene id to the session recipe.** Menu owns confirmed-card-index resolution against the pack's declared order, visited eligibility and focus/confirmation. An invalid index remains the existing no-op. The recipe restores the named scene at `pc = 0` with current state and staging carried over, before destructive cleanup; it neither performs label lookup nor derives scene order. Named restore refusal keeps the old session and follows defensive row 13.
- **Record the trade-off in ADR 0012.** The dated section owns build detail; the ADR explains why recipe ownership is separated from physical state storage and why an aggregate or effect/executor protocol was not chosen. No new glossary term or changed glossary definition is needed.

### Physical resource ownership and presentation composition

| State or responsibility | Owner after extraction |
| --- | --- |
| Navigation, focus, confirmation, origin, cached slot/Continue metadata, launch-entry marker | Menu |
| Runner/staging and pack paths (`Runtime`); pending/held decisions, auto/skip clocks | Existing runtime/core resources remain separate; session module owns coordinated entry/replacement mutation |
| `PendingAutosave` and actual stamped/pending writes | Existing `slots.rs` resource/writer; session recipes disarm where the existing transition requires it |
| Read-log knowledge, arrival status, dirty/flush lifecycle and observation/writer systems | Existing read-log owner; session recipes request position invalidation/flush at the contractual moments, never erase knowledge |
| `Notice`, `DefensiveHold`, failure texts/levels/posture | `failure.rs`; recipes report the existing row and use existing release/clear operations where prescribed |
| Preferences and their flush points; credentials, resolver/key checks and widget editing | Existing dedicated owners, invoked through concrete menu handlers; not session-owned |
| `AppliedAudio`, `AppliedVisuals`, `AppliedUi`, `TextReveal` | Presentation-owned; borrowed replay/invalidation and explicit reveal reset access, not mutable flags exposed to menu/F9 |
| `SettingsDrag`, `AssetWatch`, `MenuBuilt`, rendering/audio realization | Presentation-side owners; not moved into session by this design |

Candidate D is resolved; its [settled partition and composition](#presentation-surfaces--decided-partition) are recorded below and in [ADR 0013](adr/0013-presentation-surface-partition.md). This design fixes **when** an accepted entry/replacement requests replay or reveal reset; the play owner implements **how**. The presentation partition owns the presentation owners and adapts the concrete access bridge without moving recipe policy or adding a second owner; the architecture-deepening map records this composition. Both landed on 2026-10-15. No generic presentation trait or public low-level reset effect is introduced for this one concrete path.

### Operation semantics and interface constraints

Illustrative names below are compile-validated in the build; visibility, constructor layout and lifetimes may change without weakening the ownership contract:

```rust
// A short-lived ordinary borrow, not an ECS Resource or session snapshot.
impl SessionTransitions<'_> {
    fn load(&mut self, slot: Slot, origin: LoadOrigin) -> LoadOutcome;
    fn restore_loaded(&mut self, save: SaveFile, path: &Path) -> LoadOutcome;
    fn new_game(&mut self);
    fn resume_live_entry(&mut self);
    fn restart_scene(&mut self, scene_id: &str) -> RestartOutcome;
}
// LoadOrigin: Menu | DevShortcut. LoadOutcome: Loaded | Missing | Refused.
// RestartOutcome: Restarted | Refused. No queued or low-level reset effects.
```

`restore_loaded` is the ordinary already-read restore seam replacing `input::apply_loaded_save`, used by the shared slot-load implementation and direct defensive-refusal tests. It owns the restore-stage row-17 reporting and performs successful cleanup only after restore succeeds. Menu reacts to load/restart outcomes without reporting them a second time. Ordinary overlay Resume and Exit to title remain menu surface changes, not replacement operations. Manual capture/write and Legacy import continue using the existing slots/engine helpers through dedicated concrete access; they do not inherit replacement cleanup or change their refresh, confirmation, rename or failure rules. F5-before-F9 processing and successful-save logging remain as today.

| Operation | Required distinction behind the seam |
| --- | --- |
| Live Continue entry | No slot read/replacement/cancellation; preserve runner, remaining auto dwell, reveal, Notice and pending/held decision; independently disarm skip on title-to-play entry and request the existing read-log flush |
| Successful load (menu, Continue autosave, F9) | Restore runner/staging, cancel pending/discard held, release hold, reset auto/skip, invalidate arrival, request flush, force audio/visual/UI replay. Preserve existing load distinctions: no explicit Notice clear, reveal reset or pending-autosave clear is added by extraction |
| New Game | Rebuild/repark entry with initial state/staging; cancel/discard, release hold, reset auto/skip/reveal/arrival, clear Notice, disarm stale autosave, replay and request entry flush; retain read knowledge, preferences/credentials and slot bytes |
| Successful Timeline restart | Carry current state/staging, restore scene-from-top before cleanup; cancel/discard, release hold, reset auto/skip/reveal/arrival, clear Notice, disarm stale autosave, replay and request flush; ordinary target script events retain normal autosave/visit effects |
| Refusal | No successful-replacement cleanup. Preserve old runner/staging/pending/held state; existing failure reporting may change Notice/hold as its row requires. Menu stays on the appropriate surface; Continue refreshes metadata/focus after refusal |

No catch-all reset is shared across semantically different transitions. A load/new-game/jump cleanup does not replace decision polling, `drive_script` or the ordinary landing/mark/write pipeline. Settings/key-check polling and input-release tracking stay at their current named Bevy systems.

### Validation contract for the later build

- **Direct seam:** replace exactly the two restore-policy closures named above. Use ordinary concrete fixtures and real engine IO in temporary directories, with no new filesystem/network abstraction. Protect missing F9 no-op, dev read refusal row 16 versus menu read refusal row 17, restore refusal row 17, successful cleanup and transition-specific preservation. Share implementation between slot and already-read restore; do not bypass the seam to set reset fields in tests.
- **Load wiring and failure surfaces:** retain `save_then_load_restores_position_through_the_app` (`tests.rs:358`), `load_signals_presentation_to_replay_visuals_and_audio` (`:412`), `dev_mode_off_makes_f5_and_f9_silent_no_ops` (`:462`), `an_unreadable_slot_refuses_to_load_with_its_reason` (`:722`), and `a_load_refused_under_the_overlay_notices_row_17` (`:2347`). These keep actual input/resource wiring, replay and distinct log/Notice protection after the plain tests migrate.
- **Async lifecycle and exactly-once writes:** retain `loading_a_decision_wait_refires_the_decide_line` (`:1127`), `a_model_answer_arriving_while_open_is_held_until_resume` (`:1245`), `a_held_fallback_writes_no_autosave` (`:1294`), `loading_discards_the_held_answer_and_refires_the_decide_line` (`:1318`), `new_game_cancels_an_in_flight_decision` (`:7561`) and `exit_to_title_then_continue_applies_a_held_answer_without_a_second_decide` (`:9120`). Cancellation observes actual pre-completion future drop; held-answer application protects subsequent-frame autosave stability, not merely command intent.
- **Timeline and frame order:** retain `a_jump_restores_the_target_from_pc_zero_with_carried_staging_and_state` (`:14199`), `a_jump_cancels_a_pending_request_and_discards_a_held_answer` (`:14251`), `a_held_ctrl_through_a_timeline_jump_disarms_and_needs_a_fresh_press` (`:14299`), `a_restore_refusal_stays_on_the_timeline_and_preserves_the_session` (`:14418`), `a_jump_selection_writes_no_autosave_even_with_a_stale_pending_flag` (`:14471`), read-log landing/flush tests (`:14541`, `:14573`) and `a_jump_begins_only_the_targets_fresh_decision_request` (`:14889`). Keep real key release/repress and same-frame disarm-before-writer evidence.
- **Menu/presentation/input integration:** retain existing real keyboard/click navigation, confirmation cancellation, disabled controls, title freeze/audio/frame suppression, Settings persistence/key/clipboard editing, Gallery/still and Timeline scroll/focus tests. The other 45 one-shot sites remain, including Monitor capture and Bevy queued-edit integration; no broad `tests.rs` refactor. Test direct state rules only where existing coverage does not already protect the same distinct risk.
- **Repair prerequisites:** separately prove live Continue resumes the remaining auto dwell while preserving required title-to-play skip disarm; retain load/New Game/Timeline discontinuity resets. Prove a forced defensive load restore refusal preserves session state and cannot close/enter play before fixing its completion handling. Keep the normative Title §2 resume requirement; do not update it to match the defect.
- **Shipping and rendered acceptance:** `cargo test --locked -p unmei-app --test shipping_schedule` must preserve `the_shipping_composition_builds_clean_schedules_and_runs_frames` with ambiguity detection still strict. Run affected real rendered title/play/overlay/confirmation/Settings/Gallery/still/Timeline flows through `cargo run -p unmei-app --example monitor_acceptance`, and `cargo run -p unmei-app --example monitor_smoke` for cross-surface input/layout regressions, plus a real shipped-app boot. Record actual fixture/profile/platform and human inspection; headless tests alone do not prove rendering/boot.
- **Build gates:** focused session/menu/load suites, then `cargo fmt --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace`. Exact source lines above are the pre-extraction baseline, not stable post-refactor locations. None of these production/build checks is claimed by this plan-only interview.

### Handoff and scope

**Reconciled 2026-10-17:** the handoffs this paragraph planned were created during the map's close decomposition and have landed — the contract repairs, the behaviour-preserving extraction that the repairs natively blocked, and the presentation extraction. Whichever implementation lands second must honor the recorded replay/reset composition. No engine/DSL/save-format/slot-layout change, menu behavior/layout/copy change, generic port, app-wide controller, new action queue, broad test-file restructuring, presentation partition implementation or journey-page update is included here. The human confirmed this synthesized decision in Q10; implementation and its validation remain separate work.

---

## Presentation surfaces — decided partition

Settled 2026-10-13: four interview rounds (Q1–Q9) and the complete-design confirmation (Q10). Planning candidate D of the architecture-deepening map. **Implemented 2026-10-15:** the thin `app/src/presentation.rs` coordinator and eight app-private realization/support owners are in production; ownership of presentation state was tightened on 2026-10-16. The settled design below is the living contract; its baseline descriptions and later-build validation list record the original planning context, not newly observed validation. Behaviour, layout and copy remain exactly as [Title and menu](title-and-menu.md) and this document's earlier sections fix them; nothing observable changes. Design record: [ADR 0013](adr/0013-presentation-surface-partition.md). The Monitor reader's shared metrics are [reading anchor](#reading-anchor--decided-seam-2026-10-13)'s and the session/menu ownership is [menu and live-session transitions](#menu-and-live-session-transitions--ownership-design)'s; this section composes with both and reopens neither.

### The partition

`app/src/presentation.rs` (3,918 lines) becomes a thin assembly coordinator plus eight app-private submodules. Every item stays `pub(crate)` or narrower; no public surface and no test-only door is added.

| Owner | Owns | Exposes |
| --- | --- | --- |
| `presentation.rs` — the assembly coordinator | `MenuBuilt` and `MenuRoot`; the one rebuild signature and the root despawn/rebuild transaction; the title/overlay door dispatch (`sync_menu_ui`); the single focus repaint (`sync_menu_focus`, `focus_border`) | the surface builders, invoked in the existing dispatch |
| `presentation/play.rs` | `AppliedAudio`, `AppliedVolume`, `AppliedVisuals`, `AppliedUi`, `TextReveal`, the `Bgm`/`Sfx` channels; the dialogue band and choice buttons, staging draws, audio application, the `AUTO`/`SKIP` cues, the Notice text node, the startup error screen | `request_replay` (a concrete operation hiding the applied caches), the reveal operations, `setup_ui` |
| `presentation/title.rs` | the wordmark, the five rows, the tagline, the neutral gradient, the `title_backdrop` spawn and fit | builder |
| `presentation/overlay.rs` | the rail, the top bar, the slot cards, the confirm-dialog visuals and the Legacy picker | builder |
| `presentation/settings_view.rs` | the Settings, Play and Decisions groups, their labels and tracks, the window mode, the key row/field and the drag | builder + the drag-cancel operation |
| `presentation/gallery.rs` | the grid, the card, the still, the title-door scrim and the art/column/fit/scroll syncs | builder + pure derivations |
| `presentation/timeline.rs` | the surface, its cards and the scroll sync | builder + sync |
| `presentation/assets.rs` | `AssetWatch`, the asset path producers and the one-per-path watch/settle | watch/settle operations |
| `presentation/style.rs` | the embedded `TextFaces`, the shared tones (including `FOCUS`) and the cover/contain fit maths | faces + fit maths |

- The rebuild rules are unchanged: focus is never part of the signature (a focus move repaints a border only), a credential-label or pronunciation change never rebuilds, and only a confirmation change carries the Timeline body's scroll offset across the rebuild.
- Settings and Gallery each remain one implementation for both doors; the menu-owned origin is passed in as a render input. The two-level dispatch that exists today (title vs title-origin Settings/Gallery vs overlay, with the overlay dispatching its sub-view content) is kept, so no leaf decides on its own to mount.
- **Not moved:** `MenuState` and its navigation/focus/confirmation/origin facts stay `menu.rs`'s; the settings file and `SettingsState` stay `settings.rs`'s; `Notice`, `DefensiveHold` and the failure texts/levels stay `failure.rs`'s; `Runtime`, the runner and staging stay core's; the read log, the slots and the auto/skip clocks keep their owners. The Monitor drawer's presentation and widget syncs stay [reading anchor](#reading-anchor--decided-seam-2026-10-13)'s; only its imports change, and its two `SettingsDrag` clears become the settings view's cancel operation.
- `report_silent_degradations` stays with the play group because it sits in the Runtime-gated play chain and funnels into the failure posture; the watcher it settles is `assets`'s.

### Interfaces

- `play`'s `request_replay` replaces the free `force_replay` call the load, New Game and Timeline paths make today (`menu.rs:2267`, `menu.rs:2396`, `input.rs:454`), and the reveal reset (`menu.rs:2256`, `menu.rs:2386`) stays behind `TextReveal`'s existing methods. Callers borrow the separately stored resources and call the operation; they never set a reset flag or clear a cache field.
- `assets`'s watch operations are the only asset-load path; the one-per-path deduplication is preserved, including across the play path, the title backdrop and the Gallery.
- The shared `style` faces and maths are the single producers the play, title, overlay, Settings, Gallery and Timeline builders — and the Monitor drawer's font read — use. No duplicated metric formula and no reader state: the drawer consumes [reading anchor](#reading-anchor--decided-seam-2026-10-13)'s derivation as settled there.
- Visibility: builders are `pub(super)` (the coordinator calls them); the derivations and components the existing tests already cross stay `pub(crate)` as today.

### Registration and schedule

- The same 27 named systems (20 `Update`, 4 `PostUpdate` geometry, 2 `Content`, 1 `Startup`), run conditions and edges remain: the `Update` set chain `RuntimeCoreSet → NoticeSet → PresentationSet → MonitorDrawerSet`, the Runtime-gated play chain, the menu-tree chain, the four `PostUpdate` systems between `UiSystems::Layout` and `PostLayout`, the two `Content` systems before `EditableTextSystems`, the ownership gates, `PresentationSet`'s deliberate self-ambiguity and the shipping-schedule ambiguity gate.
- Each module registers its own group (its systems with their intrinsic edges); `lib.rs` composes the groups in today's order and keeps the app-level set chain and `register_ownership_gated_presentation`.
- Resource registration splits as today: `RuntimeCorePlugin` initializes the six headless state resources through the owners' functions, so a headless core app keeps the state its input and session tests exercise; `UnmeiPlugin` registers the audio channels, `AssetWatch`, `MenuBuilt`, the observer, `setup_ui` and every system.

### Composition with the settled ownership

- **[ADR 0012](adr/0012-menu-and-live-session-transition-ownership.md).** Session recipes own *when* an entry or replacement replays or resets; `play` owns *how* through `request_replay` and the reveal reset. No second recipe owner, no new transition phase and no exposed reset fields. The session-design contract repairs and extraction keep their own sequencing, and whichever implementation lands second honours this composition.
- **Reading anchor.** The partition owns the layout/font/window inputs and the rendering and consumes the reader's shared metric derivation; it introduces no second metric formula and no reader state. The drawer's widget syncs are not redesigned.

### Validation contract for the later build

- **Direct seam cases (the only additions):** the coordinator's rebuild boundary (a focus-only or label-only change does not rebuild; a structural change does; a confirmation change carries the Timeline scroll) and the play replay operation (one request replays audio, visuals and UI exactly as the three call sites do today). Both use the production `pub(crate)` seam; no new test door.
- **Existing suite:** `app/src/tests.rs` keeps its scenarios and assertions; only the `presentation::` paths of moved items requalify. The named witnesses the extraction must keep green include the title (`boot_leaves_the_title_up_over_the_entry_scenes_opening_frame`, `the_title_freezes_the_play_path_byte_identical`, `the_title_draws_the_declared_backdrop_covering_the_window`, `no_play_frame_is_drawn_while_the_title_is_up`), the overlay (`esc_opens_and_closes_from_every_parked_state_unchanged`, `menu_cards_render_exactly_what_list_slots_yields`, the Legacy card and import tests), Settings (`the_key_field_focus_model_walks_edits_commits_and_discards`, `the_editable_field_receives_keyboard_input_and_a_trimmed_in_process_paste`, `title_settings_opens_the_shared_view_with_the_title_door_and_backs_out`, `the_decisions_group_is_the_last_walk_stop_from_the_title_door`), the Gallery (`the_gallery_grid_is_three_across_with_a_twenty_pixel_gap`, `fit_gallery_still_contain_fits_the_image_above_the_foot_strip`, `a_path_the_play_path_reported_is_not_reported_again_by_the_gallery`, `the_gallery_back_control_is_door_aware`), the Timeline (`focus_starts_at_the_first_visited_declared_card_otherwise_the_first_declared`, `the_confirm_copy_is_exact_and_uses_jump_and_cancel`, `cancelling_the_confirm_restores_the_scrolled_body_offset`), the play frame (`at_instant_the_say_line_is_whole_on_the_first_frame_and_a_press_advances`, `a_fifty_char_reveal_takes_about_a_second_and_never_splits_a_codepoint`, `a_press_mid_reveal_completes_the_line_and_is_consumed`, `a_say_wait_restored_from_a_slot_re_reveals_from_the_first_character`, `the_overlay_freezes_the_reveal_and_resumes_where_it_stopped`, `load_signals_presentation_to_replay_visuals_and_audio`, `missing_background_sprite_and_music_each_warn_once_and_play_continues`, `a_missing_choice_sound_reports_row_6_once`, `the_error_screen_quits_nonzero_on_a_key_or_a_click`, `a_broken_pack_never_shows_the_title`) and the face tests.
- **Rendered and whole-app:** `cargo test --locked -p unmei-app --test shipping_schedule` (schedule construction with ambiguity detection still `Error`); the `monitor_acceptance` walk in its four modes; `monitor_smoke` at the default, `MONITOR_SMOKE_FRAME_MS=150` and `MONITOR_SMOKE_FRAME_MS=400`; and a real shipped-app boot with human inspection. Headless green alone does not prove the drawn surfaces.
- **Build gates:** the focused play/title/overlay/settings/gallery/timeline suites, then `cargo fmt --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace`.
- None of these checks is run or claimed by this planning ticket. No behaviour, layout, copy, format or persistence change is included.

### Handoff and scope

**Reconciled 2026-10-16:** the extraction landed on 2026-10-15, followed by the ownership tightening on 2026-10-16; it is no longer a pending implementation handoff. The remaining notes in this paragraph describe the historical planning baseline, not current coverage claims. Recorded direct-coverage gaps that were residuals unless the two new cases closed one: the Notice text node, the Timeline scroll system alone, the replay operation and the choice-SFX load have no direct test today. One living-spec sentence names `app/src/presentation.rs` as a module home (`docs/title-and-menu.md` § Gallery `### Entries` §4, the sprite-path producer); the extraction requalifies that path then — this design amends neither that document nor the dated research files that cite the old module — and `docs/title-and-menu.md`'s behaviour, layout and type-system rules are untouched. No `engine/` or CLI change, no `MenuState` semantics change, no Monitor widget redesign, no new domain concept and no glossary change.

---

## Shipping composition — proof-gated shared owner

Selected during the save map's design work: six interview rounds, Q1–Q8 and explicit complete-design confirmation Q9. **Conditional candidate, plan-only; extraction is not authorized before the bounded proof below passes.** No Rust implementation, build, test, boot or prototype ran in that design work. No current schedule-relevant drift was demonstrated: this addresses future plugin-selection drift, not a known broken gate.

**Production delivery (2026-10-17).** `app/src/composition.rs` now owns the single upstream-derived selection, shipping overrides and private headless delta through the two constructors below. `app/src/main.rs` calls `shipping_app`; `app/tests/shipping_schedule.rs` calls `schedule_app`. The separately authorized extraction is implemented and delivered on `main`; a later delivery tip also includes the target API narrowing and preconstructs all startup resources in the binary before its insertion-only hook. Fresh fault/recovery, actual-binary startup/sink and Windows/Linux evidence was gathered, and its explicit acceptance status — including which witnesses were not re-run — is stated where that evidence is recorded, not claimed here. The dated plan/prototype/outcome paragraphs below remain historical; this paragraph supersedes their former pending-production status, not their observations or the settled constructor/exclusion contract. No prototype branch was merged and no throwaway path, observer or workflow reaches production.

**Reconciled 2026-10-15:** the bounded proof has since run and passed on a throwaway prototype branch; see [Bounded feasibility outcome](#bounded-feasibility-outcome--fully-proven-2026-10-15). The candidate was recorded **fully proven** at that time; extraction was treated as permitted as a separate, still-unstarted behavior-preserving handoff. No prototype content is on the shipping branch and this section's ownership/exclusion decisions are unchanged.

**Superseded 2026-10-17:** the fresh carried-candidate evidence — three separate checks of schedule sensitivity, candidate-binary startup/resource/sink parity and native candidate shipping acceptance — re-observed the candidate atop the delivery baseline and did not complete the applicable F witness contract. The bounded status is **feasibility unmet**, not fully proven and not no-change: the earlier fully-proven record stands as history, no extraction handoff is published, and the binary and curated gate stay unchanged. See [Bounded feasibility outcome — feasibility unmet](#bounded-feasibility-outcome--feasibility-unmet-2026-10-17).

Source baseline is the then-current local `HEAD`: `app/src/main.rs:83–159` installs `RuntimeConfig` and the session-log result before configured `DefaultPlugins`, then kira, production startup resources and `UnmeiPlugin`; `app/tests/shipping_schedule.rs:55–111` independently enumerates the schedule-relevant headless set, sets ambiguity detection to `Error`, executes three updates and asserts `Runtime` with no `StartupFailure`. The motivating schedule defect and earlier experiments were recorded while this composition was designed; those historical reports are not a current-HEAD feasibility result.

### Interface and implementation ownership

A new production library module, `app/src/composition.rs`, exposes only two named constructors through `unmei_app::composition`. The external integration test calls the real owner, not a test-only reconstruction. Illustrative spelling is compile-validated in the later build; ownership, order and witness claims are binding:

```rust
pub fn shipping_app(
    config: RuntimeConfig,
    window: Window,
    session_log: Result<SessionLog, LogOpenError>,
    install_startup: impl FnOnce(&mut App),
) -> App;
pub fn schedule_app(config: RuntimeConfig) -> App;
```

The module owns the shared `DefaultPlugins`-derived selection, variant delta, plugin overrides/prerequisites and runtime installation order. These dependencies are concrete and in-process: Bevy's group builder, app plugins and supplied resources. No adapter trait, generic container, public plugin-list builder, mode flag, resource bundle or process-input resolver is introduced. The functions return an unrun `App`; neither runs frames nor owns `AppExit` or assertion policy.

**Shipping sequence:** `App::new` → supplied `RuntimeConfig` → supplied `SessionLog` or `LogOpenError` → configured `DefaultPlugins` in its native order → kira audio → insertion-only startup hook exactly once → `UnmeiPlugin` → return `App`.

- The binary retains launch parsing and all help/usage early returns before IO or App creation, best-effort pack-path resolution, environment/credential/settings access, resolver factory and checker construction, window resolution, and session-log opening/truncation before pack startup. It passes the pre-resolved log result; the owner inserts it before the fixed `failure::app_log_layer` hook. A failed open remains the existing `LogOpenError`/row-21 posture, not a new constructor error.
- Derive `AssetPlugin.file_path` once from `config.pack_root.join("assets")`. Preserve the existing `LogPlugin { custom_layer: app_log_layer, ..default() }`, `AssetPlugin { file_path, ..default() }` and `WindowPlugin { primary_window: Some(supplied_window), ..default() }`, including every unspecified default. `main` supplies `Window { mode: settings::window_mode(loaded.settings.play.fullscreen), ..settings::window_config() }`; no windowed first frame or new exit behavior.
- The hook inserts already-created `StartupSettings`, `StartupCredentials`, `ClipboardPort::System`, `KeyCheckChannel` and `DecisionFactory`, after kira and before `UnmeiPlugin` as today. It performs no IO, installs no plugins and does not replace `RuntimeConfig`. This concrete phase hook is not a framework; its restrictions are a documented caller contract, **not type-enforced policing of arbitrary closure behavior**. Independent production witnesses must protect the joins.
- Keep both distinct audio plugins exactly once: Bevy audio from the upstream group, and kira after that group. Keep `TextPlugin` before embedded-face use and kira before app audio-channel registration. Preserve the binary's final `app.run()` and returned `AppExit`, never a hard process exit.
- Freshness and one-shot construction are structural: callers supply no App with preinstalled configuration/plugins. `App::new()` itself installs `MainSchedulePlugin`; “fresh” does not mean literally plugin-free. Locked Bevy exposes typed plugin queries, not a public full registry, so the earlier mutable-installer idea could not universally reject an arbitrary caller's preinstalled plugin. Q6's note added the log-result argument; Q8 replaced mutable installation with fresh constructors, superseding Q4's illustrative shape without broadening resource/IO ownership.

### Headless variant and authoritative selection

Both constructors consume the same private upstream group-building/configuration owner. The shipping branch retains `DefaultPlugins` unchanged apart from today's three overrides. The schedule branch uses a **private, coverage-preserving subtraction/addition delta**, not another independently enumerated complete plugin list.

The gate keeps input/focus/dispatch, Window, Asset, Image/TextureAtlas, Text, picking, UI/widgets, both audio plugins and the real `UnmeiPlugin`, with dev mode explicitly on as today. It omits renderer/window backend/global logging, terminal-signal handling, gamepad probing and real app credential/network/clipboard ports. Only source-justified in-process extras are eligible; their retention needs a reason and strict executed-frame evidence. This is not permission to homogenize every harness or change Cargo features.

Locked Bevy 0.19.1 facts bound the candidate (local registry source under `$CARGO_HOME/registry/src/<registry>/`):

- `bevy_internal-0.19.1/src/default_plugins.rs` declares feature/platform-dependent membership and native order. Its final private `IgnoreAmbiguitiesPlugin` cannot be named for disabling. Its current acceptances activate for editable-text input + `SpritePlugin`, and `AnimationPlugin` + `UiPlugin`.
- Keep those inherited suppression branches **inactive** by excluding Sprite and Animation. Supply required atlas support explicitly at a source-backed slot; excluding Sprite also removes its nested support, so a superficial two-disable recipe is insufficient. Audit the private plugin on a dependency upgrade; do not silently inherit a newly active suppression.
- `bevy_app-0.19.1/src/plugin_group.rs` provides `contains`, `set`, `disable` and ordered finishing. `disable<T>` preserves the slot but panics if the type is absent; finishing overlapping groups can duplicate plugins. Keep the feature/platform census in the composition owner, not in callers; never mirror upstream cfgs casually or suppress absence without documenting why a target is optional versus required.
- Do **not** add `MinimalPlugins` over the narrowed upstream group: overlapping task/time/frame plugins would duplicate. The gate adopts upstream relative order and is explicitly `update()`-only; it need not install a runtime schedule runner. This is a deliberate gate change requiring sensitivity evidence, not a production runner change.
- Render dependants require inspection beyond a `RenderApp` guard. For example, `bevy_core_pipeline-0.19.1/src/lib.rs::CorePipelinePlugin::build` registers an embedded asset and child plugins **before** its render-app guard. Source plausibility is not runnable proof.

Existing audio initialization, asset tasks and Bevy clipboard-plugin availability remain retained prerequisites/exceptions; this does **not** promise zero OS work. The gate must not access real app clipboard/credentials/provider ports. Real resource construction remains binary-owned; gate resources retain the current hermetic defaults.

`shipping_schedule` visibly retains `ScheduleBuildSettings { ambiguity_detection: LogLevel::Error, ..default() }`, three updates, and the actual demo-pack `Runtime`/no-`StartupFailure` assertions. No new ambiguity acceptance or broad ignore is permitted. Existing `UnmeiPlugin` acceptances stay unchanged, including `PresentationSet`'s deliberate self-acceptance and the named third-party conflicts; that existing intra-phase blind spot is not expanded or claimed solved. Headless green does not witness Monitor capture/file sinks, GPU/window startup or rendered behavior.

### Depth, alternatives and settled-owner composition

The leverage is one authoritative upstream-derived selection plus variant/prerequisite/phase knowledge used by both real callers. Delete this module and those choices and order constraints reappear at both entry points. Merely co-locating two lists, asserting plugin names, or sharing only kira/`UnmeiPlugin` does not pass that deletion test.

The design-it-twice comparison rejected an explicit common list plus production insertions: retaining overlapping `DefaultPlugins` duplicates plugins, while reconstructing it duplicates feature/platform order and private behavior. A broad complete-App resource bundle was rejected for needless startup-type exposure and because wrapping two lists alone gives no depth. A shared-tail installer was rejected for leaving the load-bearing head lists independent; a proposed log-after-plugin sequence also contradicted the real hook. The selected fresh constructor is **not** that broad bundle: IO and resource construction/insertion stay at the caller's existing phase, and real shared selection remains required. A reasoned no-change result is first-class if feasibility cannot earn the seam.

Compose with [ADR 0013](adr/0013-presentation-surface-partition.md): library plugin code still composes app-level named schedule groups; per-surface realization, embedded-font ownership, input ownership and the ownership-gated edit drain retain their accepted owners/edges. This entry-point module becomes no second presentation coordinator. `monitor_smoke` and `monitor_acceptance` are optional future adopters, not required justification or part of this extraction. Their fixture/mode/resolver/geometry/pacing behavior remains separate.

### Bounded feasibility and independent validation

At the save map's close, create a **separate feasibility handoff before extraction**. A proof-gated candidate is the resolved planning outcome, not an approved implementation. The handoff must specify:

1. **Census before trials.** Record each upstream member as keep/disable/add with its reason, nested prerequisites, feature/platform availability, slot/order and inherited suppression consequences. Record field-by-field Log/Asset/Window override parity, every resource phase, loss of `MinimalPlugins` and update-only semantics. Cover the supported Windows/Linux targets; all delta knowledge remains private/local. If keeping the delta becomes unmanageable upstream cfg-tree duplication, stop rather than silently increasing scope.
2. **At most two source-backed delta candidates.** One initial candidate, then at most one correction supported by the actual failure and locked source. This bounds candidate designs, not test-command repetitions or required platform/sensitivity witnesses. No repeated disabling guesses. Apply the repository recovery rules; a missing prerequisite is not permission for broad ignores, new suppression, unrelated fixes or changed production behavior.
3. **Real shared owner and strict positive witness.** Put the actual candidate constructors behind both binary and integration test. `cargo test --locked -p unmei-app --test shipping_schedule` must construct strict schedules, run three frames, load the real demo pack and retain both assertions without GPU/display/credentials. Verify currently exercised paths/prerequisites, no duplicate plugin and the absence of prohibited variants and Sprite/Animation suppression triggers. Plugin-presence checks supplement, never replace, executed behavior.
4. **Two independent negative controls.** Reintroduce the known ownership-drain cycle and independently inject a distinct app-owned unordered conflict; the strict candidate gate must fail on each. Record actual failures using an explicit throwaway prototype branch per repository policy, then remove each fault and require the positive gate again. Do not replace sensitivity evidence with call counts or a plugin-name comparison; permanent redundant negative tests are not required.
5. **App/launch/resource witnesses.** Run `cargo test --locked -p unmei-app`, preserving the real binary help/usage exit-and-no-IO tests, app integration, credential safety and sink tests. Independently exercise the shipping constructor's file sink through the binary: the pre-opened log must actually receive expected startup/pack diagnostics, not merely exist as a resource. Retain config-before-capture, failed-open independence, pre-runtime startup values and hermetic gate defaults; never claim existing layer-only tests alone prove the binary join.
6. **Independent shipping and rendered evidence.** Observe real debug and release-`--dev` shipping boot through the existing on-demand boot path (bounded, panic-free operation, not a clean self-exit). Use isolated pack/config directories and no real key/provider credentials. Check affected real shipping window/fullscreen, input, embedded-font and log behavior; retain applicable existing rendered geometry/input/sink walks and their distinct claims. An example that does not adopt the new owner cannot prove that owner's wiring. Cover Windows and Linux before adoption; missing platform/boot evidence leaves the prerequisite unmet. Headless green cannot substitute for these checks or change CI frequency.
7. **Outcome gate.** Only after all constraints/witnesses pass may a separately sequenced behavior-preserving extraction handoff proceed. If neither candidate satisfies the contract within the bound, record **no-change**, retain today's binary and curated gate, and stop. No automatic switch to another interface; redesign requires explicit reopening. Unavailable evidence is not a pass or evidence that the candidate is impossible.

Before merge of any later extraction: `cargo fmt --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace`, `python3 scripts/check-invariants.py`, `python3 scripts/check-invariants.py --self-test`. Follow [the testing page](testing.md#shipped-path) and the repository's verification guidance. These are future commands, **not validation run in this design session**.

### Record, handoff and exclusions

This dated conditional section is the owning record; [Testing policy — Shipped Path](testing.md#shipped-path) distinguishes the current witness from the proposed one. No ADR: the selected entry-point wiring remains reversible and conditional, not a hard-to-reverse domain trade-off. No glossary change: no new domain concept. The portable resolution is published on the composition-selection design record; feasibility and subsequent extraction handoffs are created when the map closes, not here, and extraction is sequenced after successful proof, not after mere closure of the design work.

Remaining risks are unproven runnable delta/maintenance cost, platform/production parity and the documented startup-hook contract. Preserve all prior save-restoration working-tree edits. Explicit exclusions: production implementation or Rust validation/prototypes in this design; Cargo-feature pruning; renderer/window/backend behavior changes; process/global logging in the hermetic gate; credential sourcing/storage/diagnostic changes; new ambiguity suppression or weakened tests; all-harness standardization; presentation/menu/session redesign; generic plugin/container frameworks; CI cadence changes; commits/pushes, journey-page updates or another child resolution.

### Maintainer-approved F evidence reconciliation (2026-10-17)

The maintainer approved the explicit reconciliation recommended after the renewed evidence audit: portable authorization and revised requirements. This section supersedes the **live** feasibility-unmet disposition below, not its historical evidence or correctness under the previous bar. It amends F's evidence requirements, not the constructor interface or the two-candidate bound.

- **Windows** retains real-GPU human-reviewed affected fullscreen/input/font acceptance, native debug/release-`--dev` boots and actual-binary startup/sink observations. The existing reviewed candidate frames remain attributed to their original revision; later delivery requires fresh affected native execution.
- **Linux** is CI-only: native debug/release-`--dev` bounded boots, functional checks and console/session-log diagnostics remain required. Real-compositor fullscreen geometry, physical input and visible embedded-font rendering are **accepted unverified residuals, never PASS**; their absence no longer blocks the composition's feasibility or extraction. The later archive-bound release-certification contract remains independent.
- **Key-check composition evidence** requires observed channel installation/startup phase, source verification of the binary's construction/insertion of `HttpKeyCheck`, and independent request/status/redirect behavior tests at the existing checker boundary. The actual binary supplied-key/verdict path remains unverified and is not required for composition acceptance. No real provider request or endpoint-injection feature is authorized.
- All other constraints remain: one private upstream-derived owner, strict executed schedules, independent cycle/conflict controls and recoveries, all other meaningful startup resource values, actual file sinks and failed-open behavior, unchanged ambiguity acceptances/presentation ownership, and complete workspace/invariant checks. A later extraction still needs **fresh production-revision evidence**; prototype success cannot certify it.

Under that revised bar, carried candidate 2 atop the same delivery baseline is **fully proven for bounded composition feasibility**: the three fresh evidence records establish the strict schedules, two faults/recoveries, startup/resources/sinks and native observations, and the renewed audit independently repeated both schedule faults/recoveries. Source shows the production checker explicitly inserted by the binary; existing engine request/status/redirect and app lifecycle tests protect its own boundary. Neither missing Linux real-compositor proof nor an unobserved binary verdict is relabelled PASS. No third candidate or retrospective extraction authorization is implied.

The separately scoped delivery owner is the production delivery work, a native child of the feasibility decision with a verified blocked-by edge to the reconciliation; its production implementation and fresh validation receipt are recorded above. The maintainer separately authorized pushing a retained verification/prototype branch and dispatching the existing on-demand Linux workflow for fresh delivery evidence, **not pushing main or changing CI cadence**. The feasibility decision and its parent map remain open until delivery, validation and the complete handoff/residual record are satisfied.

### Bounded feasibility outcome — fully proven (2026-10-15)

> **Superseded 2026-10-17 by [the feasibility-unmet outcome](#bounded-feasibility-outcome--feasibility-unmet-2026-10-17).** This is the historical prototype record, preserved with its evidence and limitations. Its "fully proven" status and extraction authorization are not live: the current bounded status is **feasibility unmet** and no extraction handoff is published.

Ran on a throwaway prototype branch, branched from `main`. **Two source-backed candidates** (the initial design plus one correction), within the bound. Candidate content is one prototype commit; the branch tip restores that content after two evidence-only fault/revert commits. Linux boot evidence was recorded by a GitHub Actions run. Throwaway only: **no prototype content is on the shipping branch**, and it was recorded as authorizing a separate extraction handoff rather than performing it.

**Census, before trials (locked Bevy 0.19.1).** With the enabled feature set (`bevy_window`, `bevy_winit`, `bevy_render`, `bevy_pbr`, `bevy_sprite`, `bevy_animation`, `bevy_gizmos`, `bevy_gilrs`, `bevy_state`, `system_clipboard`, …, no `http`/`https`, no `dlss`, no `bevy_ci_testing`, no `bevy_dev_tools`, no `hotpatching`), the active `DefaultPlugins` members in native order are: `PanicHandlerPlugin`, `LogPlugin`, `TaskPoolPlugin`, `FrameCountPlugin`, `TimePlugin`, `TransformPlugin`, `DiagnosticsPlugin`, `InputPlugin`, `InputFocusPlugin`, `InputDispatchPlugin`, `WindowPlugin`, `AccessibilityPlugin`, `TerminalCtrlCHandlerPlugin`, `AssetPlugin`, `WorldSerializationPlugin`, `ScenePlugin`, `WinitPlugin`, `RenderPlugin`, `ImagePlugin`, `MeshPlugin`, `CameraPlugin`, `LightPlugin`, `PipelinedRenderingPlugin`, `CorePipelinePlugin`, `PostProcessPlugin`, `AntiAliasPlugin`, `SpritePlugin`, `SpriteRenderPlugin`, `ClipboardPlugin`, `TextPlugin`, `UiPlugin`, `UiRenderPlugin`, `GltfPlugin`, `PbrPlugin`, `AudioPlugin`, `GilrsPlugin`, `AnimationPlugin`, `GizmoPlugin`, `GizmoRenderPlugin`, `StatesPlugin`, `UiWidgetsPlugins`, `DefaultPickingPlugins`, `IgnoreAmbiguitiesPlugin`. Absent by cfg (and therefore not disabled): `WebAssetPlugin`, `DlssInitPlugin`, `CiTestingPlugin`, `RenderDebugOverlayPlugin`, `HotPatchPlugin`, and — because `bevy_window` is on — `ScheduleRunnerPlugin`. The private `IgnoreAmbiguitiesPlugin` cannot be named; its two conditional branches stay inactive because `SpritePlugin` and `AnimationPlugin` are disabled.

**Delta and parity.** The shared owner is `DefaultPlugins.build()`; shipping applies today's three overrides (`LogPlugin { custom_layer: app_log_layer, ..default() }`, `AssetPlugin { file_path: pack_root/assets, ..default() }`, `WindowPlugin { primary_window: Some(window), ..default() }`) and every other upstream default, then kira audio, the caller's one-shot insertion hook (startup settings/credentials, `ClipboardPort::System`, key-check channel, resolver factory), then `UnmeiPlugin`. The gate's coverage-preserving subtraction keeps `TaskPoolPlugin`, `FrameCountPlugin`, `TimePlugin`, `InputPlugin`, `InputFocusPlugin`, `InputDispatchPlugin`, `WindowPlugin`, `AssetPlugin`, `ImagePlugin`, `TextPlugin`, `UiPlugin`, `AudioPlugin`, `UiWidgetsPlugins`, `DefaultPickingPlugins` and the private group; disables the renderer/window backend/global logger/terminal-signal/gamepad set plus the extras today's `MinimalPlugins` gate never installed; adds `TextureAtlasPlugin` before `TextPlugin` (where `SpritePlugin` used to nest it), `bevy_kira_audio::AudioPlugin` and `UnmeiPlugin`. `ClipboardPlugin` is disabled but `TextPlugin` re-adds it when absent, reproducing today's ordering. No `MinimalPlugins` is layered — the gate is upstream-ordered and `update()`-only (no runtime schedule runner). Both audio plugins occur exactly once; no prohibited plugin is present.

**Candidate 1 — rejected.** Same owner and shipping overrides, disabling only the renderer/backend/logger/signal/gamepad set and keeping the remaining upstream extras. Strict gate panicked: `Encountered an error in system bevy_gizmos::update_gizmo_meshes<bevy_light::gizmos::LightGizmoConfigGroup>: Parameter ResMut<Assets<GizmoAsset>> failed validation: Resource does not exist`.

**Candidate 2 — the passing design.** Source-backed correction: `bevy_light::LightPlugin` unconditionally registers `LightGizmoPlugin` (under `bevy_gizmos`), which needs the disabled `GizmoPlugin`'s `Assets<GizmoAsset>`; the correction disables the whole upstream-extra set the `MinimalPlugins` gate never installed, preserving the gate's existing coverage instead of widening its prerequisite/ambiguity surface. `cargo test --locked -p unmei-app --test shipping_schedule` is green: real `schedule_app(config)`, visible `ambiguity_detection: Error`, three `update()`s, demo-pack `Runtime` present and no `StartupFailure`, with supplementary presence checks (no `WinitPlugin`/`RenderPlugin`/`SpritePlugin`; `AudioPlugin` and `ImagePlugin`/`TextureAtlasPlugin` present).

**Independent negative controls.** Re-introducing the exact ownership-drain edge (`.after(InputFocusSystems::FocusChangeEvents)` on the ownership-gated drain) failed the strict gate with `Error when initializing schedule PostUpdate: schedule has 6930 before/after cycle(s)` (exit 101). A distinct app-owned unordered conflict (two `Update` systems sharing a resource, outside `PresentationSet`) failed with `Error when initializing schedule Update: 1 pairs of systems with conflicting data access have indeterminate execution order` (exit 101). Each fault was removed and the positive gate re-passed; no permanent negative suite, weaker assertion or new ignore was added.

**Shipping/resource parity (real binary, isolated pack and config, no real key).** On Windows, debug and release `--dev` both ran to a 20 s bound (exit 137, no panic; window created on Vulkan/NVIDIA), and the real session-log sink carried the startup diagnostics, a broken pack's `pack.ron: invalid RON: 1:5` diagnostic, and a malformed `StartupSettings` warning — observing `RuntimeConfig`-before-capture and those pre-runtime startup values. With `<pack>.log` made a directory, the failed open stayed independent: the row-21 `Access denied (os error 5)` was reported and the app continued to the bound. On Linux (GitHub `boot` workflow run `38758787144`, `ubuntu-latest`, xvfb, llvmpipe) the same candidate built and booted debug and release `--dev` to a bounded kill with no panic lines (job `success`); that run is boot smoke evidence only — the sink/resource witnesses above are Windows.

**Validation observed.** `cargo fmt --check` clean; `cargo clippy --locked --workspace --all-targets -- -D warnings` clean; `cargo test --locked -p unmei-app` green (583 unit + all integration witnesses); `cargo test --locked --workspace` 1018 passed / 0 failed; `python scripts/check-invariants.py` 3 invariants ok and `--self-test` 8 ok. (`python3` is shadowed by the Windows Store alias on this host; `python` 3.12.1 was used.)

**Outcome: recorded fully proven against the bounded feasibility criteria at the time.** This classifies feasibility, not general rendering or input correctness, and not exhaustive startup-resource parity: the hook's credential/channel/resolver wiring is preserved by source comparison with `main` and is not independently witnessed by the sink logs. Residuals: the candidate preserves [ADR 0013](adr/0013-presentation-surface-partition.md)'s presentation ownership, so no separate rendered geometry/input walk was re-run — the bounded shipping boot is not a rendering or input-geometry proof, and a real-compositor walk remains the human residual if extraction later touches presentation. The startup hook's restrictions remain a documented caller contract, protected here by the sink/resource witnesses; upstream `IgnoreAmbiguitiesPlugin` must be audited on dependency upgrades. Extraction is a separate handoff and still requires its own full pre-merge bar.

### Bounded feasibility outcome — feasibility unmet (2026-10-17)

> **Historical disposition under the previous evidence bar.** Superseded prospectively by the [explicit maintainer-approved evidence amendment](#maintainer-approved-f-evidence-reconciliation-2026-10-17), not by fabricated evidence or a reinterpretation of the later reconciliation. The missing observations below remain unverified; the amended contract accepts those limitations.

A later reconciliation of the fresh carried-candidate evidence against the contract in the maintainer-approved F evidence amendment recorded exactly one bounded status. The full synthesis is summarised in this section, which is the owning status.

**Common revision.** The three evidence owners share one identity: one clean full candidate commit, mechanically carried onto the same delivery baseline, toolchain `1.98.1`, `Cargo.lock` sha256 `2c5ef5d4766265ec290e8949a6fb1700492e1ddbebc1597b259fe101206e8dfa`. The four candidate paths are byte-identical to the content authority commit; no third candidate, alternative interface, feature pruning, suppression or unrelated repair was introduced. Historical revisions and the earlier fully-proven outcomes are leads, not current observations.

**Proven by the fresh evidence.** The strict schedule gate was green on the carried candidate and remained *sensitive*: a re-injected ownership-drain cycle and a distinct app-owned unordered conflict each failed it for its own intended reason, with the positive gate re-observed after each revert. The actual binary held meaningful pre-runtime `StartupSettings`/`StartupCredentials`, `ClipboardPort::System` and the resolver factory, wrote the expected startup/specific-broken-pack/malformed-settings diagnostics into the real `<pack>.log`, and kept the deterministic log-open failure row-21-only with no Notice. Debug and release-`--dev` bounded boots ran panic-free on native Windows (real GPU, human real-compositor review of the affected fullscreen/input/font frames) and under the maintainer's Linux-CI-only amendment on native Linux.

**Not established.** Two required witnesses remain missing or partial, so *fully proven* is unavailable:

1. The key-check channel was observed **presence-only** (`Some` versus the hermetic `None`), because `HttpKeyCheck` deliberately targets the fixed real provider origin and exercising it would be a real provider request (a recorded residual). The F contract asks for pre-runtime **values** and phase, not presence alone.
2. The Linux affected window/fullscreen/input/font **rendered** cells are recorded **unmet-by-CI** under the non-negotiable Linux-CI-only amendment (Xvfb + Mesa `llvmpipe` is bounded executed boot and diagnostics, not a real-compositor/GPU walk). The later evidence criteria explicitly forbid allowing fully-proven feasibility while required cells remain absent. The amendment moves the human real-compositor observations to Windows — which were satisfied — but it does not convert the absent Linux rendered cells into PASS.

**Why not no-change.** Candidate 1 failed within the bound for a real reason (`LightPlugin`'s `LightGizmoPlugin` needing the disabled `GizmoPlugin`'s `Assets<GizmoAsset>`), and candidate 2 — the single allowed source-backed correction — has positive schedule, startup/sink and native-boot evidence with **no observed contract failure**. Missing evidence is not a demonstration that neither allowed candidate can satisfy the contract, so *no-change* is not established.

**Disposition.** No production extraction work is created. The curated `shipping_composition` gate remains authoritative and today's binary and gate are unchanged; the prototype candidate stays prototype-only. Missing witnesses and their owners: the key-check channel's meaningful value and the Linux real-compositor rendered behavior. The feasibility decision remains open and its body is unchanged; this synthesis records the bounded outcome but does not deliver the production shared owner or authorize extraction. A future extraction still requires a completed proof on the production delivery revision and the parent contract's full pre-merge checks selected by the repository's verification guidance; prototype success cannot certify a later extraction commit.
