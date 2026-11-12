//! The HUD snapshot and comparison the walk uses to prove the surface held.

use crate::prelude::*;

// ---------------------------------------------------------------------------
// HUD stability
// ---------------------------------------------------------------------------

/// Every node that is one of the four HUD elements the drawer must leave alone:
/// the help line, the Notice slot, the dialogue band and the cue slot.
pub(crate) fn snapshot_hud(
    nodes: &Query<(Entity, &Node, &ComputedNode, &UiGlobalTransform)>,
) -> Vec<HudNode> {
    let mut out: Vec<HudNode> = nodes
        .iter()
        .filter_map(|(_, node, computed, transform)| {
            let signature = hud_signature(node)?;
            Some(HudNode {
                signature,
                position: transform.translation,
                size: computed.size(),
            })
        })
        .collect();
    out.sort_by(|a, b| a.signature.cmp(&b.signature));
    out
}

pub(crate) fn hud_signature(node: &Node) -> Option<String> {
    let fixed = |value: Val| match value {
        Val::Px(value) => Some(value),
        _ => None,
    };
    let signature = if fixed(node.top) == Some(12.0) && fixed(node.left) == Some(16.0) {
        "help"
    } else if fixed(node.top) == Some(12.0) && fixed(node.right) == Some(48.0) {
        "notice"
    } else if fixed(node.bottom) == Some(40.0)
        && fixed(node.left) == Some(48.0)
        && fixed(node.right) == Some(160.0)
    {
        "dialogue"
    } else if fixed(node.bottom) == Some(40.0) && fixed(node.right) == Some(48.0) {
        "cue"
    } else {
        return None;
    };
    Some(signature.to_string())
}

pub(crate) fn compare_hud(before: &[HudNode], after: &[HudNode]) {
    for expected in ["help", "notice", "dialogue", "cue"] {
        if !before.iter().any(|node| node.signature == expected) {
            eprintln!("FAILED: the HUD snapshot is missing {expected}");
            std::process::exit(1);
        }
    }
    println!("HUD before the drawer: {before:?}");
    if before != after {
        eprintln!("FAILED: the drawer moved or resized the HUD");
        eprintln!("  after: {after:?}");
        std::process::exit(1);
    }
    println!("HUD stayed unmoved while the drawer was open");
}
