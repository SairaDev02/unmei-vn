use super::*;
use crate::credentials::{self, ClipboardPort, CopyPathLabel, CredentialsState, KeyField, KeyRow};
use crate::failure::{captures, LogOpenError, Notice, StartupFailure};
use crate::input::{InputClaim, InputOwnership};
use crate::menu::{
    self, ConfirmKind, LoadSurface, MenuAction, MenuFocus, MenuMode, MenuState, MenuView,
    SlotAction,
};
use crate::presentation::assets::AssetWatch;
use crate::presentation::play::{
    report_silent_degradations, sync_audio, sync_staging_visuals, AppliedAudio, AppliedUi,
    AppliedVisuals, Bgm, DialogueText, HelpLine, Sfx, TextReveal,
};
use crate::presentation::settings_view::{SettingsLabel, SettingsLabelKind};
use crate::session::{LoadOrigin, LoadOutcome, ReadOutcome, SessionInput};
use crate::settings::{self, AudioControl, SettingsState};
use bevy::app::AppExit;
use bevy::asset::{
    io::Reader, AssetApp, AssetLoader, AssetPlugin, LoadContext, LoadState, RenderAssetUsages,
};
use bevy::ecs::system::RunSystemOnce;
use bevy::ecs::world::FromWorld;
use bevy::reflect::TypePath;
use bevy::render::render_resource::Extent3d;
use bevy::text::{EditableText, LineBreak, LineHeight, TextLayout};
use bevy::ui::widget::NodeImageMode;
use bevy::ui_widgets::ScrollArea;
use bevy::window::{MonitorSelection, PrimaryWindow, Window, WindowMode};
use bevy::MinimalPlugins;
use bevy_kira_audio::AudioChannel;
use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};
use unmei_engine::decision::{DecisionError, DecisionResolver, ResolvedDecision};
use unmei_engine::runner::Wait;
use unmei_engine::save::{self, SaveFile, Slot, SlotStatus, StagingState};
use unmei_engine::{load_pack, DecisionConfig, KeyCheck, KeyVerdict, ScriptRunner, StateValue};

use crate::drawer::{
    self, MonitorControl, MonitorRow, MonitorRowPart, MonitorStatusText, MonitorView, MonitorWork,
    DISPLAY_ROWS, MONITOR_CLAIM, NEWEST_INSET, NEWEST_WASH, PROJECTION_SCAN_BUDGET,
    TEXT_WORK_BUDGET,
};
use crate::input::InputQuarantine;
use crate::monitor::{MonitorHistory, MonitorLine};

use support::*;
pub(crate) use support::{
    activate_control, app_for_pack, boot_widget_input_app, drawer_open, drawer_view,
    enter_play_with_input, press_and_update, press_key, step_input, tap_key, temp_pack,
};

mod asset_root;
mod comfort;
mod decisions;
mod failure_posture;
mod gallery;
mod input_ownership;
mod key_row;
mod monitor_drawer;
mod monitor_reading;
mod support;

/// [`boot_app`], but with the `Startup` schedule pinned to one thread so a
/// `Startup`-time failure log lands on the test thread's capture buffer.
fn boot_app_capturing(pack_root: PathBuf) -> App {
    let mut app = boot_app_unstarted(pack_root, None);
    app.edit_schedule(Startup, |schedule| {
        schedule.set_executor(bevy::ecs::schedule::SingleThreadedExecutor::new());
    });
    app.update();
    app
}

#[test]
fn core_parks_on_the_entry_scene_say() {
    let app = headless_app();
    let runtime = app.world().resource::<Runtime>();
    assert!(matches!(runtime.runner.wait(), Some(Wait::Say { .. })));
    assert_eq!(runtime.runner.scene_id(), "first_meeting");
}

#[test]
fn staging_snapshot_tracks_the_branch_taken() {
    let mut app = headless_app();
    let mut runtime = app.world_mut().resource_mut::<Runtime>();
    drive_runner_to_decision(&mut runtime);

    let staging = &runtime.staging;
    assert_eq!(staging.background.as_deref(), Some("school_gate"));
    assert_eq!(
        staging.sprites.get("ace").map(String::as_str),
        Some("neutral")
    );
    // Both scenes `play music_*`; the stop+play pair leaves the reunion track.
    assert_eq!(staging.bgm.as_deref(), Some("music_evening"));
}

/// Walk the whole demo script with the decision resolved to `action`,
/// returning every asset path the run staged.
fn staged_asset_paths(action: &str) -> Vec<PathBuf> {
    use unmei_engine::runner::StageCommand;

    let mut app = headless_app();
    let mut runtime = app.world_mut().resource_mut::<Runtime>();
    let asset_root = runtime.asset_root.clone();
    let mut paths = Vec::new();

    // `headless_app`'s first update already consumed the entry scene's
    // `bg`/`play`, so seed from what it left staged before walking on.
    if let Some(id) = &runtime.staging.background {
        paths.push(asset_root.join(format!("backgrounds/{id}.png")));
    }
    for (npc_id, pose) in &runtime.staging.sprites {
        paths.push(asset_root.join(crate::presentation::assets::sprite_asset_path(npc_id, pose)));
    }
    if let Some(id) = &runtime.staging.bgm {
        paths.push(asset_root.join(format!("audio/{id}.ogg")));
    }

    loop {
        let outcome = runtime.runner.run().unwrap();
        runtime.staging.apply_all(&outcome.staged);
        for command in &outcome.staged {
            let relative = match command {
                StageCommand::Bg(id) => Some(format!("backgrounds/{id}.png")),
                StageCommand::Sprite { npc_id, pose } => {
                    Some(crate::presentation::assets::sprite_asset_path(npc_id, pose))
                }
                StageCommand::Play(id) => Some(format!("audio/{id}.ogg")),
                StageCommand::Hide(_) | StageCommand::Stop(_) => None,
            };
            if let Some(relative) = relative {
                paths.push(asset_root.join(relative));
            }
        }
        match outcome.wait {
            Some(Wait::Say { .. }) => runtime.runner.advance_say().unwrap(),
            Some(Wait::Choice { .. }) => runtime.runner.choose(0).unwrap(),
            Some(Wait::Decision { .. }) => runtime.runner.resolve_decision(action).unwrap(),
            None => break,
        }
    }
    paths
}

/// Positive asset check: a boot smoke test only shows the *absence* of
/// loader errors, so assert here that every path the runtime stages for the
/// demo actually resolves to a file on disk — including both decision
/// branches and the hardwired choice SFX the script never stages.
#[test]
fn every_asset_the_demo_script_stages_exists() {
    let mut paths = staged_asset_paths("cold");
    paths.extend(staged_asset_paths("warm"));
    paths.push(demo_pack_root().join("assets/audio/sfx_select.ogg"));
    paths.sort();
    paths.dedup();

    for path in &paths {
        assert!(
            path.exists(),
            "demo pack is missing a staged asset: {}",
            path.display()
        );
    }

    let names: Vec<String> = paths
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    for expected in [
        "school_gate.png",
        "neutral.png",
        "happy.png",
        "music_morning.ogg",
        "music_evening.ogg",
        "sfx_paper_rustle.ogg",
        "sfx_select.ogg",
    ] {
        assert!(
            names.iter().any(|name| name == expected),
            "the demo walk never staged {expected} (got {names:?})"
        );
    }
}

/// The temporary sibling `write_save` uses for one slot path.
fn tmp_path(slot: &Path) -> PathBuf {
    slot.with_extension("ron.tmp")
}

/// [`app_for_pack`] with an explicit dev-mode gate, so the disabled-shortcut
/// regressions start from real play without a shortcut-enabled fixture.
fn app_for_pack_dev(pack_root: PathBuf, dev_mode: bool) -> App {
    let mut app = boot_app_unstarted_dev(pack_root, None, dev_mode);
    app.update();
    enter_play(&mut app);
    app
}

#[test]
fn save_then_load_restores_position_through_the_app() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    // Mid-scene: advance once (entry say -> Ace say).
    press_and_update(&mut app, KeyCode::Space);
    let saved_wait = app.world().resource::<Runtime>().runner.wait().cloned();
    assert!(matches!(saved_wait, Some(Wait::Say { .. })));

    press_and_update(&mut app, KeyCode::F5);
    assert!(manual_path(&app).exists(), "F5 wrote the first manual slot");

    // Move on, then load: the same line comes back.
    press_and_update(&mut app, KeyCode::Space);
    let moved_on = app.world().resource::<Runtime>().runner.wait().cloned();
    assert_ne!(moved_on, saved_wait);
    press_and_update(&mut app, KeyCode::F9);

    let restored = app.world().resource::<Runtime>().runner.wait().cloned();
    assert_eq!(restored, saved_wait);
}

#[test]
fn save_survives_an_app_restart() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");

    // First run: advance once, then save mid-scene, then shut the app down.
    let (saved_wait, saved_staging) = {
        let mut app = app_for_pack(pack_root.clone());
        press_and_update(&mut app, KeyCode::Space);
        let runtime = app.world().resource::<Runtime>();
        let wait = runtime.runner.wait().cloned();
        let staging = runtime.staging.clone();
        press_and_update(&mut app, KeyCode::F5);
        (wait, staging)
    };

    // Second run: a fresh app starts at the entry scene, then F9 resumes.
    let mut restarted = app_for_pack(pack_root);
    assert_eq!(
        restarted.world().resource::<Runtime>().runner.scene_id(),
        "first_meeting"
    );
    press_and_update(&mut restarted, KeyCode::F9);

    let runtime = restarted.world().resource::<Runtime>();
    assert_eq!(runtime.runner.wait().cloned(), saved_wait);
    assert_eq!(runtime.staging, saved_staging);
    assert_eq!(runtime.staging.background.as_deref(), Some("school_gate"));
    assert_eq!(runtime.staging.bgm.as_deref(), Some("music_morning"));
}

#[test]
fn load_without_a_save_file_is_a_clean_no_op() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    let before = app.world().resource::<Runtime>().runner.wait().cloned();
    assert!(!manual_path(&app).exists());
    press_and_update(&mut app, KeyCode::F9);
    let after = app.world().resource::<Runtime>().runner.wait().cloned();
    assert_eq!(before, after);
}

// ---------------------------------------------------------------------------
// Shared dev mode and the F5/F9 gate
// ---------------------------------------------------------------------------

#[test]
fn runtime_config_defaults_dev_mode_to_debug_assertions_with_an_override() {
    let config = RuntimeConfig::new(PathBuf::from("a-pack"), None);
    // The default is the compiler's debug-assertions setting, never the
    // optimization level or the presence of debug symbols.
    assert_eq!(config.dev_mode, cfg!(debug_assertions));
    // The explicit construction-time override wins in either direction.
    assert!(
        RuntimeConfig::new(PathBuf::from("a-pack"), None)
            .with_dev_mode(true)
            .dev_mode
    );
    assert!(!config.with_dev_mode(false).dev_mode);
}

#[test]
fn dev_mode_off_makes_f5_and_f9_silent_no_ops() {
    captures::install();
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");

    // Seed a valid manual slot from a shortcut-enabled run, so the dev-off F9
    // has something it would otherwise restore.
    {
        let mut seeded = app_for_pack(pack_root.clone());
        press_and_update(&mut seeded, KeyCode::Space);
        assert!(matches!(
            seeded.world().resource::<Runtime>().runner.wait(),
            Some(Wait::Say { .. })
        ));
        press_and_update(&mut seeded, KeyCode::F5);
        assert!(manual_path(&seeded).exists(), "the seed saved");
    }

    // The dev-off app starts real play and moves past the seeded park.
    let mut app = app_for_pack_dev(pack_root, false);
    force_single_threaded(&mut app);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    let parked = app.world().resource::<Runtime>().runner.wait().cloned();
    assert!(matches!(parked, Some(Wait::Choice { .. })));
    let slot = manual_path(&app);
    let bytes_before = std::fs::read(&slot).expect("the seeded slot exists");

    captures::take();
    app.world_mut().resource_mut::<Notice>().text = None;

    // F5 is a no-op: the slot keeps its bytes and no shortcut log is emitted.
    press_and_update(&mut app, KeyCode::F5);
    assert_eq!(
        std::fs::read(&slot).unwrap(),
        bytes_before,
        "dev-off F5 left the slot byte-identical"
    );
    assert!(
        captures::take().is_empty(),
        "dev-off F5 logged nothing at all"
    );

    // F9 is a no-op: the dev-off app stays on its own park rather than
    // restoring the seeded save.
    press_and_update(&mut app, KeyCode::F9);
    assert_eq!(
        app.world().resource::<Runtime>().runner.wait().cloned(),
        parked,
        "dev-off F9 did not restore the slot"
    );
    assert!(captures::take().is_empty(), "dev-off F9 logged nothing");
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "dev-off F9 raised no Notice"
    );

    // Player persistence is not gated: resolving the choice still autosaves.
    press_and_update(&mut app, KeyCode::Digit1);
    assert!(
        autosave_path(&app).exists(),
        "dev mode off leaves autosave active"
    );
}

// ---------------------------------------------------------------------------
// Slot write points
// ---------------------------------------------------------------------------

#[test]
fn a_boot_run_writes_no_slot() {
    let pack = temp_pack();
    let app = app_for_pack(pack.path().join("pack"));
    let runtime = app.world().resource::<Runtime>();
    for info in unmei_engine::list_slots(runtime.runner.pack()) {
        assert_eq!(
            info.status,
            SlotStatus::Empty,
            "a launch writes no slot ({:?})",
            info.slot
        );
    }
}

#[test]
fn a_choice_resolve_writes_the_autosave_once() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    assert!(!autosave_path(&app).exists());

    // Reach the choice: entry say -> Ace say -> choice.
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    assert!(matches!(
        app.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Choice { .. })
    ));
    press_and_update(&mut app, KeyCode::Digit1);

    // The autosave names the park the choice resolve reached, with the app
    // clock stamp and the staged snapshot the run applied.
    let runtime = app.world().resource::<Runtime>();
    let save = save::read_slot(runtime.runner.pack(), Slot::Autosave).expect("autosaved");
    assert!(save.saved_at.is_some(), "the app stamps saved_at");
    assert_eq!(save.wait, runtime.runner.wait().cloned());
    assert_eq!(save.staging, runtime.staging);

    // No further write without another event.
    let before = std::fs::read(autosave_path(&app)).unwrap();
    app.update();
    assert_eq!(std::fs::read(autosave_path(&app)).unwrap(), before);
    assert!(
        !app.world().resource::<crate::slots::PendingAutosave>().0,
        "the write disarmed the trigger"
    );
}

#[test]
fn a_scene_crossing_writes_the_autosave() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    press_and_update(&mut app, KeyCode::Space); // Ace say
    press_and_update(&mut app, KeyCode::Space); // choice
    press_and_update(&mut app, KeyCode::Digit1); // ask_letter say
                                                 // The next advance runs `jump reunion`, a scene crossing.
    press_and_update(&mut app, KeyCode::Space);

    let runtime = app.world().resource::<Runtime>();
    assert_eq!(runtime.runner.scene_id(), "reunion");
    let save = save::read_slot(runtime.runner.pack(), Slot::Autosave).expect("autosaved");
    assert_eq!(save.scene_id, "reunion");
    assert_eq!(save.wait, runtime.runner.wait().cloned());
    assert!(save.saved_at.is_some());

    // No further write without another event.
    let before = std::fs::read(autosave_path(&app)).unwrap();
    app.update();
    assert_eq!(std::fs::read(autosave_path(&app)).unwrap(), before);
    assert!(
        !app.world().resource::<crate::slots::PendingAutosave>().0,
        "the write disarmed the trigger"
    );
}

#[test]
fn a_load_writes_no_autosave() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    press_and_update(&mut app, KeyCode::Space); // Ace say
    press_and_update(&mut app, KeyCode::F5);
    assert!(manual_path(&app).exists());

    // Move on (say -> choice), then load back: the load itself writes nothing.
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::F9);

    assert!(
        !autosave_path(&app).exists(),
        "loading never writes the autosave"
    );
    assert!(matches!(
        app.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Say { .. })
    ));
}

#[test]
fn manual_save_failure_leaves_the_slot_byte_identical_and_notices_row_14() {
    captures::install();
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    force_single_threaded(&mut app);

    // A first save creates the slot, so "unchanged" has bytes to compare.
    press_and_update(&mut app, KeyCode::F5);
    let slot = manual_path(&app);
    let before = std::fs::read(&slot).unwrap();

    // Block the atomic write: the temp sibling is a directory, so `write`
    // fails before the slot could be touched.
    std::fs::create_dir(tmp_path(&slot)).expect("the temp path is free");

    captures::take();
    app.world_mut().resource_mut::<Notice>().text = None;
    press_and_update(&mut app, KeyCode::F5);

    assert_eq!(
        std::fs::read(&slot).unwrap(),
        before,
        "a failed save leaves the slot byte-identical"
    );
    let records = failure_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, log::Level::Error);
    assert!(
        records[0]
            .2
            .starts_with("couldn't save — the slot is unchanged"),
        "{:?}",
        records[0].2
    );
    assert_eq!(
        app.world().resource::<Notice>().text.as_deref(),
        Some("couldn't save — the slot is unchanged")
    );
}

#[test]
fn a_failed_autosave_notices_per_write_and_stays_silent_on_success() {
    captures::install();
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    force_single_threaded(&mut app);
    let tmp = tmp_path(&autosave_path(&app));
    std::fs::create_dir_all(&tmp).expect("the temp path is free");

    // First write point: a choice resolve.
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    captures::take();
    app.world_mut().resource_mut::<Notice>().text = None;
    press_and_update(&mut app, KeyCode::Digit1);
    let first = failure_lines();
    assert_eq!(first.len(), 1, "{first:?}");
    assert_eq!(first[0].0, log::Level::Error);
    assert!(
        first[0].2.starts_with("couldn't autosave"),
        "{:?}",
        first[0].2
    );

    // Second write point: advancing across the scene boundary.
    captures::take();
    app.world_mut().resource_mut::<Notice>().text = None;
    press_and_update(&mut app, KeyCode::Space);
    let second = failure_lines();
    assert_eq!(second.len(), 1, "one line per failed write: {second:?}");
    assert!(
        second[0].2.starts_with("couldn't autosave"),
        "{:?}",
        second[0].2
    );
    assert!(app.world().resource::<Notice>().is_showing());

    // Clear the block: the next crossing's write lands and stays silent.
    std::fs::remove_dir(&tmp).unwrap();
    press_and_update(&mut app, KeyCode::Space); // decision fallback, parks
    captures::take();
    press_and_update(&mut app, KeyCode::Space); // epilogue crossing
    assert!(autosave_path(&app).exists(), "the successful write landed");
    let records = failure_lines();
    assert!(
        !records
            .iter()
            .any(|(_, _, line)| line.starts_with("couldn't autosave")),
        "a successful autosave stays silent: {records:?}"
    );
}

#[test]
fn an_unreadable_slot_refuses_to_load_with_its_reason() {
    captures::install();
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    force_single_threaded(&mut app);
    let slot = manual_path(&app);
    let garbage = b"definitely not a save";
    std::fs::write(&slot, garbage).unwrap();

    captures::take();
    app.world_mut().resource_mut::<Notice>().text = None;
    press_and_update(&mut app, KeyCode::F9);

    let records = failure_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, log::Level::Error);
    assert!(
        records[0].2.starts_with("invalid save file"),
        "row 16 logs the slot's own reason: {:?}",
        records[0].2
    );
    assert!(
        records[0].2.contains(&slot.display().to_string()),
        "the reason is followed by the slot path: {:?}",
        records[0].2
    );
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "row 16 has no Notice — the slot card is its surface"
    );
    // The file is preserved and still overwritable.
    assert_eq!(std::fs::read(&slot).unwrap(), garbage);
    press_and_update(&mut app, KeyCode::F5);
    assert!(save::read_slot(
        app.world().resource::<Runtime>().runner.pack(),
        Slot::Manual(1)
    )
    .is_ok());
}

#[test]
fn a_load_refused_at_restore_time_notices_row_17() {
    captures::install();
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    // A save that names a scene the loaded pack lacks: `read_slot` would refuse
    // it while classifying, so this drives the restore-time site directly, the
    // "slot changed under the overlay" race row 17 names.
    let mut save = {
        let runtime = app.world().resource::<Runtime>();
        SaveFile::capture(&runtime.runner, &runtime.staging)
    };
    save.scene_id = "nowhere".to_string();
    let path = PathBuf::from("nowhere.slot1.save.ron");
    // A `Mutex<Option<_>>` keeps the system closure `FnMut` (a Bevy system) while
    // still handing `save` to the one call.
    let save = std::sync::Mutex::new(Some(save));

    captures::take();
    app.world_mut().resource_mut::<Notice>().text = None;
    let outcome = app
        .world_mut()
        .run_system_once(move |mut session: SessionInput| {
            let save = save
                .lock()
                .expect("the test closure lock is never poisoned")
                .take()
                .expect("the system runs once");
            session.transitions().restore_loaded(save, &path)
        })
        .expect("the one-shot restore system runs");
    assert_eq!(
        outcome,
        LoadOutcome::Refused,
        "the restore refusal reports its own outcome"
    );

    let records = failure_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, log::Level::Error);
    assert!(
        records[0].2.starts_with("couldn't load that save"),
        "{:?}",
        records[0].2
    );
    assert_eq!(
        app.world().resource::<Notice>().text.as_deref(),
        Some("couldn't load that save")
    );
}

/// Drive one typed load through the production session bridge, exactly as the
/// F9 shortcut and the overlay's load path do.
fn run_session_load(app: &mut App, slot: Slot, origin: LoadOrigin) -> LoadOutcome {
    app.world_mut()
        .run_system_once(move |mut session: SessionInput| session.transitions().load(slot, origin))
        .expect("the one-shot load system runs")
}

/// The typed origin alone selects the read-stage policy, and the outcome
/// vocabulary is what the menu's success-only closure branches on. Real engine
/// IO in the fixture's temporary pack directory: an absent dev slot, an
/// unreadable slot read by the shortcut, and the same unreadable slot read by a
/// menu origin.
#[test]
fn typed_load_origins_route_the_read_stages_to_their_own_outcomes_and_rows() {
    captures::install();
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    let slot = Slot::Manual(1);
    let path = manual_path(&app);

    // F9 with no file: the clean no-op — no log, no Notice.
    captures::take();
    assert_eq!(
        run_session_load(&mut app, slot, LoadOrigin::DevShortcut),
        LoadOutcome::Missing
    );
    assert!(
        failure_lines().is_empty(),
        "an absent dev slot logs nothing"
    );
    assert!(!app.world().resource::<Notice>().is_showing());

    // Present but unreadable: the dev origin routes row 16 with the engine's
    // reason and the slot path, Notice-less.
    std::fs::write(&path, b"not a save").unwrap();
    captures::take();
    assert_eq!(
        run_session_load(&mut app, slot, LoadOrigin::DevShortcut),
        LoadOutcome::Refused
    );
    let records = failure_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, log::Level::Error);
    assert!(
        records[0].2.starts_with("invalid save file"),
        "row 16 logs the slot's own reason: {:?}",
        records[0].2
    );
    assert!(
        records[0].2.contains(&path.display().to_string()),
        "the reason is followed by the slot path: {:?}",
        records[0].2
    );
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "row 16 has no Notice — the slot card is its surface"
    );

    // The same unreadable slot through a menu origin routes the existing row 17
    // with its Notice.
    app.world_mut().resource_mut::<Notice>().text = None;
    captures::take();
    assert_eq!(
        run_session_load(&mut app, slot, LoadOrigin::Menu),
        LoadOutcome::Refused
    );
    let records = failure_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, log::Level::Error);
    assert!(
        records[0].2.starts_with("couldn't load that save"),
        "{:?}",
        records[0].2
    );
    assert_eq!(
        app.world().resource::<Notice>().text.as_deref(),
        Some("couldn't load that save")
    );
}

/// The play path's sprite rule has one producer — `sprites/<npc_id>/<pose>.png`
/// — so the gallery cannot fork a second inline copy
/// (`docs/title-and-menu.md` § Gallery § Entries §4).
#[test]
fn sprite_asset_path_builds_the_play_rule() {
    assert_eq!(
        crate::presentation::assets::sprite_asset_path("ace", "happy"),
        "sprites/ace/happy.png"
    );
}

// ---------------------------------------------------------------------------
// Esc menu overlay
// ---------------------------------------------------------------------------

/// Open, then close with no action: the position is untouched either way.
fn assert_open_close_leaves_position(app: &mut App) {
    let before = parked_position(app);
    esc(app);
    assert!(menu(app).open, "Esc opened the overlay");
    assert_eq!(parked_position(app), before, "opening changes nothing");
    esc(app);
    assert!(!menu(app).open, "Esc closed the overlay");
    assert_eq!(parked_position(app), before, "closing changes nothing");
}

#[test]
fn esc_opens_and_closes_from_every_parked_state_unchanged() {
    // say-wait
    let pack = temp_pack();
    let mut say = app_for_pack(pack.path().join("pack"));
    assert!(matches!(
        say.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Say { .. })
    ));
    assert_open_close_leaves_position(&mut say);
    assert!(
        !autosave_path(&say).exists(),
        "opening then closing writes no autosave"
    );

    // choice-wait
    let pack = temp_pack();
    let mut choice = app_for_pack(pack.path().join("pack"));
    press_and_update(&mut choice, KeyCode::Space);
    press_and_update(&mut choice, KeyCode::Space);
    assert!(matches!(
        choice.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Choice { .. })
    ));
    assert_open_close_leaves_position(&mut choice);

    // end of scene (no parked wait)
    let pack = temp_pack();
    let mut ended = app_for_pack(pack.path().join("pack"));
    play_to_end(&mut ended);
    assert!(ended.world().resource::<Runtime>().runner.wait().is_none());
    assert_open_close_leaves_position(&mut ended);

    // decision-wait with a request in flight: the request survives too.
    let fake = FakeResolver::new(model_answer("warm"));
    let (mut inflight, _dir) = app_parked_on_decision(&fake);
    assert!(inflight.world().resource::<PendingDecision>().0.is_some());
    assert_open_close_leaves_position(&mut inflight);
    assert_eq!(fake.calls(), 1, "the overlay never re-fires or cancels");
    assert!(
        inflight.world().resource::<PendingDecision>().0.is_some(),
        "the in-flight request is untouched by open/close"
    );
}

#[test]
fn menu_cards_render_exactly_what_list_slots_yields() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    // A stamped manual save (slot 1) at the second say.
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::F5);

    // A v1-style save with no `saved_at` (slot 2): the card shows `—`, never a
    // fabricated time.
    {
        let runtime = app.world().resource::<Runtime>();
        let mut save = SaveFile::capture(&runtime.runner, &runtime.staging);
        save.saved_at = None;
        save::write_slot(runtime.runner.pack(), Slot::Manual(2), &save).unwrap();
    }

    esc(&mut app);

    let runtime = app.world().resource::<Runtime>();
    let infos = save::list_slots(runtime.runner.pack());
    let state = menu(&app);
    assert_eq!(
        state.slots,
        infos.to_vec(),
        "the cards cache list_slots verbatim"
    );

    let models = menu::card_models(state);
    assert_eq!(models.len(), 4, "four cards in Slot::ALL order");

    assert_eq!(models[0].label, "Slot 1");
    assert_eq!(models[0].kind, "manual");
    assert_eq!(models[0].status, "Ok");
    assert_eq!(models[0].detail, "first_meeting");
    assert_ne!(models[0].saved_at, "—", "the app stamp is formatted");
    assert_eq!(models[0].action.label, "Overwrite");
    assert_eq!(models[0].action.action, Some(SlotAction::Write));
    assert!(models[0].action.danger);

    assert_eq!(models[1].status, "Ok");
    assert_eq!(models[1].saved_at, "—", "an unstamped v1 save renders `—`");
    assert_eq!(models[1].action.label, "Overwrite");

    assert_eq!(models[2].status, "Empty");
    assert_eq!(models[2].detail, "—");
    assert_eq!(models[2].saved_at, "no save yet");
    assert_eq!(models[2].action.label, "Save here");
    assert_eq!(models[2].action.action, Some(SlotAction::Write));

    assert_eq!(models[3].label, "Autosave");
    assert_eq!(models[3].kind, "game-written");
    assert_eq!(models[3].action.label, "Load-only");
    assert!(!models[3].action.enabled);
    assert_eq!(models[3].action.action, None);
}

#[test]
fn autosave_offers_no_save_action_and_unreadable_keeps_overwrite() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    // Resolve the choice: the autosave is written.
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Digit1);
    // An unreadable manual slot beside it.
    std::fs::write(manual_path(&app), b"garbage bytes").unwrap();

    esc(&mut app);
    let models = menu::card_models(menu(&app));
    assert_eq!(models[3].status, "Ok");
    assert_eq!(models[3].action.label, "Load-only");
    assert!(!models[3].action.enabled, "the autosave offers no save");
    assert_eq!(models[3].action.action, None);

    assert_eq!(models[0].status, "Unreadable");
    assert!(
        models[0].detail.contains("invalid"),
        "{:?}",
        models[0].detail
    );
    assert_eq!(models[0].action.label, "Overwrite");
    assert_eq!(models[0].action.action, Some(SlotAction::Write));

    switch_mode(&mut app, MenuMode::Load);
    let models = menu::card_models(menu(&app));
    assert_eq!(models[3].action.label, "Load");
    assert_eq!(models[3].action.action, Some(SlotAction::Load));
    assert_eq!(models[0].action.label, "Overwrite", "still overwrite-only");
    assert_eq!(models[0].action.action, Some(SlotAction::Write));
    // An empty manual slot still offers `Save here`: the issue's table
    // qualifies only the occupied and autosave rows by mode.
    assert_eq!(models[1].status, "Empty");
    assert_eq!(models[1].action.label, "Save here");
    assert_eq!(models[1].action.action, Some(SlotAction::Write));
}

#[test]
fn save_over_empty_is_immediate_and_overwrite_confirms_once() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    // Save-over-empty: no second prompt.
    esc(&mut app);
    assert_eq!(menu(&app).focus, MenuFocus::Card(0));
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, None, "save-over-empty asks nothing");
    assert!(manual_path(&app).exists(), "the empty slot was written");
    assert!(matches!(menu(&app).slots[0].status, SlotStatus::Ok { .. }));

    // Move on, then reopen: the occupied card confirms once.
    esc(&mut app);
    press_and_update(&mut app, KeyCode::Space);
    let slot = manual_path(&app);
    let before = std::fs::read(&slot).unwrap();
    esc(&mut app);

    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(
        menu(&app).confirm,
        Some(ConfirmKind::Overwrite(Slot::Manual(1)))
    );
    esc(&mut app);
    assert_eq!(menu(&app).confirm, None, "Esc cancelled the confirm");
    assert!(
        menu(&app).open,
        "Esc cancelled the confirm, not the overlay"
    );
    assert_eq!(
        std::fs::read(&slot).unwrap(),
        before,
        "cancelling leaves the file byte-identical"
    );

    // Accept: the slot is overwritten with the moved position.
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(
        menu(&app).confirm,
        Some(ConfirmKind::Overwrite(Slot::Manual(1)))
    );
    press_and_update(&mut app, KeyCode::Enter);
    let runtime = app.world().resource::<Runtime>();
    let reloaded = save::read_slot(runtime.runner.pack(), Slot::Manual(1)).unwrap();
    assert_eq!(
        reloaded.wait,
        runtime.runner.wait().cloned(),
        "the overwrite captured the current park"
    );
}

#[test]
fn load_confirms_once_and_cancel_leaves_position_and_file_identical() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::F5);
    let saved_wait = app.world().resource::<Runtime>().runner.wait().cloned();
    press_and_update(&mut app, KeyCode::Space);
    let moved_wait = app.world().resource::<Runtime>().runner.wait().cloned();
    assert_ne!(saved_wait, moved_wait);
    let slot = manual_path(&app);
    let before = std::fs::read(&slot).unwrap();

    esc(&mut app);
    switch_mode(&mut app, MenuMode::Load);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::Card(0);
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::Load(Slot::Manual(1))));
    esc(&mut app);
    assert_eq!(menu(&app).confirm, None);
    assert_eq!(menu(&app).mode, MenuMode::Load);
    assert_eq!(
        app.world().resource::<Runtime>().runner.wait().cloned(),
        moved_wait,
        "a cancelled load does not move the runner"
    );
    assert_eq!(std::fs::read(&slot).unwrap(), before);

    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter);
    assert!(!menu(&app).open, "a load closes the overlay");
    assert_eq!(
        app.world().resource::<Runtime>().runner.wait().cloned(),
        saved_wait
    );
}

#[test]
fn a_load_refused_under_the_overlay_notices_row_17() {
    captures::install();
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    force_single_threaded(&mut app);

    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::F5);
    esc(&mut app);
    switch_mode(&mut app, MenuMode::Load);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::Card(0);

    // The slot changes under the overlay, after its card was classified.
    std::fs::write(manual_path(&app), b"definitely not a save").unwrap();
    captures::take();
    app.world_mut().resource_mut::<Notice>().text = None;

    press_and_update(&mut app, KeyCode::Enter); // confirm
    press_and_update(&mut app, KeyCode::Enter); // accept

    let records = failure_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, log::Level::Error);
    assert!(
        records[0].2.starts_with("couldn't load that save"),
        "{:?}",
        records[0].2
    );
    assert!(menu(&app).open, "a refused load leaves the overlay open");
    assert!(app.world().resource::<Notice>().is_showing());
}

#[test]
fn manual_save_succeeds_from_every_parked_state_including_in_flight() {
    // say-wait
    let pack = temp_pack();
    let mut say = app_for_pack(pack.path().join("pack"));
    esc(&mut say);
    press_and_update(&mut say, KeyCode::Enter);
    assert_eq!(menu(&say).confirm, None);
    assert!(manual_path(&say).exists());

    // choice-wait
    let pack = temp_pack();
    let mut choice = app_for_pack(pack.path().join("pack"));
    press_and_update(&mut choice, KeyCode::Space);
    press_and_update(&mut choice, KeyCode::Space);
    esc(&mut choice);
    press_and_update(&mut choice, KeyCode::Enter);
    assert!(manual_path(&choice).exists());

    // end of scene
    let pack = temp_pack();
    let mut ended = app_for_pack(pack.path().join("pack"));
    play_to_end(&mut ended);
    esc(&mut ended);
    press_and_update(&mut ended, KeyCode::Enter);
    assert!(manual_path(&ended).exists());

    // decision-wait with a request in flight: the parked wait is captured and
    // the request is not disturbed.
    let fake = FakeResolver::new(model_answer("warm"));
    let (mut inflight, _dir) = app_parked_on_decision(&fake);
    esc(&mut inflight);
    assert!(matches!(
        inflight.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Decision { .. })
    ));
    press_and_update(&mut inflight, KeyCode::Enter);
    assert!(manual_path(&inflight).exists());
    assert!(
        inflight.world().resource::<PendingDecision>().0.is_some(),
        "a manual save never cancels the request"
    );
    let save = save::read_slot(
        inflight.world().resource::<Runtime>().runner.pack(),
        Slot::Manual(1),
    )
    .unwrap();
    assert!(matches!(save.wait, Some(Wait::Decision { .. })));
}

