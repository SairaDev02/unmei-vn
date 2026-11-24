//! The text a node draws with: the face a typography resolves to, the tone it
//! resolves from, and the line height it states (spec §4, "Engine text").
//!
//! The face is the plugin's own when it ships that family and weight, else one of
//! the five the player embeds (spec §11, R19–R20). Nothing here reads a file:
//! [`super::look`] turned the plugin's fonts into handles once, at boot.

use bevy::prelude::*;
use bevy::text::{LineBreak, LineHeight, TextLayout};
use unmei_engine::ui::{Color, Fill, Style, Typography};

use crate::presentation::style::TextFaces;

use super::look::UiLook;
use super::paint;
use super::tree::Surface;

/// Style a text-bearing node: a `text` or `glyph` node, or the words a slot or
/// control draws.
pub(super) fn style(entity: &mut EntityCommands<'_>, surface: &Surface<'_>, style: &Style) {
    let Some(typography) = &style.text.typography else {
        return;
    };
    entity.insert(font(surface.look, surface.faces, typography));
    entity.insert(LineHeight::Px(typography.line_height_px));
    entity.insert(TextLayout {
        linebreak: LineBreak::WordOrCharacter,
        ..default()
    });
    if let Some(tone) = tone_color(style) {
        entity.insert(TextColor::from(tone));
    }
}

/// The colour a node's words draw in, opacity included: the tone a loop dims.
pub(super) fn tone_color(style: &Style) -> Option<bevy::prelude::Color> {
    let opacity = style.paint.opacity;
    tone(style).map(|tone| paint::color(tone, opacity))
}

/// The tone a node draws its text in: a `text` or `glyph` node's own `fill`,
/// then the tone an enclosing box sets; a slot or control takes the set tone
/// alone (spec §4).
fn tone(style: &Style) -> Option<Color> {
    style
        .text
        .color
        .or_else(|| fill_tone(style.paint.fill.as_ref()))
}

/// A fill's tone. A gradient has no single colour, so its first stop stands for
/// it: a text or glyph node draws one colour.
fn fill_tone(fill: Option<&Fill>) -> Option<Color> {
    match fill? {
        Fill::Color(color) => Some(*color),
        Fill::Linear { stops, .. } | Fill::Radial { stops } => stops.first().map(|stop| stop.color),
    }
}

/// The font a typography names, at its size.
pub(super) fn font(look: &UiLook, faces: &TextFaces, typography: &Typography) -> TextFont {
    let face = look
        .shipped_face(&typography.face, typography.weight)
        .or_else(|| faces.embedded(&typography.face, typography.weight));
    TextFont {
        font: face.unwrap_or_else(|| faces.sans_regular.clone()).into(),
        font_size: FontSize::Px(typography.size_px),
        ..default()
    }
}
