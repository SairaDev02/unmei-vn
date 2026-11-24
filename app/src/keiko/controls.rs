//! The control-id table's app half: the one-to-one map between the engine's
//! stable role ids (spec §3.2) and the player's [`MenuFocus`], plus the
//! observation of the controls currently on screen.

use bevy::prelude::*;
use unmei_engine::keiko::{CONTROL_IDS, CONTROL_IDS_WITHOUT_FOCUS};
use unmei_engine::save::Slot;

use super::observe::Control;
use crate::menu::{MenuAction, MenuFocus, MenuMode, MenuState, MenuView};
use crate::settings::AudioControl;

/// Every `MenuFocus` the §3.2 table names, with its id. The one map: a control
/// added later adds a row here and an id in the engine's table.
pub fn focus_ids() -> Vec<(MenuFocus, &'static str)> {
    vec![
        (MenuFocus::TitleContinue, "title.continue"),
        (MenuFocus::TitleNewGame, "title.new-game"),
        (MenuFocus::TitleSettings, "title.settings"),
        (MenuFocus::TitleGallery, "title.gallery"),
        (MenuFocus::TitleQuit, "title.quit"),
        (MenuFocus::SaveTab, "overlay.save-tab"),
        (MenuFocus::LoadTab, "overlay.load-tab"),
        (MenuFocus::Card(0), "overlay.slot.1"),
        (MenuFocus::Card(1), "overlay.slot.2"),
        (MenuFocus::Card(2), "overlay.slot.3"),
        (MenuFocus::Card(3), "overlay.slot.autosave"),
        (MenuFocus::Resume, "overlay.resume"),
        (MenuFocus::ExitToTitle, "overlay.exit-to-title"),
        (MenuFocus::SettingsButton, "overlay.settings"),
        (MenuFocus::GalleryButton, "overlay.gallery"),
        (MenuFocus::TimelineButton, "overlay.timeline"),
        (MenuFocus::LegacyCard, "overlay.legacy"),
        (MenuFocus::LegacyTarget(0), "legacy.target.1"),
        (MenuFocus::LegacyTarget(1), "legacy.target.2"),
        (MenuFocus::LegacyTarget(2), "legacy.target.3"),
        (MenuFocus::LegacyCancel, "legacy.cancel"),
        (MenuFocus::SettingsBack, "settings.back"),
        (MenuFocus::AudioMaster, "settings.master"),
        (MenuFocus::AudioMusic, "settings.music"),
        (MenuFocus::AudioSfx, "settings.sfx"),
        (MenuFocus::AudioMute, "settings.mute"),
        (MenuFocus::AudioReset, "settings.reset-audio"),
        (MenuFocus::TextSpeed, "settings.text-speed"),
        (MenuFocus::PlayAuto, "settings.auto"),
        (MenuFocus::PlayFullscreen, "settings.fullscreen"),
        (MenuFocus::PlayReset, "settings.reset-play"),
        (MenuFocus::KeyRow, "settings.key"),
        (MenuFocus::CopyKeyPath, "settings.copy-path"),
        (MenuFocus::ClearKey, "settings.clear-key"),
        (MenuFocus::ConfirmYes, "confirm.yes"),
        (MenuFocus::ConfirmNo, "confirm.no"),
    ]
}

/// The id a focus target carries, when the table names it.
pub fn id_for_focus(focus: MenuFocus) -> Option<String> {
    match focus {
        MenuFocus::GalleryCard(index) => return Some(gallery_card_id(index)),
        MenuFocus::TimelineCard(index) => return Some(timeline_card_id(index)),
        _ => {}
    }
    focus_ids()
        .into_iter()
        .find(|(candidate, _)| *candidate == focus)
        .map(|(_, id)| id.to_string())
}

/// A gallery card's id, 1-based in order.
pub fn gallery_card_id(index: usize) -> String {
    format!("gallery.card.{}", index + 1)
}

/// A timeline card's id, 1-based in order.
pub fn timeline_card_id(index: usize) -> String {
    format!("timeline.card.{}", index + 1)
}