#[test]
fn keyboard_focus_walks_the_grid_and_cycles_the_top_bar() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    esc(&mut app);

    assert_eq!(menu(&app).focus, MenuFocus::Card(0));
    press_and_update(&mut app, KeyCode::ArrowRight);
    assert_eq!(menu(&app).focus, MenuFocus::Card(1));
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::Card(3));
    press_and_update(&mut app, KeyCode::ArrowUp);
    assert_eq!(menu(&app).focus, MenuFocus::Card(1));
    press_and_update(&mut app, KeyCode::ArrowLeft);
    assert_eq!(menu(&app).focus, MenuFocus::Card(0));

    // Digits focus a manual slot (1–3) without activating it.
    press_and_update(&mut app, KeyCode::Digit2);
    assert_eq!(menu(&app).focus, MenuFocus::Card(1));

    // Tab cycles the top bar: Save → Load → Timeline → Gallery → Settings →
    // Save.
    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).focus, MenuFocus::SaveTab);
    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).focus, MenuFocus::LoadTab);
    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).focus, MenuFocus::TimelineButton);
    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryButton);
    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).focus, MenuFocus::SettingsButton);
    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).focus, MenuFocus::SaveTab);

    // Left from the grid's left column reaches the rail's Resume, which is
    // keyboard-activatable like every other control.
    press_and_update(&mut app, KeyCode::ArrowLeft);
    assert_eq!(menu(&app).focus, MenuFocus::Card(0));
    press_and_update(&mut app, KeyCode::ArrowLeft);
    assert_eq!(menu(&app).focus, MenuFocus::Resume);
    press_and_update(&mut app, KeyCode::Enter);
    assert!(!menu(&app).open, "Resume is keyboard-activatable");
}

#[test]
fn every_control_activates_by_click() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::F5);
    esc(&mut app);

    click(&mut app, MenuAction::SetMode(MenuMode::Load));
    assert_eq!(menu(&app).mode, MenuMode::Load);

    click(&mut app, MenuAction::OpenSettings);
    assert_eq!(menu(&app).view, MenuView::Settings);

    click(&mut app, MenuAction::BackToSlots);
    assert_eq!(menu(&app).view, MenuView::Slots);

    click(&mut app, MenuAction::Card(0));
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::Load(Slot::Manual(1))));

    click(&mut app, MenuAction::ConfirmCancel);
    assert_eq!(menu(&app).confirm, None);

    click(&mut app, MenuAction::Resume);
    assert!(!menu(&app).open);
}

#[test]
fn settings_route_is_reachable_and_esc_backs_out_to_the_grid() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    esc(&mut app);

    // Reach Settings by keyboard: Save → Load → Timeline → Gallery → Settings.
    press_and_update(&mut app, KeyCode::Tab);
    press_and_update(&mut app, KeyCode::Tab);
    press_and_update(&mut app, KeyCode::Tab);
    press_and_update(&mut app, KeyCode::Tab);
    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).focus, MenuFocus::SettingsButton);
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).view, MenuView::Settings);
    assert_eq!(menu(&app).focus, MenuFocus::AudioMaster);

    // Up wraps to the rail's session-scoped `Exit to title`, the walk's one
    // stop past the Decisions group; Up again reaches the group's last control.
    // The back control is a click/`Esc` affordance, not an arrow stop.
    press_and_update(&mut app, KeyCode::ArrowUp);
    assert_eq!(menu(&app).focus, MenuFocus::ExitToTitle);
    press_and_update(&mut app, KeyCode::ArrowUp);
    assert_eq!(menu(&app).focus, MenuFocus::ClearKey);
    click(&mut app, MenuAction::BackToSlots);
    assert_eq!(menu(&app).view, MenuView::Slots);

    // Esc backs out of Settings, then closes the overlay.
    click(&mut app, MenuAction::OpenSettings);
    assert_eq!(menu(&app).view, MenuView::Settings);
    esc(&mut app);
    assert_eq!(menu(&app).view, MenuView::Slots);
    assert!(
        menu(&app).open,
        "Esc backed out to the grid, not the overlay"
    );
    esc(&mut app);
    assert!(!menu(&app).open);
}

#[test]
fn the_focused_control_is_highlighted_in_every_state() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    esc(&mut app);

    // The controls the overlay renders, each starting unhighlighted.
    let spawn = |app: &mut App, action: MenuAction| {
        app.world_mut()
            .spawn((action, BorderColor::all(Color::NONE)))
            .id()
    };
    let resume = spawn(&mut app, MenuAction::Resume);
    let save_tab = spawn(&mut app, MenuAction::SetMode(MenuMode::Save));
    let load_tab = spawn(&mut app, MenuAction::SetMode(MenuMode::Load));
    let gallery_button = spawn(&mut app, MenuAction::OpenGallery);
    let settings = spawn(&mut app, MenuAction::OpenSettings);
    let back = spawn(&mut app, MenuAction::BackToSlots);
    let card0 = spawn(&mut app, MenuAction::Card(0));
    let legacy_card = spawn(&mut app, MenuAction::LegacyImport);
    let legacy_target0 = spawn(&mut app, MenuAction::LegacyTarget(Slot::Manual(1)));
    let legacy_cancel = spawn(&mut app, MenuAction::LegacyCancel);
    let controls = [
        resume,
        save_tab,
        load_tab,
        gallery_button,
        settings,
        back,
        card0,
        legacy_card,
        legacy_target0,
        legacy_cancel,
    ];

    let highlighted = |app: &App, entity: Entity| {
        app.world().get::<BorderColor>(entity).unwrap().top != Color::NONE
    };

    for (focus, expected) in [
        (MenuFocus::Card(0), card0),
        (MenuFocus::Resume, resume),
        (MenuFocus::SaveTab, save_tab),
        (MenuFocus::LoadTab, load_tab),
        (MenuFocus::GalleryButton, gallery_button),
        (MenuFocus::SettingsButton, settings),
        (MenuFocus::SettingsBack, back),
        (MenuFocus::LegacyCard, legacy_card),
        (MenuFocus::LegacyTarget(0), legacy_target0),
        (MenuFocus::LegacyCancel, legacy_cancel),
    ] {
        app.world_mut().resource_mut::<MenuState>().focus = focus;
        app.world_mut()
            .run_system_once(crate::presentation::sync_menu_focus)
            .expect("the focus system runs");
        assert!(highlighted(&app, expected), "{focus:?} is highlighted");
        for entity in controls {
            if entity != expected {
                assert!(!highlighted(&app, entity), "{focus:?} leaves others plain");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Legacy save import card
// ---------------------------------------------------------------------------

fn legacy_path(app: &App) -> PathBuf {
    let runtime = app.world().resource::<Runtime>();
    save::legacy_save_path_for(runtime.runner.pack())
}

/// Write an importable legacy save: the app-captured `SaveFile` with the
/// legacy file's name and no stamp (the shape the slice's F5 wrote).
fn write_legacy(app: &App, saved_at: Option<u64>) {
    let runtime = app.world().resource::<Runtime>();
    let mut save = SaveFile::capture(&runtime.runner, &runtime.staging);
    save.saved_at = saved_at;
    save::write_save(&save::legacy_save_path_for(runtime.runner.pack()), &save).unwrap();
}

/// Open an app whose pack has an importable legacy save sitting beside it.
fn app_with_legacy() -> (App, tempfile::TempDir) {
    let pack = temp_pack();
    let app = app_for_pack(pack.path().join("pack"));
    write_legacy(&app, None);
    (app, pack)
}

#[test]
fn the_legacy_card_is_shown_only_in_save_mode_while_importable() {
    // No legacy file: nothing to offer.
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    esc(&mut app);
    assert!(!menu::legacy_visible(menu(&app)), "no file, no card");

    // An importable legacy file: save mode shows it.
    let (mut app, _pack) = app_with_legacy();
    esc(&mut app);
    assert!(menu::legacy_visible(menu(&app)));

    // Load mode hides it; the file is still there.
    switch_mode(&mut app, MenuMode::Load);
    assert!(
        !menu::legacy_visible(menu(&app)),
        "load mode hides the card"
    );
    assert!(legacy_path(&app).exists());

    // Settings leaves the grid entirely.
    click(&mut app, MenuAction::OpenSettings);
    assert!(!menu::legacy_visible(menu(&app)), "settings hides the card");
    click(&mut app, MenuAction::BackToSlots);
    click(&mut app, MenuAction::SetMode(MenuMode::Save));
    assert!(
        menu::legacy_visible(menu(&app)),
        "back in save mode it returns"
    );

    // Keyboard reaches it: Down from the grid's bottom row.
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::Card(2);
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::LegacyCard);
    press_and_update(&mut app, KeyCode::ArrowUp);
    assert_eq!(menu(&app).focus, MenuFocus::Card(2));

    // An unreadable legacy file is neither shown nor touched.
    let (app, _pack) = app_with_legacy();
    let mut app = app;
    let before = b"not a save at all".to_vec();
    std::fs::write(legacy_path(&app), &before).unwrap();
    esc(&mut app);
    assert!(!menu::legacy_visible(menu(&app)), "unreadable is hidden");
    assert_eq!(std::fs::read(legacy_path(&app)).unwrap(), before);
}

#[test]
fn legacy_picker_labels_occupied_targets_as_overwrite() {
    let (mut app, _pack) = app_with_legacy();
    // Fill slot 1 (empty -> Save here) and slot 3 (unreadable).
    press_and_update(&mut app, KeyCode::F5);
    std::fs::write(slot_path(&app, Slot::Manual(3)), b"garbage").unwrap();
    esc(&mut app);

    click(&mut app, MenuAction::LegacyImport);
    assert_eq!(menu(&app).picker, Some(menu::LegacyPicker { target: 0 }));
    assert_eq!(menu(&app).focus, MenuFocus::LegacyTarget(0));

    let targets = menu::legacy_targets(menu(&app));
    assert_eq!(targets.len(), 3, "the three manual slots");
    assert!(targets[0].overwrite, "slot 1 is occupied");
    assert!(targets[0].display().contains("(overwrite)"));
    assert!(!targets[1].overwrite, "slot 2 is empty");
    assert!(!targets[1].display().contains("(overwrite)"));
    assert!(
        targets[2].overwrite,
        "slot 3 is unreadable, still overwritable"
    );

    // Down/Up walk the three rows; Esc backs out to the card, not the overlay.
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::LegacyTarget(1));
    esc(&mut app);
    assert_eq!(menu(&app).picker, None);
    assert!(menu(&app).open, "Esc cancelled the picker, not the overlay");
    assert_eq!(menu(&app).focus, MenuFocus::LegacyCard);

    // The picker is save-mode only: switching mode retires it and a stale
    // target action cannot import.
    let before = std::fs::read(legacy_path(&app)).unwrap();
    click(&mut app, MenuAction::LegacyImport);
    switch_mode(&mut app, MenuMode::Load);
    assert_eq!(menu(&app).picker, None, "load mode clears the picker");
    click(&mut app, MenuAction::LegacyTarget(Slot::Manual(2)));
    assert!(
        !slot_path(&app, Slot::Manual(2)).exists(),
        "no import in load mode"
    );
    assert_eq!(std::fs::read(legacy_path(&app)).unwrap(), before);
}

#[test]
fn importing_the_legacy_save_writes_the_target_and_retires_the_card() {
    let (mut app, _pack) = app_with_legacy();
    let legacy = legacy_path(&app);
    let original = std::fs::read(&legacy).unwrap();
    esc(&mut app);

    click(&mut app, MenuAction::LegacyImport);
    click(&mut app, MenuAction::LegacyTarget(Slot::Manual(2))); // Manual(2)

    // The slot holds the legacy save, unstamped, and the picker is retired.
    assert!(menu(&app).picker.is_none());
    assert!(!menu::legacy_visible(menu(&app)));
    let runtime = app.world().resource::<Runtime>();
    let imported = save::read_slot(runtime.runner.pack(), Slot::Manual(2)).unwrap();
    assert_eq!(imported.saved_at, None, "the import fabricates no time");
    assert_eq!(imported.wait, runtime.runner.wait().cloned());

    // The cards re-read: slot 2 is `Ok`, shows `—`, and the entry is gone.
    assert!(matches!(
        menu(&app).slots[1].status,
        SlotStatus::Ok { saved_at: None, .. }
    ));
    let model = menu::card_model(&menu(&app).slots[1], MenuMode::Save);
    assert_eq!(model.saved_at, "—", "an imported v1 save shows a dash");

    // The rename retires the entry without deleting the bytes.
    assert!(!legacy.exists());
    assert!(!menu::legacy_visible(menu(&app)));
    let imported_path = save::imported_legacy_path_for(runtime.runner.pack());
    assert_eq!(std::fs::read(&imported_path).unwrap(), original);

    // A second pass finds neither the entry nor an importable file.
    esc(&mut app);
    esc(&mut app);
    esc(&mut app);
    assert!(!menu::legacy_visible(menu(&app)));
}

#[test]
fn a_failed_legacy_import_notices_row_18_and_leaves_the_card() {
    captures::install();
    let (mut app, _pack) = app_with_legacy();
    force_single_threaded(&mut app);
    let legacy = legacy_path(&app);
    let before = std::fs::read(&legacy).unwrap();
    esc(&mut app);

    // Block the target write: slot 1's atomic temp path is a directory.
    std::fs::create_dir_all(tmp_path(&slot_path(&app, Slot::Manual(1)))).unwrap();
    captures::take();
    app.world_mut().resource_mut::<Notice>().text = None;

    click(&mut app, MenuAction::LegacyImport);
    click(&mut app, MenuAction::LegacyTarget(Slot::Manual(1)));

    let records = failure_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, log::Level::Error);
    assert!(
        records[0].2.starts_with("couldn't import the legacy save"),
        "{:?}",
        records[0].2
    );
    assert!(app.world().resource::<Notice>().is_showing());
    assert!(menu::legacy_visible(menu(&app)), "the card stays");
    assert!(menu(&app).picker.is_some(), "the picker stays for a retry");
    assert_eq!(std::fs::read(&legacy).unwrap(), before, "legacy untouched");
}

// ---------------------------------------------------------------------------
// Settings file + the Settings view audio group
// ---------------------------------------------------------------------------

/// The temporary sibling the atomic settings write uses.
fn settings_tmp_path(app: &App) -> PathBuf {
    settings_path(app).with_extension("ron.tmp")
}

/// Play one `←`/`→` step on the focused slider.
fn step_slider(app: &mut App, key: KeyCode) {
    assert!(
        menu::focus_control(menu(app).focus).is_some(),
        "a slider must be focused to step it"
    );
    press_and_update(app, key);
}

#[test]
fn settings_round_trip_persists_four_audio_values_and_no_play_key() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root.clone());
    open_settings(&mut app);

    // Master 100 -> 90, Music 100 -> 80, SFX 100 -> 95, Mute on.
    assert_eq!(menu(&app).focus, MenuFocus::AudioMaster);
    step_slider(&mut app, KeyCode::ArrowLeft);
    step_slider(&mut app, KeyCode::ArrowLeft);
    focus_down(&mut app, 1);
    for _ in 0..4 {
        step_slider(&mut app, KeyCode::ArrowLeft);
    }
    focus_down(&mut app, 1);
    step_slider(&mut app, KeyCode::ArrowLeft);
    focus_down(&mut app, 1);
    press_and_update(&mut app, KeyCode::Enter);

    assert_eq!(audio(&app).master, 0.90);
    assert_eq!(audio(&app).music, 0.80);
    assert_eq!(audio(&app).sfx, 0.95);
    assert!(audio(&app).muted);

    // Moving sliders writes nothing; the file appears only on leaving.
    assert!(
        !settings_path(&app).exists(),
        "no write while the view is open"
    );
    press_and_update(&mut app, KeyCode::Escape);
    assert_eq!(menu(&app).view, MenuView::Slots);
    assert!(settings_path(&app).exists(), "leaving the view flushes");

    let text = std::fs::read_to_string(settings_path(&app)).unwrap();
    assert!(text.contains("audio"), "{text}");
    assert!(!text.contains("play"), "an audio-only file has no play key");

    let reloaded = settings::load(&pack_root).settings;
    assert_eq!(reloaded.audio.master, 0.90);
    assert_eq!(reloaded.audio.music, 0.80);
    assert_eq!(reloaded.audio.sfx, 0.95);
    assert!(reloaded.audio.muted);

    // A fresh app over the same pack reads the same values.
    let fresh = app_for_pack(pack_root);
    assert_eq!(audio(&fresh), reloaded.audio);
}

#[test]
fn opening_settings_or_moving_sliders_creates_no_file() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    open_settings(&mut app);
    step_slider(&mut app, KeyCode::ArrowLeft);
    assert!(
        !settings_path(&app).exists(),
        "opening and adjusting writes nothing"
    );

    // Backing out to the grid with a pending change flushes once.
    press_and_update(&mut app, KeyCode::Escape);
    assert_eq!(menu(&app).view, MenuView::Slots);
    assert!(settings_path(&app).exists());
}

#[test]
fn a_preexisting_settings_file_is_byte_identical_until_a_real_change() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root);
    let path = settings_path(&app);
    let original =
        b"(audio: (master: 0.5, music: 0.5, sfx: 0.5, muted: false), future: 7)".to_vec();
    std::fs::write(&path, &original).unwrap();

    // Load and leave the overlay / save and load a slot: no settings change, no
    // rewrite. The unknown `future` key survives untouched.
    esc(&mut app);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::F5);
    press_and_update(&mut app, KeyCode::F9);
    esc(&mut app);
    assert_eq!(std::fs::read(&path).unwrap(), original);
}

#[test]
fn loading_a_slot_leaves_the_settings_file_byte_identical() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::F5);

    // A settings file exists and holds a non-default mix.
    {
        let mut state = app.world_mut().resource_mut::<SettingsState>();
        state.set_fraction(AudioControl::Music, 0.25);
        let settings = state.settings.clone();
        settings::save(&state.pack_root, &settings).unwrap();
    }
    let before = std::fs::read(settings_path(&app)).unwrap();

    press_and_update(&mut app, KeyCode::F9);
    assert_eq!(std::fs::read(settings_path(&app)).unwrap(), before);
}

#[test]
fn mute_then_leave_then_unmute_restores_levels_and_only_muted_changes() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root.clone());

    open_settings(&mut app);
    step_slider(&mut app, KeyCode::ArrowLeft);
    step_slider(&mut app, KeyCode::ArrowLeft); // master 90
    focus_down(&mut app, 3); // Master -> Mute
    assert_eq!(menu(&app).focus, MenuFocus::AudioMute);
    press_and_update(&mut app, KeyCode::Enter); // mute on

    press_and_update(&mut app, KeyCode::Escape); // leave -> flush
    let muted_file = settings::load(&pack_root).settings;
    assert!(muted_file.audio.muted);
    assert_eq!(muted_file.audio.master, 0.90, "muting keeps the level");

    open_settings(&mut app);
    focus_down(&mut app, 3);
    press_and_update(&mut app, KeyCode::Enter); // unmute
    press_and_update(&mut app, KeyCode::Escape);

    let unmuted_file = settings::load(&pack_root).settings;
    assert!(!unmuted_file.audio.muted);
    assert_eq!(unmuted_file.audio.master, 0.90, "the level is restored");
    assert_eq!(unmuted_file.audio.music, 1.0);
    assert_eq!(unmuted_file.audio.sfx, 1.0);
}

#[test]
fn audio_reset_confirms_once_resets_only_audio_and_never_deletes_the_file() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let path = settings::settings_path_for(&pack_root);
    std::fs::write(
        &path,
        "(audio: (master: 0.5, music: 0.5, sfx: 0.5, muted: true), play: (auto: true))",
    )
    .unwrap();
    let mut app = app_for_pack(pack_root.clone());

    open_settings(&mut app);
    focus_down(&mut app, 4); // Master -> Reset
    assert_eq!(menu(&app).focus, MenuFocus::AudioReset);
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::ResetAudio));

    // Cancelling changes nothing.
    press_and_update(&mut app, KeyCode::Escape);
    assert_eq!(menu(&app).confirm, None);
    assert_eq!(audio(&app).master, 0.5);
    assert!(audio(&app).muted);
    assert_eq!(menu(&app).view, MenuView::Settings);

    // Accepting resets the audio section only.
    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, None);
    assert_eq!(audio(&app), settings::AudioSettings::default());
    assert!(
        app.world().resource::<SettingsState>().settings.play.auto,
        "reset never reaches into play"
    );

    press_and_update(&mut app, KeyCode::Escape); // leave -> flush
    assert!(path.exists(), "reset never deletes the file");
    let written = settings::load(&pack_root).settings;
    assert_eq!(written.audio, settings::AudioSettings::default());
    assert!(written.play.auto, "the pre-existing play key survives");
}

#[test]
fn fullscreen_boot_reads_the_play_section_into_the_created_window() {
    let dir = temp_pack();
    let pack = dir.path().join("pack");
    let path = settings::settings_path_for(&pack);

    // `fullscreen: true` -> borderless primary.
    std::fs::write(&path, "(play: (fullscreen: true))").unwrap();
    assert_eq!(
        settings::window_mode(settings::load(&pack).settings.play.fullscreen),
        WindowMode::BorderlessFullscreen(MonitorSelection::Primary)
    );

    // `false` -> windowed.
    std::fs::write(&path, "(play: (fullscreen: false))").unwrap();
    assert_eq!(
        settings::window_mode(settings::load(&pack).settings.play.fullscreen),
        WindowMode::Windowed
    );

    // A missing key and an unreadable file -> windowed.
    let missing = temp_pack();
    assert_eq!(
        settings::window_mode(
            settings::load(&missing.path().join("pack"))
                .settings
                .play
                .fullscreen
        ),
        WindowMode::Windowed
    );
    let bad = temp_pack();
    let bad_pack = bad.path().join("pack");
    std::fs::write(settings::settings_path_for(&bad_pack), "not ron").unwrap();
    assert_eq!(
        settings::window_mode(settings::load(&bad_pack).settings.play.fullscreen),
        WindowMode::Windowed
    );
}

#[test]
fn the_live_window_mode_follows_the_fullscreen_field() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    let window = app
        .world_mut()
        .spawn((Window::default(), PrimaryWindow))
        .id();

    app.world_mut()
        .run_system_once(crate::presentation::settings_view::sync_window_mode)
        .unwrap();
    assert_eq!(
        app.world().get::<Window>(window).unwrap().mode,
        WindowMode::Windowed
    );

    app.world_mut()
        .resource_mut::<SettingsState>()
        .toggle_fullscreen();
    app.world_mut()
        .run_system_once(crate::presentation::settings_view::sync_window_mode)
        .unwrap();
    assert_eq!(
        app.world().get::<Window>(window).unwrap().mode,
        WindowMode::BorderlessFullscreen(MonitorSelection::Primary),
        "an F11/switch toggle reaches the live window"
    );
}

#[test]
fn setup_settings_uses_the_preloaded_settings_without_rereading_the_file() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    // The file on disk says windowed; the preloaded value says fullscreen.
    std::fs::write(
        settings::settings_path_for(&pack_root),
        "(play: (fullscreen: false))",
    )
    .unwrap();
    let mut preloaded = settings::Settings::default();
    preloaded.play.fullscreen = true;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .insert_resource(RuntimeConfig::new(pack_root, None).with_dev_mode(true))
        .insert_resource(settings::StartupSettings(settings::LoadedSettings {
            settings: preloaded,
            warning: None,
        }))
        .add_plugins(RuntimeCorePlugin);
    app.update();

    assert!(
        app.world().resource::<SettingsState>().fullscreen(),
        "the preloaded startup value wins over the file"
    );
}

#[test]
fn the_play_switches_toggle_auto_and_fullscreen_and_persist_on_leave() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root.clone());
    open_settings(&mut app);

    // Master -> PlayAuto is six rows down (Text speed precedes it).
    focus_down(&mut app, 6);
    assert_eq!(menu(&app).focus, MenuFocus::PlayAuto);
    press_and_update(&mut app, KeyCode::Enter);
    assert!(app.world().resource::<SettingsState>().auto());
    assert_eq!(
        settings_switch_text(&mut app, SettingsLabelKind::Auto),
        "On"
    );
    press_and_update(&mut app, KeyCode::Space);
    assert!(!app.world().resource::<SettingsState>().auto());
    assert_eq!(
        settings_switch_text(&mut app, SettingsLabelKind::Auto),
        "Off"
    );

    focus_down(&mut app, 1);
    assert_eq!(menu(&app).focus, MenuFocus::PlayFullscreen);
    press_and_update(&mut app, KeyCode::Enter);
    assert!(app.world().resource::<SettingsState>().fullscreen());
    assert_eq!(
        settings_switch_text(&mut app, SettingsLabelKind::Fullscreen),
        "On"
    );
    assert!(
        !settings_path(&app).exists(),
        "no write while the view is open"
    );

    press_and_update(&mut app, KeyCode::Escape); // leave -> flush
    let written = settings::load(&pack_root).settings;
    assert!(written.play.fullscreen);
    assert!(!written.play.auto);
}

#[test]
fn f11_toggles_fullscreen_marks_dirty_and_the_switch_reflects_it() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root.clone());
    assert!(!app.world().resource::<SettingsState>().fullscreen());

    press_and_update(&mut app, KeyCode::F11);
    let state = app.world().resource::<SettingsState>();
    assert!(state.fullscreen(), "F11 flips the field");
    assert!(state.dirty, "and marks the settings dirty");
    assert!(
        !settings_path(&app).exists(),
        "there is no write on key press"
    );

    // Opening the view shows the same field: the switch reflects the hotkey
    // without the overlay having been reopened to pick it up.
    open_settings(&mut app);
    assert_eq!(
        settings_switch_text(&mut app, SettingsLabelKind::Fullscreen),
        "On"
    );
}

#[test]
fn f11_is_inert_at_a_choice_wait() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root.clone());
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    assert!(matches!(
        app.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Choice { .. })
    ));

    press_and_update(&mut app, KeyCode::F11);
    let state = app.world().resource::<SettingsState>();
    assert!(!state.fullscreen(), "F11 does nothing at a choice-wait");
    assert!(!state.dirty);
    assert!(!settings_path(&app).exists());
}

#[test]
fn f11_is_ignored_while_the_overlay_is_open() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root.clone());
    esc(&mut app);
    assert!(menu(&app).open);

    press_and_update(&mut app, KeyCode::F11);
    let state = app.world().resource::<SettingsState>();
    assert!(
        !state.fullscreen(),
        "the overlay owns input while it is open"
    );
    assert!(!state.dirty);
}

#[test]
fn toggling_f11_twice_restores_the_file_bytes() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root.clone());

    // Seed a canonical file with a non-default play section.
    {
        let mut state = app.world_mut().resource_mut::<SettingsState>();
        state.settings.play.auto = true;
        settings::save(&state.pack_root, &state.settings).unwrap();
    }
    let before = std::fs::read(settings_path(&app)).unwrap();

    press_and_update(&mut app, KeyCode::F11);
    press_and_update(&mut app, KeyCode::F11);
    assert!(!app.world().resource::<SettingsState>().fullscreen());
    assert_eq!(
        std::fs::read(settings_path(&app)).unwrap(),
        before,
        "an unflushed pair of toggles touches nothing"
    );

    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<AppExit>>()
        .write(AppExit::Success);
    app.update();
    assert_eq!(
        std::fs::read(settings_path(&app)).unwrap(),
        before,
        "toggling F11 twice restores the file bytes"
    );
}

#[test]
fn an_app_exit_flushes_a_pending_fullscreen_toggle() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root.clone());

    press_and_update(&mut app, KeyCode::F11);
    assert!(app.world().resource::<SettingsState>().dirty);
    assert!(
        !settings_path(&app).exists(),
        "no write until a flush point"
    );

    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<AppExit>>()
        .write(AppExit::Success);
    app.update();

    assert!(!app.world().resource::<SettingsState>().dirty);
    assert!(settings::load(&pack_root).settings.play.fullscreen);
}

#[test]
fn play_reset_confirms_once_resets_only_play_and_persists_on_leave() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let path = settings::settings_path_for(&pack_root);
    let mut seeded = settings::Settings::default();
    seeded.audio.master = 0.5;
    seeded.audio.music = 0.5;
    seeded.audio.sfx = 0.5;
    seeded.audio.muted = true;
    seeded.play.text_speed = 0.04;
    seeded.play.auto = true;
    seeded.play.fullscreen = true;
    settings::save(&pack_root, &seeded).unwrap();
    let mut app = app_for_pack(pack_root.clone());

    open_settings(&mut app);
    focus_down(&mut app, 8); // Master -> PlayReset
    assert_eq!(menu(&app).focus, MenuFocus::PlayReset);
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::ResetPlay));

    // Cancelling changes nothing.
    press_and_update(&mut app, KeyCode::Escape);
    assert_eq!(menu(&app).confirm, None);
    assert_eq!(menu(&app).view, MenuView::Settings);
    let state = app.world().resource::<SettingsState>();
    assert!(state.auto());
    assert!(state.fullscreen());
    assert_eq!(state.settings.play.text_speed, 0.04);

    // Accepting resets the play section only.
    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, None);
    let state = app.world().resource::<SettingsState>();
    assert_eq!(state.settings.play, settings::PlaySettings::default());
    assert_eq!(state.settings.audio, seeded.audio, "audio is untouched");

    // Persist on leave: the audio bytes are canonical and identical, and the
    // now-default play section is skipped entirely.
    press_and_update(&mut app, KeyCode::Escape);
    let expected = ron::ser::to_string_pretty(
        &settings::Settings {
            audio: seeded.audio.clone(),
            play: settings::PlaySettings::default(),
        },
        ron::ser::PrettyConfig::default(),
    )
    .unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
    assert!(
        !std::fs::read_to_string(&path).unwrap().contains("play"),
        "an all-default play section carries no key"
    );
}

#[test]
fn a_failed_flush_keeps_values_notices_row_20_and_the_next_flush_retries() {
    captures::install();
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root.clone());
    force_single_threaded(&mut app);

    open_settings(&mut app);
    step_slider(&mut app, KeyCode::ArrowLeft); // master 95, dirty
    std::fs::create_dir_all(settings_tmp_path(&app)).unwrap();

    captures::take();
    app.world_mut().resource_mut::<Notice>().text = None;

    // Leaving the view attempts the one flush; it fails.
    press_and_update(&mut app, KeyCode::Escape);
    assert_eq!(menu(&app).view, MenuView::Slots);
    let state = app.world().resource::<SettingsState>();
    assert!(state.dirty, "a failed flush keeps the change pending");
    assert_eq!(state.settings.audio.master, 0.95, "in-session value kept");
    assert!(app.world().resource::<Notice>().is_showing());
    assert_eq!(
        app.world().resource::<Notice>().text.as_deref(),
        Some("settings not saved")
    );
    let records = failure_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert!(records[0].2.starts_with("settings not saved"));

    // Unblock and close the overlay: the next flush retries and succeeds.
    std::fs::remove_dir_all(settings_tmp_path(&app)).unwrap();
    press_and_update(&mut app, KeyCode::Escape);
    assert!(!menu(&app).open);
    assert!(!app.world().resource::<SettingsState>().dirty);
    assert_eq!(settings::load(&pack_root).settings.audio.master, 0.95);
}

#[test]
fn an_app_exit_flushes_a_pending_change() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root.clone());
    open_settings(&mut app);
    step_slider(&mut app, KeyCode::ArrowLeft);
    assert!(app.world().resource::<SettingsState>().dirty);
    assert!(!settings_path(&app).exists());

    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<AppExit>>()
        .write(AppExit::Success);
    app.update();

    assert!(!app.world().resource::<SettingsState>().dirty);
    assert_eq!(settings::load(&pack_root).settings.audio.master, 0.95);
}

#[test]
fn the_settings_keyboard_model_moves_controls_and_steps_sliders() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_settings(&mut app);

    assert_eq!(menu(&app).focus, MenuFocus::AudioMaster);
    // Left/Right adjust without moving focus.
    press_and_update(&mut app, KeyCode::ArrowLeft);
    assert_eq!(menu(&app).focus, MenuFocus::AudioMaster);
    assert_eq!(
        app.world()
            .resource::<SettingsState>()
            .percent(AudioControl::Master),
        95
    );
    press_and_update(&mut app, KeyCode::ArrowRight);
    assert_eq!(
        app.world()
            .resource::<SettingsState>()
            .percent(AudioControl::Master),
        100
    );

    // Down walks the controls, passing the rail's `Exit to title` and wrapping
    // to the first one.
    for expected in [
        MenuFocus::AudioMusic,
        MenuFocus::AudioSfx,
        MenuFocus::AudioMute,
        MenuFocus::AudioReset,
        MenuFocus::TextSpeed,
        MenuFocus::PlayAuto,
        MenuFocus::PlayFullscreen,
        MenuFocus::PlayReset,
        MenuFocus::KeyRow,
        MenuFocus::CopyKeyPath,
        MenuFocus::ClearKey,
        MenuFocus::ExitToTitle,
        MenuFocus::AudioMaster,
    ] {
        press_and_update(&mut app, KeyCode::ArrowDown);
        assert_eq!(menu(&app).focus, expected);
    }
    press_and_update(&mut app, KeyCode::ArrowUp);
    assert_eq!(menu(&app).focus, MenuFocus::ExitToTitle);

    // Tab cycles the top bar; Down returns to the group's first control.
    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).focus, MenuFocus::SaveTab);
    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).focus, MenuFocus::LoadTab);
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::AudioMaster);

    // Enter toggles mute while it is focused.
    focus_down(&mut app, 3);
    press_and_update(&mut app, KeyCode::Enter);
    assert!(audio(&app).muted);
    press_and_update(&mut app, KeyCode::Enter);
    assert!(!audio(&app).muted);

    // Space is the same toggle.
    press_and_update(&mut app, KeyCode::Space);
    assert!(audio(&app).muted);
    assert_eq!(menu(&app).focus, MenuFocus::AudioMute);
}

#[test]
fn a_malformed_settings_file_is_defaults_warns_row_19_and_keeps_the_bytes() {
    captures::install();
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let path = settings::settings_path_for(&pack_root);
    let original = b"not ron at all".to_vec();
    std::fs::write(&path, &original).unwrap();

    // Build the plugin (so `Notice`/`DefensiveHold` exist) but drive
    // `setup_settings` directly on this thread, so the row-19 line is captured
    // rather than emitted on a Bevy executor worker thread.
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .insert_resource(RuntimeConfig::new(pack_root, None).with_dev_mode(true))
        .add_plugins(RuntimeCorePlugin);
    captures::take();
    app.world_mut()
        .run_system_once(crate::settings::setup_settings)
        .unwrap();

    assert_eq!(
        app.world().resource::<SettingsState>().settings,
        settings::Settings::default(),
        "a malformed file is all defaults"
    );
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "row 19 is silent"
    );
    assert!(!app.world().resource::<DefensiveHold>().is_held());
    assert_eq!(std::fs::read(&path).unwrap(), original, "bytes untouched");

    let records = failure_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, log::Level::Warn);
}

// ---------------------------------------------------------------------------
// The gated title
// ---------------------------------------------------------------------------

