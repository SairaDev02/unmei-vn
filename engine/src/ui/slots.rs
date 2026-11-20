//! The interface rules a surface tree obeys (spec §5, §8): the slots, controls
//! and lists it places are the ones its surface has, everything the table
//! requires is placed exactly once, and a `text` node names a copy key its
//! surface may show.
//!
//! [`super::interface`] owns the names and this pass owns the walk, so neither
//! knows a surface's layout. A required name that is missing from the tree has
//! no node to point at: that finding names the tree, and its message says which
//! name the tree forgot.
//!
//! A slot other than [`ORNAMENT_SLOTS`] holds engine content, so content inside
//! one is `ui.slot.unknown`. The slot is what cannot take it, which is why the
//! finding is the slot family's rather than a parser's: the node kind accepts
//! `children`, and the name decides whether it may.

use std::collections::BTreeMap;

use crate::copy;
use crate::finding::{rules, Finding, Location};

use super::interface::{self, ListSpec, SurfaceSpec};
use super::node::{Node, NodeKind};
use super::source::display;
use super::surface::ORNAMENT_SLOTS;

/// Judge every tree the plugin holds against its surface's table.
pub(super) fn check(surfaces: &BTreeMap<String, Node>, findings: &mut Vec<Finding>) {
    for (id, tree) in surfaces {
        let Some(spec) = interface::surface(id) else {
            // A tree for a surface this format version has no table for already
            // failed with `ui.surface.unknown`, and that pass stopped the
            // plugin.
            continue;
        };
        let file = display(&format!("surfaces/{id}.ron"));
        let mut placed = Placed::default();
        walk(spec, tree, "root", &mut placed, &file, findings);
        missing(spec, &placed, &file, findings);
    }
}

/// What one tree placed, so the required names can be answered after the walk.
#[derive(Default)]
struct Placed {
    /// Every name placed at the surface, and the path of the node that placed
    /// it. Slots, controls and list slots share this space: one name placed
    /// twice is a duplicate whichever kind each placement is.
    names: Vec<(String, String)>,
    /// The lists the tree placed, by table.
    lists: Vec<&'static ListSpec>,
}

fn walk(
    spec: &SurfaceSpec,
    node: &Node,
    path: &str,
    placed: &mut Placed,
    file: &str,
    findings: &mut Vec<Finding>,
) {
    match &node.kind {
        NodeKind::Box(box_node) => {
            walk_children(spec, &box_node.children, path, placed, file, findings)
        }
        NodeKind::Image(_) | NodeKind::Glyph(_) => {}
        NodeKind::Text(text) => check_text(spec, &text.text, path, file, findings),
        NodeKind::Slot(slot) => {
            place(spec, &slot.slot, path, placed, file, findings);
            if !ORNAMENT_SLOTS.contains(&slot.slot.as_str()) && !slot.children.is_empty() {
                report(
                    file,
                    &format!("{path}.children"),
                    rules::UI_SLOT_UNKNOWN,
                    format!(
                        "the `{}` slot holds content; only the ornament slots `{}` and `{}` do",
                        slot.slot, ORNAMENT_SLOTS[0], ORNAMENT_SLOTS[1]
                    ),
                    findings,
                );
            }
            walk_children(spec, &slot.children, path, placed, file, findings);
        }
        NodeKind::List(list) => place_list(spec, list, path, placed, file, findings),
    }
}

fn walk_children(
    spec: &SurfaceSpec,
    nodes: &[Node],
    path: &str,
    placed: &mut Placed,
    file: &str,
    findings: &mut Vec<Finding>,
) {
    for (index, node) in nodes.iter().enumerate() {
        walk(
            spec,
            node,
            &format!("{path}.children[{index}]"),
            placed,
            file,
            findings,
        );
    }
}

fn place(
    spec: &SurfaceSpec,
    name: &str,
    path: &str,
    placed: &mut Placed,
    file: &str,
    findings: &mut Vec<Finding>,
) {
    let known = spec.slots.contains(&name)
        || spec.optional_slots.contains(&name)
        || spec.controls.contains(&name);
    if !known {
        report(
            file,
            &format!("{path}.slot"),
            rules::UI_SLOT_UNKNOWN,
            format!(
                "the `{}` surface has no slot or control named `{name}`",
                spec.id
            ),
            findings,
        );
        return;
    }
    stage(placed, name, path, spec.id, file, findings);
}

