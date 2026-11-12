//! The settings file pattern, its clamping, the pure mapping, and the flush
//! posture — the witnesses this module's contract rests on.

use super::*;
use bevy::window::{MonitorSelection, WindowMode};

use crate::failure::{DefensiveHold, Notice};

fn temp_pack() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("pack")).unwrap();
    dir
}

#[test]
fn the_settings_path_is_the_packs_sibling_never_inside_it() {
    let dir = temp_pack();
    let pack = dir.path().join("pack");
    let path = settings_path_for(&pack);
    assert_eq!(path, dir.path().join("pack.settings.ron"));
    assert!(!path.starts_with(&pack), "never a path inside the pack");
}

#[test]
fn a_missing_file_is_defaults_and_writes_nothing() {
    let dir = temp_pack();
    let pack = dir.path().join("pack");
    let loaded = load(&pack);
    assert_eq!(loaded.settings, Settings::default());
    assert!(loaded.warning.is_none());
    assert!(
        !settings_path_for(&pack).exists(),
        "loading creates nothing"
    );
}

#[test]
fn round_trip_keeps_the_four_audio_values_and_no_play_key() {
    let dir = temp_pack();
    let pack = dir.path().join("pack");
    let mut settings = Settings::default();
    settings.audio.master = 0.5;
    settings.audio.music = 0.25;
    settings.audio.sfx = 0.75;
    settings.audio.muted = true;
    save(&pack, &settings).unwrap();

    let text = std::fs::read_to_string(settings_path_for(&pack)).unwrap();
    assert!(!text.contains("play"), "an audio-only file has no play key");
    assert!(text.contains("audio"));

    let reloaded = load(&pack);
    assert!(reloaded.warning.is_none());
    assert_eq!(reloaded.settings, settings);
}

#[test]
fn a_missing_key_keeps_its_siblings() {
    let dir = temp_pack();
    let pack = dir.path().join("pack");
    std::fs::write(
        settings_path_for(&pack),
        "(audio: (music: 0.25, muted: true))",
    )
    .unwrap();

    let loaded = load(&pack);
    assert_eq!(loaded.settings.audio.music, 0.25);
    assert!(loaded.settings.audio.muted);
    assert_eq!(loaded.settings.audio.master, 1.0, "missing key -> default");
    assert_eq!(loaded.settings.audio.sfx, 1.0);
}

#[test]
fn malformed_and_wrong_typed_files_are_all_defaults_and_bytes_untouched() {
    for bytes in [
        "this is not ron",
        "(audio: (master: \"loud\"))",
        "(audio: (muted: 3))",
    ] {
        let dir = temp_pack();
        let pack = dir.path().join("pack");
        std::fs::write(settings_path_for(&pack), bytes).unwrap();

        let loaded = load(&pack);
        assert_eq!(loaded.settings, Settings::default(), "{bytes:?}");
        assert!(loaded.warning.is_some(), "{bytes:?} warns");
        assert_eq!(
            std::fs::read_to_string(settings_path_for(&pack)).unwrap(),
            bytes,
            "the bytes are untouched"
        );
    }
}

#[test]
fn out_of_range_clamps_and_non_finite_takes_the_field_default() {
    let dir = temp_pack();
    let pack = dir.path().join("pack");
    std::fs::write(
        settings_path_for(&pack),
        "(audio: (master: 3.5, music: -1.0, sfx: NaN))",
    )
    .unwrap();

    let loaded = load(&pack);
    assert_eq!(loaded.settings.audio.master, 1.0, "out of range clamps");
    assert_eq!(loaded.settings.audio.music, 0.0, "out of range clamps");
    assert_eq!(loaded.settings.audio.sfx, 1.0, "NaN -> field default");
    assert!(loaded.warning.is_none(), "a parseable file does not warn");
}

