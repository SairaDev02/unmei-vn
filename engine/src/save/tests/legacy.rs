//! Legacy eligibility, import into a manual slot, and the rename that retires
//! the source file.

use super::*;

#[test]
fn legacy_is_importable_only_when_it_parses_and_names_a_live_scene() {
    let (_dir, pack) = temp_pack();
    let legacy = legacy_save_path_for(&pack);
    assert_eq!(legacy.file_name().unwrap(), "demo-pack.save.ron");
    assert_eq!(legacy_save(&pack), None, "no legacy file is not importable");

    // A v1 legacy save is offered; reading it leaves the bytes alone.
    let save = parked_save();
    std::fs::write(&legacy, v1_ron(&save)).unwrap();
    let before = std::fs::read(&legacy).unwrap();
    let importable = legacy_save(&pack).expect("a v1 save with a live scene imports");
    assert_eq!(importable.saved_at, None);
    assert_eq!(std::fs::read(&legacy).unwrap(), before);

    // A newer version is hidden and left byte-identical.
    let mut newer = parked_save();
    newer.version = SAVE_VERSION + 1;
    std::fs::write(&legacy, newer.to_ron().unwrap()).unwrap();
    let before = std::fs::read(&legacy).unwrap();
    assert_eq!(legacy_save(&pack), None);
    assert_eq!(std::fs::read(&legacy).unwrap(), before);

    // A save naming a scene the pack lacks is hidden and left alone.
    let mut stranger = parked_save();
    stranger.scene_id = "elsewhere".to_string();
    std::fs::write(&legacy, stranger.to_ron().unwrap()).unwrap();
    let before = std::fs::read(&legacy).unwrap();
    assert_eq!(legacy_save(&pack), None);
    assert_eq!(std::fs::read(&legacy).unwrap(), before);

    // Garbage is hidden and left alone too.
    std::fs::write(&legacy, b"\x00\x01 not ron").unwrap();
    let before = std::fs::read(&legacy).unwrap();
    assert_eq!(legacy_save(&pack), None);
    assert_eq!(std::fs::read(&legacy).unwrap(), before);
}

#[test]
fn legacy_import_writes_the_target_slot_and_renames_the_file_aside() {
    let (_dir, pack) = temp_pack();
    let legacy = legacy_save_path_for(&pack);
    std::fs::write(&legacy, v1_ron(&parked_save())).unwrap();
    let original = std::fs::read(&legacy).unwrap();

    import_legacy(&pack, Slot::Manual(2)).expect("the import lands");

    // The target holds the legacy save verbatim, `saved_at` untouched.
    let imported = read_slot(&pack, Slot::Manual(2)).expect("the slot reads back");
    assert_eq!(imported.saved_at, None, "the import fabricates no time");
    assert_eq!(status(&pack, Slot::Manual(2)), status_of(&imported));
    assert!(matches!(
        status(&pack, Slot::Manual(2)),
        SlotStatus::Ok { .. }
    ));

    // The rename is what retires the entry; the old bytes survive under the
    // `.imported` name and nothing is deleted.
    assert!(!legacy.exists(), "the legacy file leaves its old name");
    assert_eq!(legacy_save(&pack), None, "no entry remains");
    let imported_path = imported_legacy_path_for(&pack);
    assert_eq!(
        imported_path.file_name().unwrap(),
        "demo-pack.save.ron.imported"
    );
    assert_eq!(std::fs::read(&imported_path).unwrap(), original);

    // No other manual slot or the autosave was touched.
    for slot in [Slot::Manual(1), Slot::Manual(3), Slot::Autosave] {
        assert_eq!(status(&pack, slot), SlotStatus::Empty, "{slot:?} untouched");
    }
}

#[test]
fn an_eligible_but_semantically_invalid_legacy_save_imports_to_an_unreadable_slot() {
    let (_dir, pack) = temp_pack();
    let legacy = legacy_save_path_for(&pack);

    // Decode + accepted version + existing scene, but a `pc` past the end:
    // eligibility deliberately does not include the semantic checks, so the
    // file is still offered.
    let mut save = parked_save();
    save.pc = pack.scenes.get("first_meeting").unwrap().lines.len() + 1;
    std::fs::write(&legacy, save.to_ron().unwrap()).unwrap();
    assert!(legacy_save(&pack).is_some(), "eligibility is unchanged");

    import_legacy(&pack, Slot::Manual(1)).expect("the import lands");

    // The slot write and the rename both happened...
    assert!(!legacy.exists(), "the source is renamed aside");
    assert!(imported_legacy_path_for(&pack).exists());
    // ...and the imported slot now classifies unreadable, preserved.
    let SlotStatus::Unreadable { reason } = status(&pack, Slot::Manual(1)) else {
        panic!("the imported slot should classify unreadable");
    };
    assert!(reason.contains("past the end"), "got: {reason}");
    assert!(save_path_for(&pack, Slot::Manual(1)).exists());
}

#[test]
fn a_failed_legacy_import_leaves_the_legacy_file_untouched() {
    let (_dir, pack) = temp_pack();
    let legacy = legacy_save_path_for(&pack);
    std::fs::write(&legacy, v1_ron(&parked_save())).unwrap();
    let before = std::fs::read(&legacy).unwrap();

    // Make the target write fail: the atomic writer's temp path is a
    // directory, so its `write` cannot land.
    std::fs::create_dir_all(tmp_path_for(&pack, Slot::Manual(1))).unwrap();

    assert!(import_legacy(&pack, Slot::Manual(1)).is_err());
    assert_eq!(std::fs::read(&legacy).unwrap(), before, "byte-identical");
    assert!(
        legacy_save(&pack).is_some(),
        "still importable — the card stays"
    );
}

#[test]
fn legacy_import_refuses_the_autosave_target() {
    let (_dir, pack) = temp_pack();
    let legacy = legacy_save_path_for(&pack);
    std::fs::write(&legacy, v1_ron(&parked_save())).unwrap();
    let before = std::fs::read(&legacy).unwrap();

    assert!(import_legacy(&pack, Slot::Autosave).is_err());
    assert_eq!(
        status(&pack, Slot::Autosave),
        SlotStatus::Empty,
        "the autosave is load-only"
    );
    assert_eq!(std::fs::read(&legacy).unwrap(), before, "still importable");
}
