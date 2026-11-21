//! The pack's UI plugin: the `ui/` folder, the manifest it holds and the
//! folder-level rules that decide whether a player reads it (UI plugin spec
//! §2, §7, §8).
//!
//! The module is Bevy-free, and every source reaches the rules through one
//! function, [`load_from`]: the check, the boot, the embedded reference plugin
//! and the export therefore accept exactly the same plugins (R24). Everything
//! a plugin's files must satisfy becomes a [`Finding`] in its own outcome and
//! never a reason for [`crate::load_pack`] to fail, because the pack still
//! plays (R14).
//!
//! The passes run in one order — the paths a source lists, the folder's caps,
//! the manifest and its copy overrides, the tokens, the fonts, then each surface
//! tree and its interface — and each pass ends the plugin when it found an
//! error, so a folder that breaks a cap is never read on, a token table that
//! failed is never resolved in a tree and a font that no file carries never
//! reaches the renderer. A tree's loops are judged by §6 while its surface
//! parses it, which is where the tree and the surface id meet; its slots are
//! judged against the surface's table once every tree has parsed, because a
//! table speaks about a whole tree.

mod caps;
mod file;
mod fonts;
mod interface;
mod manifest;
mod motion;
mod node;
mod overrides;
mod paint;
mod slots;
mod source;
mod style;
mod surface;
mod tokens;
mod value;

use std::collections::BTreeMap;
use std::io;
use std::path::Path;

use crate::finding::{rules, sort_findings, Finding, Location};

pub use self::caps::{
    MAX_COPY_CHARS, MAX_DEPTH, MAX_FILES, MAX_FILE_BYTES, MAX_IMAGE_SIDE, MAX_NODES,
    MAX_TOTAL_BYTES,
};
pub use self::fonts::{embedded, EmbeddedFace, EMBEDDED_FACES};
pub use self::interface::{surface, ListSpec, SurfaceSpec, SURFACES};
pub use self::manifest::{FontDecl, ImageDecl, Manifest, Numbering, FORMAT_VERSION};
pub use self::motion::{Easing, Keyframe, Motion, MAX_FLASHES_PER_SECOND, MIN_PERIOD_MS};
pub use self::node::{
    BoxNode, ControlStates, Fit, GlyphNode, ImageNode, ListNode, Node, NodeKind, SlotNode,
    TextNode, MAX_GLYPH_CHARS,
};
pub use self::paint::{
    Border, Fill, GradientStop, Paint, Radius, Shadow, StateOverrides, Transform,
};
pub use self::source::{DirSource, MemorySource, Source};
pub use self::style::{
    AlignItems, Component, Dimension, Direction, JustifyContent, Offsets, Padding, Position, Style,
};
pub use self::surface::{ORNAMENT_SLOTS, SURFACE_IDS};
pub use self::tokens::{Color, Tokens, Typography};

use self::source::display;

/// The folder a plugin lives in, relative to the pack root.
const FOLDER: &str = "ui";

/// A plugin that passed every static rule: the manifest this build reads, its
/// resolved tokens and the surface trees it restyles. A surface the plugin has
/// no tree for keeps the reference plugin's, so the map is the plugin's own
/// restyling and not a complete set of ten.
#[derive(Clone, Debug, PartialEq)]
pub struct UiPlugin {
    pub manifest: Manifest,
    pub tokens: Tokens,
    pub surfaces: BTreeMap<String, Node>,
}

/// What a pack's `ui/` folder gives the player (spec §8): no folder at all, a
/// plugin this build reads, or the findings that stopped it. A failure is the
/// plugin's alone; the pack still plays.
///
/// A loaded plugin owns its manifest, resolved tokens and trees, which makes it
/// the large variant. The outcome lives once per pack rather than in a
/// collection, so boxing it would add an indirection to every read and change
/// the shape ticket #105 published.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub enum PluginOutcome {
    /// The pack has no `ui/` folder, which is not a failure.
    Absent,
    /// The plugin passed every static rule.
    Loaded(UiPlugin),
    /// The plugin's findings, in the order the check reports them.
    Failed(Vec<Finding>),
}

impl PluginOutcome {
    /// The plugin's findings: none when it is absent or loaded. Every static
    /// plugin rule is an error (spec §8), so a loaded plugin keeps none.
    pub fn findings(&self) -> &[Finding] {
        match self {
            PluginOutcome::Absent | PluginOutcome::Loaded(_) => &[],
            PluginOutcome::Failed(findings) => findings,
        }
    }

    /// The plugin the player renders from, when it loaded.
    pub fn plugin(&self) -> Option<&UiPlugin> {
        match self {
            PluginOutcome::Loaded(plugin) => Some(plugin),
            PluginOutcome::Absent | PluginOutcome::Failed(_) => None,
        }
    }

    fn failed(mut findings: Vec<Finding>) -> Self {
        sort_findings(&mut findings);
        PluginOutcome::Failed(findings)
    }
}

/// Read the plugin in `pack_root`'s `ui/` folder (spec §2).
pub fn load(pack_root: &Path) -> PluginOutcome {
    let folder = pack_root.join(FOLDER);
    match std::fs::metadata(&folder) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => PluginOutcome::Absent,
        Err(error) => PluginOutcome::failed(vec![Finding::new(
            display(""),
            Location::File,
            rules::UI_FILE_READ,
            format!("cannot read the plugin folder: {error}"),
        )]),
        Ok(_) => match DirSource::open(&folder) {
            Ok(source) => load_from(&source),
            Err(findings) => PluginOutcome::failed(findings),
        },
    }
}

/// Run every static plugin rule over one source (spec §8, R24).
pub fn load_from(source: &dyn Source) -> PluginOutcome {
    // §7's confinement comes first: a path is checked before any file it names
    // is opened, and a source still holding one that leaves the folder stops
    // here.
    let findings = source::check_paths(source);
    if !findings.is_empty() {
        return PluginOutcome::failed(findings);
    }

    let mut findings = Vec::new();
    caps::check(source, &mut findings);
    // The passes after this one open files, and a folder over a cap is exactly
    // what the caps say not to open, so its content shapes no finding.
    if !findings.is_empty() {
        return PluginOutcome::failed(findings);
    }

    let Some(manifest) = manifest::check(source, &mut findings) else {
        return PluginOutcome::failed(findings);
    };
    if !findings.is_empty() {
        return PluginOutcome::failed(findings);
    }

    // An override is judged as soon as its manifest is read: no other file
    // decides whether a key or its placeholders are the table's.
    overrides::check(&manifest, &mut findings);
    if !findings.is_empty() {
        return PluginOutcome::failed(findings);
    }

    let Some(tokens) = tokens::check(source, &mut findings) else {
        return PluginOutcome::failed(findings);
    };
    if !findings.is_empty() {
        return PluginOutcome::failed(findings);
    }

    fonts::check(source, &manifest, &tokens, &mut findings);
    if !findings.is_empty() {
        return PluginOutcome::failed(findings);
    }

    let Some(surfaces) = surface::check(source, &tokens, &mut findings) else {
        return PluginOutcome::failed(findings);
    };
    if !findings.is_empty() {
        return PluginOutcome::failed(findings);
    }

    slots::check(&surfaces, &mut findings);
    if !findings.is_empty() {
        return PluginOutcome::failed(findings);
    }
    PluginOutcome::Loaded(UiPlugin {
        manifest,
        tokens,
        surfaces,
    })
}
