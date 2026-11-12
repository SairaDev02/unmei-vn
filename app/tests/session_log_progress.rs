//! End-to-end session-log check for the three progress lines an automated walk
//! waits on: `unmei: title ready`, `unmei: entered scene
//! <id>` and `unmei: saved to <path>`.
//!
//! These run through the same route as `session_log.rs`: Bevy's real
//! `LogPlugin`, the `app_log_layer` custom layer and the `<pack-dir>.log` file
//! sink, over the real core systems and real key input. It is its own test
//! binary with a single test for the same reason that file has one:
//! `LogPlugin` installs a process-global subscriber/logger, so a second
//! `LogPlugin` in one binary would be ignored and write nothing.

use std::path::{Path, PathBuf};

use bevy::app::App;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use unmei_app::failure::{app_log_layer, session_log_path_for, SessionLog};
use unmei_app::menu::MenuState;
use unmei_app::{Runtime, RuntimeConfig, RuntimeCorePlugin};
use unmei_engine::save::{save_path_for, Slot};

/// A normal-mode walk over the demo pack writes one line per progress moment,
/// and the boot park behind the title is not one of them.
#[test]
fn a_normal_mode_walk_writes_the_three_progress_lines() {
    let dir = tempfile::tempdir().expect("temp dir");
    let pack_root = dir.path().join("pack");
    copy_dir(&demo_pack_root(), &pack_root);

    // The app's entry point computes `<pack-dir>.log` and truncates it before
    // the pack load; mirror that here.
    let log_path = session_log_path_for(&pack_root);
    let log = SessionLog::open(log_path.clone(), &unmei_app::launch::version_text())
        .expect("open the session log");

    let mut app = App::new();
    app.insert_resource(log)
        // Normal mode: F5/F9 and the Monitor are unavailable, so the Esc overlay
        // is the only save surface and the keyboard is the only input.
        .insert_resource(RuntimeConfig::new(pack_root, None).with_dev_mode(false))
        .add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .add_plugins(LogPlugin {
            custom_layer: app_log_layer,
            ..Default::default()
        })
        .add_plugins(RuntimeCorePlugin);
    app.update(); // Startup: load the pack, run the boot park, open the title

    let booted = read_log(&log_path);
    assert!(
        booted.contains("unmei: title ready"),
        "the title's first show is one waitable line: {booted}"
    );
    assert_eq!(
        entered(&booted),
        Vec::<String>::new(),
        "the boot park runs behind the title, so it enters no play scene: {booted}"
    );

    // `Return` on the title's New Game starts play at the entry scene; the demo
    // packs have no autosave, so it is immediate and confirm-free.
    assert!(app.world().resource::<MenuState>().title_up());
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(scene_id(&app), "first_meeting");
    assert_eq!(
        entered(&read_log(&log_path)),
        ["first_meeting"],
        "starting play enters the entry scene"
    );

    // Two says, the choice, then the crossing into `reunion`; the offline
    // decision fallback resolves in its frame, and one more advance crosses
    // into `epilogue`.
    press_and_update(&mut app, KeyCode::Space); // say 1 -> say 2
    press_and_update(&mut app, KeyCode::Space); // say 2 -> choice
    press_and_update(&mut app, KeyCode::Digit1); // ask_letter say
    press_and_update(&mut app, KeyCode::Space); // -> reunion
    assert_eq!(scene_id(&app), "reunion");
    press_and_update(&mut app, KeyCode::Space); // -> decision fallback say
    press_and_update(&mut app, KeyCode::Space); // -> epilogue
    assert_eq!(scene_id(&app), "epilogue");

    let walked = read_log(&log_path);
    assert_eq!(
        entered(&walked),
        ["first_meeting", "reunion", "epilogue"],
        "every scene entry says so, once, in the order play entered it: {walked}"
    );

    // The Esc overlay's Save over an empty card is immediate: no confirmation.
    press_and_update(&mut app, KeyCode::Escape);
    press_and_update(&mut app, KeyCode::Enter);
    let slot1 = save_path_for(
        app.world().resource::<Runtime>().runner.pack(),
        Slot::Manual(1),
    );
    assert!(slot1.exists(), "the overlay's Save wrote slot 1");
    let saved = read_log(&log_path);
    assert!(
        saved.contains(&format!("unmei: saved to {}", slot1.display())),
        "an overlay save announces the path it wrote, exactly as F5 does: {saved}"
    );
    assert_eq!(
        entered(&saved).len(),
        3,
        "the save entered no scene: {saved}"
    );

    // The overlay's Load of the slot just written restores its scene, which is
    // a scene entry like any other.
    press_and_update(&mut app, KeyCode::Tab); // card -> Save tab
    press_and_update(&mut app, KeyCode::Tab); // Save tab -> Load tab
    press_and_update(&mut app, KeyCode::Enter); // the grid switches to Load
    press_and_update(&mut app, KeyCode::Digit1);
    press_and_update(&mut app, KeyCode::Enter); // Load asks its one confirm
    press_and_update(&mut app, KeyCode::Enter); // accept
    let loaded = read_log(&log_path);
    assert!(
        loaded.contains(&format!("unmei: loaded {}", slot1.display())),
        "the load names the slot it read: {loaded}"
    );
    assert_eq!(
        entered(&loaded),
        ["first_meeting", "reunion", "epilogue", "epilogue"],
        "a restored save announces the scene it lands in: {loaded}"
    );
}

// ---------------------------------------------------------------------------
// Fixtures and log readers
// ---------------------------------------------------------------------------

fn demo_pack_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/demo-pack")
        .canonicalize()
        .expect("demo pack path exists")
}

/// Copy the demo pack into the temp dir, so the walk's slots and read log land
/// beside the copy instead of dirtying the repo.
fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create the pack copy");
    for entry in std::fs::read_dir(from).expect("read the demo pack") {
        let entry = entry.expect("pack entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("entry type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy a pack file");
        }
    }
}

fn read_log(path: &Path) -> String {
    std::fs::read_to_string(path).expect("read the session log")
}

/// Every `unmei: entered scene <id>` line's `<id>`, in the order it was written.
fn entered(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.split_once("unmei: entered scene "))
        .map(|(_, scene_id)| scene_id.trim().to_string())
        .collect()
}

fn scene_id(app: &App) -> String {
    app.world()
        .resource::<Runtime>()
        .runner
        .scene_id()
        .to_string()
}

fn press_and_update(app: &mut App, key: KeyCode) {
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
    app.update();
    // Fully reset so the same key can be pressed again next frame.
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
}
