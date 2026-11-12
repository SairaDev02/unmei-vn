//! Real input messages and the screenshot sink the walk drives.

use std::path::{Path, PathBuf};

use bevy::ecs::entity::Entity;
use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};

/// Write one real `KeyboardInput` message (the same production path the window's
/// own events take through `bevy_input`).
pub fn key(world: &mut World, code: KeyCode, state: ButtonState) {
    world.write_message(KeyboardInput {
        key_code: code,
        logical_key: Key::Unidentified(NativeKey::Unidentified),
        state,
        text: None,
        repeat: false,
        window: Entity::PLACEHOLDER,
    });
}

/// Write one real typed character (the text path the key field receives). The
/// caller must release `KeyCode::KeyA` on a later frame: a synthetic press with
/// no release would leave the key physically down, which the production
/// held-input quarantine would then treat as a key held across the Monitor
/// transition — blocking every ordinary app-input path for the rest of the run.
pub fn typed(world: &mut World, text: &str) {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return;
    };
    if chars.next().is_some() {
        // The walk types fixtures a character at a time; a multi-character
        // literal is a harness bug, not a platform path.
        panic!("typed() accepts one character per call");
    }
    world.write_message(KeyboardInput {
        key_code: key_for(first),
        logical_key: Key::Character(first.to_string().into()),
        state: ButtonState::Pressed,
        text: Some(first.to_string().into()),
        repeat: false,
        window: Entity::PLACEHOLDER,
    });
}

/// The physical key a typed ASCII character arrives on, so the walk's typing
/// exercises the real per-key press/release bookkeeping rather than one
/// hardcoded key for every character.
pub fn key_for(character: char) -> KeyCode {
    const LETTERS: [KeyCode; 26] = [
        KeyCode::KeyA,
        KeyCode::KeyB,
        KeyCode::KeyC,
        KeyCode::KeyD,
        KeyCode::KeyE,
        KeyCode::KeyF,
        KeyCode::KeyG,
        KeyCode::KeyH,
        KeyCode::KeyI,
        KeyCode::KeyJ,
        KeyCode::KeyK,
        KeyCode::KeyL,
        KeyCode::KeyM,
        KeyCode::KeyN,
        KeyCode::KeyO,
        KeyCode::KeyP,
        KeyCode::KeyQ,
        KeyCode::KeyR,
        KeyCode::KeyS,
        KeyCode::KeyT,
        KeyCode::KeyU,
        KeyCode::KeyV,
        KeyCode::KeyW,
        KeyCode::KeyX,
        KeyCode::KeyY,
        KeyCode::KeyZ,
    ];
    const DIGITS: [KeyCode; 10] = [
        KeyCode::Digit0,
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
    match character {
        'a'..='z' => LETTERS[(character as u8 - b'a') as usize],
        '0'..='9' => DIGITS[(character as u8 - b'0') as usize],
        '-' => KeyCode::Minus,
        _ => KeyCode::KeyA,
    }
}

/// One OS-repeated press: `just_pressed` stays false, so it is never a fresh
/// activation.
pub fn repeat(world: &mut World, code: KeyCode) {
    world.write_message(KeyboardInput {
        key_code: code,
        logical_key: Key::Unidentified(NativeKey::Unidentified),
        state: ButtonState::Pressed,
        text: None,
        repeat: true,
        window: Entity::PLACEHOLDER,
    });
}

pub fn press(world: &mut World, code: KeyCode) {
    key(world, code, ButtonState::Pressed);
}

pub fn release(world: &mut World, code: KeyCode) {
    key(world, code, ButtonState::Released);
}

/// Park the real cursor at a physical position, so Bevy's UI hit-testing runs
/// over it.
pub fn move_cursor(world: &mut World, position: Vec2) {
    let mut window = world
        .query_filtered::<&mut Window, With<bevy::window::PrimaryWindow>>()
        .single_mut(world)
        .expect("the primary window");
    window.set_physical_cursor_position(Some(position.as_dvec2()));
}

/// Queue a screenshot of the primary window; the PNG lands a few frames later
/// in `target/monitor-acceptance/`.
pub fn screenshot(world: &mut World, name: &str) -> PathBuf {
    let dir = shots_dir();
    std::fs::create_dir_all(&dir).expect("the screenshot directory is writable");
    let path = dir.join(format!("{name}.png"));
    let _ = std::fs::remove_file(&path);
    let mut commands = world.spawn(Screenshot::primary_window());
    commands.observe(save_to_disk(path.clone()));
    path
}

pub fn shots_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/monitor-acceptance")
}
