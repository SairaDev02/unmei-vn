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
/// The foot's two stops keep the page's own controls keyboard-reachable, as the
/// 2026-11-18 amendment did for the Settings rail.
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

/// The walk's order past the ledger: the four slots, the legacy row when it is
/// shown, Resume, and Exit to title when it is shown.
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

/// Where the page draws its foot: `Resume reading` on every page the contents
/// overlay draws, `Exit to title` beside it only with a live session. The title's
/// door draws neither, so its gallery keeps the grid's own clamp
/// (`docs/title-and-menu.md` § Default look §4).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Foot {
    pub resume: bool,
    pub exit: bool,
}

/// Read the gallery's arrow keys into a focus move. Any arrow from a section
/// entry lands on the first card, so the grid is one key away; the digits are the
/// ledger's shortcut and do nothing here. Where the page draws its foot, the foot
/// trails the grid: `↓` from the last card reaches Resume reading and then Exit to
/// title, `↑` walks back to that card, and `←`/`→` stay inside the grid — the tail
/// the ledger and Settings already have (`docs/title-and-menu.md` § Default look
/// §4). A view with no cards reaches the foot on any arrow, so the keyboard is
/// equal to the pointer there too.
pub(super) fn gallery_arrow(
    focus: MenuFocus,
    keys: &ButtonInput<KeyCode>,
    columns: usize,
    count: usize,
    foot: Foot,
) -> Option<MenuFocus> {
    let dir = arrow_dir(keys)?;
    if foot.resume && matches!(focus, MenuFocus::Resume | MenuFocus::ExitToTitle) {
        let last = count.checked_sub(1).map(MenuFocus::GalleryCard);
        return foot_step(focus, dir, last, foot.exit);
    }
    if count == 0 {
        return foot.resume.then_some(MenuFocus::Resume);
    }
    let MenuFocus::GalleryCard(index) = focus else {
        return Some(MenuFocus::GalleryCard(0));
    };
    match gallery_move(index, count, columns, dir) {
        Some(next) => Some(MenuFocus::GalleryCard(next)),
        None if dir == GalleryDir::Down && foot.resume => Some(MenuFocus::Resume),
        None => None,
    }
}

/// One arrow move over the page's foot: `↑` returns to the walk's last card —
/// nowhere, when the view has none — `↓` reaches Exit to title when the page draws
/// it, and `←`/`→` stay out of the foot. Both ends clamp.
fn foot_step(
    focus: MenuFocus,
    dir: GalleryDir,
    last: Option<MenuFocus>,
    exit: bool,
) -> Option<MenuFocus> {
    match (focus, dir) {
        (MenuFocus::Resume, GalleryDir::Up) => last,
        (MenuFocus::Resume, GalleryDir::Down) if exit => Some(MenuFocus::ExitToTitle),
        (MenuFocus::ExitToTitle, GalleryDir::Up) => Some(MenuFocus::Resume),
        _ => None,
    }
}

/// The arrow key pressed this frame, if any: the four moves every card walk
/// shares.
fn arrow_dir(keys: &ButtonInput<KeyCode>) -> Option<GalleryDir> {
    if keys.just_pressed(KeyCode::ArrowLeft) {
        Some(GalleryDir::Left)
    } else if keys.just_pressed(KeyCode::ArrowRight) {
        Some(GalleryDir::Right)
    } else if keys.just_pressed(KeyCode::ArrowUp) {
        Some(GalleryDir::Up)
    } else if keys.just_pressed(KeyCode::ArrowDown) {
        Some(GalleryDir::Down)
    } else {
        None
    }
}

/// One Timeline arrow move: horizontal `±1`, vertical `±2`, clamped to
/// `[0, last]` inside the grid and never wrapping. The clamp may move diagonally
/// at the ends, which is deliberate (`docs/title-and-menu.md` § Timeline
/// `### Layout` `#### Walk and scrolling`). Locked cards stay in the walk, and a
/// move past the last card leaves the grid for the page's foot, which
/// [`timeline_arrow`] owns.
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
/// nothing here (`docs/title-and-menu.md` § Timeline §2). Where the page draws its
/// foot, the foot trails the cards: `↓` from the last card reaches Resume reading
/// and then Exit to title, and `↑` walks back to that card, the tail the ledger
/// and Settings already have (§ Default look §4). A Timeline with no cards reaches
/// the foot on any arrow, so the keyboard is equal to the pointer there too.
pub(super) fn timeline_arrow(
    focus: MenuFocus,
    keys: &ButtonInput<KeyCode>,
    count: usize,
    foot: Foot,
) -> Option<MenuFocus> {
    let dir = arrow_dir(keys)?;
    if foot.resume && matches!(focus, MenuFocus::Resume | MenuFocus::ExitToTitle) {
        let last = count.checked_sub(1).map(MenuFocus::TimelineCard);
        return foot_step(focus, dir, last, foot.exit);
    }
    if count == 0 {
        return foot.resume.then_some(MenuFocus::Resume);
    }
    let MenuFocus::TimelineCard(index) = focus else {
        return Some(MenuFocus::TimelineCard(0));
    };
    match timeline_move(index, count, dir) {
        Some(next) => Some(MenuFocus::TimelineCard(next)),
        None if dir == GalleryDir::Down && foot.resume => Some(MenuFocus::Resume),
        None => None,
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
