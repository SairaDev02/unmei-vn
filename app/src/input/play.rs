//! The in-play dispatch path: say-line advancement, choice picking, and the
//! `Tab`/`Ctrl` skip machine's input half.

use bevy::input::ButtonInput;
use bevy::prelude::*;
use bevy_kira_audio::{AudioChannel, AudioControl};
use unmei_engine::runner::Wait;

use crate::core::{ctrl_down, ctrl_just_pressed, SkipState};
use crate::failure::{self, DefensiveHold, FailureRow, Notice};
use crate::presentation::assets::AssetWatch;
use crate::presentation::play::{ChoiceButton, Sfx, TextReveal};
use crate::read_log::ReadLogState;
use crate::settings::{self, SettingsState};
use crate::slots::PendingAutosave;
use crate::Runtime;

/// Space / Enter / left-click advances a say-wait — except while the line is
/// still revealing: a press there **completes** the line and is consumed, so a
/// player mashing the key can never skip a line outright. The next press
/// advances. Every other park is untouched.
#[allow(clippy::too_many_arguments)]
pub(crate) fn advance_say_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    settings: Option<Res<SettingsState>>,
    mut skip: ResMut<SkipState>,
    mut reveal: ResMut<TextReveal>,
    mut runtime: ResMut<Runtime>,
    mut read_log: ResMut<ReadLogState>,
    mut hold: ResMut<DefensiveHold>,
    mut notice: ResMut<Notice>,
) {
    let Some(wait) = runtime.runner.wait().cloned() else {
        return;
    };
    if !matches!(wait, Wait::Say { .. }) {
        return;
    }
    let advance = keys.just_pressed(KeyCode::Space)
        || keys.just_pressed(KeyCode::Enter)
        || mouse.just_pressed(MouseButton::Left);
    if !advance {
        return;
    }
    // Cancelling input wins before a skip step: the toggle ends and the press
    // performs only its own action (complete a reveal, or advance once). The
    // rule holds under a force hold too — the press ends the seen toggle while
    // the physical hold continues (`docs/runtime-hardening.md` § Text skip
    // §3/§4), and `end` resets the step so the hold resumes fresh at the park
    // the press lands on.
    skip.end();

    let speed = settings::effective_text_speed(settings.as_deref());
    if reveal.is_current(&wait) && !reveal.is_complete(speed) {
        reveal.complete(speed);
        return;
    }

    read_log.on_park_departure();
    if let Err(error) = runtime.runner.advance_say() {
        // Row 13 — defensive: the wait check above makes it unreachable.
        // `report_runner_call` engages the hold from the row's posture.
        failure::report_runner_call(error, &mut notice, &mut hold);
    }
}

/// Number keys 1–9 or clicking a choice button pick a choice. A pick plays the
/// pack-optional `audio/sfx_select.ogg`, silence if the file is absent.
#[allow(clippy::too_many_arguments)]
pub(crate) fn choice_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut interactions: Query<(&Interaction, &ChoiceButton), Changed<Interaction>>,
    asset_server: Option<Res<AssetServer>>,
    watch: Option<ResMut<AssetWatch>>,
    sfx: Option<Res<AudioChannel<Sfx>>>,
    mut skip: ResMut<SkipState>,
    mut runtime: ResMut<Runtime>,
    mut hold: ResMut<DefensiveHold>,
    mut autosave: ResMut<PendingAutosave>,
    mut read_log: ResMut<ReadLogState>,
    mut notice: ResMut<Notice>,
) {
    // Only a digit inside the presented options is a pick. A digit past the
    // list is ignored, so the choice wait is never handed an out-of-range index;
    // the `choose` error below is then truly defensive (row 13).
    let option_count = match runtime.runner.wait() {
        Some(Wait::Choice { options }) => options.len(),
        _ => return,
    };

    let mut picked: Option<usize> = None;
    for (index, key) in DIGIT_KEYS.iter().enumerate().take(option_count) {
        if keys.just_pressed(*key) {
            picked = Some(index);
            break;
        }
    }
    if picked.is_none() {
        for (interaction, button) in &mut interactions {
            if *interaction == Interaction::Pressed && button.index < option_count {
                picked = Some(button.index);
            }
        }
    }

    let Some(index) = picked else {
        return;
    };
    // A choice pick ends the seen toggle and does only its own action. Under a
    // force hold the hold continues and skip resumes after the park.
    skip.end();

    // The SFX is a convention, not config: exactly `audio/sfx_select.ogg`.
    // Loading it through the watch reports a missing or undecodable file as row
    // 6, once; `play` is skipped when the file is absent so a failed load is not
    // retried every frame by kira's command queue.
    if let (Some(asset_server), Some(mut watch)) = (asset_server, watch) {
        let sfx_path = "audio/sfx_select.ogg";
        if let Some(source) = watch.watch_audio(&asset_server, FailureRow::SfxAsset, sfx_path) {
            if runtime.asset_root.join(sfx_path).exists() {
                if let Some(sfx) = sfx {
                    sfx.play(source);
                }
            }
        }
    }

    if let Err(error) = runtime.runner.choose(index) {
        // Row 13 — defensive. `report_runner_call` engages the hold from the
        // row's posture.
        failure::report_runner_call(error, &mut notice, &mut hold);
    } else {
        // A resolved player choice is an autosave write point; the follow-up
        // `run` in `drive_script` parks (and applies staging) before the write.
        autosave.arm();
        // The choice park is left: a same-position loop back still counts as a
        // fresh arrival for the mark rule.
        read_log.on_park_departure();
        // A choice resolve is a read-log flush moment too.
        read_log.request_flush();
    }
}