/// The focus target an id names: the reverse map, and the indexed families.
pub fn focus_for_id(id: &str) -> Option<MenuFocus> {
    if let Some(index) = indexed(id, "gallery.card.") {
        return Some(MenuFocus::GalleryCard(index));
    }
    if let Some(index) = indexed(id, "timeline.card.") {
        return Some(MenuFocus::TimelineCard(index));
    }
    focus_ids()
        .into_iter()
        .find(|(_, candidate)| *candidate == id)
        .map(|(focus, _)| focus)
}

fn indexed(id: &str, family: &str) -> Option<usize> {
    let rest = id.strip_prefix(family)?;
    let number: usize = rest.parse().ok()?;
    number.checked_sub(1)
}

/// The ids on the current surface, in walk order.
pub fn surface_ids(
    menu: &MenuState,
    gallery_entries: usize,
    timeline_entries: usize,
) -> Vec<String> {
    if menu.confirm.is_some() {
        return vec!["confirm.yes".to_string(), "confirm.no".to_string()];
    }
    if !menu.open {
        return Vec::new();
    }
    match menu.view {
        MenuView::Title => [
            "title.continue",
            "title.new-game",
            "title.settings",
            "title.gallery",
            "title.quit",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        MenuView::Slots => {
            let mut ids: Vec<String> = [
                "overlay.save-tab",
                "overlay.load-tab",
                "overlay.slot.1",
                "overlay.slot.2",
                "overlay.slot.3",
                "overlay.slot.autosave",
                "overlay.resume",
                "overlay.settings",
                "overlay.gallery",
                "overlay.timeline",
            ]
            .into_iter()
            .map(str::to_string)
            .collect();
            if menu.legacy_importable {
                ids.push("overlay.legacy".to_string());
            }
            if let Some(picker) = &menu.picker {
                let _ = picker;
                ids.extend(
                    [
                        "legacy.target.1",
                        "legacy.target.2",
                        "legacy.target.3",
                        "legacy.cancel",
                    ]
                    .into_iter()
                    .map(str::to_string),
                );
            }
            ids
        }
        MenuView::Settings => {
            let mut ids: Vec<String> = [
                "settings.back",
                "settings.master",
                "settings.music",
                "settings.sfx",
                "settings.mute",
                "settings.reset-audio",
                "settings.text-speed",
                "settings.auto",
                "settings.fullscreen",
                "settings.reset-play",
                "settings.key",
                "settings.copy-path",
                "settings.clear-key",
            ]
            .into_iter()
            .map(str::to_string)
            .collect();
            // The rail's session-scoped stop, past the Decisions group
            // (`docs/title-and-menu.md` §4).
            if crate::menu::exit_to_title_shown(menu) {
                ids.push("overlay.exit-to-title".to_string());
            }
            ids
        }
        MenuView::Gallery => {
            if menu.gallery_still {
                Vec::new()
            } else {
                (0..gallery_entries).map(gallery_card_id).collect()
            }
        }
        MenuView::Timeline => (0..timeline_entries).map(timeline_card_id).collect(),
    }
}

/// Whether a control can be activated right now.
pub fn is_enabled(menu: &MenuState, id: &str, world: &World) -> bool {
    match id {
        "title.continue" => menu.continue_enabled,
        "overlay.legacy" => menu.legacy_importable,
        "settings.clear-key" => world
            .get_resource::<crate::credentials::CredentialsState>()
            .is_some_and(|state| !state.row.is_from_environment()),
        id if id.starts_with("timeline.card.") => {
            let Some(index) = indexed(id, "timeline.card.") else {
                return false;
            };
            let Some(runtime) = world.get_resource::<crate::Runtime>() else {
                return false;
            };
            let entries = runtime.runner.pack().timeline_entries();
            let Some(scene) = entries.get(index) else {
                return false;
            };
            world
                .get_resource::<crate::read_log::ReadLogState>()
                .is_some_and(|read_log| read_log.is_visited(scene))
        }
        _ => true,
    }
}

/// The action a control id carries; the one map a click or an observation
/// resolves an id through.
pub(super) fn action_for_id(id: &str) -> Option<MenuAction> {
    let action = match id {
        "title.continue" => MenuAction::Continue,
        "title.new-game" => MenuAction::NewGame,
        "title.settings" => MenuAction::OpenSettings,
        "title.gallery" => MenuAction::Gallery,
        "title.quit" => MenuAction::Quit,
        "overlay.save-tab" => MenuAction::SetMode(MenuMode::Save),
        "overlay.load-tab" => MenuAction::SetMode(MenuMode::Load),
        "overlay.slot.1" => MenuAction::Card(0),
        "overlay.slot.2" => MenuAction::Card(1),
        "overlay.slot.3" => MenuAction::Card(2),
        "overlay.slot.autosave" => MenuAction::Card(3),
        "overlay.resume" => MenuAction::Resume,
        "overlay.exit-to-title" => MenuAction::ExitToTitle,
        "overlay.settings" => MenuAction::OpenSettings,
        "overlay.gallery" => MenuAction::OpenGallery,
        "overlay.timeline" => MenuAction::OpenTimeline,
        "overlay.legacy" => MenuAction::LegacyImport,
        "legacy.target.1" => MenuAction::LegacyTarget(Slot::Manual(1)),
        "legacy.target.2" => MenuAction::LegacyTarget(Slot::Manual(2)),
        "legacy.target.3" => MenuAction::LegacyTarget(Slot::Manual(3)),
        "legacy.cancel" => MenuAction::LegacyCancel,
        "settings.back" => MenuAction::BackToSlots,
        "settings.master" => MenuAction::AudioSlider(AudioControl::Master),
        "settings.music" => MenuAction::AudioSlider(AudioControl::Music),
        "settings.sfx" => MenuAction::AudioSlider(AudioControl::Sfx),
        "settings.mute" => MenuAction::AudioMute,
        "settings.reset-audio" => MenuAction::AudioReset,
        "settings.text-speed" => MenuAction::TextSpeed,
        "settings.auto" => MenuAction::PlayAuto,
        "settings.fullscreen" => MenuAction::PlayFullscreen,
        "settings.reset-play" => MenuAction::PlayReset,
        "settings.key" => MenuAction::KeyRow,
        "settings.copy-path" => MenuAction::CopyKeyPath,
        "settings.clear-key" => MenuAction::ClearKey,
        "confirm.yes" => MenuAction::ConfirmAccept,
        "confirm.no" => MenuAction::ConfirmCancel,
        id if id.starts_with("gallery.card.") => {
            MenuAction::GalleryCard(indexed(id, "gallery.card.")?)
        }
        id if id.starts_with("timeline.card.") => {
            MenuAction::TimelineCard(indexed(id, "timeline.card.")?)
        }
        _ => return None,
    };
    Some(action)
}

/// Observe every control on the current surface, in walk order.
pub fn observe_controls(world: &mut World) -> Vec<Control> {
    let Some(menu) = world.get_resource::<MenuState>().cloned() else {
        return Vec::new();
    };
    let (gallery_entries, timeline_entries) = world
        .get_resource::<crate::Runtime>()
        .map(|runtime| {
            (
                runtime.runner.pack().gallery_entries().len(),
                runtime.runner.pack().timeline_entries().len(),
            )
        })
        .unwrap_or((0, 0));

    let ids = surface_ids(&menu, gallery_entries, timeline_entries);
    let labels = labels_by_action(world);
    let focused = id_for_focus(menu.focus);
    ids.iter()
        .filter_map(|id| {
            let action = action_for_id(id)?;
            let label = labels
                .iter()
                .find(|(candidate, _)| *candidate == action)
                .map(|(_, label)| label.clone())
                .unwrap_or_default();
            Some(Control {
                id: id.clone(),
                label,
                enabled: is_enabled(&menu, id, world),
                focused: focused.as_deref() == Some(id.as_str()),
            })
        })
        .collect()
}

/// Every `MenuAction` node's rendered label, collected from its text children.
fn labels_by_action(world: &mut World) -> Vec<(MenuAction, String)> {
    let mut query = world.query::<(&MenuAction, Entity)>();
    let nodes: Vec<(MenuAction, Entity)> = query
        .iter(world)
        .map(|(action, entity)| (*action, entity))
        .collect();
    nodes
        .into_iter()
        .map(|(action, entity)| (action, node_label(world, entity)))
        .filter(|(_, label)| !label.is_empty())
        .collect()
}

/// The label a control shows. A rendered control's words may be drawn by a
/// sibling slot — the confirm's accept label is — so the renderer records them on
/// the control itself and that is the label the gate reads.
fn node_label(world: &World, entity: Entity) -> String {
    if let Some(words) = world.get::<crate::presentation::render::ControlLabel>(entity) {
        return words.0.clone();
    }
    let mut label = String::new();
    collect_label(world, entity, &mut label, 0);
    label.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn collect_label(world: &World, entity: Entity, out: &mut String, depth: usize) {
    if depth > 6 {
        return;
    }
    // A hidden node's text is not on screen, so it is not part of the
    // control's label. The key row's placeholder is the one such child: it is
    // drawn only while the row is `Not set` and the field empty.
    if world.get::<Visibility>(entity) == Some(&Visibility::Hidden) {
        return;
    }
    // Separate fragments with one space: a control's label is the text its
    // parts show, in reading order, not those parts run together. `node_label`
    // then collapses the result to single spaces.
    if out.chars().last().is_some_and(|last| last != ' ') {
        out.push(' ');
    }
    if let Some(text) = world.get::<Text>(entity) {
        out.push_str(text.as_str());
    }
    if let Some(span) = world.get::<bevy::text::TextSpan>(entity) {
        out.push_str(&span.0);
    }
    if let Some(children) = world.get::<Children>(entity) {
        for child in children.iter() {
            collect_label(world, child, out, depth + 1);
        }
    }
}

/// Every id in the engine's table that the player has a control for maps to a
/// focus target and back. The play frame's quick row has ids and no control yet,
/// so it is the one group the map skips (ticket #107; the row arrives with
/// variant E).
pub fn the_table_is_one_to_one() -> Result<(), String> {
    let mut fixed: Vec<&str> = Vec::new();
    for (focus, id) in focus_ids() {
        if focus_for_id(id) != Some(focus) {
            return Err(format!("`{id}` does not map back to its focus target"));
        }
        fixed.push(id);
    }
    for id in CONTROL_IDS {
        if id.starts_with("gallery.card.") || id.starts_with("timeline.card.") {
            continue;
        }
        if CONTROL_IDS_WITHOUT_FOCUS.contains(id) {
            continue;
        }
        if !fixed.contains(id) {
            return Err(format!("control id `{id}` has no focus target"));
        }
    }
    for (focus, id) in focus_ids() {
        if matches!(
            focus,
            MenuFocus::GalleryCard(_) | MenuFocus::TimelineCard(_)
        ) {
            continue;
        }
        if !CONTROL_IDS.contains(&id) {
            return Err(format!("`{id}` is not in the engine's control table"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::MenuFocus;

    /// Rule 10: the id table maps every fixed control to one focus and back,
    /// and every id is in the engine's table.
    #[test]
    fn the_control_id_table_is_one_to_one() {
        super::the_table_is_one_to_one().unwrap();
    }

    /// The indexed families are bounded only by what a pack declares, so their
    /// ids must stay distinct and round-trip past any fixed pool.
    #[test]
    fn indexed_control_ids_stay_distinct_and_round_trip() {
        assert_ids_unique(40, super::gallery_card_id, MenuFocus::GalleryCard);
        assert_ids_unique(40, super::timeline_card_id, MenuFocus::TimelineCard);
    }

    fn assert_ids_unique(
        count: usize,
        id_of: fn(usize) -> String,
        focus_of: fn(usize) -> MenuFocus,
    ) {
        let ids: Vec<String> = (0..count).map(id_of).collect();
        let distinct: HashSet<&str> = ids.iter().map(String::as_str).collect();
        assert_eq!(distinct.len(), ids.len(), "duplicate ids: {ids:?}");

        for (index, id) in ids.iter().enumerate() {
            assert_eq!(
                super::focus_for_id(id),
                Some(focus_of(index)),
                "{id} did not round-trip"
            );
        }
    }
}
