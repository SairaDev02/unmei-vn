//! A refused legacy-save retirement must leave the completed slot, the still
//! importable source and every other slot exactly as they were (ADR 0006).
//!
//! The legacy receipt lives here, outside the `engine/src/save/tests/` unit
//! module, so the repository's staged-file size policy keeps its existing
//! ownership for those files. It exercises the real owning operations through
//! their public API: an ordinary directory at the retired destination refuses
//! the final rename after the manual slot has already been written.

use std::path::Path;

use unmei_engine::{
    import_legacy, imported_legacy_path_for, legacy_save, legacy_save_path_for, list_slots,
    load_pack, read_slot, save_path_for, write_slot, Pack, SaveError, SaveFile, ScriptRunner, Slot,
    SlotStatus, StagingState,
};

/// The demo pack pointed at a temp dir, so sibling slot files never dirty the
/// repo. The runner reads only pack content, never `root`.
fn temp_pack() -> (tempfile::TempDir, Pack) {
    let dir = tempfile::tempdir().unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/demo-pack");
    let mut pack = load_pack(&root).expect("the demo pack loads").pack;
    pack.root = dir.path().join("demo-pack");
    (dir, pack)
}

/// A valid save parked on the demo pack's first say-line.
fn parked_save(pack: &Pack) -> SaveFile {
    let mut runner = ScriptRunner::new(pack.clone());
    runner.run();
    SaveFile::capture(&runner, &StagingState::default())
}

fn status(pack: &Pack, slot: Slot) -> SlotStatus {
    list_slots(pack)
        .into_iter()
        .find(|info| info.slot == slot)
        .expect("slot is in Slot::ALL")
        .status
}

#[test]
fn a_failed_legacy_retirement_keeps_the_written_slot_and_the_importable_source() {
    let (_dir, pack) = temp_pack();
    let legacy = legacy_save_path_for(&pack);
    std::fs::write(&legacy, parked_save(&pack).to_ron().unwrap()).unwrap();
    let source_bytes = std::fs::read(&legacy).unwrap();

    // Distinguishable saves around the import target: slot 2 is replaced by the
    // import, slot 1 must survive byte-identically.
    let mut previous_target = parked_save(&pack);
    previous_target.pc = 0;
    previous_target.wait = None;
    write_slot(&pack, Slot::Manual(2), &previous_target).unwrap();
    write_slot(&pack, Slot::Manual(1), &parked_save(&pack)).unwrap();
    let neighbour_bytes = std::fs::read(save_path_for(&pack, Slot::Manual(1))).unwrap();
    let importable = legacy_save(&pack).expect("the legacy save is offered");

    // The retired destination is a directory, so the final rename cannot land
    // even though the slot write has already succeeded.
    let imported = imported_legacy_path_for(&pack);
    std::fs::create_dir(&imported).unwrap();

    let error = import_legacy(&pack, Slot::Manual(2)).unwrap_err();
    let SaveError::Io { path, .. } = &error else {
        panic!("the retirement failure is an IO failure: {error:?}");
    };
    assert_eq!(
        path, &legacy,
        "the failure names the source it could not retire"
    );

    // The completed slot write is not rolled back and holds the import.
    let landed = read_slot(&pack, Slot::Manual(2)).expect("the slot write landed readable");
    assert_eq!(
        landed, importable,
        "the legacy save replaced the seeded slot"
    );

    // The source is untouched and still offered, the obstruction is still in
    // place, and no other slot moved.
    assert_eq!(
        std::fs::read(&legacy).unwrap(),
        source_bytes,
        "the legacy source is byte-identical"
    );
    assert!(legacy_save(&pack).is_some(), "the import stays on offer");
    assert!(imported.is_dir(), "the obstruction is untouched");
    assert_eq!(
        std::fs::read(save_path_for(&pack, Slot::Manual(1))).unwrap(),
        neighbour_bytes,
        "the neighbouring slot is byte-identical"
    );
    for slot in [Slot::Manual(3), Slot::Autosave] {
        assert_eq!(status(&pack, slot), SlotStatus::Empty, "{slot:?} untouched");
    }
}
