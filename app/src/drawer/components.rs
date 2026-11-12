//! The marker components that make the drawer tree readable as one query each.

use bevy::prelude::*;

use super::state::MonitorControl;

/// The whole drawer tree, so one despawn closes it.
#[derive(Component)]
pub(crate) struct MonitorRoot;

/// The fixed-position panel inside the full-window blocker root.
#[derive(Component)]
pub(crate) struct MonitorPanelNode;

/// The clipped log body the rows hang under `MonitorLogViewport`.
#[derive(Component)]
pub(crate) struct MonitorLogBody;

/// The log body's flexible content column, split from the scrollbar track.
#[derive(Component)]
pub(crate) struct MonitorLogViewport;

/// The reading gap that bottom-aligns a tail shorter than the viewport.
#[derive(Component)]
pub(crate) struct MonitorGapSpacer;

/// The rows' content box, shifted up as one group by the reading offset.
#[derive(Component)]
pub(crate) struct MonitorRowsContent;

/// The scrollbar's track.
#[derive(Component)]
pub(crate) struct MonitorScrollbarTrack;

/// The draggable scrollbar thumb.
#[derive(Component)]
pub(crate) struct MonitorScrollbarThumb;

/// Which part of the reader chrome a node is, so one query can sync the four
/// nodes that carry the reading geometry without four disjoint `&mut Node`
/// queries.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum MonitorReaderNode {
    Viewport,
    Rows,
    Spacer,
    Thumb,
}

/// The one-line panel-local status footer.
#[derive(Component)]
pub(crate) struct MonitorStatusText;

/// One header control, labelled by its [`MonitorControl`] meaning.
#[derive(Component, Clone, Copy)]
pub(crate) struct MonitorControlTag(pub(crate) MonitorControl);

/// The `Show everything` checkbox's own text, repainted on a view change.
#[derive(Component)]
pub(crate) struct MonitorCheckboxLabel;

/// One materialized logical row container, keyed by its admission sequence.
/// Only containers carry this; the grid children carry [`MonitorRowPart`].
/// Public so a rendered acceptance harness can match the drawn top-visible
/// event to the reader's stored anchor.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitorRow(pub u64);

/// Which grid column a row's child text renders.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum MonitorRowPart {
    Elapsed,
    Level,
    Target,
    Body,
}

/// The empty-projection state line.
#[derive(Component)]
pub(crate) struct MonitorEmptyState;

/// The unresolved-position placeholder shown while a restarted pass is still
/// locating a non-following reader's anchor. Distinct from
/// [`MonitorEmptyState`]: the view is not empty, the reader's location is simply
/// not known yet.
#[derive(Component)]
pub(crate) struct MonitorLocatingState;
