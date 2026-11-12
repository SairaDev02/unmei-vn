//! The drawer's launch-local state and its settled bounds.

use bevy::input_focus::InputFocus;
use bevy::prelude::*;

use crate::input::InputClaim;
use crate::monitor::MonitorLine;
use crate::monitor_reader::Reader;

use super::MonitorAnchor;

/// The claim name the drawer owns app input under.
pub(crate) const MONITOR_CLAIM: InputClaim = InputClaim("monitor");

/// The `PreUpdate` resolution point for the drawer's ownership claim and its
/// held-input quarantine. Ordered after the frame's physical input and UI
/// focus settle and before focused input dispatch and every `Update` action, so
/// a claim resolved here covers the whole frame it was resolved on.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct MonitorInputSet;

/// The display predicate: the exact case-sensitive `unmei::` prefix, or every
/// surviving admitted event. It changes only the projection, never capture,
/// retention or the shared history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum MonitorView {
    /// `target.starts_with("unmei::")`, case-sensitive.
    #[default]
    UnmeiOnly,
    Everything,
}

impl MonitorView {
    /// The one display predicate: exact, case-sensitive `unmei::` prefix for the
    /// default view, every surviving admitted event for Show everything.
    pub(super) fn matches(self, line: &MonitorLine) -> bool {
        match self {
            MonitorView::UnmeiOnly => line.target.starts_with("unmei::"),
            MonitorView::Everything => true,
        }
    }

    /// The status footer's view wording.
    pub(super) fn label(self) -> &'static str {
        match self {
            MonitorView::UnmeiOnly => "showing unmei:: only",
            MonitorView::Everything => "showing everything",
        }
    }

    /// The header control's label, with its live checkbox mark.
    pub(super) fn checkbox_label(self) -> &'static str {
        match self {
            MonitorView::UnmeiOnly => "[ ] Show everything",
            MonitorView::Everything => "[x] Show everything",
        }
    }

    /// The empty projection's local state line.
    pub(super) fn empty_label(self) -> &'static str {
        match self {
            MonitorView::UnmeiOnly => "no unmei:: diagnostics match this view",
            MonitorView::Everything => "no diagnostics captured",
        }
    }
}

/// The three bounded focus stops, in the settled Tab order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MonitorControl {
    ShowEverything,
    FollowLatest,
    Close,
}

impl MonitorControl {
    pub(super) fn next(self) -> Self {
        match self {
            MonitorControl::ShowEverything => MonitorControl::FollowLatest,
            MonitorControl::FollowLatest => MonitorControl::Close,
            MonitorControl::Close => MonitorControl::ShowEverything,
        }
    }

    pub(super) fn previous(self) -> Self {
        match self {
            MonitorControl::ShowEverything => MonitorControl::Close,
            MonitorControl::FollowLatest => MonitorControl::ShowEverything,
            MonitorControl::Close => MonitorControl::FollowLatest,
        }
    }
}

/// The drawer's launch-local state: visibility, display predicate, bounded
/// focus, the suspended underlying focus entity and the older-line reader's
/// position.
///
/// It lives for the whole app lifetime and is untouched by loads, New Game,
/// title/menu/Timeline transitions or hiding: this launch's reader choices
/// survive them all. A relaunch starts closed on the default view, following
/// latest, with no anchor and no expiry.
#[derive(Resource, Debug)]
pub struct MonitorPanel {
    pub(super) open: bool,
    pub(super) view: MonitorView,
    pub(super) focus: MonitorControl,
    /// The last control a previous open ended on. `None` until the first open,
    /// which is what makes the first open focus **Show everything**.
    last_control: Option<MonitorControl>,
    /// The entity that held [`InputFocus`] when the drawer opened. Restored on
    /// close when it is still a live editable key field.
    pub(super) suspended_focus: Option<Entity>,
    /// The one reader: logical position, follow choice, expiry acknowledgement
    /// and locating state. The drawer calls its operations and reads its view;
    /// it never manipulates reader fields.
    pub(super) reader: Reader,
}

impl Default for MonitorPanel {
    fn default() -> Self {
        Self {
            open: false,
            view: MonitorView::default(),
            focus: MonitorControl::ShowEverything,
            last_control: None,
            suspended_focus: None,
            reader: Reader::default(),
        }
    }
}

impl MonitorPanel {
    // The accessors below are the drawer's test-observable state and the seams
    // the older-line reader consumes.
    /// Whether the drawer currently owns the window.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// The active display predicate.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn view(&self) -> MonitorView {
        self.view
    }

    /// The focused control.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn focus(&self) -> MonitorControl {
        self.focus
    }

    /// Whether the reader is following the latest event.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn is_following(&self) -> bool {
        self.reader.is_following()
    }

    /// The stable reading anchor, if any. Public so a rendered acceptance
    /// harness can compare the drawn top-visible event to it.
    pub fn anchor(&self) -> Option<MonitorAnchor> {
        self.reader.anchor()
    }

    /// Whether retention evicted the anchor and the reader has not
    /// acknowledged it.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn is_expired(&self) -> bool {
        self.reader.is_expired()
    }

    /// Whether the projection is still locating the reader's anchor.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn is_pending(&self) -> bool {
        self.reader.is_pending()
    }

    pub(super) fn open(&mut self, input_focus: Option<&InputFocus>) {
        self.open = true;
        self.focus = self.last_control.unwrap_or(MonitorControl::ShowEverything);
        self.suspended_focus = input_focus.and_then(InputFocus::get);
    }

    pub(super) fn close(&mut self) {
        self.open = false;
        self.last_control = Some(self.focus);
    }

    pub(super) fn toggle(&mut self) {
        self.view = match self.view {
            MonitorView::UnmeiOnly => MonitorView::Everything,
            MonitorView::Everything => MonitorView::UnmeiOnly,
        };
        // A filter change is one of the defined expiry acknowledgements; the
        // anchor itself is preserved and re-resolved against the new
        // predicate.
        self.reader.on_view_change();
    }
}

/// The scrollbar drag's transient state. Cancelled on close and on window
/// focus loss; a held pointer can never become an underlying drag because a
/// fresh press is required to start one.
#[derive(Resource, Debug, Default)]
pub(crate) struct MonitorScrollbarDrag {
    pub(crate) active: bool,
}

/// The PgUp/PgDn held-repeat countdowns, in seconds. A fresh press acts
/// immediately and arms its countdown; holding the key repeats when it expires.
#[derive(Resource, Debug, Default)]
pub(crate) struct MonitorPageRepeat {
    pub(super) page_up: f32,
    pub(super) page_down: f32,
}

impl MonitorPageRepeat {
    pub(super) fn reset(&mut self) {
        self.page_up = 0.0;
        self.page_down = 0.0;
    }
}

/// Per-update work accounting, so the settled budgets are observable and a
/// regression that scans or materializes more is testable.
#[derive(Resource, Debug, Default)]
pub(crate) struct MonitorWork {
    /// Retained entries examined by the projection this update.
    pub scanned: usize,
    /// Line-sized text materializations/replacements this update.
    pub text_ops: usize,
}