#[test]
fn unknown_keys_are_tolerated_on_read_and_dropped_by_the_next_write() {
    let dir = temp_pack();
    let pack = dir.path().join("pack");
    std::fs::write(
        settings_path_for(&pack),
        "(audio: (master: 0.5, future: 9), extra: (x: 1))",
    )
    .unwrap();

    let loaded = load(&pack);
    assert_eq!(loaded.settings.audio.master, 0.5);
    save(&pack, &loaded.settings).unwrap();

    let text = std::fs::read_to_string(settings_path_for(&pack)).unwrap();
    assert!(!text.contains("future"), "{text}");
    assert!(!text.contains("extra"), "{text}");
    assert!(text.contains("master"));
}

#[test]
fn a_non_default_play_section_is_preserved_and_an_all_default_one_is_omitted() {
    let dir = temp_pack();
    let pack = dir.path().join("pack");

    let mut settings = Settings::default();
    settings.play.auto = true;
    save(&pack, &settings).unwrap();
    let text = std::fs::read_to_string(settings_path_for(&pack)).unwrap();
    assert!(text.contains("play"), "{text}");
    assert!(load(&pack).settings.play.auto);

    save(&pack, &Settings::default()).unwrap();
    let text = std::fs::read_to_string(settings_path_for(&pack)).unwrap();
    assert!(!text.contains("play"), "{text}");
}

#[test]
fn track_fraction_maps_the_track_centre_corners_and_overshoot() {
    assert_eq!(track_fraction(0.0), 0.5, "the centre is half");
    assert_eq!(track_fraction(-0.5), 0.0, "the left end is zero");
    assert_eq!(track_fraction(0.5), 1.0, "the right end is one");
    assert_eq!(track_fraction(-2.0), 0.0, "a drag past the left clamps");
    assert_eq!(track_fraction(9.0), 1.0, "a drag past the right clamps");
}

#[test]
fn a_pointer_outside_the_track_snaps_to_nothing() {
    assert_eq!(pointer_fraction(0.0), Some(0.5));
    assert_eq!(pointer_fraction(-0.5), Some(0.0), "the left end is in");
    assert_eq!(pointer_fraction(0.5), Some(1.0), "the right end is in");
    assert_eq!(
        pointer_fraction(-0.51),
        None,
        "the label has no track under it"
    );
    assert_eq!(
        pointer_fraction(0.9),
        None,
        "the value text has no track under it"
    );
}

#[test]
fn mapping_is_master_times_channel_with_mute_forcing_silence() {
    assert!((effective_amplitude(0.5, 0.5, false) - 0.25).abs() < 1e-6);
    assert_eq!(effective_amplitude(1.0, 0.0, false), 0.0);
    assert_eq!(effective_amplitude(1.0, 1.0, true), 0.0);
    assert_eq!(effective_amplitude(0.0, 1.0, true), 0.0);
}

#[test]
fn amplitude_maps_to_decibels_with_silence_special_cased() {
    let db = amplitude_to_decibels(0.25);
    assert!((db - (-12.0412)).abs() < 0.01, "≈ -12 dB, got {db}");
    assert_eq!(amplitude_to_decibels(1.0), 0.0);
    assert_eq!(amplitude_to_decibels(0.0), SILENCE_DECIBELS);
    assert_eq!(amplitude_to_decibels(f32::NAN), SILENCE_DECIBELS);
    assert_eq!(amplitude_to_decibels(f32::INFINITY), SILENCE_DECIBELS);
}

#[test]
fn sliders_step_and_snap_on_the_five_percent_grid() {
    let dir = temp_pack();
    let mut state = SettingsState::new(dir.path().join("pack"), Settings::default());

    state.set_fraction(AudioControl::Master, 0.52);
    assert_eq!(state.percent(AudioControl::Master), 50, "snaps to the grid");
    state.set_fraction(AudioControl::Master, 0.53);
    assert_eq!(state.percent(AudioControl::Master), 55);

    state.step(AudioControl::Music, -1);
    assert_eq!(state.percent(AudioControl::Music), 95);
    for _ in 0..40 {
        state.step(AudioControl::Music, -1);
    }
    assert_eq!(state.percent(AudioControl::Music), 0, "clamped at 0");
    for _ in 0..40 {
        state.step(AudioControl::Sfx, 1);
    }
    assert_eq!(state.percent(AudioControl::Sfx), 100, "clamped at 100");
}

