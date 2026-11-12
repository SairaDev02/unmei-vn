//! The stable surface signatures the walk compares across a Monitor frame.

use super::Snap;

impl Snap {
    /// A stable signature of every laid-out node outside the drawer: shape,
    /// place, size, fill and border. Text *leaves* are excluded because their
    /// glyph box legitimately grows with the live reveal; their containers are
    /// included, so a moved, resized, dimmed or reflowed surface still fails.
    pub fn underlying(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .nodes
            .iter()
            .filter(|node| !self.under_drawer(node.entity))
            .filter(|node| !self.texts.contains_key(&node.entity))
            .map(|node| {
                format!(
                    "{}|{:.1},{:.1}|{:.1},{:.1}|{:?}|{:?}",
                    node.shape,
                    node.position.x,
                    node.position.y,
                    node.size.x,
                    node.size.y,
                    node.background,
                    node.border,
                )
            })
            .collect();
        out.sort();
        out
    }

    /// The four HUD slots the drawer must leave unmoved and undimmed while it is
    /// open over play: help, Notice, the dialogue band and the cue slot.
    pub fn hud(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .nodes
            .iter()
            .filter_map(|node| {
                let signature = hud_signature(&node.shape)?;
                Some(format!(
                    "{signature}|{:.1},{:.1}|{:.1},{:.1}|{:?}",
                    node.position.x, node.position.y, node.size.x, node.size.y, node.background,
                ))
            })
            .collect();
        out.sort();
        out
    }

    /// The edit field's live value (the suspended Settings key draft).
    pub fn draft(&self) -> Option<String> {
        self.editables.values().next().cloned()
    }
}

fn hud_signature(shape: &str) -> Option<&'static str> {
    if shape.contains("t=Px(12.0)") && shape.contains("l=Px(16.0)") {
        Some("help")
    } else if shape.contains("t=Px(12.0)") && shape.contains("r=Px(48.0)") {
        Some("notice")
    } else if shape.contains("b=Px(40.0)")
        && shape.contains("l=Px(48.0)")
        && shape.contains("r=Px(160.0)")
    {
        Some("dialogue")
    } else if shape.contains("b=Px(40.0)") && shape.contains("r=Px(48.0)") {
        Some("cue")
    } else {
        None
    }
}