/// The title root's one radial gradient, so its resolved centre can be pinned
/// without re-reading the whole entity tree.
fn title_gradient_radial(app: &App) -> RadialGradient {
    let root = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<crate::presentation::MenuRoot>()
                .filter(|_| entity.get::<BackgroundGradient>().is_some())
                .map(|_| entity.id())
        })
        .expect("the title root draws the neutral gradient");
    single_radial(app, root)
}

/// The tone of Continue's reason line in the built title.
fn title_reason_color(app: &App) -> Color {
    let line = menu(app).continue_line.clone();
    app.world()
        .iter_entities()
        .find_map(|entity| {
            let text = entity.get::<Text>()?;
            if text.0 != line {
                return None;
            }
            entity.get::<TextColor>().map(|color| color.0)
        })
        .expect("the reason line renders")
}

/// Assert the built title tree opens no scroll container and caps its column at
/// the spec's width (`docs/title-and-menu.md` § Title layout §1).
fn assert_title_never_scrolls(app: &mut App) {
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(app);
    for entity in app.world().iter_entities() {
        if let Some(node) = entity.get::<Node>() {
            assert_eq!(
                node.overflow,
                Overflow::default(),
                "the title opens no scroll container"
            );
        }
    }
    let column = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            let node = entity.get::<Node>()?;
            (node.width == Val::Px(520.0)).then_some(node.clone())
        })
        .expect("the title column is the spec's 520px");
    assert_eq!(column.max_width, Val::Percent(92.0));
    assert!(
        app.world().iter_entities().any(|entity| {
            entity
                .get::<Node>()
                .map(|node| node.justify_content == JustifyContent::SpaceBetween)
                .unwrap_or(false)
        }),
        "the foot row spans the column to split reason line and legend"
    );
}

#[test]
fn boot_leaves_the_title_up_over_the_entry_scenes_opening_frame() {
    let app = temp_title_app();
    assert!(
        menu(&app).title_up(),
        "the first frame after a good load is the title"
    );

    let runtime = app.world().resource::<Runtime>();
    assert_eq!(runtime.runner.scene_id(), "first_meeting");
    assert!(matches!(runtime.runner.wait(), Some(Wait::Say { .. })));
    assert_eq!(runtime.staging.background.as_deref(), Some("school_gate"));
    assert_eq!(runtime.staging.bgm.as_deref(), Some("music_morning"));

    for info in unmei_engine::list_slots(runtime.runner.pack()) {
        assert_eq!(info.status, SlotStatus::Empty, "a launch writes no slot");
    }
}

#[test]
fn the_title_freezes_the_play_path_byte_identical() {
    let mut app = temp_title_app();
    let before = parked_position(&app);

    for key in [
        KeyCode::Space,
        KeyCode::KeyA,
        KeyCode::F11,
        KeyCode::F5,
        KeyCode::F9,
        KeyCode::Escape,
        KeyCode::Digit1,
        KeyCode::Tab,
    ] {
        press_and_update(&mut app, key);
        assert_eq!(parked_position(&app), before, "{key:?} moved the runner");
        assert!(menu(&app).title_up(), "{key:?} left the title");
    }
    assert!(
        !autosave_path(&app).exists(),
        "the title writes no autosave"
    );
}

#[test]
fn enter_play_presses_a_real_new_game() {
    let mut app = temp_title_app();
    assert!(menu(&app).title_up());
    enter_play(&mut app);
    assert!(!menu(&app).open, "New Game entered play");
    assert!(live_session(&app), "entering play sets the live session");
    assert!(matches!(
        app.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Say { .. })
    ));
}

#[test]
fn new_game_at_boot_starts_play_with_no_confirm() {
    let pack = temp_pack();
    let mut app = boot_app(pack.path().join("pack"), None);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleNewGame;
    press_and_update(&mut app, KeyCode::Enter);

    assert!(!menu(&app).open, "New Game is immediate at boot");
    assert!(menu(&app).confirm.is_none(), "no confirm at boot");
    assert!(live_session(&app));
    assert!(!app.world().resource::<crate::slots::PendingAutosave>().0);
    assert!(!autosave_path(&app).exists());
    assert!(!manual_path(&app).exists());
}

#[test]
fn quit_at_boot_exits_with_no_confirm() {
    let mut app = temp_title_app();
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleQuit;
    press_and_update(&mut app, KeyCode::Enter);
    assert!(
        menu(&app).confirm.is_none(),
        "no confirm with no live session"
    );
    assert!(matches!(app.should_exit(), Some(AppExit::Success)));
}

#[test]
fn continue_is_disabled_with_no_autosave_and_focus_falls_to_new_game() {
    let pack = temp_pack();
    let app = boot_app(pack.path().join("pack"), None);
    let state = menu(&app);
    assert!(!state.continue_enabled, "no autosave disables Continue");
    assert_eq!(state.continue_line, "No autosave yet");
    assert_eq!(
        state.focus,
        MenuFocus::TitleNewGame,
        "focus falls to New Game"
    );
}

/// Re-enter the title with the current live session, without holding a borrow
/// of `Runtime` while mutating `MenuState`.
fn reenter_title(app: &mut App) {
    let (pack, scene) = {
        let runtime = app.world().resource::<Runtime>();
        (
            runtime.runner.pack().clone(),
            runtime.runner.scene_id().to_string(),
        )
    };
    app.world_mut()
        .resource_mut::<MenuState>()
        .enter_title(&pack, &scene);
}

#[test]
fn continue_reads_the_autosave_once_the_title_is_entered_again() {
    let pack = temp_pack();
    let mut app = boot_app(pack.path().join("pack"), None);
    enter_play(&mut app);
    play_to_end(&mut app);
    reenter_title(&mut app);
    let state = menu(&app);
    assert!(
        state.continue_enabled,
        "a loadable autosave enables Continue"
    );
    assert!(
        state.continue_line.contains("reunion") || state.continue_line.contains("epilogue"),
        "Continue names the autosave's scene: {:?}",
        state.continue_line
    );
}

#[test]
fn title_settings_opens_the_shared_view_with_the_title_door_and_backs_out() {
    let mut app = temp_title_app();
    app.init_resource::<crate::presentation::MenuBuilt>();
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleSettings;
    press_and_update(&mut app, KeyCode::Enter);

    assert_eq!(menu(&app).view, MenuView::Settings);
    assert_eq!(menu(&app).settings_origin, menu::MenuOrigin::Title);

    build_menu_ui(&mut app);
    let texts = all_texts(&app);
    assert!(
        texts.iter().any(|text| text == "\u{2190} Back to title"),
        "the title door's back control reads the title label: {texts:?}"
    );
    assert!(
        !texts.iter().any(|text| text.contains("Resume")),
        "the title's Settings has no Resume"
    );
    assert!(
        !texts.iter().any(|text| text == "Exit to title"),
        "the title's Settings has no Exit to title"
    );

    esc(&mut app);
    assert_eq!(menu(&app).view, MenuView::Title);
    assert!(menu(&app).title_up());
    esc(&mut app);
    assert!(menu(&app).title_up(), "Esc on the title does nothing");
}

#[test]
fn exit_to_title_keeps_the_live_session_and_continue_resumes_it() {
    let mut app = temp_title_app();
    enter_play(&mut app);
    press_and_update(&mut app, KeyCode::Space);
    let before = parked_position(&app);

    esc(&mut app);
    click(&mut app, MenuAction::OpenSettings);
    assert_eq!(menu(&app).settings_origin, menu::MenuOrigin::Slots);
    click(&mut app, MenuAction::ExitToTitle);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::ExitToTitle));
    click(&mut app, MenuAction::ConfirmAccept);

    assert!(menu(&app).title_up(), "Exit to title shows the title");
    assert!(live_session(&app), "the session is kept");
    assert_eq!(parked_position(&app), before, "position is untouched");
    assert!(
        !autosave_path(&app).exists(),
        "Exit to title writes nothing"
    );

    // A slot that turns unreadable behind the title cannot affect a
    // live-session Continue: it reads no slot.
    std::fs::write(autosave_path(&app), b"not a save").unwrap();

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
    press_and_update(&mut app, KeyCode::Enter);
    assert!(!menu(&app).open, "Continue entered play");
    assert_eq!(parked_position(&app), before, "Continue resumes verbatim");
}

#[test]
fn new_game_with_a_live_session_confirms_once_and_resets() {
    let mut app = temp_title_app();
    enter_play(&mut app);
    press_and_update(&mut app, KeyCode::Space);
    // The fresh-boot park is the entry scene's first say.
    let boot_wait = {
        let pack = app.world().resource::<Runtime>().runner.pack().clone();
        ScriptRunner::new(pack).run().unwrap().wait
    };
    reenter_title(&mut app);

    app.world_mut().resource_mut::<Notice>().show("stale");
    app.world_mut()
        .resource_mut::<crate::slots::PendingAutosave>()
        .arm();

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleNewGame;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::NewGame));
    click(&mut app, MenuAction::ConfirmAccept);

    assert!(!menu(&app).open, "accepting starts play");
    assert!(live_session(&app));
    let runtime = app.world().resource::<Runtime>();
    assert_eq!(runtime.runner.scene_id(), "first_meeting");
    assert_eq!(
        runtime.runner.wait().cloned(),
        boot_wait,
        "back at the fresh-boot park"
    );
    assert_eq!(runtime.staging.background.as_deref(), Some("school_gate"));
    assert!(!app.world().resource::<Notice>().is_showing());
    assert!(!app.world().resource::<crate::slots::PendingAutosave>().0);
    assert!(
        app.world().resource::<AppliedAudio>().is_forced(),
        "audio replays"
    );
    assert!(
        app.world().resource::<AppliedVisuals>().is_forced(),
        "visuals replay"
    );
    assert!(
        !app.world().resource::<AppliedUi>().is_ready(),
        "the reveal is re-armed"
    );
}

#[test]
fn new_game_cancels_an_in_flight_decision() {
    let fake = FakeResolver::new(model_answer("warm"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    assert!(app.world().resource::<PendingDecision>().0.is_some());

    reenter_title(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleNewGame;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::NewGame));
    click(&mut app, MenuAction::ConfirmAccept);
    assert!(!menu(&app).open);
    assert!(app.world().resource::<PendingDecision>().0.is_none());
    assert!(app.world().resource::<HeldDecision>().0.is_none());
    fake.wait_until_dropped();
}

/// New Game writes no slot and no settings change; the autosave only lands on
/// the first scene crossing after it (`docs/title-and-menu.md` § Title §3, §7).
#[test]
fn new_game_writes_no_slot_and_the_first_crossing_autosaves() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    // A manual slot and a dirty settings file exist to prove New Game leaves
    // both alone.
    press_and_update(&mut app, KeyCode::F5);
    assert!(manual_path(&app).exists(), "the manual save landed");
    reenter_title(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleSettings;
    press_and_update(&mut app, KeyCode::Enter);
    click(&mut app, MenuAction::AudioMute);
    esc(&mut app); // back to the title, flushing the dirty settings file
    assert!(menu(&app).title_up());
    assert!(settings_path(&app).exists(), "the flush landed");

    let manual_before = std::fs::read(manual_path(&app)).unwrap();
    let settings_before = std::fs::read(settings_path(&app)).unwrap();

    // New Game with a live session: one confirm, then a fresh session.
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleNewGame;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::NewGame));
    click(&mut app, MenuAction::ConfirmAccept);
    assert!(!menu(&app).open, "New Game entered play");

    assert!(!autosave_path(&app).exists(), "New Game writes no autosave");
    assert_eq!(
        std::fs::read(manual_path(&app)).unwrap(),
        manual_before,
        "the manual slot is byte-identical"
    );
    assert_eq!(
        std::fs::read(settings_path(&app)).unwrap(),
        settings_before,
        "the settings file is byte-identical"
    );

    // The first scene crossing after New Game is what writes the autosave.
    press_and_update(&mut app, KeyCode::Space); // Ace's say
    press_and_update(&mut app, KeyCode::Space); // the choice
    press_and_update(&mut app, KeyCode::Digit1); // ask_letter's say
    press_and_update(&mut app, KeyCode::Space); // jump reunion
    assert_eq!(
        app.world().resource::<Runtime>().runner.scene_id(),
        "reunion"
    );
    let save = save::read_slot(
        app.world().resource::<Runtime>().runner.pack(),
        Slot::Autosave,
    )
    .expect("the crossing autosaved");
    assert_eq!(save.scene_id, "reunion");
}

/// Manual slots and the credential file are byte-identical after a New Game
/// (`docs/title-and-menu.md` § Title §3, §7).
#[test]
fn manual_slots_and_the_credential_file_survive_a_new_game() {
    let pack = temp_pack();
    let config_dir = tempfile::tempdir().unwrap();
    let credential = credentials::credentials_path_in(config_dir.path());
    let key = format!("sk-nano-{}", "m".repeat(60));
    credentials::write(config_dir.path(), &key).unwrap();
    let credential_before = std::fs::read(&credential).unwrap();

    let mut app = app_for_pack(pack.path().join("pack"));
    app.insert_resource(CredentialsState {
        config_dir: Some(config_dir.path().to_path_buf()),
        path: Some(credential.clone()),
        row: KeyRow::NotSet,
    });
    press_and_update(&mut app, KeyCode::F5);
    let manual_before = std::fs::read(manual_path(&app)).unwrap();

    reenter_title(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleNewGame;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::NewGame));
    click(&mut app, MenuAction::ConfirmAccept);

    assert_eq!(
        std::fs::read(manual_path(&app)).unwrap(),
        manual_before,
        "the manual slot is untouched"
    );
    assert_eq!(
        std::fs::read(&credential).unwrap(),
        credential_before,
        "the credential file is untouched"
    );
}

/// After New Game + Quit + relaunch, Continue loads the pre-New-Game autosave:
/// New Game never clobbers the checkpoint (`docs/title-and-menu.md` § Title §3's
/// accepted consequence, §7).
#[test]
fn the_old_autosave_survives_new_game_quit_and_relaunch() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");

    // Playthrough 1 crosses into `reunion`, writing the autosave.
    let mut app = app_for_pack(pack_root.clone());
    press_and_update(&mut app, KeyCode::Space); // Ace's say
    press_and_update(&mut app, KeyCode::Space); // the choice
    press_and_update(&mut app, KeyCode::Digit1); // ask_letter's say
    press_and_update(&mut app, KeyCode::Space); // jump reunion
    let (saved_scene, saved_wait) = {
        let runtime = app.world().resource::<Runtime>();
        assert_eq!(runtime.runner.scene_id(), "reunion");
        (
            runtime.runner.scene_id().to_string(),
            runtime.runner.wait().cloned(),
        )
    };

    // New Game from the live session, then Quit.
    reenter_title(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleNewGame;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::NewGame));
    click(&mut app, MenuAction::ConfirmAccept);
    assert!(!menu(&app).open, "New Game started the new session");

    reenter_title(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleQuit;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::Quit));
    click(&mut app, MenuAction::ConfirmAccept);
    assert!(matches!(app.should_exit(), Some(AppExit::Success)));

    // Relaunch: the old autosave is still the checkpoint Continue loads.
    let mut relaunched = boot_app(pack_root, None);
    assert!(menu(&relaunched).continue_enabled);
    relaunched.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
    press_and_update(&mut relaunched, KeyCode::Enter);
    assert!(!menu(&relaunched).open, "Continue entered play");
    let runtime = relaunched.world().resource::<Runtime>();
    assert_eq!(runtime.runner.scene_id(), saved_scene.as_str());
    assert_eq!(runtime.runner.wait().cloned(), saved_wait);
}

#[test]
fn the_title_renders_the_wordmark_rows_caption_and_legend() {
    let mut app = temp_title_app();
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut app);

    let name = app
        .world()
        .resource::<Runtime>()
        .runner
        .pack()
        .manifest
        .name
        .clone();
    let texts = all_texts(&app);
    assert!(
        texts.contains(&name),
        "the wordmark is the manifest name: {texts:?}"
    );
    for label in ["Continue", "New Game", "Settings", "Gallery", "Quit"] {
        assert!(
            texts.iter().any(|text| text.starts_with(label)),
            "missing {label}"
        );
    }
    assert_eq!(
        title_label(&app, MenuAction::Gallery).as_deref(),
        Some("Gallery"),
        "Gallery's row is caption-free"
    );
    let legend_count = texts
        .iter()
        .filter(|text| *text == menu::TITLE_LEGEND)
        .count();
    assert_eq!(
        legend_count, 1,
        "the legend renders exactly once: {texts:?}"
    );
    for chip in ["AUTOSAVE PRESENT", "LIVE FROZEN SESSION"] {
        assert!(
            texts.iter().all(|text| !text.contains(chip)),
            "the prototype's state chip never renders: {chip}"
        );
    }
    assert!(
        texts.iter().any(|text| text == "No autosave yet"),
        "Continue's reason line renders"
    );
}

/// The neutral title gradient's bright tone sits at the upper middle — the
/// node's horizontal centre, ~28% down — not on the right edge, where
/// `CENTER.at_percent(50, 28)` lands: a `UiPosition`'s `x`/`y` are offsets from
/// the anchor, and `resolve` yields node-centre-relative coordinates
/// (`docs/title-and-menu.md` § Title layout §2).
#[test]
fn the_title_gradient_centres_on_the_upper_middle() {
    let mut app = temp_title_app();
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut app);

    let radial = title_gradient_radial(&app);
    assert_eq!(radial.shape, RadialGradientShape::FarthestCorner);
    assert_eq!(
        radial.stops.len(),
        2,
        "the neutral backdrop is a two-stop radial"
    );
    assert_eq!(radial.stops[0].point, Val::Percent(0.0));
    assert_eq!(radial.stops[1].point, Val::Percent(100.0));
    assert_eq!(radial.stops[0].color, Color::srgb(0.106, 0.133, 0.188));
    assert_eq!(radial.stops[1].color, Color::srgb(0.043, 0.051, 0.071));

    // Resolve against a 1280x720 node. `resolve` returns node-centre-relative
    // coordinates — the middle is the origin and the corners are ±half the
    // node — so the upper middle is `(0, -0.22*H)`. The title root is
    // full-window, so the node and its render target are the same size.
    let size = Vec2::new(1280.0, 720.0);
    let centre = radial.position.resolve(1.0, size, size);
    assert_eq!(centre.x, 0.0, "the bright tone is horizontally centred");
    let from_top = centre.y + size.y / 2.0;
    assert!(
        (from_top - size.y * 0.28).abs() < 0.5,
        "the bright tone is ~28% down from the top, got {from_top}"
    );
}

/// The text of `parent`'s direct children, in child order.
fn child_texts(app: &App, parent: Entity) -> Vec<String> {
    let Some(children) = app.world().entity(parent).get::<Children>() else {
        return Vec::new();
    };
    children
        .iter()
        .filter_map(|child| app.world().entity(child).get::<Text>().map(|t| t.0.clone()))
        .collect()
}

/// The parent of the single text node holding `text`.
fn text_parent(app: &App, text: &str) -> Entity {
    app.world()
        .iter_entities()
        .find_map(|entity| {
            let matches = entity.get::<Text>().is_some_and(|t| t.0 == text);
            matches
                .then(|| entity.get::<ChildOf>().map(ChildOf::parent))
                .flatten()
        })
        .unwrap_or_else(|| panic!("no text node {text:?} with a parent"))
}

/// The demo pack's tagline renders uppercased directly under the wordmark
/// (`docs/title-and-menu.md` § Title layout §3/§8).
#[test]
fn the_title_renders_the_tagline_uppercased_under_the_wordmark() {
    let mut app = temp_title_app();
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut app);

    let name = app
        .world()
        .resource::<Runtime>()
        .runner
        .pack()
        .manifest
        .name
        .clone();
    let wordmark_block = text_parent(&app, &name);
    assert_eq!(
        child_texts(&app, wordmark_block),
        vec![name, "A DEMO PACK".to_string()],
        "the tagline is uppercased and sits directly under the wordmark"
    );
    let tracking = app.world().iter_entities().find_map(|entity| {
        let tagline = entity.get::<Text>().is_some_and(|t| t.0 == "A DEMO PACK");
        tagline
            .then(|| entity.get::<bevy::text::LetterSpacing>().copied())
            .flatten()
    });
    assert_eq!(
        tracking,
        Some(bevy::text::LetterSpacing::Px(
            crate::presentation::title::TAGLINE_TRACKING_PX
        )),
        "the tagline carries the spec's tracking"
    );
}

/// A 48-character tagline renders its full uppercased line (the warning boundary
/// in `docs/title-and-menu.md` § Title layout §8).
#[test]
fn a_forty_eight_character_tagline_renders_in_full() {
    let tagline = "x".repeat(48);
    let manifest = format!(
        "(\n    name: \"unmei demo\",\n    entry: \"first_meeting\",\n    tagline: \"{tagline}\",\n)\n"
    );
    let mut app = temp_title_app_with_manifest(Some(&manifest));
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut app);

    let texts = all_texts(&app);
    assert!(
        texts.iter().any(|text| text == &tagline.to_uppercase()),
        "the full 48-char line renders: {texts:?}"
    );
}

/// A pack with no tagline renders exactly today's title: the wordmark alone,
/// with no tagline line and no spacing shift (`docs/title-and-menu.md` § Title
/// layout §3/§8).
#[test]
fn a_pack_without_a_tagline_renders_no_tagline_line() {
    let mut app = temp_title_app_with_manifest(Some(
        "(\n    name: \"unmei demo\",\n    entry: \"first_meeting\",\n)\n",
    ));
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut app);

    let texts = all_texts(&app);
    assert!(
        texts.iter().all(|text| !text.contains("DEMO PACK")),
        "no tagline line renders: {texts:?}"
    );
    let name = app
        .world()
        .resource::<Runtime>()
        .runner
        .pack()
        .manifest
        .name
        .clone();
    assert_eq!(
        child_texts(&app, text_parent(&app, &name)),
        vec![name],
        "the wordmark sits alone"
    );
}

/// `{var}` interpolation is deliberately not performed on the tagline: the copy
/// renders literally, uppercased (`docs/content-format.md` §6).
#[test]
fn the_tagline_is_never_interpolated() {
    let mut app = temp_title_app_with_manifest(Some(
        "(\n    name: \"unmei demo\",\n    entry: \"first_meeting\",\n    tagline: \"hello {player}\",\n)\n",
    ));
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut app);

    let texts = all_texts(&app);
    assert!(
        texts.iter().any(|text| text == "HELLO {PLAYER}"),
        "the tagline renders literally: {texts:?}"
    );
}

// --- manifest title backdrop ---------------------------------------------

/// `cover_size` fills both axes, preserves the texture's aspect, and lets the
/// overflowing axis crop; a degenerate dimension falls back to the window.
#[test]
fn cover_size_fills_both_axes_and_preserves_aspect() {
    // A 4:3 image in a 16:9 window: width fills, height overflows.
    let cover =
        crate::presentation::style::cover_size(Vec2::new(1024.0, 768.0), Vec2::new(1280.0, 720.0));
    assert_eq!(cover, Vec2::new(1280.0, 960.0));
    assert!((cover.x / cover.y - 4.0 / 3.0).abs() < 1e-4);

    // A 16:9 image in a 4:3 window: height fills, width overflows.
    let cover =
        crate::presentation::style::cover_size(Vec2::new(1280.0, 720.0), Vec2::new(1024.0, 768.0));
    assert!((cover.x - 1365.3333).abs() < 0.01, "{cover:?}");
    assert!((cover.y - 768.0).abs() < 1e-4, "{cover:?}");
    assert!((cover.x / cover.y - 16.0 / 9.0).abs() < 1e-4);

    // A degenerate image or window never divides by zero.
    assert_eq!(
        crate::presentation::style::cover_size(Vec2::ZERO, Vec2::new(1280.0, 720.0)),
        Vec2::new(1280.0, 720.0)
    );
    assert_eq!(
        crate::presentation::style::cover_size(Vec2::new(1280.0, 720.0), Vec2::ZERO),
        Vec2::ZERO
    );
}

/// `contain_size` is `cover_size`'s inverse: both axes fit inside the box,
/// aspect is preserved and the free axis letterboxes; a degenerate dimension
/// falls back to the box unchanged.
#[test]
fn contain_size_fits_both_axes_and_preserves_aspect() {
    // A 16:9 image in a 4:3 box: width fits, height letterboxes.
    let box_size = Vec2::new(360.0, 270.0);
    let contain = crate::presentation::style::contain_size(Vec2::new(1280.0, 720.0), box_size);
    assert_eq!(contain, Vec2::new(360.0, 202.5));
    assert!((contain.x / contain.y - 16.0 / 9.0).abs() < 1e-4);
    assert!(
        contain.x <= box_size.x && contain.y <= box_size.y,
        "{contain:?}"
    );

    // A 1:2 sprite in the same box: height fits, width pillarboxes.
    let contain = crate::presentation::style::contain_size(Vec2::new(256.0, 512.0), box_size);
    assert_eq!(contain, Vec2::new(135.0, 270.0));
    assert!((contain.x / contain.y - 256.0 / 512.0).abs() < 1e-4);
    assert!(
        contain.x <= box_size.x && contain.y <= box_size.y,
        "{contain:?}"
    );

    // A degenerate image or box never divides by zero.
    assert_eq!(
        crate::presentation::style::contain_size(Vec2::ZERO, box_size),
        box_size
    );
    assert_eq!(
        crate::presentation::style::contain_size(Vec2::new(1280.0, 720.0), Vec2::ZERO),
        Vec2::ZERO
    );
}

/// `title_backdrop: "<id>"` draws `assets/backgrounds/<id>.png` full-window,
/// unmodified — no veil, tint or overlay — over the gradient underlay
/// (`docs/title-and-menu.md` § Title layout §2/§8).
#[test]
fn the_title_draws_the_declared_backdrop_covering_the_window() {
    // The demo pack declares `title_backdrop: "school_gate"`.
    let mut app = title_asset_app(None);
    pump_until_watch_drains(&mut app);

    let (image, node) = title_backdrop_node(&app).expect("the declared backdrop draws");
    assert_eq!(
        image.color,
        Color::WHITE,
        "the art is drawn verbatim — no veil, tint or overlay"
    );
    assert_eq!(image.image_mode, NodeImageMode::Stretch);
    // The stand-in loads as the loader's 1x1 image, so covering a 16:9 window
    // needs a 1280x1280 node cropped to the window's height; the general aspect
    // maths is pinned by `cover_size` above.
    assert_eq!(node.width, Val::Px(1280.0));
    assert_eq!(node.height, Val::Px(1280.0));
    assert_eq!(node.left, Val::Px(0.0));
    assert_eq!(node.top, Val::Px(-280.0));
    assert!(
        app.world()
            .iter_entities()
            .any(|entity| entity.get::<BackgroundGradient>().is_some()),
        "the neutral gradient underlay stays beneath the art"
    );
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "a loaded backdrop never notices"
    );

    // Re-entering the title rebuilds the tree and draws the same backdrop, once.
    // `continue_line` is part of `MenuBuilt`'s signature, so the change forces a
    // real rebuild; the rendered line proves it, and the handle proves the same
    // `backgrounds/<id>.png` resolved again.
    let before_handle = image.image.clone();
    app.world_mut().resource_mut::<MenuState>().continue_line = "re-entered".to_string();
    app.update();
    pump_until_watch_drains(&mut app);
    assert!(
        all_texts(&app).iter().any(|text| text == "re-entered"),
        "the title tree rebuilt"
    );
    let backdrops = app
        .world()
        .iter_entities()
        .filter(|entity| {
            entity
                .get::<crate::presentation::title::TitleBackdropImage>()
                .is_some()
        })
        .count();
    assert_eq!(backdrops, 1, "re-entry draws exactly one backdrop");
    let (after, _) = title_backdrop_node(&app).expect("the backdrop survives re-entry");
    assert_eq!(
        after.color,
        Color::WHITE,
        "the rebuilt art is still unmodified"
    );
    assert_eq!(
        after.image, before_handle,
        "re-entry resolves the same backdrop asset"
    );
}

/// A pack with neither `tagline` nor `title_backdrop` renders exactly today's
/// title: the gradient and no backdrop node
/// (`docs/title-and-menu.md` § Title layout §8).
#[test]
fn a_pack_without_a_title_backdrop_draws_only_the_gradient() {
    let mut app = title_asset_app(Some(
        "(\n    name: \"unmei demo\",\n    entry: \"first_meeting\",\n)\n",
    ));
    pump_until_watch_drains(&mut app);

    assert!(
        title_backdrop_node(&app).is_none(),
        "no backdrop image node is drawn"
    );
    assert!(
        app.world()
            .iter_entities()
            .any(|entity| entity.get::<BackgroundGradient>().is_some()),
        "the gradient remains the whole backdrop"
    );
}

/// Settings opened from the title keeps the same backdrop
/// (`docs/title-and-menu.md` § Title layout §5).
#[test]
fn title_settings_keeps_the_same_backdrop() {
    let mut app = title_asset_app(None);
    pump_until_watch_drains(&mut app);
    {
        let mut state = app.world_mut().resource_mut::<MenuState>();
        state.view = MenuView::Settings;
        state.settings_origin = menu::MenuOrigin::Title;
    }
    app.world_mut()
        .run_system_once(crate::presentation::sync_menu_ui)
        .expect("sync_menu_ui runs");

    assert!(
        title_backdrop_node(&app).is_some(),
        "the title door's Settings keeps the backdrop"
    );
}

/// A declared backdrop whose file is present but undecodable is the same silent
/// degradation as a missing one: one `warn!`, the gradient stands, no Notice
/// (`docs/runtime-hardening.md` §2 row 25).
#[test]
fn a_declared_but_undecodable_backdrop_warns_once_and_draws_the_gradient() {
    captures::install();
    // The loader reads the file and rejects empty bytes, the decoder-failure
    // shape of the same `LoadState::Failed` arm a corrupt PNG takes.
    let mut app = title_asset_app_with_assets(
        Some(
            "(\n    name: \"unmei demo\",\n    entry: \"first_meeting\",\n    title_backdrop: \"broken_art\",\n)\n",
        ),
        &[("backgrounds/broken_art.png", b"")],
    );
    force_single_threaded(&mut app);
    captures::take();
    pump_until_watch_drains(&mut app);

    let records = failure_lines();
    assert_eq!(
        records.len(),
        1,
        "one warn for the undecodable path: {records:?}"
    );
    assert_eq!(records[0].0, log::Level::Warn);
    assert!(
        records[0].2.starts_with("couldn't load the title backdrop")
            && records[0].2.contains("backgrounds/broken_art.png"),
        "row 25 names the undecodable path: {:?}",
        records[0].2
    );
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "a silent degradation never notices"
    );
    assert!(
        app.world()
            .iter_entities()
            .any(|entity| entity.get::<BackgroundGradient>().is_some()),
        "the gradient is the fallback"
    );
}

/// A declared backdrop whose file is missing is a silent degradation: one
/// `warn!`, the gradient stands, no Notice, and no duplicate on re-entry
/// (`docs/runtime-hardening.md` §2 row 25).
#[test]
fn a_declared_but_missing_backdrop_warns_once_and_draws_the_gradient() {
    captures::install();
    let mut app = title_asset_app(Some(
        "(\n    name: \"unmei demo\",\n    entry: \"first_meeting\",\n    title_backdrop: \"missing_art\",\n)\n",
    ));
    force_single_threaded(&mut app);
    captures::take();
    pump_until_watch_drains(&mut app);

    let records = failure_lines();
    assert_eq!(
        records.len(),
        1,
        "one warn for the failed path: {records:?}"
    );
    assert_eq!(records[0].0, log::Level::Warn);
    assert!(
        records[0].2.starts_with("couldn't load the title backdrop"),
        "row 25's short string prefixes the line: {:?}",
        records[0].2
    );
    assert!(
        records[0].2.contains("backgrounds/missing_art.png"),
        "the line names the path: {:?}",
        records[0].2
    );
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "a silent degradation never notices"
    );
    assert!(
        app.world()
            .iter_entities()
            .any(|entity| entity.get::<BackgroundGradient>().is_some()),
        "the gradient is the fallback"
    );

    // Re-entry rebuilds the title; the path is already reported, so no second
    // warn (`AssetWatch`'s one-per-path rule).
    captures::take();
    app.world_mut().resource_mut::<MenuState>().continue_line = "a changed reason line".to_string();
    app.update();
    pump_until_watch_drains(&mut app);
    assert!(
        failure_lines().is_empty(),
        "a re-entered title must not warn twice"
    );
}

/// Continue's reason line is muted while Continue is enabled and warm-warn
/// while it explains a disabled Continue (`docs/title-and-menu.md` § Title
/// layout §4, §8).
#[test]
fn the_reason_line_tone_follows_continue_enabled() {
    let mut disabled = temp_title_app();
    disabled.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut disabled);
    assert!(!menu(&disabled).continue_enabled);
    assert_eq!(
        title_reason_color(&disabled),
        Color::srgb(0.86, 0.63, 0.40),
        "a disabled Continue's reason line is warm"
    );

    let pack = pack_with_a_stamped_autosave();
    let mut enabled = boot_app(pack.path().join("pack"), None);
    assert!(menu(&enabled).continue_enabled);
    enabled.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut enabled);
    assert_eq!(
        title_reason_color(&enabled),
        Color::srgba(0.80, 0.80, 0.85, 0.62),
        "an enabled Continue's reason line is muted"
    );
}

/// The four title states lay out with no scroll container and the spec's column
/// width. The project ships no font, so the fit bullet
/// (`docs/title-and-menu.md` § Title layout §1, §8) is asserted structurally:
/// nothing in the title can scroll.
#[test]
fn the_title_fits_without_a_scroll_container_in_every_state() {
    assert_title_never_scrolls(&mut temp_title_app());

    let pack = pack_with_a_stamped_autosave();
    let mut loadable = boot_app(pack.path().join("pack"), None);
    assert!(menu(&loadable).continue_enabled);
    assert_title_never_scrolls(&mut loadable);

    let mut live = temp_title_app();
    enter_play(&mut live);
    reenter_title(&mut live);
    assert!(menu(&live).continue_enabled);
    assert_title_never_scrolls(&mut live);

    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let path = {
        let loaded = load_pack(&pack_root).expect("pack loads");
        save::save_path_for(&loaded.pack, Slot::Autosave)
    };
    std::fs::write(&path, b"not a save").unwrap();
    let mut unreadable = boot_app(pack_root, None);
    assert!(!menu(&unreadable).continue_enabled);
    assert_title_never_scrolls(&mut unreadable);
}

#[test]
fn the_title_walk_keeps_its_five_rows() {
    let mut app = temp_title_app();
    assert_eq!(menu(&app).focus, MenuFocus::TitleNewGame);

    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::TitleSettings);
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::TitleGallery);
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::TitleQuit);
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::TitleContinue);
}

/// The title's `Gallery` row is enabled and caption-free; activating it opens
/// the gallery with the title as origin, moving nothing
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §1/§3).
#[test]
fn the_titles_gallery_row_opens_the_gallery_with_the_title_as_origin() {
    let mut app = temp_title_app();
    press_and_update(&mut app, KeyCode::ArrowDown);
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::TitleGallery);
    let before = parked_position(&app);

    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).view, MenuView::Gallery);
    assert_eq!(menu(&app).gallery_origin, menu::MenuOrigin::Title);
    assert_eq!(parked_position(&app), before, "opening changes nothing");

    // The back control returns to the title.
    click(&mut app, MenuAction::GalleryBack);
    assert_eq!(menu(&app).view, MenuView::Title);
    assert!(menu(&app).title_up());
    assert_eq!(parked_position(&app), before);
}

