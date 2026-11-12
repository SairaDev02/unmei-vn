//! The four fixed save slots: their vocabulary, sibling paths, and the
//! operations that read, write and classify them
//! (`docs/adr/0006-save-slots-on-disk-format-and-version-policy.md`).

use std::path::PathBuf;

use crate::content::Pack;

use super::position::validate_position;
use super::{read_save, write_save, SaveError, SaveFile};

/// One of the four fixed save files beside the pack.
///
/// `Manual` accepts `1..=3`; [`Slot::ALL`] is the canonical list and menu
/// order. All four hold the same [`SaveFile`] — they differ only in who writes
/// them (the player, or the game's autosave).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Slot {
    Manual(u8),
    Autosave,
}

impl Slot {
    /// Every slot, in the overlay's canonical order.
    pub const ALL: [Slot; 4] = [
        Slot::Manual(1),
        Slot::Manual(2),
        Slot::Manual(3),
        Slot::Autosave,
    ];
}

/// What one slot holds, as the overlay needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotInfo {
    pub slot: Slot,
    pub status: SlotStatus,
}

/// A slot's state: empty, readable, or unreadable (with the reason to show).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotStatus {
    /// No file at the slot's path.
    Empty,
    /// A readable save; the scene id and the app-stamped epoch seconds.
    Ok {
        scene_id: String,
        saved_at: Option<u64>,
    },
    /// The bytes decode but the position is unusable, or the file cannot be
    /// read at all. The reason is a [`SaveError`]'s display text.
    Unreadable { reason: String },
}

/// The distinct sibling path for one slot: `<pack-dir>.slotN.save.ron` for a
/// manual slot, `<pack-dir>.autosave.save.ron` for the autosave.
///
/// "Next to the pack": the pack directory stays pristine and distributable;
/// every slot sits beside it (ADR 0006).
pub fn save_path_for(pack: &Pack, slot: Slot) -> PathBuf {
    let suffix = match slot {
        Slot::Manual(n) => {
            debug_assert!((1..=3).contains(&n), "manual slot {n} is outside 1..=3");
            format!("slot{n}")
        }
        Slot::Autosave => "autosave".to_string(),
    };
    crate::sidecar::sibling_path(&pack.root, &format!("{suffix}.save.ron"))
}

/// Classify every slot, in [`Slot::ALL`] order, without restoring it.
pub fn list_slots(pack: &Pack) -> [SlotInfo; 4] {
    Slot::ALL.map(|slot| SlotInfo {
        slot,
        status: slot_status(pack, slot),
    })
}

/// Read and classify one slot.
///
/// A save that decodes but describes a position the pack can no longer hold is
/// an error here, exactly as [`list_slots`] reports it — so, for the unchanged
/// saved value and this same pack, a slot listed `Ok` is one
/// [`SaveFile::restore`] will accept and a slot listed `Unreadable` never
/// reaches `restore`.
pub fn read_slot(pack: &Pack, slot: Slot) -> Result<SaveFile, SaveError> {
    let save = read_save(&save_path_for(pack, slot))?;
    validate_position(pack, &save)?;
    Ok(save)
}

/// Write one slot atomically, through the path-taking [`write_save`].
pub fn write_slot(pack: &Pack, slot: Slot, save: &SaveFile) -> Result<(), SaveError> {
    write_save(&save_path_for(pack, slot), save)
}

fn slot_status(pack: &Pack, slot: Slot) -> SlotStatus {
    let path = save_path_for(pack, slot);
    // `try_exists` separates "no file" from "cannot stat it": the latter is an
    // IO failure the caller should see as unreadable, not empty.
    if matches!(path.try_exists(), Ok(false)) {
        return SlotStatus::Empty;
    }
    match read_slot(pack, slot) {
        Ok(save) => SlotStatus::Ok {
            scene_id: save.scene_id,
            saved_at: save.saved_at,
        },
        Err(error) => SlotStatus::Unreadable {
            reason: error.to_string(),
        },
    }
}
