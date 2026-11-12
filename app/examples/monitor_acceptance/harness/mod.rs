//! Shared world inspection for the Monitor real-Bevy acceptance walk.
//!
//! The walk drives the **shipping** [`unmei_app::UnmeiPlugin`] on a real window
//! and renderer, so it has to read the laid-out UI tree rather than a private
//! test seam. Everything here works from one owned [`Snap`] of the world's
//! `Node`/`ComputedNode`/text/color state, taken per frame, so a check can look
//! at the real Bevy layout, the real font metrics and the real palette without
//! widening the production API for a developer-only example.
//!
//! The tree is found **by shape** (stacking index, header/body/footer sizes,
//! control labels, the four-column row grid), never by the drawer's crate-internal
//! component types.

mod input;
mod report;
mod signature;
mod tree;

pub use input::{key, key_for, move_cursor, press, release, repeat, screenshot, typed};
pub use report::Report;

use std::collections::HashMap;

use bevy::ecs::entity::Entity;
use bevy::prelude::*;
use bevy::text::EditableText;
use bevy::ui::UiGlobalTransform;

/// Tolerances for real font metrics and border rounding.
pub const POSITION_TOLERANCE: f32 = 1.5;
pub const SIZE_TOLERANCE: f32 = 2.5;

// The settled palette the walk asserts against (`docs/runtime-hardening.md`
// § Monitor → Layout). The production constants are crate-private, so this is
// the approved contract restated where the rendered pixels can be checked.
pub const TONE_INFO: Color = Color::srgba(204.0 / 255.0, 204.0 / 255.0, 217.0 / 255.0, 0.62);
pub const TONE_WARN: Color = Color::srgb(
    0xdb as f32 / 255.0,
    0xa1 as f32 / 255.0,
    0x66 as f32 / 255.0,
);
pub const TONE_ERROR: Color = Color::srgb(
    0xe0 as f32 / 255.0,
    0x7a as f32 / 255.0,
    0x6a as f32 / 255.0,
);
pub const NEWEST_INSET: Color = Color::srgb(
    0xe6 as f32 / 255.0,
    0x9e as f32 / 255.0,
    0x5c as f32 / 255.0,
);
pub const NEWEST_WASH: Color = Color::srgba(230.0 / 255.0, 158.0 / 255.0, 92.0 / 255.0, 0.045);

// The fixed geometry the approved layout pins.
pub const PANEL_SIDE_INSET: f32 = 24.0;
pub const PANEL_HEIGHT: f32 = 360.0;
pub const PANEL_BOTTOM_RESERVATION: f32 = 148.0;
pub const HEADER_HEIGHT: f32 = 52.0;
pub const FOOTER_HEIGHT: f32 = 13.0;
pub const ROW_ELAPSED_WIDTH: f32 = 72.0;
pub const ROW_LEVEL_WIDTH: f32 = 46.0;
pub const ROW_TARGET_WIDTH: f32 = 224.0;
pub const ROW_COLUMN_GAP: f32 = 10.0;
pub const SINGLE_LINE_ROW: f32 = 19.4;

/// One node's laid-out place and shape.
#[derive(Clone, Debug)]
pub struct NodeSnap {
    pub entity: Entity,
    pub shape: String,
    pub position: Vec2,
    pub size: Vec2,
    pub zindex: i32,
    pub background: Option<Color>,
    pub border: Option<Color>,
}

/// One rendered row of the drawer's log body.
#[derive(Clone, Debug)]
pub struct RowSnap {
    pub entity: Entity,
    pub top: f32,
    pub height: f32,
    /// The grid columns, in order, or the single collapsed body of a truncated
    /// row.
    pub parts: Vec<Entity>,
    pub truncated: bool,
}

impl RowSnap {
    pub fn body(&self, snap: &Snap) -> String {
        self.parts
            .last()
            .and_then(|part| snap.text(*part))
            .unwrap_or_default()
            .to_string()
    }

    pub fn target(&self, snap: &Snap) -> String {
        if self.truncated {
            return String::new();
        }
        self.parts
            .get(2)
            .and_then(|part| snap.text(*part))
            .unwrap_or_default()
            .to_string()
    }

