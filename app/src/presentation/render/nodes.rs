//! Finding and showing a spawned surface's nodes: the box a slot sits in, the
//! tree's one `text` node for a copy key, and the visibility pass that hides what
//! the owner has nothing for.
//!
//! A node has no name but a slot, a control, a list or a copy key, so everything
//! here is read from the same pre-order walk the renderer records its entities in:
//! the tree and `Rendered::nodes` stay aligned (`docs/ui-plugin-format.md`
//! § Nodes).

use bevy::prelude::*;
use unmei_engine::ui::{Node as PluginNode, NodeKind};

use super::tree::{Bind, Rendered};

/// The box a surface's slot sits in, and the empty boxes among its own children in
/// spawn order: what a player needs when the format cannot express the place the
/// tree leaves. A node has no name but a slot, a control or a list, so the box is
/// found by the slot it holds, and both are read from the renderer's pre-order
/// walk (`docs/ui-plugin-format.md` § Nodes).
pub(crate) struct SlotHolder {
    pub entity: Entity,
    pub boxes: Vec<Entity>,
}

pub(crate) fn slot_holder(bound: &Rendered, name: &str) -> Option<SlotHolder> {
    let tree = bound.tree.as_ref()?;
    let mut seen = 0;
    let (index, children) = holder_index(tree, name, &mut seen)?;
    let mut child_index = index + 1;
    let boxes = children
        .iter()
        .filter_map(|child| {
            let here = child_index;
            let empty = matches!(&child.kind, NodeKind::Box(kind) if kind.children.is_empty());
            skip_nodes(child, &mut child_index);
            empty
                .then_some(here)
                .and_then(|at| bound.nodes.get(at).copied())
        })
        .collect();
    Some(SlotHolder {
        entity: bound.nodes.get(index).copied()?,
        boxes,
    })
}

/// The index of the first box that places `name` among its own children, with that
/// box's children. A list spawns none of its item template's nodes on the surface —
/// items are spawned per entry — so the count stops at it and stays honest.
fn holder_index<'a>(
    node: &'a PluginNode,
    name: &str,
    seen: &mut usize,
) -> Option<(usize, &'a [PluginNode])> {
    let here = *seen;
    *seen += 1;
    match &node.kind {
        NodeKind::Box(container) => {
            if container
                .children
                .iter()
                .any(|child| matches!(&child.kind, NodeKind::Slot(slot) if slot.slot == name))
            {
                return Some((here, &container.children));
            }
            container
                .children
                .iter()
                .find_map(|child| holder_index(child, name, seen))
        }
        NodeKind::Slot(slot) => slot
            .children
            .iter()
            .find_map(|child| holder_index(child, name, seen)),
        _ => None,
    }
}

/// Advance a pre-order count over one subtree, list items excluded.
fn skip_nodes(node: &PluginNode, seen: &mut usize) {
    *seen += 1;
    match &node.kind {
        NodeKind::Box(container) => {
            for child in &container.children {
                skip_nodes(child, seen);
            }
        }
        NodeKind::Slot(slot) => {
            for child in &slot.children {
                skip_nodes(child, seen);
            }
        }
        _ => {}
    }
}

/// Hide the tree's `text` node that names the copy key `key`. The format has no
/// conditional text node and a bind answers `shows` only for slots, so the player
/// hides the node it asked the renderer to spawn: a legend is drawn only when
/// there is something to walk (`docs/title-and-menu.md` § Gallery `### Layout` §1).
pub(crate) fn hide_copy_text(commands: &mut Commands, bound: &Rendered, key: &str) {
    let Some(entity) = copy_text_node(bound, key) else {
        return;
    };
    commands
        .entity(entity)
        .entry::<Node>()
        .and_modify(|mut node| node.display = Display::None);
}

/// The entity the renderer spawned for the tree's one `text` node naming `key`:
/// the tree and its nodes are recorded in the same pre-order walk, so the two stay
/// aligned (`docs/ui-plugin-format.md` § Nodes).
fn copy_text_node(bound: &Rendered, key: &str) -> Option<Entity> {
    let tree = bound.tree.as_ref()?;
    let mut index = 0;
    copy_text_index(tree, key, &bound.nodes, &mut index)
}

fn copy_text_index(
    node: &PluginNode,
    key: &str,
    nodes: &[Entity],
    index: &mut usize,
) -> Option<Entity> {
    let entity = nodes.get(*index).copied();
    *index += 1;
    if matches!(&node.kind, NodeKind::Text(text) if text.text == key) {
        return entity;
    }
    match &node.kind {
        NodeKind::Box(container) => container
            .children
            .iter()
            .find_map(|child| copy_text_index(child, key, nodes, index)),
        NodeKind::Slot(slot) => slot
            .children
            .iter()
            .find_map(|child| copy_text_index(child, key, nodes, index)),
        _ => None,
    }
}

/// Whether the owner has anything to show in this node and, for a box, in any of
/// its authored children: the same rule [`sync_visibility`] applies later, read
/// once at spawn so the first frame already draws what the pass would.
pub(super) fn shows_now(bind: &dyn Bind, node: &PluginNode) -> bool {
    match &node.kind {
        NodeKind::Slot(slot) => bind.shows(&slot.slot),
        NodeKind::Box(kind) => {
            kind.children.is_empty() || kind.children.iter().any(|child| shows_now(bind, child))
        }
        _ => true,
    }
}

/// Show or hide each node of a spawned surface from the owner's current answer.
/// A slot the owner has nothing for is hidden with its layout, and a box whose
/// authored children are all hidden is hidden with them, so the name tab goes
/// away with narration without the plugin naming a condition. The tree and its
/// nodes are recorded in the same pre-order walk, so the two stay aligned.
pub(crate) fn sync_visibility(nodes: &mut Query<&mut Node>, bound: &Rendered, bind: &dyn Bind) {
    let Some(tree) = &bound.tree else {
        return;
    };
    if bound.nodes.is_empty() {
        return;
    }
    let mut index = 0;
    apply_visibility(nodes, tree, bind, &bound.nodes, &mut index);
}

/// Hide or show one node and its subtree, returning whether it shows. The index
/// advances over the recorded nodes in the order [`spawn_node`] recorded them:
/// the node itself, then a slot's authored children or a box's children.
fn apply_visibility(
    containers: &mut Query<&mut Node>,
    node: &PluginNode,
    bind: &dyn Bind,
    nodes: &[Entity],
    index: &mut usize,
) -> bool {
    let entity = nodes.get(*index).copied();
    *index += 1;
    let shows = match &node.kind {
        NodeKind::Slot(slot) => {
            let shows = bind.shows(&slot.slot);
            for child in &slot.children {
                apply_visibility(containers, child, bind, nodes, index);
            }
            shows
        }
        NodeKind::Box(kind) => {
            let mut any = kind.children.is_empty();
            for child in &kind.children {
                any |= apply_visibility(containers, child, bind, nodes, index);
            }
            any
        }
        _ => true,
    };
    if let Some(entity) = entity {
        if let Ok(mut container) = containers.get_mut(entity) {
            let want = if shows { Display::Flex } else { Display::None };
            if container.display != want {
                container.display = want;
            }
        }
    }
    shows
}
