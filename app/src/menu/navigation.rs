//! Pure navigation over the pause surface's controls: the title and Settings
//! walks, the contents page's ledger walk and its Tab cycle, and the gallery and
//! Timeline card walks. Every function here is a focus computation with no world
//! access.

use bevy::input::ButtonInput;
use bevy::prelude::KeyCode;

use super::labels::{SETTINGS_ORDER, TITLE_ORDER};
use super::state::MenuFocus;

/// Tab cycles the page's sections: Save → Load → Timeline → Gallery → Settings
/// → Save. Inside the gallery, Timeline and Settings the cycle narrows to Save →
/// Load, which keeps `Tab` off the section the page is already on
/// (`docs/title-and-menu.md` § Default look §4).
pub(super) fn next_top_bar(focus: MenuFocus) -> MenuFocus {
    match focus {
        MenuFocus::SaveTab => MenuFocus::LoadTab,
        MenuFocus::LoadTab => MenuFocus::TimelineButton,
        MenuFocus::TimelineButton => MenuFocus::GalleryButton,
        MenuFocus::GalleryButton => MenuFocus::SettingsButton,
        _ => MenuFocus::SaveTab,
    }
}

/// The narrowed cycle the gallery and Settings views share: Save → Load → Save.
pub(super) fn next_top_bar_subview(focus: MenuFocus) -> MenuFocus {
    match focus {
        MenuFocus::SaveTab => MenuFocus::LoadTab,
        _ => MenuFocus::SaveTab,
    }
}

/// The ledger walk in Save and Load: `↑`/`↓` step the slots in `Slot::ALL` order
/// and stop at both ends, with the legacy entry after them in save mode and the
/// page's foot — Resume, then Exit to title — past the last row. Any other focus
/// lands on the first row, so `Tab` and the digits are always one press from the
/// ledger. `←`/`→` never move here (`docs/title-and-menu.md` § Default look §4).
///
/// Resume and Exit are the walk's tail because the two controls stand at the
/// page's foot in every section: `Esc` is Resume's twin, but focus is how the
/// keyboard (and Keiko) reaches a control, and §4's "every section reaches it"
/// needs a stop. `app/src/tests/overlay.rs` pins both ends.
pub(super) fn ledger_target(
    focus: MenuFocus,
    keys: &ButtonInput<KeyCode>,
    legacy_shown: bool,
    exit_shown: bool,
) -> Option<MenuFocus> {
    let delta = if keys.just_pressed(KeyCode::ArrowDown) {
        1
    } else if keys.just_pressed(KeyCode::ArrowUp) {
        -1
    } else {
        return None;
    };
    let order = ledger_order(legacy_shown, exit_shown);
    let Some(index) = order.iter().position(|entry| *entry == focus) else {
        return Some(order[0]);
    };
    let next = (index as i32 + delta).clamp(0, order.len() as i32 - 1) as usize;
    Some(order[next])
}

/// The ledger walk's order: the four slots, the legacy row when it is shown,
/// Resume, and Exit to title when it is shown.
pub(super) fn ledger_order(legacy_shown: bool, exit_shown: bool) -> Vec<MenuFocus> {
    let mut order = vec![
        MenuFocus::Card(0),
        MenuFocus::Card(1),
        MenuFocus::Card(2),
        MenuFocus::Card(3),
    ];
    if legacy_shown {
        order.push(MenuFocus::LegacyCard);
    }
    order.push(MenuFocus::Resume);
    if exit_shown {
        order.push(MenuFocus::ExitToTitle);
    }
    order
}

/// Digits `1`–`3` focus a manual slot; they never activate it.
pub(super) fn focus_by_digit(keys: &ButtonInput<KeyCode>) -> Option<MenuFocus> {
    for (index, key) in DIGIT_KEYS.iter().enumerate() {
        if keys.just_pressed(*key) {
            return Some(MenuFocus::Card(index));
        }
    }
    None
}

/// One gallery arrow move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GalleryDir {
    Left,
    Right,
    Up,
    Down,
}

/// The card a gallery arrow move lands on, or `None` for a no-op. The walk does
/// not wrap: `←`/`→` step one card; `↑`/`↓` step one row by `columns` and clamp
/// to the grid's last entry when the destination row is short
/// (`docs/title-and-menu.md` § Gallery `### Layout` §5).
pub fn gallery_move(index: usize, count: usize, columns: usize, dir: GalleryDir) -> Option<usize> {
    if count == 0 || index >= count {
        return None;
    }
    let columns = columns.max(1);
    let next = match dir {
        GalleryDir::Left => index.checked_sub(1),
        GalleryDir::Right => (index + 1 < count).then_some(index + 1),
        GalleryDir::Up => index.checked_sub(columns),
        GalleryDir::Down => {
            let down = index + columns;
            if down < count {
                Some(down)
            } else if index + 1 < count {
                Some(count - 1)
            } else {
                None
            }
        }
    };
    next.filter(|target| *target != index)
}

/// The entry a still browse step lands on: `←`/`→` step one entry and wrap at
/// both ends. `None` only when the gallery has no entries to browse
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §5).
pub fn gallery_browse(index: usize, count: usize, delta: isize) -> Option<usize> {
    if count == 0 {
        return None;
    }
    Some((index as isize + delta).rem_euclid(count as isize) as usize)
}

