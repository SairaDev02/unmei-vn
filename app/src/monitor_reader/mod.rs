//! The Monitor's one older-line reader: logical position, follow choice,
//! expiry acknowledgement, locating state and the one shared row metric
//! derivation.
//!
//! The drawer owns everything the developer touches — visibility, the display
//! predicate, focus, held-input quarantine, real input precedence, wheel
//! hit-testing, scrollbar drag lifetime, rendering and status — and calls this
//! module's concrete operations instead of manipulating reader fields. The
//! reader is **Bevy-free**: the drawer supplies the current window/font
//! geometry and the borrowed projection rows, and reads back a view it can
//! materialize in pixels.
//!
//! Settled design: `docs/runtime-hardening.md` § Monitor → Reading anchor. The
//! module decides no behaviour of its own; it is the seam that extraction
//! chose, holding the stable anchor, the one metric formula and the ordered
//! operations. The reading rules themselves are Monitor → Interaction's
//! stable-reading-anchor contract.

mod metrics;
mod reader;
#[cfg(test)]
mod tests;
mod walk;

pub(crate) use metrics::Metrics;
pub(crate) use reader::{Reader, ReaderFrame};

/// The reader's stable position while not following: the **top visible
/// retained event**'s admission identity plus the **within-row reading
/// offset**, measured against that row's actual laid-out extent.
///
/// The identity is durable — it survives arrivals, hide/reopen, a filter
/// refresh and rewrap — and the offset is clamped when the anchor row's own
/// laid-out wrapping changes. It is deliberately *not* a display index and
/// *not* a pixel count.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonitorAnchor {
    /// The anchor event's retained admission identity/order.
    pub sequence: u64,
    /// Display lines scrolled past the anchor row's top, in single-line row
    /// units of that row's laid-out height.
    pub offset_lines: f32,
}
