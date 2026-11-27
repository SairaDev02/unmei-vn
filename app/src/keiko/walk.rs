//! The arrows an activation walks with: which key to try first so one fixed order
//! reaches every control a case names (`docs/keiko-spec.md` §3.1).

use bevy::prelude::KeyCode;

use crate::menu::MenuFocus;

/// The arrows an activation walks with, and the order it tries them in: vertical
/// first, so a slot ledger, a Settings group and a column of sections all step.
pub(super) const WALK_KEYS: [KeyCode; 4] = [
    KeyCode::ArrowDown,
    KeyCode::ArrowRight,
    KeyCode::ArrowUp,
    KeyCode::ArrowLeft,
];

/// The arrows to try from `focus` towards `target`, which is what lets one fixed
/// order reach every control.
///
/// Two card focuses of one family compare by index, so a target before the focused
/// card is walked leftwards and one after it rightwards: [`WALK_KEYS`] alone would
/// step down to the page's foot and back, never reaching the card in between. From
/// the page's foot the walk leaves upwards, because that is where its cards are —
/// `↓` there only reaches `Exit to title` and back. Anything else keeps
/// [`WALK_KEYS`].
pub(super) fn walk_keys(focus: MenuFocus, target: MenuFocus) -> [KeyCode; 4] {
    let pair = match (focus, target) {
        (MenuFocus::GalleryCard(here), MenuFocus::GalleryCard(want)) => Some((here, want)),
        (MenuFocus::TimelineCard(here), MenuFocus::TimelineCard(want)) => Some((here, want)),
        (MenuFocus::Card(here), MenuFocus::Card(want)) => Some((here, want)),
        _ => None,
    };
    if pair.is_none()
        && matches!(focus, MenuFocus::Resume | MenuFocus::ExitToTitle)
        && matches!(
            target,
            MenuFocus::GalleryCard(_) | MenuFocus::TimelineCard(_) | MenuFocus::Card(_)
        )
    {
        return [
            KeyCode::ArrowUp,
            KeyCode::ArrowLeft,
            KeyCode::ArrowDown,
            KeyCode::ArrowRight,
        ];
    }
    match pair {
        Some((here, want)) if here > want => [
            KeyCode::ArrowLeft,
            KeyCode::ArrowUp,
            KeyCode::ArrowDown,
            KeyCode::ArrowRight,
        ],
        Some((here, want)) if here < want => [
            KeyCode::ArrowRight,
            KeyCode::ArrowDown,
            KeyCode::ArrowUp,
            KeyCode::ArrowLeft,
        ],
        _ => WALK_KEYS,
    }
}
