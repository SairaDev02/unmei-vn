//! The four state overrides a tree authors: `focused`, `disabled`, `lit` and
//! `current` (spec §4).
//!
//! A tree paints a control in the state it is in, and focus changes without a
//! rebuild, so one system repaints every rendered node whose state moved. The
//! precedence is fixed because the format lets several states be true at once and
//! says nothing about their order: focused, then disabled, then lit, then
//! current. The focused state wins because it is the one the player moved to.
//!
//! Each state's face and tone are resolved once, when the tree is spawned: the
//! repaint only copies the components a state selects.

use bevy::prelude::*;
use bevy::text::LineHeight;
use bevy::ui::{BackgroundGradient, BoxShadow, UiTransform};
use unmei_engine::ui::{Color, ControlStates, Paint, StateOverrides, Style};

use crate::menu::{self, MenuAction, MenuState};
use crate::presentation::style::TextFaces;
use crate::presentation::PresentationSet;

use super::look::UiLook;
use super::paint;
use super::text;
use super::Repaint;

/// The states in the order they win.
const STATES: usize = 4;
const FOCUSED: usize = 0;
const DISABLED: usize = 1;
const LIT: usize = 2;

/// The state a rendered node's overrides follow, so a repaint reads one
/// component instead of the whole surface.
#[derive(Component, Clone, Debug)]
pub(crate) struct UiStates {
    pub action: Option<MenuAction>,
    pub disabled: bool,
    pub lit: bool,
    pub current: bool,
    /// The paint the node draws with when no state is in force.
    pub paint: Paint,
    /// The overrides the tree authored, and each one's face and tone resolved.
    pub states: ControlStates,
    /// The face, line and tone the tree's own style draws, so leaving a state
    /// restores them instead of keeping the state's paint.
    base_font: Option<TextFont>,
    base_line: Option<LineHeight>,
    base_color: Option<bevy::prelude::Color>,
    fonts: [Option<TextFont>; STATES],
    lines: [Option<LineHeight>; STATES],
    colors: [Option<Color>; STATES],
}

impl UiStates {
    /// The state facts of one rendered node, with every override's face and tone
    /// resolved through the look in force.
    pub(crate) fn new(
        action: Option<MenuAction>,
        state: super::tree::ControlState,
        states: &ControlStates,
        style: &Style,
        look: &UiLook,
        faces: &TextFaces,
    ) -> Self {
        let mut resolved = Self {
            action,
            disabled: state.disabled,
            lit: state.lit,
            current: state.current,
            paint: style.paint.clone(),
            states: states.clone(),
            base_font: style
                .text
                .typography
                .as_ref()
                .map(|typography| text::font(look, faces, typography)),
            base_line: style
                .text
                .typography
                .as_ref()
                .map(|typography| LineHeight::Px(typography.line_height_px)),
            base_color: text::tone_color(style),
            fonts: std::array::from_fn(|_| None),
            lines: std::array::from_fn(|_| None),
            colors: std::array::from_fn(|_| None),
        };
        let authored = [
            states.focused.as_ref(),
            states.disabled.as_ref(),
            states.lit.as_ref(),
            states.current.as_ref(),
        ];
        for (index, overrides) in authored.into_iter().enumerate() {
            if let Some(typography) = overrides.and_then(|overrides| overrides.typography.as_ref())
            {
                resolved.fonts[index] = Some(text::font(look, faces, typography));
                resolved.lines[index] = Some(LineHeight::Px(typography.line_height_px));
            }
            resolved.colors[index] = overrides.and_then(|overrides| overrides.color);
        }
        resolved
    }
}

