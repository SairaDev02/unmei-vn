//! The rendered acceptance gate's four plans.

mod broken;
mod gate;
mod hold;
mod walk;

use crate::mode::Mode;
use crate::walk::Step;

/// The plan for `mode`, in its original step order.
pub(crate) fn plan(mode: Mode) -> Vec<Step> {
    match mode {
        Mode::Walk => walk::walk_plan(),
        Mode::Hold => hold::hold_plan(),
        Mode::Broken => broken::broken_plan(),
        Mode::Gate => gate::gate_plan(),
    }
}
