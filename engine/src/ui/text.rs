//! Every text show must resolve a face and a tone (spec §4, "Engine text",
//! rule 36).
//!
//! A `text` or `glyph` node draws words by itself, and a slot or control draws
//! the engine's. Each must name a `typography` token and a tone — its own, or
//! the nearest enclosing `Box`'s — so no string reaches the renderer without a
//! look the layout rules can measure.
//!
//! [`super::interface`] owns which slots and controls show text, and
//! [`super::node`] owns the tree and its inherited style; this pass only walks
//! them.

use std::collections::BTreeMap;

use crate::finding::{rules, Finding, Location};

use super::interface;
use super::node::{Node, NodeKind};
use super::source::display;

/// What a text show is missing, in the words the finding uses.
#[derive(Clone, Copy)]
enum Missing {
    Both,
    Typography,
    Tone,
}

/// Judge every tree's text shows.
pub(super) fn check(surfaces: &BTreeMap<String, Node>, findings: &mut Vec<Finding>) {
    for (id, tree) in surfaces {
        let file = display(&format!("surfaces/{id}.ron"));
        walk(tree, "root", &file, findings);
    }
}

fn walk(node: &Node, path: &str, file: &str, findings: &mut Vec<Finding>) {
    let shows = match &node.kind {
        NodeKind::Text(_) | NodeKind::Glyph(_) => true,
        NodeKind::Slot(slot) => interface::shows_text(&slot.slot),
        _ => false,
    };
    if shows {
        if let Some(missing) = missing(node) {
            findings.push(Finding::new(
                file,
                Location::Field(path.to_string()),
                rules::UI_TEXT_UNSTYLED,
                message(&node.kind, missing),
            ));
        }
    }
    match &node.kind {
        NodeKind::Box(container) => {
            for (index, child) in container.children.iter().enumerate() {
                walk(child, &format!("{path}.children[{index}]"), file, findings);
            }
        }
        NodeKind::Slot(slot) => {
            for (index, child) in slot.children.iter().enumerate() {
                walk(child, &format!("{path}.children[{index}]"), file, findings);
            }
        }
        NodeKind::List(list) => walk(&list.item, &format!("{path}.item"), file, findings),
        NodeKind::Image(_) | NodeKind::Glyph(_) | NodeKind::Text(_) => {}
    }
}

/// The tone a `text` or `glyph` node draws in is its `fill`, which is paint, or
/// the `color` it inherits from an enclosing `Box`.
fn missing(node: &Node) -> Option<Missing> {
    let typography = node.style.text.typography.is_some();
    let tone = match &node.kind {
        NodeKind::Text(_) | NodeKind::Glyph(_) => {
            node.style.paint.fill.is_some() || node.style.text.color.is_some()
        }
        _ => node.style.text.color.is_some(),
    };
    match (typography, tone) {
        (true, true) => None,
        (false, false) => Some(Missing::Both),
        (false, true) => Some(Missing::Typography),
        (true, false) => Some(Missing::Tone),
    }
}

fn message(kind: &NodeKind, missing: Missing) -> String {
    let what = match kind {
        NodeKind::Text(_) => "a `text` node",
        NodeKind::Glyph(_) => "a `glyph` node",
        _ => "a text slot",
    };
    let ink = match kind {
        NodeKind::Text(_) | NodeKind::Glyph(_) => "no `fill`",
        _ => "no `color`",
    };
    match missing {
        Missing::Both => format!(
            "{what} shows text with no `typography` and {ink}, and no enclosing `Box` sets either"
        ),
        Missing::Typography => {
            format!("{what} shows text with no `typography`, and no enclosing `Box` sets one")
        }
        Missing::Tone => format!("{what} shows text with {ink}, and no enclosing `Box` sets one"),
    }
}