    pub fn level(&self, snap: &Snap) -> String {
        if self.truncated {
            return String::new();
        }
        self.parts
            .get(1)
            .and_then(|part| snap.text(*part))
            .unwrap_or_default()
            .to_string()
    }

    pub fn level_color(&self, snap: &Snap) -> Option<Color> {
        if self.truncated {
            return None;
        }
        self.parts.get(1).and_then(|part| snap.color(*part))
    }
}

/// The drawer panel's split: the panel rect plus header/body/footer sizes.
#[derive(Clone, Copy, Debug)]
pub struct PanelGeometry {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub header: f32,
    pub body: f32,
    pub footer: f32,
    pub body_top: f32,
}

/// Everything a check needs to read about the current frame.
#[derive(Default)]
pub struct Snap {
    pub nodes: Vec<NodeSnap>,
    pub texts: HashMap<Entity, String>,
    pub colors: HashMap<Entity, Color>,
    pub children: HashMap<Entity, Vec<Entity>>,
    pub parents: HashMap<Entity, Entity>,
    pub editables: HashMap<Entity, String>,
}

/// Take the frame's snapshot. Exclusive systems hand out `&mut World`, so every
/// query runs to completion into owned data before the next borrow.
pub fn snap(world: &mut World) -> Snap {
    let mut out = Snap::default();
    {
        let mut query = world.query::<(
            Entity,
            &Node,
            &ComputedNode,
            &UiGlobalTransform,
            Option<&GlobalZIndex>,
        )>();
        for (entity, node, computed, transform, zindex) in query.iter(world) {
            let size = computed.size();
            out.nodes.push(NodeSnap {
                entity,
                shape: shape(node),
                position: transform.translation - size / 2.0,
                size,
                zindex: zindex.map_or(0, |index| index.0),
                background: None,
                border: None,
            });
        }
    }
    {
        let mut query = world.query::<(Entity, &BackgroundColor)>();
        let colors: Vec<(Entity, Color)> = query.iter(world).map(|(e, c)| (e, c.0)).collect();
        for node in &mut out.nodes {
            node.background = colors
                .iter()
                .find(|(entity, _)| *entity == node.entity)
                .map(|(_, color)| *color);
        }
    }
    {
        let mut query = world.query::<(Entity, &BorderColor)>();
        let colors: Vec<(Entity, Color)> = query.iter(world).map(|(e, c)| (e, c.left)).collect();
        for node in &mut out.nodes {
            node.border = colors
                .iter()
                .find(|(entity, _)| *entity == node.entity)
                .map(|(_, color)| *color);
        }
    }
    {
        let mut query = world.query::<(Entity, &Text)>();
        for (entity, text) in query.iter(world) {
            out.texts.insert(entity, text.as_str().to_string());
        }
    }
    {
        let mut query = world.query::<(Entity, &TextColor)>();
        for (entity, color) in query.iter(world) {
            out.colors.insert(entity, color.0);
        }
    }
    {
        let mut query = world.query::<(Entity, &Children)>();
        for (entity, children) in query.iter(world) {
            out.children
                .insert(entity, children.iter().collect::<Vec<_>>());
        }
    }
    {
        let mut query = world.query::<(Entity, &ChildOf)>();
        for (entity, parent) in query.iter(world) {
            out.parents.insert(entity, parent.0);
        }
    }
    {
        let mut query = world.query::<(Entity, &EditableText)>();
        for (entity, editable) in query.iter(world) {
            out.editables.insert(entity, editable.value().to_string());
        }
    }
    out
}

fn shape(node: &Node) -> String {
    format!(
        "w={:?} h={:?} l={:?} r={:?} t={:?} b={:?} p={:?} bd={:?} dir={:?} pos={:?} \
         ai={:?} jc={:?} gap={:?}/{:?} grow={} shrink={} disp={:?} clip={:?}",
        node.width,
        node.height,
        node.left,
        node.right,
        node.top,
        node.bottom,
        node.padding,
        node.border,
        node.flex_direction,
        node.position_type,
        node.align_items,
        node.justify_content,
        node.column_gap,
        node.row_gap,
        node.flex_grow,
        node.flex_shrink,
        node.display,
        node.overflow,
    )
}
