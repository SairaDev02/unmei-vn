//! The take's in-process seams: a test-only plugin reaches the two rules the
//! shipping binary cannot install (spec §15 rules 23 and 24).
//!
//! Rule 23 is "a test-only plugin in a test build", and rule 24's guard only
//! fires for a world that never quiets, which no pack can produce. Both tests
//! therefore add a plugin through `run_with` and drive a real take.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use bevy::prelude::*;
use bevy_kira_audio::{AudioChannel, AudioPlugin};
use unmei_engine::keiko::{parse_case, Case};
use unmei_engine::{load_pack, Pack, StateValue};

use super::{run, run_with, TakeRequest};
use crate::keiko::finding::rules;
use crate::presentation::play::Bgm;
use crate::RuntimeConfig;

/// The demo pack, copied to a temp dir so the take's sandbox and sidecars never
/// touch the repository.
fn demo_pack() -> (tempfile::TempDir, PathBuf, Pack) {
    let dir = crate::tests::temp_pack();
    let root = dir.path().join("pack");
    let loaded = load_pack(&root).expect("the demo pack loads");
    (dir, root, loaded.pack)
}

fn request<'a>(case: &'a Case, pack: &'a Pack, root: &'a Path) -> TakeRequest<'a> {
    TakeRequest {
        case,
        case_file: "tests/probe.keiko".to_string(),
        original_pack: root,
        pack,
        take_index: 1,
        keep: false,
        live: None,
        frames: None,
    }
}

/// A system that panics on its first run (spec §15 rule 23).
struct PanicOnce;

impl Plugin for PanicOnce {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, |mut fired: Local<bool>| {
            if !*fired {
                *fired = true;
                panic!("the test panicked on purpose");
            }
        });
    }
}

/// A system that changes the observed Notice every frame once the case's
/// `state` header has put `letters_found = 1` into the runner (spec §15 rule
/// 24). The gate keeps the earlier settles — the title and the first park,
/// which run before the `start` header's state override — from spending the
/// budget: the 600 frames the counter sees are exactly the failing settle's.
struct TickNotice {
    frames: Arc<AtomicU32>,
}

impl Plugin for TickNotice {
    fn build(&self, app: &mut App) {
        let frames = self.frames.clone();
        app.add_systems(
            PostUpdate,
            move |runtime: Option<Res<crate::Runtime>>,
                  notice: Option<ResMut<crate::failure::Notice>>| {
                let Some(runtime) = runtime else {
                    return;
                };
                let Some(mut notice) = notice else {
                    return;
                };
                let gated = matches!(
                    runtime.runner.state().get("letters_found"),
                    Some(StateValue::Num(value)) if *value == 1.0
                );
                if !gated {
                    return;
                }
                let frame = frames.fetch_add(1, Ordering::Relaxed) + 1;
                notice.show(format!("tick {frame}"));
            },
        );
    }
}

/// Delays a key check's verdict by wall time once one is in flight, so the
/// released verdict cannot land within the frames settling runs. This is a
/// starved machine made deterministic.
struct SlowKeyCheck;

impl Plugin for SlowKeyCheck {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            |mut pending: ResMut<crate::core::PendingKeyCheck>, mut wrapped: Local<bool>| {
                if *wrapped {
                    return;
                }
                let Some(original) = pending.task.take() else {
                    return;
                };
                *wrapped = true;
                pending.task = Some(bevy::tasks::AsyncComputeTaskPool::get().spawn(async move {
                    std::thread::sleep(std::time::Duration::from_millis(300));
                    original.await
                }));
            },
        );
    }
}

#[test]
fn a_released_key_check_lands_before_the_take_settles() {
    let (_dir, root, pack) = demo_pack();
    let case = parse_case(
        "tests/key.keiko",
        "keiko 1
fault key-check hold
setting play.text_speed 0

activate title.settings
         activate settings.key
type \"sk-nano-abcdefghijklmnop\"
press Enter
release-key-check
",
    )
    .expect("the case parses");
    let result = run_with(request(&case, &pack, &root), vec![SlowKeyCheck]);

    result.outcome.expect("the take passes");
    let row = result
        .observation
        .controls
        .iter()
        .find(|control| control.id == "settings.key")
        .expect("the key row");
    assert!(
        !row.label.starts_with("Checking"),
        "the settled observation shows the verdict, got `{}`",
        row.label
    );
}

