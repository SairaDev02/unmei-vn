//! The play frame drawn from the plugin's `play` surface: the dialogue box, the
//! name tab, the prose, the two markers, the choices, the quick row, the deciding
//! line and the end-of-scene line (Default look §1–§2, spec §11).
//!
//! The tree decides the look; this module decides what it says. Every frame it
//! reads the parked `Wait`, the reveal, the line the player just read, and the
//! auto and skip modes, and writes only what changed: the words, which slots have
//! anything to show, the option buttons, and the row's lit labels. Focus, keys
//! and walking stay the input code's, and a choice button carries its own index
//! for the click that picks it.
//!
//! The previous say-line is a presentation memory, not a script change: at a
//! choice-wait the box keeps it, fully revealed and without the advance mark, so
//! the options answer a question still on screen. A load clears the memory with
//! the rest of the replay caches, so a save restored onto a choice-wait shows an
//! empty box (§2).

use bevy::prelude::*;
use bevy::ui::Display;
use unmei_engine::copy;
use unmei_engine::runner::Wait;

use super::reveal::{say_parts, TextReveal, NARRATOR};
use super::state::{AppliedUi, ChoiceButton};
use crate::core::SkipState;
use crate::menu::{MenuAction, MenuState};
use crate::presentation::render::tree::Rendered;
use crate::presentation::render::{Bind, ControlState, Surface, UiLook, UiStates, Words};
use crate::presentation::style::TextFaces;
use crate::settings::{self, SettingsState};
use crate::Runtime;

/// The surface's id, as the plugin folder names it.
const SURFACE: &str = "play";

/// The quick row's five controls, in Default look §1's order.
const QUICK_ROW: [&str; 5] = [
    "play.save",
    "play.load",
    "play.auto",
    "play.skip",
    "play.menu",
];

/// The play frame the renderer spawned, and what it currently shows.
#[derive(Resource, Default)]
pub(crate) struct PlayFrame {
    bound: Option<Rendered>,
    shown: PlayShown,
    options: Vec<Entity>,
    option_texts: Vec<String>,
}

/// What the frame shows now: the whole of what the tree draws, so one answer
/// decides which slots have anything to show.
#[derive(Clone, Debug, Default, PartialEq)]
struct PlayShown {
    hidden: bool,
    speaker: Option<String>,
    prose: String,
    advance: bool,
    deciding: Option<String>,
    scene_end: Option<String>,
    options: Vec<String>,
    auto_lit: bool,
    skip_lit: bool,
}

impl PlayFrame {
    /// The prose the player is reading, which the Keiko gate reports.
    pub(crate) fn prose(&self) -> Option<&str> {
        (!self.shown.prose.is_empty()).then_some(self.shown.prose.as_str())
    }

    /// The speaker the name tab shows, when the frame draws one.
    #[cfg(test)]
    pub(crate) fn speaker(&self) -> Option<&str> {
        self.shown.speaker.as_deref()
    }
}

/// What the frame draws from: the answer, and the numbering its option badges
/// follow.
struct Frame<'a> {
    shown: &'a PlayShown,
}

