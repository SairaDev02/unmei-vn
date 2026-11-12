//! The contextual card model: what `list_slots` implies for one slot, in the
//! player's words, and the legacy picker's target rows.

use unmei_engine::save::{Slot, SlotInfo, SlotStatus};

use super::state::{MenuMode, MenuState};

/// What activating a card does. `Write` is the save-like action whose label
/// varies (`Save here` / `Overwrite`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotAction {
    Write,
    Load,
}

/// A card's contextual action, rendered verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardAction {
    pub label: &'static str,
    pub enabled: bool,
    /// `Overwrite` is the one destructive action; the card styles it as such.
    pub danger: bool,
    pub action: Option<SlotAction>,
}

impl CardAction {
    fn new(label: &'static str, enabled: bool, danger: bool, action: Option<SlotAction>) -> Self {
        CardAction {
            label,
            enabled,
            danger,
            action,
        }
    }
}

/// Exactly what `list_slots` yields for one slot, in the player's words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardModel {
    pub slot: Slot,
    pub label: &'static str,
    pub kind: &'static str,
    pub status: &'static str,
    pub detail: String,
    pub saved_at: String,
    pub action: CardAction,
}

/// The canonical label for a slot.
pub fn slot_label(slot: Slot) -> &'static str {
    match slot {
        Slot::Manual(1) => "Slot 1",
        Slot::Manual(2) => "Slot 2",
        Slot::Manual(3) => "Slot 3",
        Slot::Autosave => "Autosave",
        Slot::Manual(_) => "Slot",
    }
}

fn slot_kind(slot: Slot) -> &'static str {
    match slot {
        Slot::Manual(_) => "manual",
        Slot::Autosave => "game-written",
    }
}

fn status_label(status: &SlotStatus) -> &'static str {
    match status {
        SlotStatus::Empty => "Empty",
        SlotStatus::Ok { .. } => "Ok",
        SlotStatus::Unreadable { .. } => "Unreadable",
    }
}

/// The scene id, the refusal reason, or `—` for an empty slot: the one detail
/// line a card and a legacy target both render.
fn status_detail(status: &SlotStatus) -> String {
    match status {
        SlotStatus::Empty => "—".to_string(),
        SlotStatus::Ok { scene_id, .. } => scene_id.clone(),
        SlotStatus::Unreadable { reason } => reason.clone(),
    }
}

/// Index of a slot inside `Slot::ALL` (which is also the card model order).
pub fn slot_index(slot: Slot) -> usize {
    match slot {
        Slot::Manual(1) => 0,
        Slot::Manual(2) => 1,
        Slot::Manual(3) => 2,
        Slot::Autosave => 3,
        Slot::Manual(_) => 0,
    }
}

/// Format an app-stamped epoch as local time. `None` renders as `—`; a value the
/// clock cannot represent falls back to the raw epoch rather than a fabricated
/// date.
pub fn format_saved_at(saved_at: Option<u64>) -> String {
    let Some(seconds) = saved_at else {
        return "—".to_string();
    };
    let formatted = jiff::Timestamp::from_second(seconds as i64)
        .ok()
        .map(|timestamp| timestamp.to_zoned(jiff::tz::TimeZone::system()))
        .map(|zoned| zoned.strftime("%d %b %Y %H:%M").to_string());
    formatted.unwrap_or_else(|| seconds.to_string())
}

/// The contextual action for one slot, from §3 "Selectable when":
///
/// - **Empty** (manual) → `Save here`, immediate (in either mode) — the issue's
///   table qualifies only the occupied and autosave rows by mode.
/// - **Ok** (manual) → `Overwrite` in save mode, `Load` in load mode.
/// - **Unreadable** (manual) → `Overwrite` only; load disabled.
/// - **Autosave** → `Load` in load mode; a disabled `Load-only` in save mode.
///
/// The autosave is never a write target, so it offers no save action at all.
pub fn card_action(info: &SlotInfo, mode: MenuMode) -> CardAction {
    match (info.slot, &info.status, mode) {
        (Slot::Manual(_), SlotStatus::Empty, _) => {
            CardAction::new("Save here", true, false, Some(SlotAction::Write))
        }
        (Slot::Manual(_), SlotStatus::Ok { .. }, MenuMode::Save) => {
            CardAction::new("Overwrite", true, true, Some(SlotAction::Write))
        }
        (Slot::Manual(_), SlotStatus::Ok { .. }, MenuMode::Load) => {
            CardAction::new("Load", true, false, Some(SlotAction::Load))
        }
        (Slot::Manual(_), SlotStatus::Unreadable { .. }, _) => {
            CardAction::new("Overwrite", true, true, Some(SlotAction::Write))
        }
        (Slot::Autosave, SlotStatus::Ok { .. }, MenuMode::Load) => {
            CardAction::new("Load", true, false, Some(SlotAction::Load))
        }
        (Slot::Autosave, _, _) => {
            let label = if mode == MenuMode::Save {
                "Load-only"
            } else {
                "Load"
            };
            CardAction::new(label, false, false, None)
        }
    }
}

/// Build the card model `list_slots` implies, in the current mode.
pub fn card_model(info: &SlotInfo, mode: MenuMode) -> CardModel {
    let saved_at = match &info.status {
        SlotStatus::Empty => "no save yet".to_string(),
        SlotStatus::Ok { saved_at, .. } => format_saved_at(*saved_at),
        SlotStatus::Unreadable { .. } => "—".to_string(),
    };
    CardModel {
        slot: info.slot,
        label: slot_label(info.slot),
        kind: slot_kind(info.slot),
        status: status_label(&info.status),
        detail: status_detail(&info.status),
        saved_at,
        action: card_action(info, mode),
    }
}

/// Every card, in `Slot::ALL` order — the exact four rows the grid draws.
pub fn card_models(menu: &MenuState) -> Vec<CardModel> {
    menu.slots
        .iter()
        .map(|info| card_model(info, menu.mode))
        .collect()
}

/// One row of the legacy slot picker: a manual target, with `(overwrite)` when
/// it already holds something.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyTargetModel {
    pub slot: Slot,
    pub label: &'static str,
    pub status: &'static str,
    pub detail: String,
    /// An occupied target is imported over — the picker labels it `(overwrite)`.
    pub overwrite: bool,
}

impl LegacyTargetModel {
    /// The picker row's rendered label, e.g. `Slot 2  (overwrite)`.
    pub fn display(&self) -> String {
        if self.overwrite {
            format!("{}  (overwrite)", self.label)
        } else {
            self.label.to_string()
        }
    }
}

/// The three manual targets the legacy picker offers, in slot order.
pub fn legacy_targets(menu: &MenuState) -> Vec<LegacyTargetModel> {
    menu.slots
        .iter()
        .filter(|info| matches!(info.slot, Slot::Manual(_)))
        .map(|info| LegacyTargetModel {
            slot: info.slot,
            label: slot_label(info.slot),
            status: status_label(&info.status),
            detail: status_detail(&info.status),
            overwrite: !matches!(info.status, SlotStatus::Empty),
        })
        .collect()
}
