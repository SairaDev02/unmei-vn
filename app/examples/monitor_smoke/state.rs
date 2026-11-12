//! The walk's constants, captured rows and probe state.

use crate::prelude::*;

/// The arrival the walk admits under the held anchor, so the row-set change can
/// be checked in both the retained history and the drawn display.
pub(crate) const ARRIVAL_MESSAGE: &str = "arrival under the held anchor";

/// The walk's emulated per-frame delay from `MONITOR_SMOKE_FRAME_MS`, zero by
/// default. CI's software-rendered frames ran about 130 ms, past the drawer's
/// held-key repeat cadence; a slow value reproduces that pacing so the walk's
/// cadence independence is checkable anywhere.
pub(crate) fn frame_delay() -> Duration {
    static DELAY: std::sync::OnceLock<Duration> = std::sync::OnceLock::new();
    *DELAY.get_or_init(|| {
        std::env::var("MONITOR_SMOKE_FRAME_MS")
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .filter(|millis| *millis > 0)
            .map_or(Duration::ZERO, Duration::from_millis)
    })
}

/// Tolerances for real font metrics and border rounding.
pub(crate) const POSITION_TOLERANCE: f32 = 1.5;
pub(crate) const SIZE_TOLERANCE: f32 = 2.5;

/// The settled single-line row height at the 12px/1.45 Fira Mono type.
pub(crate) const SINGLE_LINE_ROW: f32 = 19.4;

/// One measured HUD element: a node signature plus its laid-out place, so an
/// open/close that moved or resized it is caught.
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct HudNode {
    pub(crate) signature: String,
    pub(crate) position: Vec2,
    pub(crate) size: Vec2,
}

#[derive(Default)]
pub(crate) struct Probe {
    pub(crate) frame: usize,
    pub(crate) hud_before: Option<Vec<HudNode>>,
    pub(crate) clicked_control: Option<Entity>,
    /// The reader's own anchored admission identity: the deliberate reading
    /// position recorded before the rewrap and required to survive it. Only the
    /// identity is durable — the within-row offset is re-clamped against the
    /// rewrapped row, so a replacement arrival or a rewrap is allowed to change
    /// it; the drawn row's position is checked instead.
    pub(crate) anchor: Option<u64>,
}

/// One laid-out row: its top edge, height, entity and rendered message/target
/// columns.
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct RenderedRow {
    pub(crate) entity: Entity,
    pub(crate) top: f32,
    pub(crate) height: f32,
    pub(crate) message: String,
    pub(crate) target: String,
}

impl RenderedRow {
    /// The row's one-line name for the walk output: the rendered target and the
    /// trimmed message, with a long wrapped body shortened so the walk's own
    /// report stays readable. The admission identity the walk compares is the
    /// [`MonitorRow`] component on the same row, never this text.
    pub(crate) fn label(&self) -> String {
        let message = self.message.trim();
        let body: String = message.chars().take(48).collect();
        match body.is_empty() {
            true => self.target.clone(),
            false if message.chars().count() > 48 => format!("{} {body}…", self.target),
            false => format!("{} {body}", self.target),
        }
    }
}