/// A minimal presentation app over the demo pack with the audio/visual sync
/// systems, so the title's silence and suppression can be driven headless.
fn chrome_app(staging: StagingState) -> App {
    // The interface faces are embedded, so `sync_wait_ui` can build the
    // dialogue spans; see `attach_text_faces`.
    let loaded = load_pack(&demo_pack_root()).expect("demo pack loads");
    let runtime = Runtime {
        runner: ScriptRunner::new(loaded.pack),
        staging,
        pack_root: demo_pack_root(),
        asset_root: demo_pack_root().join("assets"),
    };
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(AssetPlugin {
            file_path: demo_pack_root()
                .join("assets")
                .to_string_lossy()
                .into_owned(),
            ..default()
        })
        .init_asset::<Image>()
        .init_asset::<bevy_kira_audio::AudioSource>()
        .insert_resource(runtime)
        .init_resource::<MenuState>()
        .init_resource::<Notice>()
        .init_resource::<DefensiveHold>()
        .init_resource::<AssetWatch>()
        .init_resource::<AppliedVisuals>()
        .init_resource::<AppliedAudio>()
        .init_resource::<AppliedUi>()
        .init_resource::<TextReveal>()
        .insert_resource(AudioChannel::<Bgm>::default());
    attach_text_faces(&mut app);
    app.world_mut()
        .spawn((DialogueText, Text::new("a stale line")));
    app.world_mut()
        .spawn((HelpLine, Text::new("a stale help line")));
    app
}

fn raise_title(app: &mut App) {
    let mut menu_state = app.world_mut().resource_mut::<MenuState>();
    menu_state.open = true;
    menu_state.view = MenuView::Title;
}

fn lower_title(app: &mut App) {
    let mut menu_state = app.world_mut().resource_mut::<MenuState>();
    menu_state.open = false;
    menu_state.view = MenuView::Slots;
}

#[test]
fn the_title_is_silent_until_play_is_entered() {
    let mut app = chrome_app(StagingState {
        bgm: Some("music_morning".into()),
        ..default()
    });
    raise_title(&mut app);
    app.world_mut()
        .run_system_once(crate::presentation::play::sync_audio)
        .expect("sync_audio runs");
    assert!(
        app.world()
            .resource::<AppliedAudio>()
            .current_bgm()
            .is_none(),
        "the title stages music but never sounds it"
    );

    lower_title(&mut app);
    app.world_mut()
        .run_system_once(crate::presentation::play::sync_audio)
        .expect("sync_audio runs");
    assert_eq!(
        app.world().resource::<AppliedAudio>().current_bgm(),
        Some("music_morning"),
        "entering play starts the entry scene's track from the top"
    );
}

/// The title's Settings and gallery doors keep the title's silence: the play
/// path stays silent behind them too (`docs/title-and-menu.md` § Gallery
/// `### Behavior` §6).
#[test]
fn the_titles_sub_views_keep_the_title_silent() {
    let mut app = chrome_app(StagingState {
        bgm: Some("music_morning".into()),
        ..default()
    });
    raise_title(&mut app);

    for (view, settings_origin, gallery_origin) in [
        (
            MenuView::Settings,
            menu::MenuOrigin::Title,
            menu::MenuOrigin::Slots,
        ),
        (
            MenuView::Gallery,
            menu::MenuOrigin::Slots,
            menu::MenuOrigin::Title,
        ),
    ] {
        {
            let mut menu_state = app.world_mut().resource_mut::<MenuState>();
            menu_state.view = view;
            menu_state.settings_origin = settings_origin;
            menu_state.gallery_origin = gallery_origin;
        }
        app.world_mut()
            .run_system_once(crate::presentation::play::sync_audio)
            .expect("sync_audio runs");
        assert!(
            app.world()
                .resource::<AppliedAudio>()
                .current_bgm()
                .is_none(),
            "{view:?} from the title stays silent"
        );
    }

    // The overlay's own door sounds the staged track as before.
    {
        let mut menu_state = app.world_mut().resource_mut::<MenuState>();
        menu_state.view = MenuView::Slots;
        menu_state.settings_origin = menu::MenuOrigin::Slots;
    }
    app.world_mut()
        .run_system_once(crate::presentation::play::sync_audio)
        .expect("sync_audio runs");
    assert_eq!(
        app.world().resource::<AppliedAudio>().current_bgm(),
        Some("music_morning"),
        "the overlay's door sounds the staged track"
    );
}

#[test]
fn no_play_frame_is_drawn_while_the_title_is_up() {
    let mut app = chrome_app(StagingState {
        background: Some("school_gate".into()),
        sprites: [("ace".to_string(), "neutral".to_string())].into(),
        bgm: Some("music_morning".into()),
    });

    app.world_mut()
        .run_system_once(crate::presentation::play::sync_staging_visuals)
        .unwrap();
    app.world_mut()
        .run_system_once(crate::presentation::play::sync_wait_ui)
        .unwrap();
    let sprites = |app: &App| {
        app.world()
            .iter_entities()
            .filter(|entity| {
                entity
                    .get::<crate::presentation::play::BackgroundSprite>()
                    .is_some()
                    || entity
                        .get::<crate::presentation::play::NpcSprite>()
                        .is_some()
            })
            .count()
    };
    assert!(sprites(&app) > 0, "play draws its sprites");

    raise_title(&mut app);
    app.world_mut()
        .run_system_once(crate::presentation::play::sync_staging_visuals)
        .unwrap();
    app.world_mut()
        .run_system_once(crate::presentation::play::sync_wait_ui)
        .unwrap();
    assert_eq!(sprites(&app), 0, "no sprite is drawn behind the title");
    let texts = all_texts(&app);
    assert!(
        !texts.iter().any(|text| text == "a stale line"),
        "the dialogue box is cleared"
    );
    assert!(
        !texts.iter().any(|text| text == "a stale help line"),
        "the in-play help line is cleared"
    );
}

#[test]
fn a_broken_pack_never_shows_the_title() {
    let app = app_for_broken_pack();
    assert!(
        !menu(&app).open,
        "the title never appears for a broken pack"
    );
    assert!(!menu(&app).title_up());
    assert!(app.world().get_resource::<Runtime>().is_none());
}

#[test]
fn pointer_and_keyboard_agree_on_the_title_rows() {
    let mut app = temp_title_app();
    click(&mut app, MenuAction::Gallery);
    assert_eq!(menu(&app).view, MenuView::Gallery);
    click(&mut app, MenuAction::GalleryBack);
    assert_eq!(menu(&app).view, MenuView::Title);
    click(&mut app, MenuAction::NewGame);
    assert!(!menu(&app).open, "New Game's click starts play");
}

#[test]
fn the_title_freezes_the_reveal_and_the_auto_dwell() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    let _node = spawn_dialogue(&mut app);
    set_text_speed(&mut app, 0.04);
    enable_auto(&mut app);
    render_dialogue(&mut app);

    {
        let mut menu_state = app.world_mut().resource_mut::<MenuState>();
        menu_state.open = true;
        menu_state.view = MenuView::Title;
    }
    let chars = app.world().resource::<TextReveal>().chars_shown(0.04);
    let parked = wait(&app);
    advance_time(&mut app, 5.0);
    assert_eq!(
        app.world().resource::<TextReveal>().chars_shown(0.04),
        chars,
        "the reveal is frozen while the title is up"
    );
    tick_update(&mut app);
    assert_eq!(
        wait(&app),
        parked,
        "the auto dwell never accumulates on the title"
    );
}

#[test]
fn leaving_the_titles_settings_flushes_a_dirty_file() {
    let pack = temp_pack();
    let mut app = boot_app(pack.path().join("pack"), None);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleSettings;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).view, MenuView::Settings);

    click(&mut app, MenuAction::AudioMute);
    assert!(app.world().resource::<SettingsState>().muted());
    assert!(!settings_path(&app).exists(), "opening writes nothing");

    esc(&mut app); // back out to the title
    assert_eq!(menu(&app).view, MenuView::Title);
    assert!(settings_path(&app).exists(), "leaving flushes the file");
    assert!(audio(&app).muted);
}

#[test]
fn the_title_copy_survives_the_play_ui_writer() {
    let mut app = temp_title_app();
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut app);
    let name = app
        .world()
        .resource::<Runtime>()
        .runner
        .pack()
        .manifest
        .name
        .clone();

    // The per-frame play-UI writer runs after the title is built; it must not
    // blank the title's own text nodes.
    app.world_mut()
        .run_system_once(crate::presentation::play::sync_wait_ui)
        .expect("sync_wait_ui runs");
    let texts = all_texts(&app);
    assert!(texts.contains(&name), "the wordmark survives: {texts:?}");
    assert!(
        texts.iter().any(|text| text == menu::TITLE_LEGEND),
        "the legend survives"
    );
    assert_eq!(
        title_label(&app, MenuAction::Gallery).as_deref(),
        Some("Gallery"),
        "Gallery's caption-free row survives"
    );
}

#[test]
fn continue_loads_the_autosave_through_the_existing_load_path() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let (saved_wait, saved_staging) = {
        let mut app = app_for_pack(pack_root.clone());
        press_and_update(&mut app, KeyCode::Space);
        let runtime = app.world().resource::<Runtime>();
        let wait = runtime.runner.wait().cloned();
        let staging = runtime.staging.clone();
        crate::slots::write_stamped(&runtime.runner, &runtime.staging, Slot::Autosave).unwrap();
        (wait, staging)
    };

    let mut app = boot_app(pack_root, None);
    assert!(menu(&app).continue_enabled, "the autosave enables Continue");
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
    press_and_update(&mut app, KeyCode::Enter);

    assert!(!menu(&app).open, "Continue entered play");
    assert!(live_session(&app));
    let runtime = app.world().resource::<Runtime>();
    assert_eq!(runtime.runner.wait().cloned(), saved_wait);
    assert_eq!(runtime.staging, saved_staging);
}

#[test]
fn an_answer_arriving_while_the_title_is_up_is_held_until_continue() {
    let fake = FakeResolver::new(model_answer("warm"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    reenter_title(&mut app);

    fake.release();
    pump_until(&mut app, held);
    assert!(held(&app), "the answer is held while the title is up");

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
    press_and_update(&mut app, KeyCode::Enter);
    assert!(!menu(&app).open, "Continue resumed the live session");
    assert!(!held(&app), "the held answer landed on resume");
    assert!(parked_on_say(&app));
}

#[test]
fn continue_resumes_a_mid_reveal_line_where_it_stopped() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    let _node = spawn_dialogue(&mut app);
    set_text_speed(&mut app, 0.04);
    render_dialogue(&mut app);
    advance_time(&mut app, 0.5);
    let mid = app.world().resource::<TextReveal>().chars_shown(0.04);
    assert!(mid > 0, "the line is mid-reveal");
    assert!(!app.world().resource::<TextReveal>().is_complete(0.04));

    reenter_title(&mut app);
    assert!(menu(&app).title_up());
    // Time spent on the title does not advance the reveal clock.
    advance_time(&mut app, 5.0);
    assert_eq!(
        app.world().resource::<TextReveal>().chars_shown(0.04),
        mid,
        "the time on the title leaves the reveal clock alone"
    );
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
    press_and_update(&mut app, KeyCode::Enter);
    assert!(!menu(&app).open, "Continue resumed the live session");

    render_dialogue(&mut app);
    assert_eq!(
        app.world().resource::<TextReveal>().chars_shown(0.04),
        mid,
        "the reveal clock resumes where it stopped"
    );
}

/// A live Continue is the Esc resume one level up: the auto dwell resumes with
/// exactly the time it had left, and skip is disarmed on entry so preserving
/// the dwell cannot become an unexpected fast-forward. The
/// remainder is anchored to real ticks, so a reset to the full `2.0 s` fails
/// this test — the defect this repair removes.
#[test]
fn live_continue_resumes_the_remaining_auto_dwell_and_disarms_skip() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    // Arm a real (non-instant) reveal and let it finish, so the reveal clock is
    // a live, observable value the resumed Continue has to preserve.
    set_text_speed(&mut app, 0.04);
    let _node = spawn_dialogue(&mut app);
    render_dialogue(&mut app);
    enable_auto(&mut app);
    advance_time(&mut app, 3.0); // finish the reveal
    tick_update(&mut app); // start the dwell
    assert!(
        app.world().resource::<TextReveal>().is_complete(0.04),
        "the entry reveal is finished before the dwell"
    );
    let reveal = app.world().resource::<TextReveal>().chars_shown(0.04);
    assert!(reveal > 0, "the reveal clock is armed");
    advance_time(&mut app, 1.5);
    tick_update(&mut app); // ~0.5 s left
    let line = wait(&app);
    assert!(line.is_some(), "the say is parked");

    // A skip mode engaged in play, then the title: it must not survive into the
    // resumed session, or the preserved dwell would be skipped over instead.
    app.world_mut()
        .resource_mut::<crate::core::SkipState>()
        .engage();
    assert!(skip_on(&app));

    reenter_title(&mut app);
    assert!(menu(&app).title_up());
    for _ in 0..3 {
        advance_time(&mut app, 2.0);
        tick_update(&mut app);
    }
    assert_eq!(wait(&app), line, "no dwell fires while the title is up");

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
    press_and_update(&mut app, KeyCode::Enter);
    assert!(!menu(&app).open, "Continue resumed the live session");
    assert!(!skip_on(&app), "title→play disarms skip");
    assert_eq!(wait(&app), line, "the park is unchanged by Continue");
    assert_eq!(
        app.world().resource::<TextReveal>().chars_shown(0.04),
        reveal,
        "the reveal clock is preserved by Continue"
    );

    advance_time(&mut app, 0.4);
    tick_update(&mut app);
    assert_eq!(
        wait(&app),
        line,
        "the dwell resumes from the remaining ~0.5 s, not a fresh 2.0 s"
    );
    advance_time(&mut app, 0.3);
    tick_update(&mut app);
    assert_ne!(wait(&app), line, "the remaining dwell completes");
}

/// New Game is a position replacement, so it resets the auto dwell even when it
/// lands back on the very park the dwell was counting on: the entry line gets
/// the full `2.0 s`, not the remainder the discarded session had left — New
/// Game's discontinuity-reset protection.
#[test]
fn new_game_resets_the_auto_dwell_on_the_same_park() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    enable_auto(&mut app);
    tick_update(&mut app); // start the dwell on the entry say
    advance_time(&mut app, 1.5);
    tick_update(&mut app); // ~0.5 s left
    let entry = wait(&app);
    assert!(entry.is_some(), "the entry say is parked");

    reenter_title(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleNewGame;
    press_and_update(&mut app, KeyCode::Enter); // confirm: a live session is lost
    click(&mut app, MenuAction::ConfirmAccept);
    assert!(!menu(&app).open, "New Game starts play");
    assert_eq!(wait(&app), entry, "New Game lands on the same entry park");

    advance_time(&mut app, 1.9);
    tick_update(&mut app);
    assert_eq!(
        wait(&app),
        entry,
        "the discarded dwell's remainder does not carry into New Game"
    );
    advance_time(&mut app, 0.2);
    tick_update(&mut app);
    assert_ne!(wait(&app), entry, "the entry line gets its full 2.0 s");
}

/// Force the documented stale-acceptance case through ordinary production
/// operations: a real `read` of `slot`, then a public-field edit the read
/// cannot see, then the menu's own completion. The save is genuinely refused at
/// restoration, not at read, so the branch under test is the real completion
/// decision.
fn force_restore_refusal(
    app: &mut App,
    slot: Slot,
    origin: LoadOrigin,
    surface: LoadSurface,
) -> LoadOutcome {
    app.world_mut()
        .run_system_once(
            move |mut menu: ResMut<MenuState>, mut session: SessionInput| {
                let ReadOutcome::Save { mut save, path } = session.transitions().read(slot, origin)
                else {
                    panic!("the fixture slot must read successfully, so the refusal is at restore");
                };
                save.scene_id = "nowhere".to_string();
                crate::menu::complete_load(
                    &mut menu,
                    &mut session,
                    ReadOutcome::Save { save, path },
                    surface,
                )
            },
        )
        .expect("the one-shot load-completion system runs")
}

/// A fresh boot has no live session, so this is the launch-entry marker the
/// refusal must not set: a load genuinely refused at restoration leaves the
/// title up and does not enter play. The read succeeds on a
/// real autosave; only the already-read value's restore is refused.
#[test]
fn a_forced_restore_refusal_at_the_title_leaves_no_launch_entry_marker() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    {
        let mut app = app_for_pack(pack_root.clone());
        press_and_update(&mut app, KeyCode::Space);
        let runtime = app.world().resource::<Runtime>();
        crate::slots::write_stamped(&runtime.runner, &runtime.staging, Slot::Autosave).unwrap();
    }

    let mut app = boot_app(pack_root, None);
    assert!(!live_session(&app), "a fresh boot has no live session");
    assert!(menu(&app).continue_enabled, "the autosave is loadable");
    let before = parked_position(&app);
    let staging = app.world().resource::<Runtime>().staging.clone();
    let replay_before = (
        app.world().resource::<AppliedAudio>().is_forced(),
        app.world().resource::<AppliedVisuals>().is_forced(),
    );
    assert_eq!(
        replay_before,
        (false, false),
        "the fixture starts with no pending replay, so the comparison is meaningful"
    );

    let outcome = force_restore_refusal(
        &mut app,
        Slot::Autosave,
        LoadOrigin::Menu,
        LoadSurface::Title,
    );

    assert_eq!(
        outcome,
        LoadOutcome::Refused,
        "the restoration refused, not the read"
    );
    assert!(menu(&app).title_up(), "the title stays up");
    assert!(
        !live_session(&app),
        "a refused load cannot acquire the live-session entry marker"
    );
    assert_eq!(parked_position(&app), before, "the boot position survives");
    assert_eq!(
        app.world().resource::<Runtime>().staging,
        staging,
        "the boot staging survives"
    );
    assert_eq!(
        app.world().resource::<Notice>().text.as_deref(),
        Some("couldn't load that save"),
        "the existing load Notice fires"
    );
    assert!(
        !on_hold(&app),
        "row 17 is continue-with-message, not the defensive hold"
    );
    assert_eq!(
        (
            app.world().resource::<AppliedAudio>().is_forced(),
            app.world().resource::<AppliedVisuals>().is_forced(),
        ),
        replay_before,
        "a refusal requests no presentation replay"
    );
}

/// The restore-stage refusal is defensive — the read already ran the semantic
/// checks — and unreachable through file IO by construction, so this forces it
/// at the production seam: a real read, an edit the read cannot see, then the
/// menu's own completion. It witnesses the refusal outcome, its row-17
/// reporting and the no-mutation contract: the old runner, staging and
/// pending/held session survive intact, the title stays up, and no
/// successful-replacement cleanup runs.
#[test]
fn a_forced_restore_refusal_cannot_close_the_menu_or_enter_play() {
    let fake = FakeResolver::new(model_answer("warm"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    reenter_title(&mut app);

    // The in-flight answer lands while the title is up: it is held, not
    // applied, so a refused load owes it back untouched.
    fake.release();
    pump_until(&mut app, held);
    assert!(held(&app), "the answer is held behind the title");

    // A real, loadable autosave so the read accepts the slot.
    {
        let runtime = app.world().resource::<Runtime>();
        crate::slots::write_stamped(&runtime.runner, &runtime.staging, Slot::Autosave).unwrap();
    }

    let before = parked_position(&app);
    let staging = app.world().resource::<Runtime>().staging.clone();

    let outcome = force_restore_refusal(
        &mut app,
        Slot::Autosave,
        LoadOrigin::Menu,
        LoadSurface::Title,
    );

    assert_eq!(outcome, LoadOutcome::Refused, "the refusal is reported");
    assert!(menu(&app).title_up(), "the refusal keeps the title up");
    assert!(live_session(&app), "the live session is untouched");
    assert_eq!(parked_position(&app), before, "the old runner survives");
    assert_eq!(
        app.world().resource::<Runtime>().staging,
        staging,
        "the old staging survives"
    );
    assert!(
        app.world().resource::<PendingDecision>().0.is_none(),
        "the pending slot is exactly as it was"
    );
    assert!(held(&app), "the held answer survives");
    assert_eq!(
        app.world().resource::<Notice>().text.as_deref(),
        Some("couldn't load that save"),
        "the existing load Notice fires"
    );
    assert!(
        !on_hold(&app),
        "row 17 is continue-with-message, not the defensive hold"
    );
}

/// The overlay's own completion: a restore refused under the load grid keeps
/// the overlay on that surface and the old runner/staging intact.
#[test]
fn a_forced_restore_refusal_keeps_the_overlay_open() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    force_single_threaded(&mut app);

    // A real manual slot, so the grid's card is actionable and the read accepts
    // it; only the already-read restore is refused.
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::F5);
    esc(&mut app);
    switch_mode(&mut app, MenuMode::Load);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::Card(0);
    assert!(menu(&app).open, "the overlay is open on the load grid");

    let before = parked_position(&app);
    let staging = app.world().resource::<Runtime>().staging.clone();

    let outcome = force_restore_refusal(
        &mut app,
        Slot::Manual(1),
        LoadOrigin::Menu,
        LoadSurface::Overlay,
    );

    assert_eq!(outcome, LoadOutcome::Refused, "the refusal is reported");
    assert!(menu(&app).open, "a refused restore leaves the overlay open");
    assert_eq!(
        menu(&app).view,
        MenuView::Slots,
        "the overlay stays on the slot grid"
    );
    assert_eq!(parked_position(&app), before, "the old runner survives");
    assert_eq!(
        app.world().resource::<Runtime>().staging,
        staging,
        "the old staging survives"
    );
    assert_eq!(
        app.world().resource::<Notice>().text.as_deref(),
        Some("couldn't load that save"),
        "the existing load Notice fires"
    );
}

#[test]
fn a_refused_continue_load_refreshes_the_titles_metadata() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    {
        let mut app = app_for_pack(pack_root.clone());
        press_and_update(&mut app, KeyCode::Space);
        let runtime = app.world().resource::<Runtime>();
        crate::slots::write_stamped(&runtime.runner, &runtime.staging, Slot::Autosave).unwrap();
    }

    let mut app = boot_app(pack_root, None);
    assert!(menu(&app).continue_enabled);
    // The autosave turns unreadable after the metadata read.
    let path = save::save_path_for(
        app.world().resource::<Runtime>().runner.pack(),
        Slot::Autosave,
    );
    std::fs::write(&path, b"not a save").unwrap();

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
    press_and_update(&mut app, KeyCode::Enter);

    assert!(menu(&app).title_up(), "a refused load stays on the title");
    assert_eq!(
        app.world().resource::<Notice>().text.as_deref(),
        Some("couldn't load that save"),
        "the refusal raises the existing load Notice"
    );
    assert!(
        !menu(&app).continue_enabled,
        "the refreshed metadata flips Continue to disabled"
    );
    assert_ne!(menu(&app).continue_line, "No autosave yet");
}

// ---------------------------------------------------------------------------
// Continue's autosave path and its reason line
// ---------------------------------------------------------------------------

/// A temp pack carrying a loadable, app-stamped autosave at the second park,
/// so every Continue-autosave test starts from the same fixture.
fn pack_with_a_stamped_autosave() -> tempfile::TempDir {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    press_and_update(&mut app, KeyCode::Space);
    let runtime = app.world().resource::<Runtime>();
    crate::slots::write_stamped(&runtime.runner, &runtime.staging, Slot::Autosave).unwrap();
    pack
}

/// An unreadable autosave disables Continue with the engine's own
/// `SlotInfo::Unreadable { reason }` string, verbatim — one source of truth with
/// the overlay's card.
#[test]
fn continue_disables_with_the_engines_unreadable_reason_verbatim() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let path = {
        let loaded = load_pack(&pack_root).expect("pack loads");
        save::save_path_for(&loaded.pack, Slot::Autosave)
    };
    std::fs::write(&path, b"not a save").unwrap();

    let app = boot_app(pack_root, None);
    let state = menu(&app);
    let info = state
        .slots
        .get(menu::slot_index(Slot::Autosave))
        .expect("the autosave is listed");
    let SlotStatus::Unreadable { reason } = &info.status else {
        panic!("expected an unreadable autosave, got {:?}", info.status);
    };
    assert!(
        !state.continue_enabled,
        "an unreadable autosave disables Continue"
    );
    assert_eq!(
        &state.continue_line, reason,
        "the engine's reason, verbatim"
    );
    // The overlay's card shows the very same string.
    assert_eq!(
        &menu::card_model(info, MenuMode::Load).detail,
        reason,
        "identical to the overlay's card"
    );
    assert_eq!(
        state.focus,
        MenuFocus::TitleNewGame,
        "focus falls to New Game"
    );
}

/// A live session has no `saved_at`, so the line is the runner's current scene
/// id alone — even when a loadable autosave sits behind it.
#[test]
fn continue_line_names_the_live_sessions_scene_alone() {
    let mut app = temp_title_app();
    enter_play(&mut app);
    {
        let runtime = app.world().resource::<Runtime>();
        crate::slots::write_stamped(&runtime.runner, &runtime.staging, Slot::Autosave).unwrap();
    }
    reenter_title(&mut app);

    let scene = app
        .world()
        .resource::<Runtime>()
        .runner
        .scene_id()
        .to_string();
    assert!(menu(&app).continue_enabled);
    assert_eq!(
        menu(&app).continue_line,
        scene,
        "a live session names the scene id alone"
    );
    assert!(
        !menu(&app).continue_line.contains('\u{00b7}'),
        "a live session has no saved_at, so there is no separator"
    );
}

/// A loadable autosave renders the scene id plus the app-formatted local time
/// from `SlotInfo::Ok`, matching the overlay's card; `saved_at: None` renders
/// the dash rather than a fabricated time.
#[test]
fn continue_line_appends_the_autosaves_formatted_time_and_a_dash_when_unstamped() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let (scene_id, captured) = {
        let mut app = app_for_pack(pack_root.clone());
        press_and_update(&mut app, KeyCode::Space);
        let runtime = app.world().resource::<Runtime>();
        (
            runtime.runner.scene_id().to_string(),
            SaveFile::capture(&runtime.runner, &runtime.staging),
        )
    };

    // The engine's `capture` never stamps, so this autosave has no `saved_at`.
    {
        let loaded = load_pack(&pack_root).expect("pack loads");
        save::write_slot(&loaded.pack, Slot::Autosave, &captured).unwrap();
    }
    let app = boot_app(pack_root.clone(), None);
    assert_eq!(
        menu(&app).continue_line,
        format!("{scene_id} \u{00b7} \u{2014}"),
        "no saved_at renders the dash"
    );
    assert!(menu(&app).continue_enabled);

    // A stamped autosave renders the app-formatted local time.
    let stamped_at = 1_760_000_000;
    {
        let loaded = load_pack(&pack_root).expect("pack loads");
        let mut stamped = captured.clone();
        stamped.saved_at = Some(stamped_at);
        save::write_slot(&loaded.pack, Slot::Autosave, &stamped).unwrap();
    }
    let app = boot_app(pack_root, None);
    let formatted = menu::format_saved_at(Some(stamped_at));
    assert_eq!(
        menu(&app).continue_line,
        format!("{scene_id} \u{00b7} {formatted}"),
        "the scene id plus the app-formatted local time"
    );
    // The same fields and formatting as the overlay's card.
    let info = &menu(&app).slots[menu::slot_index(Slot::Autosave)];
    assert_eq!(
        menu::card_model(info, MenuMode::Load).saved_at,
        formatted,
        "Continue and the card format the time identically"
    );
}

/// Continue's autosave path onto a decision-wait re-fires the `decide` line.
#[test]
fn continue_onto_a_decision_wait_refires_the_decide_line() {
    let dir = tempfile::tempdir().unwrap();
    let pack_root = dir.path().join("pack");
    copy_dir(&demo_pack_root(), &pack_root);

    // Park on the decision in a throwaway run and capture the autosave there.
    {
        let mut app = app_with_decisions_at(pack_root.clone(), None);
        {
            let mut runtime = app.world_mut().resource_mut::<Runtime>();
            drive_runner_to_decision(&mut runtime);
        }
        let runtime = app.world().resource::<Runtime>();
        let save = SaveFile::capture(&runtime.runner, &runtime.staging);
        save::write_slot(runtime.runner.pack(), Slot::Autosave, &save).unwrap();
    }

    let fake = FakeResolver::new(model_answer("warm"));
    let mut app = boot_app(pack_root, Some(Arc::new(fake.clone())));
    assert!(
        menu(&app).continue_enabled,
        "the decision-wait autosave enables Continue"
    );
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
    press_and_update(&mut app, KeyCode::Enter);

    assert!(!menu(&app).open, "Continue entered play");
    assert!(live_session(&app));
    assert!(matches!(
        app.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Decision { .. })
    ));
    pump_until(&mut app, |_| fake.calls() == 1);
    assert_eq!(fake.calls(), 1, "the restored decision re-fired");
}

/// Slot metadata is read once per title entry: a slot written behind the app
/// after the title is up does not change the line until the title is re-entered.
#[test]
fn the_title_reads_slot_metadata_once_per_entry() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = boot_app(pack_root.clone(), None);
    assert_eq!(menu(&app).continue_line, "No autosave yet");
    assert!(!menu(&app).continue_enabled);

    // A loadable autosave appears behind the app after the title is up.
    let mut save = {
        let loaded = load_pack(&pack_root).expect("pack loads");
        let mut runner = ScriptRunner::new(loaded.pack);
        runner.run().unwrap();
        SaveFile::capture(&runner, &StagingState::default())
    };
    save.saved_at = Some(1_760_000_000);
    {
        let loaded = load_pack(&pack_root).expect("pack loads");
        save::write_slot(&loaded.pack, Slot::Autosave, &save).unwrap();
    }

    app.update();
    assert_eq!(
        menu(&app).continue_line,
        "No autosave yet",
        "the line is frozen until the title is re-entered"
    );
    assert!(!menu(&app).continue_enabled);

    reenter_title(&mut app);
    assert!(
        menu(&app).continue_enabled,
        "re-entry re-reads the metadata"
    );
    assert!(
        menu(&app).continue_line.starts_with(&save.scene_id),
        "the re-read names the new autosave: {:?}",
        menu(&app).continue_line
    );
}

/// Continue rewrites nothing: a load writes no autosave, and nothing
/// outstanding after it does either.
#[test]
fn continue_writes_no_autosave() {
    let pack = pack_with_a_stamped_autosave();
    let mut app = boot_app(pack.path().join("pack"), None);
    assert!(menu(&app).continue_enabled);
    let before = std::fs::read(autosave_path(&app)).unwrap();

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
    press_and_update(&mut app, KeyCode::Enter);
    assert!(!menu(&app).open, "Continue entered play");
    assert!(live_session(&app), "Continue sets the live-session flag");
    assert!(
        !app.world().resource::<crate::slots::PendingAutosave>().0,
        "a load arms no autosave write"
    );
    assert_eq!(
        std::fs::read(autosave_path(&app)).unwrap(),
        before,
        "Continue rewrites nothing"
    );
    app.update();
    assert_eq!(
        std::fs::read(autosave_path(&app)).unwrap(),
        before,
        "nothing outstanding writes no autosave"
    );
}

/// A disabled Continue (nothing outstanding) is a no-op that writes nothing.
#[test]
fn a_disabled_continue_is_a_no_op_that_writes_nothing() {
    let mut app = temp_title_app();
    let before = parked_position(&app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
    press_and_update(&mut app, KeyCode::Enter);

    assert!(menu(&app).title_up(), "a disabled Continue is a no-op");
    assert_eq!(parked_position(&app), before, "position is untouched");
    assert!(
        !autosave_path(&app).exists(),
        "nothing outstanding writes no autosave"
    );
}

/// No key state gates play: with no key, a malformed key, a present key or the
/// environment key, Continue stays enabled and enters play, and New Game still
/// starts play.
#[test]
fn no_key_state_gates_continue_or_new_game() {
    let pack = pack_with_a_stamped_autosave();
    let pack_root = pack.path().join("pack");
    let rows = [
        KeyRow::NotSet,
        KeyRow::Invalid,
        KeyRow::Set {
            fingerprint: "sk-nano-\u{2026}a1b2".to_string(),
        },
        KeyRow::FromEnvironment {
            fingerprint: "sk-nano-\u{2026}env".to_string(),
        },
    ];

    for row in &rows {
        // Continue: enabled and it enters play.
        let mut app = boot_app(pack_root.clone(), None);
        app.insert_resource(CredentialsState {
            config_dir: None,
            path: None,
            row: row.clone(),
        });
        assert!(
            menu(&app).continue_enabled,
            "Continue stays enabled with {row:?}"
        );
        app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
        press_and_update(&mut app, KeyCode::Enter);
        assert!(!menu(&app).open, "Continue is not gated by {row:?}");
        assert!(live_session(&app), "{row:?} does not block play");

        // New Game: still starts play.
        let mut app = boot_app(pack_root.clone(), None);
        app.insert_resource(CredentialsState {
            config_dir: None,
            path: None,
            row: row.clone(),
        });
        app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleNewGame;
        press_and_update(&mut app, KeyCode::Enter);
        assert!(!menu(&app).open, "New Game is not gated by {row:?}");
        assert!(live_session(&app), "{row:?} does not block New Game");
    }
}

/// The app's one `saved_at` formatter: `DD Mon YYYY HH:MM`, or the dash when
/// unstamped. The concrete day/hour depend on the machine's timezone, so this
/// pins the shape; Continue and the overlay's card share the function.
#[test]
fn format_saved_at_pins_the_app_time_shape() {
    assert_eq!(menu::format_saved_at(None), "\u{2014}");
    let formatted = menu::format_saved_at(Some(1_760_000_000));
    assert_eq!(formatted.len(), 17, "DD Mon YYYY HH:MM: {formatted:?}");
    assert_eq!(
        &formatted[2..3],
        " ",
        "a space after the day: {formatted:?}"
    );
    assert_eq!(
        &formatted[6..7],
        " ",
        "a space after the month: {formatted:?}"
    );
    assert_eq!(
        &formatted[14..15],
        ":",
        "a colon in the time: {formatted:?}"
    );
}

#[test]
fn exit_to_title_renders_from_the_overlay_rail_with_its_confirm_copy() {
    let mut app = temp_title_app();
    app.init_resource::<crate::presentation::MenuBuilt>();
    enter_play(&mut app);
    esc(&mut app); // open the overlay
    click(&mut app, MenuAction::OpenSettings);
    assert_eq!(menu(&app).settings_origin, menu::MenuOrigin::Slots);

    build_menu_ui(&mut app);
    let texts = all_texts(&app);
    assert!(
        texts.iter().any(|text| text == "Exit to title"),
        "the overlay rail renders Exit to title: {texts:?}"
    );

    click(&mut app, MenuAction::ExitToTitle);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::ExitToTitle));
    build_menu_ui(&mut app);
    let texts = all_texts(&app);
    assert!(
        texts.iter().any(|text| text == "Exit to the title?"),
        "the exit confirm title renders: {texts:?}"
    );
    assert!(texts.iter().any(
        |text| text == "Your session is kept \u{2014} Continue resumes it where you left off."
    ));
    assert!(texts.iter().any(|text| text == "Exit"), "the accept button");
    assert!(
        texts.iter().any(|text| text == "Cancel"),
        "the cancel button"
    );
    assert!(
        !texts
            .iter()
            .any(|text| text.contains("Unsaved progress is lost")),
        "Exit never renders the Load card's sentence: {texts:?}"
    );
}

