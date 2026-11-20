//! `ui/surfaces/<surface>.ron`: the ten surface ids of format version 1, the
//! tree files that carry them, and the rule a tree for any other id breaks
//! (spec §4, §5).
//!
//! A surface with no tree file keeps the reference plugin's tree, so a missing
//! file is not a failure. A tree file whose name is not one of the ten is: the
//! author asked to restyle a surface this format version does not have, and
//! silently ignoring the file would hide the typo. The tree itself is parsed by
//! [`super::node`], which owns its node paths and caps, and judged for §6's loop
//! rules here, where the tree meets its surface id.

use std::collections::BTreeMap;

use crate::finding::{rules, Finding, Location};

use super::file;
use super::motion;
use super::node::{self, Node};
use super::source::Source;
use super::tokens::Tokens;

/// The surfaces format version 1 has (spec §5).
pub const SURFACE_IDS: [&str; 10] = [
    "title",
    "title-settings",
    "play",
    "contents",
    "settings",
    "gallery",
    "still",
    "timeline",
    "legacy-picker",
    "confirm",
];

const FOLDER: &str = "surfaces/";

/// The slots that carry a plugin's own ornament rather than engine content
/// (spec §5). Loops are allowed inside these two and refused on the slots
/// themselves (spec §6).
pub const ORNAMENT_SLOTS: [&str; 2] = ["advance-marker", "deciding-marker"];

/// Read every surface tree the source holds. `None` means at least one tree
/// failed; the plugin keeps none of them, because the player draws the
/// reference look on every surface when any static rule fails.
pub(super) fn check(
    source: &dyn Source,
    tokens: &Tokens,
    findings: &mut Vec<Finding>,
) -> Option<BTreeMap<String, Node>> {
    let mut surfaces = BTreeMap::new();
    let mut ok = true;
    for path in source.paths() {
        let Some(id) = surface_id(path) else {
            continue;
        };
        let file = source.display(path);
        if !SURFACE_IDS.contains(&id) {
            findings.push(Finding::new(
                file,
                Location::File,
                rules::UI_SURFACE_UNKNOWN,
                format!("`{id}` is not a surface this format version has"),
            ));
            ok = false;
            continue;
        }
        match read(source, path, &file, tokens, findings) {
            Some(tree) => {
                ok &= motion::check(id, &tree, &file, findings);
                surfaces.insert(id.to_string(), tree);
            }
            None => ok = false,
        }
    }
    ok.then_some(surfaces)
}

/// The surface a tree path names: exactly `surfaces/<id>.ron`, never a file
/// nested below it.
fn surface_id(path: &str) -> Option<&str> {
    let name = path.strip_prefix(FOLDER)?.strip_suffix(".ron")?;
    if name.is_empty() || name.contains('/') {
        return None;
    }
    Some(name)
}

fn read(
    source: &dyn Source,
    path: &str,
    file: &str,
    tokens: &Tokens,
    findings: &mut Vec<Finding>,
) -> Option<Node> {
    let value = file::read_ron(source, path, "surface tree", findings)?;
    node::parse_tree(value, tokens, file, findings)
}
