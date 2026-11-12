//! The full walk plan, assembled from its families in the original step order.

mod doors;
mod history;
mod input;
mod overlay;
mod projection;

use crate::walk::Step;

/// The fixture pack's full surface, reader and style walk.
pub(crate) fn walk_plan() -> Vec<Step> {
    let mut steps = Vec::new();
    steps.extend(projection::projection_steps());
    steps.extend(history::history_steps());
    steps.extend(input::input_steps());
    steps.extend(overlay::overlay_steps());
    steps.extend(doors::door_steps());
    steps
}