/// Repaint the paint of every rendered node in the state it is now: the state's
/// overrides when one is in force, the tree's own paint otherwise. Writing both
/// is what unlights a control without a rebuild, and a value that did not move
/// is left alone, so an unchanged node keeps its change tick.
#[allow(
    clippy::type_complexity,
    reason = "one node's paint is the six components Bevy carries it in"
)]
pub(crate) fn sync_paint_states(
    menu: Res<MenuState>,
    mut nodes: Query<(
        &UiStates,
        &mut Node,
        Option<&mut BackgroundColor>,
        Option<&mut BackgroundGradient>,
        Option<&mut BorderColor>,
        Option<&mut BoxShadow>,
        Option<&mut UiTransform>,
    )>,
) {
    for (states, mut node, background, gradient, border, shadow, transform) in &mut nodes {
        let overrides = active(&menu, states).and_then(|index| overrides_at(&states.states, index));
        let drawn = effective(&states.paint, overrides);
        let (fill_background, fill_gradient) = match &drawn.fill {
            Some(fill) => {
                let (value, value_gradient) = paint::background(fill, drawn.opacity);
                (Some(value), value_gradient)
            }
            None => (None, None),
        };
        if let (Some(mut component), Some(value)) = (background, fill_background) {
            component.set_if_neq(value);
        }
        if let Some(mut component) = gradient {
            component.set_if_neq(fill_gradient.unwrap_or(BackgroundGradient(Vec::new())));
        }
        if let (Some(mut component), Some(value)) = (border, drawn.border) {
            let (value, _) = paint::border(value, drawn.opacity);
            component.set_if_neq(value);
        }
        let mut next = node.clone();
        next.border = match drawn.border {
            Some(value) => UiRect::all(Val::Px(value.width)),
            None => UiRect::ZERO,
        };
        next.border_radius = drawn.radius.map(paint::radius).unwrap_or_default();
        node.set_if_neq(next);
        if let Some(mut component) = shadow {
            component.set_if_neq(paint::shadows(&drawn.shadows, drawn.opacity));
        }
        if let Some(mut component) = transform {
            component.set_if_neq(paint::transform(drawn.transform));
        }
    }
}

/// Repaint the face, tone and line height of every rendered label in the state
/// it is now, with the tree's own values standing in when the state sets none.
/// A value that did not move is left alone.
#[allow(
    clippy::type_complexity,
    reason = "a label's face, tone and line height are the three components Bevy carries them in"
)]
pub(crate) fn sync_text_states(
    menu: Res<MenuState>,
    mut labels: Query<(
        &UiStates,
        Option<&mut TextFont>,
        Option<&mut LineHeight>,
        Option<&mut TextColor>,
    )>,
) {
    for (states, font, line, color) in &mut labels {
        let index = active(&menu, states);
        let wanted_font = index
            .and_then(|index| states.fonts[index].clone())
            .or_else(|| states.base_font.clone());
        let wanted_line = index
            .and_then(|index| states.lines[index])
            .or(states.base_line);
        if let (Some(mut component), Some(value)) = (font, wanted_font) {
            component.set_if_neq(value);
        }
        if let (Some(mut component), Some(value)) = (line, wanted_line) {
            component.set_if_neq(value);
        }
        let wanted_color = index
            .and_then(|index| states.colors[index])
            .map(|value| paint::color(value, states.paint.opacity))
            .or(states.base_color);
        if let (Some(mut component), Some(value)) = (color, wanted_color) {
            component.set_if_neq(TextColor::from(value));
        }
    }
}

/// The index of the state in force, if any.
fn active(menu: &MenuState, states: &UiStates) -> Option<usize> {
    let focused = states
        .action
        .is_some_and(|action| menu::action_for_focus(menu.focus) == action);
    if focused && states.states.focused.is_some() {
        return Some(FOCUSED);
    }
    for (index, (set, authored)) in [
        (states.disabled, states.states.disabled.is_some()),
        (states.lit, states.states.lit.is_some()),
        (states.current, states.states.current.is_some()),
    ]
    .into_iter()
    .enumerate()
    {
        if set && authored {
            return Some(index + 1);
        }
    }
    None
}

fn overrides_at(states: &ControlStates, index: usize) -> Option<&StateOverrides> {
    match index {
        FOCUSED => states.focused.as_ref(),
        DISABLED => states.disabled.as_ref(),
        LIT => states.lit.as_ref(),
        _ => states.current.as_ref(),
    }
}

/// The paint a node draws with when `overrides` are in force: each property the
/// override sets replaces the base's.
fn effective(base: &Paint, overrides: Option<&StateOverrides>) -> Paint {
    let Some(overrides) = overrides else {
        return base.clone();
    };
    Paint {
        fill: overrides.fill.clone().or_else(|| base.fill.clone()),
        border: overrides.border.or(base.border),
        radius: overrides.radius.or(base.radius),
        shadows: overrides
            .shadows
            .clone()
            .unwrap_or_else(|| base.shadows.clone()),
        opacity: overrides.opacity.unwrap_or(base.opacity),
        transform: overrides.transform.unwrap_or(base.transform),
    }
}

/// Register the repaint systems, after the tree that spawns the nodes.
pub(crate) fn register(app: &mut App) {
    app.add_systems(
        Update,
        (sync_paint_states, sync_text_states)
            .chain()
            .in_set(PresentationSet)
            .in_set(Repaint),
    );
}
