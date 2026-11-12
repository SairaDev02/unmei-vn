//! The play surface's own tests, beside the caches they exercise.
//!
//! `use super::*` reaches the surface's own items; these three are the
//! neighbours the tests read out of the app.

use crate::settings::{self, SettingsState};
use std::collections::BTreeMap;
use unmei_engine::runner::Wait;

use super::*;
use crate::tests::{app_for_pack, press_and_update, temp_pack};
use bevy::ecs::system::RunSystemOnce;

/// A load must request a replay even when the caches look already applied:
/// the precondition is armed directly because the headless core app does
/// not run the play sync systems that would consume a prior `force`.
#[test]
fn load_signals_presentation_to_replay_visuals_and_audio() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::F5);

    // Simulate the presentation layer having already applied the current
    // staging, so a load must *force* a rebuild rather than no-op.
    app.world_mut().resource_mut::<AppliedVisuals>().force = false;
    app.world_mut().resource_mut::<AppliedAudio>().force = false;
    app.world_mut().resource_mut::<AppliedUi>().ready = true;

    press_and_update(&mut app, KeyCode::F9);

    assert!(app.world().resource::<AppliedVisuals>().is_forced());
    assert!(app.world().resource::<AppliedAudio>().is_forced());
    assert!(!app.world().resource::<AppliedUi>().is_ready());
}

/// The volume path drives its own bookkeeping but must not disturb the
/// play/stop cache, and no channel means nothing is applied at all.
#[test]
fn volume_changes_never_disturb_the_play_bookkeeping() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    // A track already playing through `sync_audio`: a volume change must
    // not disturb the play/stop bookkeeping, so the loop is never
    // restarted. New Game legitimately forces a replay; clear that here so
    // this test isolates the volume path.
    app.world_mut().resource_mut::<AppliedAudio>().bgm = Some("music_evening".into());
    app.world_mut().resource_mut::<AppliedAudio>().force = false;
    {
        let mut state = app.world_mut().resource_mut::<SettingsState>();
        state.set_fraction(settings::AudioControl::Master, 0.5);
        state.set_fraction(settings::AudioControl::Music, 0.5);
    }
    app.world_mut().run_system_once(sync_audio_volume).unwrap();
    // The dB mapping itself is unit-tested in `settings.rs`; here the point
    // is that the volume path touches neither `AppliedAudio` (which drives
    // stop/play) nor a channel that is absent.
    assert_eq!(
        app.world().resource::<AppliedVolume>().bgm,
        0.0,
        "without a channel nothing is applied"
    );
    let playbook = app.world().resource::<AppliedAudio>();
    assert_eq!(
        playbook.bgm.as_deref(),
        Some("music_evening"),
        "a volume change never stops or restarts the track"
    );
    assert!(!playbook.force);

    app.world_mut()
        .resource_mut::<SettingsState>()
        .toggle_mute();
    app.world_mut().run_system_once(sync_audio_volume).unwrap();
    let playbook = app.world().resource::<AppliedAudio>();
    assert_eq!(
        playbook.bgm.as_deref(),
        Some("music_evening"),
        "muting never stops or restarts the track"
    );
}

/// The play surface's one replay request (ADR 0013): a single call puts the
/// audio, visual and dialogue/choice caches back to "apply everything from
/// the top", exactly as the load, New Game and Timeline-restart call sites
/// need. The caches are left looking already applied, which is the state
/// that made the three pre-partition call sites force a replay.
#[test]
fn one_replay_request_resets_audio_visuals_and_ui_together() {
    let mut audio = AppliedAudio {
        bgm: Some("music_morning".to_string()),
        force: false,
    };
    let mut visuals = AppliedVisuals {
        bg: Some("school_gate".to_string()),
        sprites: BTreeMap::new(),
        force: false,
    };
    visuals.sprites.insert("a".to_string(), "idle".to_string());
    let mut ui = AppliedUi {
        wait: Some(Wait::Say {
            speaker: "A".to_string(),
            text: "one.".to_string(),
        }),
        ready: true,
    };

    request_replay(&mut audio, &mut visuals, &mut ui);

    assert_eq!(audio.bgm, None);
    assert!(audio.force, "audio replays from the top");
    assert_eq!(visuals.bg, None);
    assert!(visuals.sprites.is_empty());
    assert!(
        visuals.force,
        "the sprites rebuild from the staging snapshot"
    );
    assert_eq!(ui.wait, None);
    assert!(!ui.ready, "the dialogue band and choice buttons rebuild");
}
