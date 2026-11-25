//! Applying one step to the running take: every intent and raw input reaches
//! the player as real input, the clock moves only on `wait`, and a step that
//! cannot reach its target fails with the spec's rule id (spec §3, K9).
//!
//! Input is written as the same `KeyboardInput` messages the window backend
//! produces, never by poking `ButtonInput` directly: the app's own input plugin
//! folds them, so input ownership, the held-key quarantine and focused dispatch
//! behave exactly as in play.

use std::time::Duration;

use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
use bevy::input::ButtonState;
use bevy::prelude::*;
use unmei_engine::keiko::{exact, Expect, LoadTarget, Pick, Step};
use unmei_engine::runner::Wait;

use super::controls::{focus_for_id, is_enabled, surface_ids};
use super::finding::{rules, Finding, Location};
use crate::credentials::ClipboardPort;
use crate::menu::{MenuFocus, MenuState, MenuView};
use crate::Runtime;

/// The frame the take moves its clock by (`1/60 s`, spec §4.3).
pub const FRAME: Duration = Duration::from_nanos(1_000_000_000 / 60);

/// Runs one take frame. The take supplies it, so the virtual clock stays the
/// take's own.
pub type FrameRun<'a> = &'a mut dyn FnMut(&mut App);

/// What the take does after a step applied.
#[derive(Debug, Clone, PartialEq)]
pub enum Applied {
    /// An input step: settle, then observe.
    Input,
    /// A clock step: move the clock by this much, then settle.
    Wait(Duration),
    /// An assertion: observe and evaluate; no settling.
    Assert(Expect),
}

/// Apply one step. `render` is the take's rendered composition when the case
/// asks for a frame or a click and the machine has a software rasterizer
/// (spec §13).
pub fn apply(
    app: &mut App,
    step: &Step,
    run: FrameRun,
    resolver: &super::decide::KeikoResolver,
    render: Option<&mut super::render::Rendered>,
) -> Result<Applied, Finding> {
    let fail = |rule: &str, message: String| {
        Finding::new(
            "case",
            Location::Field(format!("step `{}`", verb(step))),
            rule,
            message,
        )
    };
    match step {
        Step::NewGame => {
            activate_control(app, run, "title.new-game")
                .map_err(|error| fail(error.rule, error.message))?;
            Ok(Applied::Input)
        }
        Step::Continue => {
            activate_control(app, run, "title.continue")
                .map_err(|error| fail(error.rule, error.message))?;
            Ok(Applied::Input)
        }
        Step::Advance(count) => {
            for _ in 0..*count {
                tap(app, run, KeyCode::Space);
            }
            Ok(Applied::Input)
        }
        Step::Pick(pick) => {
            let index = match pick {
                Pick::Index(index) => {
                    if *index < 1 || *index > 9 {
                        return Err(fail(rules::STEP_NO_TARGET, format!("no option {index}")));
                    }
                    *index as usize
                }
                Pick::Text(text) => {
                    let matches: Vec<usize> = choice_options(app)
                        .iter()
                        .enumerate()
                        .filter(|(_, option)| exact(option, text))
                        .map(|(index, _)| index + 1)
                        .collect();
                    match matches.as_slice() {
                        [index] => *index,
                        [] => {
                            return Err(fail(
                                rules::STEP_NO_TARGET,
                                format!("no option matches \"{text}\""),
                            ))
                        }
                        _ => {
                            return Err(fail(
                                rules::STEP_NO_TARGET,
                                format!(
                                    "\"{text}\" matches {} options; name one by index",
                                    matches.len()
                                ),
                            ))
                        }
                    }
                }
            };
            tap(app, run, digit(index as u8));
            Ok(Applied::Input)
        }
        Step::Menu => {
            tap(app, run, KeyCode::Escape);
            Ok(Applied::Input)
        }
        Step::Auto => {
            tap(app, run, KeyCode::KeyA);
            Ok(Applied::Input)
        }
        Step::Skip => {
            tap(app, run, KeyCode::Tab);
            Ok(Applied::Input)
        }
        Step::Press(key) => {
            visit(app, run, key).map_err(|message| fail(rules::CASE_SYNTAX, message))?;
            Ok(Applied::Input)
        }
        Step::Hold(key) => {
            hold(app, run, key).map_err(|message| fail(rules::CASE_SYNTAX, message))?;
            Ok(Applied::Input)
        }
        Step::Release(key) => {
            let code = key_code(key)
                .ok_or_else(|| fail(rules::CASE_SYNTAX, format!("unknown key `{key}`")))?;
            send(app, code, ButtonState::Released, None);
            run(app);
            Ok(Applied::Input)
        }
        Step::Type(text) => {
            for ch in text.chars() {
                send(app, KeyCode::KeyA, ButtonState::Pressed, Some(ch));
                run(app);
                send(app, KeyCode::KeyA, ButtonState::Released, None);
                run(app);
            }
            Ok(Applied::Input)
        }
        Step::Paste(text) => {
            *app.world_mut().resource_mut::<ClipboardPort>() =
                ClipboardPort::Memory(Some(text.clone()));
            hold(app, run, "ControlLeft").map_err(|message| fail(rules::CASE_SYNTAX, message))?;
            tap(app, run, KeyCode::KeyV);
            release_message(app, run, KeyCode::ControlLeft);
            Ok(Applied::Input)
        }
        Step::Wait(duration) => Ok(Applied::Wait(*duration)),
        Step::Activate(id) => {
            activate_control(app, run, id).map_err(|error| fail(error.rule, error.message))?;
            Ok(Applied::Input)
        }
        Step::Save(slot) => {
            save_or_load(app, run, true, LoadTarget::Slot(*slot))
                .map_err(|error| fail(error.rule, error.message))?;
            Ok(Applied::Input)
        }
        Step::Load(slot) => {
            save_or_load(app, run, false, *slot)
                .map_err(|error| fail(error.rule, error.message))?;
            Ok(Applied::Input)
        }
        Step::ReleaseDecision(point) => {
            resolver.release(point);
            Ok(Applied::Input)
        }
        // The take handles these two itself: one releases the key check's held
        // verdict, the other removes the sandbox obstruction it knows the path
        // of.
        Step::ReleaseKeyCheck | Step::Unblock(_) => Ok(Applied::Input),
        Step::Frame(name) => {
            let Some(render) = render else {
                return Err(unavailable(step));
            };
            render
                .capture(app, run, name.as_deref())
                .map_err(|message| fail(rules::RENDER_UNAVAILABLE, message))?;
            Ok(Applied::Input)
        }
        Step::Click(click) => {
            let Some(render) = render else {
                return Err(unavailable(step));
            };
            render
                .click_control(app, run, click)
                .map_err(|error| fail(error.rule, error.message))?;
            Ok(Applied::Input)
        }
        Step::Expect(expect) => Ok(Applied::Assert(expect.clone())),
    }
}

