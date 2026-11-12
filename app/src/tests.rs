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

fn demo_pack_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/demo-pack")
        .canonicalize()
        .expect("demo pack path exists")
}

fn headless_app() -> App {
    app_with_decisions(None)
}

/// Attach the embedded interface faces to a headless test world. The core test
/// apps do not add `TextPlugin`, so this inserts a bare `Assets<Font>` and the
/// [`crate::presentation::style::TextFaces`] resource the presentation systems read; no
/// pack asset root is touched (`docs/title-and-menu.md` § Type system §1).
fn attach_text_faces(app: &mut App) {
    let mut fonts = bevy::asset::Assets::<bevy::text::Font>::default();
    let faces = crate::presentation::style::TextFaces::embed(&mut fonts);
    app.insert_resource(fonts);
    app.insert_resource(faces);
}

/// A headless core app over an explicit pack root and resolver. `ButtonInput`
/// is initialised directly (not via `InputPlugin`) so a manually pressed key
/// survives into `Update` for the input systems to see. The first frame leaves
/// the gated title up.
fn boot_app_unstarted(pack_root: PathBuf, decisions: Option<Arc<dyn DecisionResolver>>) -> App {
    boot_app_unstarted_dev(pack_root, decisions, true)
}

/// [`boot_app_unstarted`], with the shared dev-mode gate set explicitly. Test
/// fixtures that press F5/F9 pass `true` so a release test run protects the same
/// shortcut behavior; the dev-off regressions pass `false`.
fn boot_app_unstarted_dev(
    pack_root: PathBuf,
    decisions: Option<Arc<dyn DecisionResolver>>,
    dev_mode: bool,
) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .insert_resource(RuntimeConfig::new(pack_root, decisions).with_dev_mode(dev_mode))
        .add_plugins(RuntimeCorePlugin);
    attach_text_faces(&mut app);
    app
}

fn boot_app(pack_root: PathBuf, decisions: Option<Arc<dyn DecisionResolver>>) -> App {
    let mut app = boot_app_unstarted(pack_root, decisions);
    app.update(); // Startup: load the pack, then show the title
    app
}

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

/// Cross the real boot path: press the title's `New Game` (the demo packs have
/// no autosave, so it is immediate and confirm-free). Every pre-existing test
/// enters play through this, so no production bypass exists.
fn enter_play(app: &mut App) {
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleNewGame;
    press_and_update(app, KeyCode::Enter);
    assert!(!menu(app).open, "enter_play must actually start play");
}

/// A headless core app with an explicit decision resolver: `None` exercises the
/// offline author fallback, `Some` a test-local fake driven in-process.
fn app_with_decisions(decisions: Option<Arc<dyn DecisionResolver>>) -> App {
    app_with_decisions_at(demo_pack_root(), decisions)
}

/// A headless core app over an explicit pack root and resolver, already in
/// play: boot the title, then press `New Game`.
fn app_with_decisions_at(pack_root: PathBuf, decisions: Option<Arc<dyn DecisionResolver>>) -> App {
    let mut app = boot_app(pack_root, decisions);
    enter_play(&mut app);
    app
}

/// Advance the runner directly to the pack's decision point, staging as the
/// core would, so the app systems can be exercised from a known park.
fn drive_runner_to_decision(runtime: &mut Runtime) {
    loop {
        let outcome = runtime.runner.run();
        runtime.staging.apply_all(&outcome.staged);
        match outcome.wait {
            Some(Wait::Say { .. }) => runtime.runner.advance_say().unwrap(),
            Some(Wait::Choice { .. }) => runtime.runner.choose(0).unwrap(),
            Some(Wait::Decision { .. }) => return,
            None => panic!("script ended before the decision point"),
        }
    }
}

#[test]
fn core_parks_on_the_entry_scene_say() {
    let app = headless_app();
    let runtime = app.world().resource::<Runtime>();
    assert!(matches!(runtime.runner.wait(), Some(Wait::Say { .. })));
    assert_eq!(runtime.runner.scene_id(), "first_meeting");
}

#[test]
fn decision_without_a_client_takes_the_author_fallback() {
    let mut app = headless_app();
    {
        let mut runtime = app.world_mut().resource_mut::<Runtime>();
        drive_runner_to_decision(&mut runtime);
    }
    // The core sees the decision park and resolves it (fallback `cold`),
    // then the next step parks on `reply.cold`'s say-line.
    app.update();
    app.update();

    let runtime = app.world().resource::<Runtime>();
    let Some(Wait::Say { speaker, text }) = runtime.runner.wait() else {
        panic!(
            "expected a say after the fallback, got {:?}",
            runtime.runner.wait()
        );
    };
    assert_eq!(speaker, "Ace");
    assert!(text.contains("It's you."), "got {text:?}");
    assert!(app.world().resource::<PendingDecision>().0.is_none());
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
        let outcome = runtime.runner.run();
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

/// Copy the demo pack into a temp dir so F5/F9 write `<temp>/pack.slot1.save.ron`
/// instead of dirtying the repo.
pub(crate) fn temp_pack() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let dst = dir.path().join("pack");
    copy_dir(&demo_pack_root(), &dst);
    dir
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// The sibling path one slot resolves to for the running app's pack.
fn slot_path(app: &App, slot: Slot) -> PathBuf {
    let runtime = app.world().resource::<Runtime>();
    save::save_path_for(runtime.runner.pack(), slot)
}

/// The F5/F9 manual target (`Manual(1)`) for the running app's pack.
fn manual_path(app: &App) -> PathBuf {
    slot_path(app, Slot::Manual(1))
}

/// The game-written autosave path for the running app's pack.
fn autosave_path(app: &App) -> PathBuf {
    slot_path(app, Slot::Autosave)
}

/// The temporary sibling `write_save` uses for one slot path.
fn tmp_path(slot: &Path) -> PathBuf {
    slot.with_extension("ron.tmp")
}

pub(crate) fn app_for_pack(pack_root: PathBuf) -> App {
    let mut app = boot_app(pack_root, None);
    enter_play(&mut app);
    app
}

/// [`app_for_pack`] with an explicit dev-mode gate, so the disabled-shortcut
/// regressions start from real play without a shortcut-enabled fixture.
fn app_for_pack_dev(pack_root: PathBuf, dev_mode: bool) -> App {
    let mut app = boot_app_unstarted_dev(pack_root, None, dev_mode);
    app.update();
    enter_play(&mut app);
    app
}

pub(crate) fn press_and_update(app: &mut App, key: KeyCode) {
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
    app.update();
    // Fully reset so the same key can be pressed again next frame.
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
}

fn click_and_update(app: &mut App) {
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .reset_all();
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

// ---------------------------------------------------------------------------
// Test-local decision resolver
// ---------------------------------------------------------------------------

/// The shared control surface of [`FakeResolver`]: it withholds a resolution
/// until [`FakeResolver::release`] and counts how many of its futures were
/// dropped **before producing an answer** (the signal a load cancels the
/// in-flight request).
#[derive(Default)]
struct FakeState {
    released: AtomicBool,
    dropped: AtomicUsize,
    waker: Mutex<Option<Waker>>,
}

/// A jointly-controllable, network-free resolver: the app spawns it exactly as
/// it spawns the concrete client, and the test decides when it answers.
#[derive(Clone)]
struct FakeResolver {
    resolution: ResolvedDecision,
    calls: Arc<AtomicUsize>,
    state: Arc<FakeState>,
}

impl FakeResolver {
    fn new(resolution: ResolvedDecision) -> Self {
        FakeResolver {
            resolution,
            calls: Arc::new(AtomicUsize::new(0)),
            state: Arc::new(FakeState::default()),
        }
    }

    fn release(&self) {
        self.state.released.store(true, Ordering::SeqCst);
        if let Some(waker) = self.state.waker.lock().unwrap().take() {
            waker.wake();
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn dropped(&self) -> usize {
        self.state.dropped.load(Ordering::SeqCst)
    }

    /// Dropping `Task` cancels it, which drops this future — possibly on a pool
    /// worker, so observe it with a bounded wait rather than assuming a frame.
    /// A completed future does not count as dropped (see [`FakeFuture`]'s
    /// `Drop`), which is why a released fake cannot mask a real cancellation.
    /// The deadline is only a safety bound (the assertion is about cancellation,
    /// not latency); it is generous so a loaded test pool cannot flake it.
    fn wait_until_dropped(&self) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while self.dropped() == 0 {
            assert!(
                Instant::now() < deadline,
                "the fake future was never dropped"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

struct FakeFuture {
    resolution: Option<ResolvedDecision>,
    state: Arc<FakeState>,
}

impl Future for FakeFuture {
    type Output = ResolvedDecision;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<ResolvedDecision> {
        if self.state.released.load(Ordering::SeqCst) {
            return Poll::Ready(self.take_resolution());
        }
        *self.state.waker.lock().unwrap() = Some(cx.waker().clone());
        // `release` may have raced the registration above.
        if self.state.released.load(Ordering::SeqCst) {
            return Poll::Ready(self.take_resolution());
        }
        Poll::Pending
    }
}

impl FakeFuture {
    fn take_resolution(&mut self) -> ResolvedDecision {
        self.resolution
            .take()
            .expect("the fake future was polled after it completed")
    }
}

impl Drop for FakeFuture {
    fn drop(&mut self) {
        // A completed future is emptied by `take_resolution`, so a remaining
        // resolution means it was dropped before it ever answered — a
        // cancellation (a load replacing the parked task), not the routine drop
        // that follows `Ready`.
        if self.resolution.is_some() {
            self.state.dropped.fetch_add(1, Ordering::SeqCst);
        }
    }
}

impl DecisionResolver for FakeResolver {
    fn resolve<'a>(
        &'a self,
        _model: &'a str,
        _question_id: &'a str,
        _config: &'a DecisionConfig,
        _state: &'a HashMap<String, StateValue>,
    ) -> Pin<Box<dyn Future<Output = ResolvedDecision> + Send + 'a>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(FakeFuture {
            resolution: Some(self.resolution.clone()),
            state: self.state.clone(),
        })
    }
}

// ---------------------------------------------------------------------------
// Test-local key check
// ---------------------------------------------------------------------------

/// The shared control surface of [`FakeKeyCheck`]: the call count, and the one
/// verdict it answers with once released. An unreleased fake never answers — the
/// "ping that never answers" case.
#[derive(Default)]
struct FakeKeyCheckState {
    calls: AtomicUsize,
    released: AtomicBool,
    verdict: Mutex<Option<KeyVerdict>>,
    waker: Mutex<Option<Waker>>,
}

/// A jointly-controllable, network-free [`KeyCheck`]: the app spawns it exactly
/// as it spawns `HttpKeyCheck`, and the test decides when (and with what) it
/// answers.
#[derive(Clone, Default)]
struct FakeKeyCheck {
    state: Arc<FakeKeyCheckState>,
}

impl FakeKeyCheck {
    fn new() -> Self {
        Self::default()
    }

    /// Answer with `verdict` at the next poll.
    fn release(&self, verdict: KeyVerdict) {
        *self.state.verdict.lock().unwrap() = Some(verdict);
        self.state.released.store(true, Ordering::SeqCst);
        if let Some(waker) = self.state.waker.lock().unwrap().take() {
            waker.wake();
        }
    }

    fn calls(&self) -> usize {
        self.state.calls.load(Ordering::SeqCst)
    }

    /// Arm the fake for a second commit: clear the released flag and the answer.
    /// Only valid once the previous future has completed.
    fn reset(&self) {
        self.state.released.store(false, Ordering::SeqCst);
        *self.state.verdict.lock().unwrap() = None;
    }
}

struct FakeKeyCheckFuture {
    state: Arc<FakeKeyCheckState>,
}

impl FakeKeyCheckFuture {
    fn verdict(&self) -> KeyVerdict {
        self.state
            .verdict
            .lock()
            .unwrap()
            .take()
            .unwrap_or(KeyVerdict::Unreachable)
    }
}

impl Future for FakeKeyCheckFuture {
    type Output = KeyVerdict;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<KeyVerdict> {
        if self.state.released.load(Ordering::SeqCst) {
            return Poll::Ready(self.verdict());
        }
        *self.state.waker.lock().unwrap() = Some(cx.waker().clone());
        // `release` may have raced the registration above.
        if self.state.released.load(Ordering::SeqCst) {
            return Poll::Ready(self.verdict());
        }
        Poll::Pending
    }
}

impl KeyCheck for FakeKeyCheck {
    fn check<'a>(&'a self, _key: &'a str) -> Pin<Box<dyn Future<Output = KeyVerdict> + Send + 'a>> {
        self.state.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(FakeKeyCheckFuture {
            state: self.state.clone(),
        })
    }
}

fn model_answer(action: &str) -> ResolvedDecision {
    ResolvedDecision {
        action: action.to_string(),
        model: Some("fake/model".to_string()),
        confidence: Some(0.9),
        fallback_reason: None,
    }
}

/// A fallback the test controls directly: the engine's transport error, so the
/// row the app reports is the real classification of a real variant.
fn fallback_answer(action: &str) -> ResolvedDecision {
    ResolvedDecision {
        action: action.to_string(),
        model: None,
        confidence: None,
        fallback_reason: Some(DecisionError::Http),
    }
}

/// Build an app whose resolver is the fake, parked by the core on the demo
/// pack's decision point, with the request spawned. The pack is a temp copy so
/// a model-answer autosave never dirties the repo; the caller must hold the
/// returned guard for the app's lifetime.
fn app_parked_on_decision(fake: &FakeResolver) -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let dst = dir.path().join("pack");
    copy_dir(&demo_pack_root(), &dst);
    let mut app = app_with_decisions_at(dst, Some(Arc::new(fake.clone())));
    {
        let mut runtime = app.world_mut().resource_mut::<Runtime>();
        drive_runner_to_decision(&mut runtime);
    }
    app.update(); // `drive_script` spawns the resolver task

    // `start_decision` calls the resolver inside the spawned future, so the
    // call is observed on the task pool — a frame after the spawn.
    pump_until(&mut app, |_| fake.calls() == 1);
    (app, dir)
}

#[test]
fn a_model_answered_decision_writes_the_autosave_once() {
    let fake = FakeResolver::new(model_answer("warm"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    assert!(
        !autosave_path(&app).exists(),
        "a request in flight writes no autosave yet"
    );

    fake.release();
    pump_until(&mut app, |app| autosave_path(app).exists());

    let runtime = app.world().resource::<Runtime>();
    let save = save::read_slot(runtime.runner.pack(), Slot::Autosave).expect("autosaved");
    assert!(save.saved_at.is_some());
    assert_eq!(save.wait, runtime.runner.wait().cloned());
    assert!(
        matches!(save.wait, Some(Wait::Say { ref text, .. }) if text.contains("You came back!")),
        "the autosave captures the post-model-answer park: {:?}",
        save.wait
    );

    let before = std::fs::read(autosave_path(&app)).unwrap();
    app.update();
    assert_eq!(std::fs::read(autosave_path(&app)).unwrap(), before);
}

#[test]
fn a_fallback_resolved_decision_writes_no_autosave() {
    let fake = FakeResolver::new(fallback_answer("cold"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    fake.release();
    pump_until(&mut app, parked_on_say);
    assert!(
        !autosave_path(&app).exists(),
        "a fallback resolve never autosaves"
    );
}

#[test]
fn loading_a_decision_wait_refires_the_decide_line() {
    let fake = FakeResolver::new(model_answer("warm"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    assert_eq!(fake.calls(), 1);

    // Save while parked on the decision, then load: the wait comes back, the
    // in-flight request is cancelled, and the next `run` re-fires `decide`.
    press_and_update(&mut app, KeyCode::F5);
    press_and_update(&mut app, KeyCode::F9);

    // The restored decision-wait re-fires `decide`; the re-fire is a fresh
    // async request, so it can land on a later frame than the load.
    pump_until(&mut app, |_| fake.calls() == 2);
    assert!(matches!(
        app.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Decision { .. })
    ));
    assert_eq!(fake.calls(), 2, "the restored decision re-fired");
}

#[test]
fn fake_resolver_withholds_a_model_answer_then_lands_the_runner() {
    let fake = FakeResolver::new(model_answer("warm"));
    let (mut app, _dir) = app_parked_on_decision(&fake);

    assert_eq!(fake.calls(), 1, "the core called the resolver once");
    assert!(app.world().resource::<PendingDecision>().0.is_some());

    // Withheld: the answer does not exist yet, so the runner stays parked.
    app.update();
    assert!(matches!(
        app.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Decision { .. })
    ));

    fake.release();
    pump_until(&mut app, parked_on_say);
    let runtime = app.world().resource::<Runtime>();
    let Some(Wait::Say { speaker, text }) = runtime.runner.wait() else {
        panic!(
            "expected a say after the model answer, got {:?}",
            runtime.runner.wait()
        );
    };
    assert_eq!(speaker, "Ace");
    assert!(text.contains("You came back!"), "got {text:?}");
    assert!(app.world().resource::<PendingDecision>().0.is_none());
}

#[test]
fn fake_resolver_delivers_a_fallback_resolution() {
    let fake = FakeResolver::new(fallback_answer("cold"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    fake.release();
    pump_until(&mut app, parked_on_say);
    let runtime = app.world().resource::<Runtime>();
    let Some(Wait::Say { text, .. }) = runtime.runner.wait() else {
        panic!(
            "expected a say after the fallback, got {:?}",
            runtime.runner.wait()
        );
    };
    assert!(text.contains("It's you."), "got {text:?}");
}

#[test]
fn dropping_the_pending_decision_cancels_the_fake_future() {
    let fake = FakeResolver::new(model_answer("warm"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    assert_eq!(fake.calls(), 1);
    assert_eq!(fake.dropped(), 0);

    // What a load does: replace the parked task with `None`.
    app.world_mut().resource_mut::<PendingDecision>().0 = None;
    fake.wait_until_dropped();
    assert_eq!(fake.dropped(), 1, "dropping the task must drop the future");
    // The runner is still parked where the cancelled request left it.
    assert!(matches!(
        app.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Decision { .. })
    ));
}

// ---------------------------------------------------------------------------
// Held answers while the overlay is open
// ---------------------------------------------------------------------------

/// Update until `check` holds. The fake's answer is polled through the async
/// task pool, so it can land on a later frame than the release.
fn pump_until(app: &mut App, mut check: impl FnMut(&App) -> bool) {
    for _ in 0..64 {
        if check(app) {
            return;
        }
        app.update();
    }
    panic!("the condition never became true");
}

/// Whether a completed answer is buffered in `HeldDecision`.
fn held(app: &App) -> bool {
    app.world().resource::<HeldDecision>().0.is_some()
}

/// Whether the defensive hold is engaged.
fn on_hold(app: &App) -> bool {
    app.world().resource::<DefensiveHold>().0
}

/// Whether the runner is parked on a say-line.
fn parked_on_say(app: &App) -> bool {
    matches!(
        app.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Say { .. })
    )
}

#[test]
fn a_model_answer_arriving_while_open_is_held_until_resume() {
    let fake = FakeResolver::new(model_answer("warm"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    let parked = parked_position(&app);

    // Open the overlay while the request is in flight, then let it answer.
    esc(&mut app);
    assert!(menu(&app).open);
    fake.release();
    pump_until(&mut app, held);

    // Held: `pc` and the parked wait are unchanged, the request completed, and
    // nothing was written.
    assert_eq!(parked_position(&app), parked, "the answer is not applied");
    assert!(matches!(
        app.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Decision { .. })
    ));
    assert!(app.world().resource::<PendingDecision>().0.is_none());
    assert!(held(&app), "the completed answer is buffered");
    assert!(!autosave_path(&app).exists(), "held means no write yet");

    // Resume: the buffered answer is applied, the runner parks at the next
    // wait, and the model resolve writes the autosave exactly once.
    esc(&mut app);
    assert!(!menu(&app).open);
    assert!(!held(&app));
    let runtime = app.world().resource::<Runtime>();
    let Some(Wait::Say { text, .. }) = runtime.runner.wait() else {
        panic!(
            "expected a say after resume, got {:?}",
            runtime.runner.wait()
        );
    };
    assert!(text.contains("You came back!"), "got {text:?}");

    let save = save::read_slot(runtime.runner.pack(), Slot::Autosave).expect("autosaved");
    assert_eq!(save.wait, runtime.runner.wait().cloned());

    let before = std::fs::read(autosave_path(&app)).unwrap();
    app.update();
    assert_eq!(
        std::fs::read(autosave_path(&app)).unwrap(),
        before,
        "a held answer writes the autosave exactly once"
    );
}

#[test]
fn a_held_fallback_writes_no_autosave() {
    let fake = FakeResolver::new(fallback_answer("cold"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    let parked = parked_position(&app);

    esc(&mut app);
    fake.release();
    pump_until(&mut app, held);
    assert_eq!(parked_position(&app), parked, "the held fallback waits");
    assert!(held(&app));

    esc(&mut app);
    assert!(!held(&app));
    let Some(Wait::Say { text, .. }) = app.world().resource::<Runtime>().runner.wait() else {
        panic!("expected a say after the held fallback");
    };
    assert!(text.contains("It's you."), "got {text:?}");
    assert!(
        !autosave_path(&app).exists(),
        "a held fallback writes no autosave"
    );
}

#[test]
fn loading_discards_the_held_answer_and_refires_the_decide_line() {
    let fake = FakeResolver::new(model_answer("warm"));
    let (mut app, _dir) = app_parked_on_decision(&fake);

    // Save the parked decision-wait, open the overlay, and let it hold an answer.
    press_and_update(&mut app, KeyCode::F5);
    esc(&mut app);
    fake.release();
    pump_until(&mut app, held);

    // Load through the overlay: the held answer is discarded, the request is
    // cancelled, and the restored decision-wait re-fires `decide`.
    switch_mode(&mut app, MenuMode::Load);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::Card(0);
    press_and_update(&mut app, KeyCode::Enter); // confirm
    press_and_update(&mut app, KeyCode::Enter); // accept

    assert!(!menu(&app).open, "a load closes the overlay");
    assert!(!held(&app), "the held answer is discarded by the load");
    // The restored decision-wait re-fires `decide`; the re-fire goes through
    // the task pool, so it can be observed on a later frame. This fake is
    // already released, so the fresh request answers at once rather than
    // parking: the runner carries on to the next say.
    pump_until(&mut app, |_| fake.calls() == 2);
    assert_eq!(fake.calls(), 2, "the restored decision re-fired");
    pump_until(&mut app, parked_on_say);
}

/// The load's own state transition, observed without the immediate re-fire that
/// the full app frame follows it with: `SessionTransitions::restore_loaded`
/// empties both the in-flight request and the held answer.
#[test]
fn a_load_clears_the_held_and_pending_decision_state() {
    let fake = FakeResolver::new(model_answer("warm"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    let save = {
        let runtime = app.world().resource::<Runtime>();
        SaveFile::capture(&runtime.runner, &runtime.staging)
    };

    esc(&mut app);
    fake.release();
    pump_until(&mut app, held);

    let path = PathBuf::from("held.slot1.save.ron");
    let save = std::sync::Mutex::new(Some(save));
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
    assert_eq!(outcome, LoadOutcome::Loaded, "the restore succeeds");

    assert!(!held(&app));
    assert!(app.world().resource::<PendingDecision>().0.is_none());
}

// ---------------------------------------------------------------------------
// Failure posture: the Notice and the startup error screen
// ---------------------------------------------------------------------------

#[test]
fn an_active_notice_does_not_consume_input() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    app.world_mut()
        .resource_mut::<Notice>()
        .show("something failed");

    let before = app.world().resource::<Runtime>().runner.wait().cloned();
    press_and_update(&mut app, KeyCode::Space);
    let after = app.world().resource::<Runtime>().runner.wait().cloned();

    assert_ne!(
        before, after,
        "the Notice must not consume the advance press"
    );
    assert!(
        app.world().resource::<Notice>().is_showing(),
        "a press must not dismiss the Notice"
    );
}

fn app_for_broken_pack() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .insert_resource(
            RuntimeConfig::new(PathBuf::from("definitely/not/a/pack"), None).with_dev_mode(true),
        )
        .add_plugins(RuntimeCorePlugin);
    app.update(); // Startup: the failed load is recorded, not exited
    app
}

#[test]
fn a_broken_pack_records_the_failure_and_never_opens_play() {
    let app = app_for_broken_pack();
    let failure = app
        .world()
        .get_resource::<StartupFailure>()
        .expect("the failed load is recorded");
    let StartupFailure::Pack {
        errors, defaulted, ..
    } = failure
    else {
        panic!("a broken pack is a pack failure: {failure:?}");
    };
    assert!(!errors.is_empty(), "the error list is kept whole");
    assert!(!defaulted, "the test app gives an explicit pack path");
    assert!(
        app.world().get_resource::<Runtime>().is_none(),
        "the play path must stay shut without a pack"
    );
}

/// The origin the entry point resolved reaches the screen: a defaulted pack that
/// is missing explains the default rule with the hint line, and the same failure
/// from an explicit path does not.
#[test]
fn the_resolved_pack_origin_reaches_the_error_screen_hint() {
    let broken = |origin| {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .insert_resource(
                RuntimeConfig::new(PathBuf::from("definitely/not/a/demo-pack"), None)
                    .with_dev_mode(true)
                    .with_pack_origin(origin),
            )
            .add_plugins(RuntimeCorePlugin);
        app.update();
        app
    };

    let defaulted = broken(crate::launch::PackOrigin::Defaulted);
    let failure = defaulted
        .world()
        .get_resource::<StartupFailure>()
        .expect("the missing default pack is recorded");
    assert!(
        failure
            .screen_lines()
            .iter()
            .any(|line| line == &crate::failure::default_pack_hint()),
        "the defaulted path explains itself: {:?}",
        failure.screen_lines()
    );

    let explicit = broken(crate::launch::PackOrigin::Explicit);
    let failure = explicit
        .world()
        .get_resource::<StartupFailure>()
        .expect("the missing explicit pack is recorded");
    assert!(
        !failure
            .screen_lines()
            .iter()
            .any(|line| line.contains("No PACK_DIR given")),
        "an explicit pack shows no hint: {:?}",
        failure.screen_lines()
    );
}

#[test]
fn the_error_screen_quits_nonzero_on_a_key_or_a_click() {
    // Each fresh app gets one quit; `should_exit` drains the message.
    for quit in [KeyCode::Escape, KeyCode::Enter, KeyCode::Space] {
        let mut app = app_for_broken_pack();
        assert_eq!(app.should_exit(), None);
        press_and_update(&mut app, quit);
        assert!(
            matches!(app.should_exit(), Some(AppExit::Error(_))),
            "{quit:?} must quit non-zero"
        );
    }

    let mut app = app_for_broken_pack();
    assert_eq!(app.should_exit(), None);
    click_and_update(&mut app);
    assert!(matches!(app.should_exit(), Some(AppExit::Error(_))));
}

#[test]
fn a_healthy_pack_never_records_a_startup_failure() {
    let app = headless_app();
    assert!(app.world().get_resource::<StartupFailure>().is_none());
    assert!(app.world().get_resource::<Runtime>().is_some());
}

#[test]
fn a_defensive_failure_holds_the_runner_and_shows_the_notice() {
    // The fake answers with an action the loaded pack has no label for, which
    // the loader's validation makes unreachable — but the hold is specified.
    captures::install();
    let fake = FakeResolver::new(model_answer("no_such_label"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    force_single_threaded(&mut app);
    captures::take();
    app.world_mut().resource_mut::<Notice>().text = None;
    fake.release();
    pump_until(&mut app, on_hold);

    let records = failure_lines();
    assert_eq!(records.len(), 1, "row 12 logs once: {records:?}");
    assert_eq!(records[0].0, log::Level::Error);
    assert!(
        records[0].2.starts_with("the story can't continue"),
        "{:?}",
        records[0].2
    );

    let runtime = app.world().resource::<Runtime>();
    assert!(
        matches!(runtime.runner.wait(), Some(Wait::Decision { .. })),
        "a defensive failure must leave the runner parked"
    );
    let notice = app.world().resource::<Notice>();
    assert!(notice.is_showing(), "the hold must be visible");
    assert_eq!(
        notice.text.as_deref(),
        Some("the story can't continue — Esc for the menu")
    );
    assert!(on_hold(&app), "rows 11–13 set the defensive hold");

    // Esc opens the overlay under the hold; the play path stays frozen and the
    // missing label never re-fires the request.
    let parked = parked_position(&app);
    esc(&mut app);
    assert!(menu(&app).open, "Esc opens the overlay under a hold");
    esc(&mut app);
    app.update();
    assert_eq!(
        parked_position(&app),
        parked,
        "a held runner never advances"
    );
    assert_eq!(fake.calls(), 1, "the missing label never re-fires");
}

/// Build an app parked on the demo decision whose pack has had its deciding NPC
/// removed, so `start_decision` finds no matching NPC (row 11) — otherwise
/// unreachable, since the loader enforces the reference.
fn app_with_missing_decide_npc(fake: &FakeResolver) -> App {
    let mut app = app_with_decisions(Some(Arc::new(fake.clone())));
    let loaded = load_pack(&demo_pack_root()).expect("demo pack loads");
    let mut pack = loaded.pack;
    pack.npcs.remove("ace");
    let mut runtime = Runtime {
        runner: ScriptRunner::new(pack),
        staging: StagingState::default(),
        pack_root: demo_pack_root(),
        asset_root: demo_pack_root().join("assets"),
    };
    drive_runner_to_decision(&mut runtime);
    app.world_mut().insert_resource(runtime);
    app
}

#[test]
fn a_decide_point_with_no_matching_npc_holds_the_runner() {
    captures::install();
    let fake = FakeResolver::new(model_answer("warm"));
    let mut app = app_with_missing_decide_npc(&fake);
    force_single_threaded(&mut app);
    captures::take();
    app.world_mut().resource_mut::<Notice>().text = None;

    app.update();

    let records = failure_lines();
    assert_eq!(records.len(), 1, "row 11 logs once: {records:?}");
    assert_eq!(records[0].0, log::Level::Error);
    assert!(
        records[0].2.starts_with("the story can't continue"),
        "{:?}",
        records[0].2
    );
    assert_eq!(
        app.world().resource::<Notice>().text.as_deref(),
        Some("the story can't continue — Esc for the menu")
    );
    assert!(on_hold(&app));
    assert_eq!(
        fake.calls(),
        0,
        "no resolver call for a missing decide point"
    );
    assert!(matches!(
        app.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Decision { .. })
    ));

    let parked = parked_position(&app);
    esc(&mut app);
    assert!(menu(&app).open, "Esc opens the overlay under the hold");
    esc(&mut app);
    app.update();
    assert_eq!(parked_position(&app), parked);
}

#[test]
fn a_digit_past_the_choice_options_is_ignored_not_a_defensive_hold() {
    captures::install();
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    force_single_threaded(&mut app);

    // Park on first_meeting's two-option choice, then press a digit past it.
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    let parked = parked_position(&app);
    assert!(matches!(parked.1, Some(Wait::Choice { .. })));

    captures::take();
    app.world_mut().resource_mut::<Notice>().text = None;
    press_and_update(&mut app, KeyCode::Digit9);

    // A mispress is not a row-13 RunnerError: nothing logs, nothing holds, and
    // the choice is still live, so the next valid pick advances normally.
    assert!(failure_lines().is_empty(), "a mispress is not a failure");
    assert!(!on_hold(&app), "a mispress must not freeze the game");
    assert_eq!(parked_position(&app), parked);
    assert!(!app.world().resource::<Notice>().is_showing());

    press_and_update(&mut app, KeyCode::Digit1);
    assert!(
        !matches!(
            app.world().resource::<Runtime>().runner.wait(),
            Some(Wait::Choice { .. })
        ),
        "a valid pick after a mispress still advances"
    );
}

#[test]
fn a_pack_warning_is_logged_but_never_builds_an_error_screen() {
    // A `choice` config with no `"none"` criterion is the canonical warning.
    let dir = tempfile::tempdir().unwrap();
    let pack_root = dir.path().join("warn-pack");
    std::fs::create_dir_all(pack_root.join("npcs")).unwrap();
    std::fs::create_dir_all(pack_root.join("scenes")).unwrap();
    std::fs::write(
        pack_root.join("pack.ron"),
        "(name: \"warn\", entry: \"scene\")",
    )
    .unwrap();
    std::fs::write(pack_root.join("state.ron"), "(met_ace: false)").unwrap();
    std::fs::write(
        pack_root.join("scenes/scene.unmei"),
        "Narrator: hi\n\ndecide ace.greet -> reply\n\nlabel reply.warm\nAce: warm\n\nlabel reply.cold\nAce: cold\n",
    )
    .unwrap();
    std::fs::write(
        pack_root.join("npcs/ace.ron"),
        "(\n    id: \"ace\",\n    name: \"Ace\",\n    decisions: {\n        \"greet\": (\n            instructions: \"Greet?\",\n            state_keys: [\"met_ace\"],\n            criteria: {\n                \"warm\": \"warm\",\n                \"cold\": \"cold\",\n            },\n            fallback: \"cold\",\n        ),\n    },\n)\n",
    )
    .unwrap();

    let loaded = unmei_engine::load_pack(&pack_root).expect("the warning pack still loads");
    assert!(
        !loaded.warnings.is_empty(),
        "the fixture must actually warn, or the test proves nothing"
    );

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .insert_resource(RuntimeConfig::new(pack_root, None).with_dev_mode(true))
        .add_plugins(RuntimeCorePlugin);
    app.update();

    assert!(
        app.world().get_resource::<Runtime>().is_some(),
        "the pack plays despite the warning"
    );
    assert!(
        app.world().get_resource::<StartupFailure>().is_none(),
        "a warning never reaches the error screen"
    );
}

#[test]
fn an_unwritable_log_reports_without_a_notice_and_the_pack_still_plays() {
    let pack = temp_pack();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .insert_resource(LogOpenError {
            path: PathBuf::from("/read-only/pack.log"),
            message: "permission denied".into(),
        })
        .insert_resource(RuntimeConfig::new(pack.path().join("pack"), None).with_dev_mode(true))
        .add_plugins(RuntimeCorePlugin);
    app.update();

    assert!(
        app.world().get_resource::<Runtime>().is_some(),
        "play continues despite the log failure"
    );
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "row 21 must never show a Notice"
    );
}

// ---------------------------------------------------------------------------
// Silent asset degradation (rows 3–6)
// ---------------------------------------------------------------------------

/// A test image loader: read the file, reject empty bytes as undecodable,
/// otherwise hand back a plain in-memory image. It lets one sibling load while
/// another fails in the loader, without a renderer.
#[derive(TypePath)]
struct PathImageLoader;

impl FromWorld for PathImageLoader {
    fn from_world(_world: &mut bevy::ecs::world::World) -> Self {
        PathImageLoader
    }
}

impl AssetLoader for PathImageLoader {
    type Asset = Image;
    type Settings = ();
    type Error = std::io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        if bytes.is_empty() {
            return Err(std::io::Error::other("undecodable image"));
        }
        Ok(Image::default())
    }

    fn extensions(&self) -> &[&str] {
        &["png"]
    }
}

/// A presentation-only app rooted at `asset_root`, with the demo pack's runner
/// held but undriven, so the real load sites and the watcher run with no
/// window, renderer or audio device.
fn asset_watch_app(asset_root: PathBuf) -> App {
    let loaded = load_pack(&demo_pack_root()).expect("the demo pack loads");
    let runtime = Runtime {
        runner: ScriptRunner::new(loaded.pack),
        staging: StagingState::default(),
        pack_root: demo_pack_root(),
        asset_root: asset_root.clone(),
    };

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(AssetPlugin {
            file_path: asset_root.to_string_lossy().into_owned(),
            ..default()
        })
        // The real app registers these asset types through its render and audio
        // plugins. The image loader here is a test double that reads the file
        // and either rejects it or returns a plain `Image`, so one sibling can
        // load while an unreadable one fails in the loader — the same
        // `LoadState::Failed` path an undecodable file takes.
        .init_asset::<Image>()
        .init_asset_loader::<PathImageLoader>()
        .init_asset::<bevy_kira_audio::AudioSource>()
        .insert_resource(runtime)
        .init_resource::<Notice>()
        .init_resource::<DefensiveHold>()
        .init_resource::<AssetWatch>()
        .init_resource::<AppliedVisuals>()
        .init_resource::<AppliedAudio>()
        .init_resource::<AppliedUi>()
        .insert_resource(AudioChannel::<Bgm>::default())
        .add_systems(
            Update,
            (sync_staging_visuals, sync_audio, report_silent_degradations).chain(),
        );
    force_single_threaded(&mut app);
    app
}

/// Pin a schedule to the calling thread, so the thread-local log capture sees
/// every line the systems emit (Bevy's default executor may use task threads).
fn force_single_threaded(app: &mut App) {
    app.edit_schedule(Update, |schedule| {
        schedule.set_executor(bevy::ecs::schedule::SingleThreadedExecutor::new());
    });
}

/// Drive the asset server until every watched load has settled — the same
/// `app.update()` loop Bevy's own asset tests use, with a wall-clock deadline so
/// a busy task pool cannot make the wait spin forever.
fn pump_until_watch_drains(app: &mut App) {
    // The load sites add their watches during Update, so the first frame has to
    // run before "settled" can be judged.
    app.update();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut spins = 0u32;
    while !app.world().resource::<AssetWatch>().is_empty() {
        assert!(
            Instant::now() < deadline,
            "the watched asset loads never settled"
        );
        app.update();
        spins += 1;
        if spins.is_multiple_of(64) {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

/// The `unmei::failure` lines captured so far.
fn failure_lines() -> Vec<(log::Level, String, String)> {
    captures::take()
        .into_iter()
        .filter(|(_, target, _)| target == crate::failure::LOG_TARGET)
        .collect()
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

/// A gallery entry resolves to its kind's one path producer and failure row: a
/// background to `backgrounds/<id>.png` under `BackgroundAsset`, a sprite to
/// `sprites/<npc_id>/<pose>.png` under `SpriteAsset`, so the grid and the still
/// share the play path's two producers and never fork a second inline copy
/// (`docs/title-and-menu.md` § Gallery § Entries §4/§5).
#[test]
fn gallery_entries_resolve_through_the_shared_path_producers() {
    let background = unmei_engine::GalleryEntry::Background {
        id: "school_gate".to_string(),
    };
    assert_eq!(
        crate::presentation::assets::gallery_entry_asset(&background),
        (
            crate::failure::FailureRow::BackgroundAsset,
            "backgrounds/school_gate.png".to_string()
        )
    );
    assert_eq!(
        crate::presentation::assets::gallery_entry_asset(&background).1,
        crate::presentation::assets::background_asset_path("school_gate"),
        "the gallery reuses the play path's background producer"
    );

    let sprite = unmei_engine::GalleryEntry::Sprite {
        npc_id: "ace".to_string(),
        pose: "happy".to_string(),
    };
    assert_eq!(
        crate::presentation::assets::gallery_entry_asset(&sprite),
        (
            crate::failure::FailureRow::SpriteAsset,
            "sprites/ace/happy.png".to_string()
        )
    );
    assert_eq!(
        crate::presentation::assets::gallery_entry_asset(&sprite).1,
        crate::presentation::assets::sprite_asset_path("ace", "happy"),
        "the gallery reuses the play path's sprite producer"
    );
}

#[test]
fn missing_background_sprite_and_music_each_warn_once_and_play_continues() {
    captures::install();
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = asset_watch_app(dir.path().to_path_buf());
    {
        let mut runtime = app.world_mut().resource_mut::<Runtime>();
        runtime.staging = StagingState {
            background: Some("missing_bg".into()),
            sprites: [("ace".to_string(), "missing_pose".to_string())].into(),
            bgm: Some("missing_track".into()),
        };
    }

    captures::take();
    pump_until_watch_drains(&mut app);
    let records = failure_lines();

    assert_eq!(records.len(), 3, "one warn per failed asset: {records:?}");
    assert!(records
        .iter()
        .all(|(level, _, _)| *level == log::Level::Warn));
    for (prefix, path) in [
        ("couldn't load a background", "backgrounds/missing_bg.png"),
        ("couldn't load a sprite", "sprites/ace/missing_pose.png"),
        ("couldn't load the music", "audio/missing_track.ogg"),
    ] {
        assert!(
            records
                .iter()
                .any(|(_, _, line)| line.starts_with(prefix) && line.contains(path)),
            "missing row for {prefix:?} ({path}): {records:?}"
        );
    }
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "silent degradations never notice"
    );

    // Re-applying the same staging must not warn again: the paths were already
    // reported, so the watch is deduplicated. `request_replay` is the
    // production way to force that re-application.
    app.world_mut()
        .run_system_once(
            |mut audio: ResMut<AppliedAudio>,
             mut visuals: ResMut<AppliedVisuals>,
             mut ui: ResMut<AppliedUi>| {
                crate::presentation::play::request_replay(&mut audio, &mut visuals, &mut ui);
            },
        )
        .unwrap();
    captures::take();
    app.update();
    pump_until_watch_drains(&mut app);
    assert!(
        failure_lines().is_empty(),
        "a repeated staging command must not warn again"
    );

    // Play continues: the runner still reaches its next wait.
    let mut runtime = app.world_mut().resource_mut::<Runtime>();
    let outcome = runtime.runner.run();
    runtime.staging.apply_all(&outcome.staged);
    assert!(
        outcome.wait.is_some(),
        "a missing asset never stops the runner"
    );
}

#[test]
fn a_missing_choice_sound_reports_row_6_once() {
    captures::install();
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    std::fs::remove_file(pack_root.join("assets/audio/sfx_select.ogg")).expect("remove the sfx");

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(AssetPlugin {
            file_path: pack_root.join("assets").to_string_lossy().into_owned(),
            ..default()
        })
        .init_asset::<bevy_kira_audio::AudioSource>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<Notice>()
        .init_resource::<AssetWatch>()
        .insert_resource(AudioChannel::<Sfx>::default())
        .insert_resource(RuntimeConfig::new(pack_root, None).with_dev_mode(true))
        .add_plugins(RuntimeCorePlugin)
        .add_systems(Update, report_silent_degradations);
    force_single_threaded(&mut app);
    app.update(); // load the pack, then show the title
    enter_play(&mut app);

    {
        let mut runtime = app.world_mut().resource_mut::<Runtime>();
        loop {
            let outcome = runtime.runner.run();
            runtime.staging.apply_all(&outcome.staged);
            match outcome.wait {
                Some(Wait::Say { .. }) => runtime.runner.advance_say().expect("advance"),
                Some(Wait::Choice { .. }) => break,
                Some(Wait::Decision { .. }) => {
                    runtime.runner.resolve_decision("cold").expect("resolve");
                }
                None => panic!("the script ended before a choice"),
            }
        }
    }

    captures::take();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Digit1);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
    pump_until_watch_drains(&mut app);

    let records = failure_lines();
    assert_eq!(
        records.len(),
        1,
        "one warn for the missing sfx: {records:?}"
    );
    assert_eq!(records[0].0, log::Level::Warn);
    assert!(
        records[0].2.starts_with("couldn't load the choice sound"),
        "{:?}",
        records[0].2
    );
    assert!(records[0].2.contains("audio/sfx_select.ogg"));
    assert!(!app.world().resource::<Notice>().is_showing());
}

#[test]
fn an_undecodable_sprite_warns_while_the_valid_background_still_loads() {
    captures::install();
    let dir = tempfile::tempdir().expect("temp dir");
    let assets = dir.path().join("assets");
    std::fs::create_dir_all(assets.join("backgrounds")).expect("backgrounds dir");
    std::fs::create_dir_all(assets.join("sprites/ace")).expect("sprites dir");
    // A readable sibling and an unreadable one; the loader rejects empty bytes.
    std::fs::write(assets.join("backgrounds/school_gate.png"), b"a background")
        .expect("valid background");
    std::fs::write(assets.join("sprites/ace/happy.png"), b"").expect("empty sprite");

    let mut app = asset_watch_app(assets);
    {
        let mut runtime = app.world_mut().resource_mut::<Runtime>();
        runtime.staging = StagingState {
            background: Some("school_gate".into()),
            sprites: [("ace".to_string(), "happy".to_string())].into(),
            bgm: None,
        };
    }

    captures::take();
    pump_until_watch_drains(&mut app);

    let records = failure_lines();
    assert_eq!(
        records.len(),
        1,
        "only the undecodable sprite warns: {records:?}"
    );
    assert_eq!(records[0].0, log::Level::Warn);
    assert!(
        records[0].2.starts_with("couldn't load a sprite"),
        "{:?}",
        records[0].2
    );
    assert!(!app.world().resource::<Notice>().is_showing());

    // The other visual stayed: the valid background decoded and is resident.
    let server = app.world().resource::<bevy::asset::AssetServer>().clone();
    let handle = server
        .get_handle::<Image>("backgrounds/school_gate.png")
        .expect("the background handle exists");
    assert!(
        app.world()
            .resource::<bevy::asset::Assets<Image>>()
            .contains(handle.id()),
        "a failed sibling must not clear the loaded background"
    );
}

// ---------------------------------------------------------------------------
// Esc menu overlay
// ---------------------------------------------------------------------------

fn menu(app: &App) -> &MenuState {
    app.world().resource::<MenuState>()
}

/// The one live-session predicate the title branches share.
fn live_session(app: &App) -> bool {
    menu::live_session(menu(app))
}

fn parked_position(app: &App) -> (usize, Option<Wait>) {
    let runtime = app.world().resource::<Runtime>();
    (runtime.runner.pc(), runtime.runner.wait().cloned())
}

/// Press Esc through the input system: open, back out, or close.
fn esc(app: &mut App) {
    press_and_update(app, KeyCode::Escape);
}

/// Click a control by driving the same `Interaction` the pointer would.
fn click(app: &mut App, action: MenuAction) {
    app.world_mut().spawn((action, Interaction::Pressed));
    app.update();
}

fn switch_mode(app: &mut App, mode: MenuMode) {
    click(app, MenuAction::SetMode(mode));
}

/// Play the demo script to its end through the app systems, taking the first
/// choice and the author fallback at the decide point.
fn play_to_end(app: &mut App) {
    for _ in 0..64 {
        match app.world().resource::<Runtime>().runner.wait().cloned() {
            Some(Wait::Say { .. }) => press_and_update(app, KeyCode::Space),
            Some(Wait::Choice { .. }) => press_and_update(app, KeyCode::Digit1),
            Some(Wait::Decision { .. }) => app.update(),
            None => return,
        }
    }
    panic!("the demo script never ended");
}

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

    // Up wraps to the Decisions group's last control; the back control is a
    // click/`Esc` affordance, not an arrow stop.
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
// The gallery surface: two doors, back-out and the empty state
// ---------------------------------------------------------------------------

/// A copied demo pack whose scenes declare no `bg`/`sprite` and whose manifest
/// declares no `title_backdrop`, so its gallery has no entries
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §6).
fn temp_pack_without_gallery_entries() -> tempfile::TempDir {
    let dir = temp_pack();
    let root = dir.path().join("pack");
    std::fs::write(
        root.join("pack.ron"),
        "(\n    name: \"unmei demo\",\n    entry: \"entry\",\n)\n",
    )
    .unwrap();
    for entry in std::fs::read_dir(root.join("scenes")).unwrap() {
        std::fs::remove_file(entry.unwrap().path()).unwrap();
    }
    std::fs::write(root.join("scenes/entry.unmei"), "Narrator: Nothing here.\n").unwrap();
    dir
}

/// Whether the built pause tree carries a real control with this action. The
/// `MenuAction` a `click` spawns carries no `Button`, so it never counts.
fn has_action(app: &App, action: MenuAction) -> bool {
    app.world().iter_entities().any(|entity| {
        entity.get::<MenuAction>() == Some(&action) && entity.get::<Button>().is_some()
    })
}

/// Build the pause tree once, initialising the structural cache a core-only
/// test world does not carry.
fn build_pause_tree(app: &mut App) {
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(app);
}

/// Repaint the still's caption and art for the current focus, as the real
/// presentation does every frame. The core test world registers no such system.
fn sync_still(app: &mut App) {
    app.world_mut()
        .run_system_once(crate::presentation::gallery::sync_gallery_still)
        .expect("sync_gallery_still runs");
}

/// The parent of the built control carrying `action`.
fn action_parent(app: &App, action: MenuAction) -> Entity {
    app.world()
        .iter_entities()
        .find_map(|entity| {
            (entity.get::<MenuAction>() == Some(&action))
                .then(|| entity.get::<ChildOf>().map(ChildOf::parent))
                .flatten()
        })
        .unwrap_or_else(|| panic!("no built control {action:?}"))
}

/// Open the overlay, then its `Gallery` control: the overlay's door into the
/// gallery. Assumes the overlay is closed.
fn open_overlay_gallery(app: &mut App) {
    esc(app);
    assert!(menu(app).open, "the overlay opened");
    assert_eq!(menu(app).view, MenuView::Slots);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::GalleryButton;
    press_and_update(app, KeyCode::Enter);
    assert_eq!(menu(app).view, MenuView::Gallery);
}

/// The title's `Gallery` row is enabled and caption-free
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §1/§7).
#[test]
fn the_title_gallery_row_is_enabled_and_caption_free() {
    let mut app = temp_title_app();
    build_pause_tree(&mut app);

    assert_eq!(
        title_label(&app, MenuAction::Gallery).as_deref(),
        Some(menu::GALLERY_HEADING),
        "the row carries no caption"
    );
    let gallery = app
        .world()
        .get::<TextColor>(text_entity(&app, menu::GALLERY_HEADING))
        .expect("the row label is tinted")
        .0;
    let settings = app
        .world()
        .get::<TextColor>(text_entity(&app, "Settings"))
        .expect("the Settings row label is tinted")
        .0;
    assert_eq!(
        gallery, settings,
        "Gallery renders in the same enabled tone as Settings"
    );
}

/// `Tab` cycles Save → Load → Timeline → Gallery → Settings in the slot grid,
/// and narrows to Save → Load in the gallery, Timeline and Settings views
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §1, § Timeline §1).
#[test]
fn tab_cycles_through_gallery_only_while_the_slot_grid_is_up() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    esc(&mut app);

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::SaveTab;
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

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::GalleryButton;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).view, MenuView::Gallery);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::SaveTab;
    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).focus, MenuFocus::LoadTab, "gallery: Save → Load");
    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).focus, MenuFocus::SaveTab, "gallery: Load → Save");

    click(&mut app, MenuAction::GalleryBack);
    click(&mut app, MenuAction::OpenSettings);
    assert_eq!(menu(&app).view, MenuView::Settings);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::SaveTab;
    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::LoadTab,
        "settings: Save → Load"
    );
    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::SaveTab,
        "settings: Load → Save"
    );
}

/// From the title's door no top bar is drawn, so `Tab` cannot reach an
/// invisible Save/Load control and flip the mode
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §1).
#[test]
fn tab_is_inert_on_the_titles_gallery_and_settings() {
    let mut title = temp_title_app();
    title.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleGallery;
    press_and_update(&mut title, KeyCode::Enter);
    assert_eq!(menu(&title).view, MenuView::Gallery);
    let focus = menu(&title).focus;
    press_and_update(&mut title, KeyCode::Tab);
    assert_eq!(menu(&title).focus, focus, "Tab is inert in the gallery");
    assert_eq!(menu(&title).mode, MenuMode::Save);

    let mut settings = temp_title_app();
    settings.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleSettings;
    press_and_update(&mut settings, KeyCode::Enter);
    assert_eq!(menu(&settings).view, MenuView::Settings);
    let focus = menu(&settings).focus;
    press_and_update(&mut settings, KeyCode::Tab);
    assert_eq!(menu(&settings).focus, focus, "Tab is inert in Settings");
    assert_eq!(menu(&settings).mode, MenuMode::Save);
}

/// The overlay's top bar renders `Gallery` immediately left of `Settings` only
/// while the slot grid is up (`docs/title-and-menu.md` § Gallery `### Behavior`
/// §1).
#[test]
fn the_overlay_top_bar_renders_gallery_left_of_settings_only_in_the_grid() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    esc(&mut app);
    build_pause_tree(&mut app);

    let gallery_parent = action_parent(&app, MenuAction::OpenGallery);
    let settings_parent = action_parent(&app, MenuAction::OpenSettings);
    assert_eq!(
        gallery_parent, settings_parent,
        "both controls share the top bar"
    );
    let children = app
        .world()
        .entity(gallery_parent)
        .get::<Children>()
        .expect("the top bar has children");
    let index = |action: MenuAction| {
        children
            .iter()
            .position(|child| app.world().get::<MenuAction>(child) == Some(&action))
            .unwrap_or_else(|| panic!("no {action:?} in the top bar"))
    };
    assert_eq!(
        index(MenuAction::OpenGallery) + 1,
        index(MenuAction::OpenSettings),
        "Gallery sits immediately left of Settings"
    );

    // Settings carries neither sub-view control.
    click(&mut app, MenuAction::OpenSettings);
    build_pause_tree(&mut app);
    assert!(!has_action(&app, MenuAction::OpenGallery));
    assert!(!has_action(&app, MenuAction::OpenSettings));

    // The gallery carries no Settings control.
    click(&mut app, MenuAction::BackToSlots);
    click(&mut app, MenuAction::OpenGallery);
    build_pause_tree(&mut app);
    assert!(!has_action(&app, MenuAction::OpenGallery));
    assert!(!has_action(&app, MenuAction::OpenSettings));
}

/// The overlay's top-bar `Gallery` control renders the spec's copy verbatim in
/// the bar's sans face, like its `Settings` neighbour
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §7, § Type system §2/§4).
#[test]
fn the_overlay_gallery_control_renders_the_copy_string() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    esc(&mut app);
    build_pause_tree(&mut app);

    let control = action_entity(&app, MenuAction::OpenGallery);
    assert_eq!(
        app.world()
            .get::<Text>(control)
            .expect("the control carries its label")
            .0,
        "Gallery",
        "the overlay control's copy is exact"
    );
    let settings = action_entity(&app, MenuAction::OpenSettings);
    assert_eq!(
        face_handle(&app, control),
        face_handle(&app, settings),
        "Gallery and Settings share the top bar's sans face"
    );
    assert_eq!(
        face_handle(&app, control),
        app.world()
            .resource::<crate::presentation::style::TextFaces>()
            .sans_regular,
        "the control label is the sans regular face"
    );
}

/// The back control is door-aware, and either door returns to where it came
/// from (`docs/title-and-menu.md` § Gallery `### Behavior` §3/§7).
#[test]
fn the_gallery_back_control_is_door_aware() {
    // Overlay door: `← Back to slots`.
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);
    assert_eq!(menu(&app).gallery_origin, menu::MenuOrigin::Slots);
    build_pause_tree(&mut app);
    assert!(all_texts(&app)
        .iter()
        .any(|text| text == "\u{2190} Back to slots"));
    click(&mut app, MenuAction::GalleryBack);
    assert_eq!(menu(&app).view, MenuView::Slots);
    assert!(menu(&app).open, "the overlay is still up");

    // Title door: `← Back to title`.
    let mut title = temp_title_app();
    title.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleGallery;
    press_and_update(&mut title, KeyCode::Enter);
    assert_eq!(menu(&title).gallery_origin, menu::MenuOrigin::Title);
    build_pause_tree(&mut title);
    assert!(all_texts(&title)
        .iter()
        .any(|text| text == "\u{2190} Back to title"));
    click(&mut title, MenuAction::GalleryBack);
    assert_eq!(menu(&title).view, MenuView::Title);
    assert!(menu(&title).title_up());
}

/// `Esc` unwinds exactly one level per press: the gallery to its door, the
/// overlay's grid to closed, and the title's door ends where `Esc` is inert
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §3).
#[test]
fn esc_unwinds_the_gallery_one_level_per_press() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);

    esc(&mut app);
    assert_eq!(menu(&app).view, MenuView::Slots);
    assert!(menu(&app).open, "the first Esc backed out to the grid");
    esc(&mut app);
    assert!(!menu(&app).open, "the second Esc closed the overlay");

    let mut title = temp_title_app();
    title.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleGallery;
    press_and_update(&mut title, KeyCode::Enter);
    esc(&mut title);
    assert_eq!(menu(&title).view, MenuView::Title);
    assert!(menu(&title).title_up());
    esc(&mut title);
    assert!(
        menu(&title).title_up(),
        "Esc is inert on the title, ending the chain"
    );
}

/// Settings offers no gallery control and the gallery offers no Settings
/// control, from either door (`docs/title-and-menu.md` § Gallery `### Behavior`
/// §1).
#[test]
fn neither_settings_nor_the_gallery_is_a_door_to_the_other() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    esc(&mut app);

    click(&mut app, MenuAction::OpenSettings);
    build_pause_tree(&mut app);
    assert!(
        !has_action(&app, MenuAction::OpenGallery),
        "Settings carries no gallery control"
    );
    click(&mut app, MenuAction::BackToSlots);
    click(&mut app, MenuAction::OpenGallery);
    build_pause_tree(&mut app);
    assert!(
        !has_action(&app, MenuAction::OpenSettings),
        "the gallery carries no Settings control"
    );

    // Same from the title's door.
    let mut title = temp_title_app();
    title.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleSettings;
    press_and_update(&mut title, KeyCode::Enter);
    build_pause_tree(&mut title);
    assert!(!has_action(&title, MenuAction::OpenGallery));
    click(&mut title, MenuAction::BackToSlots);
    title.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleGallery;
    press_and_update(&mut title, KeyCode::Enter);
    build_pause_tree(&mut title);
    assert!(!has_action(&title, MenuAction::OpenSettings));
}

/// The gallery opens at every parked wait and leaves the session unchanged
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §2).
#[test]
fn the_gallery_opens_at_every_parked_wait_leaving_the_session_unchanged() {
    // say-wait
    let pack = temp_pack();
    let mut say = app_for_pack(pack.path().join("pack"));
    assert!(matches!(
        say.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Say { .. })
    ));
    let before = parked_position(&say);
    open_overlay_gallery(&mut say);
    assert_eq!(parked_position(&say), before);
    assert!(!autosave_path(&say).exists());

    // choice-wait
    let pack = temp_pack();
    let mut choice = app_for_pack(pack.path().join("pack"));
    press_and_update(&mut choice, KeyCode::Space);
    press_and_update(&mut choice, KeyCode::Space);
    assert!(matches!(
        choice.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Choice { .. })
    ));
    let before = parked_position(&choice);
    open_overlay_gallery(&mut choice);
    assert_eq!(parked_position(&choice), before);

    // end of scene (no parked wait)
    let pack = temp_pack();
    let mut ended = app_for_pack(pack.path().join("pack"));
    play_to_end(&mut ended);
    assert!(ended.world().resource::<Runtime>().runner.wait().is_none());
    let before = parked_position(&ended);
    open_overlay_gallery(&mut ended);
    assert_eq!(parked_position(&ended), before);

    // decision-wait with the request in flight: the request survives too.
    let fake = FakeResolver::new(model_answer("warm"));
    let (mut inflight, _dir) = app_parked_on_decision(&fake);
    let before = parked_position(&inflight);
    open_overlay_gallery(&mut inflight);
    assert_eq!(parked_position(&inflight), before);
    assert!(inflight.world().resource::<PendingDecision>().0.is_some());
    assert_eq!(fake.calls(), 1, "the gallery never re-fires or cancels");

    // a held answer stays held
    let fake = FakeResolver::new(model_answer("warm"));
    let (mut held_app, _dir) = app_parked_on_decision(&fake);
    esc(&mut held_app);
    fake.release();
    pump_until(&mut held_app, held);
    let before = parked_position(&held_app);
    held_app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::GalleryButton;
    press_and_update(&mut held_app, KeyCode::Enter);
    assert_eq!(menu(&held_app).view, MenuView::Gallery);
    assert!(held(&held_app), "the held answer stays held");
    assert_eq!(parked_position(&held_app), before);
}

/// Opening, walking and closing the gallery writes nothing and reads no slot
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §1).
#[test]
fn opening_walking_and_closing_the_gallery_writes_nothing_and_reads_no_slot() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    esc(&mut app);
    let slots_before = menu(&app).slots.clone();

    // A slot appears on disk after the overlay read the list: the gallery must
    // not re-read it.
    {
        let runtime = app.world().resource::<Runtime>();
        let mut save = SaveFile::capture(&runtime.runner, &runtime.staging);
        save.saved_at = Some(1);
        save::write_slot(runtime.runner.pack(), Slot::Manual(2), &save).unwrap();
    }
    let settings = settings_path(&app);

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::GalleryButton;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).view, MenuView::Gallery);
    assert_eq!(menu(&app).slots, slots_before, "opening reads no slot");

    // Walk the top bar and the grid: moving focus writes nothing either.
    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).focus, MenuFocus::SaveTab);
    press_and_update(&mut app, KeyCode::ArrowRight);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(0));
    press_and_update(&mut app, KeyCode::ArrowRight);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(1));

    click(&mut app, MenuAction::GalleryBack);
    assert_eq!(menu(&app).view, MenuView::Slots);
    assert_eq!(menu(&app).slots, slots_before, "closing reads no slot");
    esc(&mut app);
    assert!(!menu(&app).open);

    assert!(!settings.exists(), "the gallery never flushes settings");
    assert!(
        !autosave_path(&app).exists(),
        "the gallery writes no autosave"
    );
}

/// A pack that declares no entries shows the header and the single
/// `No images in this pack` line, with no grid, error or Notice, from either
/// door (`docs/title-and-menu.md` § Gallery `### Behavior` §6/§7).
#[test]
fn an_empty_pack_draws_the_header_and_the_single_empty_line() {
    let dir = temp_pack_without_gallery_entries();
    let mut app = app_for_pack(dir.path().join("pack"));
    esc(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::GalleryButton;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).view, MenuView::Gallery);
    build_pause_tree(&mut app);

    let texts = all_texts(&app);
    assert!(
        texts.iter().any(|text| text == menu::GALLERY_EMPTY),
        "the empty line renders: {texts:?}"
    );
    assert!(texts.iter().any(|text| text == menu::GALLERY_HEADING));
    assert!(!has_action(&app, MenuAction::Card(0)), "no grid draws");
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "the empty state raises no Notice"
    );

    // The heading is sans SemiBold; the empty-state line is sans prose.
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();
    assert_eq!(
        face_handle(&app, text_entity(&app, menu::GALLERY_HEADING)),
        faces.sans_semibold,
        "the heading is sans SemiBold"
    );
    assert_eq!(
        face_handle(&app, text_entity(&app, menu::GALLERY_EMPTY)),
        faces.sans_regular,
        "the empty-state line is sans prose"
    );

    // The title's door draws the same empty state.
    let dir = temp_pack_without_gallery_entries();
    let mut title = boot_app(dir.path().join("pack"), None);
    title.insert_resource(TempPack(dir));
    title.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleGallery;
    press_and_update(&mut title, KeyCode::Enter);
    build_pause_tree(&mut title);
    let texts = all_texts(&title);
    assert!(
        texts.iter().any(|text| text == menu::GALLERY_EMPTY),
        "the empty line renders from the title's door: {texts:?}"
    );
    assert!(!has_action(&title, MenuAction::Card(0)), "no grid draws");
}

// ---------------------------------------------------------------------------
// The gallery grid and its walk
// ---------------------------------------------------------------------------

/// The card a gallery arrow move lands on for each direction and boundary. The
/// walk does not wrap; `↑`/`↓` step one row and clamp to the last entry when
/// the destination row is short (`docs/title-and-menu.md` § Gallery
/// `### Layout` §5).
#[test]
fn gallery_move_steps_one_card_or_row_and_clamps_without_wrapping() {
    use menu::GalleryDir::{Down, Left, Right, Up};

    // A three-column grid of seven entries: rows 0-2 / 3-5 / 6.
    assert_eq!(menu::gallery_move(0, 7, 3, Right), Some(1));
    assert_eq!(menu::gallery_move(6, 7, 3, Left), Some(5));
    assert_eq!(
        menu::gallery_move(0, 7, 3, Left),
        None,
        "no wrap at the start"
    );
    assert_eq!(
        menu::gallery_move(6, 7, 3, Right),
        None,
        "no wrap at the end"
    );
    assert_eq!(menu::gallery_move(0, 7, 3, Down), Some(3), "one row down");
    assert_eq!(menu::gallery_move(3, 7, 3, Down), Some(6));
    assert_eq!(
        menu::gallery_move(4, 7, 3, Down),
        Some(6),
        "a short destination row clamps to the last entry"
    );
    assert_eq!(menu::gallery_move(2, 7, 3, Down), Some(5));
    assert_eq!(
        menu::gallery_move(6, 7, 3, Down),
        None,
        "the last card's down is a no-op"
    );
    assert_eq!(menu::gallery_move(3, 7, 3, Up), Some(0));
    assert_eq!(menu::gallery_move(6, 7, 3, Up), Some(3));
    assert_eq!(
        menu::gallery_move(0, 7, 3, Up),
        None,
        "the first card's up is a no-op"
    );

    // Degenerate inputs never panic or invent a card.
    assert_eq!(menu::gallery_move(0, 0, 3, Down), None);
    assert_eq!(
        menu::gallery_move(0, 7, 0, Down),
        Some(1),
        "a zero column count steps one"
    );
    assert_eq!(
        menu::gallery_move(9, 7, 3, Down),
        None,
        "an out-of-range focus is inert"
    );
}

/// The grid packs `minmax(300px, 1fr)` tracks with 20px gutters: three across
/// at 1280×720 from both doors (`docs/title-and-menu.md` § Gallery `### Layout`
/// §4).
#[test]
fn gallery_column_count_is_three_across_at_the_reference_window() {
    // The title door's surface is the window less its 32px padding.
    assert_eq!(
        crate::presentation::gallery::gallery_column_count(1280.0 - 64.0),
        3
    );
    // The overlay door's surface is the window less the 240px rail and 24px padding.
    assert_eq!(
        crate::presentation::gallery::gallery_column_count(1280.0 - 240.0 - 48.0),
        3
    );
    // A fullscreen-width surface packs more columns.
    assert_eq!(
        crate::presentation::gallery::gallery_column_count(1856.0),
        5
    );
    // A degenerate width still draws one column.
    assert_eq!(crate::presentation::gallery::gallery_column_count(0.0), 1);
}

/// The column count is read from the grid's laid-out width in logical pixels,
/// so a HiDPI scale factor cannot shift the walk's row step.
#[test]
fn sync_gallery_columns_reads_the_grids_logical_width() {
    let mut app = App::new();
    app.init_resource::<MenuState>();
    app.world_mut().spawn((
        crate::presentation::gallery::GalleryGrid,
        ComputedNode {
            size: Vec2::new(2432.0, 0.0),
            inverse_scale_factor: 0.5,
            ..default()
        },
    ));
    app.add_systems(Update, crate::presentation::gallery::sync_gallery_columns);
    app.update();
    assert_eq!(
        menu(&app).gallery_columns,
        3,
        "2432 physical at 2x is 1216 logical: three tracks"
    );
}

/// The card entity carrying one gallery action.
fn action_entity(app: &App, action: MenuAction) -> Entity {
    app.world()
        .iter_entities()
        .find_map(|entity| {
            (entity.get::<MenuAction>() == Some(&action) && entity.get::<Button>().is_some())
                .then_some(entity.id())
        })
        .unwrap_or_else(|| panic!("no built control {action:?}"))
}

/// A gallery card's 4:3 art box.
fn gallery_art_box(app: &App, index: usize) -> Entity {
    let card = action_entity(app, MenuAction::GalleryCard(index));
    app.world()
        .get::<Children>(card)
        .expect("the card has children")
        .iter()
        .find(|child| {
            app.world()
                .get::<crate::presentation::gallery::GalleryArtBox>(*child)
                .is_some()
        })
        .expect("the card carries an art box")
}

/// The built gallery body's scroll container.
fn gallery_body(app: &App) -> Entity {
    app.world()
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<crate::presentation::gallery::GalleryBody>()
                .map(|_| entity.id())
        })
        .expect("the gallery body draws")
}

/// The built gallery body's current scroll offset.
fn gallery_body_scroll(app: &App) -> Vec2 {
    app.world()
        .get::<ScrollPosition>(gallery_body(app))
        .expect("the gallery body carries a ScrollPosition")
        .0
}

/// The entry index of one gallery label for the running pack.
fn entry_index(app: &App, label: &str) -> usize {
    let runtime = app.world().resource::<Runtime>();
    runtime
        .runner
        .pack()
        .gallery_entries()
        .iter()
        .position(|entry| entry.label() == label)
        .unwrap_or_else(|| panic!("no gallery entry {label:?}"))
}

/// A demo-pack copy with extra declared backgrounds, so the gallery grid has
/// enough cards to scroll and walk multiple rows.
fn temp_pack_with_backgrounds(names: &[&str]) -> tempfile::TempDir {
    let dir = temp_pack();
    let path = dir.path().join("pack/scenes/first_meeting.unmei");
    let scene = std::fs::read_to_string(&path).unwrap();
    let mut text = String::new();
    for name in names {
        text.push_str(&format!("bg {name}\n"));
    }
    text.push_str(&scene);
    std::fs::write(&path, text).unwrap();
    dir
}

/// The gallery body is a scroll container over a `minmax(300px, 1fr)` grid with
/// a 20px gap and one framed card per entry, each with a 4:3 art box
/// (`docs/title-and-menu.md` § Gallery `### Layout` §1/§2/§4).
#[test]
fn the_gallery_grid_is_three_across_with_a_twenty_pixel_gap() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);
    build_pause_tree(&mut app);

    let body = gallery_body(&app);
    let body_node = app.world().get::<Node>(body).unwrap();
    assert_eq!(body_node.overflow, Overflow::scroll_y());
    assert_eq!(body_node.min_height, Val::Px(0.0));
    // The body lets the grid keep its natural height: a cross-axis stretched or
    // shrunk grid hides its overflow, so the focused-card scroll can never move
    // (`docs/title-and-menu.md` § Gallery `### Layout` §1/§4).
    assert_eq!(body_node.flex_direction, FlexDirection::Column);
    assert_eq!(body_node.align_items, AlignItems::Start);
    assert!(
        app.world().get::<ScrollArea>(body).is_some(),
        "the body is a scroll area"
    );

    let grid = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<crate::presentation::gallery::GalleryGrid>()
                .map(|_| entity.id())
        })
        .expect("the grid draws");
    let node = app.world().get::<Node>(grid).unwrap();
    assert_eq!(node.display, Display::Grid);
    assert_eq!(node.column_gap, Val::Px(20.0));
    assert_eq!(node.row_gap, Val::Px(20.0));
    let expected: Vec<RepeatedGridTrack> = RepeatedGridTrack::minmax(
        GridTrackRepetition::AutoFill,
        MinTrackSizingFunction::Px(300.0),
        MaxTrackSizingFunction::Fraction(1.0),
    );
    assert_eq!(node.grid_template_columns, expected);
    assert_eq!(
        node.flex_shrink, 0.0,
        "the grid keeps its natural height inside the scrolling body"
    );

    // One card per demo entry, each with a 4:3 art box and its label.
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();
    for (index, label) in ["school_gate", "ace \u{b7} happy", "ace \u{b7} neutral"]
        .iter()
        .enumerate()
    {
        assert!(has_action(&app, MenuAction::GalleryCard(index)));
        let art = gallery_art_box(&app, index);
        assert_eq!(
            app.world().get::<Node>(art).unwrap().aspect_ratio,
            Some(4.0 / 3.0)
        );
        assert!(
            all_texts(&app).iter().any(|text| text == label),
            "the {label:?} card label renders"
        );
        assert_eq!(
            face_handle(&app, text_entity(&app, label)),
            faces.mono_medium,
            "the {label:?} card label is mono"
        );
    }
}

/// The title's door draws the same three-across `minmax(300px, 1fr)` grid with
/// a 20px gap as the overlay's: the door changes the backdrop and the way back,
/// never the grid (`docs/title-and-menu.md` § Gallery `### Layout` §1/§4).
#[test]
fn the_title_doors_grid_is_the_same_three_across_layout() {
    let pack = temp_pack();
    let mut title = boot_app(pack.path().join("pack"), None);
    title.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleGallery;
    press_and_update(&mut title, KeyCode::Enter);
    assert_eq!(menu(&title).view, MenuView::Gallery);
    build_pause_tree(&mut title);

    let grid = title
        .world()
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<crate::presentation::gallery::GalleryGrid>()
                .map(|_| entity.id())
        })
        .expect("the grid draws from the title's door");
    let node = title.world().get::<Node>(grid).unwrap();
    assert_eq!(node.display, Display::Grid);
    assert_eq!(node.column_gap, Val::Px(20.0));
    assert_eq!(node.row_gap, Val::Px(20.0));
    let expected: Vec<RepeatedGridTrack> = RepeatedGridTrack::minmax(
        GridTrackRepetition::AutoFill,
        MinTrackSizingFunction::Px(300.0),
        MaxTrackSizingFunction::Fraction(1.0),
    );
    assert_eq!(node.grid_template_columns, expected);
    assert_eq!(
        node.flex_shrink, 0.0,
        "the grid keeps its natural height inside the scrolling body"
    );
    let title_body = title.world().get::<Node>(gallery_body(&title)).unwrap();
    assert_eq!(
        title_body.overflow,
        Overflow::scroll_y(),
        "the body scrolls from the title's door too"
    );
    assert_eq!(title_body.flex_direction, FlexDirection::Column);
    assert_eq!(title_body.align_items, AlignItems::Start);

    // At 1280×720 both doors' surfaces pack exactly three tracks with a 20px
    // gutter: the window less its padding, and the overlay's column less the
    // rail and padding.
    assert_eq!(
        crate::presentation::gallery::gallery_column_count(1280.0 - 64.0),
        3
    );
    assert_eq!(
        crate::presentation::gallery::gallery_column_count(1280.0 - 240.0 - 48.0),
        3
    );
}

/// The gallery reuses the overlay's card vocabulary — the slot card's fill and
/// its 2px border — with the entry's label as a footer line under the art, and
/// the cards stand in the pack's entry order, so the walk follows it
/// (`docs/title-and-menu.md` § Gallery `### Layout` §2, § Entries §3,
/// § Menu overlay §1).
#[test]
fn gallery_cards_reuse_the_panel_vocabulary_in_entry_order() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    // The slot card's panel treatment, captured before the gallery opens.
    esc(&mut app);
    build_pause_tree(&mut app);
    let slot = action_entity(&app, MenuAction::Card(0));
    let slot_fill = *app.world().get::<BackgroundColor>(slot).unwrap();
    let slot_border = app.world().get::<Node>(slot).unwrap().border;

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::GalleryButton;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).view, MenuView::Gallery);
    build_pause_tree(&mut app);

    let entries = app
        .world()
        .resource::<Runtime>()
        .runner
        .pack()
        .gallery_entries();
    for (index, entry) in entries.iter().enumerate() {
        let card = action_entity(&app, MenuAction::GalleryCard(index));
        assert_eq!(
            *app.world().get::<BackgroundColor>(card).unwrap(),
            slot_fill,
            "card {index} uses the overlay's panel fill"
        );
        assert_eq!(
            app.world().get::<Node>(card).unwrap().border,
            slot_border,
            "card {index} uses the overlay's hairline border"
        );
        let children = app.world().get::<Children>(card).unwrap();
        let art = children
            .iter()
            .position(|child| {
                app.world()
                    .get::<crate::presentation::gallery::GalleryArtBox>(child)
                    .is_some()
            })
            .expect("the card carries its 4:3 art box");
        let footer = children
            .iter()
            .position(|child| app.world().get::<Text>(child).is_some())
            .expect("the card carries its footer label");
        assert!(
            art < footer,
            "card {index}'s label is a footer line under the art"
        );
        let label = app.world().get::<Text>(children[footer]).unwrap().0.clone();
        assert_eq!(
            label,
            entry.label(),
            "card {index} shows the entry it stands for"
        );
    }
}

/// `sync_gallery_art` sizes a loaded texture contain-fit inside the 4:3 box:
/// the free axis letterboxes and the node keeps the image's aspect
/// (`docs/title-and-menu.md` § Gallery `### Layout` §3).
#[test]
fn sync_gallery_art_contain_fits_a_loaded_texture() {
    let mut app = App::new();
    app.init_resource::<Assets<Image>>();
    let art_box = app
        .world_mut()
        .spawn((
            crate::presentation::gallery::GalleryArtBox,
            Outline::new(Val::Px(1.0), Val::Px(0.0), Color::WHITE),
        ))
        .id();
    let background = add_test_image(&mut app, 1280, 720);
    let sprite = add_test_image(&mut app, 256, 512);

    // A 16:9 background fills the width and letterboxes to 75% height; a 1:2
    // sprite fills the height and pillarboxes to 37.5% width.
    let wide = app
        .world_mut()
        .spawn((
            crate::presentation::gallery::GalleryArt,
            ImageNode::new(background),
            Node::default(),
            ChildOf(art_box),
        ))
        .id();
    let tall = app
        .world_mut()
        .spawn((
            crate::presentation::gallery::GalleryArt,
            ImageNode::new(sprite),
            Node::default(),
            ChildOf(art_box),
        ))
        .id();
    app.add_systems(Update, crate::presentation::gallery::sync_gallery_art);
    app.update();

    assert_eq!(
        app.world().get::<Node>(wide).unwrap().width,
        Val::Percent(100.0)
    );
    assert_eq!(
        app.world().get::<Node>(wide).unwrap().height,
        Val::Percent(75.0)
    );
    assert_eq!(
        app.world().get::<Node>(tall).unwrap().width,
        Val::Percent(37.5)
    );
    assert_eq!(
        app.world().get::<Node>(tall).unwrap().height,
        Val::Percent(100.0)
    );
    assert_eq!(
        app.world().get::<Outline>(art_box).unwrap().color,
        Color::NONE,
        "once a texture decodes the placeholder frame clears"
    );
}

/// Insert an in-memory image of a given size into the app's `Assets<Image>`.
fn add_test_image(app: &mut App, width: u32, height: u32) -> Handle<Image> {
    let mut image = Image::default();
    image.texture_descriptor.size = Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    app.world_mut().resource_mut::<Assets<Image>>().add(image)
}

/// Focus is the overlay's 2px `FOCUS` border and nothing else: the focused
/// card and an unfocused one share every other property, and focus starts on
/// the first card (`docs/title-and-menu.md` § Gallery `### Layout` §5).
#[test]
fn the_focused_gallery_card_differs_only_by_its_focus_border() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);
    build_pause_tree(&mut app);

    let focused = action_entity(&app, MenuAction::GalleryCard(0));
    let plain = action_entity(&app, MenuAction::GalleryCard(1));
    let focus_color = app.world().get::<BorderColor>(focused).unwrap().top;
    assert_ne!(focus_color, Color::NONE, "the first card starts focused");
    assert_eq!(
        app.world().get::<BorderColor>(plain).unwrap().top,
        Color::NONE
    );
    assert_eq!(
        app.world().get::<Node>(focused).unwrap(),
        app.world().get::<Node>(plain).unwrap(),
        "focus changes no size or spacing"
    );
    assert_eq!(
        app.world().get::<BackgroundColor>(focused).unwrap(),
        app.world().get::<BackgroundColor>(plain).unwrap(),
        "focus changes no fill or brightness"
    );

    // Moving the walk moves the one ring.
    press_and_update(&mut app, KeyCode::ArrowRight);
    app.world_mut()
        .run_system_once(crate::presentation::sync_menu_focus)
        .expect("the focus system runs");
    assert_eq!(
        app.world().get::<BorderColor>(plain).unwrap().top,
        focus_color,
        "the moved-to card takes the ring"
    );
    assert_eq!(
        app.world().get::<BorderColor>(focused).unwrap().top,
        Color::NONE,
        "the left card drops the ring"
    );
    assert_eq!(
        app.world().get::<Node>(focused).unwrap(),
        app.world().get::<Node>(plain).unwrap(),
        "the move still changes nothing but the border colour"
    );
}

/// The grid legend renders exactly the spec's string, right-aligned and mono,
/// and only when entries exist (`docs/title-and-menu.md` § Gallery
/// `### Behavior` §7, `### Layout` §1).
#[test]
fn the_gallery_grid_legend_renders_right_aligned_and_mono() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);
    build_pause_tree(&mut app);

    let legend = text_entity(&app, menu::GALLERY_GRID_LEGEND);
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();
    assert_eq!(face_handle(&app, legend), faces.mono_medium);
    assert_eq!(
        face_handle(&app, text_entity(&app, menu::GALLERY_HEADING)),
        faces.sans_semibold,
        "the heading keeps the Settings panel title's face"
    );

    // A spacer before the legend pushes it to the header's right edge.
    let header = app.world().get::<ChildOf>(legend).unwrap().parent();
    let children = app.world().get::<Children>(header).unwrap();
    let position = children
        .iter()
        .position(|child| child == legend)
        .expect("the legend is in the header");
    assert_eq!(
        position,
        children.len() - 1,
        "the legend is last in the row"
    );
    assert_eq!(
        app.world()
            .get::<Node>(children[position - 1])
            .unwrap()
            .flex_grow,
        1.0,
        "a spacer right-aligns the legend"
    );

    // The header never scrolls: it is not inside the body.
    let body = gallery_body(&app);
    let mut ancestor = app
        .world()
        .get::<ChildOf>(legend)
        .map(|child| child.parent());
    while let Some(entity) = ancestor {
        assert_ne!(entity, body, "the header sits outside the scroll body");
        ancestor = app
            .world()
            .get::<ChildOf>(entity)
            .map(|child| child.parent());
    }

    // The empty state draws no legend.
    let dir = temp_pack_without_gallery_entries();
    let mut empty = app_for_pack(dir.path().join("pack"));
    esc(&mut empty);
    empty.world_mut().resource_mut::<MenuState>().focus = MenuFocus::GalleryButton;
    press_and_update(&mut empty, KeyCode::Enter);
    build_pause_tree(&mut empty);
    assert!(
        !all_texts(&empty)
            .iter()
            .any(|text| text == menu::GALLERY_GRID_LEGEND),
        "the empty state shows no grid legend"
    );
}

/// Focus starts on the first card at every open, and the arrows walk the grid:
/// `←`/`→` step one card, `↑`/`↓` step one row and clamp on a short row, the
/// walk does not wrap and digits do nothing (`docs/title-and-menu.md`
/// § Gallery `### Behavior` §4, `### Layout` §5).
#[test]
fn the_gallery_walk_starts_on_the_first_card_and_does_not_wrap() {
    // Seven entries at three columns: rows 0-2 / 3-5 / 6.
    let dir = temp_pack_with_backgrounds(&["g1", "g2", "g3", "g4"]);
    let mut app = app_for_pack(dir.path().join("pack"));
    open_overlay_gallery(&mut app);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::GalleryCard(0),
        "focus starts on the first card"
    );

    press_and_update(&mut app, KeyCode::ArrowRight);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(1));
    press_and_update(&mut app, KeyCode::ArrowLeft);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(0));
    press_and_update(&mut app, KeyCode::ArrowLeft);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::GalleryCard(0),
        "no wrap at the start"
    );

    // Down steps one row: 0 -> 3 -> 6; the last card's down is a no-op.
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(3));
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(6));
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::GalleryCard(6),
        "no wrap at the end"
    );

    // Up steps one row: 6 -> 3 -> 0; the first card's up is a no-op.
    press_and_update(&mut app, KeyCode::ArrowUp);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(3));
    press_and_update(&mut app, KeyCode::ArrowUp);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(0));
    press_and_update(&mut app, KeyCode::ArrowUp);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::GalleryCard(0),
        "no wrap at the start"
    );

    // A short destination row clamps: 4 (row 1, col 1) down lands on 6, and
    // 2 (row 0, col 2) down lands on the row's 5.
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::GalleryCard(4);
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(6));
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::GalleryCard(2);
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(5));

    // A digit does nothing in the grid.
    let before = menu(&app).focus;
    press_and_update(&mut app, KeyCode::Digit1);
    assert_eq!(menu(&app).focus, before, "a digit does nothing");

    // Re-opening starts on the first card again.
    click(&mut app, MenuAction::GalleryBack);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::GalleryButton;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::GalleryCard(0),
        "every open starts on the first card"
    );

    // The title's door starts on the first card too.
    let dir = temp_pack_with_backgrounds(&["g1", "g2", "g3", "g4"]);
    let mut title = boot_app(dir.path().join("pack"), None);
    title.insert_resource(TempPack(dir));
    title.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleGallery;
    press_and_update(&mut title, KeyCode::Enter);
    assert_eq!(menu(&title).view, MenuView::Gallery);
    assert_eq!(menu(&title).focus, MenuFocus::GalleryCard(0));
}

// ---------------------------------------------------------------------------
// The still view
// ---------------------------------------------------------------------------

/// The label of the entry the still is showing, from the runtime pack's entry
/// order and the menu's focused card.
fn still_label(app: &App) -> String {
    let index = match menu(app).focus {
        MenuFocus::GalleryCard(index) => index,
        other => panic!("the still's focus is a gallery card, got {other:?}"),
    };
    app.world()
        .resource::<Runtime>()
        .runner
        .pack()
        .gallery_entries()
        .get(index)
        .expect("the focused entry exists")
        .label()
}

/// Open the still from the grid on one entry index through `Enter`.
fn open_still(app: &mut App, index: usize) {
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::GalleryCard(index);
    press_and_update(app, KeyCode::Enter);
    assert!(menu(app).gallery_still, "the still opened");
}

/// One still browse step wraps at both ends; the gallery's entry order is the
/// still's order (`docs/title-and-menu.md` § Gallery `### Behavior` §5).
#[test]
fn gallery_browse_steps_and_wraps_at_both_ends() {
    assert_eq!(menu::gallery_browse(1, 3, 1), Some(2));
    assert_eq!(menu::gallery_browse(1, 3, -1), Some(0));
    assert_eq!(menu::gallery_browse(2, 3, 1), Some(0), "wraps forward");
    assert_eq!(menu::gallery_browse(0, 3, -1), Some(2), "wraps back");
    assert_eq!(menu::gallery_browse(0, 0, 1), None, "no entries, no step");
}

/// `Enter`, `Space` and a click each open the still on the focused entry, from
/// either door (`docs/title-and-menu.md` § Gallery `### Behavior` §4/§5/§8).
#[test]
fn enter_space_and_click_each_open_the_still_for_the_focused_entry() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);

    // Enter on the first card opens the still on it, and the label is up.
    press_and_update(&mut app, KeyCode::Enter);
    assert!(menu(&app).gallery_still, "Enter opens the still");
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(0));
    build_pause_tree(&mut app);
    let caption = still_label(&app);
    assert!(
        all_texts(&app).iter().any(|text| text == &caption),
        "the still caption renders: {caption:?}"
    );

    // Space opens it on the entry the walk moved to.
    esc(&mut app);
    assert!(!menu(&app).gallery_still, "Esc closes the still");
    press_and_update(&mut app, KeyCode::ArrowRight);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(1));
    press_and_update(&mut app, KeyCode::Space);
    assert!(menu(&app).gallery_still, "Space opens the still");
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(1));

    // A click on a card opens it on that entry too.
    esc(&mut app);
    click(&mut app, MenuAction::GalleryCard(2));
    assert!(menu(&app).gallery_still, "a click opens the still");
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(2));

    // The title's door opens it the same way.
    let mut title = boot_app(pack.path().join("pack"), None);
    title.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleGallery;
    press_and_update(&mut title, KeyCode::Enter);
    assert_eq!(menu(&title).view, MenuView::Gallery);
    press_and_update(&mut title, KeyCode::Enter);
    assert!(
        menu(&title).gallery_still,
        "the title's door opens the still"
    );
    assert!(menu(&title).gallery_origin == menu::MenuOrigin::Title);
}

/// The still's caption is the entry label verbatim in the mono face, the still
/// legend renders exactly the spec's string, and the caption sits centred below
/// the art box in the dim tone with the legend in the faint tone, never over the
/// art (`docs/title-and-menu.md` § Gallery `### Behavior` §5/§7, `### Layout`
/// §6).
#[test]
fn the_still_caption_is_the_label_verbatim_mono_with_the_legend_under_it() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);
    // Capture the grid's own label for the entry the still will show, before
    // the still replaces the grid.
    build_pause_tree(&mut app);
    let card_label = flattened_text(&app, action_entity(&app, MenuAction::GalleryCard(1)));
    open_still(&mut app, 1);
    build_pause_tree(&mut app);

    let label = still_label(&app);
    let caption = text_entity(&app, &label);
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();
    assert_eq!(
        face_handle(&app, caption),
        faces.mono_medium,
        "the still caption is mono"
    );
    assert_eq!(
        card_label, label,
        "the still's caption is the card's label, verbatim"
    );
    let legend = text_entity(&app, menu::GALLERY_STILL_LEGEND);
    assert_eq!(
        face_handle(&app, legend),
        faces.mono_medium,
        "the still legend is mono"
    );

    // The foot strip holding the caption is a sibling after the art box in the
    // still column, so the caption is never drawn over the art.
    let foot = app.world().get::<ChildOf>(caption).unwrap().parent();
    let still = app.world().get::<ChildOf>(foot).unwrap().parent();
    let children = app.world().get::<Children>(still).unwrap();
    let art_index = children
        .iter()
        .position(|child| {
            app.world()
                .get::<crate::presentation::gallery::GalleryStillArtBox>(child)
                .is_some()
        })
        .expect("the still carries an art box");
    let foot_index = children
        .iter()
        .position(|child| child == foot)
        .expect("the foot strip is in the still column");
    assert!(art_index < foot_index, "the caption sits under the art");

    // The foot strip is a centred column: the caption and the legend are
    // centred under the image (`docs/title-and-menu.md` § Gallery `### Layout`
    // §6).
    let foot_node = app.world().get::<Node>(foot).unwrap();
    assert_eq!(foot_node.flex_direction, FlexDirection::Column);
    assert_eq!(foot_node.align_items, AlignItems::Center);

    // The caption is the dim tone and the legend the faint tone
    // (`docs/title-and-menu.md` § Gallery `### Layout` §6).
    assert_eq!(
        app.world().get::<TextColor>(caption).unwrap().0,
        Color::srgba(0.80, 0.80, 0.85, 0.62),
        "the still caption is the dim tone"
    );
    assert_eq!(
        app.world().get::<TextColor>(legend).unwrap().0,
        Color::srgba(0.80, 0.80, 0.85, 0.35),
        "the still legend is the faint tone"
    );
}

/// The still draws its art verbatim over the door's backdrop and scrim: the
/// surface and the art box carry no fill of their own, so a transparent sprite
/// stays transparent and no checkerboard is drawn
/// (`docs/title-and-menu.md` § Gallery `### Layout` §3/§6).
#[test]
fn the_still_surface_and_art_box_carry_no_fill() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);
    open_still(&mut app, 0);
    build_pause_tree(&mut app);

    let still = action_entity(&app, MenuAction::GalleryStillClose);
    assert_eq!(
        app.world().get::<BackgroundColor>(still).unwrap().0,
        Color::NONE,
        "the still surface lets the backdrop show through"
    );
    let box_entity = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<crate::presentation::gallery::GalleryStillArtBox>()
                .map(|_| entity.id())
        })
        .expect("the still art box draws");
    assert_eq!(
        app.world().get::<BackgroundColor>(box_entity).unwrap().0,
        Color::NONE,
        "the letterbox shows the backdrop, not a fill"
    );
    let art = app
        .world()
        .get::<Children>(box_entity)
        .unwrap()
        .iter()
        .find(|child| {
            app.world()
                .get::<crate::presentation::gallery::GalleryStillArt>(*child)
                .is_some()
        })
        .expect("the art box carries an image node");
    assert_eq!(
        app.world().get::<ImageNode>(art).unwrap().color,
        Color::WHITE,
        "the art is drawn verbatim, with no tint"
    );
}

/// `fit_gallery_still` contain-fits the image into the art box above the foot
/// strip: both axes fit, the aspect is preserved, and the free axis letterboxes
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §8, `### Layout` §3/§6).
#[test]
fn fit_gallery_still_contain_fits_the_image_above_the_foot_strip() {
    let mut app = App::new();
    app.init_resource::<Assets<Image>>();
    let box_size = Vec2::new(800.0, 700.0);
    let art_box = app
        .world_mut()
        .spawn((
            crate::presentation::gallery::GalleryStillArtBox,
            ComputedNode {
                size: box_size,
                ..default()
            },
            Outline::new(Val::Px(1.0), Val::Px(0.0), Color::WHITE),
        ))
        .id();
    let wide = add_test_image(&mut app, 1280, 720);
    let tall = add_test_image(&mut app, 256, 512);

    let wide_art = app
        .world_mut()
        .spawn((
            crate::presentation::gallery::GalleryStillArt { shown: None },
            ImageNode::new(wide),
            Node::default(),
            ChildOf(art_box),
        ))
        .id();
    let tall_art = app
        .world_mut()
        .spawn((
            crate::presentation::gallery::GalleryStillArt { shown: None },
            ImageNode::new(tall),
            Node::default(),
            ChildOf(art_box),
        ))
        .id();
    app.add_systems(Update, crate::presentation::gallery::fit_gallery_still);
    app.update();

    // A 1280x720 image in the 800x700 box: the width fits and the height
    // letterboxes to 800 * 720 / 1280 = 450.
    let wide_node = app.world().get::<Node>(wide_art).unwrap().clone();
    assert_eq!(wide_node.width, Val::Px(800.0));
    assert_eq!(wide_node.height, Val::Px(450.0));
    // A 256x512 sprite: the height fits and the width pillarboxes to 700 * 256
    // / 512 = 350.
    let tall_node = app.world().get::<Node>(tall_art).unwrap().clone();
    assert_eq!(tall_node.width, Val::Px(350.0));
    assert_eq!(tall_node.height, Val::Px(700.0));

    // Both axes fit and the aspect is preserved.
    for (node, natural) in [
        (wide_node, Vec2::new(1280.0, 720.0)),
        (tall_node, Vec2::new(256.0, 512.0)),
    ] {
        let (Val::Px(width), Val::Px(height)) = (node.width, node.height) else {
            panic!("the still art is sized in pixels");
        };
        assert!(width <= box_size.x && height <= box_size.y, "both axes fit");
        assert_eq!(
            width / height,
            natural.x / natural.y,
            "the aspect is preserved"
        );
    }
    assert_eq!(
        app.world().get::<Outline>(art_box).unwrap().color,
        Color::NONE,
        "once a texture decodes the placeholder frame clears"
    );
}

/// The transparent fallback is a resident 1x1 image, so it must not be treated
/// as decoded art: a failed or absent entry keeps the placeholder frame and a
/// zero-sized node instead of stretching the 1x1 to the box
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §6).
#[test]
fn fit_gallery_still_keeps_the_placeholder_for_the_transparent_fallback() {
    let mut app = App::new();
    app.init_resource::<Assets<Image>>();
    // The real app's `ImagePlugin` inserts the transparent handle as a decoded
    // asset; insert it here so the sentinel cannot pass as art.
    app.world_mut()
        .resource_mut::<Assets<Image>>()
        .insert(&bevy::image::TRANSPARENT_IMAGE_HANDLE, Image::transparent())
        .unwrap();
    let art_box = app
        .world_mut()
        .spawn((
            crate::presentation::gallery::GalleryStillArtBox,
            ComputedNode {
                size: Vec2::new(800.0, 700.0),
                ..default()
            },
            Outline::new(Val::Px(1.0), Val::Px(0.0), Color::WHITE),
        ))
        .id();
    let art = app
        .world_mut()
        .spawn((
            crate::presentation::gallery::GalleryStillArt { shown: Some(0) },
            ImageNode::new(bevy::image::TRANSPARENT_IMAGE_HANDLE),
            Node {
                width: Val::Px(0.0),
                height: Val::Px(0.0),
                ..default()
            },
            ChildOf(art_box),
        ))
        .id();
    app.add_systems(Update, crate::presentation::gallery::fit_gallery_still);
    app.update();

    assert_eq!(
        app.world().get::<Node>(art).unwrap().width,
        Val::Px(0.0),
        "the transparent fallback is not sized as art"
    );
    assert_ne!(
        app.world().get::<Outline>(art_box).unwrap().color,
        Color::NONE,
        "the placeholder frame stands for absent art"
    );
}

/// The still is modal: the overlay's chrome stands but is inert, so a Save/Load
/// tab or the rail Resume does nothing while the still is up — the mode, the
/// focus and the shown entry cannot drift apart
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §4/§5).
#[test]
fn the_overlay_chrome_is_inert_while_the_still_is_up() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);
    open_still(&mut app, 1);
    let before = menu(&app).focus;

    click(&mut app, MenuAction::SetMode(MenuMode::Load));
    assert!(menu(&app).gallery_still, "a Save/Load tab does not leave");
    assert_eq!(menu(&app).mode, MenuMode::Save, "the mode does not switch");
    assert_eq!(menu(&app).focus, before, "the shown entry does not move");

    click(&mut app, MenuAction::Resume);
    assert!(menu(&app).gallery_still, "Resume does not close the still");
    assert!(menu(&app).open, "Resume does not close the overlay");
    assert_eq!(
        menu(&app).focus,
        before,
        "the shown entry still does not move"
    );

    // The still's own click still closes it.
    click(&mut app, MenuAction::GalleryStillClose);
    assert!(!menu(&app).gallery_still, "the still's own click closes it");
}

/// `←`/`→` browse the entry order cyclically; the caption and the grid's focus
/// follow the step, so closing returns to the entry that was shown
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §4/§5).
#[test]
fn the_still_browses_cyclically_and_the_caption_and_grid_focus_follow() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    let count = app
        .world()
        .resource::<Runtime>()
        .runner
        .pack()
        .gallery_entries()
        .len();
    assert_eq!(count, 3, "the demo pack's three entries");
    open_overlay_gallery(&mut app);
    open_still(&mut app, 0);
    build_pause_tree(&mut app);
    sync_still(&mut app);
    let first = still_label(&app);

    // Left wraps back to the last entry; the caption follows.
    press_and_update(&mut app, KeyCode::ArrowLeft);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(count - 1));
    build_pause_tree(&mut app);
    sync_still(&mut app);
    let last = still_label(&app);
    assert_ne!(last, first);
    assert!(
        all_texts(&app).iter().any(|text| text == &last),
        "the caption follows the step: {last:?}"
    );

    // Right wraps forward to the first entry.
    press_and_update(&mut app, KeyCode::ArrowRight);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(0));

    // Right steps one entry; closing returns to that grid card.
    press_and_update(&mut app, KeyCode::ArrowRight);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(1));
    esc(&mut app);
    assert!(!menu(&app).gallery_still, "closing returns to the grid");
    assert_eq!(menu(&app).view, MenuView::Gallery);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::GalleryCard(1),
        "the grid's focus follows the step"
    );
}

/// Assert the still closed back to the grid on one entry.
fn assert_still_closed_on(app: &App, index: usize, what: &str) {
    assert!(!menu(app).gallery_still, "{what} closes the still");
    assert_eq!(
        menu(app).view,
        MenuView::Gallery,
        "{what} returns to the grid"
    );
    assert_eq!(
        menu(app).focus,
        MenuFocus::GalleryCard(index),
        "{what} keeps the entry that was shown"
    );
}

/// `↑`, `↓`, `Esc` and a click each close the still back to the grid, focused on
/// the entry it was showing (`docs/title-and-menu.md` § Gallery `### Behavior`
/// §5/§8).
#[test]
fn up_down_esc_and_click_each_close_the_still_on_that_entry() {
    let pack = temp_pack();

    let mut up = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut up);
    open_still(&mut up, 1);
    press_and_update(&mut up, KeyCode::ArrowUp);
    assert_still_closed_on(&up, 1, "Up");

    let mut down = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut down);
    open_still(&mut down, 1);
    press_and_update(&mut down, KeyCode::ArrowDown);
    assert_still_closed_on(&down, 1, "Down");

    let mut escape = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut escape);
    open_still(&mut escape, 1);
    esc(&mut escape);
    assert_still_closed_on(&escape, 1, "Esc");

    let mut clicked = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut clicked);
    open_still(&mut clicked, 1);
    click(&mut clicked, MenuAction::GalleryStillClose);
    assert_still_closed_on(&clicked, 1, "a click");
}

/// The still is a hold, not a second menu: no input other than `←`/`→`/`↑`/`↓`,
/// `Esc` and a click does anything (`docs/title-and-menu.md` § Gallery
/// `### Behavior` §5).
#[test]
fn no_other_input_moves_the_still() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);
    open_still(&mut app, 1);
    let before = menu(&app).focus;

    for key in [
        KeyCode::Tab,
        KeyCode::Digit1,
        KeyCode::Enter,
        KeyCode::Space,
    ] {
        press_and_update(&mut app, key);
        assert!(menu(&app).gallery_still, "{key:?} leaves the still up");
        assert_eq!(menu(&app).focus, before, "{key:?} moves nothing");
    }
    assert_eq!(
        menu(&app).mode,
        MenuMode::Save,
        "Tab does not flip the mode"
    );
    assert_eq!(menu(&app).view, MenuView::Gallery);
}

/// Stepping the still changes only the image and the caption: the door's
/// backdrop and scrim stand as entities behind it, and the still calls no
/// runner method and writes no slot, autosave or settings file
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §5/§6, `### Layout`
/// §6/§7).
#[test]
fn stepping_the_still_leaves_the_backdrop_scrim_and_session_unchanged() {
    let mut app = gallery_asset_app(&[]);
    open_title_gallery(&mut app);
    pump_until_watch_drains(&mut app);
    press_and_update(&mut app, KeyCode::Enter);
    assert!(menu(&app).gallery_still, "the still opened");
    // One frame so the presentation builds the still tree (this harness adds no
    // ordering between the input and the UI systems) before the backdrop and
    // scrim entities are captured.
    app.update();

    let backdrop = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<crate::presentation::title::TitleBackdropImage>()
                .map(|_| entity.id())
        })
        .expect("the declared backdrop draws");
    let scrim = gallery_scrim(&app).expect("the title door draws the scrim");
    let gradient = scrim_gradient(&app);
    let root = scrim_root(&app);
    let before = child_order(&app, root);
    let parked = parked_position(&app);

    // Step forward and back: the walk crosses both entries' art.
    press_and_update(&mut app, KeyCode::ArrowRight);
    press_and_update(&mut app, KeyCode::ArrowLeft);

    assert!(
        app.world()
            .get::<crate::presentation::title::TitleBackdropImage>(backdrop)
            .is_some(),
        "the same backdrop entity survives the step"
    );
    assert!(
        app.world()
            .get::<crate::presentation::gallery::GalleryScrim>(scrim)
            .is_some(),
        "the same scrim entity survives the step"
    );
    assert_eq!(child_order(&app, root), before, "the tree is not rebuilt");
    assert_eq!(scrim_gradient(&app), gradient, "the scrim is unchanged");
    assert_eq!(
        parked_position(&app),
        parked,
        "the still calls no runner method"
    );
    assert!(
        !autosave_path(&app).exists(),
        "the still writes no autosave"
    );
    assert!(
        !settings_path(&app).exists(),
        "the still never flushes settings"
    );
    assert!(
        !slot_path(&app, Slot::Manual(1)).exists(),
        "the still writes no slot file"
    );
}

/// The focused card's scroll-into-view fires on a real focus change only, so a
/// wheel scroll or an idle frame never yanks the view back
/// (`docs/title-and-menu.md` § Gallery `### Layout` §4).
#[derive(Resource, Default)]
struct GalleryScrolls(Vec<Entity>);

fn gallery_scrolls(app: &App) -> &[Entity] {
    app.world().resource::<GalleryScrolls>().0.as_slice()
}

#[test]
fn moving_the_gallery_focus_triggers_scroll_into_view() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);
    build_pause_tree(&mut app);
    app.init_resource::<GalleryScrolls>();
    app.add_observer(
        |scroll: On<bevy::ui_widgets::ScrollIntoView>, mut seen: ResMut<GalleryScrolls>| {
            seen.0.push(scroll.entity);
        },
    );
    app.add_systems(
        Update,
        crate::presentation::gallery::scroll_gallery_focus.after(RuntimeCoreSet),
    );

    // The initial focus reveals itself; an idle frame does not repeat.
    app.update();
    assert_eq!(
        gallery_scrolls(&app),
        [action_entity(&app, MenuAction::GalleryCard(0))]
    );
    app.update();
    assert_eq!(
        gallery_scrolls(&app).len(),
        1,
        "an idle frame triggers nothing"
    );

    // An arrow move reveals the new card.
    press_and_update(&mut app, KeyCode::ArrowRight);
    assert_eq!(
        gallery_scrolls(&app).len(),
        2,
        "the new focus reveals itself"
    );
    assert_eq!(
        gallery_scrolls(&app)[1],
        action_entity(&app, MenuAction::GalleryCard(1))
    );
}

/// In a 19-card grid every arrow move asks the body to reveal the card it lands
/// on, so the focused card is never scrolled off-screen
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §4, `### Layout` §4/§8).
#[test]
fn every_arrow_move_in_a_scrolling_grid_reveals_the_focused_card() {
    let extras: Vec<String> = (1..=16).map(|index| format!("g{index:02}")).collect();
    let extras: Vec<&str> = extras.iter().map(String::as_str).collect();
    let dir = temp_pack_with_backgrounds(&extras);
    let mut app = app_for_pack(dir.path().join("pack"));
    open_overlay_gallery(&mut app);
    build_pause_tree(&mut app);
    app.init_resource::<GalleryScrolls>();
    app.add_observer(
        |scroll: On<bevy::ui_widgets::ScrollIntoView>, mut seen: ResMut<GalleryScrolls>| {
            seen.0.push(scroll.entity);
        },
    );
    app.add_systems(
        Update,
        crate::presentation::gallery::scroll_gallery_focus.after(RuntimeCoreSet),
    );

    let count = app
        .world()
        .resource::<Runtime>()
        .runner
        .pack()
        .gallery_entries()
        .len();
    assert_eq!(count, 19);
    app.update(); // the initial focus reveals itself

    for key in [
        KeyCode::ArrowRight,
        KeyCode::ArrowDown,
        KeyCode::ArrowLeft,
        KeyCode::ArrowUp,
    ] {
        let before = gallery_scrolls(&app).len();
        press_and_update(&mut app, key);
        let scrolls = gallery_scrolls(&app);
        assert_eq!(
            scrolls.len(),
            before + 1,
            "{key:?} reveals the newly focused card"
        );
        let focused = match menu(&app).focus {
            MenuFocus::GalleryCard(index) => action_entity(&app, MenuAction::GalleryCard(index)),
            other => panic!("focus left the grid: {other:?}"),
        };
        assert_eq!(
            scrolls[before], focused,
            "{key:?} reveals the card the walk landed on"
        );
    }
}

/// The overlay door's grid frame at the reference window, measured from the
/// rendered acceptance screenshots and the documented grid
/// (`docs/title-and-menu.md` § Gallery `### Layout` §4): a 992x568 body over a
/// three-across grid of 317x260 cards with 20px gaps.
const RIG_VIEWPORT: Vec2 = Vec2::new(992.0, 568.0);
const RIG_CARD: Vec2 = Vec2::new(317.0, 260.0);
const RIG_GAP: f32 = 20.0;
const RIG_COLUMNS: usize = 3;
/// The 19-card grid's content height: seven card rows with a gap between each.
const RIG_CONTENT_HEIGHT: f32 = 7.0 * RIG_CARD.y + 6.0 * RIG_GAP;
/// The 19th card's (index 18) reveal: its bottom edge against the viewport's,
/// the minimal scroll `ScrollIntoView` asks for.
const RIG_CARD_18_REVEAL: f32 = RIG_CONTENT_HEIGHT - RIG_VIEWPORT.y;

/// Stand in for the UI layout pass a headless core app does not run: give the
/// built grid the real frame, so a `ScrollIntoView` on the spawned tree really
/// moves the body's `ScrollPosition` instead of computing against zero-sized
/// nodes.
fn rig_gallery_scroll(
    mut commands: Commands,
    mut bodies: Query<(Entity, &mut ComputedNode), With<crate::presentation::gallery::GalleryBody>>,
    mut cards: Query<
        (Entity, &MenuAction, &mut ComputedNode),
        Without<crate::presentation::gallery::GalleryBody>,
    >,
) {
    for (entity, mut node) in &mut bodies {
        node.size = RIG_VIEWPORT;
        node.content_size = Vec2::new(RIG_VIEWPORT.x, RIG_CONTENT_HEIGHT);
        commands
            .entity(entity)
            .insert(UiGlobalTransform::from_translation(RIG_VIEWPORT / 2.0));
    }
    for (entity, action, mut node) in &mut cards {
        let MenuAction::GalleryCard(index) = *action else {
            continue;
        };
        node.size = RIG_CARD;
        let column = (index % RIG_COLUMNS) as f32;
        let row = (index / RIG_COLUMNS) as f32;
        commands
            .entity(entity)
            .insert(UiGlobalTransform::from_translation(
                Vec2::new(column, row) * (RIG_CARD + Vec2::splat(RIG_GAP)) + RIG_CARD / 2.0,
            ));
    }
}

/// Closing the still returns to the grid focused on the entry the still was
/// showing, with that card revealed: the still's structural rebuild respawns
/// the body at the top, so the unchanged focus re-issues the card's reveal. Any
/// other rebuild that keeps the gallery up re-reveals it the same way, and an
/// idle frame never yanks the view back (`docs/title-and-menu.md` § Gallery
/// `### Behavior` §4/§5, `### Layout` §4).
#[test]
fn closing_the_still_reveals_the_focused_card_again() {
    let extras: Vec<String> = (1..=16).map(|index| format!("g{index:02}")).collect();
    let extras: Vec<&str> = extras.iter().map(String::as_str).collect();
    let dir = temp_pack_with_backgrounds(&extras);
    let mut app = app_for_pack(dir.path().join("pack"));
    open_overlay_gallery(&mut app);
    build_pause_tree(&mut app);
    // The real scroll plugin and a layout stand-in, so the reveal is asserted
    // as the body's own `ScrollPosition`, not as a bare trigger.
    app.add_plugins(bevy::ui_widgets::ScrollAreaPlugin);
    app.add_systems(
        Update,
        (
            rig_gallery_scroll,
            crate::presentation::gallery::scroll_gallery_focus,
        )
            .chain()
            .after(RuntimeCoreSet),
    );

    // Walk down the grid to the 19th card: its reveal scrolls the body.
    for _ in 0..6 {
        press_and_update(&mut app, KeyCode::ArrowDown);
    }
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(18));
    let walked = gallery_body_scroll(&app);
    assert_eq!(
        walked,
        Vec2::new(0.0, RIG_CARD_18_REVEAL),
        "the walk reveals the 19th card at the viewport's foot"
    );

    // `Enter` opens the still: the grid gives way to the one entry.
    press_and_update(&mut app, KeyCode::Enter);
    build_pause_tree(&mut app);
    assert!(menu(&app).gallery_still, "Enter opens the still");
    assert!(
        has_action(&app, MenuAction::GalleryStillClose),
        "the still surface is up"
    );

    // `↓` closes it on the same card, and the rebuilt grid reveals it again.
    press_and_update(&mut app, KeyCode::ArrowDown);
    build_pause_tree(&mut app);
    assert!(!menu(&app).gallery_still, "↓ closes the still");
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(18));
    app.update();
    assert_eq!(
        gallery_body_scroll(&app),
        walked,
        "the returned grid reveals the still's entry"
    );

    // An idle frame leaves the scroll alone.
    app.update();
    assert_eq!(
        gallery_body_scroll(&app),
        walked,
        "an idle frame holds the view"
    );

    // The rebuild family is covered, not the still alone: a signature change
    // that keeps the card focus respawns the body the same way. It is written
    // directly because every reachable tab switch moves the focus to the tab.
    app.world_mut().resource_mut::<MenuState>().mode = MenuMode::Load;
    build_pause_tree(&mut app);
    app.update();
    assert_eq!(
        gallery_body_scroll(&app),
        walked,
        "a structural rebuild re-reveals the same card"
    );
}

/// The grid body really is a Bevy scroll area: a `ScrollIntoView` on a card
/// moves the body's `ScrollPosition` by the minimal amount that reveals it, so
/// the walk's focus stays visible (`docs/title-and-menu.md` § Gallery
/// `### Layout` §4).
#[test]
fn the_gallery_body_scrolls_the_focused_card_into_view() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(bevy::ui_widgets::ScrollAreaPlugin);
    let body = app
        .world_mut()
        .spawn((
            crate::presentation::gallery::GalleryBody,
            ScrollArea,
            Node {
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ComputedNode {
                size: Vec2::new(300.0, 100.0),
                content_size: Vec2::new(300.0, 400.0),
                ..default()
            },
            UiGlobalTransform::from_translation(Vec2::new(150.0, 50.0)),
        ))
        .id();
    // A card one-and-a-bit viewports down: its bottom sits at y 300.
    let card = app
        .world_mut()
        .spawn((
            Node::default(),
            ComputedNode {
                size: Vec2::new(300.0, 100.0),
                ..default()
            },
            UiGlobalTransform::from_translation(Vec2::new(150.0, 250.0)),
            ChildOf(body),
        ))
        .id();

    app.world_mut()
        .trigger(bevy::ui_widgets::ScrollIntoView { entity: card });

    let scroll = app.world().get::<ScrollPosition>(body).unwrap().0;
    assert_eq!(
        scroll.y, 200.0,
        "the body scrolls just enough to pull the card's bottom into view"
    );
}

/// `Tab` still reaches the overlay's narrowed top bar from a gallery card, and
/// an arrow returns to the grid; entering the top bar only changes mode
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §1, `### Layout` §5).
#[test]
fn tab_leaves_the_gallery_grid_and_arrows_return_to_it() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(0));

    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).focus, MenuFocus::SaveTab, "Tab leaves the grid");
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::GalleryCard(0),
        "an arrow returns to the grid"
    );

    press_and_update(&mut app, KeyCode::Tab);
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(
        menu(&app).view,
        MenuView::Gallery,
        "Save stays in the gallery"
    );
    assert_eq!(menu(&app).focus, MenuFocus::SaveTab);
    assert_eq!(menu(&app).mode, MenuMode::Save);
    press_and_update(&mut app, KeyCode::Tab);
    press_and_update(&mut app, KeyCode::Space);
    assert_eq!(menu(&app).mode, MenuMode::Load, "Space picks the Load tab");
    assert_eq!(menu(&app).focus, MenuFocus::LoadTab);
}

/// A title-door gallery app with the asset rig attached, so cards load through
/// the real watcher and the play path's staging shares that watcher too: a
/// present file decodes and a missing one fails. `remove` names pack-relative
/// asset paths deleted before the pack loads. The still sync is attached as
/// well; `fit_gallery_still` is deliberately not, because this minimal app has
/// no UI layout pass and its contain-fit maths is unit-tested directly.
fn gallery_asset_app(remove: &[&str]) -> App {
    let dir = temp_pack();
    let pack_root = dir.path().join("pack");
    for rel in remove {
        std::fs::remove_file(pack_root.join("assets").join(rel)).expect("remove the asset");
    }
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(AssetPlugin {
            file_path: pack_root.join("assets").to_string_lossy().into_owned(),
            ..default()
        })
        .init_asset::<Image>()
        .init_asset_loader::<PathImageLoader>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<AssetWatch>()
        .init_resource::<AppliedVisuals>()
        .init_resource::<crate::presentation::MenuBuilt>()
        .insert_resource(RuntimeConfig::new(pack_root, None).with_dev_mode(true))
        .add_plugins(RuntimeCorePlugin)
        .add_systems(
            Update,
            (
                crate::presentation::play::sync_staging_visuals,
                crate::presentation::sync_menu_ui,
                crate::presentation::gallery::sync_gallery_art,
                crate::presentation::gallery::sync_gallery_still,
                report_silent_degradations,
            )
                .chain(),
        );
    app.world_mut()
        .spawn((settings::window_config(), PrimaryWindow));
    attach_text_faces(&mut app);
    force_single_threaded(&mut app);
    app.insert_resource(TempPack(dir));
    app.update(); // Startup: load the pack, then show the title
    app
}

/// A missing card's art keeps the placeholder frame with its label, warns once
/// through rows 3/4, and raises no Notice (`docs/title-and-menu.md` § Gallery
/// `### Behavior` §6, `### Layout` §2).
#[test]
fn a_missing_art_card_draws_the_placeholder_frame_with_its_label() {
    captures::install();
    let mut app = gallery_asset_app(&["sprites/ace/happy.png"]);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleGallery;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).view, MenuView::Gallery);
    captures::take();
    pump_until_watch_drains(&mut app);

    // Every declared card still carries its label, the failed one included.
    let texts = all_texts(&app);
    for label in ["school_gate", "ace \u{b7} happy", "ace \u{b7} neutral"] {
        assert!(
            texts.iter().any(|text| text == label),
            "the {label:?} card label renders"
        );
    }

    // The decoded cards drop the placeholder frame; the failed card keeps it
    // and sizes no image, with no glyph or marker inside the art box.
    let gate = gallery_art_box(&app, entry_index(&app, "school_gate"));
    assert_eq!(
        app.world().get::<Outline>(gate).unwrap().color,
        Color::NONE,
        "a decoded card drops the placeholder frame"
    );
    let happy = gallery_art_box(&app, entry_index(&app, "ace \u{b7} happy"));
    assert_ne!(
        app.world().get::<Outline>(happy).unwrap().color,
        Color::NONE,
        "a failed card keeps the placeholder frame"
    );
    assert!(
        app.world()
            .get::<Children>(happy)
            .unwrap()
            .iter()
            .all(|child| app
                .world()
                .get::<crate::presentation::gallery::GalleryArt>(child)
                .is_some()),
        "the placeholder art box holds no glyph or marker"
    );

    // A decoded card draws its texture verbatim — white, untinted — over the
    // card's own fill, so a sprite's transparency shows the fill and there is
    // no checkerboard or veil.
    let gate_art = app
        .world()
        .get::<Children>(gate)
        .unwrap()
        .iter()
        .find(|child| {
            app.world()
                .get::<crate::presentation::gallery::GalleryArt>(*child)
                .is_some()
        })
        .expect("the decoded card carries an image node");
    assert_eq!(
        app.world().get::<ImageNode>(gate_art).unwrap().color,
        Color::WHITE,
        "the art is drawn verbatim"
    );
    let card = action_entity(
        &app,
        MenuAction::GalleryCard(entry_index(&app, "school_gate")),
    );
    assert_eq!(
        app.world().get::<BackgroundColor>(gate).unwrap().0,
        app.world().get::<BackgroundColor>(card).unwrap().0,
        "the letterbox is the card's own fill"
    );

    // Exactly one warn for the one absent path, under row 4, with no Notice.
    let records = failure_lines();
    assert_eq!(records.len(), 1, "one warn per failed path: {records:?}");
    assert_eq!(records[0].0, log::Level::Warn);
    assert!(
        records[0].2.starts_with("couldn't load a sprite")
            && records[0].2.contains("sprites/ace/happy.png"),
        "{:?}",
        records[0].2
    );
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "failed card art never notices"
    );
}

/// The still's one art node.
fn still_art(app: &App) -> Entity {
    app.world()
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<crate::presentation::gallery::GalleryStillArt>()
                .map(|_| entity.id())
        })
        .expect("the still carries an art node")
}

/// Whether a card's art box carries an image node. A card whose path the
/// watcher refused to attempt (already reported) has none; a present path has
/// one.
fn art_box_has_image(app: &App, art_box: Entity) -> bool {
    app.world()
        .get::<Children>(art_box)
        .map(|children| {
            children.iter().any(|child| {
                app.world()
                    .get::<crate::presentation::gallery::GalleryArt>(child)
                    .is_some()
            })
        })
        .unwrap_or(false)
}

/// Activating a failed card opens the still on the same placeholder frame with
/// the entry's label as its caption, and the whole open/walk/close path logs
/// no second line for a path the grid already reported
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §5/§6, `### Layout` §2).
#[test]
fn a_failed_cards_still_keeps_the_placeholder_and_the_label() {
    captures::install();
    let mut app = gallery_asset_app(&["sprites/ace/happy.png"]);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleGallery;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).view, MenuView::Gallery);
    captures::take();
    pump_until_watch_drains(&mut app);

    // The grid reported the missing sprite once; the still reuses that path and
    // must not add a line of its own.
    let grid_warns = failure_lines();
    assert_eq!(grid_warns.len(), 1, "one warn for the path: {grid_warns:?}");

    let failed = entry_index(&app, "ace \u{b7} happy");
    open_still(&mut app, failed);
    pump_until_watch_drains(&mut app);
    app.update();

    // The caption is the card's label verbatim, so the placeholder still draws
    // the entry's identity, and the art node holds the transparent fallback
    // that `fit_gallery_still` keeps unsized, so the frame stands.
    assert_eq!(
        still_label(&app),
        "ace \u{b7} happy",
        "the still shows the failed card's label"
    );
    assert!(
        all_texts(&app)
            .iter()
            .any(|text| text == "ace \u{b7} happy"),
        "the placeholder still draws the entry's label"
    );
    assert_eq!(
        app.world().get::<ImageNode>(still_art(&app)).unwrap().image,
        bevy::image::TRANSPARENT_IMAGE_HANDLE,
        "absent art leaves the transparent fallback, not a stretched sentinel"
    );

    // No line was added by the still, and nothing noticed.
    assert!(
        failure_lines().is_empty(),
        "the still must not warn again for a reported path"
    );
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "a placeholder still never notices"
    );

    // A decoded entry still loads into the still: stepping to its neighbour
    // replaces the transparent fallback with the real image, so the fallback
    // above is a placeholder and not a rig where nothing ever loads.
    press_and_update(&mut app, KeyCode::ArrowLeft);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::GalleryCard(entry_index(&app, "school_gate")),
        "the still stepped to the previous entry"
    );
    pump_until_watch_drains(&mut app);
    app.update();
    assert_eq!(
        still_label(&app),
        "school_gate",
        "the caption follows the step"
    );
    assert_ne!(
        app.world().get::<ImageNode>(still_art(&app)).unwrap().image,
        bevy::image::TRANSPARENT_IMAGE_HANDLE,
        "a decoded entry loads its real image into the still"
    );
}

/// The shared image load asks for render-world-only usage. This harness has no
/// render world, so nothing extracts or unloads the image here: the witness is
/// the usage the load requested, not a released texture.
#[test]
fn a_pack_image_load_is_render_world_only() {
    let dir = temp_pack();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(AssetPlugin {
            file_path: dir
                .path()
                .join("pack/assets")
                .to_string_lossy()
                .into_owned(),
            ..default()
        })
        // The real decoder: `ImageLoader` comes from `bevy_image::ImagePlugin`
        // in `DefaultPlugins`, which `MinimalPlugins` omits — not from
        // `RenderPlugin`, which the headless composition disables. A
        // test-double loader builds its own `Image` and so could not show what
        // the shared image load asked the loader for.
        .init_asset::<Image>()
        .register_asset_loader(bevy::image::ImageLoader::new(
            bevy::image::CompressedImageFormats::NONE,
        ))
        .init_resource::<AssetWatch>();

    let server = app.world().resource::<AssetServer>().clone();
    let handle = app
        .world_mut()
        .resource_mut::<AssetWatch>()
        .watch_image(
            &server,
            crate::failure::FailureRow::BackgroundAsset,
            "backgrounds/school_gate.png",
        )
        .expect("the first load of a path is watched");

    // No render world and no settle system in this app, so
    // `pump_until_watch_drains` has nothing to drain: this waits on the load
    // state directly, keeping that helper's 10 s deadline and periodic 1 ms
    // sleep so a busy task pool cannot make the loop spin.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut spins = 0u32;
    while matches!(
        server.load_state(handle.id()),
        LoadState::NotLoaded | LoadState::Loading
    ) {
        assert!(
            Instant::now() < deadline,
            "the background load never settled"
        );
        app.update();
        spins += 1;
        if spins.is_multiple_of(64) {
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    let images = app.world().resource::<Assets<Image>>();
    let image = images.get(&handle).expect("the demo background decodes");
    assert_eq!(
        image.asset_usage,
        RenderAssetUsages::RENDER_WORLD,
        "the shared image load asks for render-world-only usage"
    );
}

/// A path the play path already reported is not reported a second time by the
/// gallery: the card draws its placeholder silently and the log stays at one
/// line (`docs/title-and-menu.md` § Gallery `### Behavior` §6/§8).
#[test]
fn a_path_the_play_path_reported_is_not_reported_again_by_the_gallery() {
    captures::install();
    let mut app = gallery_asset_app(&["backgrounds/school_gate.png"]);
    captures::take();

    // The play path stages the entry scene's background and reports row 3 once.
    enter_play(&mut app);
    app.update();
    pump_until_watch_drains(&mut app);
    let reported = failure_lines();
    let background_warns: Vec<_> = reported
        .iter()
        .filter(|(_, _, line)| line.starts_with("couldn't load a background"))
        .collect();
    assert_eq!(
        background_warns.len(),
        1,
        "the play path reports the missing background once: {reported:?}"
    );
    assert!(
        background_warns[0]
            .2
            .contains("backgrounds/school_gate.png"),
        "{:?}",
        background_warns[0].2
    );

    // The gallery's `school_gate` card loads the same path under the same row;
    // the one-per-path rule means no second line.
    open_overlay_gallery(&mut app);
    pump_until_watch_drains(&mut app);
    assert!(
        failure_lines().is_empty(),
        "a path already reported by the play path logs no second line"
    );

    // The card still draws its placeholder frame with its label.
    let gate = gallery_art_box(&app, entry_index(&app, "school_gate"));
    assert_ne!(
        app.world().get::<Outline>(gate).unwrap().color,
        Color::NONE,
        "the already-reported card keeps the placeholder frame"
    );
    // The card consulted the watcher and the one-per-path rule refused the
    // load: no image node is spawned at all, so the frame stands and no second
    // warn is queued. A present sibling still spawns its art node, proving the
    // build really ran `watch_image` for these cards.
    assert!(
        !art_box_has_image(&app, gate),
        "an already-reported path spawns no art node"
    );
    assert!(
        art_box_has_image(
            &app,
            gallery_art_box(&app, entry_index(&app, "ace \u{b7} neutral"))
        ),
        "a present path still spawns its art node"
    );
    assert!(
        all_texts(&app).iter().any(|text| text == "school_gate"),
        "the placeholder card still carries its label"
    );
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "a re-reported path never notices"
    );
}

/// A 19-entry pack scrolls its body, and every arrow move keeps focus on a real
/// card (`docs/title-and-menu.md` § Gallery `### Layout` §4, `### Behavior`
/// §4).
#[test]
fn a_nineteen_entry_grid_scrolls_with_the_focus_in_range() {
    let extras: Vec<String> = (1..=16).map(|index| format!("g{index:02}")).collect();
    let extras: Vec<&str> = extras.iter().map(String::as_str).collect();
    let dir = temp_pack_with_backgrounds(&extras);
    let mut app = app_for_pack(dir.path().join("pack"));
    open_overlay_gallery(&mut app);
    build_pause_tree(&mut app);

    let count = app
        .world()
        .resource::<Runtime>()
        .runner
        .pack()
        .gallery_entries()
        .len();
    assert_eq!(count, 19);
    for index in 0..count {
        assert!(has_action(&app, MenuAction::GalleryCard(index)));
    }
    assert_eq!(
        app.world()
            .get::<Node>(gallery_body(&app))
            .unwrap()
            .overflow,
        Overflow::scroll_y(),
        "the body scrolls"
    );

    // The walk stays on a real card and reaches both ends.
    for _ in 0..(count + 4) {
        press_and_update(&mut app, KeyCode::ArrowRight);
        assert_in_grid(&app, count);
    }
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(count - 1));
    for _ in 0..(count + 4) {
        press_and_update(&mut app, KeyCode::ArrowLeft);
        assert_in_grid(&app, count);
    }
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(0));
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::GalleryCard(0);
    for _ in 0..count {
        press_and_update(&mut app, KeyCode::ArrowDown);
        assert_in_grid(&app, count);
    }
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(count - 1));
    for _ in 0..count {
        press_and_update(&mut app, KeyCode::ArrowUp);
        assert_in_grid(&app, count);
    }
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(0));
}

fn assert_in_grid(app: &App, count: usize) {
    match menu(app).focus {
        MenuFocus::GalleryCard(index) => {
            assert!(index < count, "focus {index} left the {count}-card grid")
        }
        other => panic!("focus left the grid: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// The gallery's door backdrop and the title-door scrim
// ---------------------------------------------------------------------------

/// The built gallery's title-door scrim node, if the title door drew one.
fn gallery_scrim(app: &App) -> Option<Entity> {
    app.world().iter_entities().find_map(|entity| {
        entity
            .get::<crate::presentation::gallery::GalleryScrim>()
            .map(|_| entity.id())
    })
}

/// The one radial gradient `entity` draws, so its stops or centre can be
/// pinned.
fn single_radial(app: &App, entity: Entity) -> RadialGradient {
    match app.world().get::<BackgroundGradient>(entity) {
        Some(BackgroundGradient(gradients)) => match gradients.as_slice() {
            [Gradient::Radial(radial)] => radial.clone(),
            other => panic!("one radial gradient expected, got {other:?}"),
        },
        None => panic!("the entity draws a gradient"),
    }
}

/// The scrim's one radial gradient, so its stops can be pinned.
fn scrim_gradient(app: &App) -> RadialGradient {
    let scrim = gallery_scrim(app).expect("the title door draws the scrim");
    single_radial(app, scrim)
}

/// The parent of the title-door scrim — the gallery's root column, whose paint
/// order and gradient underlay the sibling tests read.
fn scrim_root(app: &App) -> Entity {
    let scrim = gallery_scrim(app).expect("the title door draws the scrim");
    app.world().get::<ChildOf>(scrim).unwrap().parent()
}

/// A node's children in paint order.
fn child_order(app: &App, parent: Entity) -> Vec<Entity> {
    app.world()
        .get::<Children>(parent)
        .map(|children| children.iter().collect())
        .unwrap_or_default()
}

/// Open the gallery from the title's `Gallery` row: the title door.
fn open_title_gallery(app: &mut App) {
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleGallery;
    press_and_update(app, KeyCode::Enter);
    assert_eq!(menu(app).view, MenuView::Gallery);
    assert_eq!(menu(app).gallery_origin, menu::MenuOrigin::Title);
}

/// From the title's door the gallery stands on the pack's backdrop and draws
/// the gallery-local radial scrim between that backdrop and the surface, so it
/// darkens the backdrop only and never a card's art
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §6, `### Layout` §4/§7).
#[test]
fn the_title_gallery_draws_the_backdrop_under_the_scrim_under_the_surface() {
    let mut app = gallery_asset_app(&[]);
    open_title_gallery(&mut app);
    pump_until_watch_drains(&mut app);

    // The declared `school_gate` backdrop draws, unmodified, over the neutral
    // gradient underlay.
    let (image, _) = title_backdrop_node(&app).expect("the declared backdrop draws");
    assert_eq!(image.color, Color::WHITE, "the art is drawn verbatim");

    // The scrim is the prototype's radial: 0.34 at the middle, 0.74 at the
    // edges, centred on the window.
    let radial = scrim_gradient(&app);
    assert_eq!(radial.position, UiPosition::CENTER, "the scrim is centred");
    // `resolve` yields node-centre-relative coordinates, so the middle is the
    // origin; a `CENTER.at_percent(50, 50)` offset would land on the
    // bottom-right corner instead.
    assert_eq!(
        radial
            .position
            .resolve(1.0, Vec2::new(1280.0, 720.0), Vec2::new(1280.0, 720.0)),
        Vec2::ZERO,
        "the scrim's centre is the middle of the window"
    );
    assert_eq!(radial.shape, RadialGradientShape::FarthestCorner);
    assert_eq!(radial.stops.len(), 2, "two stops");
    assert_eq!(radial.stops[0].point, Val::Percent(0.0));
    assert_eq!(radial.stops[1].point, Val::Percent(100.0));
    assert_eq!(
        radial.stops[0].color,
        Color::srgba(5.0 / 255.0, 7.0 / 255.0, 10.0 / 255.0, 0.34),
        "the middle is the spec's 0.34 tone"
    );
    assert_eq!(
        radial.stops[1].color,
        Color::srgba(5.0 / 255.0, 7.0 / 255.0, 10.0 / 255.0, 0.74),
        "the edges are the spec's 0.74 tone"
    );

    // The scrim is full-window, and its paint order is backdrop → scrim →
    // surface, so no card's art is ever covered.
    let scrim = gallery_scrim(&app).unwrap();
    let node = app.world().get::<Node>(scrim).unwrap();
    assert_eq!(node.position_type, PositionType::Absolute);
    assert_eq!(node.width, Val::Percent(100.0));
    assert_eq!(node.height, Val::Percent(100.0));

    let root = scrim_root(&app);
    let siblings = child_order(&app, root);
    let scrim_index = siblings.iter().position(|entity| *entity == scrim).unwrap();
    let backdrop_index = siblings
        .iter()
        .position(|entity| {
            app.world()
                .get::<crate::presentation::title::TitleBackdropImage>(*entity)
                .is_some()
        })
        .expect("the backdrop is a sibling of the scrim");
    let surface = app
        .world()
        .get::<ChildOf>(gallery_body(&app))
        .unwrap()
        .parent();
    let surface_index = siblings
        .iter()
        .position(|entity| *entity == surface)
        .unwrap();
    assert!(
        backdrop_index < scrim_index && scrim_index < surface_index,
        "the scrim paints between the backdrop and the surface"
    );
    assert!(
        app.world().get::<BackgroundGradient>(root).is_some(),
        "the neutral gradient underlay stays beneath the art"
    );
}

/// A pack that declares no `title_backdrop` draws the neutral gradient behind
/// the title-door gallery, with the scrim still over it
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §6).
#[test]
fn a_title_gallery_without_a_backdrop_keeps_the_gradient_and_the_scrim() {
    let manifest = "(\n    name: \"unmei demo\",\n    entry: \"first_meeting\",\n)\n";
    let mut app = title_asset_app(Some(manifest));
    pump_until_watch_drains(&mut app);
    {
        let mut state = app.world_mut().resource_mut::<MenuState>();
        state.view = MenuView::Gallery;
        state.gallery_origin = menu::MenuOrigin::Title;
    }
    pump_until_watch_drains(&mut app);

    assert_eq!(menu(&app).view, MenuView::Gallery);
    assert!(
        title_backdrop_node(&app).is_none(),
        "no backdrop image node is drawn"
    );
    let root = scrim_root(&app);
    assert!(
        app.world().get::<BackgroundGradient>(root).is_some(),
        "the gradient is the whole backdrop"
    );
}

/// From the overlay's door the gallery keeps the overlay's dimmed freeze of the
/// play frame and draws no scrim and no gradient
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §6, `### Layout` §7).
#[test]
fn the_overlay_gallery_draws_no_scrim() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);
    build_pause_tree(&mut app);

    assert!(
        gallery_scrim(&app).is_none(),
        "the overlay's door adds no scrim"
    );
    // The overlay root keeps its dim over the frozen play frame and draws no
    // gradient of its own.
    let root = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<crate::presentation::MenuRoot>()
                .map(|_| entity.id())
        })
        .expect("the overlay root draws");
    assert_eq!(
        app.world().get::<BackgroundColor>(root).unwrap().0,
        Color::srgba(0.03, 0.035, 0.05, 0.86),
        "the overlay keeps its dim over the frozen play frame"
    );
    assert!(
        app.world().get::<BackgroundGradient>(root).is_none(),
        "the overlay's door draws no gradient"
    );
}

/// A declared but missing title backdrop degrades silently behind the
/// title-door gallery: the gradient and the scrim stand, each load of the path
/// logs once as a warning and no Notice appears
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §6).
#[test]
fn a_missing_title_backdrop_behind_the_gallery_degrades_silently() {
    captures::install();
    let manifest = "(\n    name: \"unmei demo\",\n    entry: \"first_meeting\",\n    title_backdrop: \"missing_art\",\n)\n";
    let mut app = title_asset_app(Some(manifest));
    force_single_threaded(&mut app);
    captures::take();
    pump_until_watch_drains(&mut app);
    {
        let mut state = app.world_mut().resource_mut::<MenuState>();
        state.view = MenuView::Gallery;
        state.gallery_origin = menu::MenuOrigin::Title;
    }
    pump_until_watch_drains(&mut app);

    assert_eq!(menu(&app).view, MenuView::Gallery);
    assert!(
        title_backdrop_node(&app).is_none(),
        "a failed backdrop draws nothing"
    );
    let root = scrim_root(&app);
    assert!(
        app.world().get::<BackgroundGradient>(root).is_some(),
        "the gradient stands behind the gallery"
    );

    // Row 25 is the title backdrop's one row, and the gallery reuses the
    // title's watch, so opening it adds no second title-backdrop line. The
    // gallery's "row 3" wording is deliberately not taken for the backdrop: row 3
    // is the play path's `bg` row (`couldn't load a background`), while the
    // `title_backdrop` asset's documented row is 25. The one row-3 line here
    // is the gallery *card* for the same id — a distinct load with its own
    // funnel, not the backdrop's.
    let records = failure_lines();
    assert!(
        records
            .iter()
            .all(|(level, _, _)| *level == log::Level::Warn),
        "every gallery degradation is silent: {records:?}"
    );
    assert_eq!(records.len(), 2, "one line per load: {records:?}");
    let count_of = |prefix: &str| {
        records
            .iter()
            .filter(|(_, _, line)| line.starts_with(prefix))
            .count()
    };
    assert_eq!(
        count_of("couldn't load the title backdrop"),
        1,
        "the backdrop logs exactly once: {records:?}"
    );
    assert_eq!(
        count_of("couldn't load a background"),
        1,
        "the card logs exactly once: {records:?}"
    );
    let backdrop_line = records
        .iter()
        .find(|(_, _, line)| line.starts_with("couldn't load the title backdrop"))
        .expect("the backdrop logs");
    assert!(
        backdrop_line.2.contains("backgrounds/missing_art.png"),
        "the line names the path: {:?}",
        backdrop_line.2
    );
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "a silent degradation never notices"
    );
}

/// Opening and walking the title-door gallery leaves the backdrop and its scrim
/// exactly as they were — the surface is not rebuilt behind the walk — and
/// backing out to the title keeps the same backdrop
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §6, `### Layout` §6).
#[test]
fn walking_the_title_gallery_leaves_the_backdrop_and_scrim_unchanged() {
    let mut app = gallery_asset_app(&[]);
    open_title_gallery(&mut app);
    pump_until_watch_drains(&mut app);

    let backdrop = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<crate::presentation::title::TitleBackdropImage>()
                .map(|_| entity.id())
        })
        .expect("the declared backdrop draws");
    let scrim = gallery_scrim(&app).unwrap();
    let gradient = scrim_gradient(&app);
    let root = scrim_root(&app);
    let before = child_order(&app, root);

    press_and_update(&mut app, KeyCode::ArrowRight);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::GalleryCard(1),
        "the walk moved"
    );

    assert!(
        app.world()
            .get::<crate::presentation::title::TitleBackdropImage>(backdrop)
            .is_some(),
        "the same backdrop entity survives the walk"
    );
    assert!(
        app.world()
            .get::<crate::presentation::gallery::GalleryScrim>(scrim)
            .is_some(),
        "the same scrim entity survives the walk"
    );
    assert_eq!(child_order(&app, root), before, "the tree is not rebuilt");
    assert_eq!(scrim_gradient(&app), gradient, "the scrim is unchanged");

    // Backing out to the title leaves the title's backdrop standing.
    esc(&mut app);
    assert_eq!(menu(&app).view, MenuView::Title);
    pump_until_watch_drains(&mut app);
    assert!(
        title_backdrop_node(&app).is_some(),
        "the title's backdrop is unchanged by the gallery round trip"
    );
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

/// The running app's settings path: the pack directory's sibling.
fn settings_path(app: &App) -> PathBuf {
    let runtime = app.world().resource::<Runtime>();
    settings::settings_path_for(&runtime.pack_root)
}

/// The temporary sibling the atomic settings write uses.
fn settings_tmp_path(app: &App) -> PathBuf {
    settings_path(app).with_extension("ron.tmp")
}

/// Open the overlay and click through to the Settings view.
fn open_settings(app: &mut App) {
    while menu(app).open {
        esc(app);
    }
    esc(app);
    click(app, MenuAction::OpenSettings);
    assert_eq!(menu(app).view, MenuView::Settings);
}

/// The live audio mix.
fn audio(app: &App) -> settings::AudioSettings {
    app.world()
        .resource::<SettingsState>()
        .settings
        .audio
        .clone()
}

/// Play one `←`/`→` step on the focused slider.
fn step_slider(app: &mut App, key: KeyCode) {
    assert!(
        menu::focus_control(menu(app).focus).is_some(),
        "a slider must be focused to step it"
    );
    press_and_update(app, key);
}

/// Walk the Settings view's vertical order down by `count` controls.
fn focus_down(app: &mut App, count: usize) {
    for _ in 0..count {
        press_and_update(app, KeyCode::ArrowDown);
    }
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

/// The rendered value of one live Settings label, read through the same
/// `sync_settings_labels` the overlay runs. This is the value seam: no
/// rendering, just the string the value node would show. The switch row's sans
/// label is a separate node, so a switch reads back as its bare `On`/`Off`
/// (`docs/title-and-menu.md` § Type system §2/§4).
fn settings_switch_text(app: &mut App, kind: SettingsLabelKind) -> String {
    let entity = app
        .world_mut()
        .spawn((SettingsLabel { kind }, Text::new(String::new())))
        .id();
    app.world_mut()
        .run_system_once(crate::presentation::settings_view::sync_settings_labels)
        .expect("the label sync runs");
    app.world()
        .get::<Text>(entity)
        .expect("the spawned label still exists")
        .0
        .clone()
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

    // Down walks the controls, wrapping through the back control.
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
        MenuFocus::AudioMaster,
    ] {
        press_and_update(&mut app, KeyCode::ArrowDown);
        assert_eq!(menu(&app).focus, expected);
    }
    press_and_update(&mut app, KeyCode::ArrowUp);
    assert_eq!(menu(&app).focus, MenuFocus::ClearKey);

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
// Text speed reveal
// ---------------------------------------------------------------------------

/// A dialogue node built through the production helper, so the test fixture and
/// the app's span layout cannot drift (`docs/title-and-menu.md` § Type system
/// §4).
fn spawn_dialogue(app: &mut App) -> Entity {
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();
    let node = {
        let mut commands = app.world_mut().commands();
        crate::presentation::play::spawn_dialogue(&mut commands, &faces)
    };
    app.world_mut().flush();
    node
}

/// A text block's root `Text` plus its `TextSpan` children, concatenated in
/// render order.
fn flattened_text(app: &App, root: Entity) -> String {
    fn walk(app: &App, entity: Entity, out: &mut String) {
        if let Some(text) = app.world().get::<Text>(entity) {
            out.push_str(&text.0);
        } else if let Some(span) = app.world().get::<TextSpan>(entity) {
            out.push_str(&span.0);
        }
        if let Some(children) = app.world().get::<Children>(entity) {
            for child in children.iter() {
                walk(app, child, out);
            }
        }
    }
    let mut out = String::new();
    walk(app, root, &mut out);
    out
}

fn dialogue_text(app: &App, node: Entity) -> String {
    flattened_text(app, node)
}

/// The parked say-wait, as `(speaker, text)`.
fn say_parts(app: &App) -> (String, String) {
    match app.world().resource::<Runtime>().runner.wait() {
        Some(Wait::Say { speaker, text }) => (speaker.clone(), text.clone()),
        other => panic!("expected a say, got {other:?}"),
    }
}

/// Run the presentation's one dialogue writer over the current park.
fn render_dialogue(app: &mut App) {
    app.world_mut()
        .run_system_once(crate::presentation::play::sync_wait_ui)
        .expect("sync_wait_ui runs");
}

/// Age the simulated clock and the reveal; the app is never slept on.
fn advance_time(app: &mut App, seconds: f32) {
    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f32(seconds));
    app.world_mut()
        .run_system_once(crate::presentation::play::advance_reveal)
        .expect("advance_reveal runs");
}

/// Set the stored reveal pace directly (persistence is exercised separately).
fn set_text_speed(app: &mut App, text_speed: f32) {
    app.world_mut()
        .resource_mut::<SettingsState>()
        .settings
        .play
        .text_speed = text_speed;
}

#[test]
fn at_instant_the_say_line_is_whole_on_the_first_frame_and_a_press_advances() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    let node = spawn_dialogue(&mut app);
    let (speaker, text) = say_parts(&app);
    assert_eq!(app.world().resource::<SettingsState>().text_speed(), 0.0);

    render_dialogue(&mut app);
    assert_eq!(dialogue_text(&app, node), format!("{speaker}: {text}"));
    assert!(
        app.world().resource::<TextReveal>().is_complete(0.0),
        "Instant positions the whole line with no partial state"
    );

    let before = app.world().resource::<Runtime>().runner.wait().cloned();
    press_and_update(&mut app, KeyCode::Space);
    assert_ne!(
        app.world().resource::<Runtime>().runner.wait().cloned(),
        before,
        "an Instant line has no reveal for a press to consume"
    );
}

#[test]
fn a_reveal_advances_with_simulated_time_and_keeps_the_prefix_whole() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    let node = spawn_dialogue(&mut app);
    set_text_speed(&mut app, 0.02);
    let (speaker, full) = say_parts(&app);
    assert_eq!(speaker, "Narrator", "the entry line is narration");
    let prefix = format!("{speaker}: ");

    render_dialogue(&mut app);
    assert_eq!(
        dialogue_text(&app, node),
        prefix,
        "the first frame shows the name plate and no text"
    );

    advance_time(&mut app, 0.5);
    render_dialogue(&mut app);
    let partial = dialogue_text(&app, node);
    assert!(
        partial.starts_with(&prefix),
        "the prefix is whole: {partial}"
    );
    let body = partial.strip_prefix(&prefix).unwrap();
    assert!(!body.is_empty() && full.starts_with(body), "a char prefix");

    advance_time(&mut app, 2.0);
    render_dialogue(&mut app);
    assert_eq!(dialogue_text(&app, node), format!("{speaker}: {full}"));
}

#[test]
fn a_fifty_char_reveal_takes_about_a_second_and_never_splits_a_codepoint() {
    let mut reveal = TextReveal::default();
    let text = "é".repeat(50);
    let wait = Some(Wait::Say {
        speaker: "Narrator".to_string(),
        text: text.clone(),
    });
    reveal.arm(&wait, 0.02);
    let say = wait.clone().unwrap();

    reveal.advance(0.99, &say, 0.02);
    assert!(
        reveal.chars_shown(0.02) < 50,
        "not yet whole just under a second"
    );
    reveal.advance(0.02, &say, 0.02);
    assert!(reveal.is_complete(0.02), "whole at ~1 s of 0.02 s/char");

    // Re-run the reveal from zero, one step at a time: every intermediate
    // render is a real `chars()` prefix, so a multi-byte char never splits.
    let mut stepwise = TextReveal::default();
    stepwise.arm(&wait, 0.02);
    let mut previous = 0;
    for _ in 0..60 {
        stepwise.advance(0.02, &say, 0.02);
        let rendered = crate::presentation::play::rendered_line(&wait, &stepwise, 0.02);
        let body = rendered.strip_prefix("Narrator: ").expect("prefix whole");
        let count = body.chars().count();
        assert!(text.starts_with(body), "never a mid-codepoint slice");
        assert!(count >= previous, "the reveal never runs backwards");
        previous = count;
    }
    assert_eq!(previous, 50);

    // A pace change mid-line alters the rate going forward but never rolls
    // already-revealed characters back (the progress is stored in characters).
    let mut slowed = TextReveal::default();
    slowed.arm(&wait, 0.02);
    slowed.advance(0.5, &say, 0.02);
    assert_eq!(slowed.chars_shown(0.02), 25);
    slowed.advance(0.0, &say, 0.08);
    assert_eq!(slowed.chars_shown(0.08), 25, "slowing never retreats");
}

#[test]
fn a_press_mid_reveal_completes_the_line_and_is_consumed() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    let node = spawn_dialogue(&mut app);
    set_text_speed(&mut app, 0.02);

    let before = app.world().resource::<Runtime>().runner.wait().cloned();
    render_dialogue(&mut app);
    let (_, full) = say_parts(&app);
    assert!(!app.world().resource::<TextReveal>().is_complete(0.02));

    press_and_update(&mut app, KeyCode::Space);
    assert_eq!(
        app.world().resource::<Runtime>().runner.wait().cloned(),
        before,
        "the completing press is consumed, never an advance"
    );
    render_dialogue(&mut app);
    assert!(dialogue_text(&app, node).ends_with(&full));

    press_and_update(&mut app, KeyCode::Space);
    assert_ne!(
        app.world().resource::<Runtime>().runner.wait().cloned(),
        before,
        "the next press advances"
    );
}

#[test]
fn a_left_click_mid_reveal_completes_and_is_consumed_like_a_key() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    let node = spawn_dialogue(&mut app);
    set_text_speed(&mut app, 0.02);

    let before = app.world().resource::<Runtime>().runner.wait().cloned();
    render_dialogue(&mut app);
    let (_, full) = say_parts(&app);

    click_and_update(&mut app);
    assert_eq!(
        app.world().resource::<Runtime>().runner.wait().cloned(),
        before,
        "the click completes rather than advances"
    );
    render_dialogue(&mut app);
    assert!(dialogue_text(&app, node).ends_with(&full));

    click_and_update(&mut app);
    assert_ne!(
        app.world().resource::<Runtime>().runner.wait().cloned(),
        before,
        "the next click advances"
    );
}

#[test]
fn the_decision_placeholder_and_end_marker_never_reveal() {
    let reveal = TextReveal::default();
    let decision = Some(Wait::Decision {
        npc_id: "ace".to_string(),
        question: "greet".to_string(),
        prefix: "reply".to_string(),
    });
    assert_eq!(
        crate::presentation::play::rendered_line(&decision, &reveal, 0.02),
        "ace is deciding…"
    );
    assert_eq!(
        crate::presentation::play::rendered_line(&None, &reveal, 0.02),
        "— end of scene —"
    );
    let choice = Some(Wait::Choice {
        options: vec![("Ask".to_string(), "ask".to_string())],
    });
    assert_eq!(
        crate::presentation::play::rendered_line(&choice, &reveal, 0.02),
        ""
    );
}

#[test]
fn a_say_wait_restored_from_a_slot_re_reveals_from_the_first_character() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    let node = spawn_dialogue(&mut app);
    set_text_speed(&mut app, 0.02);

    press_and_update(&mut app, KeyCode::Space);
    render_dialogue(&mut app);
    advance_time(&mut app, 0.5);
    render_dialogue(&mut app);
    let (speaker, full) = say_parts(&app);
    let partial = dialogue_text(&app, node);
    assert_ne!(partial, format!("{speaker}: {full}"), "mid-reveal");

    press_and_update(&mut app, KeyCode::F5);
    press_and_update(&mut app, KeyCode::F9);
    render_dialogue(&mut app);
    assert_eq!(
        dialogue_text(&app, node),
        format!("{speaker}: "),
        "a load re-arms the reveal at the first character"
    );
    assert!(!app.world().resource::<TextReveal>().is_complete(0.02));
}

#[test]
fn the_overlay_freezes_the_reveal_and_resumes_where_it_stopped() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    let node = spawn_dialogue(&mut app);
    set_text_speed(&mut app, 0.02);

    render_dialogue(&mut app);
    advance_time(&mut app, 0.5);
    render_dialogue(&mut app);
    let frozen_text = dialogue_text(&app, node);
    let shown_before = app.world().resource::<TextReveal>().chars_shown(0.02);
    assert!(shown_before > 0);

    esc(&mut app);
    assert!(menu(&app).open);
    advance_time(&mut app, 5.0);
    render_dialogue(&mut app);
    assert_eq!(
        app.world().resource::<TextReveal>().chars_shown(0.02),
        shown_before,
        "the reveal clock is frozen while the overlay is open"
    );
    assert_eq!(dialogue_text(&app, node), frozen_text);

    esc(&mut app);
    assert!(!menu(&app).open);
    advance_time(&mut app, 0.5);
    render_dialogue(&mut app);
    assert!(
        app.world().resource::<TextReveal>().chars_shown(0.02) > shown_before,
        "closing resumes from where it stopped"
    );
}

#[test]
fn the_text_speed_control_walks_the_named_ladder_and_persists_on_leave() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root.clone());
    open_settings(&mut app);
    focus_down(&mut app, 5); // Master -> Text speed
    assert_eq!(menu(&app).focus, MenuFocus::TextSpeed);
    assert_eq!(
        settings_switch_text(&mut app, SettingsLabelKind::TextSpeed),
        "Instant"
    );

    for name in ["Fast", "Normal", "Slow", "Slowest"] {
        press_and_update(&mut app, KeyCode::ArrowRight);
        assert_eq!(
            settings_switch_text(&mut app, SettingsLabelKind::TextSpeed),
            name
        );
    }
    press_and_update(&mut app, KeyCode::ArrowRight);
    assert_eq!(
        settings_switch_text(&mut app, SettingsLabelKind::TextSpeed),
        "Slowest",
        "clamped at the slowest step"
    );
    press_and_update(&mut app, KeyCode::ArrowLeft);
    assert_eq!(
        settings_switch_text(&mut app, SettingsLabelKind::TextSpeed),
        "Slow"
    );

    // A click on the row focuses the control. The pointer geometry that snaps a
    // click to a step lives in `settings_drag`, which a headless UI cannot lay
    // out; `set_text_speed_fraction`'s unit test covers the snapping rule.
    click(&mut app, MenuAction::TextSpeed);
    assert_eq!(menu(&app).focus, MenuFocus::TextSpeed);
    assert_eq!(
        settings_switch_text(&mut app, SettingsLabelKind::TextSpeed),
        "Slow",
        "the injected click does not press the mouse, so it changes no value"
    );

    assert!(
        !settings_path(&app).exists(),
        "no write while the view is open"
    );
    press_and_update(&mut app, KeyCode::Escape); // leave -> flush
    assert_eq!(settings::load(&pack_root).settings.play.text_speed, 0.04);
}

// ---------------------------------------------------------------------------
// Auto mode and the AUTO cue
// ---------------------------------------------------------------------------

/// The parked wait, cloned for comparison.
fn wait(app: &App) -> Option<Wait> {
    app.world().resource::<Runtime>().runner.wait().cloned()
}

/// Whether `play.auto` is on.
fn auto_on(app: &App) -> bool {
    app.world().resource::<SettingsState>().auto()
}

/// Turn auto on through the in-play `A` hotkey, exercising the real input path.
fn enable_auto(app: &mut App) {
    assert!(!auto_on(app), "auto starts off");
    press_and_update(app, KeyCode::KeyA);
    assert!(auto_on(app), "A turned auto on");
}

/// Run the core's auto dwell once, with whatever `Time` currently holds.
fn tick_auto(app: &mut App) {
    app.world_mut()
        .run_system_once(crate::core::auto_dwell)
        .expect("auto_advance runs");
}

/// Run the real `Update` schedule, so the plugin's `run_if` gates (the overlay
/// and the defensive hold) are honoured exactly as in the app.
fn tick_update(app: &mut App) {
    app.world_mut().run_schedule(Update);
}

/// Build the presentation UI once so the `AUTO` cue and help line exist.
fn build_ui(app: &mut App) {
    app.world_mut()
        .run_system_once(crate::presentation::play::setup_ui)
        .expect("setup_ui runs");
}

/// The rendered `AUTO` cue, read through the same `sync_auto_cue` the app runs.
fn auto_cue_text(app: &mut App) -> String {
    app.world_mut()
        .run_system_once(crate::presentation::play::sync_auto_cue)
        .expect("sync_auto_cue runs");
    let world = app.world_mut();
    let mut query = world.query::<(&crate::presentation::play::AutoCue, &Text)>();
    query
        .iter(world)
        .map(|(_, text)| text.0.clone())
        .next()
        .expect("the AUTO cue exists")
}

/// The always-visible in-play help line text.
fn help_line(app: &mut App) -> String {
    let world = app.world_mut();
    let mut query = world.query::<&Text>();
    query
        .iter(world)
        .map(|text| text.0.clone())
        .find(|text| text.contains("advance"))
        .expect("the in-play help line exists")
}

#[test]
fn auto_advances_an_instant_line_two_seconds_after_it_parks() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    enable_auto(&mut app);
    let start = wait(&app);
    assert!(start.is_some());

    advance_time(&mut app, 1.9);
    tick_auto(&mut app);
    assert_eq!(wait(&app), start, "1.9 s is short of the dwell");

    advance_time(&mut app, 0.2);
    tick_auto(&mut app);
    assert_ne!(wait(&app), start, "the say advances at 2.0 s");
}

#[test]
fn auto_waits_for_the_reveal_and_then_dwells_two_seconds() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    set_text_speed(&mut app, 0.02);
    render_dialogue(&mut app); // arm the reveal for the parked line
    enable_auto(&mut app);

    // A tick while the line is still revealing must not start the countdown. If
    // it did, the 1.9 s below would fire the say early.
    advance_time(&mut app, 0.5);
    tick_auto(&mut app);
    let line = wait(&app);
    assert!(line.is_some(), "the line is still parked");

    // This span finishes the reveal; the dwell begins exactly at its end.
    advance_time(&mut app, 1.9);
    tick_auto(&mut app);
    assert_eq!(
        wait(&app),
        line,
        "the dwell starts only when the reveal ends"
    );

    advance_time(&mut app, 1.9);
    tick_auto(&mut app);
    assert_eq!(
        wait(&app),
        line,
        "the whole dwell runs from the reveal's end"
    );

    advance_time(&mut app, 0.2);
    tick_auto(&mut app);
    assert_ne!(wait(&app), line, "2.0 s after a completed reveal");
}

#[test]
fn a_manual_advance_onto_an_identical_line_restarts_the_dwell() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    // Two consecutive identical say-lines are indistinguishable by their
    // `Wait` (the recorded reveal quirk), but they are distinct parks with
    // distinct `pc`, so the second must get its own full dwell.
    std::fs::write(
        pack_root.join("scenes/first_meeting.unmei"),
        "bg school_gate\nplay music_morning\nAce: Same line.\nAce: Same line.\nNarrator: End.\n",
    )
    .unwrap();
    let mut app = app_for_pack(pack_root);
    enable_auto(&mut app);
    let first = wait(&app).expect("the first say parks");

    advance_time(&mut app, 1.5);
    tick_auto(&mut app); // ~0.5 s left on the first line
    press_and_update(&mut app, KeyCode::Space);
    let second = wait(&app).expect("the duplicate say parks");
    assert_eq!(
        second, first,
        "the two lines really are identical to the waiter"
    );

    advance_time(&mut app, 1.9);
    tick_auto(&mut app);
    assert_eq!(
        wait(&app),
        Some(second),
        "the duplicate line gets a fresh full 2.0 s"
    );
}

#[test]
fn a_manual_advance_during_the_dwell_restarts_the_full_two_seconds() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    enable_auto(&mut app);
    let first = wait(&app).expect("parked on the entry say");

    advance_time(&mut app, 1.5);
    tick_auto(&mut app); // ~0.5 s left on the first line

    // The manual press lands on the next line; its dwell starts at full again.
    press_and_update(&mut app, KeyCode::Space);
    let second = wait(&app).expect("parked on the next say");
    assert_ne!(second, first);

    advance_time(&mut app, 1.9);
    tick_auto(&mut app);
    assert_eq!(
        wait(&app),
        Some(second.clone()),
        "no remainder carried into the new line"
    );

    advance_time(&mut app, 0.2);
    tick_auto(&mut app);
    assert_ne!(wait(&app), Some(second), "a full 2.0 s on the new line");
}

#[test]
fn auto_holds_at_a_choice_wait_and_never_chooses() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    enable_auto(&mut app);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));

    let before = parked_position(&app);
    for _ in 0..3 {
        advance_time(&mut app, 5.0);
        tick_auto(&mut app);
    }
    assert_eq!(
        parked_position(&app),
        before,
        "auto never calls choose at a choice-wait"
    );
}

#[test]
fn auto_holds_at_a_decision_wait_and_never_touches_the_resolver() {
    let fake = FakeResolver::new(model_answer("cold"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    enable_auto(&mut app);
    assert!(auto_on(&app));
    assert_eq!(fake.calls(), 1, "the parked request was spawned once");

    let before = parked_position(&app);
    for _ in 0..3 {
        advance_time(&mut app, 5.0);
        tick_auto(&mut app);
    }
    assert_eq!(
        parked_position(&app),
        before,
        "auto never resolves or advances a decision-wait"
    );
    assert_eq!(fake.calls(), 1, "auto never starts another request");
}

#[test]
fn auto_holds_at_the_end_of_the_scene() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    enable_auto(&mut app);
    play_to_end(&mut app);
    let before = parked_position(&app);
    assert!(before.1.is_none(), "the script ended");

    for _ in 0..3 {
        advance_time(&mut app, 5.0);
        tick_auto(&mut app);
    }
    assert_eq!(parked_position(&app), before, "nothing left to advance");
}

#[test]
fn the_overlay_freezes_the_auto_dwell_without_accumulating() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    enable_auto(&mut app);
    tick_update(&mut app); // start the dwell
    advance_time(&mut app, 1.5);
    tick_update(&mut app); // ~0.5 s left
    let line = wait(&app);
    assert!(line.is_some());

    esc(&mut app);
    assert!(menu(&app).open);
    for _ in 0..3 {
        advance_time(&mut app, 2.0);
        tick_update(&mut app);
    }
    assert_eq!(wait(&app), line, "no dwell fires while the overlay is open");

    esc(&mut app);
    assert!(!menu(&app).open);
    advance_time(&mut app, 0.4);
    tick_update(&mut app);
    assert_eq!(wait(&app), line, "the dwell resumes from where it stopped");
    advance_time(&mut app, 0.3);
    tick_update(&mut app);
    assert_ne!(wait(&app), line, "the remaining ~0.5 s completes");
}

#[test]
fn the_auto_hotkey_and_settings_switch_stay_in_sync_and_flush_later() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root.clone());

    press_and_update(&mut app, KeyCode::KeyA);
    assert!(auto_on(&app));
    assert!(app.world().resource::<SettingsState>().dirty);
    assert!(
        !settings_path(&app).exists(),
        "there is no write on key press"
    );

    open_settings(&mut app);
    assert_eq!(
        settings_switch_text(&mut app, SettingsLabelKind::Auto),
        "On",
        "the switch reflects the hotkey without a reopen"
    );

    // The Settings switch and the hotkey share one toggle.
    focus_down(&mut app, 6); // Master -> PlayAuto
    assert_eq!(menu(&app).focus, MenuFocus::PlayAuto);
    press_and_update(&mut app, KeyCode::Enter);
    assert!(!auto_on(&app), "the switch turns auto off");
    press_and_update(&mut app, KeyCode::Enter);
    assert!(auto_on(&app));

    // Leaving Settings is a flush point: the pending toggle lands.
    press_and_update(&mut app, KeyCode::Escape);
    assert!(!app.world().resource::<SettingsState>().dirty);
    assert!(settings::load(&pack_root).settings.play.auto);
}

#[test]
fn a_is_inert_at_a_choice_wait_and_ignored_while_the_overlay_is_open() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root.clone());

    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
    press_and_update(&mut app, KeyCode::KeyA);
    assert!(!auto_on(&app), "A does nothing at a choice-wait");
    assert!(!app.world().resource::<SettingsState>().dirty);
    assert!(!settings_path(&app).exists());

    esc(&mut app);
    assert!(menu(&app).open);
    press_and_update(&mut app, KeyCode::KeyA);
    assert!(!auto_on(&app), "the overlay owns input while it is open");
    assert!(!app.world().resource::<SettingsState>().dirty);
}

#[test]
fn auto_survives_a_choice_pick_and_a_load() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    enable_auto(&mut app);

    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
    press_and_update(&mut app, KeyCode::Digit1);
    assert!(auto_on(&app), "auto survives a choice pick");

    press_and_update(&mut app, KeyCode::F5);
    assert!(auto_on(&app));
    press_and_update(&mut app, KeyCode::F9);
    assert!(auto_on(&app), "auto survives a load");
}

#[test]
fn the_auto_cue_flips_with_the_hotkey_and_the_help_line_carries_the_hotkeys() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    build_ui(&mut app);

    assert_eq!(auto_cue_text(&mut app), "", "the cue is absent by default");
    press_and_update(&mut app, KeyCode::KeyA);
    assert!(auto_on(&app));
    assert_eq!(auto_cue_text(&mut app), "AUTO", "the cue appears with A");
    press_and_update(&mut app, KeyCode::KeyA);
    assert!(!auto_on(&app));
    assert_eq!(auto_cue_text(&mut app), "", "the cue disappears with A");

    let help = help_line(&mut app);
    assert!(help.contains("A auto"), "{help}");
    assert!(help.contains("F11 fullscreen"), "{help}");
    assert!(!help.contains("F5"), "{help}");
    assert!(!help.contains("F9"), "{help}");
}

#[test]
fn a_load_onto_the_parked_line_restarts_the_dwell() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    enable_auto(&mut app);
    let line = wait(&app).expect("the entry say parks");

    advance_time(&mut app, 1.5);
    tick_auto(&mut app); // ~0.5 s left

    // Save then load the very line the dwell is counting on.
    press_and_update(&mut app, KeyCode::F5);
    press_and_update(&mut app, KeyCode::F9);
    assert_eq!(wait(&app), Some(line.clone()));
    assert!(auto_on(&app));

    advance_time(&mut app, 1.9);
    tick_auto(&mut app);
    assert_eq!(
        wait(&app),
        Some(line.clone()),
        "the loaded line gets its own full 2.0 s"
    );

    advance_time(&mut app, 0.2);
    tick_auto(&mut app);
    assert_ne!(wait(&app), Some(line));
}

#[test]
fn toggling_auto_off_and_on_restarts_the_dwell() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    enable_auto(&mut app);
    let line = wait(&app).expect("the entry say parks");

    advance_time(&mut app, 1.5);
    tick_auto(&mut app); // ~0.5 s left

    press_and_update(&mut app, KeyCode::KeyA); // off: disarm
    assert!(!auto_on(&app));
    press_and_update(&mut app, KeyCode::KeyA); // on: a fresh park
    assert!(auto_on(&app));

    advance_time(&mut app, 1.9);
    tick_auto(&mut app);
    assert_eq!(
        wait(&app),
        Some(line.clone()),
        "turning auto off clears the elapsed time"
    );

    advance_time(&mut app, 0.2);
    tick_auto(&mut app);
    assert_ne!(wait(&app), Some(line));
}

// ---------------------------------------------------------------------------
// The gated title
// ---------------------------------------------------------------------------

/// A copied pack kept alive for the test app's lifetime, so a title test never
/// reads the developer's save files beside `examples/demo-pack`.
#[derive(Resource)]
struct TempPack(#[allow(dead_code)] tempfile::TempDir);

/// Boot the title over a fresh copy of the demo pack: deterministic (no
/// autosave, so Continue is disabled) and never touching the repo.
fn temp_title_app() -> App {
    temp_title_app_with_manifest(None)
}

/// Boot the title over a fresh demo-pack copy, replacing `pack.ron` when a
/// manifest is given (for tagline rendering tests).
fn temp_title_app_with_manifest(manifest: Option<&str>) -> App {
    let dir = temp_pack();
    if let Some(manifest) = manifest {
        std::fs::write(dir.path().join("pack/pack.ron"), manifest).unwrap();
    }
    let mut app = boot_app(dir.path().join("pack"), None);
    app.insert_resource(TempPack(dir));
    app
}

/// A title app with the asset rig attached, so a declared `title_backdrop`
/// loads through the real watcher and can be sized to cover. A `PrimaryWindow`
/// supplies the fit's target size.
fn title_asset_app(manifest: Option<&str>) -> App {
    title_asset_app_with_assets(manifest, &[])
}

/// As [`title_asset_app`], with extra bytes written under the pack's `assets/`
/// before the pack load — an empty `png` is the loader's undecodable case.
fn title_asset_app_with_assets(manifest: Option<&str>, extra: &[(&str, &[u8])]) -> App {
    let dir = temp_pack();
    if let Some(manifest) = manifest {
        std::fs::write(dir.path().join("pack/pack.ron"), manifest).unwrap();
    }
    let pack_root = dir.path().join("pack");
    for (rel, bytes) in extra {
        let path = pack_root.join("assets").join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(AssetPlugin {
            file_path: pack_root.join("assets").to_string_lossy().into_owned(),
            ..default()
        })
        // The same 1x1 test double the asset-watch tests use, so a present file
        // loads and a missing one fails without a real decoder.
        .init_asset::<Image>()
        .init_asset_loader::<PathImageLoader>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<AssetWatch>()
        .init_resource::<crate::presentation::MenuBuilt>()
        .insert_resource(RuntimeConfig::new(pack_root, None).with_dev_mode(true))
        .add_plugins(RuntimeCorePlugin)
        .add_systems(
            Update,
            (
                crate::presentation::sync_menu_ui,
                crate::presentation::title::fit_title_backdrop,
                report_silent_degradations,
            )
                .chain(),
        );
    app.world_mut().spawn((
        // The real window config, so the title-layout tests target the size and
        // floor the shipping primary window carries.
        settings::window_config(),
        PrimaryWindow,
    ));
    attach_text_faces(&mut app);
    app.insert_resource(TempPack(dir));
    app.update(); // Startup: load the pack, then show the title
    app
}

/// The built title's backdrop image node and layout node, if the title drew one.
fn title_backdrop_node(app: &App) -> Option<(ImageNode, Node)> {
    app.world().iter_entities().find_map(|entity| {
        entity
            .get::<crate::presentation::title::TitleBackdropImage>()
            .is_some()
            .then(|| {
                let image = entity.get::<ImageNode>()?.clone();
                let node = entity.get::<Node>()?.clone();
                Some((image, node))
            })
            .flatten()
    })
}

/// Every text node's string, for asserting the title's rendered copy.
fn all_texts(app: &App) -> Vec<String> {
    app.world()
        .iter_entities()
        .filter_map(|entity| entity.get::<Text>().map(|text| text.0.clone()))
        .collect()
}

/// The label rendered beside one title action, if the title is built: the sans
/// root plus any mono caption span, so Gallery reads back as one two-line
/// string.
fn title_label(app: &App, action: MenuAction) -> Option<String> {
    app.world().iter_entities().find_map(|entity| {
        match (entity.get::<MenuAction>(), entity.get::<Text>()) {
            (Some(found), Some(_)) if *found == action => Some(flattened_text(app, entity.id())),
            _ => None,
        }
    })
}

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

/// Build the title/overlay tree through the real presentation system.
fn build_menu_ui(app: &mut App) {
    app.world_mut()
        .run_system_once(crate::presentation::sync_menu_ui)
        .expect("sync_menu_ui runs");
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
        ScriptRunner::new(pack).run().wait
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
    let key = format!("sk-or-v1-{}", "m".repeat(60));
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
        runner.run();
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
            fingerprint: "sk-or-\u{2026}a1b2".to_string(),
        },
        KeyRow::FromEnvironment {
            fingerprint: "sk-or-\u{2026}env".to_string(),
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
// The Decisions group and the credential file
// ---------------------------------------------------------------------------

/// The Decisions group's key row state line, if the group is built.
fn key_row_text(app: &App) -> Option<String> {
    app.world().iter_entities().find_map(|entity| {
        entity.get::<crate::presentation::settings_view::KeyRowText>()?;
        entity.get::<Text>().map(|text| text.0.clone())
    })
}

/// The credential records captured under the failure target.
fn credential_lines() -> Vec<(log::Level, String, String)> {
    captures::take()
        .into_iter()
        .filter(|(_, target, _)| target == crate::failure::LOG_TARGET)
        .collect()
}

#[test]
fn the_decisions_group_renders_last_with_the_read_only_key_row_and_path_line() {
    let pack = temp_pack();
    let config_dir = tempfile::tempdir().unwrap();
    let path = credentials::credentials_path_in(config_dir.path());
    let key = format!("sk-or-v1-{}", "a".repeat(60));
    let fingerprint = credentials::fingerprint(&key);

    // The overlay door.
    let mut app = app_for_pack(pack.path().join("pack"));
    open_settings(&mut app);
    app.init_resource::<crate::presentation::MenuBuilt>();
    app.insert_resource(CredentialsState {
        config_dir: Some(config_dir.path().to_path_buf()),
        path: Some(path.clone()),
        row: KeyRow::Set {
            fingerprint: fingerprint.clone(),
        },
    });
    build_menu_ui(&mut app);

    let texts = all_texts(&app);
    for group in ["Audio", "Play", "Decisions"] {
        assert!(texts.iter().any(|text| text == group), "missing {group}");
    }
    let expected = format!("Set \u{2014} {fingerprint}");
    assert_eq!(
        key_row_text(&app).as_deref(),
        Some(expected.as_str()),
        "{texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|text| text == &format!("Stored in {}", path.display())),
        "the path line renders: {texts:?}"
    );
    assert!(
        texts.iter().all(|text| !text.contains(&key)),
        "the raw key is never rendered"
    );

    // The title door renders the same group.
    let mut titled = temp_title_app();
    titled.init_resource::<crate::presentation::MenuBuilt>();
    titled.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleSettings;
    press_and_update(&mut titled, KeyCode::Enter);
    titled.insert_resource(CredentialsState {
        config_dir: Some(config_dir.path().to_path_buf()),
        path: Some(path.clone()),
        row: KeyRow::Set { fingerprint },
    });
    build_menu_ui(&mut titled);
    assert!(
        all_texts(&titled).iter().any(|text| text == "Decisions"),
        "the Decisions group is shared by both doors"
    );
    let titled_expected = format!("Set \u{2014} sk-or-\u{2026}{}", "a".repeat(4));
    assert_eq!(
        key_row_text(&titled).as_deref(),
        Some(titled_expected.as_str())
    );
}

#[test]
fn the_key_row_renders_the_environment_state_with_the_env_note() {
    let pack = temp_pack();
    let env = "sk-or-v1-0123456789abcdef";
    let mut app = app_for_pack(pack.path().join("pack"));
    app.init_resource::<crate::presentation::MenuBuilt>();
    app.insert_resource(CredentialsState {
        config_dir: None,
        path: None,
        row: KeyRow::FromEnvironment {
            fingerprint: credentials::fingerprint(env),
        },
    });
    open_settings(&mut app);
    build_menu_ui(&mut app);

    let text = key_row_text(&app).expect("the key row renders");
    assert!(
        text.starts_with(&format!(
            "From environment \u{2014} {}",
            credentials::fingerprint(env)
        )),
        "{text}"
    );
    assert!(text.contains(credentials::ENV_NOTE), "{text}");
    assert!(
        all_texts(&app)
            .iter()
            .any(|text| text == "Stored in \u{2014}"),
        "no derivable directory names a dash"
    );
}

#[test]
fn the_key_row_renders_the_invalid_state_for_a_shape_failure() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_settings(&mut app);
    app.init_resource::<crate::presentation::MenuBuilt>();
    app.insert_resource(CredentialsState {
        config_dir: None,
        path: None,
        row: KeyRow::Invalid,
    });
    build_menu_ui(&mut app);

    assert_eq!(
        key_row_text(&app).as_deref(),
        Some("Invalid \u{2014} does not look like a key")
    );
}

#[test]
fn a_default_credentials_resource_renders_not_set_without_a_path() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    app.init_resource::<crate::presentation::MenuBuilt>();
    open_settings(&mut app);
    build_menu_ui(&mut app);

    assert_eq!(key_row_text(&app).as_deref(), Some("Not set"));
    assert!(
        all_texts(&app)
            .iter()
            .any(|text| text == "Stored in \u{2014}"),
        "a host with no preloaded read renders a dash, not a path"
    );
}

#[test]
fn the_decisions_group_is_the_last_walk_stop() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_settings(&mut app);

    focus_down(&mut app, 8); // Master -> PlayReset, the play group's last control
    assert_eq!(menu(&app).focus, MenuFocus::PlayReset);
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::KeyRow,
        "the Decisions group follows the play group"
    );
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::CopyKeyPath);
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::ClearKey,
        "the Decisions group's last control is the walk's last stop"
    );
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::AudioMaster,
        "the walk wraps to the first control"
    );
    press_and_update(&mut app, KeyCode::ArrowUp);
    assert_eq!(menu(&app).focus, MenuFocus::ClearKey);
}

/// The Decisions group is the last walk stop from the title door too
/// (`docs/title-and-menu.md` § Decisions settings §6).
#[test]
fn the_decisions_group_is_the_last_walk_stop_from_the_title_door() {
    let mut app = temp_title_app();
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleSettings;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).view, MenuView::Settings);
    assert_eq!(menu(&app).settings_origin, menu::MenuOrigin::Title);

    focus_down(&mut app, 11); // AudioMaster -> ClearKey
    assert_eq!(
        menu(&app).focus,
        MenuFocus::ClearKey,
        "the Decisions group is the title door's last stop"
    );
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::AudioMaster,
        "the walk wraps to the first control"
    );
}

/// One Settings door for the headless scroll regressions. Both doors mount the
/// same body inside a bounded `SettingsBody` viewport; only the surrounding
/// chrome differs.
#[derive(Clone, Copy, Debug)]
enum SettingsDoor {
    Title,
    Overlay,
}

impl SettingsDoor {
    /// The door's app with its Settings view open and its tree built. The temp
    /// pack is kept alive by a `TempPack` resource.
    fn app(self) -> App {
        let dir = temp_pack();
        let pack_root = dir.path().join("pack");
        let mut app = match self {
            SettingsDoor::Title => boot_app(pack_root, None),
            SettingsDoor::Overlay => app_for_pack(pack_root),
        };
        app.insert_resource(TempPack(dir));
        match self {
            SettingsDoor::Title => {
                app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleSettings;
                press_and_update(&mut app, KeyCode::Enter);
            }
            SettingsDoor::Overlay => open_settings(&mut app),
        }
        assert_eq!(menu(&app).view, MenuView::Settings, "{self:?} door open");
        build_pause_tree(&mut app);
        app
    }
}

/// A synthetic Settings frame for the headless scroll tests: a bounded viewport
/// with the twelve walk stops and the door-aware back control laid out one row
/// at a time, standing in for the layout pass a core app does not run. The
/// content keeps its natural height (13 rows), so the body has a real scroll
/// range.
const SETTINGS_VIEWPORT: Vec2 = Vec2::new(680.0, 300.0);
const SETTINGS_ROW: Vec2 = Vec2::new(440.0, 40.0);
const SETTINGS_ROW_STEP: f32 = 60.0;
const SETTINGS_ROWS: usize = 13;
const SETTINGS_CONTENT_HEIGHT: f32 = SETTINGS_ROWS as f32 * SETTINGS_ROW_STEP;
const SETTINGS_MAX_SCROLL: f32 = SETTINGS_CONTENT_HEIGHT - SETTINGS_VIEWPORT.y;

/// Lay the built Settings body and its controls out with the synthetic frame.
fn rig_settings_frame(
    mut commands: Commands,
    mut bodies: Query<
        (Entity, &mut ComputedNode),
        With<crate::presentation::settings_view::SettingsBody>,
    >,
    mut controls: Query<
        (Entity, &MenuAction, &mut ComputedNode),
        Without<crate::presentation::settings_view::SettingsBody>,
    >,
) {
    for (entity, mut node) in &mut bodies {
        node.size = SETTINGS_VIEWPORT;
        node.content_size = Vec2::new(SETTINGS_VIEWPORT.x, SETTINGS_CONTENT_HEIGHT);
        commands
            .entity(entity)
            .insert(UiGlobalTransform::from_translation(SETTINGS_VIEWPORT / 2.0));
    }
    let order: Vec<MenuAction> = crate::menu::SETTINGS_ORDER
        .iter()
        .map(|focus| crate::menu::action_for_focus(*focus))
        .collect();
    for (entity, action, mut node) in &mut controls {
        let index = if *action == MenuAction::BackToSlots {
            order.len()
        } else {
            match order.iter().position(|candidate| *candidate == *action) {
                Some(index) => index,
                None => continue,
            }
        };
        node.size = SETTINGS_ROW;
        let y = index as f32 * SETTINGS_ROW_STEP;
        commands
            .entity(entity)
            .insert(UiGlobalTransform::from_translation(Vec2::new(
                SETTINGS_ROW.x / 2.0,
                y + SETTINGS_ROW.y / 2.0,
            )));
    }
}

/// Register the real scroll systems and the synthetic frame over a built door.
fn attach_settings_scroll_rig(app: &mut App) {
    app.add_plugins(bevy::ui_widgets::ScrollAreaPlugin)
        .add_message::<bevy::input::mouse::MouseWheel>()
        .add_systems(
            Update,
            (
                rig_settings_frame,
                crate::presentation::settings_view::scroll_settings_focus,
                crate::presentation::settings_view::scroll_settings_wheel,
            )
                .chain()
                .after(RuntimeCoreSet),
        );
    app.world_mut().spawn((Window::default(), PrimaryWindow));
}

fn settings_body(app: &App) -> Entity {
    entity_with::<crate::presentation::settings_view::SettingsBody>(app)
}

fn settings_scroll(app: &App) -> f32 {
    app.world()
        .get::<ScrollPosition>(settings_body(app))
        .expect("the Settings body carries a ScrollPosition")
        .y
}

/// The focused control's top and bottom edges in the synthetic frame.
fn settings_control_extent(app: &App, action: MenuAction) -> (f32, f32) {
    let entity = action_entity(app, action);
    let node = app.world().get::<ComputedNode>(entity).unwrap();
    let transform = app.world().get::<UiGlobalTransform>(entity).unwrap();
    let bottom = transform.translation.y + node.size.y / 2.0;
    (bottom - node.size.y, bottom)
}

fn set_settings_cursor(app: &mut App, position: Vec2) {
    let mut query = app
        .world_mut()
        .query_filtered::<&mut Window, With<PrimaryWindow>>();
    query
        .single_mut(app.world_mut())
        .expect("the Settings window")
        .set_cursor_position(Some(position));
}

fn send_settings_wheel(app: &mut App, y: f32) {
    app.world_mut()
        .write_message(bevy::input::mouse::MouseWheel {
            unit: bevy::input::mouse::MouseScrollUnit::Line,
            x: 0.0,
            y,
            window: Entity::PLACEHOLDER,
            phase: bevy::input::touch::TouchPhase::Moved,
        });
}

/// Both Settings doors mount a bounded, scrollable viewport with the groups at
/// their natural height, so the lower controls the 1280×720 clipping hid keep
/// their reach (`docs/title-and-menu.md` § Title layout §5).
#[test]
fn both_settings_doors_have_a_bounded_scroll_body() {
    for door in [SettingsDoor::Title, SettingsDoor::Overlay] {
        let mut app = door.app();
        attach_settings_scroll_rig(&mut app);
        app.update();
        let body = settings_body(&app);
        assert!(
            app.world()
                .get::<bevy::ui_widgets::ScrollArea>(body)
                .is_some(),
            "{door:?}: the body is a Bevy scroll area"
        );
        let node = app.world().get::<Node>(body).unwrap();
        assert_eq!(
            node.overflow.y,
            bevy::ui::OverflowAxis::Scroll,
            "{door:?}: the body scrolls vertically"
        );
        // A bounded viewport with the automatic minimum size released, so the
        // content keeps its natural height and overflows into a real scroll
        // range instead of running past the window edge.
        assert_eq!(
            node.min_height,
            bevy::ui::Val::Px(0.0),
            "{door:?}: shrinkable"
        );
        assert!(
            node.max_height != bevy::ui::Val::Auto || node.flex_grow >= 1.0,
            "{door:?}: the viewport is bounded"
        );
    }
}

/// A wheel over either Settings body changes what is visible, and reaching the
/// bottom brings the door-aware back control into the viewport; a wheel off the
/// body is ignored (`docs/title-and-menu.md` § Title layout §5/§7).
#[test]
fn the_settings_wheel_scrolls_the_lower_controls_and_back_control_into_view() {
    for door in [SettingsDoor::Title, SettingsDoor::Overlay] {
        let mut app = door.app();
        attach_settings_scroll_rig(&mut app);
        app.update();
        assert_eq!(settings_scroll(&app), 0.0, "{door:?}: starts at the top");

        // A wheel off the body leaves it where it was.
        set_settings_cursor(&mut app, SETTINGS_VIEWPORT + Vec2::splat(50.0));
        send_settings_wheel(&mut app, -1.0);
        app.update();
        assert_eq!(
            settings_scroll(&app),
            0.0,
            "{door:?}: a wheel off the body does not scroll it"
        );

        // A wheel over the body moves it by the widget's line convention.
        set_settings_cursor(&mut app, SETTINGS_VIEWPORT / 2.0);
        send_settings_wheel(&mut app, -1.0);
        app.update();
        assert_eq!(
            settings_scroll(&app),
            bevy::input::mouse::MouseScrollUnit::SCROLL_UNIT_CONVERSION_FACTOR,
            "{door:?}: the wheel moved the panel one line"
        );

        // Wheeling to the bottom clamps and reveals the back control, which the
        // 1280×720 clipping had left past the window edge.
        for _ in 0..SETTINGS_ROWS {
            send_settings_wheel(&mut app, -1.0);
            app.update();
        }
        assert_eq!(
            settings_scroll(&app),
            SETTINGS_MAX_SCROLL,
            "{door:?}: the wheel clamps at the last group"
        );
        let (back_top, back_bottom) = settings_control_extent(&app, MenuAction::BackToSlots);
        let view_bottom = settings_scroll(&app) + SETTINGS_VIEWPORT.y;
        assert!(
            back_top >= settings_scroll(&app) && back_bottom <= view_bottom,
            "{door:?}: the back control is fully inside the viewport"
        );
    }
}

/// An open confirmation freezes the Settings panel, exactly as it freezes the
/// rest of the menu: a wheel over the backdrop never scrolls the content
/// underneath (`docs/runtime-hardening.md` § Menu overlay §6).
#[test]
fn the_settings_wheel_is_frozen_under_an_open_confirmation() {
    for door in [SettingsDoor::Title, SettingsDoor::Overlay] {
        let mut app = door.app();
        attach_settings_scroll_rig(&mut app);
        app.update();

        // Raise a confirmation from a mid-panel control. The structural rebuild
        // respawns the body at the top.
        app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::AudioReset;
        press_and_update(&mut app, KeyCode::Enter);
        assert_eq!(
            menu(&app).confirm,
            Some(ConfirmKind::ResetAudio),
            "{door:?}: the confirmation is up"
        );
        build_pause_tree(&mut app);
        app.update();
        assert_eq!(settings_scroll(&app), 0.0, "{door:?}: rebuilt at the top");

        // A wheel over the still-laid-out body is inert behind the confirm.
        set_settings_cursor(&mut app, SETTINGS_VIEWPORT / 2.0);
        send_settings_wheel(&mut app, -1.0);
        app.update();
        assert_eq!(
            settings_scroll(&app),
            0.0,
            "{door:?}: the confirmation freezes the panel"
        );
    }
}

/// Walking the Settings focus to every control, including the last stop
/// `Clear key`, scrolls the focused control fully into view on both doors
/// (`docs/title-and-menu.md` § Decisions settings §6).
#[test]
fn the_settings_walk_reveals_every_focused_control() {
    for door in [SettingsDoor::Title, SettingsDoor::Overlay] {
        let mut app = door.app();
        attach_settings_scroll_rig(&mut app);
        app.update();
        assert_eq!(
            menu(&app).focus,
            MenuFocus::AudioMaster,
            "{door:?}: first stop"
        );

        let stops = crate::menu::SETTINGS_ORDER.len();
        for step in 0..stops {
            let focus = menu(&app).focus;
            let action = crate::menu::action_for_focus(focus);
            let (top, bottom) = settings_control_extent(&app, action);
            let scroll = settings_scroll(&app);
            assert!(
                top >= scroll - 0.5 && bottom <= scroll + SETTINGS_VIEWPORT.y + 0.5,
                "{door:?}: {focus:?} is fully visible (top {top}, bottom {bottom}, scroll {scroll})"
            );
            if step + 1 < stops {
                press_and_update(&mut app, KeyCode::ArrowDown);
            }
        }
        assert_eq!(
            menu(&app).focus,
            MenuFocus::ClearKey,
            "{door:?}: the walk ends on the last stop"
        );
        assert!(
            settings_scroll(&app) > 0.0,
            "{door:?}: reaching the lower controls scrolled the body"
        );
    }
}

#[test]
fn opening_the_settings_view_creates_no_credential_file() {
    let pack = temp_pack();
    let config_dir = tempfile::tempdir().unwrap();
    let mut app = app_for_pack(pack.path().join("pack"));
    app.insert_resource(CredentialsState {
        config_dir: Some(config_dir.path().to_path_buf()),
        path: Some(credentials::credentials_path_in(config_dir.path())),
        row: KeyRow::NotSet,
    });

    open_settings(&mut app);

    assert!(
        !credentials::credentials_path_in(config_dir.path()).exists(),
        "opening the view writes nothing"
    );
    assert!(
        !config_dir.path().join("credentials.tmp").exists(),
        "and leaves no temp sibling"
    );
}

#[test]
fn a_boot_without_preloaded_credentials_stays_not_set_and_silent() {
    captures::install();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<CredentialsState>()
        .init_resource::<Notice>()
        .init_resource::<DefensiveHold>();

    // Drive the startup system directly: no preload means no read at all.
    captures::take();
    app.world_mut()
        .run_system_once(credentials::setup_credentials)
        .unwrap();

    assert!(
        credential_lines().is_empty(),
        "the hermetic default logs no credential line"
    );
    let state = app.world().resource::<CredentialsState>();
    assert_eq!(state.row, KeyRow::NotSet);
    assert!(state.path.is_none(), "no directory is derived in tests");
    assert!(state.config_dir.is_none());
}

#[test]
fn a_shape_failure_logs_once_without_quoting_the_value_or_its_fingerprint() {
    captures::install();
    let dir = tempfile::tempdir().unwrap();
    let config_dir = dir.path().join("unmei");
    std::fs::create_dir_all(&config_dir).unwrap();
    let value = "sk-or-v1-not-a-key-because of the space";
    std::fs::write(credentials::credentials_path_in(&config_dir), value).unwrap();

    let launch = credentials::resolve_launch(Some(&config_dir), None, |_| {
        panic!("a shape-invalid stored value must not build a resolver")
    });
    assert_eq!(launch.state.row, KeyRow::Invalid);
    assert!(launch.resolver.is_none());
    let warning = launch.warning.expect("a shape failure warns once");

    let pack = temp_pack();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .insert_resource(RuntimeConfig::new(pack.path().join("pack"), None).with_dev_mode(true))
        .insert_resource(credentials::StartupCredentials {
            state: launch.state,
            warning: Some(warning),
        });
    app.add_plugins(RuntimeCorePlugin);

    // Drive the startup system on this thread, so the line is captured rather
    // than emitted on a Bevy executor worker thread.
    captures::take();
    app.world_mut()
        .run_system_once(credentials::setup_credentials)
        .unwrap();

    let records = credential_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, log::Level::Warn);
    assert!(!records[0].2.contains(value), "{:?}", records[0].2);
    assert!(
        !records[0].2.contains(&credentials::fingerprint(value)),
        "{:?}",
        records[0].2
    );
    assert_eq!(
        app.world().resource::<CredentialsState>().row,
        KeyRow::Invalid,
        "the preloaded read lands on the resource"
    );
}

// ---------------------------------------------------------------------------
// The interactive key row
// ---------------------------------------------------------------------------

/// The built `EditableText` field's entity, if the row built one.
fn key_field_entity(app: &App) -> Option<Entity> {
    app.world()
        .iter_entities()
        .find_map(|entity| entity.get::<KeyField>().map(|_| entity.id()))
}

/// The field's current buffer.
fn field_text(app: &App) -> String {
    let entity = key_field_entity(app).expect("the key field is built");
    app.world()
        .entity(entity)
        .get::<EditableText>()
        .expect("the field is an EditableText")
        .value()
        .to_string()
}

/// Put `text` into the field directly, the way typing would.
fn set_field_text(app: &mut App, text: &str) {
    let entity = key_field_entity(app).expect("the key field is built");
    let mut entity = app.world_mut().entity_mut(entity);
    let mut editable = entity.get_mut::<EditableText>().expect("EditableText");
    editable.editor_mut().set_text(text);
}

/// The placeholder's current `Visibility`.
fn placeholder_visibility(app: &App) -> Option<Visibility> {
    app.world().iter_entities().find_map(|entity| {
        entity
            .get::<crate::presentation::settings_view::KeyPlaceholder>()
            .and_then(|_| entity.get::<Visibility>().copied())
    })
}

/// Open Settings and build the UI tree, so the field exists.
fn open_settings_with_ui(app: &mut App) {
    open_settings(app);
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(app);
}

#[test]
fn the_key_field_starts_empty_hides_the_key_and_shows_the_placeholder_only_on_not_set() {
    let pack = temp_pack();
    let config_dir = tempfile::tempdir().unwrap();
    let key = format!("sk-or-v1-{}", "a".repeat(60));
    credentials::write(config_dir.path(), &key).unwrap();

    let mut app = app_for_pack(pack.path().join("pack"));
    app.insert_resource(CredentialsState {
        config_dir: Some(config_dir.path().to_path_buf()),
        path: Some(credentials::credentials_path_in(config_dir.path())),
        row: KeyRow::NotSet,
    });
    open_settings_with_ui(&mut app);

    assert_eq!(
        app.world().resource::<CredentialsState>().row,
        KeyRow::Set {
            fingerprint: credentials::fingerprint(&key)
        },
        "the open read lands on the stored key"
    );
    assert_eq!(
        field_text(&app),
        "",
        "the stored key never enters the field"
    );
    assert!(
        all_texts(&app).iter().all(|text| !text.contains(&key)),
        "the raw key is never rendered"
    );
    app.world_mut()
        .run_system_once(crate::presentation::settings_view::sync_key_row)
        .unwrap();
    assert_eq!(
        placeholder_visibility(&app),
        Some(Visibility::Hidden),
        "a Set row shows no placeholder"
    );

    // Not set: the placeholder is the fixed copy and is visible.
    app.world_mut().resource_mut::<CredentialsState>().row = KeyRow::NotSet;
    app.world_mut()
        .run_system_once(crate::presentation::settings_view::sync_key_row)
        .unwrap();
    assert_eq!(placeholder_visibility(&app), Some(Visibility::Inherited));
    assert!(
        all_texts(&app)
            .iter()
            .any(|text| text == credentials::KEY_PLACEHOLDER),
        "the placeholder copy is fixed"
    );
}

#[test]
fn the_from_environment_row_has_no_editable_field() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    app.insert_resource(CredentialsState {
        config_dir: None,
        path: None,
        row: KeyRow::FromEnvironment {
            fingerprint: "sk-or-\u{2026}abcd".to_string(),
        },
    });
    open_settings_with_ui(&mut app);

    assert!(
        key_field_entity(&app).is_none(),
        "the environment row is read-only"
    );
}

#[test]
fn the_key_field_focus_model_walks_edits_commits_and_discards() {
    let pack = temp_pack();
    let config_dir = tempfile::tempdir().unwrap();
    let path = credentials::credentials_path_in(config_dir.path());
    let mut app = app_for_pack(pack.path().join("pack"));
    app.insert_resource(CredentialsState {
        config_dir: Some(config_dir.path().to_path_buf()),
        path: Some(path.clone()),
        row: KeyRow::NotSet,
    });
    open_settings_with_ui(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::KeyRow;

    // Enter enters editing; arrows no longer walk.
    press_and_update(&mut app, KeyCode::Enter);
    assert!(menu(&app).key_editing);
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::KeyRow,
        "arrows do not walk while editing"
    );

    // A shape failure writes nothing and keeps the typed text.
    set_field_text(&mut app, "short");
    press_and_update(&mut app, KeyCode::Enter);
    assert!(
        !menu(&app).key_editing,
        "Enter commits and returns to the walk"
    );
    assert_eq!(
        app.world().resource::<CredentialsState>().row,
        KeyRow::Invalid
    );
    assert!(!path.exists(), "a shape failure writes nothing");
    assert_eq!(field_text(&app), "short", "the typed text is kept");

    // A valid key lands the file, shows `Checking…`, and becomes `Set`. Every
    // test world keeps the hermetic `None` key-check default, which trusts the
    // key without a network call.
    press_and_update(&mut app, KeyCode::Enter);
    assert!(menu(&app).key_editing);
    let key = format!("sk-or-v1-{}", "b".repeat(60));
    set_field_text(&mut app, &key);
    press_and_update(&mut app, KeyCode::Enter);
    assert!(!menu(&app).key_editing);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        key,
        "the key is on disk before the ping"
    );
    pump_until(&mut app, |app| {
        app.world().resource::<CredentialsState>().row
            == KeyRow::Set {
                fingerprint: credentials::fingerprint(&key),
            }
    });
    assert_eq!(
        field_text(&app),
        "",
        "a committed key never stays in the field"
    );

    // The first Esc discards and stays; the second backs out of Settings.
    press_and_update(&mut app, KeyCode::Enter);
    set_field_text(&mut app, "another-value-here");
    press_and_update(&mut app, KeyCode::Escape);
    assert!(!menu(&app).key_editing);
    assert_eq!(menu(&app).view, MenuView::Settings);
    assert_eq!(field_text(&app), "", "Esc discards the typed text");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), key);
    press_and_update(&mut app, KeyCode::Escape);
    assert_eq!(menu(&app).view, MenuView::Slots, "the second Esc backs out");
}

#[test]
fn copy_path_writes_the_path_to_the_in_process_clipboard_and_returns_the_label() {
    let pack = temp_pack();
    let config_dir = tempfile::tempdir().unwrap();
    let path = credentials::credentials_path_in(config_dir.path());
    let mut app = app_for_pack(pack.path().join("pack"));
    app.insert_resource(CredentialsState {
        config_dir: Some(config_dir.path().to_path_buf()),
        path: Some(path.clone()),
        row: KeyRow::NotSet,
    });
    open_settings_with_ui(&mut app);

    click(&mut app, MenuAction::CopyKeyPath);
    assert_eq!(
        app.world().resource::<ClipboardPort>(),
        &ClipboardPort::Memory(Some(path.display().to_string())),
        "the path lands in the in-process buffer"
    );
    assert!(app.world().resource::<CopyPathLabel>().remaining > 0.0);

    app.world_mut()
        .run_system_once(crate::presentation::settings_view::sync_copy_path_label)
        .unwrap();
    assert!(
        all_texts(&app).iter().any(|text| text == "Copied"),
        "the label flips to Copied"
    );

    app.world_mut().resource_mut::<CopyPathLabel>().remaining = 0.0;
    app.world_mut()
        .run_system_once(crate::presentation::settings_view::sync_copy_path_label)
        .unwrap();
    assert!(
        all_texts(&app).iter().any(|text| text == "Copy path"),
        "the label returns to Copy path"
    );
}

#[test]
fn clear_key_confirms_once_deletes_the_file_and_returns_the_resolver_to_none() {
    let pack = temp_pack();
    let config_dir = tempfile::tempdir().unwrap();
    let path = credentials::credentials_path_in(config_dir.path());
    let key = format!("sk-or-v1-{}", "c".repeat(60));
    credentials::write(config_dir.path(), &key).unwrap();

    let mut app = app_for_pack(pack.path().join("pack"));
    app.insert_resource(CredentialsState {
        config_dir: Some(config_dir.path().to_path_buf()),
        path: Some(path.clone()),
        row: KeyRow::Set {
            fingerprint: credentials::fingerprint(&key),
        },
    });
    app.world_mut()
        .resource_mut::<crate::DecisionChannel>()
        .resolver = Some(Arc::new(FakeResolver::new(ResolvedDecision {
        action: "cold".to_string(),
        model: None,
        confidence: None,
        fallback_reason: None,
    })));

    open_settings_with_ui(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::ClearKey;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::ClearKey));

    // Cancelling changes nothing.
    click(&mut app, MenuAction::ConfirmCancel);
    assert!(path.exists());
    assert!(app
        .world()
        .resource::<crate::DecisionChannel>()
        .resolver
        .is_some());

    // Confirming deletes the file and drops the resolver the play path reads.
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::ClearKey;
    press_and_update(&mut app, KeyCode::Enter);
    click(&mut app, MenuAction::ConfirmAccept);
    assert!(!path.exists(), "the file is deleted");
    assert_eq!(
        app.world().resource::<CredentialsState>().row,
        KeyRow::NotSet
    );
    assert!(
        app.world()
            .resource::<crate::DecisionChannel>()
            .resolver
            .is_none(),
        "Clear returns the live resolver to None"
    );
}

#[test]
fn clear_key_is_disabled_while_the_environment_variable_wins() {
    let pack = temp_pack();
    let config_dir = tempfile::tempdir().unwrap();
    let path = credentials::credentials_path_in(config_dir.path());
    let key = format!("sk-or-v1-{}", "d".repeat(60));
    credentials::write(config_dir.path(), &key).unwrap();

    let mut app = app_for_pack(pack.path().join("pack"));
    app.insert_resource(CredentialsState {
        config_dir: Some(config_dir.path().to_path_buf()),
        path: Some(path.clone()),
        row: KeyRow::FromEnvironment {
            fingerprint: credentials::fingerprint(&key),
        },
    });
    open_settings_with_ui(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::ClearKey;
    press_and_update(&mut app, KeyCode::Enter);

    assert_eq!(
        menu(&app).confirm,
        None,
        "Clear is a no-op under the env key"
    );
    assert!(path.exists(), "the stored bytes are untouched");
}

#[test]
fn opening_settings_picks_up_a_hand_edit_without_rewriting_the_file() {
    let pack = temp_pack();
    let config_dir = tempfile::tempdir().unwrap();
    let path = credentials::credentials_path_in(config_dir.path());
    let first = format!("sk-or-v1-{}", "e".repeat(60));
    credentials::write(config_dir.path(), &first).unwrap();
    let bytes = std::fs::read(&path).unwrap();

    let mut app = app_for_pack(pack.path().join("pack"));
    app.insert_resource(CredentialsState {
        config_dir: Some(config_dir.path().to_path_buf()),
        path: Some(path.clone()),
        row: KeyRow::NotSet,
    });

    open_settings(&mut app);
    assert_eq!(
        app.world().resource::<CredentialsState>().row,
        KeyRow::Set {
            fingerprint: credentials::fingerprint(&first)
        }
    );
    assert_eq!(
        std::fs::read(&path).unwrap(),
        bytes,
        "a read rewrites nothing"
    );

    // A hand-edit behind the app is picked up on the next open.
    let second = format!("sk-or-v1-{}", "f".repeat(60));
    std::fs::write(&path, &second).unwrap();
    open_settings(&mut app);
    assert_eq!(
        app.world().resource::<CredentialsState>().row,
        KeyRow::Set {
            fingerprint: credentials::fingerprint(&second)
        },
        "the hand-edit is picked up on open"
    );
}

#[test]
fn the_editable_field_receives_keyboard_input_and_a_trimmed_in_process_paste() {
    use bevy::clipboard::ClipboardPlugin;
    use bevy::input::keyboard::{Key, KeyboardInput};
    use bevy::input::{ButtonState, InputPlugin};
    use bevy::input_focus::{FocusCause, InputDispatchPlugin, InputFocus, InputFocusPlugin};
    use bevy::picking::events::{Pointer, Release};
    use bevy::text::{apply_text_edits, EditableTextFilter, FontCx, LayoutCx};
    use bevy::ui::UiScale;
    use bevy::ui_widgets::EditableTextInputPlugin;
    use bevy::window::Ime;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(InputPlugin)
        .add_plugins(InputFocusPlugin)
        .add_plugins(InputDispatchPlugin)
        .add_plugins(ClipboardPlugin)
        .add_plugins(EditableTextInputPlugin)
        .init_resource::<FontCx>()
        .init_resource::<LayoutCx>()
        .init_resource::<UiScale>()
        .init_resource::<ClipboardPort>()
        .add_message::<Ime>()
        .add_message::<Pointer<Release>>()
        .add_systems(
            PostUpdate,
            crate::presentation::settings_view::trim_pasted_text
                .before(bevy::text::EditableTextSystems),
        )
        .add_systems(
            PostUpdate,
            apply_text_edits.in_set(bevy::text::EditableTextSystems),
        );

    let window = app
        .world_mut()
        .spawn((Window::default(), PrimaryWindow))
        .id();
    let field = app
        .world_mut()
        .spawn((
            KeyField,
            EditableText::new(""),
            Node::default(),
            TextFont::default(),
            TextColor::default(),
            EditableTextFilter::new(|character| character != '\n' && character != '\r'),
        ))
        .id();
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(field, FocusCause::Navigated);

    // Typing 'a' through the real keyboard chain.
    app.world_mut().write_message(KeyboardInput {
        key_code: KeyCode::KeyA,
        logical_key: Key::Character("a".into()),
        state: ButtonState::Pressed,
        text: Some("a".into()),
        repeat: false,
        window,
    });
    app.update();
    assert_eq!(
        app.world()
            .entity(field)
            .get::<EditableText>()
            .unwrap()
            .value()
            .to_string(),
        "a"
    );

    // Ctrl+V through the in-process clipboard: the trailing newline is trimmed
    // before the all-or-nothing filter, so the paste is not dropped whole.
    *app.world_mut().resource_mut::<ClipboardPort>() =
        ClipboardPort::Memory(Some("sk-pasted\n".into()));
    app.world_mut()
        .resource_mut::<ButtonInput<Key>>()
        .press(Key::Control);
    app.world_mut().write_message(KeyboardInput {
        key_code: KeyCode::KeyV,
        logical_key: Key::Character("v".into()),
        state: ButtonState::Pressed,
        text: Some("v".into()),
        repeat: false,
        window,
    });
    app.update();
    assert_eq!(
        app.world()
            .entity(field)
            .get::<EditableText>()
            .unwrap()
            .value()
            .to_string(),
        "ask-pasted",
        "the paste is trimmed and appended, not dropped whole"
    );
}

// ---------------------------------------------------------------------------
// Key validation on write
// ---------------------------------------------------------------------------

/// Install a fake key check on a booted app.
fn install_key_check(app: &mut App, checker: &FakeKeyCheck) {
    app.insert_resource(crate::KeyCheckChannel {
        checker: Some(Arc::new(checker.clone())),
    });
}

/// The keys recorded by [`recording_factory`].
type RecordedKeys = Arc<Mutex<Vec<String>>>;

/// A resolver builder that records the key it was built from and returns one
/// fixed resolver, so a rebuild is observable and provably keyed.
fn recording_factory(
    resolver: Arc<dyn DecisionResolver>,
) -> (RecordedKeys, crate::DecisionFactory) {
    let keys: RecordedKeys = Arc::new(Mutex::new(Vec::new()));
    let log = keys.clone();
    let factory = crate::DecisionFactory {
        builder: Some(Arc::new(move |key: &str| {
            log.lock().unwrap().push(key.to_string());
            resolver.clone()
        })),
    };
    (keys, factory)
}

/// Focus the key row, enter edit mode, type `key`, and commit with `Enter`.
fn commit_key_at(app: &mut App, key: &str) {
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::KeyRow;
    press_and_update(app, KeyCode::Enter);
    assert!(menu(app).key_editing, "Enter enters the field");
    set_field_text(app, key);
    press_and_update(app, KeyCode::Enter);
    assert!(
        !menu(app).key_editing,
        "Enter commits and returns to the walk"
    );
}

/// A booted, play-entered app with the Settings view open, a temp credential
/// directory, a fake key check and a recording resolver factory.
struct KeyCheckRig {
    app: App,
    _pack: tempfile::TempDir,
    config_dir: tempfile::TempDir,
    path: PathBuf,
    checker: FakeKeyCheck,
    keys: RecordedKeys,
}

fn key_check_rig(resolver: Arc<dyn DecisionResolver>) -> KeyCheckRig {
    let pack = temp_pack();
    let config_dir = tempfile::tempdir().unwrap();
    let path = credentials::credentials_path_in(config_dir.path());
    let checker = FakeKeyCheck::new();
    let (keys, factory) = recording_factory(resolver);
    let mut app = app_for_pack(pack.path().join("pack"));
    app.insert_resource(CredentialsState {
        config_dir: Some(config_dir.path().to_path_buf()),
        path: Some(path.clone()),
        row: KeyRow::NotSet,
    });
    install_key_check(&mut app, &checker);
    app.insert_resource(factory);
    open_settings_with_ui(&mut app);
    KeyCheckRig {
        app,
        _pack: pack,
        config_dir,
        path,
        checker,
        keys,
    }
}

#[test]
fn a_shape_failure_writes_nothing_and_never_pings() {
    let mut rig = key_check_rig(Arc::new(FakeResolver::new(model_answer("warm"))));
    commit_key_at(&mut rig.app, "short");
    rig.app.update();

    assert_eq!(rig.checker.calls(), 0, "a shape failure never pings");
    assert_eq!(
        rig.app.world().resource::<CredentialsState>().row,
        KeyRow::Invalid
    );
    assert!(!rig.path.exists(), "nothing is written");
    assert_eq!(field_text(&rig.app), "short", "the typed text is kept");
    assert!(rig.keys.lock().unwrap().is_empty(), "no resolver is built");
}

/// A write that fails reports row 23, leaves the row and the buffer as they
/// were, and runs no ping (`docs/title-and-menu.md` § Decisions settings §4.2,
/// §6).
#[test]
fn a_failed_write_runs_no_ping_and_keeps_the_row_and_buffer() {
    captures::install();
    let mut rig = key_check_rig(Arc::new(FakeResolver::new(model_answer("warm"))));
    force_single_threaded(&mut rig.app);
    // Block the atomic write: a directory where the temp sibling should go makes
    // `create_new` fail before any byte lands.
    std::fs::create_dir(rig.path.with_extension("tmp")).unwrap();

    let key = format!("sk-or-v1-{}", "w".repeat(60));
    captures::take();
    commit_key_at(&mut rig.app, &key);
    rig.app.update();

    assert_eq!(rig.checker.calls(), 0, "a failed write never pings");
    assert_eq!(
        rig.app.world().resource::<CredentialsState>().row,
        KeyRow::NotSet,
        "the row is left as it was"
    );
    assert_eq!(
        field_text(&rig.app),
        key,
        "the buffer is kept so the commit can be retried"
    );
    assert!(!rig.path.exists(), "no credential file lands");
    assert!(rig.keys.lock().unwrap().is_empty(), "no resolver is built");

    let notice = rig.app.world().resource::<Notice>();
    assert!(notice.is_showing(), "a failed write notices");
    assert_eq!(notice.text.as_deref(), Some("Could not save the key"));

    let records = credential_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, log::Level::Error, "row 23 logs error!");
    assert!(
        records[0].2.starts_with("Could not save the key"),
        "{records:?}"
    );
    assert!(!records[0].2.contains(&key), "the value is never logged");
}

#[test]
fn a_committed_key_is_on_disk_before_the_ping_and_shows_accepted() {
    let mut rig = key_check_rig(Arc::new(FakeResolver::new(model_answer("warm"))));
    let checker = rig.checker.clone();
    let key = format!("sk-or-v1-{}", "a".repeat(60));
    commit_key_at(&mut rig.app, &key);

    assert!(rig.path.exists(), "the write precedes the ping");
    assert_eq!(std::fs::read_to_string(&rig.path).unwrap(), key);
    assert_eq!(
        rig.app.world().resource::<CredentialsState>().row,
        KeyRow::Checking,
        "the row shows Checking while the ping runs"
    );

    // `Checking…` disables the field: Enter on the row is a no-op again.
    rig.app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::KeyRow;
    press_and_update(&mut rig.app, KeyCode::Enter);
    assert!(!menu(&rig.app).key_editing, "Checking… disables the input");

    pump_until(&mut rig.app, |_| checker.calls() == 1);
    checker.release(KeyVerdict::Accepted);
    let fingerprint = credentials::fingerprint(&key);
    pump_until(&mut rig.app, |app| {
        app.world().resource::<CredentialsState>().row
            == KeyRow::Set {
                fingerprint: fingerprint.clone(),
            }
    });
    assert_eq!(
        rig.keys.lock().unwrap().as_slice(),
        std::slice::from_ref(&key),
        "the resolver was rebuilt from the committed key"
    );
}

#[test]
fn an_unreachable_ping_shows_not_checked_and_rebuilds_the_resolver() {
    let mut rig = key_check_rig(Arc::new(FakeResolver::new(model_answer("warm"))));
    let checker = rig.checker.clone();
    let key = format!("sk-or-v1-{}", "b".repeat(60));
    commit_key_at(&mut rig.app, &key);

    pump_until(&mut rig.app, |_| checker.calls() == 1);
    checker.release(KeyVerdict::Unreachable);
    let fingerprint = credentials::fingerprint(&key);
    pump_until(&mut rig.app, |app| {
        app.world().resource::<CredentialsState>().row
            == KeyRow::SetUnchecked {
                fingerprint: fingerprint.clone(),
            }
    });
    assert_eq!(
        rig.keys.lock().unwrap().as_slice(),
        std::slice::from_ref(&key),
        "an unreachable ping still rebuilds"
    );
    assert!(rig.path.exists(), "the key stays on disk");
}

#[test]
fn a_rejected_ping_keeps_the_file_and_leaves_the_resolver_alone() {
    let original: Arc<dyn DecisionResolver> = Arc::new(FakeResolver::new(model_answer("cold")));
    let mut rig = key_check_rig(original.clone());
    rig.app
        .world_mut()
        .resource_mut::<DecisionChannel>()
        .resolver = Some(original.clone());
    let checker = rig.checker.clone();
    let key = format!("sk-or-v1-{}", "c".repeat(60));
    commit_key_at(&mut rig.app, &key);

    pump_until(&mut rig.app, |_| checker.calls() == 1);
    checker.release(KeyVerdict::Rejected);
    let fingerprint = credentials::fingerprint(&key);
    pump_until(&mut rig.app, |app| {
        app.world().resource::<CredentialsState>().row
            == KeyRow::Rejected {
                fingerprint: fingerprint.clone(),
            }
    });

    assert_eq!(
        std::fs::read_to_string(&rig.path).unwrap(),
        key,
        "the rejected key stays on disk"
    );
    let current = rig
        .app
        .world()
        .resource::<DecisionChannel>()
        .resolver
        .clone()
        .expect("a resolver is still configured");
    assert!(Arc::ptr_eq(&current, &original), "no swap happened");
    assert!(
        rig.keys.lock().unwrap().is_empty(),
        "a rejection never rebuilds"
    );
}

#[test]
fn a_ping_that_never_answers_still_leaves_the_key_on_disk() {
    let mut rig = key_check_rig(Arc::new(FakeResolver::new(model_answer("warm"))));
    let checker = rig.checker.clone();
    let key = format!("sk-or-v1-{}", "d".repeat(60));
    commit_key_at(&mut rig.app, &key);

    pump_until(&mut rig.app, |_| checker.calls() == 1);
    // Never release: the ping stays in flight across several frames.
    for _ in 0..4 {
        rig.app.update();
    }
    assert_eq!(
        std::fs::read_to_string(&rig.path).unwrap(),
        key,
        "the key is on disk before the ping answers"
    );
    assert_eq!(
        rig.app.world().resource::<CredentialsState>().row,
        KeyRow::Checking
    );
}

#[test]
fn there_is_no_standing_ping_at_launch_or_on_view_open() {
    let mut rig = key_check_rig(Arc::new(FakeResolver::new(model_answer("warm"))));
    for _ in 0..4 {
        rig.app.update();
    }
    assert_eq!(rig.checker.calls(), 0, "boot and view-open ping nothing");

    // Back out and re-open Settings: still no ping.
    press_and_update(&mut rig.app, KeyCode::Escape);
    click(&mut rig.app, MenuAction::OpenSettings);
    rig.app.update();
    assert_eq!(rig.checker.calls(), 0, "re-opening pings nothing");
}

#[test]
fn a_rejected_verdict_is_live_not_persisted() {
    let mut rig = key_check_rig(Arc::new(FakeResolver::new(model_answer("warm"))));
    let checker = rig.checker.clone();
    let key = format!("sk-or-v1-{}", "e".repeat(60));
    commit_key_at(&mut rig.app, &key);
    pump_until(&mut rig.app, |_| checker.calls() == 1);
    checker.release(KeyVerdict::Rejected);
    let fingerprint = credentials::fingerprint(&key);
    pump_until(&mut rig.app, |app| {
        app.world().resource::<CredentialsState>().row
            == KeyRow::Rejected {
                fingerprint: fingerprint.clone(),
            }
    });

    // A relaunch reads only the file: shape-valid, so it shows `Set`.
    let read = credentials::read(Some(rig.config_dir.path()), None);
    assert_eq!(
        read.state.row,
        KeyRow::Set {
            fingerprint: credentials::fingerprint(&key),
        },
        "the verdict is live, not persisted"
    );
}

#[test]
fn an_accepted_key_applies_in_session_to_a_decision_without_a_relaunch() {
    let resolver = FakeResolver::new(model_answer("warm"));
    let mut rig = key_check_rig(Arc::new(resolver.clone()));
    let checker = rig.checker.clone();

    let key = format!("sk-or-v1-{}", "f".repeat(60));
    commit_key_at(&mut rig.app, &key);
    pump_until(&mut rig.app, |_| checker.calls() == 1);
    checker.release(KeyVerdict::Accepted);
    let fingerprint = credentials::fingerprint(&key);
    pump_until(&mut rig.app, |app| {
        app.world().resource::<CredentialsState>().row
            == KeyRow::Set {
                fingerprint: fingerprint.clone(),
            }
    });
    assert_eq!(
        rig.keys.lock().unwrap().as_slice(),
        std::slice::from_ref(&key)
    );

    // Leave Settings and the overlay, then run a decision with no relaunch.
    press_and_update(&mut rig.app, KeyCode::Escape);
    press_and_update(&mut rig.app, KeyCode::Escape);
    assert!(!menu(&rig.app).open, "back in play");
    {
        let mut runtime = rig.app.world_mut().resource_mut::<Runtime>();
        drive_runner_to_decision(&mut runtime);
    }
    pump_until(&mut rig.app, |_| resolver.calls() == 1);
    assert_eq!(
        resolver.calls(),
        1,
        "the rebuilt resolver is the one called"
    );
    resolver.release();
    pump_until(&mut rig.app, parked_on_say);
    let runtime = rig.app.world().resource::<Runtime>();
    let Some(Wait::Say { text, .. }) = runtime.runner.wait() else {
        panic!(
            "expected the model branch's say, got {:?}",
            runtime.runner.wait()
        );
    };
    assert!(text.contains("You came back!"), "got {text:?}");
}

#[test]
fn a_shape_failure_commit_never_quotes_the_value_in_a_log() {
    captures::install();
    let mut rig = key_check_rig(Arc::new(FakeResolver::new(model_answer("warm"))));
    force_single_threaded(&mut rig.app);
    let secret = "sk-or-v1-not-a-key-because of the space";
    captures::take();

    commit_key_at(&mut rig.app, secret);
    rig.app.update();

    let records = captures::take();
    assert!(
        records
            .iter()
            .all(|(_, target, _)| target != crate::failure::LOG_TARGET),
        "a commit shape failure logs nothing under the failure target: {records:?}"
    );
    for (_, _, line) in records {
        assert!(!line.contains(secret), "the value leaked: {line}");
        assert!(
            !line.contains("because of the space"),
            "the value leaked: {line}"
        );
        assert!(
            !line.contains(&credentials::fingerprint(secret)),
            "more than the four-character fingerprint leaked: {line}"
        );
    }
    assert_eq!(rig.checker.calls(), 0);
}

#[test]
fn a_commit_without_a_checker_is_trusted_without_a_ping() {
    // The hermetic default: no checker installed, so the key is trusted with no
    // network call and no failure log. Production always installs
    // `HttpKeyCheck`; the offline default only exists for hostless worlds.
    let mut rig = key_check_rig(Arc::new(FakeResolver::new(model_answer("warm"))));
    rig.app.insert_resource(crate::KeyCheckChannel::default());
    let key = format!("sk-or-v1-{}", "g".repeat(60));
    commit_key_at(&mut rig.app, &key);

    let fingerprint = credentials::fingerprint(&key);
    pump_until(&mut rig.app, |app| {
        app.world().resource::<CredentialsState>().row
            == KeyRow::Set {
                fingerprint: fingerprint.clone(),
            }
    });
    assert!(rig.path.exists(), "the key is written");
    assert_eq!(
        rig.keys.lock().unwrap().as_slice(),
        std::slice::from_ref(&key),
        "the trusted key still builds a resolver"
    );
}

#[test]
fn clearing_while_checking_cancels_the_ping_and_never_lands_a_stale_verdict() {
    let original: Arc<dyn DecisionResolver> = Arc::new(FakeResolver::new(model_answer("cold")));
    let mut rig = key_check_rig(original.clone());
    rig.app
        .world_mut()
        .resource_mut::<DecisionChannel>()
        .resolver = Some(original.clone());
    let checker = rig.checker.clone();
    let key = format!("sk-or-v1-{}", "h".repeat(60));
    commit_key_at(&mut rig.app, &key);
    pump_until(&mut rig.app, |_| checker.calls() == 1);
    assert_eq!(
        rig.app.world().resource::<CredentialsState>().row,
        KeyRow::Checking
    );

    // Clear confirms once; the file goes, the resolver goes, the task is
    // dropped.
    rig.app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::ClearKey;
    press_and_update(&mut rig.app, KeyCode::Enter);
    assert_eq!(menu(&rig.app).confirm, Some(ConfirmKind::ClearKey));
    click(&mut rig.app, MenuAction::ConfirmAccept);
    assert!(!rig.path.exists(), "the file is deleted");
    assert_eq!(
        rig.app.world().resource::<CredentialsState>().row,
        KeyRow::NotSet
    );
    assert!(
        rig.app
            .world()
            .resource::<DecisionChannel>()
            .resolver
            .is_none(),
        "Clear returns the resolver to None"
    );
    assert!(
        rig.app.world().resource::<PendingKeyCheck>().task.is_none(),
        "the in-flight ping is cancelled"
    );

    // A late answer from the dropped future never lands.
    checker.release(KeyVerdict::Accepted);
    for _ in 0..4 {
        rig.app.update();
    }
    assert_eq!(
        rig.app.world().resource::<CredentialsState>().row,
        KeyRow::NotSet,
        "a stale verdict never lands"
    );
    assert!(
        rig.keys.lock().unwrap().is_empty(),
        "a cleared key never rebuilds"
    );
}

#[test]
fn a_ping_verdict_never_quotes_the_key_in_a_log() {
    captures::install();
    let mut rig = key_check_rig(Arc::new(FakeResolver::new(model_answer("warm"))));
    force_single_threaded(&mut rig.app);
    let checker = rig.checker.clone();
    let unreachable = format!("sk-or-v1-{}", "i".repeat(60));

    // Unreachable: the warn names the outcome, never the key.
    captures::take();
    commit_key_at(&mut rig.app, &unreachable);
    pump_until(&mut rig.app, |_| checker.calls() == 1);
    checker.release(KeyVerdict::Unreachable);
    let unreachable_fp = credentials::fingerprint(&unreachable);
    pump_until(&mut rig.app, |app| {
        app.world().resource::<CredentialsState>().row
            == KeyRow::SetUnchecked {
                fingerprint: unreachable_fp.clone(),
            }
    });
    let records = captures::take();
    assert!(
        !records.is_empty(),
        "the unreachable warn was captured on this thread"
    );
    for (_, _, line) in records {
        assert!(!line.contains(&unreachable), "the key leaked: {line}");
        assert!(
            !line.contains(&unreachable_fp),
            "the fingerprint leaked: {line}"
        );
    }

    // Rejected: same rule.
    checker.reset();
    let rejected = format!("sk-or-v1-{}", "j".repeat(60));
    captures::take();
    commit_key_at(&mut rig.app, &rejected);
    pump_until(&mut rig.app, |_| checker.calls() == 2);
    checker.release(KeyVerdict::Rejected);
    let rejected_fp = credentials::fingerprint(&rejected);
    pump_until(&mut rig.app, |app| {
        app.world().resource::<CredentialsState>().row
            == KeyRow::Rejected {
                fingerprint: rejected_fp.clone(),
            }
    });
    let records = captures::take();
    assert!(
        !records.is_empty(),
        "the rejected warn was captured on this thread"
    );
    for (_, _, line) in records {
        assert!(!line.contains(&rejected), "the key leaked: {line}");
        assert!(
            !line.contains(&rejected_fp),
            "the fingerprint leaked: {line}"
        );
    }
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

/// The bundled face a text entity names. Panics on Bevy's default handle or a
/// family-name source, so a stray default font fails the test
/// (`docs/title-and-menu.md` § Type system §5).
fn face_handle(app: &App, entity: Entity) -> Handle<bevy::text::Font> {
    match &app
        .world()
        .get::<TextFont>(entity)
        .unwrap_or_else(|| panic!("{entity:?} has no TextFont"))
        .font
    {
        bevy::text::FontSource::Handle(handle) => handle.clone(),
        other => panic!("{entity:?} names a non-bundled font source: {other:?}"),
    }
}

/// The first entity whose `Text` or `TextSpan` is exactly `text`.
fn text_entity(app: &App, text: &str) -> Entity {
    app.world()
        .iter_entities()
        .find_map(|entity| {
            let matches = entity.get::<Text>().is_some_and(|t| t.0 == text)
                || entity.get::<TextSpan>().is_some_and(|t| t.0 == text);
            matches.then_some(entity.id())
        })
        .unwrap_or_else(|| panic!("no text entity {text:?}"))
}

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

/// The no-executable startup records its reason before the app exists, and the
/// pack load is skipped so a second failure cannot overwrite it.
#[test]
fn a_recorded_no_executable_failure_survives_startup_without_a_pack_load() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .insert_resource(
            RuntimeConfig::new(PathBuf::from("definitely/not/a/pack"), None).with_dev_mode(true),
        )
        .insert_resource(StartupFailure::NoExecutable)
        .add_plugins(RuntimeCorePlugin);
    app.update();

    assert!(
        matches!(
            app.world().get_resource::<StartupFailure>(),
            Some(StartupFailure::NoExecutable)
        ),
        "the recorded reason must not be replaced by a pack load"
    );
    assert!(app.world().get_resource::<Runtime>().is_none());
}

/// The no-executable screen draws the one shared message and no pack path, so the
/// player sees the same text a terminal does.
#[test]
fn the_no_executable_screen_draws_the_shared_message_without_a_path() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    attach_text_faces(&mut app);
    app.insert_resource(StartupFailure::NoExecutable);
    app.world_mut()
        .run_system_once(crate::presentation::play::draw_error_screen)
        .expect("draw_error_screen runs");

    let mut texts: Vec<String> = app
        .world()
        .iter_entities()
        .filter_map(|entity| entity.get::<Text>().map(|text| text.0.clone()))
        .collect();
    texts.sort();
    assert!(
        texts.contains(&"unmei-play could not start:".to_string()),
        "the intro is drawn: {texts:?}"
    );
    let body = texts
        .iter()
        .find(|text| text.contains(crate::launch::NO_EXECUTABLE_MESSAGE))
        .expect("the shared message reaches the screen");
    assert!(!body.contains(".log"), "no log pointer: {body:?}");
    assert!(
        !body.contains("unmei could not load the pack:"),
        "no pack path or pack wording: {body:?}"
    );
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

/// The running app's read-log sibling path.
fn read_log_path(app: &App) -> PathBuf {
    let runtime = app.world().resource::<Runtime>();
    unmei_engine::read_log::read_log_path_for(runtime.runner.pack())
}

/// The in-memory read-log state.
fn read_state(app: &App) -> &crate::read_log::ReadLogState {
    app.world().resource::<crate::read_log::ReadLogState>()
}

/// The log decoded from disk, or `None` when no file exists.
fn read_log_on_disk(app: &App) -> Option<unmei_engine::read_log::ReadLog> {
    let path = read_log_path(app);
    path.exists()
        .then(|| unmei_engine::read_log::load_read_log(&path).0)
}

/// Seed the sibling file with a hand-built log before the app boots.
fn seed_read_log(pack_root: &Path, log: &unmei_engine::read_log::ReadLog) {
    let mut pack = load_pack(pack_root).expect("pack loads").pack;
    pack.root = pack_root.to_path_buf();
    let path = unmei_engine::read_log::read_log_path_for(&pack);
    unmei_engine::read_log::write_read_log(&path, log).unwrap();
}

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

/// Whether the seen skip is engaged.
fn skip_on(app: &App) -> bool {
    app.world().resource::<crate::core::SkipState>().is_on()
}

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

/// The one entity carrying component `T`.
fn entity_with<T: bevy::prelude::Component>(app: &App) -> Entity {
    app.world()
        .iter_entities()
        .find_map(|entity| entity.get::<T>().map(|_| entity.id()))
        .expect("the component exists")
}

/// Replace the demo pack's entry scene with a say-only script. The first say
/// sits on source line 2 (line 1 is the leading `bg`).
fn pack_with_entry_says(says: &[&str]) -> tempfile::TempDir {
    let pack = temp_pack();
    let mut script = String::from("bg school_gate\n");
    for say in says {
        script.push_str(say);
        script.push('\n');
    }
    std::fs::write(pack.path().join("pack/scenes/first_meeting.unmei"), script).unwrap();
    pack
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

/// Seed every say-line of `scene_id` as arrival-read.
fn seed_scene_read(pack_root: &Path, scene_id: &str) {
    let pack = load_pack(pack_root).expect("pack loads").pack;
    let scene = pack.scenes.get(scene_id).expect("scene exists");
    let mut lines = std::collections::BTreeSet::new();
    for (index, line) in scene.lines.iter().enumerate() {
        if matches!(line, unmei_engine::content::Line::Say { .. }) {
            lines.insert(scene.source_line(index));
        }
    }
    let mut log = unmei_engine::read_log::ReadLog::default();
    log.mark_visited(scene_id);
    for line in lines {
        log.mark_read(scene_id, line);
    }
    seed_read_log(pack_root, &log);
}

/// Boot into play with every entry-scene say-line already known.
fn app_with_known_entry(pack: &tempfile::TempDir) -> App {
    let pack_root = pack.path().join("pack");
    seed_scene_read(&pack_root, "first_meeting");
    app_for_pack(pack_root)
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

/// Whether the force hold is effective this frame.
fn skip_force(app: &App) -> bool {
    app.world()
        .resource::<crate::core::SkipState>()
        .is_force_active()
}

/// A booted app whose key state comes from the real [`bevy::input::InputPlugin`]:
/// a held key is a genuine multi-frame `ButtonInput` hold built from
/// `KeyboardInput` press/release messages, not a `press_and_update`/`reset_all`
/// simulation. `Time` is advanced only through [`advance_time`], so the dwell is
/// deterministic.
fn boot_app_with_input(pack_root: PathBuf, decisions: Option<Arc<dyn DecisionResolver>>) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(bevy::input::InputPlugin)
        .insert_resource(RuntimeConfig::new(pack_root, decisions).with_dev_mode(true))
        .add_plugins(RuntimeCorePlugin);
    attach_text_faces(&mut app);
    app.update(); // Startup: load the pack, then show the title
                  // Zero the real boot delta so every later `step_input` frame is controlled
                  // only through `advance_time`.
    advance_time(&mut app, 0.0);
    app
}

fn send_key_message(app: &mut App, key: KeyCode, state: bevy::input::ButtonState, repeat: bool) {
    use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
    app.world_mut().write_message(KeyboardInput {
        key_code: key,
        logical_key: Key::Unidentified(NativeKey::Unidentified),
        state,
        text: None,
        repeat,
        window: Entity::PLACEHOLDER,
    });
}

pub(crate) fn press_key(app: &mut App, key: KeyCode) {
    send_key_message(app, key, bevy::input::ButtonState::Pressed, false);
}

fn release_key(app: &mut App, key: KeyCode) {
    send_key_message(app, key, bevy::input::ButtonState::Released, false);
}

/// One production-faithful frame: the real `PreUpdate` keyboard system settles
/// `ButtonInput`, then the app's `Update` schedule runs. Deliberately not
/// `app.update()`, which would let the real `TimePlugin` overwrite the test's
/// simulated delta.
pub(crate) fn step_input(app: &mut App) {
    app.world_mut().run_schedule(PreUpdate);
    app.world_mut().run_schedule(Update);
}

/// A tap through the real keyboard chain: press, one frame, release, one frame.
pub(crate) fn tap_key(app: &mut App, key: KeyCode) {
    press_key(app, key);
    step_input(app);
    release_key(app, key);
    step_input(app);
}

/// Cross the title's New Game through `KeyboardInput` messages.
pub(crate) fn enter_play_with_input(app: &mut App) {
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleNewGame;
    press_key(app, KeyCode::Enter);
    step_input(app);
    assert!(!menu(app).open, "input enter_play must start play");
    release_key(app, KeyCode::Enter);
    step_input(app);
}

/// Click a control and run one production-faithful frame.
fn click_input(app: &mut App, action: MenuAction) {
    app.world_mut().spawn((action, Interaction::Pressed));
    step_input(app);
}

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

/// A copied demo pack (so `state.ron`, `npcs/` and assets survive) whose scene
/// files and manifest are replaced by the given scenes. `entry` need not be in
/// `order`, so an unlisted entry can be the only visited scene.
fn temp_custom_pack(
    scenes: &[(&str, &str)],
    order: Option<&[&str]>,
    entry: &str,
) -> tempfile::TempDir {
    let dir = temp_pack();
    let root = dir.path().join("pack");
    for scene in std::fs::read_dir(root.join("scenes")).unwrap() {
        std::fs::remove_file(scene.unwrap().path()).unwrap();
    }
    for (id, body) in scenes {
        std::fs::write(root.join(format!("scenes/{id}.unmei")), body).unwrap();
    }
    let scenes_field = match order {
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
        format!("(\n    name: \"timeline test\",\n    entry: \"{entry}\",\n{scenes_field})\n"),
    )
    .unwrap();
    dir
}

/// Seed the sibling read log with the given visited scenes before boot.
fn seed_visited(pack_root: &Path, visited: &[&str]) {
    let mut log = unmei_engine::read_log::ReadLog::default();
    for id in visited {
        log.mark_visited(id);
    }
    seed_read_log(pack_root, &log);
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

// ---------------------------------------------------------------------------
// App-input ownership seam
// ---------------------------------------------------------------------------
//
// The seam is one resource (`InputOwnership`) read by `input_unclaimed`. These
// tests drive the real Bevy input/focus/text/clipboard pipeline end to end —
// `KeyboardInput` messages through focus dispatch, `EditableText.pending_edits`
// through the PostUpdate text apply, a propagated `Pointer<Press>` observer and
// a Settings `Interaction` press — rather than a reset-all `ButtonInput` flag,
// so a claim is proven to suppress (and release to restore) the production
// paths, not a mock.

/// A production-shaped full-frame app: the real Bevy input, focus dispatch,
/// clipboard and editable-text plugins over [`RuntimeCorePlugin`], plus the
/// app's own queued-edit drain, outside-press observer and Settings drag. The
/// returned `Entity` is the primary window for `KeyboardInput` messages.
pub(crate) fn boot_widget_input_app(pack_root: PathBuf) -> (App, Entity) {
    use bevy::clipboard::ClipboardPlugin;
    use bevy::input_focus::{InputDispatchPlugin, InputFocusPlugin};
    use bevy::picking::events::{Pointer, Release};
    use bevy::text::{apply_text_edits, FontCx, LayoutCx};
    use bevy::ui::UiScale;
    use bevy::ui_widgets::EditableTextInputPlugin;
    use bevy::window::Ime;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(bevy::input::InputPlugin)
        .add_plugins(InputFocusPlugin)
        .add_plugins(InputDispatchPlugin)
        .add_plugins(ClipboardPlugin)
        .add_plugins(EditableTextInputPlugin)
        .init_resource::<ClipboardPort>()
        .init_resource::<FontCx>()
        .init_resource::<LayoutCx>()
        .init_resource::<UiScale>()
        .add_message::<Ime>()
        .add_message::<Pointer<Release>>()
        .insert_resource(RuntimeConfig::new(pack_root, None).with_dev_mode(true))
        .add_plugins(RuntimeCorePlugin)
        .add_observer(crate::presentation::settings_view::discard_key_edit_on_outside_press)
        .add_systems(
            PostUpdate,
            apply_text_edits.in_set(bevy::text::EditableTextSystems),
        );
    // The production ownership gates (Settings drag, trimmed paste, the
    // queued-edit drain), registered by the same function `UnmeiPlugin` uses, so
    // this harness cannot drift from the shipped wiring.
    crate::register_ownership_gated_presentation(&mut app);
    attach_text_faces(&mut app);
    let window = app
        .world_mut()
        .spawn((Window::default(), PrimaryWindow))
        .id();
    app.update(); // Startup: load the pack, then show the title
    (app, window)
}

/// Spawn a focused editable key field, the production `KeyField` widget shape.
fn spawn_key_field(app: &mut App) -> Entity {
    use bevy::input_focus::{FocusCause, InputFocus};
    use bevy::text::EditableTextFilter;

    let field = app
        .world_mut()
        .spawn((
            crate::credentials::KeyField,
            EditableText::new(""),
            Node::default(),
            TextFont::default(),
            TextColor::default(),
            EditableTextFilter::new(|character| character != '\n' && character != '\r'),
        ))
        .id();
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(field, FocusCause::Navigated);
    field
}

/// The focused field's committed text, read from the entity under test rather
/// than by scanning for a built `KeyField`.
fn field_value(app: &App, field: Entity) -> String {
    app.world()
        .entity(field)
        .get::<EditableText>()
        .expect("the field is an EditableText")
        .value()
        .to_string()
}

/// Type one character through the real keyboard chain and run a full frame,
/// then release it on the next. The press and release are separate frames so an
/// `EditableText` observer sees them exactly as production would.
fn type_character(app: &mut App, window: Entity, character: char) {
    use bevy::input::keyboard::{Key, KeyboardInput};
    use bevy::input::ButtonState;

    for (state, text) in [
        (ButtonState::Pressed, Some(character.to_string())),
        (ButtonState::Released, None),
    ] {
        app.world_mut().write_message(KeyboardInput {
            key_code: KeyCode::KeyA,
            logical_key: Key::Character(character.to_string().into()),
            state,
            text: text.map(Into::into),
            repeat: false,
            window,
        });
        app.update();
    }
}

/// A keystroke that carries a real logical key (not `Unidentified`), so the
/// production `EditableText` widget's navigation/selection arms see it. The
/// shared `send_key_message` deliberately writes an unidentified logical key for
/// the app's own key-code routing; a caret/selection witness needs the widget's
/// own mapping instead.
fn send_logical_key(
    app: &mut App,
    window: Entity,
    logical: bevy::input::keyboard::Key,
    code: KeyCode,
    state: bevy::input::ButtonState,
) {
    use bevy::input::keyboard::KeyboardInput;
    app.world_mut().write_message(KeyboardInput {
        key_code: code,
        logical_key: logical,
        state,
        text: None,
        repeat: false,
        window,
    });
    app.update();
}

/// Extend the field's selection `count` characters left of its caret through the
/// **real** held-`Shift` + arrow path the production widget consumes.
fn select_left(app: &mut App, window: Entity, count: usize) {
    use bevy::input::keyboard::Key;
    app.world_mut()
        .resource_mut::<ButtonInput<Key>>()
        .press(Key::Shift);
    for _ in 0..count {
        send_logical_key(
            app,
            window,
            Key::ArrowLeft,
            KeyCode::ArrowLeft,
            bevy::input::ButtonState::Pressed,
        );
        send_logical_key(
            app,
            window,
            Key::ArrowLeft,
            KeyCode::ArrowLeft,
            bevy::input::ButtonState::Released,
        );
    }
    app.world_mut()
        .resource_mut::<ButtonInput<Key>>()
        .release(Key::Shift);
    app.update();
}

/// The field's `(caret byte index, selected byte range)` read from its own
/// `parley` editor, so a witness can pin a nontrivial caret and selection.
fn field_caret_and_selection(app: &App, field: Entity) -> (usize, std::ops::Range<usize>) {
    let editable = app
        .world()
        .entity(field)
        .get::<EditableText>()
        .expect("the field is an EditableText");
    let selection = editable.editor().raw_selection();
    (selection.focus().index(), selection.text_range())
}

/// Fire a propagated `Pointer<Press>` at `target` through the real observer
/// path and run a full frame.
fn outside_press(app: &mut App, target: Entity) {
    use bevy::camera::NormalizedRenderTarget;
    use bevy::picking::backend::HitData;
    use bevy::picking::events::{Pointer, Press};
    use bevy::picking::pointer::{Location, PointerButton, PointerId};

    let event = Pointer::new(
        PointerId::Mouse,
        Location {
            target: NormalizedRenderTarget::None {
                width: 1280,
                height: 720,
            },
            position: Vec2::ZERO,
        },
        Press {
            button: PointerButton::Primary,
            hit: HitData {
                camera: Entity::PLACEHOLDER,
                depth: 0.0,
                position: None,
                normal: None,
                extra: None,
            },
            count: 1,
        },
        target,
    );
    app.world_mut()
        .entity_mut(target)
        .trigger(|_| event.clone());
    app.update();
}

#[test]
fn a_claimed_frame_suppresses_play_shortcuts_and_queued_text() {
    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let field = spawn_key_field(&mut app);
    let parked = wait(&app);
    assert!(parked.is_some(), "in play on a say-line");
    assert!(!manual_path(&app).exists());

    // Claim: the play path, the F5 shortcut and the focused-text path are all
    // frozen for the frame.
    app.world_mut()
        .resource_mut::<InputOwnership>()
        .claim(InputClaim("seam-test"));
    type_character(&mut app, window, 'k');
    tap_key(&mut app, KeyCode::F5);
    tap_key(&mut app, KeyCode::Space);

    assert_eq!(field_value(&app, field), "", "claimed typing did not apply");
    assert_eq!(wait(&app), parked, "claimed Space did not advance");
    assert!(!manual_path(&app).exists(), "claimed F5 did not save");

    // Release: ordinary input works again, and the claimed keystroke does not
    // replay — one character, not two.
    app.world_mut().resource_mut::<InputOwnership>().release();
    type_character(&mut app, window, 'k');
    assert_eq!(
        field_value(&app, field),
        "k",
        "release restores ordinary editing without replaying the claimed press"
    );
    tap_key(&mut app, KeyCode::Space);
    assert_ne!(wait(&app), parked, "release restores the advance");
}

#[test]
fn a_claimed_frame_freezes_menu_navigation() {
    let pack = temp_pack();
    let (mut app, _window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open, "the overlay opens unclaimed");
    let focused = menu(&app).focus;

    app.world_mut()
        .resource_mut::<InputOwnership>()
        .claim(InputClaim("seam-test"));
    tap_key(&mut app, KeyCode::ArrowRight);
    assert_eq!(
        menu(&app).focus,
        focused,
        "claimed navigation did not move focus"
    );

    app.world_mut().resource_mut::<InputOwnership>().release();
    tap_key(&mut app, KeyCode::ArrowRight);
    assert_ne!(
        menu(&app).focus,
        focused,
        "release restores menu navigation"
    );
}

#[test]
fn a_claimed_frame_blocks_the_outside_press_discard() {
    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let field = spawn_key_field(&mut app);
    type_character(&mut app, window, 'k');
    assert_eq!(field_value(&app, field), "k");
    app.world_mut().resource_mut::<MenuState>().open = true;
    app.world_mut().resource_mut::<MenuState>().view = MenuView::Settings;
    assert!(menu(&app).key_editing, "the focused field is being edited");
    let outside = app.world_mut().spawn_empty().id();

    // A claimed outside press leaves the draft and edit mode alone.
    app.world_mut()
        .resource_mut::<InputOwnership>()
        .claim(InputClaim("seam-test"));
    outside_press(&mut app, outside);
    assert_eq!(
        field_value(&app, field),
        "k",
        "claimed press did not discard"
    );
    assert!(
        menu(&app).key_editing,
        "claimed press did not leave the field"
    );

    // Release: the ordinary outside press discards the draft.
    app.world_mut().resource_mut::<InputOwnership>().release();
    outside_press(&mut app, outside);
    assert_eq!(field_value(&app, field), "", "release restores the discard");
    assert!(!menu(&app).key_editing);
}

#[test]
fn a_claimed_frame_freezes_the_settings_drag_start() {
    let pack = temp_pack();
    let (mut app, _window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);

    // Claimed: a fresh pressed interaction starts no drag.
    app.world_mut()
        .resource_mut::<InputOwnership>()
        .claim(InputClaim("seam-test"));
    app.world_mut()
        .spawn((MenuAction::TextSpeed, Interaction::Pressed));
    step_input(&mut app);
    assert_eq!(
        app.world()
            .resource::<crate::presentation::settings_view::SettingsDrag>()
            .control(),
        None,
        "claimed press started no Settings drag"
    );

    // Released: a button held across the claim transition is still
    // quarantined, so it cannot resume the drag by itself.
    app.world_mut().resource_mut::<InputOwnership>().release();
    app.world_mut()
        .spawn((MenuAction::TextSpeed, Interaction::Pressed));
    step_input(&mut app);
    assert_eq!(
        app.world()
            .resource::<crate::presentation::settings_view::SettingsDrag>()
            .control(),
        None,
        "a held pointer cannot resume the Settings drag after release"
    );

    // Releasing and pressing again is the fresh pointer the contract requires.
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .release(MouseButton::Left);
    step_input(&mut app);
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.world_mut()
        .spawn((MenuAction::TextSpeed, Interaction::Pressed));
    step_input(&mut app);
    assert_eq!(
        app.world()
            .resource::<crate::presentation::settings_view::SettingsDrag>()
            .control(),
        Some(crate::settings::TrackControl::TextSpeed),
        "release and a fresh press restore the Settings drag start"
    );
}

#[test]
fn a_claimed_frame_keeps_a_prearmed_force_hold_live_and_mirrors_its_release() {
    let pack = pack_with_entry_says(&[
        "Narrator: One.",
        "Narrator: Two.",
        "Narrator: Three.",
        "Narrator: Four.",
    ]);
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);

    // Arm the force hold while the app owns input.
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app), "a held Ctrl armed the force hold");

    // A claim must not freeze the already-armed hold: it keeps stepping.
    app.world_mut()
        .resource_mut::<InputOwnership>()
        .claim(InputClaim("seam-test"));
    let parked = wait(&app);
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert!(skip_force(&app), "the pre-armed hold stays active");
    assert_ne!(
        wait(&app),
        parked,
        "a claimed frame still steps an armed force hold"
    );

    // Releasing the physical key clears the latch even while claimed.
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(!skip_force(&app), "the release mirror ran under the claim");

    // A Ctrl first pressed while claimed cannot arm.
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(!skip_force(&app), "a claimed press never arms");
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
}

/// A test-only request that the resolution-point system take app input.
#[derive(Resource, Default)]
struct ClaimOnResolve(bool);

fn claim_on_resolve(flag: Res<ClaimOnResolve>, mut ownership: ResMut<InputOwnership>) {
    if flag.0 {
        ownership.claim(InputClaim("seam-resolve"));
    }
}

/// Paste through the real keyboard chain and the in-process clipboard, running
/// a full frame for the press and the release.
fn paste(app: &mut App, window: Entity) {
    use bevy::input::keyboard::{Key, KeyboardInput};
    use bevy::input::ButtonState;

    app.world_mut()
        .resource_mut::<ButtonInput<Key>>()
        .press(Key::Control);
    for (state, text) in [
        (ButtonState::Pressed, Some("v".to_string())),
        (ButtonState::Released, None),
    ] {
        app.world_mut().write_message(KeyboardInput {
            key_code: KeyCode::KeyV,
            logical_key: Key::Character("v".into()),
            state,
            text: text.map(Into::into),
            repeat: false,
            window,
        });
        app.update();
    }
    app.world_mut()
        .resource_mut::<ButtonInput<Key>>()
        .release(Key::Control);
    app.update();
}

#[test]
fn a_claim_resolved_in_preupdate_covers_the_same_frame() {
    let pack = temp_pack();
    let (mut app, _window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    // A claimant scheduled at the resolution point takes app input before the
    // frame's physical tracking and app actions run.
    app.insert_resource(ClaimOnResolve(true));
    app.add_systems(
        PreUpdate,
        claim_on_resolve.in_set(crate::input::InputOwnershipSet),
    );

    tap_key(&mut app, KeyCode::F5);
    assert!(
        !manual_path(&app).exists(),
        "a claim resolved in PreUpdate suppressed the same frame's F5"
    );
}

#[test]
fn a_claimed_frame_suppresses_a_queued_paste() {
    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let field = spawn_key_field(&mut app);
    *app.world_mut().resource_mut::<ClipboardPort>() =
        ClipboardPort::Memory(Some("sk-pasted\n".into()));

    app.world_mut()
        .resource_mut::<InputOwnership>()
        .claim(InputClaim("seam-test"));
    paste(&mut app, window);
    assert_eq!(field_value(&app, field), "", "claimed paste did not apply");

    app.world_mut().resource_mut::<InputOwnership>().release();
    paste(&mut app, window);
    assert_eq!(
        field_value(&app, field),
        "sk-pasted",
        "release restores the trimmed paste without replaying the claimed one"
    );
}

#[test]
fn a_claimed_frame_blocks_the_startup_error_quit() {
    let (mut app, _window) = boot_widget_input_app(PathBuf::from("definitely/not/a/pack"));
    assert!(
        app.world().get_resource::<StartupFailure>().is_some(),
        "the broken pack is recorded"
    );
    assert_eq!(app.should_exit(), None);

    app.world_mut()
        .resource_mut::<InputOwnership>()
        .claim(InputClaim("seam-test"));
    tap_key(&mut app, KeyCode::Enter);
    assert_eq!(
        app.should_exit(),
        None,
        "a claimed Enter did not quit the error screen"
    );

    app.world_mut().resource_mut::<InputOwnership>().release();
    tap_key(&mut app, KeyCode::Enter);
    assert!(
        matches!(app.should_exit(), Some(AppExit::Error(_))),
        "release restores the startup-error quit"
    );
}

#[test]
fn a_claimed_frame_ignores_an_os_repeat_and_needs_a_fresh_press() {
    let pack = temp_pack();
    let (mut app, _window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);

    // A held F5 arriving as an OS repeat is not a fresh action.
    app.world_mut()
        .resource_mut::<InputOwnership>()
        .claim(InputClaim("seam-test"));
    send_key_message(
        &mut app,
        KeyCode::F5,
        bevy::input::ButtonState::Pressed,
        true,
    );
    step_input(&mut app);
    assert!(
        !manual_path(&app).exists(),
        "a repeat press did not save while claimed"
    );

    // Release the claim with the key still physically down: the held repeat is
    // not a fresh action.
    app.world_mut().resource_mut::<InputOwnership>().release();
    send_key_message(
        &mut app,
        KeyCode::F5,
        bevy::input::ButtonState::Pressed,
        true,
    );
    step_input(&mut app);
    assert!(
        !manual_path(&app).exists(),
        "a held repeat did not save after release"
    );

    // Release and a fresh press does act.
    release_key(&mut app, KeyCode::F5);
    step_input(&mut app);
    press_key(&mut app, KeyCode::F5);
    step_input(&mut app);
    assert!(manual_path(&app).exists(), "a fresh press saves");
}

#[test]
fn a_claimed_frame_keeps_the_draft_through_focus_loss() {
    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let field = spawn_key_field(&mut app);
    type_character(&mut app, window, 'k');
    assert_eq!(field_value(&app, field), "k");
    assert!(menu(&app).key_editing, "the focused field is being edited");

    // Claim, then move focus off the field: engine-level focus dispatch is not
    // gated, so the focused-editing sync is frozen and the drain must consume
    // the focus-loss collapse edit the observer queues in PostUpdate. That
    // producer runs after the drain in `bevy_ui`'s Content/Layout order, so the
    // queued edit is consumed on the next blocked frame, before `apply_text_edits`
    // could ever run it.
    app.world_mut()
        .resource_mut::<InputOwnership>()
        .claim(InputClaim("seam-test"));
    app.world_mut()
        .resource_mut::<bevy::input_focus::InputFocus>()
        .clear();
    app.update();
    assert_eq!(
        field_value(&app, field),
        "k",
        "a claimed focus loss kept the draft"
    );
    app.update();
    assert!(
        app.world()
            .entity(field)
            .get::<EditableText>()
            .expect("EditableText")
            .pending_edits
            .is_empty(),
        "the claim consumed the queued collapse edit"
    );
    assert!(menu(&app).key_editing);
}

// ---------------------------------------------------------------------------
// Monitor drawer
// ---------------------------------------------------------------------------
//
// The F3 drawer's safety is proven through the production-shaped harnesses
// above wherever a key, pointer or text path is involved:
// `boot_widget_input_app` runs the real `InputPlugin`, focus dispatch,
// `EditableText` observers, clipboard port and propagated pointer observers over
// `RuntimeCorePlugin`; `boot_app_with_input` gives a held key a genuine
// multi-frame `ButtonInput` life. Projection, budget and layout checks use the
// real plugin systems over a deterministically seeded retained history.

use crate::drawer::{
    self, MonitorControl, MonitorRow, MonitorRowPart, MonitorStatusText, MonitorView, MonitorWork,
    DISPLAY_ROWS, MONITOR_CLAIM, NEWEST_INSET, NEWEST_WASH, PROJECTION_SCAN_BUDGET,
    TEXT_WORK_BUDGET,
};
use crate::input::InputQuarantine;
use crate::monitor::{MonitorHistory, MonitorLine};

fn drawer_panel(app: &App) -> &drawer::MonitorPanel {
    app.world()
        .get_resource::<drawer::MonitorPanel>()
        .expect("dev mode installs the drawer state")
}

pub(crate) fn drawer_open(app: &App) -> bool {
    drawer_panel(app).is_open()
}

pub(crate) fn drawer_view(app: &App) -> MonitorView {
    drawer_panel(app).view()
}

fn drawer_focus(app: &App) -> MonitorControl {
    drawer_panel(app).focus()
}

fn drawer_claim_held(app: &App) -> bool {
    app.world().resource::<InputOwnership>().claimed_by() == Some(MONITOR_CLAIM)
}

fn drawer_roots(app: &mut App) -> usize {
    let world = app.world_mut();
    let mut query = world.query_filtered::<Entity, With<drawer::MonitorRoot>>();
    query.iter(world).count()
}

fn drawer_control_entity(app: &mut App, control: MonitorControl) -> Entity {
    let world = app.world_mut();
    let mut query = world.query::<(Entity, &drawer::MonitorControlTag)>();
    query
        .iter(world)
        .find(|(_, tag)| tag.0 == control)
        .map(|(entity, _)| entity)
        .expect("the drawer control exists")
}

/// Activate a control through the component the production `Interaction` write
/// lands on. Real cursor hit-testing belongs to the rendered walk; this
/// drives the same activation branch without a UI camera.
pub(crate) fn activate_control(app: &mut App, control: MonitorControl) {
    let entity = drawer_control_entity(app, control);
    app.world_mut()
        .entity_mut(entity)
        .insert(Interaction::Pressed);
    step_input(app);
    // `Close` despawns the tree on that frame; the release is only meaningful
    // while the control still exists.
    if let Ok(mut control) = app.world_mut().get_entity_mut(entity) {
        control.insert(Interaction::None);
    }
    step_input(app);
}

/// Pulse a key in a world without `bevy::input::InputPlugin` (the fixture apps
/// that drive `ButtonInput` directly). `just_pressed` is not cleared by the
/// engine there, so each frame clears it explicitly; production always has the
/// real input plugin, which the other harnesses use.
fn tap_key_simulated(app: &mut App, key: KeyCode) {
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .clear_just_pressed(key);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .release(key);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .clear_just_pressed(key);
}

fn test_monitor_line(sequence: u64, target: &str, level: log::Level, message: &str) -> MonitorLine {
    let text = format!("{sequence:010}ms {level:<5} {target} {message}");
    let body_start = text.len() - message.len();
    MonitorLine {
        sequence,
        elapsed_ms: sequence,
        level,
        target: target.to_string(),
        target_display: target.to_string(),
        text,
        body_start,
        truncated: false,
    }
}

fn seed_monitor_history(app: &mut App, lines: Vec<MonitorLine>) {
    app.world_mut().init_resource::<MonitorHistory>();
    app.world_mut()
        .resource_mut::<MonitorHistory>()
        .seed_lines(lines);
}

/// Every materialized row's part texts, oldest row first — the display the
/// developer actually sees.
fn drawer_rows(app: &mut App) -> Vec<(u64, MonitorRowPart, String)> {
    let world = app.world_mut();
    let rows: Vec<(u64, Entity)> = {
        let mut query = world.query::<(Entity, &MonitorRow)>();
        query.iter(world).map(|(e, row)| (row.0, e)).collect()
    };
    let parts: Vec<(Entity, MonitorRowPart, String)> = {
        let mut query = world.query::<(Entity, &MonitorRowPart, &Text)>();
        query
            .iter(world)
            .map(|(e, part, text)| (e, *part, text.as_str().to_string()))
            .collect()
    };
    let children: Vec<(Entity, Entity)> = {
        let mut query = world.query::<(Entity, &ChildOf)>();
        query
            .iter(world)
            .map(|(child, parent)| (parent.parent(), child))
            .collect()
    };
    let mut out = Vec::new();
    for (sequence, entity) in rows {
        for (part_entity, part, text) in &parts {
            if children
                .iter()
                .any(|(parent, child)| *parent == entity && child == part_entity)
            {
                out.push((sequence, *part, text.clone()));
            }
        }
    }
    out.sort_by_key(|(sequence, part, _)| (*sequence, *part as u8));
    out
}

fn drawer_status(app: &mut App) -> String {
    let world = app.world_mut();
    let mut query = world.query_filtered::<&Text, With<MonitorStatusText>>();
    query
        .iter(world)
        .map(|text| text.as_str().to_string())
        .next()
        .expect("the drawer status line exists")
}

fn has_component<T: Component>(app: &mut App) -> bool {
    let world = app.world_mut();
    let mut query = world.query_filtered::<Entity, With<T>>();
    query.iter(world).next().is_some()
}

/// A row's `(background, left border)` marker colors, by admission sequence.
fn row_marker(app: &mut App, sequence: u64) -> Option<(Color, Color)> {
    let world = app.world_mut();
    let mut query = world.query::<(&MonitorRow, &BackgroundColor, &BorderColor)>();
    query
        .iter(world)
        .find(|(row, _, _)| row.0 == sequence)
        .map(|(_, background, border)| (background.0, border.left))
}

/// Boot dev mode off and prove no Monitor drawer state, quarantine or input
/// system was installed at all.
#[test]
fn dev_mode_off_installs_no_drawer_and_f3_does_nothing() {
    let pack = temp_pack();
    let mut app = boot_app_unstarted_dev(pack.path().join("pack"), None, false);
    app.update();
    assert!(
        app.world().get_resource::<drawer::MonitorPanel>().is_none(),
        "off installs no drawer state"
    );
    assert!(
        app.world().get_resource::<InputQuarantine>().is_none(),
        "off installs no held-input quarantine"
    );
    assert_eq!(drawer_roots(&mut app), 0);

    press_and_update(&mut app, KeyCode::F3);
    assert!(
        app.world().get_resource::<drawer::MonitorPanel>().is_none(),
        "F3 does no Monitor work when the drawer is not installed"
    );
    assert_eq!(drawer_roots(&mut app), 0, "no panel input or UI work");
}

/// Enabled launches start closed, and F3 reaches ordinary play.
#[test]
fn dev_enabled_starts_closed_and_f3_opens_over_play() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    assert!(!drawer_open(&app), "an enabled launch starts closed");

    enter_play_with_input(&mut app);
    let parked = wait(&app);
    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app), "F3 opens the drawer over play");
    assert!(drawer_claim_held(&app), "the drawer owns app input");
    assert_eq!(drawer_roots(&mut app), 1, "one drawer tree");
    assert_eq!(
        drawer_focus(&app),
        MonitorControl::ShowEverything,
        "the first open focuses Show everything"
    );
    assert_eq!(wait(&app), parked, "opening is not a runtime action");
}

/// F3 reaches the title, the overlay, the startup error screen and a defensive
/// hold — the drawer never depends on a loaded `Runtime` or a closed menu.
#[test]
fn f3_reaches_every_surface() {
    // Title.
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app), "F3 opens over the title");
    tap_key(&mut app, KeyCode::F3);
    assert!(!drawer_open(&app), "F3 closes over the title");

    // Esc menu.
    enter_play_with_input(&mut app);
    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open, "the overlay is up");
    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app), "F3 opens over the overlay");
    tap_key(&mut app, KeyCode::Escape);
    assert!(
        !drawer_open(&app) && menu(&app).open,
        "Esc dismisses only Monitor"
    );

    // Startup error screen, no Runtime.
    let (mut broken, _window) = boot_widget_input_app(PathBuf::from("definitely/not/a/pack"));
    assert!(broken.world().get_resource::<StartupFailure>().is_some());
    tap_key(&mut broken, KeyCode::F3);
    assert!(
        drawer_open(&broken),
        "F3 opens over the startup error screen"
    );

    // Defensive hold.
    let mut held_app = app_with_missing_decide_npc(&FakeResolver::new(model_answer("warm")));
    held_app.update();
    assert!(on_hold(&held_app), "the runner is on a defensive hold");
    tap_key_simulated(&mut held_app, KeyCode::F3);
    assert!(drawer_open(&held_app), "F3 opens under a defensive hold");
}

/// Esc closes only Monitor: it does not open the overlay, advance a line or
/// quit the error screen. A fresh Esc afterwards still acts underneath.
#[test]
fn esc_dismisses_only_monitor_and_the_next_fresh_esc_acts() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    enter_play_with_input(&mut app);
    let parked = wait(&app);

    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app));
    tap_key(&mut app, KeyCode::Escape);
    assert!(!drawer_open(&app), "Esc closes Monitor");
    assert!(!menu(&app).open, "Esc did not also open the overlay");
    assert_eq!(wait(&app), parked, "Esc did not also advance play");

    // A fresh Esc after the release is the ordinary overlay key again.
    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open, "a fresh Esc opens the overlay");
}

/// F3 wins the overlap: closed F3+Esc opens once, open F3+Esc closes once.
#[test]
fn f3_and_esc_on_one_frame_toggle_once() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    enter_play_with_input(&mut app);

    press_key(&mut app, KeyCode::F3);
    press_key(&mut app, KeyCode::Escape);
    step_input(&mut app);
    assert!(drawer_open(&app), "closed F3+Esc opens once");
    release_key(&mut app, KeyCode::F3);
    release_key(&mut app, KeyCode::Escape);
    step_input(&mut app);
    assert!(
        drawer_open(&app),
        "the same-frame overlap did not close it again"
    );
    assert!(!menu(&app).open, "Esc did not leak into the overlay");

    // Now open, F3+Esc closes once.
    press_key(&mut app, KeyCode::F3);
    press_key(&mut app, KeyCode::Escape);
    step_input(&mut app);
    assert!(!drawer_open(&app), "open F3+Esc closes once");
    release_key(&mut app, KeyCode::F3);
    release_key(&mut app, KeyCode::Escape);
    step_input(&mut app);
    assert!(!drawer_open(&app), "the closing overlap did not reopen it");
}

/// Opening with save/advance on the same frame cannot act underneath, and the
/// quarantined shortcut never fires after release.
#[test]
fn f3_opening_with_save_and_advance_does_not_act_underneath() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    enter_play_with_input(&mut app);
    let parked = wait(&app);
    assert!(!manual_path(&app).exists());

    press_key(&mut app, KeyCode::F3);
    press_key(&mut app, KeyCode::Space);
    press_key(&mut app, KeyCode::F5);
    step_input(&mut app);

    assert!(drawer_open(&app));
    assert_eq!(wait(&app), parked, "opening did not advance the say-line");
    assert!(!manual_path(&app).exists(), "opening did not save");
    release_key(&mut app, KeyCode::F3);
    release_key(&mut app, KeyCode::Space);
    release_key(&mut app, KeyCode::F5);
    step_input(&mut app);
    assert!(
        !manual_path(&app).exists(),
        "the quarantined F5 never writes after release"
    );
}

/// Closing with an activation on the same frame cannot act underneath.
#[test]
fn esc_closing_with_activation_does_not_act_underneath() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    enter_play_with_input(&mut app);
    let parked = wait(&app);

    tap_key(&mut app, KeyCode::F3);
    press_key(&mut app, KeyCode::Escape);
    press_key(&mut app, KeyCode::Enter);
    step_input(&mut app);
    assert!(!drawer_open(&app), "Esc+Enter closes Monitor");
    assert!(
        !menu(&app).open,
        "the closing frame did not open the overlay"
    );
    assert_eq!(wait(&app), parked, "the closing frame did not advance play");
    release_key(&mut app, KeyCode::Escape);
    release_key(&mut app, KeyCode::Enter);
    step_input(&mut app);
    assert_eq!(wait(&app), parked, "the quarantined Enter never advances");
}

/// Every app hotkey and play action is inert while the drawer owns the window,
/// and none of them is buffered into the release frame.
#[test]
fn underlying_hotkeys_are_inert_while_open() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);
    let parked = wait(&app);
    let settings_before = (
        app.world().resource::<SettingsState>().fullscreen(),
        app.world().resource::<SettingsState>().auto(),
    );
    assert!(!manual_path(&app).exists());

    tap_key(&mut app, KeyCode::F3);
    let focus = drawer_focus(&app);
    let skip_seen = app.world().resource::<crate::core::SkipState>().is_seen();
    for key in [
        KeyCode::Space,
        KeyCode::KeyA,
        KeyCode::F5,
        KeyCode::F9,
        KeyCode::F11,
        KeyCode::Digit1,
        KeyCode::ArrowLeft,
    ] {
        tap_key(&mut app, key);
    }

    assert!(drawer_open(&app), "nothing closed the drawer");
    assert_eq!(wait(&app), parked, "no play action leaked");
    assert!(!manual_path(&app).exists(), "F5 was inert");
    assert!(!menu(&app).open, "no navigation leaked");
    assert_eq!(
        drawer_focus(&app),
        focus,
        "unrelated keys move no Monitor control"
    );
    let settings_after = (
        app.world().resource::<SettingsState>().fullscreen(),
        app.world().resource::<SettingsState>().auto(),
    );
    assert_eq!(settings_before, settings_after, "no Settings field changed");
    assert_eq!(
        app.world().resource::<crate::core::SkipState>().is_seen(),
        skip_seen,
        "Monitor controls neither end seen skip nor toggle auto"
    );
}

/// A Decision completes and applies normally while Monitor is open over live
/// play: no new pause, no cancellation and no discarded answer.
#[test]
fn a_decision_completes_normally_while_monitor_is_open() {
    let fake = FakeResolver::new(model_answer("warm"));
    let dir = tempfile::tempdir().unwrap();
    let dst = dir.path().join("pack");
    copy_dir(&demo_pack_root(), &dst);
    let mut app = boot_app_with_input(dst, Some(Arc::new(fake.clone())));
    enter_play_with_input(&mut app);
    {
        let mut runtime = app.world_mut().resource_mut::<Runtime>();
        drive_runner_to_decision(&mut runtime);
    }
    app.update();
    pump_until(&mut app, |_| fake.calls() == 1);

    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app));
    fake.release();
    pump_until(&mut app, |app| autosave_path(app).exists());

    assert!(drawer_open(&app), "the drawer stayed open");
    assert!(
        app.world().resource::<PendingDecision>().0.is_none(),
        "the request completed, not cancelled"
    );
    assert!(!held(&app), "live play applied the answer");
    let runtime = app.world().resource::<Runtime>();
    assert!(
        matches!(runtime.runner.wait(), Some(Wait::Say { .. })),
        "the runner parked past the decision: {:?}",
        runtime.runner.wait()
    );
}

/// A Decision completed over the overlay while Monitor is open stays held after
/// Monitor alone closes; the underlying resume applies it once.
#[test]
fn closing_monitor_alone_does_not_apply_a_held_answer() {
    let fake = FakeResolver::new(model_answer("warm"));
    let dir = tempfile::tempdir().unwrap();
    let dst = dir.path().join("pack");
    copy_dir(&demo_pack_root(), &dst);
    let mut app = boot_app_with_input(dst, Some(Arc::new(fake.clone())));
    enter_play_with_input(&mut app);
    {
        let mut runtime = app.world_mut().resource_mut::<Runtime>();
        drive_runner_to_decision(&mut runtime);
    }
    app.update();
    pump_until(&mut app, |_| fake.calls() == 1);

    // Overlay up, Monitor up, then the answer lands.
    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open);
    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app));
    fake.release();
    pump_until(&mut app, held);

    tap_key(&mut app, KeyCode::F3);
    assert!(!drawer_open(&app));
    assert!(held(&app), "closing Monitor alone left the answer held");
    assert!(menu(&app).open, "the overlay pause is intact");
    assert!(!autosave_path(&app).exists(), "no write while held");

    // The ordinary overlay resume applies it, exactly once.
    tap_key(&mut app, KeyCode::Escape);
    assert!(!menu(&app).open);
    assert!(!held(&app));
    pump_until(&mut app, |app| autosave_path(app).exists());
    assert!(
        matches!(
            app.world().resource::<Runtime>().runner.wait(),
            Some(Wait::Say { .. })
        ),
        "the held answer applied on resume"
    );
}

/// Opening and closing the drawer does not release a defensive hold.
#[test]
fn monitor_does_not_release_a_defensive_hold() {
    let fake = FakeResolver::new(model_answer("warm"));
    let mut app = app_with_missing_decide_npc(&fake);
    app.update();
    assert!(on_hold(&app));

    tap_key_simulated(&mut app, KeyCode::F3);
    assert!(drawer_open(&app) && on_hold(&app));
    tap_key_simulated(&mut app, KeyCode::F3);
    assert!(!drawer_open(&app) && on_hold(&app), "the hold survived");
}

/// A `Space`/`Enter` held across the drawer transition never types into or
/// commits the suspended draft, no matter how many OS repeats arrive after
/// close; release plus a fresh press restores ordinary editing.
#[test]
fn held_space_across_close_never_types_into_the_suspended_draft() {
    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let field = spawn_key_field(&mut app);
    type_character(&mut app, window, 'k');
    assert_eq!(field_value(&app, field), "k");
    app.world_mut().resource_mut::<MenuState>().open = true;
    app.world_mut().resource_mut::<MenuState>().view = MenuView::Settings;
    assert!(menu(&app).key_editing, "the focused field is being edited");

    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app), "Monitor owns the frame");

    // Hold Space with OS repeats while the drawer is open.
    press_key(&mut app, KeyCode::Space);
    for _ in 0..4 {
        send_key_message(
            &mut app,
            KeyCode::Space,
            bevy::input::ButtonState::Pressed,
            true,
        );
        step_input(&mut app);
    }
    assert_eq!(field_value(&app, field), "k", "held Space queued no text");

    // Close with Esc while Space is still physically down.
    tap_key(&mut app, KeyCode::Escape);
    assert!(!drawer_open(&app));
    for _ in 0..4 {
        send_key_message(
            &mut app,
            KeyCode::Space,
            bevy::input::ButtonState::Pressed,
            true,
        );
        step_input(&mut app);
    }
    assert_eq!(
        field_value(&app, field),
        "k",
        "the held repeat never typed after close"
    );
    assert!(
        app.world()
            .entity(field)
            .get::<EditableText>()
            .expect("EditableText")
            .pending_edits
            .is_empty(),
        "the quarantined tail drained the queued inserts"
    );

    // Release and a fresh press is ordinary editing again.
    release_key(&mut app, KeyCode::Space);
    step_input(&mut app);
    assert!(
        !app.world()
            .get_resource::<InputQuarantine>()
            .expect("quarantine")
            .holds_key(KeyCode::Space),
        "the release ended the quarantine"
    );
    type_character(&mut app, window, 'k');
    assert_eq!(
        field_value(&app, field),
        "kk",
        "a fresh press types normally again"
    );
}

/// Held Enter across close neither commits the draft nor starts a key check.
#[test]
fn held_enter_across_close_does_not_commit_the_draft() {
    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let checker = FakeKeyCheck::new();
    install_key_check(&mut app, &checker);
    let field = spawn_key_field(&mut app);
    type_character(&mut app, window, 'k');
    app.world_mut().resource_mut::<MenuState>().open = true;
    app.world_mut().resource_mut::<MenuState>().view = MenuView::Settings;

    tap_key(&mut app, KeyCode::F3);
    press_key(&mut app, KeyCode::Enter);
    for _ in 0..3 {
        send_key_message(
            &mut app,
            KeyCode::Enter,
            bevy::input::ButtonState::Pressed,
            true,
        );
        step_input(&mut app);
    }
    tap_key(&mut app, KeyCode::Escape);
    assert!(!drawer_open(&app));
    for _ in 0..3 {
        send_key_message(
            &mut app,
            KeyCode::Enter,
            bevy::input::ButtonState::Pressed,
            true,
        );
        step_input(&mut app);
    }
    assert_eq!(field_value(&app, field), "k", "the draft survived");
    assert!(menu(&app).key_editing, "editing was not committed away");
    assert_eq!(checker.calls(), 0, "no credential check was started");

    release_key(&mut app, KeyCode::Enter);
    step_input(&mut app);
}

/// The drawer's whole safety bundle through the production widget path: a dummy
/// draft is typed, Monitor opens, every control is pointer-activated, more
/// typing is attempted, Monitor closes, and the draft is still there with caret
/// semantics intact for the next keystroke.
#[test]
fn dummy_draft_survives_open_control_activation_and_close() {
    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let field = spawn_key_field(&mut app);
    type_character(&mut app, window, 'k');
    app.world_mut().resource_mut::<MenuState>().open = true;
    app.world_mut().resource_mut::<MenuState>().view = MenuView::Settings;
    assert_eq!(field_value(&app, field), "k");

    tap_key(&mut app, KeyCode::F3);
    // Pointer-activate each control in turn (the Show everything toggle is
    // flipped back so the draft test ends on the default view).
    activate_control(&mut app, MonitorControl::ShowEverything);
    assert_eq!(drawer_view(&app), MonitorView::Everything);
    activate_control(&mut app, MonitorControl::FollowLatest);
    activate_control(&mut app, MonitorControl::ShowEverything);
    assert_eq!(drawer_view(&app), MonitorView::UnmeiOnly);
    // Typing while open is drained, not queued for later.
    type_character(&mut app, window, 'x');
    assert_eq!(
        field_value(&app, field),
        "k",
        "typing while open was blocked"
    );

    tap_key(&mut app, KeyCode::Escape);
    assert!(!drawer_open(&app));
    assert_eq!(field_value(&app, field), "k", "the draft is intact");
    assert!(menu(&app).key_editing, "editing mode survived");
    type_character(&mut app, window, 'q');
    assert_eq!(
        field_value(&app, field),
        "kq",
        "editing resumed with the draft and caret"
    );
}

/// The draft, a nontrivial caret and a selection survive real Monitor control
/// focus movement and dismissal; the restore waits for the held activation input
/// to release, and no credential check, write or diagnostic leakage is
/// Monitor-caused.
#[test]
fn draft_caret_and_selection_survive_monitor_focus_movement_and_close() {
    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let checker = FakeKeyCheck::new();
    install_key_check(&mut app, &checker);
    let field = spawn_key_field(&mut app);
    let draft = "sk-dummy-draft";
    for character in draft.chars() {
        type_character(&mut app, window, character);
    }
    app.world_mut().resource_mut::<MenuState>().open = true;
    app.world_mut().resource_mut::<MenuState>().view = MenuView::Settings;
    select_left(&mut app, window, 3);
    let seeded = field_caret_and_selection(&app, field);
    assert_eq!(field_value(&app, field), draft);
    assert_eq!(
        seeded,
        (11, 11..14),
        "a nontrivial caret and non-empty tail selection are seeded"
    );
    let row_before = app.world().resource::<CredentialsState>().row.clone();

    // Open Monitor and move focus across every control with real presses.
    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app));
    assert_eq!(drawer_focus(&app), MonitorControl::ShowEverything);
    tap_key(&mut app, KeyCode::Tab);
    tap_key(&mut app, KeyCode::Tab);
    assert_eq!(drawer_focus(&app), MonitorControl::Close);
    tap_key(&mut app, KeyCode::Tab);
    assert_eq!(
        drawer_focus(&app),
        MonitorControl::ShowEverything,
        "Tab wraps the three controls"
    );
    tap_key(&mut app, KeyCode::Tab);
    tap_key(&mut app, KeyCode::Enter);
    assert!(
        drawer_panel(&app).is_following(),
        "a real Enter activated the focused control"
    );

    // An engine-level focus loss under ownership drops InputFocus, then a held
    // Space keeps the restore quarantined through the closing frame.
    app.world_mut()
        .resource_mut::<bevy::input_focus::InputFocus>()
        .clear();
    press_key(&mut app, KeyCode::Space);
    step_input(&mut app);
    tap_key(&mut app, KeyCode::Escape);
    assert!(!drawer_open(&app));
    step_input(&mut app);
    assert_eq!(
        app.world()
            .resource::<bevy::input_focus::InputFocus>()
            .get(),
        None,
        "the suspended focus is not restored while activation input is held"
    );
    assert_eq!(field_value(&app, field), draft, "the draft is untouched");

    release_key(&mut app, KeyCode::Space);
    step_input(&mut app);
    assert_eq!(
        app.world()
            .resource::<bevy::input_focus::InputFocus>()
            .get(),
        Some(field),
        "the release restored the suspended field focus"
    );
    assert_eq!(field_value(&app, field), draft, "the draft is intact");
    assert_eq!(
        field_caret_and_selection(&app, field),
        seeded,
        "the caret and selection are unchanged by Monitor"
    );
    assert!(menu(&app).key_editing, "editing resumed");
    assert_eq!(checker.calls(), 0, "no credential check was started");
    assert_eq!(
        app.world().resource::<CredentialsState>().row,
        row_before,
        "Monitor did not commit, read or rewrite the credential row"
    );
    assert!(
        !app.world()
            .get_resource::<MonitorHistory>()
            .is_some_and(|history| history.text().contains(draft)),
        "the secret draft never reached the retained Monitor output"
    );
    assert!(
        all_texts(&app).iter().all(|text| !text.contains(draft)),
        "no Monitor-managed rendered text shows the secret draft"
    );
}

/// A removed or read-only field never resurrects editing or keeps the stale
/// secret draft; the current safe focus is used instead.
#[test]
fn invalidated_draft_is_cleared_not_restored() {
    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let field = spawn_key_field(&mut app);
    type_character(&mut app, window, 'k');
    app.world_mut().resource_mut::<MenuState>().open = true;
    app.world_mut().resource_mut::<MenuState>().view = MenuView::Settings;
    assert_eq!(field_value(&app, field), "k");

    tap_key(&mut app, KeyCode::F3);
    // A verdict lands while the drawer is open: the row is Checking (read-only).
    app.world_mut().resource_mut::<CredentialsState>().row = KeyRow::Checking;
    tap_key(&mut app, KeyCode::Escape);
    assert!(!drawer_open(&app));
    for _ in 0..3 {
        step_input(&mut app);
    }
    assert_eq!(
        field_value(&app, field),
        "",
        "the invalidated draft was cleared, not restored"
    );
    assert!(
        !menu(&app).key_editing,
        "editing did not resume on a read-only field"
    );
    assert_ne!(
        app.world()
            .resource::<bevy::input_focus::InputFocus>()
            .get(),
        Some(field),
        "focus moved off the invalidated field"
    );
}

/// A departed field is never resurrected: its suspended draft is not restored
/// onto the live replacement field, and no editing is forced onto the missing
/// view.
#[test]
fn a_departed_field_is_not_resurrected_and_its_draft_is_discarded() {
    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let checker = FakeKeyCheck::new();
    install_key_check(&mut app, &checker);
    let departed = spawn_key_field(&mut app);
    type_character(&mut app, window, 'k');
    app.world_mut().resource_mut::<MenuState>().open = true;
    app.world_mut().resource_mut::<MenuState>().view = MenuView::Settings;
    assert_eq!(field_value(&app, departed), "k");

    tap_key(&mut app, KeyCode::F3);
    // The edited view departs while the drawer is open, and a replacement field
    // with its own draft takes the focus.
    app.world_mut().entity_mut(departed).despawn();
    let replacement = spawn_key_field(&mut app);
    app.world_mut()
        .entity_mut(replacement)
        .get_mut::<EditableText>()
        .expect("the replacement is an EditableText")
        .editor_mut()
        .set_text("b");
    tap_key(&mut app, KeyCode::Escape);
    for _ in 0..3 {
        step_input(&mut app);
    }
    assert!(!drawer_open(&app));
    assert_eq!(
        field_value(&app, replacement),
        "b",
        "the departed draft was not restored onto the live replacement"
    );
    assert_eq!(
        app.world()
            .resource::<bevy::input_focus::InputFocus>()
            .get(),
        Some(replacement),
        "focus stayed on the live replacement, not the departed entity"
    );
    assert!(
        all_texts(&app).iter().all(|text| !text.contains('k')),
        "the departed draft never reappeared in any rendered text"
    );
    assert_eq!(checker.calls(), 0, "no credential check was started");
}

/// An environment-controlled row is read-only for the session: the suspended
/// secret draft is cleared and editing does not resume.
#[test]
fn an_environment_row_clears_the_suspended_draft_and_leaves_editing() {
    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let field = spawn_key_field(&mut app);
    type_character(&mut app, window, 'k');
    app.world_mut().resource_mut::<MenuState>().open = true;
    app.world_mut().resource_mut::<MenuState>().view = MenuView::Settings;
    assert_eq!(field_value(&app, field), "k");

    tap_key(&mut app, KeyCode::F3);
    // The environment variable wins while the drawer is open.
    app.world_mut().resource_mut::<CredentialsState>().row = KeyRow::FromEnvironment {
        fingerprint: "sk-dummy\u{2026}tail".to_string(),
    };
    tap_key(&mut app, KeyCode::Escape);
    for _ in 0..3 {
        step_input(&mut app);
    }
    assert_eq!(
        field_value(&app, field),
        "",
        "the stale secret draft was cleared, not restored"
    );
    assert!(
        !menu(&app).key_editing,
        "editing did not resume on a read-only environment row"
    );
}

/// A changed but still-editable row is not an invalidation: the valid suspended
/// draft, caret and selection are restored, unlike the read-only rows.
#[test]
fn a_changed_editable_row_keeps_the_valid_draft() {
    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let field = spawn_key_field(&mut app);
    type_character(&mut app, window, 'k');
    select_left(&mut app, window, 1);
    app.world_mut().resource_mut::<MenuState>().open = true;
    app.world_mut().resource_mut::<MenuState>().view = MenuView::Settings;
    let seeded = field_caret_and_selection(&app, field);

    tap_key(&mut app, KeyCode::F3);
    // The stored identity re-resolves to another editable verdict while the
    // draft is suspended.
    app.world_mut().resource_mut::<CredentialsState>().row = KeyRow::SetUnchecked {
        fingerprint: "sk-changed\u{2026}tail".to_string(),
    };
    tap_key(&mut app, KeyCode::Escape);
    for _ in 0..3 {
        step_input(&mut app);
    }
    assert_eq!(field_value(&app, field), "k", "the valid draft is restored");
    assert_eq!(
        field_caret_and_selection(&app, field),
        seeded,
        "the caret and selection are unchanged"
    );
    assert!(
        menu(&app).key_editing,
        "editing resumed on the changed editable row"
    );
}

/// Opening cancels an active Settings drag at its last applied value, and the
/// held pointer cannot resume it after close.
#[test]
fn opening_cancels_an_active_settings_drag() {
    let pack = temp_pack();
    let (mut app, _window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    // A fresh press on the TextSpeed track starts the drag through the real
    // pointer path; the first `drag.control()` is the start witness.
    app.world_mut()
        .spawn((MenuAction::TextSpeed, Interaction::Pressed));
    step_input(&mut app);
    assert_eq!(
        app.world()
            .resource::<crate::presentation::settings_view::SettingsDrag>()
            .control(),
        Some(crate::settings::TrackControl::TextSpeed),
        "the fresh press started the drag"
    );
    let before = (*app.world().resource::<SettingsState>()).text_speed();

    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app));
    assert_eq!(
        app.world()
            .resource::<crate::presentation::settings_view::SettingsDrag>()
            .control(),
        None,
        "opening cancelled the drag"
    );
    assert!(
        app.world()
            .get_resource::<InputQuarantine>()
            .expect("quarantine")
            .holds_button(MouseButton::Left),
        "the held pointer is quarantined across the transition"
    );
    assert_eq!(
        app.world().resource::<SettingsState>().text_speed(),
        before,
        "the last applied value is preserved, with no rollback or flush"
    );

    // The held pointer cannot resume the drag after close.
    tap_key(&mut app, KeyCode::Escape);
    assert!(!drawer_open(&app));
    app.world_mut()
        .spawn((MenuAction::TextSpeed, Interaction::Pressed));
    step_input(&mut app);
    assert_eq!(
        app.world()
            .resource::<crate::presentation::settings_view::SettingsDrag>()
            .control(),
        None,
        "held pointer state cannot restart the drag"
    );
}

/// First open focuses Show everything; Tab/Shift+Tab wrap the three controls;
/// a reopen remembers the last valid control.
#[test]
fn focus_cycle_wraps_and_reopen_remembers() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    enter_play_with_input(&mut app);

    tap_key(&mut app, KeyCode::F3);
    assert_eq!(drawer_focus(&app), MonitorControl::ShowEverything);
    tap_key(&mut app, KeyCode::Tab);
    assert_eq!(drawer_focus(&app), MonitorControl::FollowLatest);
    tap_key(&mut app, KeyCode::Tab);
    assert_eq!(drawer_focus(&app), MonitorControl::Close);
    tap_key(&mut app, KeyCode::Tab);
    assert_eq!(
        drawer_focus(&app),
        MonitorControl::ShowEverything,
        "Tab wraps"
    );

    press_key(&mut app, KeyCode::ShiftLeft);
    tap_key(&mut app, KeyCode::Tab);
    release_key(&mut app, KeyCode::ShiftLeft);
    step_input(&mut app);
    assert_eq!(
        drawer_focus(&app),
        MonitorControl::Close,
        "Shift+Tab wraps backwards"
    );

    tap_key(&mut app, KeyCode::Escape);
    assert!(!drawer_open(&app));
    tap_key(&mut app, KeyCode::F3);
    assert_eq!(
        drawer_focus(&app),
        MonitorControl::Close,
        "the reopen remembered the last control"
    );
}

/// Follow latest is a truthful idempotent state; Show everything toggles only
/// the display predicate.
#[test]
fn filter_control_toggles_and_follow_latest_is_idempotent() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    enter_play_with_input(&mut app);
    tap_key(&mut app, KeyCode::F3);

    activate_control(&mut app, MonitorControl::FollowLatest);
    assert!(drawer_open(&app));
    assert!(drawer_panel(&app).is_following());
    assert_eq!(drawer_view(&app), MonitorView::UnmeiOnly);

    activate_control(&mut app, MonitorControl::ShowEverything);
    assert_eq!(drawer_view(&app), MonitorView::Everything);
    activate_control(&mut app, MonitorControl::ShowEverything);
    assert_eq!(drawer_view(&app), MonitorView::UnmeiOnly);

    // Close is a control too.
    activate_control(&mut app, MonitorControl::Close);
    assert!(!drawer_open(&app));
}

/// An OS repeat of F3 never oscillates the drawer; release plus a fresh press
/// is one action.
#[test]
fn held_and_repeated_f3_does_not_oscillate() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    enter_play_with_input(&mut app);

    press_key(&mut app, KeyCode::F3);
    step_input(&mut app);
    assert!(drawer_open(&app));
    for _ in 0..5 {
        send_key_message(
            &mut app,
            KeyCode::F3,
            bevy::input::ButtonState::Pressed,
            true,
        );
        step_input(&mut app);
        assert!(drawer_open(&app), "a repeat never toggles");
    }

    release_key(&mut app, KeyCode::F3);
    step_input(&mut app);
    assert!(drawer_open(&app), "the release alone does not close");
    press_key(&mut app, KeyCode::F3);
    step_input(&mut app);
    assert!(!drawer_open(&app), "a fresh press closes once");
    release_key(&mut app, KeyCode::F3);
    step_input(&mut app);
    assert!(!drawer_open(&app));
}

/// The already-armed pre-Monitor aggregate Ctrl hold keeps running while
/// physically down, its release mirrors under Monitor, and a Ctrl first pressed
/// inside never arms without a release and a fresh press.
#[test]
fn prearmed_ctrl_keeps_running_under_monitor_and_a_new_ctrl_never_arms() {
    let pack = pack_with_entry_says(&[
        "Narrator: One.",
        "Narrator: Two.",
        "Narrator: Three.",
        "Narrator: Four.",
        "Narrator: Five.",
    ]);
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);

    // Arm the aggregate hold in play, then open Monitor without releasing it.
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app), "the hold armed in play");
    tap_key(&mut app, KeyCode::F3);
    assert!(
        drawer_open(&app) && skip_force(&app),
        "the hold survived opening"
    );

    let parked = wait(&app);
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert!(skip_force(&app), "the pre-armed hold stays active");
    assert_ne!(wait(&app), parked, "the hold kept advancing under Monitor");

    // The release mirror runs even with Monitor open.
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(!skip_force(&app), "the release cleared the latch");

    // A Ctrl first pressed inside cannot arm, here or after close.
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(!skip_force(&app), "a press inside never arms");
    tap_key(&mut app, KeyCode::Escape);
    assert!(!drawer_open(&app));
    assert!(!skip_force(&app), "closing did not arm the held Ctrl");
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(!skip_force(&app));
    // A fresh press in play after the release arms again.
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app), "a fresh press in play arms again");
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
}

/// Reveal and auto keep their ordinary clocks while Monitor is open: no new
/// pause, no catch-up freeze.
#[test]
fn reveal_and_auto_keep_running_under_monitor() {
    let pack = pack_with_entry_says(&["Narrator: A long enough line to reveal."]);
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);
    tap_key(&mut app, KeyCode::KeyA);
    assert!(auto_on(&app), "A turned auto on");

    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app));
    let parked = wait(&app);
    advance_time(&mut app, 2.5);
    step_input(&mut app);
    assert_ne!(
        wait(&app),
        parked,
        "auto advanced the line while Monitor was open"
    );
}

/// An already-engaged seen skip keeps its own step clock under Monitor:
/// opening the drawer neither pauses it nor ends it, and no input leaks.
#[test]
fn an_engaged_seen_skip_keeps_stepping_under_monitor() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let mut app = app_with_known_entry(&pack);
    build_ui(&mut app);
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));
    let first = wait(&app).expect("parked on the first say");

    press_and_update(&mut app, KeyCode::F3);
    assert!(drawer_open(&app));
    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert!(skip_on(&app), "Monitor did not end the seen skip");
    assert_ne!(
        wait(&app),
        Some(first),
        "the seen skip kept its ordinary step clock under Monitor"
    );
}

/// A Notice keeps its ordinary countdown while Monitor owns input: no new
/// presentation pause and no input-driven dismissal.
#[test]
fn a_notice_keeps_its_countdown_under_monitor() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    app.world_mut()
        .resource_mut::<Notice>()
        .show("something failed");

    press_and_update(&mut app, KeyCode::F3);
    assert!(drawer_open(&app));
    advance_time(&mut app, crate::failure::NOTICE_SECONDS + 0.1);
    tick_update(&mut app);

    assert!(drawer_open(&app), "the Notice clock did not close Monitor");
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "the Notice expired on its own clock under Monitor"
    );
}

/// A committed key's ping keeps its ordinary completion clock while Monitor is
/// open: the verdict lands, the resolver rebuilds and the check neither
/// cancels, refires nor pauses.
#[test]
fn a_key_check_completes_while_monitor_is_open() {
    let mut rig = key_check_rig(Arc::new(FakeResolver::new(model_answer("warm"))));
    let checker = rig.checker.clone();
    let key = format!("sk-or-v1-{}", "k".repeat(60));
    commit_key_at(&mut rig.app, &key);
    pump_until(&mut rig.app, |_| checker.calls() == 1);
    assert_eq!(
        rig.app.world().resource::<CredentialsState>().row,
        KeyRow::Checking,
        "the ping is in flight"
    );

    press_and_update(&mut rig.app, KeyCode::F3);
    assert!(drawer_open(&rig.app), "F3 opened Monitor over Settings");

    checker.release(KeyVerdict::Accepted);
    let fingerprint = credentials::fingerprint(&key);
    pump_until(&mut rig.app, |app| {
        app.world().resource::<CredentialsState>().row
            == KeyRow::Set {
                fingerprint: fingerprint.clone(),
            }
    });

    assert!(drawer_open(&rig.app), "the verdict left Monitor open");
    assert_eq!(
        rig.keys.lock().unwrap().as_slice(),
        std::slice::from_ref(&key),
        "the resolver rebuilt from the completed check under Monitor"
    );
    assert_eq!(checker.calls(), 1, "the check did not refire");
}

/// The default projection is the exact, case-sensitive `unmei::` prefix;
/// Show everything reveals the other surviving admitted targets from the same
/// history, with no rename, reset or second buffer.
#[test]
fn default_projection_is_the_exact_unmei_prefix_and_everything_shares_it() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    seed_monitor_history(
        &mut app,
        vec![
            test_monitor_line(0, "unmei::failure", log::Level::Info, "first"),
            test_monitor_line(1, "unmeiX", log::Level::Info, "lookalike"),
            test_monitor_line(2, "unmei", log::Level::Info, "no separator"),
            test_monitor_line(3, "dep::crate", log::Level::Info, "dependency"),
            test_monitor_line(4, "UNMEI::x", log::Level::Info, "wrong case"),
            test_monitor_line(5, "unmei::decision", log::Level::Warn, "explicit"),
        ],
    );
    tap_key(&mut app, KeyCode::F3);
    for _ in 0..4 {
        step_input(&mut app);
    }
    let projection = app.world().resource::<crate::drawer::MonitorProjection>();
    assert!(!projection.is_updating(), "a six-line projection settles");
    assert_eq!(
        projection.targets(),
        vec!["unmei::failure", "unmei::decision"],
        "only the exact case-sensitive prefix matches by default"
    );

    let retained_before = app.world().resource::<MonitorHistory>().retained();
    activate_control(&mut app, MonitorControl::ShowEverything);
    for _ in 0..4 {
        step_input(&mut app);
    }
    let projection = app.world().resource::<crate::drawer::MonitorProjection>();
    assert_eq!(
        projection.targets(),
        vec![
            "unmei::failure",
            "unmeiX",
            "unmei",
            "dep::crate",
            "UNMEI::x",
            "unmei::decision"
        ],
        "Show everything removes only the display predicate"
    );
    assert_eq!(
        app.world().resource::<MonitorHistory>().retained(),
        retained_before,
        "toggling views neither clears nor duplicates the shared history"
    );
    assert_eq!(
        drawer_rows(&mut app)
            .iter()
            .filter(|(_, part, _)| *part == MonitorRowPart::Target)
            .count(),
        6,
        "the display is the projection, not a second history"
    );
}

/// A genuine `log`-bridge record and an ordinary event carrying a bridge-named
/// field reach the same retained history. Only the real metadata target drives
/// the projection, so the ordinary event stays out of the default view and
/// appears under Show everything without losing its level or the shared
/// history.
#[test]
fn a_forged_bridge_field_does_not_enter_the_default_projection() {
    use crate::monitor::{capture_layer, MonitorCapture, SystemClock};
    use bevy::log::tracing_subscriber::prelude::*;

    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    let capture = MonitorCapture::install(Arc::new(SystemClock::new()));
    {
        let subscriber = bevy::log::tracing_subscriber::registry().with(capture_layer(&capture));
        bevy::log::tracing::subscriber::with_default(subscriber, || {
            // An ordinary event whose field is named like bridge metadata.
            bevy::log::tracing::event!(
                target: "dep::crate",
                bevy::log::tracing::Level::INFO,
                { "log.target" } = "unmei::forged",
                "the real body"
            );
            // A genuine bridged record, at its own level.
            let record = log::Record::builder()
                .args(format_args!("bridged body"))
                .level(log::Level::Warn)
                .target("unmei::bridged")
                .build();
            tracing_log::format_trace(&record).expect("the bridge dispatches");
        });
    }
    app.insert_resource(capture);
    app.world_mut().init_resource::<MonitorHistory>();
    app.world_mut()
        .run_system_once(crate::monitor::ingest_capture)
        .expect("the ingest system runs");

    tap_key(&mut app, KeyCode::F3);
    for _ in 0..4 {
        step_input(&mut app);
    }
    let default_rows = projected_rows(&mut app);
    assert_eq!(
        default_rows,
        vec![("unmei::bridged".to_string(), log::Level::Warn)],
        "only the genuine unmei:: target matches the default view"
    );

    activate_control(&mut app, MonitorControl::ShowEverything);
    for _ in 0..4 {
        step_input(&mut app);
    }
    assert_eq!(
        projected_rows(&mut app),
        vec![
            ("dep::crate".to_string(), log::Level::Info),
            ("unmei::bridged".to_string(), log::Level::Warn),
        ],
        "Show everything reveals both surviving events in admission order"
    );
    assert_eq!(
        app.world().resource::<MonitorHistory>().retained(),
        2,
        "filtering never drops the shared history"
    );
}

/// The projected `(target, level)` rows, oldest first.
fn projected_rows(app: &mut App) -> Vec<(String, log::Level)> {
    app.world()
        .resource::<crate::drawer::MonitorProjection>()
        .rows()
        .iter()
        .map(|row| (row.target.clone(), row.level))
        .collect()
}

/// A large projection is progressive: bounded per update, marked while
/// incomplete, and complete without a long-frame catch-up.
#[test]
fn progressive_projection_is_bounded_marked_and_completes() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    seed_monitor_history(
        &mut app,
        (0..1000)
            .map(|i| test_monitor_line(i, "unmei::seq", log::Level::Info, &format!("event {i}")))
            .collect(),
    );
    tap_key(&mut app, KeyCode::F3);
    step_input(&mut app);

    let work = app.world().resource::<MonitorWork>();
    assert!(
        work.scanned <= PROJECTION_SCAN_BUDGET,
        "at most {PROJECTION_SCAN_BUDGET} entries per update: {}",
        work.scanned
    );
    assert!(work.text_ops <= TEXT_WORK_BUDGET);
    let projection = app.world().resource::<crate::drawer::MonitorProjection>();
    assert!(projection.is_updating(), "the marked in-progress flag");
    // Three frames have run while open (the F3 press, its release and one
    // step), so the projection holds at most three update budgets.
    assert!(projection.shown() <= 3 * PROJECTION_SCAN_BUDGET);
    assert!(
        drawer_status(&mut app).contains("updating view"),
        "the footer marks the incomplete projection"
    );

    for _ in 0..16 {
        step_input(&mut app);
        if !app
            .world()
            .resource::<crate::drawer::MonitorProjection>()
            .is_updating()
        {
            break;
        }
        assert!(
            app.world().resource::<MonitorWork>().scanned <= PROJECTION_SCAN_BUDGET,
            "no catch-up scan over the budget"
        );
        assert!(app.world().resource::<MonitorWork>().text_ops <= TEXT_WORK_BUDGET);
    }
    let projection = app.world().resource::<crate::drawer::MonitorProjection>();
    assert!(!projection.is_updating(), "the pass finished");
    assert_eq!(projection.shown(), 1000, "all matching retained lines");
    assert_eq!(
        drawer_rows(&mut app)
            .iter()
            .filter(|(_, part, _)| *part == MonitorRowPart::Target)
            .count(),
        DISPLAY_ROWS,
        "only the newest display window is materialized"
    );

    // A settled frame does no projection or text work at all.
    step_input(&mut app);
    assert_eq!(app.world().resource::<MonitorWork>().scanned, 0);
    assert_eq!(app.world().resource::<MonitorWork>().text_ops, 0);
    assert!(
        !drawer_status(&mut app).contains("updating view"),
        "the marked flag cleared"
    );
}

/// A filter change cancels the obsolete pass: only current-predicate rows are
/// ever displayed, and the shared history is untouched.
#[test]
fn filter_change_cancels_obsolete_work_and_never_mixes_predicates() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    let mut lines = Vec::new();
    for i in 0..300u64 {
        lines.push(test_monitor_line(
            i * 2,
            "unmei::kept",
            log::Level::Info,
            "kept",
        ));
        lines.push(test_monitor_line(
            i * 2 + 1,
            "dep::noise",
            log::Level::Info,
            "noise",
        ));
    }
    seed_monitor_history(&mut app, lines);
    tap_key(&mut app, KeyCode::F3);
    step_input(&mut app);

    // Mid-refresh, the old predicate's rows are already up and all match.
    assert!(app
        .world()
        .resource::<crate::drawer::MonitorProjection>()
        .is_updating());
    assert!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .targets()
            .iter()
            .all(|target| target == "unmei::kept"),
        "only the current predicate is displayed mid-refresh"
    );

    // Everything is a real second predicate: mid-refresh it may hold both
    // target families (all of them match it).
    activate_control(&mut app, MonitorControl::ShowEverything);
    assert!(app
        .world()
        .resource::<crate::drawer::MonitorProjection>()
        .is_updating());
    // Switching back cancels that obsolete pass: no dependency row from it may
    // survive into the `unmei::` view.
    activate_control(&mut app, MonitorControl::ShowEverything);
    let projection = app.world().resource::<crate::drawer::MonitorProjection>();
    assert!(
        projection
            .targets()
            .iter()
            .all(|target| target == "unmei::kept"),
        "obsolete rows were cancelled, not mixed in: {:?}",
        projection.targets().first()
    );
    for _ in 0..16 {
        step_input(&mut app);
        if !app
            .world()
            .resource::<crate::drawer::MonitorProjection>()
            .is_updating()
        {
            break;
        }
        assert!(
            app.world()
                .resource::<crate::drawer::MonitorProjection>()
                .targets()
                .iter()
                .all(|target| target == "unmei::kept"),
            "only current-predicate rows at any point of the refresh"
        );
    }
    let projection = app.world().resource::<crate::drawer::MonitorProjection>();
    assert!(!projection.is_updating(), "the new pass finished");
    assert_eq!(projection.shown(), 300);
}

/// The newest marker follows the newest **retained** line, never the newest
/// matching one.
#[test]
fn newest_marker_follows_the_retained_tail_not_the_matching_tail() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    seed_monitor_history(
        &mut app,
        vec![
            test_monitor_line(0, "unmei::failure", log::Level::Info, "first"),
            test_monitor_line(1, "unmei::failure", log::Level::Info, "second"),
            test_monitor_line(2, "dep::noise", log::Level::Error, "newest retained"),
        ],
    );
    tap_key(&mut app, KeyCode::F3);
    for _ in 0..3 {
        step_input(&mut app);
    }
    assert_eq!(
        row_marker(&mut app, 2),
        None,
        "the filtered-out newest retained line is not shown, so nothing is marked"
    );

    activate_control(&mut app, MonitorControl::ShowEverything);
    for _ in 0..3 {
        step_input(&mut app);
    }
    let newest = row_marker(&mut app, 2).expect("the newest retained line is shown");
    assert_eq!(
        newest.0, NEWEST_WASH,
        "the newest retained row carries the wash"
    );
    assert_eq!(newest.1, NEWEST_INSET, "and the warm inset");
    for sequence in [0, 1] {
        let older = row_marker(&mut app, sequence).expect("older rows exist");
        assert_eq!(older.0, Color::NONE, "no other row is decorated");
        assert_eq!(older.1, Color::NONE);
    }
}

/// The empty projection has an empty state; later matching arrivals neither
/// resurrect anything nor silently enable a different reader state.
#[test]
fn empty_projection_shows_an_empty_state_and_later_matches_appear() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    tap_key(&mut app, KeyCode::F3);
    for _ in 0..3 {
        step_input(&mut app);
    }
    assert!(
        has_component::<drawer::MonitorEmptyState>(&mut app),
        "the empty view has an empty state"
    );
    assert!(drawer_status(&mut app).contains("shown 0"));

    seed_monitor_history(
        &mut app,
        vec![test_monitor_line(
            0,
            "unmei::failure",
            log::Level::Info,
            "later",
        )],
    );
    for _ in 0..3 {
        step_input(&mut app);
    }
    assert!(
        !has_component::<drawer::MonitorEmptyState>(&mut app),
        "the empty state cleared"
    );
    assert!(drawer_panel(&app).is_following());
    assert!(drawer_status(&mut app).contains("shown 1"));
}

/// Closed means no hidden projection, no hidden text work and no hidden tree.
#[test]
fn a_closed_drawer_does_no_projection_or_text_work() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    seed_monitor_history(
        &mut app,
        (0..200)
            .map(|i| test_monitor_line(i, "unmei::seq", log::Level::Info, "x"))
            .collect(),
    );
    app.update();
    assert!(!drawer_open(&app));
    let work = app.world().resource::<MonitorWork>();
    assert_eq!(work.scanned, 0, "no hidden scans");
    assert_eq!(work.text_ops, 0, "no hidden text work");
    assert_eq!(drawer_roots(&mut app), 0, "no hidden tree");

    // Reopen resumes exactly where the pass stopped and sees the history.
    tap_key(&mut app, KeyCode::F3);
    for _ in 0..4 {
        step_input(&mut app);
    }
    assert_eq!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .shown(),
        200
    );
}

/// The footer reports the settled status vocabulary truthfully.
#[test]
fn status_footer_reports_view_counts_counters_and_following() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    seed_monitor_history(
        &mut app,
        vec![
            test_monitor_line(0, "unmei::failure", log::Level::Info, "a"),
            test_monitor_line(1, "dep::noise", log::Level::Warn, "b"),
        ],
    );
    tap_key(&mut app, KeyCode::F3);
    for _ in 0..3 {
        step_input(&mut app);
    }
    assert_eq!(
        drawer_status(&mut app),
        "showing unmei:: only · retained 2 · shown 1 · evicted 0 · lost 0 · truncated 0 · following latest · F3/Esc close · Tab cycles controls"
    );

    activate_control(&mut app, MonitorControl::ShowEverything);
    for _ in 0..3 {
        step_input(&mut app);
    }
    assert_eq!(
        drawer_status(&mut app),
        "showing everything · retained 2 · shown 2 · evicted 0 · lost 0 · truncated 0 · following latest · F3/Esc close · Tab cycles controls"
    );
}

/// Launch-lifetime counters survive a display toggle and hide/reopen: the
/// projection and reader never touch `MonitorCapture`, so status keeps the same
/// evicted/lost/truncated values across ordinary Monitor transitions.
#[test]
fn counters_survive_toggling_and_hide_reopen() {
    use bevy::log::tracing;
    use bevy::log::tracing_subscriber;
    use tracing_subscriber::prelude::*;

    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    seed_numbered_monitor(&mut app, 0..10, "unmei::seq");

    // Admit one oversized event through the real capture layer: a successful
    // admission counts as exactly one truncation.
    let capture =
        crate::monitor::MonitorCapture::install(Arc::new(crate::monitor::SystemClock::new()));
    let subscriber = tracing_subscriber::registry().with(crate::monitor::capture_layer(&capture));
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(target: "unmei::seq", "{}", "y".repeat(9000));
    });
    assert_eq!(
        capture.counters().truncated,
        1,
        "the oversized line was admitted truncated"
    );
    app.insert_resource(capture);
    app.world_mut().init_resource::<MonitorHistory>();

    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    assert!(drawer_status(&mut app).contains("truncated 1"));

    // A filter toggle and hide/reopen leave the shared history and counters as
    // they were.
    activate_control(&mut app, MonitorControl::ShowEverything);
    settle_reader(&mut app);
    tap_key(&mut app, KeyCode::F3);
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    let status = drawer_status(&mut app);
    assert!(
        status.contains("retained 10 · shown 10 · evicted 0 · lost 0 · truncated 1"),
        "{status}"
    );
}

/// Genuinely admitted events — not a seeded history — survive the live session
/// transitions: entering play, hide/reopen, the manual slot load and a real
/// player New Game. The launch-local history and the saturating counters are
/// unchanged throughout.
#[test]
fn captured_history_and_counters_survive_live_session_transitions() {
    use bevy::log::tracing;
    use bevy::log::tracing_subscriber;
    use tracing_subscriber::prelude::*;

    let pack = temp_pack();
    let capture =
        crate::monitor::MonitorCapture::install(Arc::new(crate::monitor::SystemClock::new()));
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    app.world_mut().spawn((Window::default(), PrimaryWindow));
    // The capture must exist before the ingest system is added; the headless
    // boot has no logger, so this is the only source of retained lines.
    app.insert_resource(capture.clone());
    app.world_mut()
        .init_resource::<crate::monitor::MonitorHistory>();
    app.add_systems(Update, crate::monitor::ingest_capture);

    // Admit real events through the production capture layer: 30 ordinary lines
    // and one oversized event that counts as exactly one truncation.
    let subscriber = tracing_subscriber::registry().with(crate::monitor::capture_layer(&capture));
    tracing::subscriber::with_default(subscriber, || {
        for index in 0..30 {
            tracing::info!(target: "unmei::seq", "captured event {index:04}");
        }
        tracing::info!(target: "unmei::seq", "{}", "y".repeat(9000));
    });
    step_input(&mut app); // ingest the admitted events

    assert_eq!(
        app.world()
            .resource::<crate::monitor::MonitorHistory>()
            .retained(),
        31,
        "all 31 admitted events are retained"
    );
    assert_eq!(capture.counters().truncated, 1);

    enter_play_with_input(&mut app);
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    let status = drawer_status(&mut app);
    assert!(status.contains("retained 31"), "{status}");
    tap_key(&mut app, KeyCode::F3);
    assert!(!drawer_open(&app));

    // The manual slot round trip and a real player New Game both perform their
    // own session resets while the launch-local Monitor state stays put.
    tap_key(&mut app, KeyCode::F5);
    assert!(manual_path(&app).exists(), "F5 wrote the manual slot 1");
    tap_key(&mut app, KeyCode::F9);
    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open);
    click_input(&mut app, MenuAction::NewGame);
    click_input(&mut app, MenuAction::ConfirmAccept);
    assert!(!menu(&app).open, "New Game starts play");
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);

    let history = app.world().resource::<crate::monitor::MonitorHistory>();
    assert_eq!(
        history.retained(),
        31,
        "the history survives the transitions"
    );
    assert!(history.text().contains("captured event 0000"));
    assert!(history.text().contains("captured event 0029"));
    assert!(history.text().contains("[truncated]"));
    let counters = capture.counters();
    assert_eq!(counters.truncated, 1, "the truncation counter survives");
    assert_eq!(counters.evicted, 0, "nothing was evicted");
    assert_eq!(counters.lost, 0, "nothing was lost");
}

/// A poisoned capture is reported as local unavailable status, never a
/// synthetic row, tracing event or Notice.
#[test]
fn capture_unavailable_is_local_status_only() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    // Install a capture and poison its completed queue through the same seam
    // the capture tests use.
    let capture =
        crate::monitor::MonitorCapture::install(Arc::new(crate::monitor::SystemClock::new()));
    app.insert_resource(capture);
    app.world_mut().init_resource::<MonitorHistory>();
    // Poison the mutex by locking and panicking on another thread.
    let shared = app.world().resource::<crate::monitor::MonitorCapture>();
    shared.poison_for_test();
    assert!(shared.unavailable());

    seed_monitor_history(
        &mut app,
        vec![test_monitor_line(0, "unmei::x", log::Level::Info, "a")],
    );
    tap_key(&mut app, KeyCode::F3);
    for _ in 0..3 {
        step_input(&mut app);
    }
    assert!(drawer_status(&mut app).contains("capture unavailable"));
    assert!(
        !drawer_rows(&mut app)
            .iter()
            .any(|(_, _, text)| text.contains("unavailable")),
        "status is never a log row"
    );
    assert!(app.world().resource::<Notice>().text.is_none());
}

/// Retention eviction drops the evicted payload from the projection in the same
/// update, so no second history keeps it alive.
#[test]
fn evicted_rows_are_dropped_from_the_projection() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    seed_monitor_history(
        &mut app,
        (0..10)
            .map(|i| test_monitor_line(i, "unmei::seq", log::Level::Info, "old"))
            .collect(),
    );
    tap_key(&mut app, KeyCode::F3);
    for _ in 0..3 {
        step_input(&mut app);
    }
    assert_eq!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .sequences(),
        (0..10).collect::<Vec<_>>()
    );

    // The oldest five age out of retained history.
    seed_monitor_history(
        &mut app,
        (5..15)
            .map(|i| test_monitor_line(i, "unmei::seq", log::Level::Info, "new"))
            .collect(),
    );
    for _ in 0..3 {
        step_input(&mut app);
    }
    let projection = app.world().resource::<crate::drawer::MonitorProjection>();
    assert_eq!(
        projection.sequences(),
        (5..15).collect::<Vec<_>>(),
        "evicted rows are gone, not cached"
    );
}

/// A hidden drawer prunes evicted references without materializing or replacing
/// hidden text, so a previously full projection can never keep a second copy of
/// a payload retention already evicted, and reopening cannot resurrect it
/// (finding A02).
#[test]
fn a_hidden_drawer_prunes_evicted_payloads_without_text_work() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    seed_numbered_monitor(&mut app, 0..1000, "unmei::seq");
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    assert_eq!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .shown(),
        1000,
        "the whole retained history is projected"
    );

    // Close, then replace every retained entry while hidden.
    tap_key(&mut app, KeyCode::F3);
    assert!(!drawer_open(&app));
    seed_numbered_monitor(&mut app, 1000..2000, "unmei::seq");
    for _ in 0..3 {
        step_input(&mut app);
    }
    assert_eq!(
        drawer_roots(&mut app),
        0,
        "closed means no hidden tree, so no hidden row text can be materialized"
    );
    assert_eq!(
        app.world().resource::<MonitorWork>().text_ops,
        0,
        "the hidden update reports no visible text work"
    );
    assert_eq!(
        app.world().resource::<MonitorWork>().scanned,
        0,
        "the hidden update reports no metadata scan"
    );
    assert_eq!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .shown(),
        0,
        "every evicted payload reference is dropped while hidden"
    );

    // Reopen: the replacement history projects from its own oldest entry and no
    // evicted text is resurrected.
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    let sequences = app
        .world()
        .resource::<crate::drawer::MonitorProjection>()
        .sequences();
    assert_eq!(sequences.first().copied(), Some(1000));
    assert!(
        sequences.iter().all(|sequence| *sequence >= 1000),
        "no evicted payload survives in the projection"
    );
    assert_eq!(sequences.len(), 1000, "the replacement history is complete");
}

/// A history replaced by a strictly older range (a reseeded fixture) must not
/// leave rows the retained history no longer owns looking like coverage: the
/// projection restarts and shows only the replacement's own identities.
#[test]
fn a_shrunk_history_reprojects_without_stale_rows() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    seed_numbered_monitor(&mut app, 0..100, "unmei::seq");
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    assert_eq!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .shown(),
        100
    );

    // The replacement's newest entry is below the examined band.
    seed_numbered_monitor(&mut app, 0..50, "unmei::seq");
    settle_reader(&mut app);
    assert_eq!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .sequences(),
        (0..50).collect::<Vec<_>>(),
        "no row survives from the replaced range"
    );
}

/// The settled drawer geometry at the reference size: fixed px insets, height,
/// bottom reservation, header/body/footer split and the row grid.
#[test]
fn drawer_geometry_is_the_settled_pixels() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    seed_monitor_history(
        &mut app,
        vec![test_monitor_line(
            0,
            "unmei::failure",
            log::Level::Warn,
            "row",
        )],
    );
    tap_key(&mut app, KeyCode::F3);
    for _ in 0..3 {
        step_input(&mut app);
    }

    let world = app.world_mut();
    let root = {
        let mut query = world.query::<(&Node, &GlobalZIndex)>();
        query
            .iter(world)
            .find_map(|(node, index)| {
                // The drawer root is the only absolute full-window node with a
                // global index.
                (index.0 == 100).then_some(node.clone())
            })
            .expect("the drawer root exists with its stacking index")
    };
    assert_eq!(root.position_type, PositionType::Absolute);
    assert_eq!(root.width, Val::Percent(100.0));
    assert_eq!(root.height, Val::Percent(100.0));

    let panel = {
        let mut query = world.query_filtered::<&Node, With<drawer::MonitorPanelNode>>();
        query.iter(world).next().cloned().expect("the panel node")
    };
    assert_eq!(panel.left, Val::Px(24.0));
    assert_eq!(panel.right, Val::Px(24.0));
    assert_eq!(panel.bottom, Val::Px(148.0));
    assert_eq!(panel.height, Val::Px(360.0));
    assert_eq!(panel.border, UiRect::all(Val::Px(1.0)));
    assert_eq!(panel.flex_direction, FlexDirection::Column);

    let body = {
        let mut query = world.query_filtered::<&Node, With<drawer::MonitorLogBody>>();
        query.iter(world).next().cloned().expect("the log body")
    };
    assert_eq!(body.flex_grow, 1.0);
    assert_eq!(body.flex_direction, FlexDirection::Row);
    assert!(!body.overflow.is_visible(), "the body clips the tail");

    // The flexible viewport carries the following bottom alignment and the
    // clipped body above it owns the clipping rect, so the reader's relative
    // offset can never move the clip with it.
    let viewport = {
        let mut query = world.query_filtered::<&Node, With<drawer::MonitorLogViewport>>();
        query.iter(world).next().cloned().expect("the viewport")
    };
    assert_eq!(viewport.flex_grow, 1.0);
    assert_eq!(viewport.justify_content, JustifyContent::FlexEnd);
    assert_eq!(viewport.min_width, Val::Px(0.0));

    // The scrollbar is real chrome: a fixed-width track with a draggable thumb.
    let track = {
        let mut query = world.query_filtered::<&Node, With<drawer::MonitorScrollbarTrack>>();
        query
            .iter(world)
            .next()
            .cloned()
            .expect("the scrollbar track")
    };
    assert_eq!(track.width, Val::Px(drawer::SCROLLBAR_WIDTH));
    assert_eq!(track.height, Val::Percent(100.0));
    let thumb = {
        let mut query = world.query_filtered::<&Node, With<drawer::MonitorScrollbarThumb>>();
        query
            .iter(world)
            .next()
            .cloned()
            .expect("the scrollbar thumb")
    };
    assert_eq!(thumb.width, Val::Percent(100.0));
    assert_eq!(thumb.position_type, PositionType::Absolute);

    let status = {
        let mut query = world.query_filtered::<&Node, With<MonitorStatusText>>();
        query.iter(world).next().cloned().expect("the status node")
    };
    assert_eq!(status.height, Val::Px(13.0));

    let row = {
        let mut query = world.query::<(&MonitorRow, &Node)>();
        query
            .iter(world)
            .find(|(row, _)| row.0 == 0)
            .map(|(_, node)| node.clone())
            .expect("the row container")
    };
    assert_eq!(row.column_gap, Val::Px(10.0));
    assert_eq!(row.padding.top, Val::Px(1.0));
    assert_eq!(row.border.left, Val::Px(2.0));

    let parts: Vec<(
        MonitorRowPart,
        Val,
        TextFont,
        TextColor,
        LineHeight,
        f32,
        Val,
    )> = {
        let mut query =
            world.query::<(&MonitorRowPart, &Node, &TextFont, &TextColor, &LineHeight)>();
        query
            .iter(world)
            .map(|(part, node, font, color, height)| {
                let width = if node.width == Val::Auto {
                    Val::Auto
                } else {
                    node.width
                };
                (
                    *part,
                    width,
                    font.clone(),
                    *color,
                    *height,
                    node.flex_shrink,
                    node.min_width,
                )
            })
            .collect()
    };
    let part = |kind: MonitorRowPart| {
        parts
            .iter()
            .find(|(part, ..)| *part == kind)
            .expect("the row part exists")
    };
    assert_eq!(part(MonitorRowPart::Elapsed).1, Val::Px(72.0));
    assert_eq!(part(MonitorRowPart::Level).1, Val::Px(46.0));
    assert_eq!(part(MonitorRowPart::Target).1, Val::Px(224.0));
    assert_eq!(part(MonitorRowPart::Body).1, Val::Auto);
    // A fixed column holds its width; the message column takes the remaining
    // space and may shrink below its content width so long text wraps inside
    // the panel instead of overflowing the row.
    assert_eq!(part(MonitorRowPart::Target).5, 0.0);
    assert_eq!(part(MonitorRowPart::Body).5, 1.0);
    assert_eq!(part(MonitorRowPart::Body).6, Val::Px(0.0));
    for kind in [
        MonitorRowPart::Elapsed,
        MonitorRowPart::Level,
        MonitorRowPart::Target,
        MonitorRowPart::Body,
    ] {
        let (_, _, font, _, line_height, ..) = part(kind);
        assert_eq!(
            font.font_size,
            bevy::text::FontSize::Px(12.0),
            "{kind:?} uses Fira Mono Medium 12px"
        );
        assert_eq!(*line_height, LineHeight::RelativeToFont(1.45));
    }
    assert_eq!(
        part(MonitorRowPart::Elapsed).3 .0,
        crate::drawer::TONE_FAINT
    );
    assert_eq!(part(MonitorRowPart::Level).3 .0, crate::drawer::TONE_WARN);
    assert_eq!(part(MonitorRowPart::Target).3 .0, crate::drawer::TONE_INFO);
    assert_eq!(part(MonitorRowPart::Body).3 .0, crate::drawer::TONE_TEXT);

    // The header carries Monitor then the three controls in the settled order.
    let labels: Vec<String> = {
        let mut query = world.query::<(&drawer::MonitorControlTag, &Text)>();
        let mut controls: Vec<(MonitorControl, String)> = query
            .iter(world)
            .map(|(tag, text)| (tag.0, text.as_str().to_string()))
            .collect();
        controls.sort_by_key(|(control, _)| match control {
            MonitorControl::ShowEverything => 0,
            MonitorControl::FollowLatest => 1,
            MonitorControl::Close => 2,
        });
        controls.into_iter().map(|(_, label)| label).collect()
    };
    assert_eq!(
        labels,
        vec!["[ ] Show everything", "Follow latest", "Close"],
        "the settled control order and labels"
    );
}

/// Fullscreen keeps fixed px anchoring: no proportional inset, height or
/// reservation, so the drawer widens to the frame.
#[test]
fn drawer_fullscreen_anchoring_is_fixed_pixels() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    tap_key(&mut app, KeyCode::F3);
    for _ in 0..2 {
        step_input(&mut app);
    }
    let world = app.world_mut();
    let panel = {
        let mut query = world.query_filtered::<&Node, With<drawer::MonitorPanelNode>>();
        query.iter(world).next().cloned().expect("the panel node")
    };
    // The drawer is laid out from the window edges with a fixed px inset, so at
    // 1920x1080 its width is 1920 - 2*24 = 1872px with no width cap or scale.
    assert_eq!(panel.left, Val::Px(drawer::PANEL_SIDE_INSET));
    assert_eq!(panel.right, Val::Px(drawer::PANEL_SIDE_INSET));
    assert_eq!(panel.height, Val::Px(drawer::PANEL_HEIGHT));
    assert_eq!(panel.bottom, Val::Px(drawer::PANEL_BOTTOM_RESERVATION));
    assert!(
        matches!(panel.width, Val::Auto | Val::Percent(100.0)),
        "no width cap or proportional width"
    );
}

/// An oversized event collapses the grid into one warm-toned line with a
/// visible `[truncated]` suffix; an escaped event keeps one logical row.
#[test]
fn truncated_and_escaped_rows_render_as_one_line() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    let escaped = test_monitor_line(
        0,
        "unmei::failure",
        log::Level::Error,
        "line one\\nline two \\\\n literal",
    );
    let mut truncated = test_monitor_line(1, "unmei::failure", log::Level::Warn, "huge");
    truncated.text = format!(
        "0000000001ms WARN  unmei::failure {}{}",
        "x".repeat(100),
        " [truncated]"
    );
    truncated.body_start = truncated.text.len() - " [truncated]".len();
    truncated.truncated = true;
    seed_monitor_history(&mut app, vec![escaped, truncated]);
    tap_key(&mut app, KeyCode::F3);
    for _ in 0..3 {
        step_input(&mut app);
    }

    let world = app.world_mut();
    let rows: Vec<(u64, usize)> = {
        let mut query = world.query::<(&MonitorRow, &Children)>();
        query
            .iter(world)
            .map(|(row, children)| (row.0, children.len()))
            .collect()
    };
    assert!(rows.contains(&(0, 4)), "the ordinary grid has four columns");
    assert!(
        rows.contains(&(1, 1)),
        "the truncated event collapses to one line"
    );

    let truncated_row = {
        let mut query = world.query::<(&MonitorRow, Entity)>();
        query
            .iter(world)
            .find(|(row, _)| row.0 == 1)
            .map(|(_, entity)| entity)
            .expect("the truncated row")
    };
    let collapsed_child = world
        .get::<Children>(truncated_row)
        .expect("the collapsed row's one child")[0];
    let body_text = world
        .get::<Text>(collapsed_child)
        .expect("the collapsed line text")
        .as_str()
        .to_string();
    assert!(
        body_text.ends_with("[truncated]"),
        "the visible suffix: {body_text:?}"
    );
    assert!(!body_text.contains('\n'), "one logical row only");
    let warm = world
        .get::<TextColor>(collapsed_child)
        .expect("the collapsed line color")
        .0;
    assert_eq!(warm, crate::drawer::TONE_WARN, "the collapsed line is warm");
    assert_eq!(
        drawer_rows(&mut app)
            .iter()
            .find(|(sequence, part, _)| *sequence == 0 && *part == MonitorRowPart::Body)
            .map(|(_, _, text)| text.as_str()),
        Some("line one\\nline two \\\\n literal"),
        "real newline and literal backslash-n stay distinguishable in one row"
    );
}

/// A click outside the drawer neither closes it nor discards the suspended
/// draft; only the Monitor controls are inertly hit.
#[test]
fn outside_clicks_are_inert_while_open() {
    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let field = spawn_key_field(&mut app);
    type_character(&mut app, window, 'k');
    app.world_mut().resource_mut::<MenuState>().open = true;
    app.world_mut().resource_mut::<MenuState>().view = MenuView::Settings;
    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app));

    let outside = app.world_mut().spawn_empty().id();
    outside_press(&mut app, outside);

    assert!(
        drawer_open(&app),
        "an outside click does not close the drawer"
    );
    assert_eq!(field_value(&app, field), "k", "the draft survived");
    assert!(menu(&app).key_editing, "editing stayed suspended");
}

/// Reader choices are launch-local: hiding and reopening, the overlay, a save
/// and a load never reset visibility, filter or the remembered control.
#[test]
fn reader_choices_survive_hide_reopen_and_session_transitions() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    enter_play_with_input(&mut app);

    tap_key(&mut app, KeyCode::F3);
    activate_control(&mut app, MonitorControl::ShowEverything);
    tap_key(&mut app, KeyCode::Tab);
    assert_eq!(drawer_focus(&app), MonitorControl::FollowLatest);
    tap_key(&mut app, KeyCode::F3);
    assert!(!drawer_open(&app));

    tap_key(&mut app, KeyCode::F3);
    assert_eq!(
        drawer_view(&app),
        MonitorView::Everything,
        "filter survived"
    );
    assert_eq!(
        drawer_focus(&app),
        MonitorControl::FollowLatest,
        "focus survived"
    );
    tap_key(&mut app, KeyCode::F3);

    // The overlay and the player shortcuts are ordinary while Monitor is closed.
    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open);
    tap_key(&mut app, KeyCode::Escape);
    assert!(!menu(&app).open);
    tap_key(&mut app, KeyCode::F5);
    tap_key(&mut app, KeyCode::F9);
    assert_eq!(
        drawer_view(&app),
        MonitorView::Everything,
        "a load or New Game does not reset the Monitor view"
    );
    assert_eq!(drawer_focus(&app), MonitorControl::FollowLatest);
}

/// A competing claimant is never displaced: the drawer still opens, but leaves
/// the other claim's protection in place and releases nothing on close.
#[test]
fn a_competing_claim_is_not_stolen() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    enter_play_with_input(&mut app);
    app.world_mut()
        .resource_mut::<InputOwnership>()
        .claim(InputClaim("seam-test"));

    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app));
    assert_eq!(
        app.world().resource::<InputOwnership>().claimed_by(),
        Some(InputClaim("seam-test")),
        "the drawer never steals another claimant's input"
    );
    tap_key(&mut app, KeyCode::F3);
    assert_eq!(
        app.world().resource::<InputOwnership>().claimed_by(),
        Some(InputClaim("seam-test")),
        "closing releases only the drawer's own claim"
    );
}

/// The reader commands address the log anywhere in the drawer and never leak
/// into the app underneath or move the three-control focus cycle.
#[test]
fn reader_commands_scroll_without_leaking_into_play() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);
    let parked = wait(&app);
    tap_key(&mut app, KeyCode::F3);
    let focus = drawer_focus(&app);
    seed_numbered_monitor(&mut app, 0..100, "unmei::seq");
    settle_reader(&mut app);
    assert!(drawer_panel(&app).is_following());

    // Keyboard paging, Home and End address the log regardless of the focused
    // control.
    tap_key(&mut app, KeyCode::PageUp);
    assert!(
        !drawer_panel(&app).is_following(),
        "paging away disables following"
    );
    tap_key(&mut app, KeyCode::Home);
    assert_eq!(
        drawer_window(&mut app).first().copied(),
        Some(0),
        "Home selects the oldest displayed event"
    );
    tap_key(&mut app, KeyCode::End);
    assert!(
        drawer_panel(&app).is_following(),
        "End resumes following latest"
    );

    assert!(drawer_open(&app), "no reader command closes Monitor");
    assert_eq!(
        drawer_focus(&app),
        focus,
        "no reader command moves the control cycle"
    );
    assert_eq!(wait(&app), parked, "none of them leaked into play");
    assert!(!menu(&app).open, "none of them opened the overlay");

    // Unrelated keys still do nothing here or underneath.
    for key in [KeyCode::ArrowUp, KeyCode::ArrowDown] {
        tap_key(&mut app, key);
    }
    assert_eq!(drawer_focus(&app), focus);
    assert_eq!(wait(&app), parked);
    assert!(!menu(&app).open);

    // A wheel without a laid-out log body hit is swallowed, never leaked.
    app.world_mut()
        .write_message(bevy::input::mouse::MouseWheel {
            unit: bevy::input::mouse::MouseScrollUnit::Line,
            x: 0.0,
            y: -3.0,
            window: Entity::PLACEHOLDER,
            phase: bevy::input::touch::TouchPhase::Moved,
        });
    step_input(&mut app);
    assert!(drawer_panel(&app).is_following());
    assert_eq!(wait(&app), parked, "the wheel never leaked into play");
}

/// Keyboard activation is a fresh Enter/Space on the focused control; an OS
/// repeat or a held key activates once.
#[test]
fn controls_activate_by_fresh_keyboard_press_once() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    enter_play_with_input(&mut app);
    tap_key(&mut app, KeyCode::F3);
    assert_eq!(drawer_focus(&app), MonitorControl::ShowEverything);

    // A held Enter with repeats toggles once; the repeats arrive while the key
    // is still down, exactly as the OS sends them.
    press_key(&mut app, KeyCode::Enter);
    step_input(&mut app);
    assert_eq!(drawer_view(&app), MonitorView::Everything);
    for _ in 0..4 {
        send_key_message(
            &mut app,
            KeyCode::Enter,
            bevy::input::ButtonState::Pressed,
            true,
        );
        step_input(&mut app);
        assert_eq!(drawer_view(&app), MonitorView::Everything, "one activation");
    }
    release_key(&mut app, KeyCode::Enter);
    step_input(&mut app);

    // Space activates the focused control too.
    tap_key(&mut app, KeyCode::Space);
    assert_eq!(drawer_view(&app), MonitorView::UnmeiOnly);

    // Tab moves focus to Follow latest; Space is idempotent there.
    tap_key(&mut app, KeyCode::Tab);
    assert_eq!(drawer_focus(&app), MonitorControl::FollowLatest);
    tap_key(&mut app, KeyCode::Space);
    assert!(drawer_open(&app) && drawer_panel(&app).is_following());
}

/// Regaining window focus after a loss never replays activation, never quits
/// and adds no game pause.
#[test]
fn window_focus_regain_does_not_replay() {
    let pack = temp_pack();
    let (mut app, _window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let parked = wait(&app);
    tap_key(&mut app, KeyCode::F3);
    let focus = drawer_focus(&app);

    let set_focus = |app: &mut App, focused: bool| {
        let mut query = app
            .world_mut()
            .query_filtered::<&mut Window, With<PrimaryWindow>>();
        let mut window = query.single_mut(app.world_mut()).expect("the window");
        window.focused = focused;
    };
    set_focus(&mut app, false);
    step_input(&mut app);
    set_focus(&mut app, true);
    step_input(&mut app);

    assert!(
        drawer_open(&app),
        "the drawer survived the focus round trip"
    );
    assert_eq!(drawer_focus(&app), focus, "no activation was replayed");
    assert_eq!(wait(&app), parked, "no underlying action ran");
    assert_eq!(app.should_exit(), None, "no quit was manufactured");
}

/// Drive the runner to the demo pack's first player choice.
fn drive_runner_to_choice(runtime: &mut Runtime) {
    loop {
        let outcome = runtime.runner.run();
        runtime.staging.apply_all(&outcome.staged);
        match outcome.wait {
            Some(Wait::Choice { .. }) => return,
            Some(Wait::Say { .. }) => runtime.runner.advance_say().unwrap(),
            other => panic!("expected a player choice, got {other:?}"),
        }
    }
}

/// F3 opens over a player-choice wait and a decision wait; a digit pressed
/// while open never picks the choice.
#[test]
fn f3_reaches_a_choice_wait_without_picking_it() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    enter_play_with_input(&mut app);
    {
        let mut runtime = app.world_mut().resource_mut::<Runtime>();
        drive_runner_to_choice(&mut runtime);
    }
    app.update();
    let parked = wait(&app);
    assert!(
        matches!(parked, Some(Wait::Choice { .. })),
        "parked on the choice: {parked:?}"
    );

    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app), "F3 opens over a player choice");
    tap_key(&mut app, KeyCode::Digit1);
    assert_eq!(wait(&app), parked, "Monitor input never picks a choice");
    tap_key(&mut app, KeyCode::Escape);
    assert_eq!(wait(&app), parked, "closing never picks a choice either");
}

/// F3 opens over the overlay's Gallery/grid/still and Timeline views and changes
/// nothing about the underlying surface.
#[test]
fn f3_reaches_gallery_and_timeline_surfaces() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    enter_play_with_input(&mut app);
    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open, "the overlay is up");
    let focus_before = menu(&app).focus;

    for (view, still) in [
        (MenuView::Gallery, false),
        (MenuView::Gallery, true),
        (MenuView::Timeline, false),
    ] {
        app.world_mut().resource_mut::<MenuState>().view = view;
        app.world_mut().resource_mut::<MenuState>().gallery_still = still;
        step_input(&mut app);

        tap_key(&mut app, KeyCode::F3);
        assert!(drawer_open(&app), "F3 opens over {view:?} (still: {still})");
        tap_key(&mut app, KeyCode::Escape);
        assert!(
            !drawer_open(&app) && menu(&app).open,
            "Esc dismisses only Monitor"
        );
        assert_eq!(menu(&app).view, view, "the surface is unchanged");
        assert_eq!(menu(&app).gallery_still, still);
    }
    assert_eq!(menu(&app).focus, focus_before, "the selection is unchanged");
}

/// The grid's target column renders the escaped target; the original metadata
/// still drives filtering.
#[test]
fn the_target_column_renders_the_escaped_target() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    let mut bidi = test_monitor_line(0, "unmei::\u{202E}x", log::Level::Info, "bidi");
    bidi.target_display = "unmei::\\u{202e}x".to_string();
    seed_monitor_history(&mut app, vec![bidi]);
    tap_key(&mut app, KeyCode::F3);
    for _ in 0..3 {
        step_input(&mut app);
    }

    let target = drawer_rows(&mut app)
        .into_iter()
        .find(|(sequence, part, _)| *sequence == 0 && *part == MonitorRowPart::Target)
        .map(|(_, _, text)| text)
        .expect("the rendered target column");
    assert_eq!(
        target, "unmei::\\u{202e}x",
        "the raw bidi control never renders"
    );
    assert!(
        !target.contains('\u{202E}'),
        "no raw bidi override reaches the UI"
    );
    // The original metadata still matches the exact-prefix filter.
    assert_eq!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .targets(),
        vec!["unmei::\u{202E}x"]
    );
}

/// The queued-edit drain follows the ownership polarity exactly: while input is
/// blocked (claimed or quarantined) it consumes an edit the focus-change
/// producer queued, so the draft cannot mutate; once input is ready it is a
/// no-op, and the ordinary focus-loss discard then applies. Monitor never
/// *causes* a discard, but it does not hide a genuine focus loss after it stops
/// owning input.
#[test]
fn the_queued_edit_drain_follows_the_claim_polarity() {
    use bevy::input_focus::process_recorded_focus_changes;
    use bevy::text::apply_text_edits;

    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let field = spawn_key_field(&mut app);
    type_character(&mut app, window, 'k');
    assert_eq!(field_value(&app, field), "k");
    app.world_mut().resource_mut::<MenuState>().key_editing = true;

    // Queue the real focus-loss collapse edit through its real producer.
    app.world_mut()
        .resource_mut::<bevy::input_focus::InputFocus>()
        .clear();
    app.world_mut()
        .run_system_once(process_recorded_focus_changes)
        .expect("the focus-change producer runs");
    assert!(
        !app.world()
            .entity(field)
            .get::<EditableText>()
            .unwrap()
            .pending_edits
            .is_empty(),
        "the focus-loss producer queued its collapse edit"
    );

    // Blocked: the drain consumes it, the draft survives, and apply is gated.
    app.world_mut()
        .resource_mut::<InputOwnership>()
        .claim(InputClaim("seam-test"));
    app.world_mut()
        .run_system_once(crate::presentation::settings_view::discard_queued_key_edits_when_blocked)
        .expect("the drain runs");
    assert!(
        app.world()
            .entity(field)
            .get::<EditableText>()
            .unwrap()
            .pending_edits
            .is_empty(),
        "the blocked drain consumed the queued edit"
    );
    assert_eq!(field_value(&app, field), "k", "the draft is untouched");

    // Queue it again, then let the claim end: the drain no longer consumes, and
    // the ordinary apply performs the focus-loss discard.
    app.world_mut()
        .resource_mut::<bevy::input_focus::InputFocus>()
        .set(field, bevy::input_focus::FocusCause::Navigated);
    app.world_mut()
        .resource_mut::<bevy::input_focus::InputFocus>()
        .clear();
    app.world_mut()
        .run_system_once(process_recorded_focus_changes)
        .expect("the focus-change producer runs");
    app.world_mut().resource_mut::<InputOwnership>().release();
    app.world_mut()
        .run_system_once(crate::presentation::settings_view::discard_queued_key_edits_when_blocked)
        .expect("the drain runs");
    assert!(
        !app.world()
            .entity(field)
            .get::<EditableText>()
            .unwrap()
            .pending_edits
            .is_empty(),
        "a ready drain is a no-op"
    );
    app.world_mut()
        .run_system_once(apply_text_edits)
        .expect("the apply runs");
    assert_eq!(
        field_value(&app, field),
        "k",
        "a collapse edit moves the caret; it never erases the draft"
    );
    assert!(
        app.world()
            .entity(field)
            .get::<EditableText>()
            .unwrap()
            .pending_edits
            .is_empty(),
        "the ordinary apply consumed the queued edit"
    );

    // With the claim gone and focus genuinely off the field, the ordinary
    // focused-editing sync discards the draft on the next full frame.
    app.update();
    assert_eq!(
        field_value(&app, field),
        "",
        "the ordinary focus-loss discard applied after the release"
    );
    assert!(!menu(&app).key_editing, "editing left with the focus");
}

/// A real clipboard paste while Monitor is open never reaches the suspended
/// draft, and it is not replayed after close; a fresh paste afterwards applies.
#[test]
fn a_paste_while_monitor_is_open_never_reaches_the_draft() {
    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let field = spawn_key_field(&mut app);
    type_character(&mut app, window, 'k');
    app.world_mut().resource_mut::<MenuState>().key_editing = true;
    *app.world_mut().resource_mut::<ClipboardPort>() =
        ClipboardPort::Memory(Some("sk-pasted\n".into()));

    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app));
    paste(&mut app, window);
    assert_eq!(field_value(&app, field), "k", "the paste was blocked");

    tap_key(&mut app, KeyCode::Escape);
    assert!(!drawer_open(&app));
    step_input(&mut app);
    assert_eq!(
        field_value(&app, field),
        "k",
        "the claimed paste was never replayed"
    );

    // Release and a fresh paste is the ordinary path again.
    paste(&mut app, window);
    assert_eq!(
        field_value(&app, field),
        "ksk-pasted",
        "a fresh paste applies after release"
    );
}

// ---------------------------------------------------------------------------
// Monitor reader: stable older-line reading
// ---------------------------------------------------------------------------
//
// The reader is driven through the production paths: real `KeyboardInput`
// press/release/repeat for paging and Home/End, real `MouseWheel` messages
// hit-tested against the log body's own laid-out rect, and the real widget
// `Interaction` that starts a scrollbar drag. Assertions are on the rendered
// row window, the stable event anchor and the panel-local status.

/// A drawer harness with a real primary window, so cursor-driven wheel and
/// scrollbar hit-testing have a window to address. The world is otherwise the
/// same `RuntimeCorePlugin` one the other drawer tests use.
fn boot_reader_app(pack_root: PathBuf) -> App {
    let mut app = boot_app_with_input(pack_root, None);
    app.world_mut().spawn((Window::default(), PrimaryWindow));
    app
}

fn reader_anchor(app: &App) -> Option<drawer::MonitorAnchor> {
    drawer_panel(app).anchor()
}

fn scroll_drag_active(app: &App) -> bool {
    app.world()
        .resource::<drawer::MonitorScrollbarDrag>()
        .active
}

fn reader_node(app: &mut App, part: drawer::MonitorReaderNode) -> Entity {
    let world = app.world_mut();
    let mut query = world.query::<(Entity, &drawer::MonitorReaderNode)>();
    query
        .iter(world)
        .find(|(_, node)| **node == part)
        .map(|(entity, _)| entity)
        .expect("the reader node exists")
}

fn log_body_entity(app: &mut App) -> Entity {
    let world = app.world_mut();
    let mut query = world.query_filtered::<Entity, With<drawer::MonitorLogBody>>();
    query.iter(world).next().expect("the log body exists")
}

fn scrollbar_track_entity(app: &mut App) -> Entity {
    let world = app.world_mut();
    let mut query = world.query_filtered::<Entity, With<drawer::MonitorScrollbarTrack>>();
    query
        .iter(world)
        .next()
        .expect("the scrollbar track exists")
}

/// Place a drawer node's laid-out rect in the headless world exactly as the
/// production `ComputedNode`/`UiGlobalTransform` would, so cursor hit-testing
/// runs the same arithmetic.
fn place_reader_rect(app: &mut App, entity: Entity, center: Vec2, size: Vec2) {
    app.world_mut().entity_mut(entity).insert((
        bevy::ui::ComputedNode { size, ..default() },
        bevy::ui::UiGlobalTransform::from_translation(center),
    ));
}

fn set_reader_cursor(app: &mut App, position: Vec2) {
    let mut query = app
        .world_mut()
        .query_filtered::<&mut Window, With<PrimaryWindow>>();
    let mut window = query
        .single_mut(app.world_mut())
        .expect("the reader window");
    window.set_cursor_position(Some(position));
}

fn send_reader_wheel(app: &mut App, y: f32, unit: bevy::input::mouse::MouseScrollUnit) {
    app.world_mut()
        .write_message(bevy::input::mouse::MouseWheel {
            unit,
            x: 0.0,
            y,
            window: Entity::PLACEHOLDER,
            phase: bevy::input::touch::TouchPhase::Moved,
        });
}

/// Step frames until the current-predicate projection pass has settled.
fn settle_reader(app: &mut App) {
    for _ in 0..20 {
        step_input(app);
        if !app
            .world()
            .resource::<crate::drawer::MonitorProjection>()
            .is_updating()
        {
            return;
        }
    }
    panic!("the projection never settled");
}

fn seed_numbered_monitor(app: &mut App, range: std::ops::Range<u64>, target: &str) {
    seed_monitor_history(
        app,
        range
            .map(|i| test_monitor_line(i, target, log::Level::Info, &format!("event {i}")))
            .collect(),
    );
}

/// The materialized rows' admission sequences in display order, top row first
/// — the window the developer actually sees.
fn drawer_window(app: &mut App) -> Vec<u64> {
    let world = app.world_mut();
    let content = {
        let mut query = world.query_filtered::<Entity, With<drawer::MonitorRowsContent>>();
        query.iter(world).next().expect("the rows content exists")
    };
    let children: Vec<Entity> = world
        .get::<Children>(content)
        .map(|children| children.iter().collect())
        .unwrap_or_default();
    children
        .iter()
        .filter_map(|entity| world.get::<MonitorRow>(*entity).map(|row| row.0))
        .collect()
}

/// Drag the reader's scrollbar to a track fraction (`0.0` = top of the track,
/// `1.0` = bottom), the production navigation that establishes a deliberate
/// reading position. The track is laid out exactly as the 1280×720 drawer
/// draws it (y 212–505); a fresh press on the thumb starts the drag, the
/// cursor's landing place is the position, and the release ends it.
fn drag_reader_to(app: &mut App, track_fraction: f32) {
    let track = scrollbar_track_entity(app);
    place_reader_rect(app, track, Vec2::new(1251.0, 358.5), Vec2::new(10.0, 293.0));
    let thumb = reader_node(app, drawer::MonitorReaderNode::Thumb);
    set_reader_cursor(app, Vec2::new(1251.0, 212.0 + track_fraction * 293.0));
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.world_mut()
        .entity_mut(thumb)
        .insert(Interaction::Pressed);
    step_input(app);
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .release(MouseButton::Left);
    step_input(app);
    app.world_mut().entity_mut(thumb).insert(Interaction::None);
    step_input(app);
}

/// A rendered row's imposed height, by admission sequence.
fn row_node_height(app: &mut App, sequence: u64) -> Option<Val> {
    let world = app.world_mut();
    let mut query = world.query::<(&MonitorRow, &Node)>();
    query
        .iter(world)
        .find(|(row, _)| row.0 == sequence)
        .map(|(_, node)| node.height)
}

#[test]
fn wheel_scrolls_only_when_hit_tested_to_the_log_body() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    seed_numbered_monitor(&mut app, 0..100, "unmei::seq");
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    assert!(
        drawer_panel(&app).is_following(),
        "an enabled launch follows latest"
    );

    // The log body's rect at 1280x720: x 24-1256, y 212-505.
    let body = log_body_entity(&mut app);
    place_reader_rect(
        &mut app,
        body,
        Vec2::new(640.0, 358.5),
        Vec2::new(1232.0, 293.0),
    );

    // A wheel outside the body is swallowed: no scroll and no follow change.
    set_reader_cursor(&mut app, Vec2::new(40.0, 40.0));
    send_reader_wheel(&mut app, 1.0, bevy::input::mouse::MouseScrollUnit::Line);
    step_input(&mut app);
    assert!(
        drawer_panel(&app).is_following(),
        "outside-panel wheel never scrolls the log"
    );
    assert_eq!(drawer_window(&mut app).last().copied(), Some(99));

    // Hit-tested to the body, one notch moves three display lines older and
    // disables following; the anchor is a retained admission identity.
    set_reader_cursor(&mut app, Vec2::new(640.0, 358.5));
    send_reader_wheel(&mut app, 1.0, bevy::input::mouse::MouseScrollUnit::Line);
    step_input(&mut app);
    assert!(
        !drawer_panel(&app).is_following(),
        "scrolling away disables following"
    );
    let anchor = reader_anchor(&app).expect("the reader anchored");
    assert_eq!(anchor.sequence, 82, "the 15-line tail minus a 3-line notch");
    assert_eq!(
        drawer_window(&mut app).first().copied(),
        Some(82),
        "the anchor is the top visible event"
    );
    assert!(drawer_status(&mut app).contains("reading older lines"));
    assert!(!drawer_panel(&app).is_pending());

    // Arrivals never pull the older-lines reader to the tail.
    seed_numbered_monitor(&mut app, 0..105, "unmei::seq");
    settle_reader(&mut app);
    assert!(
        !drawer_panel(&app).is_following(),
        "new arrivals never enable follow"
    );
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(82),
        "the same event stays anchored"
    );
    assert_eq!(drawer_window(&mut app).first().copied(), Some(82));

    // Only a deliberate return to the bottom resumes following.
    for _ in 0..3 {
        send_reader_wheel(&mut app, -1.0, bevy::input::mouse::MouseScrollUnit::Line);
        step_input(&mut app);
    }
    assert!(
        drawer_panel(&app).is_following(),
        "a deliberate bottom arrival resumes follow"
    );
    assert!(drawer_status(&mut app).contains("following latest"));
    assert_eq!(drawer_window(&mut app).last().copied(), Some(104));
}

#[test]
fn page_home_end_and_conflicting_navigation_follow_the_contract() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    seed_numbered_monitor(&mut app, 0..200, "unmei::seq");
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    // Focus is irrelevant: paging always addresses the log.
    tap_key(&mut app, KeyCode::Tab);
    assert_eq!(drawer_focus(&app), MonitorControl::FollowLatest);

    // A fresh PgUp pages a full viewport older.
    press_key(&mut app, KeyCode::PageUp);
    step_input(&mut app);
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(170),
        "200 events minus a 15-line bottom viewport, one page older"
    );
    assert!(!drawer_panel(&app).is_following());

    // The held key does not repeat before the cadence, and does after it.
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(170),
        "no repeat before the paging cadence"
    );
    advance_time(&mut app, 0.12);
    step_input(&mut app);
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(155),
        "the held key repeats"
    );
    // A repeat is a reader-position change like any other: the window that
    // update draws starts at the reader's own anchor, so the frame displayed
    // after a slow-frame repeat is the repeat's own page, never an older one
    // (the stale-drawing class once mistaken for a drifting anchor).
    assert_eq!(
        drawer_window(&mut app).first().copied(),
        reader_anchor(&app).map(|anchor| anchor.sequence),
        "the drawn top row is the repeat's own anchor"
    );
    release_key(&mut app, KeyCode::PageUp);
    step_input(&mut app);

    // PgDn moves newer; reaching the bottom resumes following.
    tap_key(&mut app, KeyCode::PageDown);
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(170));
    assert!(!drawer_panel(&app).is_following());

    // Home selects the oldest displayed event; End returns to the newest.
    tap_key(&mut app, KeyCode::Home);
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(0));
    assert_eq!(drawer_window(&mut app).first().copied(), Some(0));
    assert!(!drawer_panel(&app).is_following());
    tap_key(&mut app, KeyCode::End);
    assert!(drawer_panel(&app).is_following());

    // A repeat is not a fresh press: End's fresh press runs once, and holding
    // it while Home reads older never re-enables follow.
    press_key(&mut app, KeyCode::End);
    step_input(&mut app);
    assert!(drawer_panel(&app).is_following());
    press_key(&mut app, KeyCode::Home);
    step_input(&mut app);
    assert!(!drawer_panel(&app).is_following());
    send_key_message(
        &mut app,
        KeyCode::End,
        bevy::input::ButtonState::Pressed,
        true,
    );
    step_input(&mut app);
    assert!(
        !drawer_panel(&app).is_following(),
        "an OS repeat is not a fresh End press"
    );
    release_key(&mut app, KeyCode::End);
    release_key(&mut app, KeyCode::Home);
    step_input(&mut app);

    // Simultaneous conflicting navigation is ignored whole.
    let anchor = reader_anchor(&app);
    press_key(&mut app, KeyCode::PageUp);
    press_key(&mut app, KeyCode::Home);
    step_input(&mut app);
    assert_eq!(
        reader_anchor(&app),
        anchor,
        "PgUp+Home conflicted and neither applied"
    );
    release_key(&mut app, KeyCode::PageUp);
    release_key(&mut app, KeyCode::Home);
    step_input(&mut app);

    // Esc beats scroll for the frame: a closing frame never scrolls away.
    tap_key(&mut app, KeyCode::End);
    assert!(drawer_panel(&app).is_following());
    press_key(&mut app, KeyCode::Escape);
    press_key(&mut app, KeyCode::PageUp);
    step_input(&mut app);
    assert!(!drawer_open(&app), "Esc closed Monitor");
    release_key(&mut app, KeyCode::Escape);
    release_key(&mut app, KeyCode::PageUp);
    step_input(&mut app);
    tap_key(&mut app, KeyCode::F3);
    assert!(
        drawer_panel(&app).is_following(),
        "the closing frame's PgUp never scrolled"
    );
}

#[test]
fn scrollbar_drag_positions_the_reader_and_cancels_safely() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    seed_numbered_monitor(&mut app, 0..100, "unmei::seq");
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);

    // The track's rect at 1280x720: x 1246-1256, y 212-505.
    let track = scrollbar_track_entity(&mut app);
    place_reader_rect(
        &mut app,
        track,
        Vec2::new(1251.0, 358.5),
        Vec2::new(10.0, 293.0),
    );
    let thumb = reader_node(&mut app, drawer::MonitorReaderNode::Thumb);

    // A fresh press on the thumb starts a drag; the thumb centre follows the
    // cursor to the track's middle: event 42 with half a line read into it.
    set_reader_cursor(&mut app, Vec2::new(1251.0, 358.5));
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.world_mut()
        .entity_mut(thumb)
        .insert(Interaction::Pressed);
    step_input(&mut app);
    let anchor = reader_anchor(&app).expect("the drag anchored the reader");
    assert_eq!(anchor.sequence, 42);
    assert!(
        (anchor.offset_lines - 0.5).abs() < 0.01,
        "the within-row offset is half the anchor row: {}",
        anchor.offset_lines
    );
    assert!(!drawer_panel(&app).is_following());
    assert!(scroll_drag_active(&app));

    // Dragging to the bottom of the travel is a deliberate bottom arrival.
    set_reader_cursor(&mut app, Vec2::new(1251.0, 505.0));
    step_input(&mut app);
    assert!(
        drawer_panel(&app).is_following(),
        "the bottom of the track resumes following"
    );
    assert!(scroll_drag_active(&app), "the button is still held");

    // The release ends the drag; reader state is preserved.
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .release(MouseButton::Left);
    step_input(&mut app);
    assert!(!scroll_drag_active(&app));
    app.world_mut().entity_mut(thumb).insert(Interaction::None);
    step_input(&mut app);

    // A close cancels an active drag, and a held pointer cannot resume it.
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.world_mut()
        .entity_mut(thumb)
        .insert(Interaction::Pressed);
    step_input(&mut app);
    assert!(scroll_drag_active(&app));
    tap_key(&mut app, KeyCode::F3);
    assert!(!drawer_open(&app));
    assert!(
        !scroll_drag_active(&app),
        "close cancels the scrollbar drag"
    );
    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app));
    assert!(
        !scroll_drag_active(&app),
        "a held pointer cannot resume the drag on reopen"
    );
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .release(MouseButton::Left);
    step_input(&mut app);

    // Window focus loss cancels an active drag but preserves the reader state.
    tap_key(&mut app, KeyCode::Home);
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(0));
    let thumb = reader_node(&mut app, drawer::MonitorReaderNode::Thumb);
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.world_mut()
        .entity_mut(thumb)
        .insert(Interaction::Pressed);
    step_input(&mut app);
    assert!(scroll_drag_active(&app));
    {
        let mut query = app
            .world_mut()
            .query_filtered::<&mut Window, With<PrimaryWindow>>();
        let mut window = query.single_mut(app.world_mut()).expect("the window");
        window.focused = false;
    }
    step_input(&mut app);
    assert!(
        !scroll_drag_active(&app),
        "focus loss cancels the scrollbar drag"
    );
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(0),
        "focus loss preserves the reader position"
    );
    {
        let mut query = app
            .world_mut()
            .query_filtered::<&mut Window, With<PrimaryWindow>>();
        let mut window = query.single_mut(app.world_mut()).expect("the window");
        window.focused = true;
    }
    step_input(&mut app);
    assert!(!scroll_drag_active(&app), "regain does not resume the drag");
}

/// Home chooses the oldest **currently displayed** matching event during an
/// incomplete refresh and keeps that event as older matches are prepended; it
/// does not silently retarget to the whole retained history (approved meaning,
/// finding B4).
#[test]
fn home_keeps_the_oldest_currently_displayed_event_through_the_refresh() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    seed_numbered_monitor(&mut app, 0..300, "unmei::seq");

    // One opening frame examines exactly the newest `PROJECTION_SCAN_BUDGET`
    // retained entries, so the oldest currently displayed event is derived from
    // the seeded history and the documented budget — not read back from the
    // projection the resolver then uses.
    press_key(&mut app, KeyCode::F3);
    step_input(&mut app);
    let oldest_displayed = 300 - crate::drawer::PROJECTION_SCAN_BUDGET as u64;
    assert_eq!(
        oldest_displayed, 172,
        "the opening pass examined 128 newest"
    );
    assert!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .is_updating(),
        "the refresh is still incomplete"
    );

    tap_key(&mut app, KeyCode::Home);
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(oldest_displayed),
        "Home selected the oldest currently displayed event"
    );

    // As the pass prepends older matches the selected event is preserved, not
    // retargeted to the newly arrived oldest retained event.
    settle_reader(&mut app);
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(oldest_displayed),
        "the selected event survives older matches being prepended"
    );
    assert_eq!(
        drawer_window(&mut app).first().copied(),
        Some(oldest_displayed)
    );
    assert!(!drawer_panel(&app).is_following());
}

#[test]
fn a_reading_anchor_survives_hide_reopen_and_clamps_to_its_rewrap() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    let mut lines: Vec<MonitorLine> = (0..100)
        .map(|i| test_monitor_line(i, "unmei::seq", log::Level::Info, &format!("event {i}")))
        .collect();
    // Event 42 is a long wrapped event, so it can be read part-way down.
    lines[42].text = format!("0000000042ms INFO  unmei::seq {}", "x".repeat(400));
    lines[42].body_start = lines[42].text.len() - 400;
    seed_monitor_history(&mut app, lines);
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);

    // A production scrollbar drag into the wrapped event establishes a
    // fractional reading position: the mid travel over 100 single-line rows
    // lands on event 42, and the four-line row admits an offset past one line.
    drag_reader_to(&mut app, 0.5);
    let anchor = reader_anchor(&app).expect("the drag anchored the reader");
    assert_eq!(anchor.sequence, 42);
    assert!(
        anchor.offset_lines > 1.0,
        "the within-row offset is fractional: {}",
        anchor.offset_lines
    );
    assert_eq!(drawer_window(&mut app).first().copied(), Some(42));

    // Hiding and reopening preserves the event and the reading offset.
    let offset = anchor.offset_lines;
    tap_key(&mut app, KeyCode::F3);
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    let anchor = reader_anchor(&app).expect("the anchor survived the reopen");
    assert_eq!(anchor.sequence, 42);
    assert!((anchor.offset_lines - offset).abs() < 0.01);
    assert_eq!(drawer_window(&mut app).first().copied(), Some(42));

    // A fullscreen-like resize rewraps the same event to one line, clamping
    // the offset inside its new extent instead of scrolling past it.
    {
        let mut query = app
            .world_mut()
            .query_filtered::<&mut Window, With<PrimaryWindow>>();
        let mut window = query.single_mut(app.world_mut()).expect("the window");
        window.resolution = bevy::window::WindowResolution::new(4000, 720);
    }
    settle_reader(&mut app);
    let anchor = reader_anchor(&app).expect("the anchor survived the rewrap");
    assert_eq!(anchor.sequence, 42, "the event is kept across a rewrap");
    assert!(
        anchor.offset_lines < 1.0,
        "the offset clamped into the one-line row: {}",
        anchor.offset_lines
    );
    assert_eq!(
        drawer_window(&mut app).first().copied(),
        Some(42),
        "the drawn top event is the clamped anchor"
    );

    // When the whole remaining tail fits the viewport, the offset drops to
    // zero rather than shifting the short content up.
    seed_numbered_monitor(&mut app, 42..45, "unmei::seq");
    settle_reader(&mut app);
    let anchor = reader_anchor(&app).expect("the anchor survived");
    assert_eq!(anchor.sequence, 42);
    assert_eq!(anchor.offset_lines, 0.0, "a fitting tail clamps to zero");
    assert_eq!(drawer_window(&mut app).first().copied(), Some(42));
}

#[test]
fn a_surviving_rows_imposed_height_follows_a_rewrap() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    // Event 5 is four lines at the 1280px column and one line at 4000px.
    let mut lines: Vec<MonitorLine> = (0..20)
        .map(|i| test_monitor_line(i, "unmei::seq", log::Level::Info, "short"))
        .collect();
    lines[5].text = format!("0000000005ms INFO  unmei::seq {}", "x".repeat(400));
    lines[5].body_start = lines[5].text.len() - 400;
    seed_monitor_history(&mut app, lines);
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);

    let before = row_node_height(&mut app, 5).expect("event 5 is rendered");
    {
        let mut query = app
            .world_mut()
            .query_filtered::<&mut Window, With<PrimaryWindow>>();
        let mut window = query.single_mut(app.world_mut()).expect("the window");
        window.resolution = bevy::window::WindowResolution::new(4000, 720);
    }
    settle_reader(&mut app);
    let after = row_node_height(&mut app, 5).expect("event 5 survived the rewrap");

    // The surviving row keeps its text but its imposed height is recomputed for
    // the wider column, so the shift/spacer/clamp cannot disagree with the row.
    assert!(
        matches!(after, Val::Px(value) if value < 20.0),
        "the rewrapped row collapsed to one line: {after:?}"
    );
    assert!(
        matches!(before, Val::Px(value) if value > 60.0),
        "the row was four lines before the rewrap: {before:?}"
    );
}

#[test]
fn an_evicted_anchor_clamps_to_the_oldest_match_and_acknowledges_expiry() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    seed_numbered_monitor(&mut app, 0..100, "unmei::seq");
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);

    tap_key(&mut app, KeyCode::PageUp);
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(70));

    // Retention evicts the anchored event.
    seed_numbered_monitor(&mut app, 80..180, "unmei::seq");
    settle_reader(&mut app);
    assert!(drawer_panel(&app).is_expired(), "eviction is acknowledged");
    assert!(
        !drawer_panel(&app).is_following(),
        "expiry keeps following off"
    );
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(80),
        "the anchor clamps to the oldest surviving matching event"
    );
    assert_eq!(drawer_window(&mut app).first().copied(), Some(80));
    assert!(drawer_status(&mut app).contains("older lines expired"));

    // The acknowledgement is panel-local, never a row, tracing event or Notice.
    assert!(
        !drawer_rows(&mut app)
            .iter()
            .any(|(_, _, text)| text.contains("expired")),
        "the expiry status is not a retained row"
    );
    assert!(app.world().resource::<Notice>().text.is_none());

    // Hide/reopen does not clear it; repeated eviction keeps it set.
    tap_key(&mut app, KeyCode::F3);
    tap_key(&mut app, KeyCode::F3);
    assert!(
        drawer_panel(&app).is_expired(),
        "hide/reopen preserves the acknowledgement"
    );
    seed_numbered_monitor(&mut app, 200..300, "unmei::seq");
    settle_reader(&mut app);
    assert!(
        drawer_panel(&app).is_expired(),
        "a repeated eviction keeps the acknowledgement set"
    );

    // A deliberate navigation clears it.
    tap_key(&mut app, KeyCode::PageUp);
    assert!(
        !drawer_panel(&app).is_expired(),
        "deliberate navigation acknowledges the loss"
    );

    // A filter change clears it too.
    seed_numbered_monitor(&mut app, 400..500, "unmei::seq");
    settle_reader(&mut app);
    assert!(drawer_panel(&app).is_expired());
    activate_control(&mut app, MonitorControl::ShowEverything);
    assert!(!drawer_panel(&app).is_expired());

    // `End` is a deliberate bottom arrival and acknowledges the loss as well.
    seed_numbered_monitor(&mut app, 500..600, "unmei::seq");
    settle_reader(&mut app);
    assert!(drawer_panel(&app).is_expired());
    tap_key(&mut app, KeyCode::End);
    assert!(drawer_panel(&app).is_following());
    assert!(
        !drawer_panel(&app).is_expired(),
        "End acknowledges the loss"
    );

    // And Follow latest resumes following and clears it.
    tap_key(&mut app, KeyCode::Home);
    seed_numbered_monitor(&mut app, 700..800, "unmei::seq");
    settle_reader(&mut app);
    assert!(drawer_panel(&app).is_expired());
    assert!(!drawer_panel(&app).is_following());
    activate_control(&mut app, MonitorControl::FollowLatest);
    assert!(drawer_panel(&app).is_following());
    assert!(!drawer_panel(&app).is_expired());
}

#[test]
fn filter_changes_keep_or_fall_back_to_matching_anchors() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    // Even sequences below 84 are the default view's matches; odd sequences
    // and everything at or above 84 are noise, so a non-matching anchor above
    // the last match exercises the nearest-earlier fallback.
    let lines: Vec<MonitorLine> = (0..100u64)
        .map(|i| {
            if i % 2 == 0 && i < 84 {
                test_monitor_line(i, "unmei::kept", log::Level::Info, &format!("kept {i}"))
            } else {
                test_monitor_line(i, "dep::noise", log::Level::Info, &format!("noise {i}"))
            }
        })
        .collect();
    seed_monitor_history(&mut app, lines);
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    tap_key(&mut app, KeyCode::PageUp);
    let anchored = reader_anchor(&app).expect("anchored").sequence;
    assert_eq!(anchored % 2, 0, "the anchor is a matching event");

    // A matching anchor survives the filter change, and an old-predicate row
    // never leaks into the new view.
    activate_control(&mut app, MonitorControl::ShowEverything);
    settle_reader(&mut app);
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(anchored),
        "a matching anchor is kept across the filter change"
    );
    assert_eq!(drawer_window(&mut app).first().copied(), Some(anchored));
    assert!(
        !drawer_panel(&app).is_following(),
        "a filter change preserves follow intent"
    );
    activate_control(&mut app, MonitorControl::ShowEverything);
    settle_reader(&mut app);
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(anchored)
    );
    assert!(
        drawer_rows(&mut app)
            .iter()
            .filter(|(_, part, _)| *part == MonitorRowPart::Target)
            .all(|(_, _, target)| target == "unmei::kept"),
        "only current-predicate rows are displayed"
    );

    // A non-matching anchor falls back to the first surviving matching newer
    // event. The drag lands on odd event 65: 100 rows give the mid-lower track
    // a position just inside 65 across the 85-line bottom range.
    activate_control(&mut app, MonitorControl::ShowEverything);
    settle_reader(&mut app);
    drag_reader_to(&mut app, 0.726);
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(65));
    activate_control(&mut app, MonitorControl::ShowEverything);
    settle_reader(&mut app);
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(66),
        "the first surviving matching newer event"
    );
    assert!(!drawer_panel(&app).is_following());

    // ...else the nearest preceding match. Event 83 is past the last match at
    // 82, so no newer match survives.
    activate_control(&mut app, MonitorControl::ShowEverything);
    settle_reader(&mut app);
    drag_reader_to(&mut app, 0.91);
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(83));
    activate_control(&mut app, MonitorControl::ShowEverything);
    settle_reader(&mut app);
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(82),
        "the nearest preceding match when no newer one survives"
    );
    assert!(!drawer_panel(&app).is_following());
}

#[test]
fn an_empty_filter_view_stays_empty_and_later_matches_never_enable_follow() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    seed_monitor_history(
        &mut app,
        (0..100)
            .map(|i| test_monitor_line(i, "dep::noise", log::Level::Info, &format!("noise {i}")))
            .collect(),
    );
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);

    // Anchor a dependency event under Show everything, then switch the display
    // predicate to `unmei::` only: there is no match at all. The drag's mid
    // travel over the 100 rows lands on event 42.
    activate_control(&mut app, MonitorControl::ShowEverything);
    settle_reader(&mut app);
    drag_reader_to(&mut app, 0.5);
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(42));
    activate_control(&mut app, MonitorControl::ShowEverything);
    settle_reader(&mut app);
    assert_eq!(reader_anchor(&app), None, "no matches, no anchor");
    assert!(!drawer_panel(&app).is_following());
    assert!(
        has_component::<drawer::MonitorEmptyState>(&mut app),
        "the empty view is shown, not a filter reset"
    );

    // A later matching arrival anchors at the oldest new match and never
    // silently enables follow.
    seed_monitor_history(
        &mut app,
        vec![test_monitor_line(
            100,
            "unmei::kept",
            log::Level::Info,
            "later",
        )],
    );
    settle_reader(&mut app);
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(100),
        "the first later match anchors the empty view"
    );
    assert!(
        !drawer_panel(&app).is_following(),
        "a later arrival never silently enables follow"
    );
    assert_eq!(drawer_window(&mut app).first().copied(), Some(100));

    // Further arrivals accumulate below the anchored event, not at the tail.
    seed_monitor_history(
        &mut app,
        (100..105)
            .map(|i| test_monitor_line(i, "unmei::kept", log::Level::Info, "later"))
            .collect(),
    );
    settle_reader(&mut app);
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(100));
    assert!(!drawer_panel(&app).is_following());
    assert!(drawer_status(&mut app).contains("reading older lines"));
}

/// A retained anchor that the restarted pass has not reached yet must not be
/// presented as a "no matches" empty view: the body stays blank under the
/// truthful `updating view` until the anchor is located.
#[test]
fn a_filter_refresh_never_shows_a_false_empty_state_while_locating_the_anchor() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    let mut lines: Vec<MonitorLine> = (0..100)
        .map(|i| test_monitor_line(i, "unmei::kept", log::Level::Info, &format!("kept {i}")))
        .collect();
    lines.extend(
        (100..228)
            .map(|i| test_monitor_line(i, "dep::noise", log::Level::Info, &format!("noise {i}"))),
    );
    seed_monitor_history(&mut app, lines);
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    tap_key(&mut app, KeyCode::PageUp);
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(70));

    // Show everything keeps the matching anchor.
    activate_control(&mut app, MonitorControl::ShowEverything);
    settle_reader(&mut app);
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(70));

    // Back to `unmei::` only: the restart's first pass examines the newest 128
    // entries — all dependency noise — so there are zero current-predicate rows
    // while the retained anchor (70) is still unscanned.
    let control = drawer_control_entity(&mut app, MonitorControl::ShowEverything);
    app.world_mut()
        .entity_mut(control)
        .insert(Interaction::Pressed);
    step_input(&mut app);
    assert!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .is_updating(),
        "the restarted pass is still running"
    );
    assert!(
        drawer_panel(&app).is_pending(),
        "the retained anchor is still being located"
    );
    assert!(
        !has_component::<drawer::MonitorEmptyState>(&mut app),
        "no false empty state while the anchor is unscanned"
    );
    assert!(drawer_status(&mut app).contains("updating view"));

    // The pass completes and the same anchor reappears.
    app.world_mut()
        .entity_mut(control)
        .insert(Interaction::None);
    step_input(&mut app);
    settle_reader(&mut app);
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(70),
        "the anchor survived the refresh"
    );
    assert!(!drawer_panel(&app).is_pending());
}

/// A restarted pass that already holds positive current-predicate rows must not
/// present the matching tail as the older non-following reader's location. The
/// retained anchor is older than everything the first pass examined, so the
/// body stays on the locating placeholder until the anchor is reached, then
/// jumps straight to it — never to the tail and back (finding A07).
#[test]
fn a_pending_refresh_never_presents_the_matching_tail_as_the_olders_location() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    // Even sequences match the default view; odd sequences are dependency noise.
    let lines: Vec<MonitorLine> = (0..1000u64)
        .map(|i| {
            if i % 2 == 0 {
                test_monitor_line(i, "unmei::kept", log::Level::Info, &format!("kept {i}"))
            } else {
                test_monitor_line(i, "dep::noise", log::Level::Info, &format!("noise {i}"))
            }
        })
        .collect();
    seed_monitor_history(&mut app, lines);
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    // The drag's travel over the 500 matching rows is 485 lines deep, so a
    // track fraction of 0.2155 lands just inside the 100th match — retained
    // event 200.
    drag_reader_to(&mut app, 0.2155);
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(200));

    // Toggle the view to restart the pass: its first window is the newest 128
    // retained entries (872..999), which hold positive `unmei::` matches, while
    // the retained anchor 200 is still unscanned.
    activate_control(&mut app, MonitorControl::ShowEverything);
    let projection = app.world().resource::<crate::drawer::MonitorProjection>();
    assert!(projection.is_updating(), "the restarted pass is incomplete");
    assert!(
        projection.shown() > 0,
        "the incomplete first pass already holds positive current-predicate rows"
    );
    assert!(
        drawer_panel(&app).is_pending(),
        "the retained anchor is still being located"
    );
    assert!(
        drawer_window(&mut app).is_empty(),
        "the matching tail is not shown as the older reader's location"
    );
    assert!(
        has_component::<drawer::MonitorLocatingState>(&mut app),
        "the unresolved position is truthfully represented"
    );
    assert!(drawer_status(&mut app).contains("updating view"));
    assert!(drawer_status(&mut app).contains("reading older lines"));

    // The pass reaches the anchor and the display jumps straight to it.
    settle_reader(&mut app);
    assert!(!drawer_panel(&app).is_pending());
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(200));
    assert_eq!(drawer_window(&mut app).first().copied(), Some(200));
}

#[test]
fn automatic_clamps_never_enable_follow() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    // Rows below 70 are `unmei::` matches; everything above is noise, so a
    // noise anchor sits past the last match.
    let lines: Vec<MonitorLine> = (0..100u64)
        .map(|i| {
            if i < 70 {
                test_monitor_line(i, "unmei::kept", log::Level::Info, "kept")
            } else {
                test_monitor_line(i, "dep::noise", log::Level::Info, "noise")
            }
        })
        .collect();
    seed_monitor_history(&mut app, lines);
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);

    // Anchor a noise event past every match under Show everything (the drag
    // lands near event 80), then switch back: the nearest preceding match is
    // the projection's newest row — the automatic fallback must not enable
    // follow.
    activate_control(&mut app, MonitorControl::ShowEverything);
    settle_reader(&mut app);
    drag_reader_to(&mut app, 0.88);
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(80));
    activate_control(&mut app, MonitorControl::ShowEverything);
    settle_reader(&mut app);
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(69),
        "the nearest preceding match"
    );
    assert!(
        !drawer_panel(&app).is_following(),
        "an automatic bottom fallback never enables follow"
    );

    // The same holds for an eviction clamp to a single surviving match at the
    // tail of the projection.
    let tail: Vec<MonitorLine> = (100..120u64)
        .map(|i| {
            if i == 119 {
                test_monitor_line(i, "unmei::kept", log::Level::Info, "kept")
            } else {
                test_monitor_line(i, "dep::noise", log::Level::Info, "noise")
            }
        })
        .collect();
    seed_monitor_history(&mut app, tail);
    settle_reader(&mut app);
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(119),
        "the only surviving matching event"
    );
    assert!(
        !drawer_panel(&app).is_following(),
        "the eviction clamp never enables follow"
    );
    assert!(drawer_panel(&app).is_expired());
}

#[test]
fn reader_scrolling_and_filter_refresh_stay_within_the_settled_budgets() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    seed_numbered_monitor(&mut app, 0..1000, "unmei::seq");
    tap_key(&mut app, KeyCode::F3);
    for _ in 0..20 {
        step_input(&mut app);
        let work = app.world().resource::<MonitorWork>();
        assert!(work.scanned <= PROJECTION_SCAN_BUDGET);
        assert!(work.text_ops <= TEXT_WORK_BUDGET);
        if !app
            .world()
            .resource::<crate::drawer::MonitorProjection>()
            .is_updating()
        {
            break;
        }
    }
    assert!(!app
        .world()
        .resource::<crate::drawer::MonitorProjection>()
        .is_updating());

    // Paging a deep history never scans or materializes beyond the budget.
    for _ in 0..4 {
        tap_key(&mut app, KeyCode::PageUp);
        let work = app.world().resource::<MonitorWork>();
        assert!(work.scanned <= PROJECTION_SCAN_BUDGET);
        assert!(work.text_ops <= TEXT_WORK_BUDGET);
    }
    // A settled reading frame does no metadata scan and no text work.
    step_input(&mut app);
    assert_eq!(app.world().resource::<MonitorWork>().scanned, 0);
    assert_eq!(app.world().resource::<MonitorWork>().text_ops, 0);

    // A filter refresh cancels obsolete work, stays inside the budgets and
    // shows only current-predicate rows at every point.
    activate_control(&mut app, MonitorControl::ShowEverything);
    for _ in 0..20 {
        step_input(&mut app);
        let work = app.world().resource::<MonitorWork>();
        assert!(work.scanned <= PROJECTION_SCAN_BUDGET);
        assert!(work.text_ops <= TEXT_WORK_BUDGET);
        if !app
            .world()
            .resource::<crate::drawer::MonitorProjection>()
            .is_updating()
        {
            break;
        }
    }
    assert_eq!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .shown(),
        1000
    );
    assert_eq!(
        app.world().resource::<MonitorHistory>().retained(),
        1000,
        "the reader never retains a second history"
    );
}

#[test]
fn reader_position_and_expiry_survive_session_transitions() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    seed_numbered_monitor(&mut app, 0..100, "unmei::seq");
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    tap_key(&mut app, KeyCode::PageUp);
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(70));
    seed_numbered_monitor(&mut app, 80..180, "unmei::seq");
    settle_reader(&mut app);
    assert!(drawer_panel(&app).is_expired());
    let anchor = reader_anchor(&app);

    // Hide/reopen preserves the reader choice.
    tap_key(&mut app, KeyCode::F3);
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    assert!(!drawer_panel(&app).is_following());
    assert_eq!(
        reader_anchor(&app),
        anchor,
        "the anchor survives hide/reopen"
    );
    assert!(drawer_panel(&app).is_expired());

    // The overlay transition and the ordinary manual save/load preserve it too.
    tap_key(&mut app, KeyCode::F3);
    assert!(!drawer_open(&app));
    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open);
    tap_key(&mut app, KeyCode::Escape);
    assert!(!menu(&app).open);
    tap_key(&mut app, KeyCode::F5);
    tap_key(&mut app, KeyCode::F9);
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    assert!(
        !drawer_panel(&app).is_following(),
        "a load does not reset the reader choice"
    );
    assert_eq!(reader_anchor(&app), anchor, "the anchor survives a load");
    assert!(
        drawer_panel(&app).is_expired(),
        "the expiry acknowledgement survives a load"
    );

    // A real player-facing New Game transition (confirm once) preserves the
    // reader state while performing its own session reset.
    tap_key(&mut app, KeyCode::F3);
    assert!(!drawer_open(&app));
    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open);
    click_input(&mut app, MenuAction::NewGame);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::NewGame));
    click_input(&mut app, MenuAction::ConfirmAccept);
    assert!(!menu(&app).open, "accepting New Game starts play");
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    assert!(
        !drawer_panel(&app).is_following(),
        "a New Game does not reset the reader choice"
    );
    assert_eq!(
        reader_anchor(&app),
        anchor,
        "the anchor survives a New Game"
    );
    assert!(
        drawer_panel(&app).is_expired(),
        "the expiry acknowledgement survives a New Game"
    );
}

/// A real Timeline jump is a position replacement; the reader's anchor, follow
/// choice and expiry acknowledgement survive it while the underlying session
/// performs its own reset. This needs a pack with declared timeline scenes, so
/// it runs on a synthetic one rather than the auto-discovered demo pack.
#[test]
fn reader_position_and_expiry_survive_a_timeline_jump() {
    let pack = temp_custom_pack(
        &[("a", "A: entry.\n"), ("b", "B: b.\n")],
        Some(&["a", "b"]),
        "a",
    );
    let root = pack.path().join("pack");
    seed_visited(&root, &["a"]);
    let mut app = boot_reader_app(root);
    enter_play_with_input(&mut app);
    seed_numbered_monitor(&mut app, 0..100, "unmei::seq");
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    tap_key(&mut app, KeyCode::PageUp);
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(70));
    seed_numbered_monitor(&mut app, 80..180, "unmei::seq");
    settle_reader(&mut app);
    assert!(drawer_panel(&app).is_expired());
    let anchor = reader_anchor(&app);

    // The jump goes through the real overlay, Timeline view, card press and
    // confirmation while the drawer is closed.
    tap_key(&mut app, KeyCode::F3);
    assert!(!drawer_open(&app));
    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open);
    click_input(&mut app, MenuAction::OpenTimeline);
    assert_eq!(menu(&app).view, MenuView::Timeline);
    click_input(&mut app, MenuAction::TimelineCard(0));
    assert_eq!(
        menu(&app).confirm,
        Some(ConfirmKind::TimelineJump(0)),
        "the visited entry scene asks to confirm the jump"
    );
    click_input(&mut app, MenuAction::ConfirmAccept);
    assert!(!menu(&app).open, "accepting jumps to the timeline scene");

    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    assert!(
        !drawer_panel(&app).is_following(),
        "a Timeline jump does not reset the reader choice"
    );
    assert_eq!(
        reader_anchor(&app),
        anchor,
        "the anchor survives a Timeline jump"
    );
    assert!(
        drawer_panel(&app).is_expired(),
        "the expiry acknowledgement survives a Timeline jump"
    );
}

/// Regression for the projection's arrivals band: once a batch of arrivals has
/// established the new block, the next batch above it must still be examined.
/// The following view is the observable outcome — the newest matching arrival
/// is emitted, not silently skipped.
#[test]
fn later_arrivals_above_an_established_band_are_projected() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    seed_numbered_monitor(&mut app, 0..100, "unmei::seq");
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);

    seed_numbered_monitor(&mut app, 0..105, "unmei::seq");
    settle_reader(&mut app);
    assert_eq!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .sequences()
            .last()
            .copied(),
        Some(104),
        "the first arrival batch is projected"
    );
    let _ = drawer_window(&mut app);

    // A second batch above the established band is new work, not something an
    // earlier pass already reviewed.
    seed_numbered_monitor(&mut app, 0..110, "unmei::seq");
    settle_reader(&mut app);
    assert_eq!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .sequences()
            .last()
            .copied(),
        Some(109),
        "arrivals above the established band are examined"
    );
    assert_eq!(drawer_window(&mut app).last().copied(), Some(109));
    assert!(drawer_panel(&app).is_following());
}

/// A third batch, arriving above the band a *previous* pass established, is
/// examined whole. The pass must resume the arrivals band from the last
/// examined entry rather than restart at the newest arrival: treating the
/// newest as reviewed would otherwise mark every older, still-unexamined
/// arrival in the same batch as already reviewed and drop it from the
/// projection forever. The rendered acceptance walk caught the
/// drawer advancing one row per update while the rest of every batch was
/// silently lost.
#[test]
fn a_third_arrival_batch_above_an_established_band_is_projected() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    seed_numbered_monitor(&mut app, 0..100, "unmei::seq");
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);

    // Establish the arrivals band above the old block.
    seed_numbered_monitor(&mut app, 0..105, "unmei::seq");
    settle_reader(&mut app);
    assert_eq!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .sequences()
            .last()
            .copied(),
        Some(104),
        "the first arrival batch established the band"
    );

    // A whole second arrival batch above that band, in one pass.
    seed_numbered_monitor(&mut app, 0..130, "unmei::seq");
    settle_reader(&mut app);
    let projection = app.world().resource::<crate::drawer::MonitorProjection>();
    assert_eq!(projection.shown(), 130, "every arrival is projected");
    assert_eq!(
        projection.sequences().last().copied(),
        Some(129),
        "the newest arrival is projected"
    );
    assert_eq!(
        drawer_window(&mut app).last().copied(),
        Some(129),
        "the newest arrival is rendered"
    );
}

/// Successive 128-event arrival batches under an incomplete refresh leave no
/// unexamined gap: the projection extends its examined band contiguously from
/// the entry it last stopped on, so the settled projection equals the
/// independently selected current matching retained identities (finding A05).
/// The fixture slides the retained window as the batches arrive, so the pass
/// also prunes entries the window has moved past.
#[test]
fn arrival_batches_under_an_incomplete_refresh_settle_complete() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    seed_numbered_monitor(&mut app, 0..1000, "unmei::seq");
    tap_key(&mut app, KeyCode::F3);
    step_input(&mut app);
    assert!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .is_updating(),
        "the opening pass left the older block incomplete"
    );

    // Three 128-event batches arrive while the older block is still incomplete,
    // sliding the retained window forward by 128 each time (the fixture seeds
    // the retained set directly; real retention eviction and its counter are
    // covered by the capture suites). The pass budget for arrivals is smaller
    // than each batch, so the gap between batches is where A05 lost identities.
    for end in [1128u64, 1256, 1384] {
        seed_numbered_monitor(&mut app, (end - 1000)..end, "unmei::seq");
        step_input(&mut app);
    }
    settle_reader(&mut app);

    // Independently select the current matching retained identities; do not
    // mirror the projection's own interval arithmetic.
    let expected: Vec<u64> = app
        .world()
        .resource::<MonitorHistory>()
        .lines()
        .iter()
        .filter(|line| line.target.starts_with("unmei::"))
        .map(|line| line.sequence)
        .collect();
    let projected = app
        .world()
        .resource::<crate::drawer::MonitorProjection>()
        .sequences();
    assert_eq!(
        projected, expected,
        "every retained matching identity is projected"
    );
    assert_eq!(projected, (384..1384).collect::<Vec<_>>());

    // A settled band does no metadata scan and no text work at all.
    step_input(&mut app);
    let work = app.world().resource::<MonitorWork>();
    assert_eq!(work.scanned, 0, "settled frames scan nothing");
    assert_eq!(work.text_ops, 0, "settled frames materialize nothing");
}
