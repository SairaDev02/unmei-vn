//! The plugin renderer: one surface's resolved tree as Bevy UI nodes.
//!
//! The tree decides the look; the surface owner decides the words and the
//! controls. An owner implements [`Bind`] to answer both, then keeps its focus,
//! walks and actions exactly as it did over hand-built nodes (spec §11): a
//! control is named by its Keiko id, never by a node.
//!
//! A node that shows engine text draws it here — a `Text` node's copy key, a
//! `Glyph`'s ornament, a text slot's words, or a control's label. The engine's
//! text pass guarantees every one of those resolved a face and a tone (spec §4),
//! so the renderer never invents either.

use std::collections::BTreeMap;

use bevy::prelude::*;
use bevy::ui::UiSystems;
use unmei_engine::ui::{
    shows_text, AlignItems as EngineAlign, ControlStates, Dimension, Direction,
    JustifyContent as EngineJustify, Node as PluginNode, NodeKind, Padding, Position,
};

use crate::menu::MenuAction;
use crate::presentation::style::TextFaces;

use super::look::UiLook;
use super::paint;
use super::states::UiStates;
use super::text;

/// The words a slot or control draws: a copy key and the values its
/// placeholders take, so a plugin's copy override fills the same way the table's
/// text does (spec §5).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Words {
    pub key: &'static str,
    pub values: Vec<(&'static str, String)>,
}

impl Words {
    /// The words of one key, with no placeholder to fill.
    pub(crate) fn new(key: &'static str) -> Self {
        Self {
            key,
            values: Vec::new(),
        }
    }

    /// The words of one key and the values its placeholders take.
    pub(crate) fn with(key: &'static str, values: &[(&'static str, &str)]) -> Self {
        Self {
            key,
            values: values
                .iter()
                .map(|(name, value)| (*name, (*value).to_string()))
                .collect(),
        }
    }
}

/// The states a surface owner reports for one control. Focus is not one of them:
/// it changes without a rebuild, so the renderer reads it live from the menu.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ControlState {
    pub disabled: bool,
    pub lit: bool,
    pub current: bool,
}

/// What a surface owner fills in for the renderer.
pub(crate) trait Bind {
    /// The words the slot or control `name` draws, when it is one that shows
    /// text.
    fn words(&self, _name: &str) -> Option<Words> {
        None
    }

    /// The action the control `name` carries, when the owner knows it.
    fn action(&self, _name: &str) -> Option<MenuAction> {
        None
    }

    /// The states the control `name` is in now.
    fn state(&self, _name: &str) -> ControlState {
        ControlState::default()
    }
}

/// The entities one spawned surface holds: its root, its slots and controls by
/// name, and each list's item template for the owner's own binding.
#[derive(Clone, Debug, Default)]
pub(crate) struct Rendered {
    pub root: Option<Entity>,
    pub slots: BTreeMap<String, Entity>,
    pub controls: BTreeMap<String, Entity>,
    pub lists: BTreeMap<String, Entity>,
    pub items: BTreeMap<String, PluginNode>,
}

/// The words a control stands for: a reader that asks what a control is labelled
/// finds them even when a sibling slot draws them.
#[derive(Component, Clone, Debug)]
pub(crate) struct ControlLabel(pub String);

/// An image drawn with one fit, and the image's own size in pixels so a cover or
/// contain fit is measured against the box that holds it.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct UiImageFit {
    pub fit: unmei_engine::ui::Fit,
    pub size: Vec2,
}

/// One surface's render context.
pub(crate) struct Surface<'a> {
    pub look: &'a UiLook,
    pub faces: &'a TextFaces,
    pub bind: &'a dyn Bind,
}

impl Surface<'_> {
    /// Spawn the surface `id`'s tree under `parent`. It returns `None` when the
    /// look has no tree for the surface, which no build's plugin does.
    pub(crate) fn spawn(&self, id: &str, parent: &mut ChildSpawnerCommands) -> Option<Rendered> {
        let tree = self.look.surface(id)?.clone();
        let mut out = Rendered::default();
        spawn_node(self, &tree, parent, &mut out);
        Some(out)
    }

    /// Spawn one copy of a list's item template under `parent`: the owner binds
    /// the engine's items, one call each. The confirm has no list; the first
    /// surface that binds one is the contents page's ledger.
    #[allow(
        dead_code,
        reason = "the format's list kind, bound by the contents page next"
    )]
    pub(crate) fn spawn_item(
        &self,
        item: &PluginNode,
        parent: &mut ChildSpawnerCommands,
    ) -> Entity {
        let mut out = Rendered::default();
        spawn_node(self, item, parent, &mut out);
        out.root.expect("every spawned node has an entity")
    }
}