#[test]
fn mute_preserves_the_stored_levels_and_reset_touches_only_audio() {
    let dir = temp_pack();
    let mut state = SettingsState::new(dir.path().join("pack"), Settings::default());
    state.set_fraction(AudioControl::Master, 0.5);
    state.settings.play.text_speed = 0.04;

    state.toggle_mute();
    assert!(state.muted());
    assert_eq!(state.percent(AudioControl::Master), 50, "levels preserved");

    state.reset_audio();
    assert!(!state.muted());
    assert_eq!(state.percent(AudioControl::Master), 100);
    assert_eq!(
        state.settings.play.text_speed, 0.04,
        "reset never reaches into play"
    );
}

#[test]
fn out_of_range_and_non_finite_text_speed_clamp_to_the_ladder_ends() {
    for (bytes, expected) in [
        ("(play: (text_speed: 3.5))", MAX_TEXT_SPEED),
        ("(play: (text_speed: -1.0))", 0.0),
        ("(play: (text_speed: NaN))", 0.0),
        ("(play: (text_speed: inf))", 0.0),
    ] {
        let dir = temp_pack();
        let pack = dir.path().join("pack");
        std::fs::write(settings_path_for(&pack), bytes).unwrap();

        let loaded = load(&pack);
        assert_eq!(loaded.settings.play.text_speed, expected, "{bytes}");
        assert!(loaded.warning.is_none(), "a parseable value does not warn");
    }
}

#[test]
fn a_play_section_with_only_auto_keeps_siblings_and_the_next_write_emits_all() {
    let dir = temp_pack();
    let pack = dir.path().join("pack");
    std::fs::write(settings_path_for(&pack), "(play: (auto: true))").unwrap();

    let loaded = load(&pack);
    assert!(loaded.settings.play.auto);
    assert_eq!(
        loaded.settings.play.text_speed, 0.0,
        "missing key -> default"
    );
    assert!(!loaded.settings.play.fullscreen);

    save(&pack, &loaded.settings).unwrap();
    let text = std::fs::read_to_string(settings_path_for(&pack)).unwrap();
    for key in ["text_speed", "auto", "fullscreen"] {
        assert!(text.contains(key), "the rewrite emits {key}: {text}");
    }
}

#[test]
fn text_speed_walks_the_named_ladder_and_clamps_at_the_ends() {
    let dir = temp_pack();
    let mut state = SettingsState::new(dir.path().join("pack"), Settings::default());
    assert_eq!(state.text_speed_name(), "Instant");

    for expected in [
        ("Fast", 0.012),
        ("Normal", 0.02),
        ("Slow", 0.04),
        ("Slowest", 0.08),
    ] {
        state.step_text_speed(1);
        assert_eq!(state.text_speed_name(), expected.0);
        assert_eq!(state.text_speed(), expected.1);
    }
    state.step_text_speed(1);
    assert_eq!(state.text_speed_name(), "Slowest", "clamped at the top");
    assert_eq!(state.text_speed(), MAX_TEXT_SPEED);

    for _ in 0..10 {
        state.step_text_speed(-1);
    }
    assert_eq!(state.text_speed_name(), "Instant", "clamped at the bottom");
    assert_eq!(state.text_speed(), 0.0);
    assert!(state.dirty);
}

#[test]
fn text_speed_fraction_snaps_to_the_same_ladder_steps() {
    let dir = temp_pack();
    let mut state = SettingsState::new(dir.path().join("pack"), Settings::default());
    for (fraction, name) in [
        (0.0, "Instant"),
        (0.12, "Instant"),
        (0.13, "Fast"),
        (0.5, "Normal"),
        (0.75, "Slow"),
        (1.0, "Slowest"),
        (-3.0, "Instant"),
        (9.0, "Slowest"),
        (f32::NAN, "Instant"),
    ] {
        state.set_text_speed_fraction(fraction);
        assert_eq!(state.text_speed_name(), name, "fraction {fraction}");
    }
    state.set_text_speed_fraction(1.0);
    assert_eq!(state.text_speed_percent(), 100);
}