#[test]
fn a_panicking_system_ends_the_take_with_keiko_panic_and_the_next_take_passes() {
    let (_dir, root, pack) = demo_pack();
    let booming = parse_case("tests/boom.keiko", "keiko 1\n").expect("the case parses");
    let result = run_with(request(&booming, &pack, &root), vec![PanicOnce]);

    let finding = result
        .outcome
        .expect_err("a system that panics ends the take");
    assert_eq!(finding.rule, rules::PANIC);
    assert!(
        finding.message.contains("the test panicked on purpose"),
        "the finding carries the panic's message, got `{}`",
        finding.message
    );

    // The boundary leaves nothing behind: a fresh take in the same process
    // builds its own app, reaches play and reads the pack's first park.
    let afterwards =
        parse_case("tests/after.keiko", "keiko 1\n\nnew-game\n").expect("the case parses");
    let second = run(request(&afterwards, &pack, &root));
    let error = second.outcome.err().map(|finding| finding.message);
    assert!(error.is_none(), "the next take passes, got {error:?}");
    assert_eq!(second.observation.surface, "play");
    assert_eq!(
        second.observation.text.as_deref(),
        Some("Morning light. A folded letter on the ground, half under a shoe.")
    );
}

/// A plugin that panics while the app is still being set up.
struct PanicAtSetup;

impl Plugin for PanicAtSetup {
    fn build(&self, _app: &mut App) {
        panic!("the setup panicked on purpose");
    }
}

#[test]
fn a_panic_while_setting_up_the_app_is_a_keiko_panic_and_removes_the_sandbox() {
    let (_dir, root, pack) = demo_pack();
    let case = parse_case("tests/boom.keiko", "keiko 1\n").expect("the case parses");
    let mut asked = request(&case, &pack, &root);
    asked.keep = true;
    let result = run_with(asked, vec![PanicAtSetup]);

    let finding = result.outcome.expect_err("a setup panic ends the take");
    assert_eq!(finding.rule, rules::PANIC);
    assert!(finding.message.contains("the setup panicked on purpose"));
    let kept = result.kept.expect("--keep reports the folder");
    assert!(kept.exists(), "a kept sandbox is not removed");
    std::fs::remove_dir_all(kept.parent().unwrap()).unwrap();
}

#[test]
fn a_world_that_changes_every_frame_ends_unsettled_at_the_600_frame_guard() {
    let (_dir, root, pack) = demo_pack();
    let ticking = parse_case(
        "tests/tick.keiko",
        "keiko 1\nstart first_meeting\nstate letters_found = 1\n",
    )
    .expect("the case parses");
    let frames = Arc::new(AtomicU32::new(0));
    let result = run_with(
        request(&ticking, &pack, &root),
        vec![TickNotice {
            frames: frames.clone(),
        }],
    );

    let finding = result
        .outcome
        .expect_err("a world that never quiets ends the take");
    assert_eq!(finding.rule, rules::UNSETTLED);
    assert_eq!(
        finding.message,
        "the world did not become quiet within 600 frames"
    );
    assert_eq!(
        frames.load(Ordering::Relaxed),
        600,
        "the guard spends its whole budget before giving up"
    );
}

#[test]
fn a_start_case_whose_pack_copy_fails_to_load_names_the_pack_failure() {
    let (_dir, root, pack) = demo_pack();
    std::fs::write(root.join("pack.ron"), "(not a pack").unwrap();
    let case =
        parse_case("tests/start.keiko", "keiko 1\nstart first_meeting\n").expect("the case parses");
    let finding = run(request(&case, &pack, &root))
        .outcome
        .expect_err("a pack that does not load cannot start a scene");
    assert!(
        finding.message.contains("unmei could not load the pack"),
        "{finding:?}"
    );
    assert!(finding.message.contains("pack.ron"), "{finding:?}");
}

