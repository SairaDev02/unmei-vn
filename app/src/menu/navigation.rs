//! Pure navigation over the pause surface's controls: the title and Settings
//! walks, the slot grid's arrow/digit rule, and the gallery and Timeline card
//! walks. Every function here is a focus computation with no world access.

use bevy::input::ButtonInput;
use bevy::prelude::KeyCode;

use super::labels::{SETTINGS_ORDER, TITLE_ORDER};
use super::state::MenuFocus;

/// Tab cycles the top bar: Save → Load → Timeline → Gallery → Settings → Save.
/// In the gallery, Timeline and Settings views none of the three sub-view
/// controls is rendered, so the cycle narrows to Save → Load
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §1, § Timeline §1).
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

/// Arrow navigation over the 2×2 grid; any arrow from a non-card control lands
/// on the first card, and `Left` from the grid's left column reaches the rail's
/// Resume button (so Resume is keyboard-reachable, not mouse-only). When the
/// Legacy card is shown, `Down` from the grid's bottom row reaches it, and
/// `Up` returns.
pub(super) fn arrow_target(
    focus: MenuFocus,
    keys: &ButtonInput<KeyCode>,
    legacy_shown: bool,
) -> Option<MenuFocus> {
    let MenuFocus::Card(index) = focus else {
        if focus == MenuFocus::LegacyCard && keys.just_pressed(KeyCode::ArrowUp) {
            return Some(MenuFocus::Card(2));
        }
        let moved = keys.just_pressed(KeyCode::ArrowLeft)
            || keys.just_pressed(KeyCode::ArrowRight)
            || keys.just_pressed(KeyCode::ArrowUp)
            || keys.just_pressed(KeyCode::ArrowDown);
        return moved.then_some(MenuFocus::Card(0));
    };

    let right = if index % 2 == 0 { index + 1 } else { index };
    let left = if index % 2 == 1 { index - 1 } else { index };
    let down = if index < 2 { index + 2 } else { index };
    let up = if index >= 2 { index - 2 } else { index };

    if keys.just_pressed(KeyCode::ArrowRight) {
        Some(MenuFocus::Card(right))
    } else if keys.just_pressed(KeyCode::ArrowLeft) {
        if index % 2 == 0 {
            Some(MenuFocus::Resume)
        } else {
            Some(MenuFocus::Card(left))
        }
    } else if keys.just_pressed(KeyCode::ArrowDown) {
        if legacy_shown && index >= 2 {
            Some(MenuFocus::LegacyCard)
        } else {
            Some(MenuFocus::Card(down))
        }
    } else if keys.just_pressed(KeyCode::ArrowUp) {
        Some(MenuFocus::Card(up))
    } else {
        None
    }
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

/// Read the gallery's arrow keys into a focus move. Any arrow from a top-bar
/// control lands on the first card, so the grid is one key away; digits remain
/// the slot grid's shortcut and do nothing here.
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

/// Read the Timeline's arrow keys into a focus move. Any arrow from a top-bar
/// control lands on the first card; digits remain the slot grid's shortcut and
/// do nothing here (`docs/title-and-menu.md` § Timeline §2).
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
/// [`SETTINGS_ORDER`] with wrap-around, and any vertical move from the top bar
/// lands on the first slider (or the back control going up).
pub(super) fn settings_target(focus: MenuFocus, keys: &ButtonInput<KeyCode>) -> Option<MenuFocus> {
    let delta = if keys.just_pressed(KeyCode::ArrowDown) {
        1
    } else if keys.just_pressed(KeyCode::ArrowUp) {
        -1
    } else {
        return None;
    };
    let Some(index) = SETTINGS_ORDER.iter().position(|entry| *entry == focus) else {
        return Some(if delta > 0 {
            SETTINGS_ORDER[0]
        } else {
            SETTINGS_ORDER[SETTINGS_ORDER.len() - 1]
        });
    };
    let next = (index as i32 + delta).rem_euclid(SETTINGS_ORDER.len() as i32) as usize;
    Some(SETTINGS_ORDER[next])
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