impl Bind for Frame<'_> {
    fn words(&self, name: &str) -> Option<Words> {
        match name {
            "prose" => Some(Words::text(self.shown.prose.clone())),
            "speaker" => self.shown.speaker.clone().map(Words::text),
            "deciding" => self.shown.deciding.clone().map(Words::text),
            "scene-end" => self.shown.scene_end.clone().map(Words::text),
            "play.save" => Some(Words::new("play.quick.save")),
            "play.load" => Some(Words::new("play.quick.load")),
            "play.auto" => Some(Words::new("play.quick.auto")),
            "play.skip" => Some(Words::new("play.quick.skip")),
            "play.menu" => Some(Words::new("play.quick.menu")),
            _ => None,
        }
    }

    fn shows(&self, name: &str) -> bool {
        match name {
            "prose" => !self.shown.prose.is_empty(),
            "speaker" => self.shown.speaker.is_some(),
            "advance-marker" => self.shown.advance,
            "deciding" | "deciding-marker" => self.shown.deciding.is_some(),
            "scene-end" => self.shown.scene_end.is_some(),
            _ => true,
        }
    }

    fn action(&self, name: &str) -> Option<MenuAction> {
        match name {
            "play.save" => Some(MenuAction::QuickSave),
            "play.load" => Some(MenuAction::QuickLoad),
            "play.auto" => Some(MenuAction::QuickAuto),
            "play.skip" => Some(MenuAction::QuickSkip),
            "play.menu" => Some(MenuAction::QuickMenu),
            _ => None,
        }
    }

    fn state(&self, name: &str) -> ControlState {
        match name {
            "play.auto" => ControlState {
                lit: self.shown.auto_lit,
                ..default()
            },
            "play.skip" => ControlState {
                lit: self.shown.skip_lit,
                ..default()
            },
            _ => ControlState::default(),
        }
    }
}

/// One option's words: its number in the plugin's style and the script's own
/// text (Default look §2).
struct Choice<'a> {
    number: String,
    text: &'a str,
}

impl Bind for Choice<'_> {
    fn words(&self, name: &str) -> Option<Words> {
        match name {
            "choice.number" => Some(Words::text(self.number.clone())),
            "choice.text" => Some(Words::text(self.text.to_string())),
            _ => None,
        }
    }
}

/// Spawn the play frame once the look is in place. A failed pack never gets a
/// look, so its error screen draws with no frame behind it.
pub(crate) fn spawn_play_frame(
    mut commands: Commands,
    look: Option<Res<UiLook>>,
    faces: Option<Res<TextFaces>>,
    mut frame: ResMut<PlayFrame>,
) {
    let (Some(look), Some(faces)) = (look, faces) else {
        return;
    };
    if frame.bound.is_some() {
        return;
    }
    let shown = PlayShown {
        hidden: true,
        ..default()
    };
    let bind = Frame { shown: &shown };
    let surface = Surface {
        look: &look,
        faces: &faces,
        bind: &bind,
    };
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        })
        .with_children(|parent| {
            if let Some(bound) = surface.spawn(SURFACE, parent) {
                frame.bound = Some(bound);
            }
        });
    frame.shown = shown;
}

/// Read the parked wait into what the frame should show, arming the reveal on a
/// new park first so the prose starts empty. Runs before the writers, so they
/// all read one answer. The row's lit labels are Default look §1's mode
/// indicator: `Skip Tab` is lit whenever skip is effective, held `Ctrl`
/// included, and `Auto A` only while auto runs and skip does not.
pub(crate) fn read_play_frame(
    runtime: Option<Res<Runtime>>,
    menu: Option<Res<MenuState>>,
    settings_state: Option<Res<SettingsState>>,
    skip: Option<Res<SkipState>>,
    mut reveal: Option<ResMut<TextReveal>>,
    mut applied: ResMut<AppliedUi>,
    mut frame: ResMut<PlayFrame>,
) {
    let Some(runtime) = runtime.as_deref() else {
        return;
    };
    let speed = settings::effective_text_speed(settings_state.as_deref());
    let wait = runtime.runner.wait().cloned();
    let Some(reveal) = reveal.as_deref_mut() else {
        return;
    };
    if !applied.ready || applied.wait != wait {
        reveal.arm(&wait, speed);
        applied.wait = wait.clone();
        applied.ready = true;
    }
    let mut shown = PlayShown::default();
    if menu.as_deref().is_some_and(MenuState::title_surface_up) {
        shown.hidden = true;
    } else {
        match &wait {
            Some(say @ Wait::Say { speaker, text }) => {
                if let Some((tab, prose)) = say_parts(say, reveal, speed) {
                    shown.speaker = tab;
                    shown.prose = prose;
                }
                shown.advance = reveal.is_whole_for(say, speed);
                applied.say = Some((speaker.clone(), text.clone()));
            }
            Some(Wait::Choice { options }) => {
                shown.options = options.iter().map(|(text, _)| text.clone()).collect();
                if let Some((speaker, text)) = &applied.say {
                    shown.prose = text.clone();
                    shown.speaker = Some(speaker.clone()).filter(|name| name != NARRATOR);
                }
            }
            Some(Wait::Decision { npc_id, .. }) => {
                shown.deciding = Some(copy::render("play.deciding", &[("npc", npc_id)]));
            }
            None => shown.scene_end = Some(copy::text("play.end-of-scene").to_string()),
        }
    }
    let skip_on = skip.as_deref().is_some_and(SkipState::is_on);
    shown.skip_lit = skip_on;
    shown.auto_lit = settings_state.as_deref().is_some_and(SettingsState::auto) && !skip_on;
    frame.shown = shown;
}