#[test]
fn a_take_boots_with_no_audio_device() {
    let (_dir, root, pack) = demo_pack();
    let app = crate::composition::keiko_app(
        RuntimeConfig::new(root.clone(), None),
        crate::failure::SessionLogResult::NotAttempted,
        crate::credentials::ClipboardPort::Memory(None),
        crate::KeyCheckChannel::default(),
        crate::DecisionFactory { builder: None },
        Arc::new(crate::keiko::assets::AssetFaults::default()),
    );
    assert!(
        !app.is_plugin_added::<AudioPlugin>(),
        "spec §4.1: a take has no audio device"
    );
    // The player's audio systems still have the resources they read: the two
    // channels the mix and the staging bookkeeping touch, and the decoded-track
    // asset the play sync loads.
    assert!(app.world().contains_resource::<AudioChannel<Bgm>>());
    assert!(app
        .world()
        .contains_resource::<Assets<bevy_kira_audio::AudioSource>>());

    // A take whose entry scene starts a track still boots and stages it, and
    // the decoder settles it with no row-5 asset failure.
    let case = parse_case(
        "tests/no-device.keiko",
        "keiko 1

new-game
",
    )
    .expect("parses");
    let result = run(request(&case, &pack, &root));
    assert!(result.outcome.is_ok(), "{:?}", result.outcome);
    assert_eq!(
        result.observation.staging.music.as_deref(),
        Some("music_morning")
    );
    assert!(
        !result.log.iter().any(|line| line.row == 5),
        "no row-5 asset failure, got {:?}",
        result.log
    );
}