/// The step's verb, for a finding's path.
pub fn verb(step: &Step) -> &'static str {
    match step {
        Step::NewGame => "new-game",
        Step::Continue => "continue",
        Step::Advance(_) => "advance",
        Step::Pick(_) => "pick",
        Step::Menu => "menu",
        Step::Activate(_) => "activate",
        Step::Save(_) => "save",
        Step::Load(_) => "load",
        Step::Auto => "auto",
        Step::Skip => "skip",
        Step::Press(_) => "press",
        Step::Hold(_) => "hold",
        Step::Release(_) => "release",
        Step::Type(_) => "type",
        Step::Paste(_) => "paste",
        Step::Wait(_) => "wait",
        Step::ReleaseDecision(_) => "release-decision",
        Step::ReleaseKeyCheck => "release-key-check",
        Step::Unblock(_) => "unblock",
        Step::Frame(_) => "frame",
        Step::Click(_) => "click",
        Step::Expect(_) => "expect",
    }
}

fn choice_options(app: &App) -> Vec<String> {
    app.world()
        .get_resource::<Runtime>()
        .and_then(|runtime| match runtime.runner.wait() {
            Some(Wait::Choice { options }) => {
                Some(options.iter().map(|(text, _)| text.clone()).collect())
            }
            _ => None,
        })
        .unwrap_or_default()
}

fn digit(index: u8) -> KeyCode {
    match index {
        1 => KeyCode::Digit1,
        2 => KeyCode::Digit2,
        3 => KeyCode::Digit3,
        4 => KeyCode::Digit4,
        5 => KeyCode::Digit5,
        6 => KeyCode::Digit6,
        7 => KeyCode::Digit7,
        8 => KeyCode::Digit8,
        _ => KeyCode::Digit9,
    }
}

/// Why an activation did not reach its control. The rule id follows the cause:
/// a control the surface does not show is `keiko.step.no-target`, and one it
/// shows but that holds no action right now is `keiko.step.disabled` (spec
/// §3.1's `activate` row, rule 9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationError {
    /// The rule id the step's finding is filed under.
    pub rule: &'static str,
    /// The human-readable detail.
    pub message: String,
}

