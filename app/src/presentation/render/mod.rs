//! The plugin renderer: a surface's resolved tree as Bevy UI nodes, and the look
//! a boot draws with (spec §11).
//!
//! Each surface keeps its one owner ([ADR 0013](../../../docs/adr/0013-presentation-surface-partition.md)):
//! the owner asks [`tree::Surface`] for the trees it needs, fills the slots that
//! are its own, and keeps its focus, walks and actions keyed by control id. The
//! renderer owns only what a tree decides — the layout, the paint, the faces and
//! the four states — so no surface code reads a node.
//!
//! [`look`] resolves what to draw with once per boot: the pack's plugin when it
//! loaded, the embedded reference plugin otherwise, and the fonts and images
//! either ships, read as bytes through the engine's confined source.

pub(crate) mod look;
mod motion;
mod paint;
mod states;
mod text;
pub(crate) mod tree;

/// The paint repaint, for a test that asserts what a state draws.
#[cfg(test)]
pub(crate) use states::sync_paint_states;

pub(crate) use look::{load_look, UiLook};
pub(crate) use states::UiStates;
pub(crate) use tree::{Bind, ControlLabel, Surface, Words};

use bevy::prelude::*;

use crate::presentation::{sync_menu_ui, PresentationSet};

/// The repaint systems, which write a node's own components. A surface that
/// shows and hides its slots' nodes itself orders before them, because both
/// answer for the same components.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Repaint;

/// Register the renderer: the look once the pack is loaded, the state repaints
/// every frame, and the image fits after layout.
pub(crate) fn register(app: &mut App) {
    app.add_systems(
        Update,
        load_look
            .before(sync_menu_ui)
            .in_set(PresentationSet)
            .run_if(not(resource_exists::<UiLook>)),
    );
    states::register(app);
    tree::register(app);
    app.add_systems(
        Update,
        motion::sync_loops
            .after(Repaint)
            .in_set(crate::presentation::PresentationSet),
    );
}