/// `Tab` toggles the seen skip, but only on an **arrival-read** say-park; on an
/// arrival-unread line, at any other park and at end-of-scene it is inert and
/// flashes no cue. `Esc` clears the toggle on the frame that opens the overlay
/// and `1`–`9` clear it whether or not they select a choice
/// (`docs/runtime-hardening.md` § Text skip §3), because this system is chained
/// before `menu::menu_input` and before the skip step. `F11` and `F5`
/// deliberately leave the toggle alone.
///
/// A held `Ctrl` is the **force** hold (`docs/runtime-hardening.md` § Text skip
/// §4): a fresh press observed here arms it and the physical key is mirrored
/// every play frame. It is an override, not a second toggle — while it runs
/// `Tab` is inert, and releasing it restores an otherwise eligible seen toggle.
/// A press made under a defensive hold, the overlay or the title is not observed
/// here, so it cannot arm; the every-frame release mirror
/// ([`crate::core::force_release_mirror`]) clears a latch whose key is released
/// behind a gate.
///
/// This system is **not** gated by `not_held`: `Esc` must always end the toggle
/// before opening the overlay, even while a defensive hold freezes the play
/// path, so no toggle can be left running behind the menu. `Tab` and the force
/// arm honour the hold, so neither can arm skip for the moment it releases.
pub(crate) fn skip_input(
    keys: Res<ButtonInput<KeyCode>>,
    hold: Res<DefensiveHold>,
    read_log: Res<ReadLogState>,
    mut skip: ResMut<SkipState>,
) {
    // `Esc` ends the seen toggle on the frame that opens the overlay, force
    // hold or not: the toggle is never left latched behind the pause
    // (`docs/runtime-hardening.md` § Text skip §3/§5). Only the physical hold
    // survives the overlay.
    if keys.just_pressed(KeyCode::Escape) {
        skip.end();
    }

    // Digits are cancelling input: `1`–`9` end the seen toggle whether or not
    // they select a choice, and their ordinary say/non-choice action stays
    // inert (`docs/runtime-hardening.md` § Text skip §3). `choice_input` ends it
    // too when a digit actually picks; the force hold is untouched.
    if DIGIT_KEYS.iter().any(|key| keys.just_pressed(*key)) {
        skip.end();
    }

    if ctrl_just_pressed(&keys) && !hold.0 {
        skip.arm_force();
    }
    let armed = skip.is_force_armed();
    skip.set_force_active(ctrl_down(&keys) && armed);

    // `Tab` toggles the seen skip, but is inert while the force hold runs; on
    // release an otherwise eligible seen toggle resumes.
    if keys.just_pressed(KeyCode::Tab) && !hold.0 && !skip.is_force_active() {
        if skip.is_seen() {
            skip.end();
        } else if read_log.arrival_read() == Some(true) {
            skip.engage();
        }
    }
}

const DIGIT_KEYS: [KeyCode; 9] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
];