/// Spawn, respawn or clear the option buttons, and keep the frame's own record.
pub(crate) fn sync_play_options(
    mut commands: Commands,
    look: Option<Res<UiLook>>,
    faces: Option<Res<TextFaces>>,
    mut frame: ResMut<PlayFrame>,
) {
    let (Some(look), Some(faces)) = (look, faces) else {
        return;
    };
    let want = frame.shown.options.clone();
    if frame.option_texts == want {
        return;
    }
    for entity in frame.options.drain(..) {
        commands.entity(entity).despawn();
    }
    frame.option_texts = want.clone();
    let Some(bound) = &frame.bound else {
        return;
    };
    let (Some(list), Some(item)) = (
        bound.lists.get("choices").copied(),
        bound.items.get("choices").cloned(),
    ) else {
        return;
    };
    let numbering = look.numbering();
    let surface = Surface {
        look: &look,
        faces: &faces,
        bind: &Frame {
            shown: &frame.shown,
        },
    };
    let options = want;
    let mut spawned = Vec::with_capacity(options.len());
    commands.entity(list).with_children(|parent| {
        for (index, text) in options.iter().enumerate() {
            let rendered = surface.spawn_item(
                &item,
                parent,
                &Choice {
                    number: numbering.label(index + 1),
                    text,
                },
            );
            if let Some(root) = rendered.root {
                parent
                    .commands()
                    .entity(root)
                    .insert((Button, ChoiceButton { index }));
                spawned.push(root);
            }
        }
    });
    frame.options = spawned;
}

/// Write the words and the slots' visibility when the frame's answer changed,
/// and the row's lit state, which the renderer's state pass repaints in the
/// same frame.
pub(crate) fn sync_play_frame(
    mut commands: Commands,
    look: Option<Res<UiLook>>,
    frame: ResMut<PlayFrame>,
    mut nodes: Query<&mut Node>,
    mut states: Query<&mut UiStates>,
) {
    let Some(look) = look else {
        return;
    };
    let Some(bound) = &frame.bound else {
        return;
    };
    let bind = Frame {
        shown: &frame.shown,
    };
    for (name, words) in [
        ("prose", bind.words("prose")),
        ("speaker", bind.words("speaker")),
        ("deciding", bind.words("deciding")),
        ("scene-end", bind.words("scene-end")),
    ] {
        let Some(entity) = bound.slots.get(name) else {
            continue;
        };
        if let Some(words) = words {
            commands
                .entity(*entity)
                .insert(Text::new(look.words(&words)));
        }
    }
    crate::presentation::render::tree::sync_visibility(&mut nodes, bound, &bind);
    for name in QUICK_ROW {
        let lit = bind.state(name).lit;
        for entity in [bound.controls.get(name), bound.slots.get(name)]
            .into_iter()
            .flatten()
        {
            if let Ok(mut drawn) = states.get_mut(*entity) {
                drawn.lit = lit;
            }
        }
    }
    let hidden = frame.shown.hidden;
    if let Some(root) = bound.root {
        if let Ok(mut container) = nodes.get_mut(root) {
            let want = if hidden { Display::None } else { Display::Flex };
            if container.display != want {
                container.display = want;
            }
        }
    }
}