#[test]
fn cancelling_exit_to_title_changes_nothing() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    let _node = spawn_dialogue(&mut app);
    set_text_speed(&mut app, 0.04);
    render_dialogue(&mut app);
    advance_time(&mut app, 0.5);
    app.world_mut().resource_mut::<Notice>().show("stale");

    let position = parked_position(&app);
    let chars = app.world().resource::<TextReveal>().chars_shown(0.04);
    assert!(chars > 0, "the line is mid-reveal");

    esc(&mut app);
    click(&mut app, MenuAction::OpenSettings);
    click(&mut app, MenuAction::ExitToTitle);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::ExitToTitle));
    click(&mut app, MenuAction::ConfirmCancel);

    assert_eq!(menu(&app).confirm, None, "cancel drops the confirm");
    assert_eq!(
        menu(&app).view,
        MenuView::Settings,
        "cancel stays in Settings"
    );
    assert_eq!(
        parked_position(&app),
        position,
        "cancel leaves position alone"
    );
    assert_eq!(
        app.world().resource::<TextReveal>().chars_shown(0.04),
        chars,
        "cancel leaves the reveal clock alone"
    );
    assert!(app.world().resource::<Notice>().is_showing());
    assert!(!autosave_path(&app).exists(), "cancel writes nothing");
}

#[test]
fn exit_to_title_then_continue_applies_a_held_answer_without_a_second_decide() {
    let fake = FakeResolver::new(model_answer("warm"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    assert_eq!(fake.calls(), 1);

    esc(&mut app);
    click(&mut app, MenuAction::OpenSettings);
    click(&mut app, MenuAction::ExitToTitle);
    click(&mut app, MenuAction::ConfirmAccept);
    assert!(menu(&app).title_up(), "Exit to title shows the title");
    assert!(live_session(&app), "the session is kept");

    fake.release();
    pump_until(&mut app, held);
    assert!(held(&app), "the answer is held while the title is up");
    assert!(!autosave_path(&app).exists(), "held means no write yet");

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
    press_and_update(&mut app, KeyCode::Enter);
    assert!(!menu(&app).open, "Continue resumed the live session");
    assert!(parked_on_say(&app), "the held answer landed on resume");
    assert_eq!(fake.calls(), 1, "resuming fires no second decide call");

    let runtime = app.world().resource::<Runtime>();
    let save = save::read_slot(runtime.runner.pack(), Slot::Autosave).expect("autosaved once");
    assert_eq!(save.wait, runtime.runner.wait().cloned());
    let before = std::fs::read(autosave_path(&app)).unwrap();
    app.update();
    assert_eq!(
        std::fs::read(autosave_path(&app)).unwrap(),
        before,
        "a held model answer writes the autosave exactly once"
    );
}

#[test]
fn a_held_fallback_answer_across_the_title_writes_no_autosave() {
    let fake = FakeResolver::new(fallback_answer("cold"));
    let (mut app, _dir) = app_parked_on_decision(&fake);

    esc(&mut app);
    click(&mut app, MenuAction::OpenSettings);
    click(&mut app, MenuAction::ExitToTitle);
    click(&mut app, MenuAction::ConfirmAccept);
    assert!(menu(&app).title_up());

    fake.release();
    pump_until(&mut app, held);
    assert!(held(&app), "the fallback is held while the title is up");
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
    press_and_update(&mut app, KeyCode::Enter);

    assert!(parked_on_say(&app), "the held fallback landed on resume");
    assert!(
        !autosave_path(&app).exists(),
        "a held fallback writes no autosave"
    );
}

#[test]
fn new_game_with_a_live_session_renders_its_confirm_and_cancel_changes_nothing() {
    let mut app = temp_title_app();
    app.init_resource::<crate::presentation::MenuBuilt>();
    enter_play(&mut app);
    press_and_update(&mut app, KeyCode::Space);
    reenter_title(&mut app);
    let position = parked_position(&app);

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleNewGame;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::NewGame));

    build_menu_ui(&mut app);
    let texts = all_texts(&app);
    assert!(texts.iter().any(|text| text == "Start a new game?"));
    assert!(texts.iter().any(|text| {
        text
        == "The session in progress is discarded. Your saved slots and the autosave are untouched."
    }));
    assert!(texts.iter().any(|text| text == "Start"));
    assert!(texts.iter().any(|text| text == "Cancel"));

    click(&mut app, MenuAction::ConfirmCancel);
    assert_eq!(menu(&app).confirm, None);
    assert!(menu(&app).title_up(), "cancel stays on the title");
    assert!(live_session(&app), "cancel keeps the live session");
    assert_eq!(parked_position(&app), position, "cancel changes nothing");
}

#[test]
fn quit_with_a_live_session_renders_its_confirm_and_cancel_changes_nothing() {
    let mut app = temp_title_app();
    app.init_resource::<crate::presentation::MenuBuilt>();
    enter_play(&mut app);
    reenter_title(&mut app);

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleQuit;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(
        menu(&app).confirm,
        Some(ConfirmKind::Quit),
        "a live session confirms once"
    );

    build_menu_ui(&mut app);
    let texts = all_texts(&app);
    assert!(texts.iter().any(|text| text == "Quit?"));
    assert!(texts
        .iter()
        .any(|text| text == "Unsaved progress is lost and any decision in flight is cancelled."));
    assert!(texts.iter().any(|text| text == "Quit"));
    assert!(texts.iter().any(|text| text == "Cancel"));

    click(&mut app, MenuAction::ConfirmCancel);
    assert_eq!(menu(&app).confirm, None);
    assert!(menu(&app).title_up(), "cancel stays on the title");
    assert!(live_session(&app), "cancel keeps the live session");
    assert!(app.should_exit().is_none(), "cancel does not exit");
}

#[test]
fn quitting_with_a_held_answer_writes_no_autosave() {
    let fake = FakeResolver::new(model_answer("warm"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    reenter_title(&mut app);

    fake.release();
    pump_until(&mut app, held);
    assert!(held(&app), "the answer is held while the title is up");

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleQuit;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::Quit));
    click(&mut app, MenuAction::ConfirmAccept);

    assert!(matches!(app.should_exit(), Some(AppExit::Success)));
    assert!(
        held(&app),
        "quitting drops rather than applies the held answer"
    );
    assert!(
        !autosave_path(&app).exists(),
        "quitting with a held answer writes no autosave"
    );
}

/// A core app over the demo pack with the presentation's audio rig attached, so
/// the real menu actions can drive `sync_audio` end to end.
fn core_audio_app() -> App {
    let mut app = boot_app(demo_pack_root(), None);
    app.add_plugins(AssetPlugin {
        file_path: demo_pack_root()
            .join("assets")
            .to_string_lossy()
            .into_owned(),
        ..default()
    })
    .init_asset::<Image>()
    .init_asset::<bevy_kira_audio::AudioSource>()
    .init_resource::<AssetWatch>()
    .insert_resource(AudioChannel::<Bgm>::default());
    app
}

#[test]
fn bgm_stops_on_exit_to_title_and_restarts_on_continue() {
    let mut app = core_audio_app();
    enter_play(&mut app);
    // In play, the entry scene's track sounds.
    app.world_mut()
        .run_system_once(sync_audio)
        .expect("sync_audio runs");
    assert_eq!(
        app.world().resource::<AppliedAudio>().current_bgm(),
        Some("music_morning")
    );

    // The real `Exit to title`: overlay Settings rail, confirm, accept.
    esc(&mut app);
    click(&mut app, MenuAction::OpenSettings);
    click(&mut app, MenuAction::ExitToTitle);
    click(&mut app, MenuAction::ConfirmAccept);
    assert!(menu(&app).title_up());
    app.world_mut()
        .run_system_once(sync_audio)
        .expect("sync_audio runs");
    assert!(
        app.world()
            .resource::<AppliedAudio>()
            .current_bgm()
            .is_none(),
        "Exit to title stops the music"
    );

    // The real `Continue` (live session): the title closes and the track replays.
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
    press_and_update(&mut app, KeyCode::Enter);
    assert!(!menu(&app).open, "Continue entered play");
    app.world_mut()
        .run_system_once(sync_audio)
        .expect("sync_audio runs");
    assert_eq!(
        app.world().resource::<AppliedAudio>().current_bgm(),
        Some("music_morning"),
        "Continue restarts the music"
    );
}

// ---------------------------------------------------------------------------
// The embedded interface typefaces
// ---------------------------------------------------------------------------

/// The five interface faces are embedded in the binary and resolve with no
/// pack asset root: the asset root here points at a directory that does not
/// exist, so nothing can come from disk (`docs/title-and-menu.md`
/// § Type system §1).
#[test]
fn the_five_interface_faces_are_embedded_and_load_without_a_pack_asset_root() {
    use crate::presentation::style::{
        embed_text_faces, TextFaces, FIRAMONO_MEDIUM, FIRAMONO_REGULAR, IBMPLEXSANS_BOLD,
        IBMPLEXSANS_REGULAR, IBMPLEXSANS_SEMIBOLD,
    };
    use bevy::asset::Assets;
    use bevy::text::{Font, FontCx, TextPlugin};

    let dir = tempfile::tempdir().unwrap();
    let asset_root = dir.path().join("no-such-pack-assets");

    // The real delivery path: `TextPlugin` owns `Assets<Font>` (as it does in
    // `DefaultPlugins`), then the presentation plugin's `embed_text_faces`
    // installs the five embedded faces into it. The asset root does not exist,
    // so nothing can load from disk.
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(AssetPlugin {
            file_path: asset_root.to_string_lossy().into_owned(),
            ..default()
        })
        .add_plugins(TextPlugin);
    embed_text_faces(&mut app);
    app.update();

    let faces = app.world().resource::<TextFaces>().clone();

    // One distinct asset per face.
    let ids = [
        faces.sans_regular.id(),
        faces.sans_semibold.id(),
        faces.sans_bold.id(),
        faces.mono_regular.id(),
        faces.mono_medium.id(),
    ];
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), 5, "the five faces are distinct: {ids:?}");

    // Each handle is the embedded file for its named face, so
    // `mono_medium` is Fira Mono Medium and `mono_regular` Fira Mono Regular.
    let font_bytes = |handle: &Handle<Font>| {
        app.world()
            .resource::<Assets<Font>>()
            .get(handle)
            .unwrap_or_else(|| panic!("no asset for {handle:?}"))
            .data
            .data()
            .to_vec()
    };
    assert_eq!(font_bytes(&faces.sans_regular), IBMPLEXSANS_REGULAR);
    assert_eq!(font_bytes(&faces.sans_semibold), IBMPLEXSANS_SEMIBOLD);
    assert_eq!(font_bytes(&faces.sans_bold), IBMPLEXSANS_BOLD);
    assert_eq!(font_bytes(&faces.mono_regular), FIRAMONO_REGULAR);
    assert_eq!(font_bytes(&faces.mono_medium), FIRAMONO_MEDIUM);

    // Loadable: `TextPlugin`'s font-collection system registered both families
    // from the embedded bytes, so the handles resolve to real faces.
    let mut font_cx = app.world_mut().resource_mut::<FontCx>();
    for family in ["IBM Plex Sans", "Fira Mono"] {
        assert!(
            font_cx.collection.family_id(family).is_some(),
            "the embedded faces register the {family} family"
        );
    }
}

// ---------------------------------------------------------------------------
// The sans/mono type split
// ---------------------------------------------------------------------------

/// The five face handles, for membership checks.
fn face_handles(app: &App) -> Vec<Handle<bevy::text::Font>> {
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>();
    vec![
        faces.sans_regular.clone(),
        faces.sans_semibold.clone(),
        faces.sans_bold.clone(),
        faces.mono_regular.clone(),
        faces.mono_medium.clone(),
    ]
}

/// Every text entity in the world names one of the five bundled faces and never
/// Bevy's default handle (`docs/title-and-menu.md` § Type system §5).
fn assert_every_text_names_a_bundled_face(app: &App) {
    let bundled = face_handles(app);
    let mut checked = 0;
    for entity in app.world().iter_entities() {
        let is_text = entity.get::<Text>().is_some() || entity.get::<TextSpan>().is_some();
        if !is_text {
            continue;
        }
        let font = entity
            .get::<TextFont>()
            .unwrap_or_else(|| panic!("{:?} is text but has no TextFont", entity.id()));
        let handle = match &font.font {
            bevy::text::FontSource::Handle(handle) => handle.clone(),
            other => panic!("{:?} names a non-bundled source: {other:?}", entity.id()),
        };
        assert!(
            bundled.contains(&handle),
            "{:?} uses an unknown face: {handle:?}",
            entity.id()
        );
        checked += 1;
    }
    assert!(checked > 0, "the app built some text");
}

/// The faces of the title's wordmark, tagline, rows, reason line and legend
/// (`docs/title-and-menu.md` § Type system §2/§3/§4).
#[test]
fn the_titles_faces_follow_the_type_system() {
    let mut app = temp_title_app();
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut app);
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();

    let name = app
        .world()
        .resource::<Runtime>()
        .runner
        .pack()
        .manifest
        .name
        .clone();
    assert_eq!(
        face_handle(&app, text_entity(&app, &name)),
        faces.sans_bold,
        "the wordmark is Plex Sans Bold"
    );
    assert_eq!(
        face_handle(&app, text_entity(&app, "A DEMO PACK")),
        faces.mono_medium,
        "the tagline is Fira Mono Medium"
    );
    for label in ["Continue", "New Game", "Settings", "Gallery", "Quit"] {
        assert_eq!(
            face_handle(&app, text_entity(&app, label)),
            faces.sans_regular,
            "the {label} row label is sans"
        );
    }
    assert_eq!(
        face_handle(&app, text_entity(&app, "No autosave yet")),
        faces.mono_medium,
        "Continue's reason line is mono"
    );
    assert_eq!(
        face_handle(&app, text_entity(&app, menu::TITLE_LEGEND)),
        faces.mono_medium,
        "the legend is mono"
    );
    assert_every_text_names_a_bundled_face(&app);
}

/// The say-line's root is the mono speaker prefix and its child span the sans
/// prose, and the reveal writes the span only
/// (`docs/title-and-menu.md` § Type system §4/§5).
#[test]
fn the_say_line_splits_into_a_mono_prefix_and_a_sans_prose_span() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    let node = spawn_dialogue(&mut app);
    set_text_speed(&mut app, 0.02);
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();
    let (speaker, full) = say_parts(&app);
    let prefix = format!("{speaker}: ");

    render_dialogue(&mut app);
    let prose = app
        .world()
        .get::<Children>(node)
        .and_then(|children| children.first().copied())
        .expect("the dialogue node has one prose span");
    let root_text = |app: &App| app.world().get::<Text>(node).expect("root").0.clone();
    let prose_text = |app: &App| app.world().get::<TextSpan>(prose).expect("span").0.clone();

    assert_eq!(root_text(&app), prefix, "the root is the whole prefix");
    assert_eq!(prose_text(&app), "", "the span starts empty");
    assert_eq!(
        face_handle(&app, node),
        faces.mono_medium,
        "the speaker prefix is mono"
    );
    assert_eq!(
        face_handle(&app, prose),
        faces.sans_regular,
        "the prose span is sans"
    );
    assert_ne!(
        face_handle(&app, node),
        face_handle(&app, prose),
        "the two halves use different faces"
    );

    advance_time(&mut app, 0.5);
    render_dialogue(&mut app);
    let shown = prose_text(&app);
    assert!(
        !shown.is_empty() && full.starts_with(&shown),
        "the reveal fills the prose span: {shown:?}"
    );
    assert_eq!(
        root_text(&app),
        prefix,
        "the reveal never rewrites the mono prefix"
    );
}

/// A headless UI world with the embedded faces and no pack asset root renders
/// the dialogue split (`docs/title-and-menu.md` § Type system §5).
#[test]
fn a_headless_ui_world_renders_the_dialogue_split_without_a_pack_asset_root() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    attach_text_faces(&mut app);
    app.world_mut()
        .run_system_once(crate::presentation::play::setup_ui)
        .expect("setup_ui runs");

    let root = app
        .world()
        .iter_entities()
        .find_map(|entity| entity.get::<DialogueText>().map(|_| entity.id()))
        .expect("the dialogue node exists");
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();
    assert_eq!(face_handle(&app, root), faces.mono_medium);
    let prose = app
        .world()
        .get::<Children>(root)
        .and_then(|children| children.first().copied())
        .expect("the dialogue node has a prose span");
    assert_eq!(face_handle(&app, prose), faces.sans_regular);
    assert_ne!(face_handle(&app, root), face_handle(&app, prose));
}

/// The slot card splits into a sans label root, mono machine spans and a sans
/// action-label span, each with its own colour
/// (`docs/title-and-menu.md` § Type system §4/§5).
#[test]
fn the_slot_cards_split_their_faces_per_the_type_system() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    esc(&mut app); // the overlay's slots grid
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut app);
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();

    let card = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            (entity.get::<MenuAction>() == Some(&MenuAction::Card(0))).then_some(entity.id())
        })
        .expect("card 0 is built");
    assert_eq!(
        face_handle(&app, card),
        faces.sans_regular,
        "the card label is sans"
    );
    assert!(
        app.world().get::<TextColor>(card).is_some(),
        "the label sets its own colour"
    );

    let children: Vec<Entity> = app
        .world()
        .get::<Children>(card)
        .expect("the card has spans")
        .iter()
        .collect();
    assert_eq!(
        children.len(),
        5,
        "four machine lines then the action label"
    );
    for machine in &children[..4] {
        assert_eq!(
            face_handle(&app, *machine),
            faces.mono_medium,
            "kind/status/detail/saved-at are mono"
        );
        assert!(
            app.world().get::<TextColor>(*machine).is_some(),
            "every machine span sets its own colour"
        );
    }
    let action = children[4];
    assert_eq!(
        face_handle(&app, action),
        faces.sans_regular,
        "the action label is sans"
    );
    assert!(
        app.world().get::<TextColor>(action).is_some(),
        "the action span sets its own colour"
    );
}

/// The Settings panel title is SemiBold, group headers and control values mono,
/// and row labels sans (`docs/title-and-menu.md` § Type system §2/§3/§5).
#[test]
fn the_settings_faces_follow_the_type_system() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_settings(&mut app);
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut app);
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();

    assert_eq!(
        face_handle(&app, text_entity(&app, "Settings")),
        faces.sans_semibold,
        "the panel title is SemiBold"
    );
    for header in ["Audio", "Play", "Decisions"] {
        assert_eq!(
            face_handle(&app, text_entity(&app, header)),
            faces.mono_medium,
            "the {header} group header is mono"
        );
    }
    for label in [
        "Master",
        "Music",
        "SFX",
        "Mute",
        "Text speed",
        "Auto",
        "Fullscreen",
    ] {
        assert_eq!(
            face_handle(&app, text_entity(&app, label)),
            faces.sans_regular,
            "the {label} row label is sans"
        );
    }
    for value in ["100%", "Instant", "Off", "Not set", "Stored in \u{2014}"] {
        assert_eq!(
            face_handle(&app, text_entity(&app, value)),
            faces.mono_medium,
            "the {value} control value is mono"
        );
    }
    assert_every_text_names_a_bundled_face(&app);
}

/// The startup error screen's intro is sans while its pack path, error lines and
/// quit footer are mono (`docs/title-and-menu.md` § Type system §2/§5).
#[test]
fn the_error_screen_splits_its_sans_intro_and_mono_machine_lines() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    attach_text_faces(&mut app);
    app.insert_resource(StartupFailure::Pack {
        pack_root: PathBuf::from("/games/pack"),
        errors: vec![unmei_engine::LoadError::new("pack.ron", Some(3), "boom")],
        defaulted: false,
    });
    app.world_mut()
        .run_system_once(crate::presentation::play::draw_error_screen)
        .expect("draw_error_screen runs");
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();

    assert_eq!(
        face_handle(&app, text_entity(&app, "unmei could not load the pack:")),
        faces.sans_regular,
        "the intro is sans"
    );
    let machine = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            let text = entity.get::<Text>()?;
            text.0.contains("/games/pack").then_some(entity.id())
        })
        .expect("the machine block renders");
    assert_eq!(
        face_handle(&app, machine),
        faces.mono_medium,
        "the path, error lines and footer are mono"
    );
    let body = app
        .world()
        .get::<Text>(machine)
        .expect("machine text")
        .0
        .clone();
    assert!(body.contains("pack.ron:3: boom"), "{body}");
    assert!(
        body.contains("Esc · Enter · Space · click to quit"),
        "the quit footer is mono: {body}"
    );
    assert_every_text_names_a_bundled_face(&app);
}

/// Every surface's text names a bundled face, never Bevy's default handle
/// (`docs/title-and-menu.md` § Type system §5).
#[test]
fn every_app_text_entity_names_a_bundled_face() {
    // The title and its Settings door.
    let mut app = temp_title_app();
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut app);
    assert_every_text_names_a_bundled_face(&app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleSettings;
    press_and_update(&mut app, KeyCode::Enter);
    build_menu_ui(&mut app);
    assert_every_text_names_a_bundled_face(&app);
    app.world_mut()
        .run_system_once(crate::presentation::play::setup_ui)
        .expect("setup_ui runs");
    assert_every_text_names_a_bundled_face(&app);

    // The Esc overlay's slots grid and its Settings door.
    let pack = temp_pack();
    let mut play = app_for_pack(pack.path().join("pack"));
    esc(&mut play);
    play.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut play);
    assert_every_text_names_a_bundled_face(&play);
    open_settings(&mut play);
    build_menu_ui(&mut play);
    assert_every_text_names_a_bundled_face(&play);

    // The startup error screen.
    let mut error = App::new();
    error.add_plugins(MinimalPlugins);
    attach_text_faces(&mut error);
    error.insert_resource(StartupFailure::Pack {
        pack_root: PathBuf::from("/games/pack"),
        errors: Vec::new(),
        defaulted: false,
    });
    error
        .world_mut()
        .run_system_once(crate::presentation::play::draw_error_screen)
        .expect("draw_error_screen runs");
    assert_every_text_names_a_bundled_face(&error);
}

/// The choice options are sans (`docs/title-and-menu.md` § Type system §2).
#[test]
fn the_choice_options_are_sans() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    // The entry say -> Ace say -> the choice wait.
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
    render_dialogue(&mut app);
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();

    let button = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<crate::presentation::play::ChoiceButton>()
                .map(|_| entity.id())
        })
        .expect("a choice button exists");
    assert_eq!(
        face_handle(&app, button),
        faces.sans_regular,
        "the choice option text is sans"
    );
    assert_every_text_names_a_bundled_face(&app);
}

// ---------------------------------------------------------------------------
// The read log: player knowledge across launches and save slots
// ---------------------------------------------------------------------------

#[test]
fn the_boot_park_behind_the_title_marks_nothing_and_writes_nothing() {
    let pack = temp_pack();
    let mut app = boot_app(pack.path().join("pack"), None);

    let runtime = app.world().resource::<Runtime>();
    assert!(matches!(runtime.runner.wait(), Some(Wait::Say { .. })));
    assert_eq!(runtime.runner.scene_id(), "first_meeting");

    let state = read_state(&app);
    assert!(
        !state.is_visited("first_meeting"),
        "the boot park is not play"
    );
    assert!(!state.is_read("first_meeting", 7));
    assert!(!state.is_dirty());
    assert!(
        !read_log_path(&app).exists(),
        "looking at the title gains no file"
    );

    enter_play(&mut app);
    let state = read_state(&app);
    assert!(state.is_visited("first_meeting"));
    assert!(state.is_read("first_meeting", 7));
    assert_eq!(state.arrival_read(), Some(false));
    assert!(read_log_path(&app).exists(), "play entry flushed the marks");

    let on_disk = read_log_on_disk(&app).unwrap();
    assert!(on_disk.is_visited("first_meeting"));
    assert!(on_disk.is_read("first_meeting", 7));
}

#[test]
fn a_second_visit_to_a_line_reports_arrival_read() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");

    // First launch: the entry line is marked and reads as arrival-unread.
    let first = app_for_pack(pack_root.clone());
    assert_eq!(read_state(&first).arrival_read(), Some(false));
    assert!(read_state(&first).is_read("first_meeting", 7));
    drop(first);

    // Second launch: the same line is already known, so it reads as read.
    let second = app_for_pack(pack_root);
    assert_eq!(read_state(&second).arrival_read(), Some(true));
}

#[test]
fn a_first_visit_marks_and_reports_arrival_unread_in_the_same_step() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    // Entry say -> the Ace say is the next park.
    press_and_update(&mut app, KeyCode::Space);
    let state = read_state(&app);
    assert!(state.is_read("first_meeting", 10));
    assert_eq!(
        state.arrival_read(),
        Some(false),
        "the line is marked but arrival was captured before the mark"
    );

    // Back to the entry line with a New Game: now it is arrival-read.
    press_and_update(&mut app, KeyCode::Escape);
    click(&mut app, MenuAction::ExitToTitle);
    click(&mut app, MenuAction::ConfirmAccept);
    click(&mut app, MenuAction::NewGame);
    click(&mut app, MenuAction::ConfirmAccept);
    assert_eq!(read_state(&app).arrival_read(), Some(true));
}

#[test]
fn non_say_parks_mark_nothing_and_arrival_is_absent() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    // Entry say -> Ace say -> the choice wait.
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
    assert_eq!(read_state(&app).arrival_read(), None);
    let before = read_state(&app).log.clone();

    // The choice is a flush moment, but a choice-wait itself marks nothing.
    press_and_update(&mut app, KeyCode::Digit1);
    let after = &read_state(&app).log;
    assert!(
        after.is_read("first_meeting", 21),
        "the line the choice lands on is marked"
    );
    assert_eq!(
        before.read.get("first_meeting"),
        Some(&std::collections::BTreeSet::from([7, 10])),
        "the choice-wait added no say mark"
    );
}

#[test]
fn a_decision_wait_marks_nothing_and_a_model_landing_marks_its_line() {
    let pack = temp_pack();
    let fake = FakeResolver::new(model_answer("warm"));
    let mut app = boot_app(pack.path().join("pack"), Some(Arc::new(fake.clone())));
    enter_play(&mut app);

    // Play to the reunion decision point through the real input path.
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Digit1);
    press_and_update(&mut app, KeyCode::Space); // crossing into reunion
    let before_decision = read_state(&app).log.clone();
    press_and_update(&mut app, KeyCode::Space); // reunion say -> decision
    assert!(matches!(wait(&app), Some(Wait::Decision { .. })));
    pump_until(&mut app, |_| fake.calls() == 1);
    assert_eq!(read_state(&app).arrival_read(), None);
    assert_eq!(
        read_state(&app).log,
        before_decision,
        "the decision-wait itself marks nothing"
    );

    // The model answer lands `warm` on reunion line 14 and flushes the mark.
    fake.release();
    pump_until(&mut app, parked_on_say);
    let state = read_state(&app);
    assert!(state.is_read("reunion", 14));
    assert!(state.is_visited("reunion"));
    assert_eq!(state.arrival_read(), Some(false));
    assert!(
        read_log_on_disk(&app).unwrap().is_read("reunion", 14),
        "a resolved decision is a read-log flush moment"
    );
}

#[test]
fn a_same_scene_label_jump_marks_no_scene() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    // Entry -> Ace -> choice; choosing `ask_letter` is a same-scene label jump.
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Digit1);
    let state = read_state(&app);
    assert_eq!(
        state.log.visited,
        std::collections::BTreeSet::from(["first_meeting".to_string()]),
        "a local label jump is not a crossing"
    );
    assert!(state.is_read("first_meeting", 21));
}

#[test]
fn a_crossing_marks_the_target_scene_visited() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Digit1);
    press_and_update(&mut app, KeyCode::Space); // ask_letter say -> jump reunion
    assert!(read_state(&app).is_visited("reunion"));
    assert!(read_state(&app).is_read("reunion", 7));
}

#[test]
fn a_load_marks_the_restored_scene_visited_even_when_it_was_never_played() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root);
    assert!(!read_state(&app).is_visited("reunion"));

    // A save copied in beside the pack, pointing at reunion's first say.
    let pack_res = app.world().resource::<Runtime>().runner.pack().clone();
    let save = SaveFile {
        version: save::SAVE_VERSION,
        state: pack_res.state.clone(),
        scene_id: "reunion".to_string(),
        pc: 3,
        wait: Some(Wait::Say {
            speaker: "Narrator".to_string(),
            text: "The gate again, hours later. She's still there.".to_string(),
        }),
        staging: StagingState::default(),
        saved_at: None,
    };
    save::write_slot(&pack_res, Slot::Manual(1), &save).unwrap();

    press_and_update(&mut app, KeyCode::F9);
    let state = read_state(&app);
    assert_eq!(
        state.log.visited,
        std::collections::BTreeSet::from(["first_meeting".to_string(), "reunion".to_string()])
    );
    assert!(
        state.is_read("reunion", 7),
        "the restored say-line is marked"
    );
}

#[test]
fn marks_wait_for_a_flush_moment_and_overlay_browsing_writes_nothing() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    let after_entry = read_log_on_disk(&app).unwrap();

    // A plain advance marks but is not a flush moment.
    press_and_update(&mut app, KeyCode::Space);
    assert!(read_state(&app).is_read("first_meeting", 10));
    assert_eq!(read_log_on_disk(&app).unwrap(), after_entry, "no flush yet");

    // Opening and closing the overlay adds no flush opportunity.
    press_and_update(&mut app, KeyCode::Escape);
    press_and_update(&mut app, KeyCode::Escape);
    assert_eq!(
        read_log_on_disk(&app).unwrap(),
        after_entry,
        "overlay browsing writes nothing"
    );

    // The choice resolve is the next flush moment, and it carries the deferred
    // line-10 mark.
    press_and_update(&mut app, KeyCode::Space); // choice park
    press_and_update(&mut app, KeyCode::Digit1);
    let flushed = read_log_on_disk(&app).unwrap();
    assert!(flushed.is_read("first_meeting", 10));
    assert!(flushed.is_read("first_meeting", 21));
}

#[test]
fn a_fallback_decision_flushes_the_read_log_but_leaves_the_autosave_untouched() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Digit1);
    press_and_update(&mut app, KeyCode::Space); // crossing arms + writes the autosave
    assert!(autosave_path(&app).exists());
    let autosave_before = std::fs::read(autosave_path(&app)).unwrap();
    let read_before = read_log_on_disk(&app).unwrap();

    press_and_update(&mut app, KeyCode::Space); // reunion say -> decision
    app.update(); // the author fallback resolves and lands

    let read_after = read_log_on_disk(&app).unwrap();
    assert!(
        read_after.is_read("reunion", 19),
        "the fallback flush carried the landing mark"
    );
    assert_ne!(read_after, read_before, "the read log was flushed");
    assert_eq!(
        std::fs::read(autosave_path(&app)).unwrap(),
        autosave_before,
        "a fallback answer never churns the autosave"
    );
}

#[test]
fn app_exit_flushes_pending_read_marks() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    // Mark a line without a flush moment.
    press_and_update(&mut app, KeyCode::Space);
    assert!(!read_log_on_disk(&app).unwrap().is_read("first_meeting", 10));

    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<AppExit>>()
        .write(AppExit::Success);
    app.update();

    assert!(read_log_on_disk(&app).unwrap().is_read("first_meeting", 10));
    assert!(!read_state(&app).is_dirty());
}

#[test]
fn a_missing_file_is_empty_knowledge_with_no_failure() {
    captures::install();
    let pack = temp_pack();
    let app = boot_app_capturing(pack.path().join("pack"));
    assert!(
        failure_lines().is_empty(),
        "a missing file is first-run state"
    );
    assert!(!read_state(&app).is_dirty());
}

#[test]
fn an_undecodable_file_yields_empty_knowledge_and_one_warn_only_report() {
    captures::install();
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let path = pack_root.parent().unwrap().join("pack.read.ron");
    std::fs::write(&path, "this is not ron {").unwrap();
    let before = std::fs::read(&path).unwrap();

    let app = boot_app_capturing(pack_root);
    let records = failure_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, log::Level::Warn);
    assert!(
        records[0].2.starts_with("couldn't use the read log"),
        "{:?}",
        records[0].2
    );
    assert!(!app.world().resource::<Notice>().is_showing());
    assert!(!app.world().resource::<DefensiveHold>().is_held());
    assert_eq!(std::fs::read(&path).unwrap(), before, "bytes untouched");
    assert!(read_state(&app).log.read.is_empty());
}

#[test]
fn a_wrong_version_file_yields_empty_knowledge_and_one_warn_only_report() {
    captures::install();
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let path = pack_root.parent().unwrap().join("pack.read.ron");
    std::fs::write(
        &path,
        "(version: 2, read: {\"reunion\": [7]}, visited: [\"reunion\"])",
    )
    .unwrap();
    let before = std::fs::read(&path).unwrap();

    let app = boot_app_capturing(pack_root);
    let records = failure_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, log::Level::Warn);
    assert!(!read_state(&app).is_read("reunion", 7));
    assert!(!app.world().resource::<Notice>().is_showing());
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "the file is not rewritten"
    );

    // The next ordinary dirty write replaces the wrong version in place.
    let mut app = app;
    enter_play(&mut app);
    let on_disk = read_log_on_disk(&app).unwrap();
    assert_eq!(on_disk.version, unmei_engine::read_log::READ_LOG_VERSION);
    assert!(on_disk.is_read("first_meeting", 7));
}

#[test]
fn launch_filtering_drops_unusable_entries_without_writing() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut seeded = unmei_engine::read_log::ReadLog::default();
    // A loaded scene with a valid say line, a line 0, an out-of-range line, a
    // command's source line (first_meeting line 4 is `bg`), and an unknown scene.
    seeded.mark_read("first_meeting", 7);
    seeded.mark_read("first_meeting", 0);
    seeded.mark_read("first_meeting", 9999);
    seeded.mark_read("first_meeting", 4);
    seeded.mark_read("ghost_scene", 3);
    seeded.mark_visited("first_meeting");
    seeded.mark_visited("ghost_scene");
    seed_read_log(&pack_root, &seeded);
    let path = pack_root.parent().unwrap().join("pack.read.ron");
    let before = std::fs::read(&path).unwrap();

    let app = boot_app(pack_root, None);
    let state = read_state(&app);
    assert!(state.is_read("first_meeting", 7));
    assert!(!state.is_read("first_meeting", 0));
    assert!(!state.is_read("first_meeting", 9999));
    assert!(
        !state.is_read("first_meeting", 4),
        "a command line is not a say-line"
    );
    assert!(!state.is_read("ghost_scene", 3));
    assert!(!state.is_visited("ghost_scene"));
    assert!(!state.is_dirty(), "filtering alone does not dirty the log");
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "filtering alone writes nothing"
    );

    // The invalid entries disappear on the next ordinary dirty write.
    let mut app = app;
    enter_play(&mut app);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "the clean entry wrote nothing"
    );
    press_and_update(&mut app, KeyCode::Space); // mark line 10 (dirty)
    press_and_update(&mut app, KeyCode::Space); // choice park
    press_and_update(&mut app, KeyCode::Digit1); // flush moment
    let written = read_log_on_disk(&app).unwrap();
    assert_eq!(
        written.read.get("first_meeting"),
        Some(&std::collections::BTreeSet::from([7, 10, 21])),
        "only valid say-lines survive the write"
    );
    assert!(!written.read.contains_key("ghost_scene"));
    assert_eq!(
        written.visited,
        std::collections::BTreeSet::from(["first_meeting".to_string()])
    );
}