impl ActivationError {
    pub(super) fn no_target(message: impl Into<String>) -> Self {
        Self {
            rule: rules::STEP_NO_TARGET,
            message: message.into(),
        }
    }

    fn disabled(message: impl Into<String>) -> Self {
        Self {
            rule: rules::STEP_DISABLED,
            message: message.into(),
        }
    }

    pub(super) fn render_unavailable(message: impl Into<String>) -> Self {
        Self {
            rule: rules::RENDER_UNAVAILABLE,
            message: message.into(),
        }
    }
}

/// Walk focus to a control with the arrow keys, then press Enter, exactly as a
/// player would. Fails with the rule id that names why the control is out of
/// reach.
pub fn activate_control(app: &mut App, run: FrameRun, id: &str) -> Result<(), ActivationError> {
    let Some(target) = focus_for_id(id) else {
        return Err(ActivationError::no_target(format!(
            "unknown control `{id}`"
        )));
    };
    let menu = app.world().resource::<MenuState>().clone();
    let ids = surface_ids_on(app, &menu);
    if !ids.iter().any(|candidate| candidate == id) {
        return Err(ActivationError::no_target(format!(
            "`{id}` is not on the {} surface",
            surface_name(&menu)
        )));
    }
    if !is_enabled(&menu, id, app.world()) {
        return Err(ActivationError::disabled(format!("`{id}` is disabled")));
    }
    for _ in 0..64 {
        if app.world().resource::<MenuState>().focus == target {
            break;
        }
        let before = app.world().resource::<MenuState>().focus;
        // The menus walk the arrow keys; a view that only walks one axis ignores
        // the other key, so trying both is how one case drives every surface.
        for key in [
            KeyCode::ArrowDown,
            KeyCode::ArrowRight,
            KeyCode::ArrowUp,
            KeyCode::ArrowLeft,
        ] {
            tap(app, run, key);
            if app.world().resource::<MenuState>().focus != before {
                break;
            }
        }
        if app.world().resource::<MenuState>().focus == before {
            return Err(ActivationError::no_target(format!(
                "focus cannot reach `{id}` from the current control"
            )));
        }
    }
    if app.world().resource::<MenuState>().focus != target {
        return Err(ActivationError::no_target(format!(
            "focus cannot reach `{id}`"
        )));
    }
    tap(app, run, KeyCode::Enter);
    Ok(())
}

/// The finding a `frame` or `click` step files when the take has no rendered
/// composition: no fallback adapter, or a case the take did not prepare one for.
fn unavailable(step: &Step) -> Finding {
    Finding::new(
        "case",
        Location::Field(format!("step `{}`", verb(step))),
        rules::RENDER_UNAVAILABLE,
        "no software rasterizer is available in this take",
    )
}

/// `click <control>` / `click <x> <y>`: move the stand-in window's cursor and
/// press and release the left button, so the player's own hit test decides what
/// the click reaches (spec §13, K9). A covered control therefore takes no click,
/// exactly as for a player.
pub(super) fn surface_name(menu: &MenuState) -> &'static str {
    if !menu.open {
        "play"
    } else {
        match menu.view {
            MenuView::Title => "title",
            MenuView::Slots => "menu",
            MenuView::Settings => "settings",
            MenuView::Gallery => "gallery",
            MenuView::Timeline => "timeline",
        }
    }
}

fn surface_ids_on(app: &App, menu: &MenuState) -> Vec<String> {
    let (gallery, timeline) = app
        .world()
        .get_resource::<Runtime>()
        .map(|runtime| {
            (
                runtime.runner.pack().gallery_entries().len(),
                runtime.runner.pack().timeline_entries().len(),
            )
        })
        .unwrap_or((0, 0));
    surface_ids(menu, gallery, timeline)
}

/// `save n` / `load n`: open the overlay when it is closed, switch to the right
/// tab, then activate the slot. A confirmation that appears is left open for the
/// case to answer.
fn save_or_load(
    app: &mut App,
    run: FrameRun,
    save: bool,
    target: LoadTarget,
) -> Result<(), ActivationError> {
    let menu = app.world().resource::<MenuState>().clone();
    if !menu.open || menu.view != MenuView::Slots {
        tap(app, run, KeyCode::Escape);
    }
    let tab = if save {
        "overlay.save-tab"
    } else {
        "overlay.load-tab"
    };
    // Tab is the sections' key and the arrows walk the ledger's rows, so the
    // step presses Tab until the section is in reach. No step writes menu state
    // (spec §3, K9).
    for _ in 0..4 {
        if app.world().resource::<MenuState>().focus
            == focus_for_id(tab).unwrap_or(MenuFocus::SaveTab)
        {
            break;
        }
        tap(app, run, KeyCode::Tab);
    }
    activate_control(app, run, tab)?;
    let slot = match target {
        LoadTarget::Slot(n) => format!("overlay.slot.{n}"),
        LoadTarget::Autosave => "overlay.slot.autosave".to_string(),
    };
    activate_control(app, run, &slot)
}

