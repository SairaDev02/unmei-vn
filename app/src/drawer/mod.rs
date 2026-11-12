//! The live Monitor drawer — the first usable, safe F3 view.
//!
//! This module owns everything a developer sees and touches after pressing
//! **F3**, on top of the hidden capture [`crate::monitor`] already retains:
//!
//! - [`MonitorPanel`] — the launch-local visibility/filter/focus state. It is
//!   deliberately independent of a loaded [`crate::Runtime`], the title/menu,
//!   a defensive hold and the startup error screen, so F3 works everywhere.
//! - [`crate::input::InputQuarantine`] is resolved here: the drawer claims app
//!   input on its opening frame, keeps the claim for the whole closing frame,
//!   and records every key/button held across those frames so the ordinary app
//!   paths resume only after each is released and freshly pressed.
//! - [`MonitorProjection`] — the bounded, progressive display projection over
//!   the one shared retained history: exact case-sensitive `unmei::` targets by
//!   default, everything on demand, never a second buffer.
//! - the bottom drawer itself: the approved dialogue-reserved geometry, the
//!   Fira Mono row grid, the severity tones and the newest-retained marker.
//!
//! The **older-line reader** sits on top of that following view: the
//! body wheel-scrolls only when hit-tested to the log body, a real scrollbar
//! drags, PgUp/PgDn/Home/End page anywhere in the drawer, and a non-following
//! reader is anchored to the **top visible retained event plus a within-row
//! offset measured against that row's actual laid-out extent** — never a
//! display index or a pixel count. Retention eviction clamps that anchor to the
//! oldest surviving match
//! and shows the panel-local `older lines expired` acknowledgement; a filter
//! change falls back to the matching anchor, the first newer match, or the
//! nearest preceding one. Only a deliberate bottom arrival, End or **Follow
//! latest** resumes following.

mod components;
mod geometry;
mod interaction;
mod projection;
mod rows;
mod state;
mod status;
mod tree;

pub use crate::monitor_reader::MonitorAnchor;
pub use components::MonitorRow;
pub use state::MonitorPanel;

pub(crate) use interaction::{
    cancel_on_window_focus_loss, hold_monitor_claim, panel_input, reader_input,
};
pub(crate) use projection::{project_monitor, resolve_reader, MonitorProjection};
pub(crate) use rows::{sync_panel_row_markers, sync_panel_rows};
pub(crate) use state::{MonitorInputSet, MonitorPageRepeat, MonitorScrollbarDrag, MonitorWork};
pub(crate) use status::sync_panel_status;
pub(crate) use tree::{sync_panel_controls, sync_panel_ui};

// The crate's own test module reads these panel-local markers, budgets and
// vocabulary through the module root (`crate::drawer::…`), so they are
// re-exported here instead of importing each defining submodule. Production
// builds reach them inside this module tree only.
#[cfg(test)]
pub(crate) use components::{
    MonitorControlTag, MonitorEmptyState, MonitorLocatingState, MonitorLogBody, MonitorLogViewport,
    MonitorPanelNode, MonitorReaderNode, MonitorRoot, MonitorRowPart, MonitorRowsContent,
    MonitorScrollbarThumb, MonitorScrollbarTrack, MonitorStatusText,
};
#[cfg(test)]
pub(crate) use geometry::{
    DISPLAY_ROWS, NEWEST_INSET, NEWEST_WASH, PANEL_BOTTOM_RESERVATION, PANEL_HEIGHT,
    PANEL_SIDE_INSET, PROJECTION_SCAN_BUDGET, SCROLLBAR_WIDTH, TEXT_WORK_BUDGET, TONE_FAINT,
    TONE_INFO, TONE_TEXT, TONE_WARN,
};
#[cfg(test)]
pub(crate) use state::{MonitorControl, MonitorView, MONITOR_CLAIM};