/// Another app in the same process that keeps reporting one funnel row until it
/// is told to stop, the way a concurrently running app test does.
fn spawn_polluter(
    stop: Arc<std::sync::atomic::AtomicBool>,
) -> (std::sync::mpsc::Receiver<()>, std::thread::JoinHandle<u32>) {
    let (ready, started) = std::sync::mpsc::channel();
    let handle = std::thread::spawn(move || {
        let mut app = App::new();
        app.init_resource::<crate::failure::Notice>()
            .init_resource::<crate::failure::DefensiveHold>()
            .add_systems(Update, |mut failures: crate::failure::Failures| {
                failures.report(crate::failure::Failure::new(
                    crate::failure::FailureRow::BackgroundAsset,
                    "polluter: another app's line",
                ));
            });
        let mut updates = 0;
        while !stop.load(Ordering::Relaxed) {
            app.update();
            updates += 1;
            if updates == 1 {
                let _ = ready.send(());
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        updates
    });
    (started, handle)
}

#[test]
fn a_take_records_only_its_own_apps_failure_lines() {
    let (_dir, root, pack) = demo_pack();
    let case = parse_case("tests/own-lines.keiko", "keiko 1\n\nnew-game\n").expect("parses");

    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (started, polluter) = spawn_polluter(stop.clone());
    started
        .recv_timeout(std::time::Duration::from_secs(60))
        .expect("the polluting app reported its first line");
    let result = run(request(&case, &pack, &root));
    stop.store(true, Ordering::Relaxed);
    let updates = polluter.join().expect("the polluting app ran");

    assert!(updates > 0, "the other app reported while the take ran");
    assert!(result.outcome.is_ok(), "{:?}", result.outcome);
    let leaked: Vec<_> = result
        .log
        .iter()
        .filter(|line| line.text.contains("polluter"))
        .collect();
    assert!(
        leaked.is_empty(),
        "the take's log holds {} of another app's lines, first {:?}",
        leaked.len(),
        leaked.first()
    );
}

/// Another take app in the same process that logs a marker line every frame,
/// the way a concurrently running take test does. It signals once it has
/// logged, and stops when told to (or after a minute, if the test died first).
/// It returns its own session log's text.
fn spawn_logging_take(
    stop: Arc<std::sync::atomic::AtomicBool>,
) -> (
    std::sync::mpsc::Receiver<()>,
    std::thread::JoinHandle<String>,
) {
    let (ready, started) = std::sync::mpsc::channel();
    let handle = std::thread::spawn(move || {
        let (dir, root, _pack) = demo_pack();
        let path = dir.path().join("other.log");
        let log = crate::failure::SessionLog::open(path.clone(), "other take")
            .expect("open the other take's log");
        let mut app = crate::composition::keiko_app(
            RuntimeConfig::new(root, None),
            crate::failure::SessionLogResult::Opened(log),
            crate::credentials::ClipboardPort::Memory(None),
            crate::KeyCheckChannel::default(),
            crate::DecisionFactory { builder: None },
            Arc::new(crate::keiko::assets::AssetFaults::default()),
        );
        app.add_systems(Update, || {
            bevy::log::warn!("marker: another take's line");
        });
        let began = std::time::Instant::now();
        app.update();
        let _ = ready.send(());
        while !stop.load(Ordering::Relaxed) && began.elapsed().as_secs() < 60 {
            app.update();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        drop(app);
        std::fs::read_to_string(&path).expect("read the other take's log")
    });
    (started, handle)
}

/// A take installs no global log subscriber, so its `<pack>.log` holds the
/// version header and nothing else: not its own tracing lines, and not those of
/// another take running in the same process. Its log assertions read the
/// app's own funnel recorder instead (spec §4.1).
#[test]
fn a_kept_takes_session_log_holds_only_its_version_header() {
    let (_dir, root, pack) = demo_pack();
    let case = parse_case("tests/own-log.keiko", "keiko 1\n\nnew-game\n").expect("parses");

    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (started, other) = spawn_logging_take(stop.clone());
    started
        .recv_timeout(std::time::Duration::from_secs(60))
        .expect("the other take logged its first frame");
    let mut kept_request = request(&case, &pack, &root);
    kept_request.keep = true;
    let result = run(kept_request);
    stop.store(true, Ordering::Relaxed);
    let other_log = other.join().expect("the other take ran");

    let kept = result.kept.expect("a kept take names its folder");
    let text = std::fs::read_to_string(kept.join("pack.log"));
    let _ = std::fs::remove_dir_all(&kept);
    if let Some(sandbox_root) = kept.parent() {
        let _ = std::fs::remove_dir(sandbox_root);
    }
    let text = text.expect("the kept take's session log exists");
    assert!(result.outcome.is_ok(), "{:?}", result.outcome);
    assert_eq!(
        text,
        format!("{}\n", crate::launch::version_text()),
        "the kept take's session log holds only its version header"
    );
    assert_eq!(
        other_log, "other take\n",
        "the logging take's own lines do not reach its session log either"
    );
}

/// A pack whose first scene parks on a choice of ten options, each landing on
/// its own say-line so a settled line names the option taken.
fn ten_option_pack() -> (tempfile::TempDir, PathBuf, Pack) {
    let dir = tempfile::tempdir().expect("a temp dir");
    let root = dir.path().join("ten");
    std::fs::create_dir_all(root.join("scenes")).unwrap();
    std::fs::create_dir_all(root.join("npcs")).unwrap();
    std::fs::write(root.join("pack.ron"), "(name: \"ten\", entry: \"a\")").unwrap();
    std::fs::write(root.join("state.ron"), "(met: false,)").unwrap();
    let mut scene = String::from("Narrator: entry.\nchoice\n");
    for n in 1..=10 {
        scene.push_str(&format!("  \"Option {n}\" -> o{n}\n"));
    }
    scene.push_str("end\n");
    for n in 1..=10 {
        scene.push_str(&format!("label o{n}\nNarrator: took {n}.\njump done\n"));
    }
    scene.push_str("label done\nNarrator: done.\n");
    std::fs::write(root.join("scenes/a.unmei"), scene).unwrap();
    let loaded = load_pack(&root).expect("the pack loads");
    (dir, root, loaded.pack)
}

#[test]
fn a_pick_by_text_past_option_nine_is_no_target_and_presses_no_key() {
    let (_dir, root, pack) = ten_option_pack();
    let case = parse_case(
        "tests/ten.keiko",
        "keiko 1\nsetting play.text_speed 0\n\nnew-game\nadvance\npick \"Option 10\"\n",
    )
    .expect("the case parses");
    let result = run(request(&case, &pack, &root));
    let finding = result.outcome.expect_err("option 10 has no key");
    assert_eq!(finding.rule, rules::STEP_NO_TARGET, "{finding:?}");
    assert_eq!(
        result.observation.choices.len(),
        10,
        "no key was pressed, so the choice is still pending"
    );
}