fn spawn_node(
    surface: &Surface<'_>,
    node: &PluginNode,
    parent: &mut ChildSpawnerCommands,
    out: &mut Rendered,
) {
    let paint = paint::paint(&node.style.paint);
    let mut container = layout(node);
    if let Some(width) = paint.border_width {
        container.border = width;
    }
    if let Some(radius) = paint.radius {
        container.border_radius = radius;
    }
    let mut entity = parent.spawn(container);
    if node.style.z_index != 0 {
        entity.insert(ZIndex(node.style.z_index));
    }
    out.root.get_or_insert(entity.id());
    if let Some(background) = paint.background {
        entity.insert(background);
    }
    if let Some(gradient) = paint.gradient {
        entity.insert(gradient);
    }
    if let Some(border) = paint.border_color {
        entity.insert(border);
    }
    if let Some(shadow) = paint.shadow {
        entity.insert(shadow);
    }
    if let Some(transform) = paint.transform {
        entity.insert(transform);
    }

    let name = match &node.kind {
        NodeKind::Slot(slot) => Some(slot.slot.as_str()),
        _ => None,
    };
    let action = name.and_then(|name| surface.bind.action(name));
    let state = name
        .map(|name| surface.bind.state(name))
        .unwrap_or_default();
    if let Some(action) = action {
        entity.insert(action);
        entity.insert(Button);
    }
    let states = (action.is_some() || carries_state(&node.states)).then(|| {
        UiStates::new(
            action,
            state,
            &node.states,
            &node.style,
            surface.look,
            surface.faces,
        )
    });
    if let Some(states) = &states {
        entity.insert(states.clone());
    }

    match &node.kind {
        NodeKind::Box(kind) => {
            entity.with_children(|children| {
                for child in &kind.children {
                    spawn_node(surface, child, children, out);
                }
            });
        }
        NodeKind::Image(kind) => {
            let Some((image, size)) = surface.look.image(&kind.file) else {
                return;
            };
            entity.insert(ImageNode {
                image,
                image_mode: match kind.fit {
                    unmei_engine::ui::Fit::Stretch => NodeImageMode::Stretch,
                    _ => NodeImageMode::Auto,
                },
                ..default()
            });
            if matches!(
                kind.fit,
                unmei_engine::ui::Fit::Cover | unmei_engine::ui::Fit::Contain
            ) {
                entity.insert(UiImageFit {
                    fit: kind.fit,
                    size,
                });
            }
        }
        NodeKind::Glyph(kind) => {
            entity.insert(Text::new(kind.glyph.clone()));
            text::style(&mut entity, surface, &node.style);
        }
        NodeKind::Text(kind) => {
            entity.insert(Text::new(surface.look.copy(&kind.text).to_string()));
            text::style(&mut entity, surface, &node.style);
        }
        NodeKind::Slot(kind) => {
            out.slots.insert(kind.slot.clone(), entity.id());
            if action.is_some() {
                out.controls.insert(kind.slot.clone(), entity.id());
                if let Some(word) = surface.bind.words(&kind.slot) {
                    entity.insert(ControlLabel(surface.look.words(&word)));
                }
            }
            let words = if shows_text(&kind.slot) {
                surface
                    .bind
                    .words(&kind.slot)
                    .map(|word| surface.look.words(&word))
            } else {
                None
            };
            let style = node.style.clone();
            let states = states.clone();
            entity.with_children(|children| {
                if let Some(words) = words {
                    let mut text = children.spawn(Text::new(words));
                    text::style(&mut text, surface, &style);
                    if let Some(states) = states.clone() {
                        text.insert(states);
                    }
                }
                for child in &kind.children {
                    spawn_node(surface, child, children, out);
                }
            });
        }
        NodeKind::List(kind) => {
            out.lists.insert(kind.list.clone(), entity.id());
            out.items.insert(kind.list.clone(), (*kind.item).clone());
        }
    }
}