#[test]
fn the_log_is_read_once_per_launch_and_never_again() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut seeded = unmei_engine::read_log::ReadLog::default();
    seeded.mark_read("reunion", 7);
    seed_read_log(&pack_root, &seeded);

    let mut app = boot_app(pack_root.clone(), None);
    assert!(read_state(&app).is_read("reunion", 7));

    // Overwrite the file mid-session; the in-memory log must not change.
    let path = pack_root.parent().unwrap().join("pack.read.ron");
    let mut replacement = unmei_engine::read_log::ReadLog::default();
    replacement.mark_read("first_meeting", 10);
    unmei_engine::read_log::write_read_log(&path, &replacement).unwrap();

    enter_play(&mut app);
    assert!(
        read_state(&app).is_read("reunion", 7),
        "the launch read is the only read"
    );
    assert!(!read_state(&app).is_read("first_meeting", 10));
}

#[test]
fn a_broken_pack_neither_reads_nor_writes_the_read_log() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut seeded = unmei_engine::read_log::ReadLog::default();
    seeded.mark_read("reunion", 7);
    seed_read_log(&pack_root, &seeded);
    let path = pack_root.parent().unwrap().join("pack.read.ron");
    let before = std::fs::read(&path).unwrap();

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .insert_resource(
            RuntimeConfig::new(PathBuf::from("definitely/not/a/pack"), None).with_dev_mode(true),
        )
        .add_plugins(RuntimeCorePlugin);
    app.update();
    app.update();

    assert!(app.world().get_resource::<Runtime>().is_none());
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "no write on a rejected load"
    );
}

#[test]
fn a_failed_flush_logs_once_preserves_bytes_and_retries_at_the_next_moment() {
    captures::install();
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    force_single_threaded(&mut app);
    let path = read_log_path(&app);
    let after_entry = std::fs::read(&path).unwrap();

    // Block the atomic write by putting a directory where the temp sibling goes.
    let tmp = path.with_extension("ron.tmp");
    std::fs::create_dir(&tmp).unwrap();
    captures::take();

    // Mark a line, then reach a flush moment (the choice resolve).
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Digit1);

    let records = failure_lines();
    assert_eq!(
        records.len(),
        1,
        "one warn per failed occurrence: {records:?}"
    );
    assert_eq!(records[0].0, log::Level::Warn);
    assert!(records[0].2.starts_with("couldn't use the read log"));
    assert!(!app.world().resource::<Notice>().is_showing());
    assert!(!app.world().resource::<DefensiveHold>().is_held());
    assert_eq!(
        std::fs::read(&path).unwrap(),
        after_entry,
        "bytes preserved"
    );
    assert!(read_state(&app).is_dirty(), "the dirty flag survives");

    // Unblock and reach the next flush moment: the retry succeeds.
    std::fs::remove_dir(&tmp).unwrap();
    press_and_update(&mut app, KeyCode::Space); // crossing into reunion
    assert!(!read_state(&app).is_dirty());
    assert!(read_log_on_disk(&app).unwrap().is_read("first_meeting", 10));
}

#[test]
fn new_game_retains_knowledge_and_only_a_dirty_entry_writes() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root);

    // A pending dirty mark: the entry flush wrote [7]; advancing marks 10 with
    // no flush moment in between.
    press_and_update(&mut app, KeyCode::Space);
    assert!(read_state(&app).is_dirty());
    let dirty_bytes = std::fs::read(read_log_path(&app)).unwrap();
    assert!(!read_log_on_disk(&app).unwrap().is_read("first_meeting", 10));

    // Exit to title adds no flush opportunity: the pending mark waits.
    press_and_update(&mut app, KeyCode::Escape);
    click(&mut app, MenuAction::ExitToTitle);
    click(&mut app, MenuAction::ConfirmAccept);
    assert_eq!(
        std::fs::read(read_log_path(&app)).unwrap(),
        dirty_bytes,
        "Exit to title writes nothing"
    );

    // Start a New Game: the pending knowledge survives and is flushed with the
    // entry marks.
    click(&mut app, MenuAction::NewGame);
    click(&mut app, MenuAction::ConfirmAccept);
    let flushed = read_log_on_disk(&app).unwrap();
    assert!(
        flushed.is_read("first_meeting", 7),
        "existing knowledge retained"
    );
    assert!(
        flushed.is_read("first_meeting", 10),
        "pending knowledge flushed"
    );

    // Now the log is clean and the entry is already known: a second New Game
    // adds nothing and leaves the bytes identical.
    let clean = std::fs::read(read_log_path(&app)).unwrap();
    assert!(!read_state(&app).is_dirty());
    press_and_update(&mut app, KeyCode::Escape);
    click(&mut app, MenuAction::ExitToTitle);
    click(&mut app, MenuAction::ConfirmAccept);
    click(&mut app, MenuAction::NewGame);
    click(&mut app, MenuAction::ConfirmAccept);
    assert_eq!(read_state(&app).arrival_read(), Some(true));
    assert_eq!(
        std::fs::read(read_log_path(&app)).unwrap(),
        clean,
        "a clean, already-known entry writes nothing"
    );
}

#[test]
fn an_overlay_pause_preserves_the_arrival_status() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    // Back onto the known entry line: arrival-read.
    press_and_update(&mut app, KeyCode::Escape);
    click(&mut app, MenuAction::ExitToTitle);
    click(&mut app, MenuAction::ConfirmAccept);
    click(&mut app, MenuAction::NewGame);
    click(&mut app, MenuAction::ConfirmAccept);
    assert_eq!(read_state(&app).arrival_read(), Some(true));

    // A presentation pause is not a discontinuity: opening and closing the
    // overlay leaves the park and its arrival status alone.
    press_and_update(&mut app, KeyCode::Escape);
    assert_eq!(read_state(&app).arrival_read(), Some(true));
    press_and_update(&mut app, KeyCode::Escape);
    assert_eq!(read_state(&app).arrival_read(), Some(true));
}

#[test]
fn a_load_derives_arrival_from_the_log_not_the_park_it_replaced() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root);
    // Land back on the known entry line so the current park reads arrival-true.
    press_and_update(&mut app, KeyCode::Escape);
    click(&mut app, MenuAction::ExitToTitle);
    click(&mut app, MenuAction::ConfirmAccept);
    click(&mut app, MenuAction::NewGame);
    click(&mut app, MenuAction::ConfirmAccept);
    assert_eq!(read_state(&app).arrival_read(), Some(true));

    // A save copied in beside the pack, pointing at reunion's first say, which
    // this session has never read.
    let pack_res = app.world().resource::<Runtime>().runner.pack().clone();
    let save = SaveFile {
        version: save::SAVE_VERSION,
        state: pack_res.state.clone(),
        scene_id: "reunion".to_string(),
        pc: 3,
        wait: Some(Wait::Say {
            speaker: "Narrator".to_string(),
            text: "The gate again, hours later. She's still there.".to_string(),
        }),
        staging: StagingState::default(),
        saved_at: None,
    };
    save::write_slot(&pack_res, Slot::Manual(1), &save).unwrap();

    press_and_update(&mut app, KeyCode::F9);
    assert_eq!(
        read_state(&app).arrival_read(),
        Some(false),
        "arrival is recomputed from the log, never carried across the load"
    );
    assert!(
        read_state(&app).is_read("reunion", 7),
        "the restored line marks"
    );
}

#[test]
fn continue_resuming_a_clean_session_writes_nothing() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root);

    // Flush a clean stretch so the log is settled.
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Digit1);
    let clean = std::fs::read(read_log_path(&app)).unwrap();
    assert!(!read_state(&app).is_dirty());

    // Exit to title and Continue the live session: nothing changes, nothing
    // writes.
    press_and_update(&mut app, KeyCode::Escape);
    click(&mut app, MenuAction::OpenSettings);
    click(&mut app, MenuAction::ExitToTitle);
    click(&mut app, MenuAction::ConfirmAccept);
    assert!(live_session(&app));
    click(&mut app, MenuAction::Continue);
    assert!(!menu(&app).open, "Continue resumed the live session");
    assert_eq!(
        std::fs::read(read_log_path(&app)).unwrap(),
        clean,
        "a clean Continue-resume writes nothing"
    );
}

#[test]
fn empty_knowledge_can_become_arrival_read_later_in_the_same_launch() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    // First arrival to the entry line is unread.
    assert_eq!(read_state(&app).arrival_read(), Some(false));

    // A later ordinary visit in the same launch reads it as read.
    press_and_update(&mut app, KeyCode::Escape);
    click(&mut app, MenuAction::ExitToTitle);
    click(&mut app, MenuAction::ConfirmAccept);
    click(&mut app, MenuAction::NewGame);
    click(&mut app, MenuAction::ConfirmAccept);
    assert_eq!(
        read_state(&app).arrival_read(),
        Some(true),
        "read-once does not freeze knowledge for the whole launch"
    );
}

#[test]
fn an_io_unreadable_read_log_yields_empty_knowledge_and_one_warn_only_report() {
    captures::install();
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let path = pack_root.parent().unwrap().join("pack.read.ron");
    // A directory at the log path is IO-unreadable: `read_to_string` fails.
    std::fs::create_dir(&path).unwrap();

    let app = boot_app_capturing(pack_root);
    let records = failure_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, log::Level::Warn);
    assert!(records[0].2.starts_with("couldn't use the read log"));
    assert!(!app.world().resource::<Notice>().is_showing());
    assert!(read_state(&app).log.read.is_empty());
    assert!(
        path.is_dir(),
        "the unreadable target is not deleted or renamed"
    );
}

#[test]
fn the_end_of_scene_park_marks_no_say_line() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    play_to_end(&mut app);

    assert!(wait(&app).is_none(), "the script ended");
    assert_eq!(
        read_state(&app).arrival_read(),
        None,
        "the end-of-scene marker is not a say-park"
    );
}

#[test]
fn a_held_fallback_answer_flushes_the_read_log_on_resume() {
    let pack = temp_pack();
    let fake = FakeResolver::new(fallback_answer("cold"));
    let mut app = boot_app(pack.path().join("pack"), Some(Arc::new(fake.clone())));
    enter_play(&mut app);

    // Play to the reunion decision point; the crossing wrote the autosave.
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Digit1);
    press_and_update(&mut app, KeyCode::Space); // crossing into reunion
    press_and_update(&mut app, KeyCode::Space); // reunion say -> decision
    pump_until(&mut app, |_| fake.calls() == 1);
    let autosave = std::fs::read(autosave_path(&app)).unwrap();
    let read_before = read_log_on_disk(&app).unwrap();

    // Open the overlay, then let the author fallback answer: it is held, not
    // applied, so neither log changes yet.
    press_and_update(&mut app, KeyCode::Escape);
    fake.release();
    pump_until(&mut app, |app| {
        app.world().resource::<HeldDecision>().0.is_some()
    });
    assert_eq!(read_log_on_disk(&app).unwrap(), read_before);
    assert_eq!(std::fs::read(autosave_path(&app)).unwrap(), autosave);

    // Resume: the held fallback applies, marks its landing line and flushes the
    // read log — while still leaving the autosave alone.
    press_and_update(&mut app, KeyCode::Escape);
    pump_until(&mut app, parked_on_say);
    assert!(
        read_log_on_disk(&app).unwrap().is_read("reunion", 19),
        "the held fallback flushed its knowledge"
    );
    assert_eq!(
        std::fs::read(autosave_path(&app)).unwrap(),
        autosave,
        "a fallback answer never churns the autosave"
    );
}

// ---------------------------------------------------------------------------
// Text skip: replay previously read say-lines with Tab
// ---------------------------------------------------------------------------

/// The rendered `SKIP` cue, through the same `sync_skip_cue` the app runs.
fn skip_cue_text(app: &mut App) -> String {
    app.world_mut()
        .run_system_once(crate::presentation::play::sync_skip_cue)
        .expect("sync_skip_cue runs");
    let world = app.world_mut();
    let mut query = world.query::<(&crate::presentation::play::SkipCue, &Text)>();
    query
        .iter(world)
        .map(|(_, text)| text.0.clone())
        .next()
        .expect("the SKIP cue exists")
}

/// Seed the read log with exactly the listed `(scene, source line)` say-lines,
/// marking each named scene visited.
fn seed_read_lines(pack_root: &Path, entries: &[(&str, &[usize])]) {
    let mut log = unmei_engine::read_log::ReadLog::default();
    for (scene_id, lines) in entries {
        log.mark_visited(scene_id);
        for line in *lines {
            log.mark_read(scene_id, *line);
        }
    }
    seed_read_log(pack_root, &log);
}

/// An entry scene whose two say-lines are separated by a staging command.
fn pack_with_staged_commands() -> tempfile::TempDir {
    let pack = temp_pack();
    std::fs::write(
        pack.path().join("pack/scenes/first_meeting.unmei"),
        "bg school_gate\nsprite ace neutral\nNarrator: One.\nsprite ace happy\nNarrator: Two.\n",
    )
    .unwrap();
    pack
}

/// An entry scene that jumps into a second scene.
fn pack_with_crossing() -> tempfile::TempDir {
    let pack = temp_pack();
    std::fs::write(
        pack.path().join("pack/scenes/first_meeting.unmei"),
        "bg school_gate\nNarrator: One.\nNarrator: Two.\njump second_scene\n",
    )
    .unwrap();
    std::fs::write(
        pack.path().join("pack/scenes/second_scene.unmei"),
        "Narrator: Three.\nNarrator: Four.\n",
    )
    .unwrap();
    pack
}

#[test]
fn the_skip_dwell_is_fifty_ms_and_unscaled() {
    assert_eq!(crate::core::SKIP_DWELL_SECONDS, 0.05);
}

#[test]
fn tab_engages_only_on_an_arrival_read_say_park() {
    // Unknown arrival: inert, no cue flash.
    let pack = temp_pack();
    let root = pack.path().join("pack");
    let mut app = app_for_pack(root.clone());
    build_ui(&mut app);
    assert_eq!(read_state(&app).arrival_read(), Some(false));
    press_and_update(&mut app, KeyCode::Tab);
    assert!(!skip_on(&app), "an unread arrival cannot engage");
    assert_eq!(skip_cue_text(&mut app), "", "no cue flash");

    // A non-say park is inert too.
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
    press_and_update(&mut app, KeyCode::Tab);
    assert!(!skip_on(&app), "a choice-wait cannot engage");

    // Known arrival: engages, shows the cue, and toggles back off.
    let known = pack_with_entry_says(&["Narrator: One.", "Narrator: Two."]);
    let mut known_app = app_with_known_entry(&known);
    build_ui(&mut known_app);
    assert_eq!(read_state(&known_app).arrival_read(), Some(true));
    press_and_update(&mut known_app, KeyCode::Tab);
    assert!(skip_on(&known_app));
    assert_eq!(skip_cue_text(&mut known_app), "SKIP");
    press_and_update(&mut known_app, KeyCode::Tab);
    assert!(!skip_on(&known_app));
    assert_eq!(skip_cue_text(&mut known_app), "");
}

#[test]
fn tab_engages_mid_reveal() {
    let pack = pack_with_entry_says(&[
        "Narrator: One two three four five six seven eight.",
        "Narrator: Two.",
    ]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = app_for_pack(root);
    build_ui(&mut app);
    set_text_speed(&mut app, 0.02);
    render_dialogue(&mut app);
    advance_time(&mut app, 0.1);
    render_dialogue(&mut app);
    assert!(!app.world().resource::<TextReveal>().is_complete(0.02));

    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app), "mid-reveal counts for the gate");
}

#[test]
fn an_instant_line_advances_one_step_per_fifty_ms() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let mut app = app_with_known_entry(&pack);
    let first = wait(&app).expect("parked on the first say");
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));
    assert_eq!(wait(&app), Some(first.clone()));

    advance_time(&mut app, 0.04);
    tick_update(&mut app);
    assert_eq!(wait(&app), Some(first.clone()), "0.04 s is short of a step");

    advance_time(&mut app, 0.02);
    tick_update(&mut app);
    assert_ne!(wait(&app), Some(first), "one advance at 0.05 s");
}

#[test]
fn the_engagement_frame_never_advances_even_under_a_hitch() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two."]);
    let mut app = app_with_known_entry(&pack);
    let first = wait(&app).unwrap();

    // Engage and step in one frame whose delta is five seconds. The completed
    // line must still be shown for its dwell's opening frame, not advanced
    // away before it ever renders.
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Tab);
    advance_time(&mut app, 5.0);
    tick_update(&mut app);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();

    assert!(skip_on(&app));
    assert_eq!(
        wait(&app),
        Some(first.clone()),
        "the engage frame completes the line but never advances it"
    );

    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert_ne!(wait(&app), Some(first), "then one step advances");
}

#[test]
fn a_one_second_reveal_is_completed_and_advanced_in_a_single_step() {
    let pack = pack_with_entry_says(&[
        "Narrator: One two three four five six seven.",
        "Narrator: Eight.",
    ]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = app_for_pack(root);
    set_text_speed(&mut app, 0.02);
    render_dialogue(&mut app);
    advance_time(&mut app, 0.5);
    render_dialogue(&mut app);
    assert!(!app.world().resource::<TextReveal>().is_complete(0.02));
    let line = wait(&app).unwrap();

    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));
    assert_eq!(wait(&app), Some(line.clone()), "engaging never advances");
    assert!(
        app.world().resource::<TextReveal>().is_complete(0.02),
        "the step completes the reveal"
    );

    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert_ne!(wait(&app), Some(line), "then advances once");
}

#[test]
fn a_hitch_advances_once_and_never_catches_up() {
    let pack = pack_with_entry_says(&[
        "Narrator: One.",
        "Narrator: Two.",
        "Narrator: Three.",
        "Narrator: Four.",
    ]);
    let mut app = app_with_known_entry(&pack);
    let first = wait(&app).unwrap();
    press_and_update(&mut app, KeyCode::Tab);

    advance_time(&mut app, 5.0);
    tick_update(&mut app);
    let second = wait(&app).unwrap();
    assert_ne!(second, first, "the hitch advanced exactly once");

    advance_time(&mut app, 0.0);
    tick_update(&mut app);
    assert_eq!(wait(&app), Some(second.clone()), "no banked catch-up");

    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert_ne!(wait(&app), Some(second), "a fresh step resumes normally");
}

#[test]
fn commands_between_parks_execute_inside_the_step() {
    let pack = pack_with_staged_commands();
    let root = pack.path().join("pack");
    seed_read_lines(&root, &[("first_meeting", &[3, 5])]);
    let mut app = app_for_pack(root);
    assert_eq!(
        app.world()
            .resource::<Runtime>()
            .staging
            .sprites
            .get("ace")
            .map(String::as_str),
        Some("neutral")
    );
    press_and_update(&mut app, KeyCode::Tab);
    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert_eq!(
        app.world()
            .resource::<Runtime>()
            .staging
            .sprites
            .get("ace")
            .map(String::as_str),
        Some("happy"),
        "the staging command between the two skipped says landed in the step"
    );
}

#[test]
fn a_jump_crosses_inside_the_step_and_known_text_continues() {
    let pack = pack_with_crossing();
    let root = pack.path().join("pack");
    seed_read_lines(
        &root,
        &[("first_meeting", &[2, 3]), ("second_scene", &[1, 2])],
    );
    let mut app = app_for_pack(root);
    assert_eq!(
        app.world().resource::<Runtime>().runner.scene_id(),
        "first_meeting"
    );
    press_and_update(&mut app, KeyCode::Tab);

    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert_eq!(
        app.world().resource::<Runtime>().runner.scene_id(),
        "first_meeting"
    );
    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert_eq!(
        app.world().resource::<Runtime>().runner.scene_id(),
        "second_scene",
        "the jump crossed inside the skipped step"
    );
    assert!(skip_on(&app), "known text continues across the crossing");
    assert!(
        read_state(&app).is_visited("second_scene"),
        "the crossing marked the new scene visited"
    );
}

#[test]
fn a_crossing_stops_on_the_first_arrival_unread_line() {
    let pack = pack_with_crossing();
    let root = pack.path().join("pack");
    seed_read_lines(&root, &[("first_meeting", &[2, 3])]);
    let mut app = app_for_pack(root);
    press_and_update(&mut app, KeyCode::Tab);

    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert_eq!(
        app.world().resource::<Runtime>().runner.scene_id(),
        "second_scene"
    );
    assert!(
        !skip_on(&app),
        "the first unread line ends skip on the landing frame"
    );

    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert!(!skip_on(&app), "the first unread line stopped skip");
    assert!(
        read_state(&app).is_read("second_scene", 1),
        "the stop line is marked by its ordinary park"
    );
    assert_eq!(read_state(&app).arrival_read(), Some(false));
}

#[test]
fn skip_ends_at_a_choice_wait_without_choosing() {
    let pack = temp_pack();
    let root = pack.path().join("pack");
    seed_read_lines(&root, &[("first_meeting", &[7, 10])]);
    let mut app = app_for_pack(root);
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));

    for _ in 0..4 {
        advance_time(&mut app, 0.05);
        tick_update(&mut app);
        if matches!(wait(&app), Some(Wait::Choice { .. })) {
            break;
        }
    }
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
    assert!(
        !skip_on(&app),
        "a choice-wait ends skip on the frame it lands"
    );
    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert!(!skip_on(&app), "skip holds at a choice-wait");
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
}

#[test]
fn a_space_press_ends_skip_and_advances_once() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let mut app = app_with_known_entry(&pack);
    let first = wait(&app).unwrap();
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));

    press_and_update(&mut app, KeyCode::Space);
    assert!(!skip_on(&app), "Space ends the toggle");
    assert_ne!(
        wait(&app),
        Some(first),
        "and performs exactly its own one advance"
    );
}

#[test]
fn a_space_mid_reveal_completes_and_does_not_step() {
    let pack = pack_with_entry_says(&["Narrator: One two three.", "Narrator: Four."]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = app_for_pack(root);
    set_text_speed(&mut app, 0.02);
    press_and_update(&mut app, KeyCode::Tab);
    // Advance the skip onto the second line, then arm its reveal so it is
    // mid-reveal while the toggle is still running.
    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    let line = wait(&app).unwrap();
    render_dialogue(&mut app);
    assert!(!app.world().resource::<TextReveal>().is_complete(0.02));

    press_and_update(&mut app, KeyCode::Space);
    assert!(!skip_on(&app), "the press ends the toggle");
    assert_eq!(wait(&app), Some(line), "it only completes, never advances");
    assert!(app.world().resource::<TextReveal>().is_complete(0.02));
}

#[test]
fn esc_ends_skip_before_the_overlay_and_closing_does_not_resume() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let mut app = app_with_known_entry(&pack);
    build_ui(&mut app);
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));

    esc(&mut app);
    assert!(menu(&app).open, "Esc opened the overlay");
    assert!(!skip_on(&app), "the toggle ended on the opening frame");
    assert_eq!(skip_cue_text(&mut app), "");

    esc(&mut app);
    assert!(!menu(&app).open);
    assert!(!skip_on(&app), "closing does not resurrect the toggle");
}

#[test]
fn f11_and_f5_leave_skip_running() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let mut app = app_with_known_entry(&pack);
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));
    press_and_update(&mut app, KeyCode::F11);
    assert!(skip_on(&app), "F11 does not cancel");
    press_and_update(&mut app, KeyCode::F5);
    assert!(skip_on(&app), "F5 does not cancel");
}

#[test]
fn a_ends_skip_and_toggles_auto() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two."]);
    let mut app = app_with_known_entry(&pack);
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));
    assert!(!auto_on(&app));
    press_and_update(&mut app, KeyCode::KeyA);
    assert!(!skip_on(&app));
    assert!(auto_on(&app), "A is the explicit handoff");
}

#[test]
fn a_load_disarms_skip() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let mut app = app_with_known_entry(&pack);
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));
    press_and_update(&mut app, KeyCode::F5);
    assert!(skip_on(&app), "saving is not a discontinuity");
    press_and_update(&mut app, KeyCode::F9);
    assert!(!skip_on(&app), "a load disarms the toggle");
}

#[test]
fn an_empty_log_can_become_arrival_read_later_in_the_same_launch() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two."]);
    let mut app = app_for_pack(pack.path().join("pack"));
    assert_eq!(read_state(&app).arrival_read(), Some(false));
    press_and_update(&mut app, KeyCode::Tab);
    assert!(!skip_on(&app), "first arrival cannot engage");

    // New Game returns to the same entry park, now arrival-read.
    esc(&mut app);
    click(&mut app, MenuAction::ExitToTitle);
    click(&mut app, MenuAction::ConfirmAccept);
    click(&mut app, MenuAction::NewGame);
    click(&mut app, MenuAction::ConfirmAccept);
    assert_eq!(read_state(&app).arrival_read(), Some(true));
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app), "the ordinary revisit is eligible");
}

#[test]
fn the_overlay_gate_freezes_skip_timing() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let mut app = app_with_known_entry(&pack);
    press_and_update(&mut app, KeyCode::Tab);
    let line = wait(&app).unwrap();

    // Open without Esc (a defence-in-depth check of the shared gate).
    app.world_mut().resource_mut::<MenuState>().open = true;
    for _ in 0..3 {
        advance_time(&mut app, 1.0);
        tick_update(&mut app);
    }
    assert_eq!(wait(&app), Some(line.clone()), "no step under the overlay");

    app.world_mut().resource_mut::<MenuState>().open = false;
    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert_ne!(wait(&app), Some(line), "closing resumes the step");
}

#[test]
fn skip_suppresses_auto_without_changing_the_preference() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let mut app = app_with_known_entry(&pack);
    build_ui(&mut app);
    enable_auto(&mut app);
    assert!(auto_on(&app));
    assert_eq!(auto_cue_text(&mut app), "AUTO");

    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));
    assert!(auto_on(&app), "the preference is untouched");
    assert_eq!(auto_cue_text(&mut app), "", "AUTO hides while skip runs");
    assert_eq!(skip_cue_text(&mut app), "SKIP");

    let first = wait(&app).unwrap();
    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert_ne!(
        wait(&app),
        Some(first),
        "the line advances on skip's 0.05 s, not auto's 2.0 s"
    );
}

#[test]
fn ending_skip_restarts_a_full_auto_dwell() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let mut app = app_with_known_entry(&pack);
    enable_auto(&mut app);
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));

    // Space cancels skip and advances; auto is still on and must start fresh.
    press_and_update(&mut app, KeyCode::Space);
    assert!(!skip_on(&app));
    let line = wait(&app).unwrap();

    advance_time(&mut app, 1.9);
    tick_auto(&mut app);
    assert_eq!(wait(&app), Some(line.clone()), "no remainder carried in");

    advance_time(&mut app, 0.2);
    tick_auto(&mut app);
    assert_ne!(wait(&app), Some(line), "a full 2.0 s after skip ends");
}

#[test]
fn the_help_line_carries_both_skip_tokens_and_no_dev_tokens() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    build_ui(&mut app);
    let help = help_line(&mut app);
    assert!(help.contains("Tab skip"), "{help}");
    assert!(help.contains("Ctrl skip all"), "{help}");
    assert!(help.contains("A auto"), "{help}");
    assert!(help.contains("F11 fullscreen"), "{help}");
    assert!(!help.contains("F5"), "{help}");
    assert!(!help.contains("F9"), "{help}");
}

#[test]
fn cues_hide_on_title_surfaces() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two."]);
    let mut app = app_with_known_entry(&pack);
    build_ui(&mut app);
    // Engage directly so the title-hiding rule is tested independently of Esc.
    app.world_mut()
        .resource_mut::<crate::core::SkipState>()
        .engage();
    assert_eq!(skip_cue_text(&mut app), "SKIP");

    app.world_mut().resource_mut::<MenuState>().open = true;
    app.world_mut().resource_mut::<MenuState>().view = MenuView::Title;
    assert!(menu(&app).title_surface_up());
    assert_eq!(skip_cue_text(&mut app), "", "the cue hides on the title");
}

#[test]
fn the_cue_and_dialogue_geometry_are_the_approved_1280x720_values() {
    let mut app = app_for_pack(temp_pack().path().join("pack"));
    build_ui(&mut app);
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();

    let dialogue = app
        .world_mut()
        .query_filtered::<&Node, With<crate::presentation::play::DialogueText>>()
        .iter(app.world())
        .next()
        .cloned()
        .expect("the dialogue band exists");
    assert_eq!(dialogue.left, Val::Px(48.0));
    assert_eq!(dialogue.bottom, Val::Px(40.0));
    assert_eq!(dialogue.right, Val::Px(160.0), "the cue slot is reserved");

    for (entity, colour) in [
        (
            entity_with::<crate::presentation::play::SkipCue>(&app),
            crate::presentation::play::SKIP_CUE_COLOR,
        ),
        (
            entity_with::<crate::presentation::play::AutoCue>(&app),
            crate::presentation::play::AUTO_CUE_COLOR,
        ),
    ] {
        let node = app.world().get::<Node>(entity).expect("cue node");
        assert_eq!(node.bottom, Val::Px(40.0));
        assert_eq!(node.right, Val::Px(48.0));
        let font = app.world().get::<TextFont>(entity).expect("cue font");
        assert_eq!(font.font_size, faces.mono(20.0).font_size);
        assert_eq!(face_handle(&app, entity), faces.mono_medium);
        let toned = app.world().get::<TextColor>(entity).expect("cue colour");
        assert_eq!(toned.0, colour);
    }
}

#[test]
fn a_new_game_disarms_skip() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two."]);
    let mut app = app_with_known_entry(&pack);
    // Raise the title directly, then engage so the New Game reset is observable.
    app.world_mut().resource_mut::<MenuState>().open = true;
    app.world_mut().resource_mut::<MenuState>().view = MenuView::Title;
    app.world_mut()
        .resource_mut::<crate::core::SkipState>()
        .engage();
    assert!(skip_on(&app));

    click(&mut app, MenuAction::NewGame);
    click(&mut app, MenuAction::ConfirmAccept);
    assert!(!skip_on(&app), "New Game disarms the toggle");
}

#[test]
fn a_choice_pick_performs_only_its_own_action() {
    let pack = temp_pack();
    let root = pack.path().join("pack");
    seed_read_lines(&root, &[("first_meeting", &[7, 10])]);
    let mut app = app_for_pack(root);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
    assert!(!skip_on(&app), "skip never runs at a choice-wait");
    // Arm the toggle defensively at the choice, so the digit's cancel path is
    // exercised: the pick must end it and do only its own action.
    app.world_mut()
        .resource_mut::<crate::core::SkipState>()
        .engage();
    assert!(skip_on(&app));

    let before = parked_position(&app);
    press_and_update(&mut app, KeyCode::Digit1);
    assert!(!skip_on(&app), "the digit ended the armed toggle");
    assert_ne!(
        parked_position(&app),
        before,
        "the digit performed exactly its own pick"
    );
}

/// An entry scene whose second line is a `decide` point, so a resolvable
/// decision-wait can be parked in flight.
fn pack_with_decision() -> tempfile::TempDir {
    let pack = temp_pack();
    std::fs::write(
        pack.path().join("pack/scenes/first_meeting.unmei"),
        "bg school_gate\nNarrator: One.\ndecide ace.greet -> reply\n\
         label reply.warm\nNarrator: Two.\nlabel reply.cold\nNarrator: Three.\n\
         label reply.none\nNarrator: Four.\n",
    )
    .unwrap();
    pack
}

#[test]
fn skip_ends_at_a_decision_wait_without_resolving() {
    let pack = pack_with_decision();
    let root = pack.path().join("pack");
    seed_read_lines(&root, &[("first_meeting", &[2])]);
    let fake = FakeResolver::new(model_answer("warm"));
    let mut app = app_with_decisions_at(root, Some(Arc::new(fake.clone())));
    build_ui(&mut app);
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));

    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert!(matches!(wait(&app), Some(Wait::Decision { .. })));
    assert!(
        !skip_on(&app),
        "the decision-wait ends skip on the frame it lands"
    );
    assert_eq!(skip_cue_text(&mut app), "");

    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert!(matches!(wait(&app), Some(Wait::Decision { .. })));
    assert_eq!(skip_cue_text(&mut app), "");

    pump_until(&mut app, |_| fake.calls() == 1);
    assert!(
        app.world().resource::<PendingDecision>().0.is_some(),
        "skip neither resolved nor cancelled the request"
    );
}

#[test]
fn seen_skip_stops_at_a_resolverless_decision_even_when_its_landing_is_read() {
    let pack = pack_with_decision();
    let root = pack.path().join("pack");
    // Both the engaged line and the author fallback's landing line are known,
    // so only the decision stop can end the toggle: under the old ordering the
    // immediate fallback ran to its read landing in the same frame and skip
    // continued straight through the decision.
    seed_read_lines(&root, &[("first_meeting", &[2, 7])]);
    let mut app = app_for_pack(root);
    build_ui(&mut app);
    assert!(matches!(wait(&app), Some(Wait::Say { text, .. }) if text == "One."));
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));

    // One step advances into the decision; the resolver-less fallback lands on
    // `reply.cold`'s read line in the same frame.
    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert!(
        matches!(wait(&app), Some(Wait::Say { text, .. }) if text == "Three."),
        "the immediate fallback still lands: {:?}",
        wait(&app)
    );
    assert!(
        !skip_on(&app),
        "the decision stop is not bypassed by its immediate fallback"
    );
    assert_eq!(skip_cue_text(&mut app), "");
}

#[test]
fn skip_ends_at_the_end_of_the_scene() {
    let pack = pack_with_entry_says(&["Narrator: One."]);
    let mut app = app_with_known_entry(&pack);
    build_ui(&mut app);
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));

    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert!(wait(&app).is_none(), "the script ended");
    assert!(!skip_on(&app), "the end of the scene ends skip");
    assert_eq!(skip_cue_text(&mut app), "");
}

#[test]
fn a_running_skip_never_dirties_the_read_log() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = app_for_pack(root);
    let before = std::fs::read(read_log_path(&app)).expect("the seeded log exists");
    assert!(!read_state(&app).is_dirty());

    press_and_update(&mut app, KeyCode::Tab);
    for _ in 0..2 {
        advance_time(&mut app, 0.05);
        tick_update(&mut app);
    }
    assert!(!read_state(&app).is_dirty(), "seen skip adds no marks");
    assert_eq!(std::fs::read(read_log_path(&app)).unwrap(), before);
}

