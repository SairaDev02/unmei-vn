//! The one-time legacy import picker, drawn from the plugin's `legacy-picker`
//! surface (spec §11).
//!
//! Choose a manual target. Occupied targets are labelled `(overwrite)`; there is
//! deliberately no second prompt beyond this pick. `Esc` or Cancel backs out to
//! the Legacy card.
//!
//! The tree places the dimmer, the 460px panel, the heading slot, the prompt text
//! and Cancel, and its `legacy.targets` list declares the three control rows.
//! This module fills the heading and Cancel's words, and spawns each row with the
//! label, the status line and the action its own control carries: the item
//! template places all three target controls in every row and the renderer shows
//! only the row's own ([`Surface::spawn_item`]). Focus, the arrow walk and `Esc`
//! stay the menu's, keyed by the same Keiko ids.

use bevy::prelude::*;
use unmei_engine::save::Slot;

use super::super::render::{Bind, Surface, UiLook, Words};
use super::super::style::TextFaces;
use crate::menu::{self, MenuAction, MenuState};

/// The surface's id, as the plugin folder names it.
const SURFACE: &str = "legacy-picker";

/// The picker outside its list: the heading's words and the action Cancel
/// carries. Each row brings its own binding, because the item template is spawned
/// once per target.
struct Picker;

impl Bind for Picker {
    fn words(&self, name: &str) -> Option<Words> {
        match name {
            "legacy.heading" => Some(Words::new("legacy-picker.heading")),
            "legacy.cancel" => Some(Words::new("legacy-picker.cancel")),
            _ => None,
        }
    }

    fn action(&self, name: &str) -> Option<MenuAction> {
        (name == "legacy.cancel").then_some(MenuAction::LegacyCancel)
    }
}

/// One target row: the label the picker writes — `(overwrite)` when the target
/// already holds anything — the status line under it, and the action the row's
/// own control carries. Nothing on a row is ever disabled: an occupied target is
/// imported over, which the label states.
struct Row {
    target: menu::LegacyTargetModel,
    control: &'static str,
}

impl Row {
    fn new(target: menu::LegacyTargetModel) -> Self {
        Self {
            control: row_control(target.slot),
            target,
        }
    }
}

impl Bind for Row {
    fn words(&self, name: &str) -> Option<Words> {
        match name {
            "legacy.target.label" => Some(Words::text(self.target.display())),
            "legacy.target.detail" => Some(Words::text(format!(
                "{} - {}",
                self.target.status, self.target.detail
            ))),
            _ if name == self.control => Some(Words::text(self.target.display())),
            _ => None,
        }
    }

    fn action(&self, name: &str) -> Option<MenuAction> {
        target_slot(name).map(MenuAction::LegacyTarget)
    }
}

/// The manual target a row control's Keiko id names, so every control the item
/// template places knows its action and the engine shows only the row's own.
fn target_slot(name: &str) -> Option<Slot> {
    match name {
        "legacy.target.1" => Some(Slot::Manual(1)),
        "legacy.target.2" => Some(Slot::Manual(2)),
        "legacy.target.3" => Some(Slot::Manual(3)),
        _ => None,
    }
}

/// The Keiko control id a manual target's row is: the row of the item template
/// the engine shows for it. Only the three manual targets reach here.
fn row_control(slot: Slot) -> &'static str {
    match slot {
        Slot::Manual(2) => "legacy.target.2",
        Slot::Manual(3) => "legacy.target.3",
        Slot::Manual(_) | Slot::Autosave => "legacy.target.1",
    }
}

/// Draw the open picker as the plugin in force declares it, under `parent`.
pub(super) fn legacy_picker(
    parent: &mut ChildSpawnerCommands,
    look: Option<&UiLook>,
    faces: &TextFaces,
    menu: &MenuState,
) {
    let Some(look) = look else {
        return;
    };
    let bind = Picker;
    let surface = Surface {
        look,
        faces,
        bind: &bind,
    };
    let Some(rendered) = surface.spawn(SURFACE, parent) else {
        return;
    };
    let (Some(list), Some(item)) = (
        rendered.lists.get("legacy.targets").copied(),
        rendered.items.get("legacy.targets").cloned(),
    ) else {
        return;
    };
    let rows = menu::legacy_targets(menu);
    parent.commands().entity(list).with_children(|list| {
        for target in rows {
            let row = Row::new(target);
            surface.spawn_item(&item, list, &row, Some(row.control));
        }
    });
}