#[test]
fn text_speed_round_trips_through_the_play_section() {
    let dir = temp_pack();
    let pack = dir.path().join("pack");
    let mut state = SettingsState::new(pack.clone(), Settings::default());
    state.step_text_speed(3);
    assert_eq!(state.text_speed(), 0.04);
    save(&pack, &state.settings).unwrap();

    let text = std::fs::read_to_string(settings_path_for(&pack)).unwrap();
    assert!(text.contains("text_speed: 0.04"), "{text}");
    let reloaded = load(&pack).settings;
    assert_eq!(reloaded.play.text_speed, 0.04);
    assert_eq!(reloaded.audio, AudioSettings::default());
}

#[test]
fn a_wrong_typed_play_field_is_all_defaults_warns_and_leaves_the_bytes() {
    let dir = temp_pack();
    let pack = dir.path().join("pack");
    let bytes = "(audio: (master: 0.5), play: (auto: 1))";
    std::fs::write(settings_path_for(&pack), bytes).unwrap();

    let loaded = load(&pack);
    assert_eq!(loaded.settings, Settings::default(), "all defaults");
    assert!(loaded.warning.is_some(), "wrong type warns");
    assert_eq!(
        std::fs::read_to_string(settings_path_for(&pack)).unwrap(),
        bytes,
        "the bytes are untouched"
    );
}

#[test]
fn window_mode_is_windowed_or_borderless_primary_only() {
    assert_eq!(window_mode(false), WindowMode::Windowed);
    assert_eq!(
        window_mode(true),
        WindowMode::BorderlessFullscreen(MonitorSelection::Primary)
    );
}

#[test]
fn window_config_is_the_1280_by_720_reference_with_a_matching_floor() {
    let window = window_config();
    assert_eq!(window.title, "unmei");
    assert_eq!(
        (
            window.resolution.physical_width(),
            window.resolution.physical_height()
        ),
        (1280, 720),
        "the created window is the 1280x720 reference size"
    );
    assert_eq!(
        window.resize_constraints.min_width, 1280.0,
        "the width floor is the reference width"
    );
    assert_eq!(
        window.resize_constraints.min_height, 720.0,
        "the height floor is the reference height"
    );
}

#[test]
fn reset_play_touches_only_play() {
    let dir = temp_pack();
    let mut state = SettingsState::new(dir.path().join("pack"), Settings::default());
    state.set_fraction(AudioControl::Master, 0.5);
    state.settings.play.text_speed = 0.04;
    state.settings.play.auto = true;
    state.settings.play.fullscreen = true;

    state.reset_play();
    assert_eq!(state.settings.play, PlaySettings::default());
    assert_eq!(state.percent(AudioControl::Master), 50, "audio untouched");
    assert!(state.dirty);
}

#[test]
fn a_flush_writes_once_and_clears_dirty() {
    let dir = temp_pack();
    let pack = dir.path().join("pack");
    let mut state = SettingsState::new(pack.clone(), Settings::default());
    state.set_fraction(AudioControl::Master, 0.5);
    assert!(state.dirty);

    let mut notice = Notice::default();
    let mut hold = DefensiveHold::default();
    flush(&mut state, &mut notice, &mut hold);

    assert!(!state.dirty);
    assert!(!notice.is_showing());
    assert!(!hold.is_held());
    assert_eq!(load(&pack).settings.audio.master, 0.5);
}

#[test]
fn a_failed_flush_keeps_dirty_and_notices_row_20() {
    let dir = temp_pack();
    let pack = dir.path().join("pack");
    let mut state = SettingsState::new(pack.clone(), Settings::default());
    state.set_fraction(AudioControl::Master, 0.5);

    // Block the atomic write: the temp path is a directory.
    std::fs::create_dir_all(settings_path_for(&pack).with_extension("ron.tmp")).unwrap();

    let mut notice = Notice::default();
    let mut hold = DefensiveHold::default();
    flush(&mut state, &mut notice, &mut hold);

    assert!(state.dirty, "kept for the next flush to retry");
    assert!(notice.is_showing());
    assert_eq!(notice.text.as_deref(), Some("settings not saved"));
    assert!(!hold.is_held(), "row 20 never holds the runner");
}