fn carries_state(states: &ControlStates) -> bool {
    states.focused.is_some()
        || states.disabled.is_some()
        || states.lit.is_some()
        || states.current.is_some()
}

/// The layout half of a node: the flex container and the size, position and
/// offsets every kind takes. A list is a column of its items; its own entries
/// come from the owner.
fn layout(node: &PluginNode) -> Node {
    let style = &node.style;
    let mut container = Node {
        width: dimension(style.width),
        height: dimension(style.height),
        position_type: match style.position {
            Position::Relative => PositionType::Relative,
            Position::Absolute => PositionType::Absolute,
        },
        left: offset(style.offsets.left),
        top: offset(style.offsets.top),
        right: offset(style.offsets.right),
        bottom: offset(style.offsets.bottom),
        ..default()
    };
    match &node.kind {
        NodeKind::Box(kind) => {
            container.flex_direction = match kind.direction {
                Direction::Row => FlexDirection::Row,
                Direction::Column => FlexDirection::Column,
            };
            container.row_gap = Val::Px(kind.gap);
            container.column_gap = Val::Px(kind.gap);
            container.padding = match kind.padding {
                Padding::Uniform(value) => UiRect::all(Val::Px(value)),
                Padding::Sides {
                    top,
                    right,
                    bottom,
                    left,
                } => UiRect {
                    top: Val::Px(top),
                    right: Val::Px(right),
                    bottom: Val::Px(bottom),
                    left: Val::Px(left),
                },
            };
            container.align_items = match kind.align_items {
                EngineAlign::Start => AlignItems::FlexStart,
                EngineAlign::Center => AlignItems::Center,
                EngineAlign::End => AlignItems::FlexEnd,
                EngineAlign::Stretch => AlignItems::Stretch,
            };
            container.justify_content = match kind.justify_content {
                EngineJustify::Start => JustifyContent::FlexStart,
                EngineJustify::Center => JustifyContent::Center,
                EngineJustify::End => JustifyContent::FlexEnd,
                EngineJustify::SpaceBetween => JustifyContent::SpaceBetween,
            };
        }
        NodeKind::List(_) => {
            container.flex_direction = FlexDirection::Column;
        }
        _ => {}
    }
    container
}

fn dimension(value: Dimension) -> Val {
    match value {
        Dimension::Auto => Val::Auto,
        Dimension::Fill => Val::Percent(100.0),
        Dimension::Px(value) => Val::Px(value),
    }
}

fn offset(value: Option<f32>) -> Val {
    value.map_or(Val::Auto, Val::Px)
}

/// Size a cover or contain image against the box that holds it, after layout.
pub(crate) fn sync_image_fit(
    mut nodes: Query<(&UiImageFit, &ChildOf, &mut Node)>,
    parents: Query<&ComputedNode>,
) {
    for (fit, child_of, mut node) in &mut nodes {
        let Ok(parent) = parents.get(child_of.parent()) else {
            continue;
        };
        let box_size = parent.size * parent.inverse_scale_factor;
        let size = match fit.fit {
            unmei_engine::ui::Fit::Cover => {
                crate::presentation::style::cover_size(fit.size, box_size)
            }
            unmei_engine::ui::Fit::Contain => {
                crate::presentation::style::contain_size(fit.size, box_size)
            }
            _ => continue,
        };
        if size.x <= 0.0 || size.y <= 0.0 {
            continue;
        }
        node.position_type = PositionType::Absolute;
        node.width = Val::Px(size.x);
        node.height = Val::Px(size.y);
        node.left = Val::Px((box_size.x - size.x) / 2.0);
        node.top = Val::Px((box_size.y - size.y) / 2.0);
    }
}

/// Register the renderer: nothing per frame but the image fits, which need the
/// layout to have run.
pub(crate) fn register(app: &mut App) {
    app.add_systems(
        PostUpdate,
        sync_image_fit
            .after(UiSystems::Layout)
            // First in the `PostUpdate` geometry group: a cover or contain
            // image's size is this frame's, and the group's own fits and focus
            // scrolls read the sizes the renderer wrote.
            .before(crate::presentation::gallery::sync_gallery_columns)
            .before(UiSystems::PostLayout),
    );
}
