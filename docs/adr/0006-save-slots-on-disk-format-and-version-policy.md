---
title: Four sibling save slots with an additive version floor
summary: Four sibling slot files beside the pack, each written atomically, with a version field as an additive floor that rejects zero and newer builds.
keywords:
  - save-slots
  - persistence
---

# Save slots: four sibling files and an additive version floor

The hardening effort turns the slice's single `<pack-dir>.save.ron` into **four fixed slots** — three the player writes, one the game writes. Two decisions are hard to reverse once players have files: **where the slots live on disk**, and **what a version mismatch means**. The earlier save-tooling research had already settled that no Bevy surface can carry the payload — the engine's serde/RON `SaveFile` stays, and slot *selection* is app-side — so this ADR fixes the remaining file contract.

**Decision.** Four sibling files beside the pack, 1-indexed: `<pack-dir>.slot1.save.ron`, `.slot2`, `.slot3`, and `<pack-dir>.autosave.save.ron`, each one `SaveFile` written atomically. `engine/src/save.rs` owns the slot vocabulary, suffix selection and slot operations — `Slot`, `save_path_for(pack, slot)`, `list_slots`, `read_slot` / `write_slot` — while staying Bevy-free; the shared sibling naming rule lives in `engine/src/sidecar.rs`. `app/` owns which slot the player picked and the overlay. The save format's `version` becomes an **additive floor**: `SAVE_VERSION` rises to 2 (adding `saved_at`), the loader accepts any `1..=SAVE_VERSION` with every post-v1 field carrying `#[serde(default)]`, and rejects version **zero** and versions **newer** than the build. A slot that fails to parse is shown unreadable, preserved, and still overwritable; it is never auto-deleted.

Why the floor: players' files outlive the build that wrote them. Strict `version == SAVE_VERSION` would invalidate every save on every additive field — the exact data loss this hardening effort exists to prevent — while accepting a *newer* version is unsound, since a future shape may mean something different. Preserving an unreadable slot and letting the player overwrite it keeps them in control of their own data.

## Considered options

- **One container file holding all four slots.** Rejected: a single bad byte or partial write loses all four, and per-slot atomicity would have to be invented. Four files keep today's temp-file-and-rename atomic write and isolate corruption.
- **A `saves/` directory beside the pack.** Rejected: no benefit over siblings, and it adds a directory to create and enumerate in a portable, no-installer layout.
- **Strict `version == SAVE_VERSION`.** Rejected above — every additive change would strand existing saves; retained only as the rule for *newer* versions.
- **App-side sidecar metadata** (`<slot>.meta.ron`) for the menu's scene/time. Rejected: a second file type that can desync from its slot, versus one optional `saved_at` field that travels with the save.
- **Handing layout to `app/`** (engine keeps only path-taking read/write). Rejected: the naming convention would be re-derived at each call site and would stay untestable headlessly; the engine already owned `save_path_for`.

## Consequences

- The slice's `<pack-dir>.save.ron` is no longer written. It is offered once as a **legacy save** entry and, on import, renamed to `<pack-dir>.save.ron.imported` — nothing is deleted.
- `write_save` / `read_save` remain as the path-taking primitives; the slot functions wrap them, and `save_path_for` gains the slot argument.
- A v1 save (no `saved_at`) loads under the floor with `saved_at: None`; the overlay shows its time as unknown.
- Every field added to `SaveFile` from now on must carry `#[serde(default)]`, or it silently narrows the floor.
- `.gitignore`'s `*.save.ron` keeps every slot file, and the legacy file, out of git; the `.imported` rename does not match that pattern, so an imported legacy file stops being ignored.
- Build-ready detail — autosave triggers, the `entered_scene` signal, the overlay contract, the test list — is in [`docs/runtime-hardening.md` § Save slots](../runtime-hardening.md).

## Amendment (2026-09-28, semantic slot checks)

The additive floor above is not enough on its own: a slot can decode, match the version and name a scene the pack still has, yet describe a position the script no longer has — a slot from an earlier revision of the same pack whose `pc` has drifted, or one copied in from another pack. The [runtime failure posture](../runtime-hardening.md#runtime-failure-posture) therefore **tightens the unreadable rule** with cheap semantic checks, entirely inside `engine/`: a slot is also `Unreadable` when `pc` is outside its scene's lines, when `pc == 0` and a wait is parked, when the state map holds a key `state.ron` does not declare, or when the parked wait's kind does not match the line at `pc - 1`. No field is added and no version is bumped, so the floor above is unchanged; a slot failing these checks is preserved, refuses to load with its reason shown, and stays overwritable, exactly like one that fails to parse. Detection by content — a pack identity or a position fingerprint inside `SaveFile` — was considered and rejected as a format change with its own version-policy consequences, recorded in the section's ADR [0008](0008-player-first-failure-posture.md)'s consequences and in § [Runtime failure posture](../runtime-hardening.md#runtime-failure-posture) §2's row 16. The test list in § [Save slots](../runtime-hardening.md#save-slots) §7 carries the cases.

## Amendment (2026-10-14, ownership and version wording)

The Decision paragraph now reflects the landed consolidation: `save.rs` keeps the slot vocabulary, suffix selection and slot operations; `engine/src/sidecar.rs` owns the shared `sibling_path` naming rule and `write_atomic` write idiom. This is the same ownership correction recorded in [ADR 0007](0007-settings-file-beside-the-pack.md), not a change to the sibling layout.

The version sentence also makes the existing zero-version refusal explicit: accepted versions are `1..=SAVE_VERSION`, so zero and newer versions are unsupported. The plan-only restoration design in [Runtime hardening — Save restoration](../runtime-hardening.md#save-restoration--one-semantic-validity-owner-2026-10-14) preserves that rule. Neither correction changes the format, version policy, semantic checks or Legacy save eligibility, or implements the pending restoration design.