/// Read the gallery's arrow keys into a focus move. Any arrow from a section
/// entry lands on the first card, so the grid is one key away; the digits are the
/// ledger's shortcut and do nothing here.
pub(super) fn gallery_arrow(
    focus: MenuFocus,
    keys: &ButtonInput<KeyCode>,
    columns: usize,
    count: usize,
) -> Option<MenuFocus> {
    if count == 0 {
        return None;
    }
    let dir = if keys.just_pressed(KeyCode::ArrowLeft) {
        GalleryDir::Left
    } else if keys.just_pressed(KeyCode::ArrowRight) {
        GalleryDir::Right
    } else if keys.just_pressed(KeyCode::ArrowUp) {
        GalleryDir::Up
    } else if keys.just_pressed(KeyCode::ArrowDown) {
        GalleryDir::Down
    } else {
        return None;
    };
    match focus {
        MenuFocus::GalleryCard(index) => {
            gallery_move(index, count, columns, dir).map(MenuFocus::GalleryCard)
        }
        _ => Some(MenuFocus::GalleryCard(0)),
    }
}

/// One Timeline arrow move: horizontal `±1`, vertical `±2`, clamped to
/// `[0, last]` and never wrapping. The clamp may move diagonally at the ends,
/// which is deliberate (`docs/title-and-menu.md` § Timeline `### Layout`
/// `#### Walk and scrolling`). Locked cards stay in the walk.
pub fn timeline_move(index: usize, count: usize, dir: GalleryDir) -> Option<usize> {
    if count == 0 || index >= count {
        return None;
    }
    let last = count - 1;
    let next = match dir {
        GalleryDir::Left => index.saturating_sub(1),
        GalleryDir::Right => (index + 1).min(last),
        GalleryDir::Up => index.saturating_sub(2),
        GalleryDir::Down => (index + 2).min(last),
    };
    (next != index).then_some(next)
}

/// Read the Timeline's arrow keys into a focus move. Any arrow from a section
/// entry lands on the first card; the digits are the ledger's shortcut and do
/// nothing here (`docs/title-and-menu.md` § Timeline §2).
pub(super) fn timeline_arrow(
    focus: MenuFocus,
    keys: &ButtonInput<KeyCode>,
    count: usize,
) -> Option<MenuFocus> {
    if count == 0 {
        return None;
    }
    let dir = if keys.just_pressed(KeyCode::ArrowLeft) {
        GalleryDir::Left
    } else if keys.just_pressed(KeyCode::ArrowRight) {
        GalleryDir::Right
    } else if keys.just_pressed(KeyCode::ArrowUp) {
        GalleryDir::Up
    } else if keys.just_pressed(KeyCode::ArrowDown) {
        GalleryDir::Down
    } else {
        return None;
    };
    match focus {
        MenuFocus::TimelineCard(index) => {
            timeline_move(index, count, dir).map(MenuFocus::TimelineCard)
        }
        _ => Some(MenuFocus::TimelineCard(0)),
    }
}

/// The Settings view's vertical navigation: `↑`/`↓` walk
/// [`SETTINGS_ORDER`] with wrap-around, and any vertical move from a section
/// entry lands on the first slider (or the walk's last stop going up). The page's
/// session-scoped `Exit to title` joins the walk after the Decisions group when
/// the overlay's door draws it, so the group keeps its settled position and the
/// foot control is the one stop past it (`docs/title-and-menu.md` §4).
pub(super) fn settings_target(
    focus: MenuFocus,
    keys: &ButtonInput<KeyCode>,
    exit_to_title: bool,
) -> Option<MenuFocus> {
    let delta = if keys.just_pressed(KeyCode::ArrowDown) {
        1
    } else if keys.just_pressed(KeyCode::ArrowUp) {
        -1
    } else {
        return None;
    };
    let mut order: Vec<MenuFocus> = SETTINGS_ORDER.to_vec();
    if exit_to_title {
        order.push(MenuFocus::ExitToTitle);
    }
    let Some(index) = order.iter().position(|entry| *entry == focus) else {
        return Some(if delta > 0 {
            order[0]
        } else {
            order[order.len() - 1]
        });
    };
    let next = (index as i32 + delta).rem_euclid(order.len() as i32) as usize;
    Some(order[next])
}

/// The title's vertical walk: `↑`/`↓` over [`TITLE_ORDER`], wrapping. Disabled
/// rows are ordinary entries, so their caption and reason stay reachable.
pub(super) fn title_target(focus: MenuFocus, keys: &ButtonInput<KeyCode>) -> Option<MenuFocus> {
    let delta = if keys.just_pressed(KeyCode::ArrowDown) {
        1
    } else if keys.just_pressed(KeyCode::ArrowUp) {
        -1
    } else {
        return None;
    };
    let index = TITLE_ORDER.iter().position(|entry| *entry == focus)?;
    let next = (index as i32 + delta).rem_euclid(TITLE_ORDER.len() as i32) as usize;
    Some(TITLE_ORDER[next])
}

const DIGIT_KEYS: [KeyCode; 3] = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3];
