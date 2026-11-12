//! Slot paths, listing and the version/semantic classification that gates a
//! load.

use super::*;

#[test]
fn slot_all_is_the_canonical_order_with_distinct_paths() {
    assert_eq!(
        Slot::ALL,
        [
            Slot::Manual(1),
            Slot::Manual(2),
            Slot::Manual(3),
            Slot::Autosave,
        ]
    );

    let (_dir, pack) = temp_pack();
    let paths: Vec<_> = Slot::ALL.iter().map(|s| save_path_for(&pack, *s)).collect();
    assert_eq!(
        paths
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
            .collect::<Vec<_>>(),
        [
            "demo-pack.slot1.save.ron",
            "demo-pack.slot2.save.ron",
            "demo-pack.slot3.save.ron",
            "demo-pack.autosave.save.ron",
        ]
    );
    for (i, path) in paths.iter().enumerate() {
        assert_eq!(path.parent(), pack.root.parent());
        for other in &paths[i + 1..] {
            assert_ne!(path, other);
        }
    }
}

#[test]
fn save_free_pack_lists_four_empty_slots() {
    let (_dir, pack) = temp_pack();
    for info in list_slots(&pack) {
        assert_eq!(info.status, SlotStatus::Empty, "{:?}", info.slot);
    }
}

#[test]
fn round_trip_writes_one_slot_and_leaves_the_others_empty() {
    let (_dir, pack) = temp_pack();
    let mut save = parked_save();
    save.saved_at = Some(1_700_000_000);
    write_slot(&pack, Slot::Manual(2), &save).expect("writes");

    assert_eq!(
        status(&pack, Slot::Manual(2)),
        SlotStatus::Ok {
            scene_id: "first_meeting".to_string(),
            saved_at: Some(1_700_000_000),
        }
    );
    for slot in [Slot::Manual(1), Slot::Manual(3), Slot::Autosave] {
        assert_eq!(status(&pack, slot), SlotStatus::Empty, "{slot:?}");
    }

    let loaded = read_slot(&pack, Slot::Manual(2)).expect("reads");
    assert_eq!(loaded, save);
    // A slot listed `Ok` is one that restores.
    let mut fresh = ScriptRunner::new(demo_pack());
    loaded.restore(&mut fresh).expect("Ok slots restore");
    assert_eq!(fresh.scene_id(), "first_meeting");
}

#[test]
fn a_v1_file_loads_under_the_floor_with_no_stamp() {
    let (_dir, pack) = temp_pack();
    let save = parked_save();
    std::fs::write(save_path_for(&pack, Slot::Manual(1)), v1_ron(&save)).unwrap();

    assert_eq!(
        status(&pack, Slot::Manual(1)),
        SlotStatus::Ok {
            scene_id: "first_meeting".to_string(),
            saved_at: None,
        }
    );
    let loaded = read_slot(&pack, Slot::Manual(1)).expect("v1 loads");
    assert_eq!(loaded.version, 1);
    assert_eq!(loaded.saved_at, None);

    let mut fresh = ScriptRunner::new(demo_pack());
    loaded.restore(&mut fresh).expect("v1 restores");
    assert_eq!(fresh.scene_id(), save.scene_id);
    assert_eq!(fresh.pc(), save.pc);
}

#[test]
fn newer_version_truncated_and_garbage_bytes_are_unreadable() {
    let (_dir, pack) = temp_pack();

    let mut newer = parked_save();
    newer.version = SAVE_VERSION + 1;
    write_slot(&pack, Slot::Manual(1), &newer).expect("writes");
    let mut zero = parked_save();
    zero.version = 0;
    write_slot(&pack, Slot::Manual(2), &zero).expect("writes");
    std::fs::write(save_path_for(&pack, Slot::Manual(3)), b"").unwrap();
    std::fs::write(save_path_for(&pack, Slot::Autosave), b"not ron at all").unwrap();

    for slot in Slot::ALL {
        let SlotStatus::Unreadable { reason } = status(&pack, slot) else {
            panic!("{slot:?} should be unreadable");
        };
        assert!(!reason.is_empty(), "{slot:?} carries a reason");
        assert!(read_slot(&pack, slot).is_err(), "{slot:?} refuses to read");
    }
}

#[test]
fn a_slot_naming_a_scene_the_pack_lacks_is_unreadable() {
    let (_dir, pack) = temp_pack();
    let mut save = parked_save();
    save.scene_id = "nowhere".to_string();
    write_slot(&pack, Slot::Manual(1), &save).expect("writes");

    let SlotStatus::Unreadable { reason } = status(&pack, Slot::Manual(1)) else {
        panic!("an unknown scene should be unreadable");
    };
    assert!(
        reason.contains("nowhere"),
        "reason names the scene: {reason}"
    );
    assert!(read_slot(&pack, Slot::Manual(1)).is_err());
}

#[test]
fn semantic_failures_classify_unreadable_and_stay_overwritable() {
    let (_dir, pack) = temp_pack();
    let lines = pack.scenes.get("first_meeting").unwrap().lines.len();

    // `pc` past the scene's last line.
    let mut past_end = parked_save();
    past_end.pc = lines + 1;
    write_slot(&pack, Slot::Manual(1), &past_end).expect("writes");

    // `pc == 0` with a wait still parked.
    let mut zero_pc = parked_save();
    zero_pc.pc = 0;
    write_slot(&pack, Slot::Manual(2), &zero_pc).expect("writes");

    // A state key `state.ron` does not declare.
    let mut undeclared = parked_save();
    undeclared
        .state
        .insert("surprise".to_string(), StateValue::Bool(true));
    write_slot(&pack, Slot::Manual(3), &undeclared).expect("writes");

    // The parked wait's kind does not match the line at `pc - 1`.
    let mut mismatched = parked_save();
    mismatched.wait = Some(Wait::Choice {
        options: Vec::new(),
    });
    write_slot(&pack, Slot::Autosave, &mismatched).expect("writes");

    for slot in Slot::ALL {
        assert!(
            matches!(status(&pack, slot), SlotStatus::Unreadable { .. }),
            "{slot:?} should fail a semantic check"
        );
        assert!(save_path_for(&pack, slot).exists(), "{slot:?} is preserved");
        assert!(read_slot(&pack, slot).is_err(), "{slot:?} refuses to read");
    }

    // Even a semantic failure is only ever overwritten by a new save.
    let fresh = parked_save();
    for slot in Slot::ALL {
        write_slot(&pack, slot, &fresh).expect("overwrites");
        assert!(matches!(status(&pack, slot), SlotStatus::Ok { .. }));
    }
}