#[test]
fn a_successor_line_renders_whole_for_its_full_dwell() {
    let pack = pack_with_entry_says(&[
        "Narrator: One two three four five six seven eight.",
        "Narrator: Nine ten eleven twelve thirteen fourteen.",
        "Narrator: Later.",
    ]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = app_for_pack(root);
    build_ui(&mut app);
    let node = spawn_dialogue(&mut app);
    set_text_speed(&mut app, 0.02);
    render_dialogue(&mut app);

    // Engage on the first (revealing) line: the completion frame only
    // completes it. One 0.05 s step then lands on the second line, still
    // revealing.
    press_and_update(&mut app, KeyCode::Tab);
    let first = say_parts(&app).1;
    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    render_dialogue(&mut app);
    let second = say_parts(&app).1;
    assert_ne!(second, first, "the step landed on the successor");
    assert!(
        !app.world().resource::<TextReveal>().is_complete(0.02),
        "the successor is still revealing when it lands"
    );

    // The completion frame spends no dwell: a full 0.05 s frame completes the
    // line and still shows it whole instead of advancing it away unrendered.
    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    render_dialogue(&mut app);
    assert_eq!(
        say_parts(&app).1,
        second,
        "the successor is not advanced away on its completion frame"
    );
    assert!(app.world().resource::<TextReveal>().is_complete(0.02));
    assert_eq!(
        dialogue_text(&app, node),
        format!("Narrator: {second}"),
        "the frame rendered the whole successor line"
    );

    // The next 0.05 s frame is the dwell's first and last: it advances once.
    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert_ne!(
        say_parts(&app).1,
        second,
        "the full dwell then advances once"
    );
}

#[test]
fn a_digit_ends_seen_skip_without_picking_or_stepping() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two."]);
    let mut app = app_with_known_entry(&pack);
    let first = wait(&app).unwrap();
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));

    // Arm the step to fire this frame, then press a digit: the cancelling
    // input wins before the skip step and performs no pick at a say-park.
    advance_time(&mut app, 0.05);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Digit5);
    tick_update(&mut app);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
    assert!(!skip_on(&app), "the digit ended the toggle");
    assert_eq!(wait(&app), Some(first), "the digit spent no skip step");
}

#[test]
fn enter_and_left_click_each_end_skip_and_do_only_their_action() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let mut app = app_with_known_entry(&pack);
    let first = wait(&app).unwrap();
    press_and_update(&mut app, KeyCode::Tab);
    press_and_update(&mut app, KeyCode::Enter);
    assert!(!skip_on(&app), "Enter ends the toggle");
    assert_ne!(wait(&app), Some(first), "and advances exactly once");

    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let mut app = app_with_known_entry(&pack);
    let first = wait(&app).unwrap();
    press_and_update(&mut app, KeyCode::Tab);
    click_and_update(&mut app);
    assert!(!skip_on(&app), "a left-click ends the toggle");
    assert_ne!(wait(&app), Some(first), "and advances exactly once");
}

#[test]
fn a_choice_button_performs_only_its_own_action() {
    let pack = temp_pack();
    let root = pack.path().join("pack");
    seed_read_lines(&root, &[("first_meeting", &[7, 10])]);
    let mut app = app_for_pack(root);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
    assert!(!skip_on(&app));
    app.world_mut()
        .resource_mut::<crate::core::SkipState>()
        .engage();
    assert!(skip_on(&app));

    let before = parked_position(&app);
    app.world_mut().spawn((
        crate::presentation::play::ChoiceButton { index: 0 },
        Interaction::Pressed,
    ));
    app.update();
    assert!(!skip_on(&app), "the button ended the armed toggle");
    assert_ne!(
        parked_position(&app),
        before,
        "the choice button performed exactly its own pick"
    );
}

#[test]
fn esc_still_clears_skip_under_a_defensive_hold() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let mut app = app_with_known_entry(&pack);
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));

    app.world_mut().resource_mut::<DefensiveHold>().engage();
    esc(&mut app);
    assert!(menu(&app).open, "Esc still opens the overlay under a hold");
    assert!(
        !skip_on(&app),
        "Esc clears the toggle even while a hold freezes the play path"
    );
}

// ---------------------------------------------------------------------------
// Force skip: deliberately skip all say-lines while holding Ctrl
// ---------------------------------------------------------------------------

/// Step the force hold until it reaches a choice-wait.
fn run_until_choice(app: &mut App) {
    for _ in 0..8 {
        advance_time(app, 0.05);
        step_input(app);
        if matches!(wait(app), Some(Wait::Choice { .. })) {
            return;
        }
    }
    panic!("the hold never reached a choice");
}

#[test]
fn a_held_ctrl_skips_unread_lines_and_release_stops() {
    let pack = pack_with_entry_says(&[
        "Narrator: One.",
        "Narrator: Two.",
        "Narrator: Three.",
        "Narrator: Four.",
    ]);
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);
    let first = wait(&app).unwrap();
    assert_eq!(
        read_state(&app).arrival_read(),
        Some(false),
        "unread arrival"
    );

    // Press once and keep Ctrl down: force arms on an unread line.
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_on(&app), "a held Ctrl skips an unread line");
    assert!(skip_force(&app));
    assert_eq!(skip_cue_text(&mut app), "SKIP");
    assert_eq!(wait(&app), Some(first.clone()), "the arm frame holds");

    // The hold advances on the shared 0.05 s step, across frames.
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    let second = wait(&app).unwrap();
    assert_ne!(second, first, "one step advances");
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert_ne!(
        wait(&app),
        Some(second),
        "the hold continues without a fresh press"
    );

    // Release: force stops and the cue clears.
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(!skip_on(&app), "release ends the force hold");
    assert_eq!(skip_cue_text(&mut app), "");
}

#[test]
fn tab_is_inert_during_a_force_hold_and_release_restores_seen() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = boot_app_with_input(root, None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);

    // The seen toggle is on first, so its survival through the hold is visible.
    tap_key(&mut app, KeyCode::Tab);
    assert!(app.world().resource::<crate::core::SkipState>().is_seen());

    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));

    // Tab is inert under the hold: the suspended seen toggle is unchanged.
    tap_key(&mut app, KeyCode::Tab);
    assert!(skip_force(&app), "the hold continues");
    assert!(
        app.world().resource::<crate::core::SkipState>().is_seen(),
        "the suspended seen toggle is untouched"
    );

    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(!skip_force(&app));
    assert!(skip_on(&app), "an eligible seen toggle resumes on release");
}

#[test]
fn force_holds_at_a_choice_and_resumes_after_the_pick() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));

    run_until_choice(&mut app);
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
    assert!(skip_force(&app), "the hold waits at the choice");
    assert_eq!(skip_cue_text(&mut app), "SKIP");

    // A digit still chooses while Ctrl is held; the hold continues.
    tap_key(&mut app, KeyCode::Digit1);
    assert!(skip_force(&app), "the pick does not end the hold");
    let line = wait(&app).unwrap();
    assert!(
        matches!(line, Wait::Say { .. }),
        "the choice landed on a say"
    );

    // Force resumes on the next say-park while the key is still down.
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert_ne!(wait(&app), Some(line), "force resumes after the park");
}

#[test]
fn a_choice_button_under_force_picks_and_the_hold_continues() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    run_until_choice(&mut app);
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
    assert!(skip_force(&app));

    let before = parked_position(&app);
    app.world_mut().spawn((
        crate::presentation::play::ChoiceButton { index: 0 },
        Interaction::Pressed,
    ));
    step_input(&mut app);
    assert!(skip_force(&app), "the button does not end the hold");
    assert_ne!(
        parked_position(&app),
        before,
        "the button performed exactly its own pick"
    );
}

#[test]
fn a_manual_advance_under_force_marks_its_arrival_and_never_double_steps() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let root = pack.path().join("pack");
    seed_read_lines(&root, &[("first_meeting", &[2])]);
    let mut app = boot_app_with_input(root, None);
    enter_play_with_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));
    let first = wait(&app).unwrap();

    // Let the first line's dwell run partway, then advance manually: the press
    // performs exactly its own one advance, marks the line it lands on, and the
    // hold resumes fresh at the new park rather than spending the old dwell.
    advance_time(&mut app, 0.04);
    step_input(&mut app);
    assert_eq!(wait(&app), Some(first.clone()), "0.04 s is short of a step");
    tap_key(&mut app, KeyCode::Space);
    assert!(skip_force(&app), "the press does not end the hold");
    assert_eq!(
        parked_position(&app).0,
        3,
        "the press advanced exactly once, to line 3"
    );
    assert!(
        read_state(&app).is_read("first_meeting", 3),
        "the manual arrival marks"
    );
}

#[test]
fn force_holds_at_a_decision_and_never_resolves_for_the_player() {
    let pack = pack_with_decision();
    let root = pack.path().join("pack");
    seed_read_lines(&root, &[("first_meeting", &[2])]);
    let fake = FakeResolver::new(model_answer("warm"));
    let mut app = boot_app_with_input(root, Some(Arc::new(fake.clone())));
    build_ui(&mut app);
    enter_play_with_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));

    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert!(matches!(wait(&app), Some(Wait::Decision { .. })));
    assert!(skip_force(&app), "the hold waits at the decision");
    assert!(
        app.world().resource::<PendingDecision>().0.is_some(),
        "force neither resolved nor cancelled the request"
    );

    // The request still answers; force resumes on the say the answer lands on.
    fake.release();
    pump_until(&mut app, |_| fake.calls() == 1);
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert!(skip_force(&app), "force resumes after the answer");
}

#[test]
fn a_force_advance_landing_on_a_decision_does_not_exempt_the_fallback_say() {
    let pack = pack_with_decision();
    let root = pack.path().join("pack");
    seed_read_lines(&root, &[("first_meeting", &[2])]);
    // No resolver: the decision resolves to its author fallback (`cold`) in the
    // same frame as the force advance.
    let mut app = boot_app_with_input(root, None);
    enter_play_with_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));

    advance_time(&mut app, 0.05);
    step_input(&mut app);
    // The advance landed on the decision, not a say, so its exemption is void:
    // the fallback's say (line 7) is an ordinary park and marks normally.
    assert!(
        read_state(&app).is_read("first_meeting", 7),
        "the fallback landing marks"
    );
    assert!(
        !read_state(&app).is_read("first_meeting", 3),
        "the decision line is no say and is never marked"
    );
}

#[test]
fn force_holds_at_the_end_of_the_scene() {
    let pack = pack_with_entry_says(&["Narrator: One."]);
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert!(wait(&app).is_none(), "the script ended");
    assert!(skip_force(&app), "force holds at the end of the scene");
    assert_eq!(skip_cue_text(&mut app), "SKIP");

    advance_time(&mut app, 1.0);
    step_input(&mut app);
    assert!(skip_force(&app));
    assert!(wait(&app).is_none());
}

#[test]
fn a_force_crossing_leaves_crossed_lines_unread_but_visits_the_scene() {
    let pack = pack_with_crossing();
    let root = pack.path().join("pack");
    // Only the entry line is known; the forced lines stay unread.
    seed_read_lines(&root, &[("first_meeting", &[2])]);
    let mut app = boot_app_with_input(root, None);
    enter_play_with_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));

    for _ in 0..8 {
        advance_time(&mut app, 0.05);
        step_input(&mut app);
        if wait(&app).is_none() {
            break;
        }
    }
    assert!(wait(&app).is_none(), "the script ended under the hold");

    let state = read_state(&app);
    assert!(state.is_visited("first_meeting"));
    assert!(state.is_visited("second_scene"), "the crossing is visited");
    assert!(
        state.is_read("first_meeting", 2),
        "the entry line was read before the hold"
    );
    assert!(
        !state.is_read("first_meeting", 3),
        "the forced line stays unread"
    );
    assert!(
        !state.is_read("second_scene", 1),
        "the forced crossing line stays unread"
    );
    assert!(
        !state.is_read("second_scene", 2),
        "the forced crossing line stays unread"
    );

    // The sibling bytes agree: the crossing's visited mark flushed, the forced
    // read marks did not.
    let disk = read_log_on_disk(&app).expect("the crossing flushed the log");
    assert!(disk.is_visited("second_scene"));
    assert!(!disk.is_read("first_meeting", 3));
    assert!(!disk.is_read("second_scene", 1));
}

#[test]
fn a_later_ordinary_park_marks_a_force_skipped_line() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let root = pack.path().join("pack");
    seed_read_lines(&root, &[("first_meeting", &[2])]);
    let mut app = boot_app_with_input(root, None);
    enter_play_with_input(&mut app);

    // Force one step onto line 3, then release.
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(!skip_force(&app));
    assert!(
        !read_state(&app).is_read("first_meeting", 3),
        "the forced arrival is unread"
    );
    assert_eq!(
        read_state(&app).arrival_read(),
        Some(false),
        "the forced park still evaluates its arrival status"
    );

    // Save on the forced line, advance manually, then load back: the load's
    // re-park is a later ordinary park and marks it.
    tap_key(&mut app, KeyCode::F5);
    tap_key(&mut app, KeyCode::Space);
    assert!(
        read_state(&app).is_read("first_meeting", 4),
        "the manual park marks"
    );
    assert!(
        !read_state(&app).is_read("first_meeting", 3),
        "the forced line stays unread until a later park"
    );
    tap_key(&mut app, KeyCode::F9);
    assert!(
        read_state(&app).is_read("first_meeting", 3),
        "the load's re-park marks the forced line"
    );
}

#[test]
fn an_ordinary_advance_that_loops_back_is_a_fresh_arrival() {
    // The label sits immediately before the say and the jump immediately
    // after, so an advance loops back to the exact same `(scene, pc)`.
    let pack = temp_custom_pack(
        &[(
            "a",
            "bg school_gate\nlabel again\nNarrator: One.\njump again\n",
        )],
        None,
        "a",
    );
    let mut app = app_for_pack(pack.path().join("pack"));
    build_ui(&mut app);
    assert_eq!(read_state(&app).arrival_read(), Some(false));
    press_and_update(&mut app, KeyCode::Tab);
    assert!(!skip_on(&app), "an unread arrival cannot engage");

    // The advance departs and re-enters the same coordinates; the second
    // arrival must recompute rather than inherit the first visit's status.
    press_and_update(&mut app, KeyCode::Space);
    assert_eq!(read_state(&app).arrival_read(), Some(true));
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app), "Tab engages on the re-entered, read line");
    assert_eq!(skip_cue_text(&mut app), "SKIP");
}

#[test]
fn a_forced_loop_landing_consumes_its_advance_exemption() {
    // `a` crosses into `b`; `b` jumps back to its own top, so a forced advance
    // inside `b` lands on the exact same coordinates.
    let pack = temp_custom_pack(
        &[
            ("a", "Narrator: A one.\njump b\n"),
            ("b", "Narrator: One.\njump b\n"),
        ],
        None,
        "a",
    );
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    enter_play_with_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));

    // The forced crossing into `b` is exempt: its landing line stays unread.
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert_eq!(app.world().resource::<Runtime>().runner.scene_id(), "b");
    assert_eq!(read_state(&app).arrival_read(), Some(false));
    assert!(!read_state(&app).is_read("b", 1));
    assert!(read_state(&app).is_visited("b"));

    // One more forced step loops back to the same coordinates; the landing
    // must consume that step's exemption even though the park key repeats.
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert!(skip_force(&app));
    assert!(!read_state(&app).is_read("b", 1));

    // Release and park ordinarily on that line: a stale exemption would
    // suppress this legitimate mark.
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    tap_key(&mut app, KeyCode::Space);
    assert!(
        read_state(&app).is_read("b", 1),
        "the ordinary re-park marks the line once no exemption is pending"
    );
}

#[test]
fn a_force_hold_adds_no_flush_and_writes_nothing_within_one_scene() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    enter_play_with_input(&mut app);
    let before = std::fs::read(read_log_path(&app)).expect("play entry flushed the log");
    assert!(!read_state(&app).is_dirty());

    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    for _ in 0..3 {
        advance_time(&mut app, 0.05);
        step_input(&mut app);
    }
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);

    assert!(!read_state(&app).is_dirty(), "force adds no read marks");
    assert_eq!(std::fs::read(read_log_path(&app)).unwrap(), before);
}

#[test]
fn an_armed_hold_survives_the_overlay_pause_without_catch_up() {
    let pack = pack_with_entry_says(&[
        "Narrator: One.",
        "Narrator: Two.",
        "Narrator: Three.",
        "Narrator: Four.",
    ]);
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));
    let line = wait(&app).unwrap();

    // Esc opens the overlay; the hold survives and the clock freezes.
    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open, "Esc opened the overlay");
    assert!(skip_force(&app), "the hold survives the pause");
    for _ in 0..3 {
        advance_time(&mut app, 1.0);
        step_input(&mut app);
    }
    assert_eq!(wait(&app), Some(line.clone()), "no step under the overlay");

    // Closing with the key still held resumes, without catch-up.
    advance_time(&mut app, 0.0);
    tap_key(&mut app, KeyCode::Escape);
    assert!(!menu(&app).open);
    assert!(skip_force(&app), "closing resumes the hold");
    assert_eq!(
        wait(&app),
        Some(line.clone()),
        "the frozen time is not caught up"
    );

    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert_ne!(wait(&app), Some(line), "a fresh step advances");
}

#[test]
fn esc_under_force_ends_seen_but_the_hold_survives() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = boot_app_with_input(root, None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);
    tap_key(&mut app, KeyCode::Tab);
    assert!(app.world().resource::<crate::core::SkipState>().is_seen());
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));

    // Esc ends the seen toggle (never latched behind the pause) while the
    // physical hold survives.
    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open, "Esc opened the overlay");
    assert!(!app.world().resource::<crate::core::SkipState>().is_seen());
    assert!(skip_force(&app), "the physical hold survives the pause");

    // Closing does not resurrect the toggle; the hold resumes.
    advance_time(&mut app, 0.0);
    tap_key(&mut app, KeyCode::Escape);
    assert!(!menu(&app).open);
    assert!(!app.world().resource::<crate::core::SkipState>().is_seen());
    assert!(skip_force(&app));
}

#[test]
fn force_suppresses_auto_and_release_restarts_a_full_auto_dwell() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = boot_app_with_input(root, None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);
    tap_key(&mut app, KeyCode::KeyA);
    assert!(auto_on(&app));

    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));
    assert_eq!(auto_cue_text(&mut app), "", "AUTO hides while skip runs");
    let first = wait(&app).unwrap();
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert_ne!(
        wait(&app),
        Some(first),
        "the 0.05 s skip dwell beats auto's 2.0 s"
    );

    // Release: auto's dwell restarts full, not with skip's remainder.
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(!skip_force(&app));
    let line = wait(&app).unwrap();
    advance_time(&mut app, 1.9);
    tick_auto(&mut app);
    assert_eq!(wait(&app), Some(line.clone()), "no remainder carried in");
    advance_time(&mut app, 0.2);
    tick_auto(&mut app);
    assert_ne!(wait(&app), Some(line), "a full 2.0 s after release");
}

#[test]
fn a_defensive_hold_blocks_arming_and_freezes_the_force_clock() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = boot_app_with_input(root, None);
    enter_play_with_input(&mut app);
    let line = wait(&app).unwrap();

    // A Ctrl press under a defensive hold is not armed, and the held key still
    // does not arm when the hold releases.
    app.world_mut().resource_mut::<DefensiveHold>().engage();
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(
        !skip_force(&app),
        "a press under a defensive hold is not armed"
    );
    app.world_mut().resource_mut::<DefensiveHold>().release();
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert!(
        !skip_force(&app),
        "the gate release does not arm a held key"
    );
    assert_eq!(wait(&app), Some(line.clone()));
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);

    // Armed in play, a defensive hold freezes the force clock.
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));
    app.world_mut().resource_mut::<DefensiveHold>().engage();
    for _ in 0..3 {
        advance_time(&mut app, 1.0);
        step_input(&mut app);
    }
    assert_eq!(wait(&app), Some(line.clone()), "no step under the hold");
    app.world_mut().resource_mut::<DefensiveHold>().release();
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert_ne!(wait(&app), Some(line), "the force resumes after the hold");
}

#[test]
fn a_ctrl_press_under_the_overlay_is_not_armed() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);

    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(!skip_force(&app), "a press under the overlay is not armed");

    // Closing with the key still held must not start a skip.
    tap_key(&mut app, KeyCode::Escape);
    assert!(!menu(&app).open);
    assert!(
        !skip_force(&app),
        "play resume does not arm an overlay press"
    );
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(!skip_force(&app));
}

#[test]
fn a_ctrl_press_on_the_title_is_not_armed() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two."]);
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    build_ui(&mut app);

    // Title is up: press and hold Ctrl there.
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(!skip_force(&app));

    enter_play_with_input(&mut app);
    assert!(!skip_force(&app), "a title press does not arm play skip");

    // Release and re-press in play arms it.
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app), "a fresh press in play arms");
}

#[test]
fn a_load_disarms_the_force_hold_and_needs_a_fresh_press() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = boot_app_with_input(root, None);
    enter_play_with_input(&mut app);
    tap_key(&mut app, KeyCode::F5);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));

    // F9 load disarms the hold while the key is still down.
    tap_key(&mut app, KeyCode::F9);
    assert!(!skip_force(&app), "a load disarms the hold");
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert!(!skip_force(&app), "a held but disarmed key stays stopped");

    // Release and re-press arms again.
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));
}

#[test]
fn an_overlay_load_disarms_the_force_hold() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = boot_app_with_input(root, None);
    enter_play_with_input(&mut app);
    tap_key(&mut app, KeyCode::F5);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));

    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open);
    click_input(&mut app, MenuAction::SetMode(MenuMode::Load));
    click_input(&mut app, MenuAction::Card(0));
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::Load(Slot::Manual(1))));
    click_input(&mut app, MenuAction::ConfirmAccept);
    assert!(!menu(&app).open, "the load closed the overlay");
    assert!(!skip_force(&app), "an overlay load disarms the hold");
}

#[test]
fn a_new_game_disarms_the_force_hold() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two."]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = boot_app_with_input(root, None);
    enter_play_with_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));

    tap_key(&mut app, KeyCode::Escape);
    click_input(&mut app, MenuAction::OpenSettings);
    click_input(&mut app, MenuAction::ExitToTitle);
    click_input(&mut app, MenuAction::ConfirmAccept);
    assert!(menu(&app).title_surface_up());
    click_input(&mut app, MenuAction::NewGame);
    click_input(&mut app, MenuAction::ConfirmAccept);
    assert!(!menu(&app).open, "New Game starts play");
    assert!(!skip_force(&app), "New Game disarms the hold");
}

#[test]
fn title_to_play_disarms_the_force_hold() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two."]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = boot_app_with_input(root, None);
    enter_play_with_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));

    tap_key(&mut app, KeyCode::Escape);
    click_input(&mut app, MenuAction::OpenSettings);
    click_input(&mut app, MenuAction::ExitToTitle);
    click_input(&mut app, MenuAction::ConfirmAccept);
    assert!(menu(&app).title_surface_up());
    click_input(&mut app, MenuAction::Continue);
    assert!(!menu(&app).open, "Continue resumes play");
    assert!(!skip_force(&app), "title→play disarms the hold");

    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app), "a fresh press in play re-arms");
}

#[test]
fn focus_loss_stops_the_force_hold() {
    use bevy::input::keyboard::KeyboardFocusLost;

    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = boot_app_with_input(root, None);
    enter_play_with_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));

    // The engine's forced release, not a new focus gate.
    app.world_mut().write_message(KeyboardFocusLost);
    step_input(&mut app);
    assert!(!skip_force(&app), "focus loss stops the hold");

    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app), "a fresh press re-arms");
}

#[test]
fn a_toggles_auto_without_ending_the_force_hold() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = boot_app_with_input(root, None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));
    assert!(!auto_on(&app));

    tap_key(&mut app, KeyCode::KeyA);
    assert!(auto_on(&app), "A toggles the Auto preference");
    assert!(skip_force(&app), "the hold continues");
    assert_eq!(
        auto_cue_text(&mut app),
        "",
        "AUTO is hidden while skip runs"
    );
    assert_eq!(skip_cue_text(&mut app), "SKIP");
}

#[test]
fn the_shared_skip_cue_follows_the_effective_state() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = boot_app_with_input(root, None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);

    assert_eq!(skip_cue_text(&mut app), "", "off");

    tap_key(&mut app, KeyCode::Tab);
    assert_eq!(skip_cue_text(&mut app), "SKIP", "seen");
    tap_key(&mut app, KeyCode::Tab);
    assert_eq!(skip_cue_text(&mut app), "", "seen off");

    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert_eq!(skip_cue_text(&mut app), "SKIP", "force");

    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert_eq!(skip_cue_text(&mut app), "", "a disarmed hold");

    tap_key(&mut app, KeyCode::KeyA);
    assert_eq!(auto_cue_text(&mut app), "AUTO", "auto");
    assert_eq!(skip_cue_text(&mut app), "", "AUTO and SKIP stay exclusive");
}

#[test]
fn the_cue_slot_and_dialogue_reservation_hold_in_all_four_modes() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = boot_app_with_input(root, None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();

    fn dialogue_node(app: &mut App) -> Node {
        app.world_mut()
            .query_filtered::<&Node, With<crate::presentation::play::DialogueText>>()
            .iter(app.world())
            .next()
            .cloned()
            .expect("the dialogue band exists")
    }

    // Off, seen, force and auto all keep the fixed 160 px right reservation.
    assert_eq!(dialogue_node(&mut app).right, Val::Px(160.0), "off");
    tap_key(&mut app, KeyCode::Tab);
    assert_eq!(dialogue_node(&mut app).right, Val::Px(160.0), "seen");
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert_eq!(dialogue_node(&mut app).right, Val::Px(160.0), "force");
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    tap_key(&mut app, KeyCode::KeyA);
    assert_eq!(dialogue_node(&mut app).right, Val::Px(160.0), "auto");

    // The cue nodes hold the approved slot and face in every state.
    for entity in [
        entity_with::<crate::presentation::play::SkipCue>(&app),
        entity_with::<crate::presentation::play::AutoCue>(&app),
    ] {
        let node = app.world().get::<Node>(entity).expect("cue node");
        assert_eq!(node.bottom, Val::Px(40.0));
        assert_eq!(node.right, Val::Px(48.0));
        let font = app.world().get::<TextFont>(entity).expect("cue font");
        assert_eq!(font.font_size, faces.mono(20.0).font_size);
    }

    // The help line sits top-left, clear of the bottom-right cue slot.
    let help = app
        .world_mut()
        .query_filtered::<&Node, With<HelpLine>>()
        .iter(app.world())
        .next()
        .cloned()
        .expect("the help line exists");
    assert_eq!(help.top, Val::Px(12.0));
    assert_eq!(help.left, Val::Px(16.0));
}

// ---------------------------------------------------------------------------
// The Timeline: browse visited scenes and jump
// ---------------------------------------------------------------------------

/// A copied demo pack whose manifest declares the given scene order. `None`
/// omits the field (no declared order); `Some(&[])` writes an explicit empty
/// list. The demo scenes stay as authored.
fn temp_pack_with_order(order: Option<&[&str]>) -> tempfile::TempDir {
    let dir = temp_pack();
    let root = dir.path().join("pack");
    let scenes = match order {
        None => String::new(),
        Some(list) => format!(
            "    scenes: [{}],\n",
            list.iter()
                .map(|id| format!("\"{id}\""))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    std::fs::write(
        root.join("pack.ron"),
        format!("(\n    name: \"unmei demo\",\n    entry: \"first_meeting\",\n{scenes})\n"),
    )
    .unwrap();
    dir
}

/// Open the overlay (a live session must be entered) and its Timeline door.
fn open_timeline(app: &mut App) {
    esc(app);
    assert!(menu(app).open, "the overlay opened");
    assert_eq!(menu(app).view, MenuView::Slots);
    click(app, MenuAction::OpenTimeline);
    assert_eq!(menu(app).view, MenuView::Timeline);
    build_pause_tree(app);
}

/// The built Timeline cards' labels in declared order. Builds the pause tree
/// first.
fn timeline_labels(app: &mut App) -> Vec<String> {
    build_pause_tree(app);
    let mut cards: Vec<(usize, String)> = app
        .world()
        .iter_entities()
        .filter_map(|entity| {
            let MenuAction::TimelineCard(index) = entity.get::<MenuAction>()? else {
                return None;
            };
            let text = entity.get::<Text>()?;
            Some((*index, text.0.clone()))
        })
        .collect();
    cards.sort_by_key(|(index, _)| *index);
    cards.into_iter().map(|(_, label)| label).collect()
}

/// An app already in play over a demo pack declaring `order`.
fn timeline_app(order: &[&str]) -> (App, tempfile::TempDir) {
    let pack = temp_pack_with_order(Some(order));
    let root = pack.path().join("pack");
    let app = app_for_pack(root);
    (app, pack)
}

#[test]
fn timeline_replaces_absent_scenes_with_the_exact_empty_copy_and_keeps_the_door() {
    for order in [None, Some(&[][..])] {
        let pack = temp_pack_with_order(order);
        let mut app = app_for_pack(pack.path().join("pack"));
        esc(&mut app);
        build_pause_tree(&mut app);
        assert!(
            has_action(&app, MenuAction::OpenTimeline),
            "the door is enabled without an order"
        );
        click(&mut app, MenuAction::OpenTimeline);
        assert_eq!(menu(&app).view, MenuView::Timeline);
        build_pause_tree(&mut app);
        assert!(
            !has_action(&app, MenuAction::TimelineCard(0)),
            "no cards render"
        );
        let texts = all_texts(&app);
        assert!(
            texts.iter().any(|t| t == menu::TIMELINE_EMPTY),
            "the exact empty copy renders: {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t == menu::TIMELINE_EXPLANATION),
            "the explanation renders"
        );

        // Back returns to the slot grid, and Resume closes the overlay.
        click(&mut app, MenuAction::TimelineBack);
        assert_eq!(menu(&app).view, MenuView::Slots);
        click(&mut app, MenuAction::Resume);
        assert!(!menu(&app).open);
    }
}

#[test]
fn every_declared_scene_renders_once_in_order_and_unlisted_scenes_do_not() {
    // `reunion` is loaded but unlisted, so it never renders.
    let (mut app, _pack) = timeline_app(&["first_meeting", "epilogue"]);
    open_timeline(&mut app);
    assert_eq!(timeline_labels(&mut app), vec!["first_meeting", "???"]);
    assert!(!has_action(&app, MenuAction::TimelineCard(2)));
}

#[test]
fn focus_starts_at_the_first_visited_declared_card_otherwise_the_first_declared() {
    // Entry `a` is unlisted and the only scene play visits; `c` is seeded
    // visited, so the first selectable declared card is `c`.
    let pack = temp_custom_pack(
        &[("a", "A: entry.\n"), ("b", "B: b.\n"), ("c", "C: c.\n")],
        Some(&["b", "c"]),
        "a",
    );
    let root = pack.path().join("pack");
    seed_visited(&root, &["c"]);
    let mut app = app_for_pack(root);
    open_timeline(&mut app);
    assert_eq!(menu(&app).focus, MenuFocus::TimelineCard(1));
    assert_eq!(timeline_labels(&mut app), vec!["???", "c"]);

    // All declared cards locked: focus falls to the first declared card, which
    // is still focusable though activation is inert.
    let pack = temp_custom_pack(
        &[("a", "A: entry.\n"), ("b", "B: b.\n"), ("c", "C: c.\n")],
        Some(&["b", "c"]),
        "a",
    );
    let mut app = app_for_pack(pack.path().join("pack"));
    open_timeline(&mut app);
    assert_eq!(menu(&app).focus, MenuFocus::TimelineCard(0));
    assert_eq!(timeline_labels(&mut app), vec!["???", "???"]);
}

#[test]
fn reopening_forgets_the_previous_focus() {
    let (mut app, _pack) = timeline_app(&["first_meeting", "reunion", "epilogue"]);
    open_timeline(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(2);
    click(&mut app, MenuAction::TimelineBack);
    assert_eq!(menu(&app).view, MenuView::Slots);
    assert_eq!(menu(&app).focus, MenuFocus::Card(0));
    click(&mut app, MenuAction::OpenTimeline);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::TimelineCard(0),
        "focus starts afresh at the first selectable card"
    );
}

#[test]
fn timeline_move_steps_plus_minus_one_and_two_and_clamps_without_wrapping() {
    // Horizontal ±1.
    assert_eq!(menu::timeline_move(3, 5, menu::GalleryDir::Left), Some(2));
    assert_eq!(menu::timeline_move(3, 5, menu::GalleryDir::Right), Some(4));
    // Vertical ±2.
    assert_eq!(menu::timeline_move(3, 5, menu::GalleryDir::Up), Some(1));
    assert_eq!(menu::timeline_move(1, 5, menu::GalleryDir::Down), Some(3));
    // Clamp at both ends, no wrap.
    assert_eq!(menu::timeline_move(0, 5, menu::GalleryDir::Left), None);
    assert_eq!(menu::timeline_move(0, 5, menu::GalleryDir::Up), None);
    assert_eq!(menu::timeline_move(4, 5, menu::GalleryDir::Right), None);
    assert_eq!(menu::timeline_move(4, 5, menu::GalleryDir::Down), None);
    // Diagonal end clamping is intentional: index 1 up lands on 0.
    assert_eq!(menu::timeline_move(1, 5, menu::GalleryDir::Up), Some(0));
    // Empty list has no move.
    assert_eq!(menu::timeline_move(0, 0, menu::GalleryDir::Left), None);
}

#[test]
fn the_walk_includes_locked_cards_and_activation_of_a_locked_card_is_inert() {
    let pack = temp_custom_pack(
        &[("a", "A: entry.\n"), ("b", "B: b.\n"), ("c", "C: c.\n")],
        Some(&["b", "c"]),
        "a",
    );
    let mut app = app_for_pack(pack.path().join("pack"));
    open_timeline(&mut app);
    let before = parked_position(&app);

    // Both cards are locked; the walk still reaches the next one.
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::TimelineCard(1));

    // Enter, Space and click are all inert on a locked card.
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, None);
    press_and_update(&mut app, KeyCode::Space);
    assert_eq!(menu(&app).confirm, None);
    click(&mut app, MenuAction::TimelineCard(1));
    assert_eq!(menu(&app).confirm, None);
    assert_eq!(parked_position(&app), before, "the session is untouched");
    assert_eq!(menu(&app).view, MenuView::Timeline);
}

#[test]
fn the_confirm_copy_is_exact_and_uses_jump_and_cancel() {
    let (mut app, _pack) = timeline_app(&["first_meeting", "reunion", "epilogue"]);
    open_timeline(&mut app);
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(
        menu(&app).confirm,
        Some(ConfirmKind::TimelineJump(0)),
        "the visited current card confirms"
    );
    build_pause_tree(&mut app);
    let texts = all_texts(&app);
    assert!(
        texts.iter().any(|t| t == "Jump to first_meeting?"),
        "the title names the raw target: {texts:?}"
    );
    assert!(
        texts.iter().any(|t| t == menu::TIMELINE_CONFIRM_DETAIL),
        "the detail is verbatim"
    );
    assert!(
        texts.iter().any(|t| t == "Jump"),
        "the accept label is Jump"
    );
    assert!(texts.iter().any(|t| t == "Cancel"));

    // A raw scene id is one unbroken token; the title opts into character
    // fallback so a long id wraps inside the 420px panel instead of
    // overflowing it (`docs/title-and-menu.md` § Timeline §3).
    let title = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            let text = entity.get::<Text>()?;
            (text.0 == "Jump to first_meeting?").then_some(entity.id())
        })
        .expect("the confirm title node");
    assert_eq!(
        app.world().get::<TextLayout>(title).map(|l| l.linebreak),
        Some(LineBreak::WordOrCharacter)
    );
}