/// Record `name` as placed on the surface, reporting it when it was placed
/// before. A control is one control whichever node places it, so a list's item
/// template and the surface's own tree share this bookkeeping.
fn stage(
    placed: &mut Placed,
    name: &str,
    path: &str,
    surface: &str,
    file: &str,
    findings: &mut Vec<Finding>,
) {
    if placed.names.iter().any(|(placed, _)| placed == name) {
        report(
            file,
            &format!("{path}.slot"),
            rules::UI_SLOT_DUPLICATE,
            format!("the `{surface}` surface places `{name}` twice"),
            findings,
        );
        return;
    }
    placed.names.push((name.to_string(), path.to_string()));
}

fn place_list(
    spec: &SurfaceSpec,
    list: &super::node::ListNode,
    path: &str,
    placed: &mut Placed,
    file: &str,
    findings: &mut Vec<Finding>,
) {
    let Some(list_spec) = spec.lists.iter().find(|spec| spec.slot == list.list) else {
        report(
            file,
            &format!("{path}.list"),
            rules::UI_SLOT_UNKNOWN,
            format!(
                "the `{}` surface has no list named `{}`",
                spec.id, list.list
            ),
            findings,
        );
        return;
    };
    stage(placed, list_spec.slot, path, spec.id, file, findings);
    placed.lists.push(list_spec);
    check_item(
        list_spec,
        spec,
        &list.item,
        &format!("{path}.item"),
        placed,
        file,
        findings,
    );
}

/// One list's item template: what it places, and what the list requires of it.
fn check_item(
    list: &'static ListSpec,
    spec: &SurfaceSpec,
    item: &Node,
    path: &str,
    surface_placed: &mut Placed,
    file: &str,
    findings: &mut Vec<Finding>,
) {
    let mut placed: Vec<String> = Vec::new();
    walk_item(
        list,
        spec,
        item,
        path,
        &mut placed,
        surface_placed,
        file,
        findings,
    );
    for name in list.item_slots {
        if !placed.iter().any(|identity| identity == name) {
            report(
                file,
                "",
                rules::UI_SLOT_MISSING,
                format!("the list `{}` requires the item slot `{name}`", list.slot),
                findings,
            );
        }
    }
    for control in list.item_controls {
        if !placed.iter().any(|identity| identity == control) {
            report(
                file,
                "",
                rules::UI_SLOT_MISSING,
                format!(
                    "the list `{}` requires the item control `{control}`",
                    list.slot
                ),
                findings,
            );
        }
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "one walk, one fact per argument: the surface's table, the list's, the node, its path, the item's placements, the surface's, the file and the sink; they are all distinct facts the walk needs, so a context struct would move the count rather than the coupling"
)]
fn walk_item(
    list: &'static ListSpec,
    spec: &SurfaceSpec,
    node: &Node,
    path: &str,
    placed: &mut Vec<String>,
    surface_placed: &mut Placed,
    file: &str,
    findings: &mut Vec<Finding>,
) {
    match &node.kind {
        NodeKind::Box(box_node) => item_children(
            list,
            spec,
            &box_node.children,
            path,
            placed,
            surface_placed,
            file,
            findings,
        ),
        NodeKind::Image(_) | NodeKind::Glyph(_) => {}
        NodeKind::Text(text) => check_text(spec, &text.text, path, file, findings),
        NodeKind::Slot(slot) => {
            match item_identity(list, &slot.slot) {
                Some(identity) => {
                    if placed.contains(&identity) {
                        report(
                            file,
                            &format!("{path}.slot"),
                            rules::UI_SLOT_DUPLICATE,
                            format!(
                                "the list `{}` places `{identity}` twice in one item",
                                list.slot
                            ),
                            findings,
                        );
                    } else {
                        if !is_item_slot(list, &identity) {
                            // A control is one control on the surface: the item
                            // template places the item's own, and the surface may
                            // not place the same one again.
                            stage(surface_placed, &identity, path, spec.id, file, findings);
                        }
                        placed.push(identity);
                    }
                }
                None => report(
                    file,
                    &format!("{path}.slot"),
                    rules::UI_SLOT_UNKNOWN,
                    format!(
                        "the list `{}` has no item slot or control named `{}`",
                        list.slot, slot.slot
                    ),
                    findings,
                ),
            }
            item_children(
                list,
                spec,
                &slot.children,
                path,
                placed,
                surface_placed,
                file,
                findings,
            );
        }
        NodeKind::List(inner) => report(
            file,
            &format!("{path}.list"),
            rules::UI_SLOT_UNKNOWN,
            format!(
                "a `{}` item places no list, and `{}` is not one of its slots",
                list.slot, inner.list
            ),
            findings,
        ),
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "the item walk's recursion passes the same distinct facts straight through, and the two placement ledgers have to stay separate because an item's identity and the surface's are counted apart"
)]
fn item_children(
    list: &'static ListSpec,
    spec: &SurfaceSpec,
    nodes: &[Node],
    path: &str,
    placed: &mut Vec<String>,
    surface_placed: &mut Placed,
    file: &str,
    findings: &mut Vec<Finding>,
) {
    for (index, node) in nodes.iter().enumerate() {
        walk_item(
            list,
            spec,
            node,
            &format!("{path}.children[{index}]"),
            placed,
            surface_placed,
            file,
            findings,
        );
    }
}

