//! The slice's single `<pack-dir>.save.ron`: whether it is importable, and the
//! one-time import that retires it by rename
//! (`docs/adr/0006-save-slots-on-disk-format-and-version-policy.md`).

use std::path::PathBuf;

use crate::content::Pack;

use super::position::check_version;
use super::slots::{save_path_for, write_slot, Slot};
use super::{read_save, save_io_error, SaveError, SaveFile};

/// The slice's single `<pack-dir>.save.ron` — the one-time legacy file, offered
/// for import into a manual slot and then renamed aside (ADR 0006).
pub fn legacy_save_path_for(pack: &Pack) -> PathBuf {
    crate::sidecar::sibling_path(&pack.root, "save.ron")
}

/// The legacy save, when it is importable — it parses under the version floor
/// and names a scene this pack has. Anything else is `None`: hidden, and left
/// untouched on disk.
pub fn legacy_save(pack: &Pack) -> Option<SaveFile> {
    let save = read_save(&legacy_save_path_for(pack)).ok()?;
    check_version(&save).ok()?;
    pack.scenes.contains_key(&save.scene_id).then_some(save)
}

/// The path the imported legacy file is renamed to — `<pack-dir>.save.ron.imported`.
pub fn imported_legacy_path_for(pack: &Pack) -> PathBuf {
    let mut name = legacy_save_path_for(pack).into_os_string();
    name.push(".imported");
    PathBuf::from(name)
}

/// Import the legacy save into one manual slot, then rename the legacy file
/// aside to [`imported_legacy_path_for`] — the retired entry, nothing deleted
/// (ADR 0006).
///
/// The slot is written **before** the rename, so a failed target write leaves
/// the legacy file byte-identical and the player free to retry. The imported
/// save keeps its original `saved_at` (usually `None`): an import is not a new
/// save and must not fabricate a time. The autosave is never an import target.
pub fn import_legacy(pack: &Pack, slot: Slot) -> Result<(), SaveError> {
    if !matches!(slot, Slot::Manual(_)) {
        return Err(save_io_error(
            save_path_for(pack, slot),
            "the autosave is load-only; it is never an import target",
        ));
    }
    let Some(save) = legacy_save(pack) else {
        return Err(save_io_error(
            legacy_save_path_for(pack),
            "the legacy save is not importable",
        ));
    };

    write_slot(pack, slot, &save)?;

    let legacy = legacy_save_path_for(pack);
    let imported = imported_legacy_path_for(pack);
    std::fs::rename(&legacy, &imported).map_err(|e| save_io_error(legacy, e.to_string()))
}
