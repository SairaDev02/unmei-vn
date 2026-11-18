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