/// The `KeyCode` a case's key name spells.
pub fn key_code(name: &str) -> Option<KeyCode> {
    KEY_NAMES
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, code)| *code)
}

/// The key table: the one authority for a case's key names, shared with the
/// static validator.
pub const KEY_NAMES: &[(&str, KeyCode)] = &[
    ("Enter", KeyCode::Enter),
    ("Space", KeyCode::Space),
    ("Escape", KeyCode::Escape),
    ("Tab", KeyCode::Tab),
    ("Backspace", KeyCode::Backspace),
    ("Delete", KeyCode::Delete),
    ("Home", KeyCode::Home),
    ("End", KeyCode::End),
    ("PageUp", KeyCode::PageUp),
    ("PageDown", KeyCode::PageDown),
    ("ArrowUp", KeyCode::ArrowUp),
    ("ArrowDown", KeyCode::ArrowDown),
    ("ArrowLeft", KeyCode::ArrowLeft),
    ("ArrowRight", KeyCode::ArrowRight),
    ("ControlLeft", KeyCode::ControlLeft),
    ("ControlRight", KeyCode::ControlRight),
    ("ShiftLeft", KeyCode::ShiftLeft),
    ("ShiftRight", KeyCode::ShiftRight),
    ("AltLeft", KeyCode::AltLeft),
    ("KeyA", KeyCode::KeyA),
    ("KeyC", KeyCode::KeyC),
    ("KeyF", KeyCode::KeyF),
    ("KeyN", KeyCode::KeyN),
    ("KeyR", KeyCode::KeyR),
    ("KeyV", KeyCode::KeyV),
    ("KeyX", KeyCode::KeyX),
    ("F3", KeyCode::F3),
    ("F5", KeyCode::F5),
    ("F9", KeyCode::F9),
    ("F11", KeyCode::F11),
    ("Digit1", KeyCode::Digit1),
    ("Digit2", KeyCode::Digit2),
    ("Digit3", KeyCode::Digit3),
    ("Digit4", KeyCode::Digit4),
    ("Digit5", KeyCode::Digit5),
    ("Digit6", KeyCode::Digit6),
    ("Digit7", KeyCode::Digit7),
    ("Digit8", KeyCode::Digit8),
    ("Digit9", KeyCode::Digit9),
];

/// The key names the static validator accepts.
pub fn key_names() -> Vec<&'static str> {
    KEY_NAMES.iter().map(|(name, _)| *name).collect()
}

/// Send one keyboard message, then run a frame so the app folds it.
fn send(app: &mut App, code: KeyCode, state: ButtonState, text: Option<char>) {
    app.world_mut().write_message(KeyboardInput {
        key_code: code,
        logical_key: text
            .map(|ch| Key::Character(ch.to_string().into()))
            .unwrap_or(Key::Unidentified(NativeKey::Unidentified)),
        state,
        text: text.map(|ch| ch.to_string().into()),
        repeat: false,
        window: Entity::PLACEHOLDER,
    });
}

/// Press a key, run one frame, release it, run another.
pub fn tap(app: &mut App, run: FrameRun, key: KeyCode) {
    send(app, key, ButtonState::Pressed, None);
    run(app);
    send(app, key, ButtonState::Released, None);
    run(app);
}

/// Press a key and leave it held.
fn hold(app: &mut App, run: FrameRun, name: &str) -> Result<(), String> {
    let code = key_code(name).ok_or_else(|| format!("unknown key `{name}`"))?;
    send(app, code, ButtonState::Pressed, None);
    run(app);
    Ok(())
}

/// A case's `press` name taps the key: one frame down, one frame up.
fn visit(app: &mut App, run: FrameRun, name: &str) -> Result<(), String> {
    let code = key_code(name).ok_or_else(|| format!("unknown key `{name}`"))?;
    tap(app, run, code);
    Ok(())
}

fn release_message(app: &mut App, run: FrameRun, code: KeyCode) {
    send(app, code, ButtonState::Released, None);
    run(app);
}