#[test]
fn cancelling_the_confirm_restores_the_target_focus_and_touches_nothing() {
    // Seed `epilogue` visited so the confirm targets a non-default third card,
    // proving the cancel restores the target rather than the first card.
    let pack = temp_pack_with_order(Some(&["first_meeting", "reunion", "epilogue"]));
    let root = pack.path().join("pack");
    seed_visited(&root, &["epilogue"]);
    let mut app = app_for_pack(root);
    open_timeline(&mut app);
    let before = parked_position(&app);
    let log_before = std::fs::read(read_log_path(&app)).unwrap_or_default();

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(2);
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::TimelineJump(2)));
    press_and_update(&mut app, KeyCode::Escape);
    assert_eq!(menu(&app).confirm, None);
    assert_eq!(menu(&app).focus, MenuFocus::TimelineCard(2));
    assert_eq!(menu(&app).view, MenuView::Timeline);
    assert_eq!(parked_position(&app), before);
    assert_eq!(
        std::fs::read(read_log_path(&app)).unwrap_or_default(),
        log_before,
        "cancelling writes no file"
    );
}

#[test]
fn cancelling_the_confirm_restores_the_scrolled_body_offset() {
    let pack = temp_pack_with_many_entries(30, false);
    let mut app = app_for_pack(pack.path().join("pack"));
    open_timeline(&mut app);
    build_pause_tree(&mut app);

    // Scroll the body far down, as the arrow walk to a late card would.
    let body = entity_with::<crate::presentation::timeline::TimelineBody>(&app);
    let scrolled = ScrollPosition(Vec2::new(0.0, 860.0));
    app.world_mut().entity_mut(body).insert(scrolled);

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(29);
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::TimelineJump(29)));
    build_pause_tree(&mut app);
    let body = entity_with::<crate::presentation::timeline::TimelineBody>(&app);
    assert_eq!(
        app.world()
            .get::<ScrollPosition>(body)
            .map(|position| position.0),
        Some(Vec2::new(0.0, 860.0)),
        "the card behind the confirm keeps its offset"
    );

    press_and_update(&mut app, KeyCode::Escape);
    assert_eq!(menu(&app).confirm, None);
    assert_eq!(menu(&app).focus, MenuFocus::TimelineCard(29));
    build_pause_tree(&mut app);
    let body = entity_with::<crate::presentation::timeline::TimelineBody>(&app);
    assert_eq!(
        app.world()
            .get::<ScrollPosition>(body)
            .map(|position| position.0),
        Some(Vec2::new(0.0, 860.0)),
        "Cancel restores the scroll the walk had"
    );

    // The carry is confirm-only: a fresh open still starts at the top.
    click(&mut app, MenuAction::TimelineBack);
    build_pause_tree(&mut app);
    click(&mut app, MenuAction::OpenTimeline);
    build_pause_tree(&mut app);
    let body = entity_with::<crate::presentation::timeline::TimelineBody>(&app);
    assert_eq!(
        app.world()
            .get::<ScrollPosition>(body)
            .map(|position| position.0),
        Some(Vec2::ZERO),
        "reopening forgets the previous scroll"
    );
}

/// The coordinator's rebuild boundary (ADR 0013). A focus move or a credential
/// label change repaints in place and never rebuilds the tree; a structural
/// change rebuilds it; and only a confirmation change carries the Timeline
/// body's scroll offset across that rebuild.
#[test]
fn the_coordinator_rebuilds_only_on_structure_and_carries_scroll_on_confirm() {
    let pack = temp_pack_with_many_entries(30, false);
    let mut app = app_for_pack(pack.path().join("pack"));
    app.init_resource::<CredentialsState>();
    app.world_mut().resource_mut::<MenuState>().open = true;
    app.world_mut().resource_mut::<MenuState>().view = MenuView::Settings;
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::AudioMaster;
    build_pause_tree(&mut app);
    let settings_root = entity_with::<crate::presentation::MenuRoot>(&app);

    // Focus is not part of the signature: the border repaints, the tree stays.
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::KeyRow;
    build_pause_tree(&mut app);
    assert_eq!(
        entity_with::<crate::presentation::MenuRoot>(&app),
        settings_root,
        "a focus move repaints in place"
    );

    // A credential label change is realized by the view's own in-place sync, so
    // the coordinator does not rebuild for it.
    app.world_mut().resource_mut::<CredentialsState>().row = KeyRow::Set {
        fingerprint: "9F2A".to_string(),
    };
    build_pause_tree(&mut app);
    assert_eq!(
        entity_with::<crate::presentation::MenuRoot>(&app),
        settings_root,
        "a credential-label change repaints in place"
    );
    app.world_mut()
        .run_system_once(crate::presentation::settings_view::sync_key_row)
        .expect("sync_key_row runs");
    assert!(
        app.world().iter_entities().any(|entity| {
            entity
                .get::<Text>()
                .is_some_and(|text| text.0 == "Set \u{2014} 9F2A")
        }),
        "the row repaints the new label in place"
    );

    // A structural change rebuilds the tree.
    app.world_mut().resource_mut::<MenuState>().view = MenuView::Timeline;
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(29);
    build_pause_tree(&mut app);
    let timeline_root = entity_with::<crate::presentation::MenuRoot>(&app);
    assert_ne!(
        timeline_root, settings_root,
        "a structural change rebuilds the tree"
    );

    // Only a confirmation change carries the body's offset across the rebuild:
    // a walk inside the Timeline leaves both the tree and the offset alone.
    let body = entity_with::<crate::presentation::timeline::TimelineBody>(&app);
    app.world_mut()
        .entity_mut(body)
        .insert(ScrollPosition(Vec2::new(0.0, 860.0)));
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(28);
    build_pause_tree(&mut app);
    assert_eq!(
        entity_with::<crate::presentation::MenuRoot>(&app),
        timeline_root,
        "the walk repaints without a rebuild"
    );

    app.world_mut().resource_mut::<MenuState>().confirm = Some(ConfirmKind::TimelineJump(28));
    build_pause_tree(&mut app);
    assert_ne!(
        entity_with::<crate::presentation::MenuRoot>(&app),
        timeline_root,
        "the confirmation rebuilds the tree"
    );
    let body = entity_with::<crate::presentation::timeline::TimelineBody>(&app);
    assert_eq!(
        app.world()
            .get::<ScrollPosition>(body)
            .map(|position| position.0),
        Some(Vec2::new(0.0, 860.0)),
        "the confirmation carries the body's scroll offset"
    );
}

#[test]
fn esc_winds_the_timeline_back_to_slots_then_resumes() {
    let (mut app, _pack) = timeline_app(&["first_meeting", "reunion"]);
    open_timeline(&mut app);
    esc(&mut app);
    assert_eq!(menu(&app).view, MenuView::Slots);
    assert_eq!(menu(&app).focus, MenuFocus::Card(0));
    assert!(menu(&app).open);
    esc(&mut app);
    assert!(!menu(&app).open);
}

#[test]
fn a_jump_restores_the_target_from_pc_zero_with_carried_staging_and_state() {
    let pack = temp_custom_pack(
        &[
            (
                "a",
                "set affinity_level = \"high\"\nbg gate\nplay music_a\nA: one.\n",
            ),
            ("b", "A: two.\n"),
        ],
        Some(&["a", "b"]),
        "a",
    );
    let root = pack.path().join("pack");
    seed_visited(&root, &["b"]);
    let mut app = app_for_pack(root);
    // State and staging from scene `a` are carried into the jump.
    {
        let runtime = app.world().resource::<Runtime>();
        assert_eq!(runtime.runner.scene_id(), "a");
        assert_eq!(runtime.staging.background.as_deref(), Some("gate"));
        assert_eq!(runtime.staging.bgm.as_deref(), Some("music_a"));
    }

    open_timeline(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(1);
    // The jump is a scene entry, so it announces the scene it lands in; a
    // single-threaded `Update` keeps the log record on this test's thread.
    captures::install();
    force_single_threaded(&mut app);
    captures::take();
    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter); // accept

    let records = captures::take();
    assert!(
        records
            .iter()
            .any(|(_, _, message)| message == "unmei: entered scene b"),
        "the confirmed jump is a scene entry: {records:?}"
    );
    let runtime = app.world().resource::<Runtime>();
    assert_eq!(runtime.runner.scene_id(), "b");
    let Some(Wait::Say { text, .. }) = runtime.runner.wait() else {
        panic!(
            "expected a say on the landing, got {:?}",
            runtime.runner.wait()
        );
    };
    assert_eq!(text, "two.");
    assert_eq!(
        runtime.staging.background.as_deref(),
        Some("gate"),
        "the target's absent bg keeps the carried staging"
    );
    assert_eq!(runtime.staging.bgm.as_deref(), Some("music_a"));
    assert_eq!(
        runtime.runner.state().get("affinity_level"),
        Some(&StateValue::Str("high".to_string())),
        "the state map is carried"
    );
    assert!(!menu(&app).open, "the overlay closes on the landing");
}

#[test]
fn a_jump_cancels_a_pending_request_and_discards_a_held_answer() {
    // Pending: the request is in flight when the jump is confirmed.
    let fake = FakeResolver::new(model_answer("warm"));
    let pack = temp_pack_with_order(Some(&["first_meeting", "reunion"]));
    let dst = pack.path().join("pack");
    let mut app = app_with_decisions_at(dst, Some(Arc::new(fake.clone())));
    {
        let mut runtime = app.world_mut().resource_mut::<Runtime>();
        drive_runner_to_decision(&mut runtime);
    }
    app.update();
    pump_until(&mut app, |_| fake.calls() == 1);
    assert!(app.world().resource::<PendingDecision>().0.is_some());

    open_timeline(&mut app);
    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter); // accept
    assert!(app.world().resource::<PendingDecision>().0.is_none());
    fake.wait_until_dropped();
    assert!(app.world().resource::<HeldDecision>().0.is_none());

    // Held: the request answered behind the overlay, then the jump discards it.
    let fake = FakeResolver::new(model_answer("warm"));
    let pack = temp_pack_with_order(Some(&["first_meeting", "reunion"]));
    let dst = pack.path().join("pack");
    let mut app = app_with_decisions_at(dst, Some(Arc::new(fake.clone())));
    {
        let mut runtime = app.world_mut().resource_mut::<Runtime>();
        drive_runner_to_decision(&mut runtime);
    }
    app.update();
    pump_until(&mut app, |_| fake.calls() == 1);
    esc(&mut app);
    fake.release();
    pump_until(&mut app, held);
    assert!(held(&app));

    click(&mut app, MenuAction::OpenTimeline);
    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter); // accept
    assert!(!held(&app), "the held answer is discarded");
    assert!(
        !autosave_path(&app).exists(),
        "an old answer never captures"
    );
}

#[test]
fn a_held_ctrl_through_a_timeline_jump_disarms_and_needs_a_fresh_press() {
    let pack = temp_pack_with_order(Some(&["first_meeting", "reunion"]));
    let root = pack.path().join("pack");
    seed_visited(&root, &["reunion"]);
    let mut app = boot_app_with_input(root, None);
    enter_play_with_input(&mut app);

    // A genuine physical hold, armed in play.
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));
    app.world_mut().resource_mut::<Notice>().show("something");

    // Keep Ctrl down through the overlay, the Timeline door, the confirm and
    // the jump — the real keyboard path, not a `reset_all` simulation.
    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open);
    assert!(skip_force(&app), "a hold survives the overlay pause");
    click_input(&mut app, MenuAction::OpenTimeline);
    assert_eq!(menu(&app).view, MenuView::Timeline);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(1);
    click_input(&mut app, MenuAction::TimelineCard(1));
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::TimelineJump(1)));
    click_input(&mut app, MenuAction::ConfirmAccept);

    // The landing disarms both modes: the still-physically-down Ctrl cannot
    // skip the replacement position, even after further frames.
    assert!(!menu(&app).open, "the jump closed the overlay");
    assert_eq!(
        app.world().resource::<Runtime>().runner.scene_id(),
        "reunion"
    );
    assert!(!skip_force(&app), "the held Ctrl is disarmed by the jump");
    assert!(!skip_on(&app));
    assert!(
        app.world().resource::<Notice>().text.is_none(),
        "the old Notice is cleared"
    );
    let landed = wait(&app).expect("the landing parked on a say");
    for _ in 0..3 {
        advance_time(&mut app, 0.05);
        step_input(&mut app);
    }
    assert_eq!(
        wait(&app),
        Some(landed.clone()),
        "no skip runs while Ctrl is still down"
    );

    // Release and repress in play: only a fresh press arms.
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(!skip_force(&app));
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app), "a fresh press in play re-arms");
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert_ne!(
        wait(&app),
        Some(landed),
        "the fresh hold skips the landing line"
    );
}

#[test]
fn a_same_scene_restart_recomputes_arrival_and_writes_nothing_when_clean() {
    let (mut app, _pack) = timeline_app(&["first_meeting", "reunion", "epilogue"]);
    // The entry line was marked and flushed on play entry, so the log is clean.
    let before = std::fs::read(read_log_path(&app)).unwrap();
    open_timeline(&mut app);
    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter); // accept, restart first_meeting

    let runtime = app.world().resource::<Runtime>();
    assert_eq!(runtime.runner.scene_id(), "first_meeting");
    assert!(matches!(runtime.runner.wait(), Some(Wait::Say { .. })));
    assert_eq!(
        read_state(&app).arrival_read(),
        Some(true),
        "the landing line was read before this visit"
    );
    assert_eq!(
        std::fs::read(read_log_path(&app)).unwrap(),
        before,
        "a clean same-scene restart writes no new knowledge"
    );
}

#[test]
fn a_jump_lands_only_on_its_displayed_line_and_leaves_the_rest_unread() {
    // Scene `b` is visited but never read, so its landing marks only its first
    // displayed line.
    let pack = temp_custom_pack(
        &[("a", "A: entry.\n"), ("b", "B: first.\nB: second.\n")],
        Some(&["a", "b"]),
        "a",
    );
    let root = pack.path().join("pack");
    seed_visited(&root, &["b"]);
    let mut app = app_for_pack(root);
    open_timeline(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(1);
    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter); // accept

    let state = read_state(&app);
    assert_eq!(state.arrival_read(), Some(false));
    assert_eq!(
        state.log.read.get("b"),
        // Line 1 is the first authored line of scene `b`; no directive precedes
        // it, so the landing say-line's authored source line is 1.
        Some(&std::collections::BTreeSet::from([1])),
        "only the displayed landing line is marked"
    );
    assert!(state.is_visited("b"));
}

#[test]
fn a_restore_refusal_stays_on_the_timeline_and_preserves_the_session() {
    captures::install();
    let pack = temp_pack_with_order(Some(&["first_meeting", "reunion"]));
    let mut app = app_for_pack(pack.path().join("pack"));
    force_single_threaded(&mut app);
    open_timeline(&mut app);

    // Simulate a pack whose declared order names a scene it does not contain:
    // the loader validates this away in normal play, so install the mutated
    // pack directly to reach the defensive restore path.
    let mut mutated = app.world().resource::<Runtime>().runner.pack().clone();
    mutated.manifest.scenes.insert(0, "ghost".to_string());
    app.world_mut().resource_mut::<Runtime>().runner = ScriptRunner::new(mutated);
    app.world_mut()
        .resource_mut::<crate::read_log::ReadLogState>()
        .log
        .mark_visited("ghost");

    // Re-open so the confirm targets the ghost card.
    click(&mut app, MenuAction::TimelineBack);
    click(&mut app, MenuAction::OpenTimeline);
    assert_eq!(menu(&app).focus, MenuFocus::TimelineCard(0));
    let before = parked_position(&app);
    captures::take();
    app.world_mut().resource_mut::<Notice>().text = None;

    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter); // accept the refused jump

    assert_eq!(menu(&app).view, MenuView::Timeline, "Timeline stays open");
    assert_eq!(
        parked_position(&app),
        before,
        "the old runner position is preserved"
    );
    let records = failure_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert!(
        records[0].2.starts_with("the story can't continue"),
        "the defensive funnel's existing string: {:?}",
        records[0].2
    );
    assert!(
        app.world().resource::<DefensiveHold>().is_held(),
        "a refused restore engages the defensive hold"
    );
    assert_eq!(
        app.world().resource::<Notice>().text.as_deref(),
        Some("the story can't continue — Esc for the menu")
    );
}

#[test]
fn a_jump_selection_writes_no_autosave_even_with_a_stale_pending_flag() {
    let pack = temp_custom_pack(
        &[("a", "A: entry.\n"), ("b", "B: two.\n")],
        Some(&["a", "b"]),
        "a",
    );
    let root = pack.path().join("pack");
    seed_visited(&root, &["b"]);
    let mut app = app_for_pack(root);

    open_timeline(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(1);
    press_and_update(&mut app, KeyCode::Enter); // open the confirm
                                                // `write_pending_autosave` consumes an armed flag on the next frame, so
                                                // arming it before the jump would legitimately write. Arming it in the same
                                                // frame as the accept proves the jump's disarm runs before that writer.
    app.world_mut()
        .resource_mut::<crate::slots::PendingAutosave>()
        .arm();
    click(&mut app, MenuAction::ConfirmAccept);
    assert!(
        !autosave_path(&app).exists(),
        "the selection itself writes no autosave"
    );
}

#[test]
fn a_jump_leaves_a_pre_jump_autosave_untouched_then_a_real_crossing_writes() {
    // Play the demo to the reunion crossing so an autosave exists.
    let pack = temp_pack_with_order(Some(&["first_meeting", "reunion", "epilogue"]));
    let root = pack.path().join("pack");
    let mut app = app_for_pack(root);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Digit1);
    press_and_update(&mut app, KeyCode::Space); // cross into reunion
    let autosave_before = std::fs::read(autosave_path(&app)).expect("a crossing autosaved");

    // Jump back to the visited entry scene; the autosave is untouched.
    open_timeline(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(0);
    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter); // accept
    assert_eq!(
        std::fs::read(autosave_path(&app)).unwrap(),
        autosave_before,
        "the selection writes no autosave"
    );

    // A later normal crossing still writes: remove the pre-jump file and prove
    // the crossing recreates it (its bytes may coincide within the same second).
    std::fs::remove_file(autosave_path(&app)).unwrap();
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Digit1);
    press_and_update(&mut app, KeyCode::Space); // jump to reunion
    app.update();
    assert!(
        autosave_path(&app).exists(),
        "a normal authored crossing still autosaves"
    );
    let after = save::read_slot(
        app.world().resource::<Runtime>().runner.pack(),
        Slot::Autosave,
    )
    .expect("a real autosave content");
    assert_eq!(after.scene_id, "reunion");
}

#[test]
fn a_same_scene_restart_flushes_pending_knowledge_to_the_sibling_file() {
    let pack = temp_pack_with_order(Some(&["first_meeting", "reunion"]));
    let mut app = app_for_pack(pack.path().join("pack"));

    // The play-entry flush wrote the first line; the next park's mark is dirty
    // because an in-scene advance is not a flush moment.
    press_and_update(&mut app, KeyCode::Space);
    assert!(read_state(&app).is_read("first_meeting", 10));
    assert!(read_state(&app).is_dirty());
    assert!(
        !read_log_on_disk(&app).unwrap().is_read("first_meeting", 10),
        "the second line's mark is not on disk yet"
    );

    // A same-scene restart is a scene-entry flush opportunity of its own.
    open_timeline(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(0);
    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter); // accept
    assert_eq!(
        app.world().resource::<Runtime>().runner.scene_id(),
        "first_meeting"
    );
    assert_eq!(read_state(&app).arrival_read(), Some(true));
    assert!(!read_state(&app).is_dirty(), "the landing flushed");
    assert!(
        read_log_on_disk(&app).unwrap().is_read("first_meeting", 10),
        "the same-scene landing wrote the pending knowledge"
    );
}

#[test]
fn a_different_scene_landing_flushes_and_a_failed_flush_retries() {
    let pack = temp_pack_with_order(Some(&["first_meeting", "reunion"]));
    let root = pack.path().join("pack");
    seed_visited(&root, &["reunion"]);
    let mut app = app_for_pack(root);

    open_timeline(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(1);

    // Block the flush: the temp sibling's path is a directory, so the write
    // fails after the landing's mark.
    let tmp = read_log_path(&app).with_extension("ron.tmp");
    std::fs::create_dir_all(&tmp).unwrap();
    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter); // accept

    assert_eq!(
        app.world().resource::<Runtime>().runner.scene_id(),
        "reunion"
    );
    assert!(
        read_state(&app).is_read("reunion", 7),
        "the landing marked its displayed line"
    );
    assert!(read_state(&app).is_dirty(), "a failed flush keeps the flag");
    assert!(
        !read_log_on_disk(&app).unwrap().is_read("reunion", 7),
        "the previous bytes stay"
    );

    // Unblock and reach the next flush moment (app exit): the retry succeeds.
    std::fs::remove_dir_all(&tmp).unwrap();
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<AppExit>>()
        .write(AppExit::Success);
    app.update();
    assert!(!read_state(&app).is_dirty());
    assert!(
        read_log_on_disk(&app).unwrap().is_read("reunion", 7),
        "the retry wrote the landing knowledge"
    );
}

#[test]
fn a_jump_then_quit_confirms_continue_loads_the_pre_jump_autosave() {
    // Play the demo to the reunion crossing so an autosave exists.
    let pack = temp_pack_with_order(Some(&["first_meeting", "reunion", "epilogue"]));
    let root = pack.path().join("pack");
    let mut app = app_for_pack(root.clone());
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Digit1);
    press_and_update(&mut app, KeyCode::Space); // cross into reunion
    let autosave_before = std::fs::read(autosave_path(&app)).expect("a crossing autosaved");

    // Jump back to the entry scene; the selection writes nothing.
    open_timeline(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(0);
    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter); // accept
    assert_eq!(
        app.world().resource::<Runtime>().runner.scene_id(),
        "first_meeting"
    );
    assert_eq!(std::fs::read(autosave_path(&app)).unwrap(), autosave_before);

    // Quit, relaunch over the same pack and pick Continue: the pre-jump
    // autosave — not the Timeline landing — is what resumes.
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<AppExit>>()
        .write(AppExit::Success);
    app.update();
    drop(app);

    let mut relaunched = boot_app(root, None);
    assert!(menu(&relaunched).continue_enabled, "the autosave loads");
    relaunched.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
    press_and_update(&mut relaunched, KeyCode::Enter);
    assert!(!menu(&relaunched).open, "Continue entered play");
    assert_eq!(
        relaunched.world().resource::<Runtime>().runner.scene_id(),
        "reunion",
        "Continue resumes the pre-jump autosave, not the Timeline landing"
    );
}

#[test]
fn opening_walking_and_backing_out_of_the_timeline_writes_nothing() {
    let (mut app, _pack) = timeline_app(&["first_meeting", "reunion", "epilogue"]);
    let before = parked_position(&app);
    let log_before = std::fs::read(read_log_path(&app)).unwrap_or_default();
    assert!(!autosave_path(&app).exists());

    open_timeline(&mut app);
    press_and_update(&mut app, KeyCode::ArrowDown);
    press_and_update(&mut app, KeyCode::ArrowRight);
    click(&mut app, MenuAction::TimelineBack);
    esc(&mut app);
    assert!(!menu(&app).open);
    assert_eq!(parked_position(&app), before);
    assert_eq!(
        std::fs::read(read_log_path(&app)).unwrap_or_default(),
        log_before
    );
    assert!(!autosave_path(&app).exists());
}

#[test]
fn the_timeline_opens_at_a_choice_wait_and_freezes_the_request() {
    let (mut app, _pack) = timeline_app(&["first_meeting", "reunion", "epilogue"]);
    // Entry say -> Ace say -> the choice wait.
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
    let before = parked_position(&app);
    open_timeline(&mut app);
    let focus_before = menu(&app).focus;
    assert_eq!(
        parked_position(&app),
        before,
        "the choice park is untouched"
    );
    // A digit is inert in the Timeline view: neither the park nor the focus
    // moves.
    press_and_update(&mut app, KeyCode::Digit1);
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
    assert_eq!(menu(&app).focus, focus_before);
}

#[test]
fn timeline_cards_use_the_approved_geometry_and_identical_focus_ring() {
    let (mut app, _pack) = timeline_app(&["first_meeting", "reunion", "epilogue"]);
    open_timeline(&mut app);
    build_pause_tree(&mut app);

    let cards: Vec<Entity> = app
        .world()
        .iter_entities()
        .filter_map(|entity| {
            matches!(
                entity.get::<MenuAction>(),
                Some(MenuAction::TimelineCard(_))
            )
            .then_some(entity.id())
        })
        .collect();
    assert_eq!(cards.len(), 3);
    for card in &cards {
        let node = app.world().get::<Node>(*card).expect("card node");
        assert_eq!(node.height, Val::Px(136.0));
        assert_eq!(node.padding, UiRect::all(Val::Px(18.0)));
        assert_eq!(node.border, UiRect::all(Val::Px(2.0)));
        // A raw scene id is one unbroken token: every label opts into the
        // character fallback so a long id wraps rather than truncating
        // (`docs/title-and-menu.md` § Timeline `### Layout`).
        assert_eq!(
            app.world().get::<TextLayout>(*card).map(|l| l.linebreak),
            Some(LineBreak::WordOrCharacter)
        );
    }

    // The focused card carries the reserved orange ring; an unfocused card has
    // none, and locked and visited share one fill.
    let focused_border = app
        .world()
        .get::<BorderColor>(cards[0])
        .expect("card border")
        .top;
    assert_eq!(focused_border, crate::presentation::style::FOCUS);
    let locked_border = app
        .world()
        .get::<BorderColor>(cards[1])
        .expect("card border")
        .top;
    assert_eq!(locked_border, Color::NONE, "an unfocused card has no ring");
    assert_eq!(
        app.world().get::<BackgroundColor>(cards[0]).unwrap().0,
        app.world().get::<BackgroundColor>(cards[1]).unwrap().0,
        "locked and visited share one fill"
    );

    // The grid is two equal columns with 16px gaps; the body scrolls.
    let grid = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<crate::presentation::timeline::TimelineGrid>()
                .map(|_| entity.id())
        })
        .expect("the Timeline grid");
    let grid_node = app.world().get::<Node>(grid).unwrap();
    assert_eq!(grid_node.column_gap, Val::Px(16.0));
    assert_eq!(grid_node.row_gap, Val::Px(16.0));
    assert_eq!(
        grid_node.grid_template_columns,
        vec![RepeatedGridTrack::flex(2, 1.0)]
    );

    // The body lets the grid keep its natural height: a stretched or shrunk
    // grid hides its overflow, so the focused-card scroll can never move
    // (`docs/title-and-menu.md` § Timeline `### Layout`).
    let body = entity_with::<crate::presentation::timeline::TimelineBody>(&app);
    let body_node = app.world().get::<Node>(body).unwrap();
    assert_eq!(body_node.flex_direction, FlexDirection::Column);
    assert_eq!(body_node.align_items, AlignItems::Start);
    assert_eq!(body_node.overflow.y, bevy::ui::OverflowAxis::Scroll);
    assert_eq!(grid_node.flex_shrink, 0.0);
    let body = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<crate::presentation::timeline::TimelineBody>()
                .map(|_| entity.id())
        })
        .expect("the Timeline body");
    let body_node = app.world().get::<Node>(body).unwrap();
    assert_eq!(body_node.overflow, Overflow::scroll_y());

    // Heading / explanation / legend hold the contract's type roles and sizes.
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();
    let font_size = |app: &App, want: &str| -> bevy::text::FontSize {
        app.world()
            .iter_entities()
            .find_map(|entity| {
                (entity.get::<Text>().map(|text| text.0.as_str()) == Some(want))
                    .then(|| entity.get::<TextFont>().map(|font| font.font_size))
                    .flatten()
            })
            .unwrap_or_else(|| panic!("no text node {want:?}"))
    };
    assert_eq!(
        font_size(&app, menu::TIMELINE_HEADING),
        faces.sans_semibold(24.0).font_size
    );
    assert_eq!(
        font_size(&app, menu::TIMELINE_EXPLANATION),
        faces.sans(16.0).font_size
    );
    assert_eq!(
        font_size(&app, menu::TIMELINE_LEGEND),
        faces.mono(12.0).font_size
    );
}

#[test]
fn timeline_and_gallery_doors_render_only_in_the_slot_grid() {
    let (mut app, _pack) = timeline_app(&["first_meeting", "reunion"]);
    esc(&mut app);
    build_pause_tree(&mut app);
    assert!(has_action(&app, MenuAction::OpenTimeline));
    assert!(has_action(&app, MenuAction::OpenGallery));

    click(&mut app, MenuAction::OpenSettings);
    assert_eq!(menu(&app).view, MenuView::Settings);
    build_pause_tree(&mut app);
    assert!(!has_action(&app, MenuAction::OpenTimeline));
    assert!(!has_action(&app, MenuAction::OpenGallery));

    click(&mut app, MenuAction::BackToSlots);
    click(&mut app, MenuAction::OpenGallery);
    assert_eq!(menu(&app).view, MenuView::Gallery);
    build_pause_tree(&mut app);
    assert!(!has_action(&app, MenuAction::OpenTimeline));
}

#[test]
fn a_jump_applies_the_targets_leading_staging_changes() {
    let pack = temp_custom_pack(
        &[
            ("a", "bg a\nplay music_a\nA: one.\n"),
            ("b", "bg b\nplay music_b\nA: two.\n"),
        ],
        Some(&["a", "b"]),
        "a",
    );
    let root = pack.path().join("pack");
    seed_visited(&root, &["b"]);
    let mut app = app_for_pack(root);
    open_timeline(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(1);
    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter); // accept

    let staging = &app.world().resource::<Runtime>().staging;
    assert_eq!(staging.background.as_deref(), Some("b"));
    assert_eq!(staging.bgm.as_deref(), Some("music_b"));
}

#[test]
fn a_jump_to_a_scene_that_ends_immediately_parks_at_the_end() {
    let pack = temp_custom_pack(
        &[("a", "bg a\nA: one.\n"), ("b", "stop music_a\n")],
        Some(&["a", "b"]),
        "a",
    );
    let root = pack.path().join("pack");
    seed_visited(&root, &["b"]);
    let mut app = app_for_pack(root);
    open_timeline(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(1);
    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter); // accept

    assert!(
        wait(&app).is_none(),
        "the target ended immediately; no invented wait"
    );
    assert!(!menu(&app).open, "the overlay closed on the landing");
}

#[test]
fn a_jump_begins_only_the_targets_fresh_decision_request() {
    let fake = FakeResolver::new(model_answer("warm"));
    let pack = temp_custom_pack(
        &[
            ("a", "A: entry.\n"),
            (
                "b",
                "decide ace.greet -> reply\nlabel reply.warm\nA: warm.\nlabel reply.cold\nA: cold.\nlabel reply.none\nA: none.\n",
            ),
        ],
        Some(&["a", "b"]),
        "a",
    );
    let root = pack.path().join("pack");
    seed_visited(&root, &["b"]);
    let mut app = app_with_decisions_at(root, Some(Arc::new(fake.clone())));
    assert_eq!(fake.calls(), 0, "no request before the jump");

    open_timeline(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(1);
    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter); // accept

    pump_until(&mut app, |_| fake.calls() == 1);
    assert!(matches!(wait(&app), Some(Wait::Decision { .. })));
    fake.release();
    pump_until(&mut app, parked_on_say);
    let Some(Wait::Say { text, .. }) = wait(&app) else {
        panic!(
            "expected the target's say after its decision, got {:?}",
            wait(&app)
        );
    };
    assert_eq!(text, "warm.");
}

/// A copied demo pack with `count` generated scenes; the first id is long when
/// `long_first` is set. All ids are declared in order and the first is the
/// entry, so play visits it.
fn temp_pack_with_many_entries(count: usize, long_first: bool) -> tempfile::TempDir {
    const LONG: &str = "scene_with_a_deliberately_long_representative_identifier_00";
    let dir = temp_pack();
    let root = dir.path().join("pack");
    for scene in std::fs::read_dir(root.join("scenes")).unwrap() {
        std::fs::remove_file(scene.unwrap().path()).unwrap();
    }
    let mut ids = Vec::new();
    for index in 0..count {
        let id = if index == 0 && long_first {
            LONG.to_string()
        } else {
            format!("tl_scene_{index:02}")
        };
        std::fs::write(root.join(format!("scenes/{id}.unmei")), "A: line.\n").unwrap();
        ids.push(id);
    }
    std::fs::write(
        root.join("pack.ron"),
        format!(
            "(\n    name: \"timeline many\",\n    entry: \"{}\",\n    scenes: [{}],\n)\n",
            ids[0],
            ids.iter()
                .map(|id| format!("\"{id}\""))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    )
    .unwrap();
    let visited: Vec<&str> = ids.iter().map(String::as_str).collect();
    seed_visited(&root, &visited);
    dir
}

#[test]
fn a_thirty_entry_timeline_renders_every_card_with_long_ids_verbatim() {
    let pack = temp_pack_with_many_entries(30, true);
    let mut app = app_for_pack(pack.path().join("pack"));
    open_timeline(&mut app);
    let labels = timeline_labels(&mut app);
    assert_eq!(labels.len(), 30, "every declared scene renders once");
    assert_eq!(
        labels[0], "scene_with_a_deliberately_long_representative_identifier_00",
        "the long raw id is rendered verbatim"
    );
    assert_eq!(labels[29], "tl_scene_29");
    assert!(has_action(&app, MenuAction::TimelineCard(29)));
    assert!(
        !has_action(&app, MenuAction::TimelineCard(30)),
        "no card past the declared list"
    );

    // The walk clamps on the last card rather than wrapping.
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(29);
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::TimelineCard(29));
    press_and_update(&mut app, KeyCode::ArrowRight);
    assert_eq!(menu(&app).focus, MenuFocus::TimelineCard(29));
}

#[test]
fn the_timeline_opens_at_the_end_of_the_scene_unchanged() {
    let (mut app, _pack) = timeline_app(&["first_meeting", "reunion", "epilogue"]);
    play_to_end(&mut app);
    assert!(wait(&app).is_none(), "the demo runs to its end");
    esc(&mut app);
    assert!(menu(&app).open, "the overlay opens at scene end");
    click(&mut app, MenuAction::OpenTimeline);
    assert_eq!(menu(&app).view, MenuView::Timeline);
    assert!(wait(&app).is_none(), "the end park is untouched");
    assert_eq!(
        app.world().resource::<Runtime>().runner.scene_id(),
        "epilogue"
    );
}