/// Whether an item identity is one of the list's item slots rather than one of
/// its controls.
fn is_item_slot(list: &ListSpec, identity: &str) -> bool {
    list.item_slots.contains(&identity) || list.optional_item_slots.contains(&identity)
}

/// The identity a name has inside `list`'s item template. A family member
/// answers its family, because one item shows one control of it, so a template
/// placing two members places the item's control twice.
fn item_identity(list: &ListSpec, name: &str) -> Option<String> {
    if list.item_slots.contains(&name) || list.optional_item_slots.contains(&name) {
        return Some(name.to_string());
    }
    list.item_controls.iter().find_map(|declared| {
        let member = *declared == name || is_member(declared, name);
        member.then(|| (*declared).to_string())
    })
}

/// Whether `name` is a member of the indexed family `declared`: `gallery.card.<n>`
/// answers `gallery.card.3`. Keiko owns what an indexed id is; a table owns
/// which family its list places.
fn is_member(declared: &str, name: &str) -> bool {
    declared
        .strip_suffix(".<n>")
        .is_some_and(|family| name.starts_with(family) && crate::keiko::is_control_id(name))
}

/// The names the table requires and the tree did not place.
fn missing(spec: &SurfaceSpec, placed: &Placed, file: &str, findings: &mut Vec<Finding>) {
    for name in spec.slots {
        if !placed.names.iter().any(|(placed, _)| placed == name) {
            report(
                file,
                "",
                rules::UI_SLOT_MISSING,
                format!("the `{}` surface requires the slot `{name}`", spec.id),
                findings,
            );
        }
    }
    for control in spec.controls {
        // A control that is also a required slot is one name, and the slot's own
        // finding already names it.
        if spec.slots.contains(control) {
            continue;
        }
        let carried = placed
            .lists
            .iter()
            .any(|list| list.item_controls.contains(control));
        if !carried && !placed.names.iter().any(|(placed, _)| placed == control) {
            report(
                file,
                "",
                rules::UI_SLOT_MISSING,
                format!("the `{}` surface requires the control `{control}`", spec.id),
                findings,
            );
        }
    }
}

fn check_text(spec: &SurfaceSpec, key: &str, path: &str, file: &str, findings: &mut Vec<Finding>) {
    let message = match copy::key(key) {
        None => format!("the copy table has no key `{key}`"),
        Some(_) if !spec.copy_keys.contains(&key) => {
            format!("`{key}` is not a `{}` copy key", spec.id)
        }
        Some(_) => return,
    };
    report(
        file,
        &format!("{path}.text"),
        rules::UI_COPY_UNKNOWN,
        message,
        findings,
    );
}

fn report(file: &str, path: &str, rule: &str, message: String, findings: &mut Vec<Finding>) {
    let location = if path.is_empty() {
        Location::File
    } else {
        Location::Field(path.to_string())
    };
    findings.push(Finding::new(file, location, rule, message));
}
