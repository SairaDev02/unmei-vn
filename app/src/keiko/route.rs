//! `activate`'s planned routes (spec §3.1): a breadth-first search over the
//! menu's one walk function for the shortest key sequence to a control.

use bevy::prelude::KeyCode;

use crate::menu::{walk, MenuFocus, MenuState, WalkCounts};

/// The walk keys, in the order a tie between two shortest routes is broken.
pub const WALK_KEYS: [KeyCode; 5] = [
    KeyCode::ArrowDown,
    KeyCode::ArrowRight,
    KeyCode::ArrowUp,
    KeyCode::ArrowLeft,
    KeyCode::Tab,
];

/// No route reaches the target: the search exhausted every focus it could
/// reach from `from`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unreachable {
    pub from: MenuFocus,
    pub searched: usize,
}

/// One planned step: the key to press and the focus it must leave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hop {
    pub key: KeyCode,
    pub to: MenuFocus,
}

/// The shortest walk-key route from `from` to `target` on `menu`'s current
/// surface. Between two routes of the same length, the one whose first
/// differing key comes first in [`WALK_KEYS`] wins.
pub fn plan(
    menu: &MenuState,
    counts: WalkCounts,
    from: MenuFocus,
    target: MenuFocus,
) -> Result<Vec<Hop>, Unreachable> {
    let mut reached: Vec<(MenuFocus, Option<(usize, KeyCode)>)> = vec![(from, None)];
    let mut next = 0;
    while next < reached.len() {
        let (focus, _) = reached[next];
        if focus == target {
            return Ok(route_to(&reached, next));
        }
        for key in WALK_KEYS {
            let Some(to) = walk(menu, counts, focus, key) else {
                continue;
            };
            if reached.iter().all(|(seen, _)| *seen != to) {
                reached.push((to, Some((next, key))));
            }
        }
        next += 1;
    }
    Err(Unreachable {
        from,
        searched: reached.len(),
    })
}

fn route_to(reached: &[(MenuFocus, Option<(usize, KeyCode)>)], mut at: usize) -> Vec<Hop> {
    let mut route = Vec::new();
    while let (to, Some((parent, key))) = reached[at] {
        route.push(Hop { key, to });
        at = parent;
    }
    route.reverse();
    route
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::{MenuOrigin, MenuView};

    fn keys(route: &[Hop]) -> Vec<KeyCode> {
        route.iter().map(|hop| hop.key).collect()
    }

    fn surface(view: MenuView) -> MenuState {
        let mut menu = MenuState::default();
        menu.open = true;
        menu.view = view;
        menu.settings_origin = MenuOrigin::Title;
        menu
    }

    #[test]
    fn the_overlays_timeline_control_is_three_tabs_from_the_first_slot() {
        let menu = surface(MenuView::Slots);
        let route = plan(
            &menu,
            WalkCounts::default(),
            MenuFocus::Card(0),
            MenuFocus::TimelineButton,
        )
        .expect("Tab reaches the top bar");
        assert_eq!(keys(&route), vec![KeyCode::Tab; 3]);
        assert_eq!(
            route.iter().map(|hop| hop.to).collect::<Vec<_>>(),
            vec![
                MenuFocus::SaveTab,
                MenuFocus::LoadTab,
                MenuFocus::TimelineButton
            ]
        );
    }

    #[test]
    fn a_tie_between_two_shortest_routes_takes_the_earlier_key_first() {
        let menu = surface(MenuView::Slots);
        let route = plan(
            &menu,
            WalkCounts::default(),
            MenuFocus::Card(0),
            MenuFocus::Card(3),
        )
        .expect("the grid's far corner is reachable");
        assert_eq!(keys(&route), vec![KeyCode::ArrowDown, KeyCode::ArrowRight]);
    }

    #[test]
    fn the_title_walk_takes_the_short_way_round() {
        let menu = surface(MenuView::Title);
        let route = plan(
            &menu,
            WalkCounts::default(),
            MenuFocus::TitleNewGame,
            MenuFocus::TitleQuit,
        )
        .expect("the title wraps");
        assert_eq!(keys(&route), vec![KeyCode::ArrowUp, KeyCode::ArrowUp]);
    }

    #[test]
    fn a_route_to_the_current_focus_is_empty() {
        let menu = surface(MenuView::Title);
        let route = plan(
            &menu,
            WalkCounts::default(),
            MenuFocus::TitleGallery,
            MenuFocus::TitleGallery,
        );
        assert_eq!(route, Ok(Vec::new()));
    }

    #[test]
    fn a_pointer_only_control_is_unreachable_after_every_stop_is_searched() {
        let menu = surface(MenuView::Settings);
        let route = plan(
            &menu,
            WalkCounts::default(),
            MenuFocus::AudioMaster,
            MenuFocus::SettingsBack,
        );
        assert_eq!(
            route,
            Err(Unreachable {
                from: MenuFocus::AudioMaster,
                searched: 12,
            })
        );
    }

    #[test]
    fn a_gallery_walk_from_the_title_door_never_leaves_the_cards() {
        let mut menu = surface(MenuView::Gallery);
        menu.gallery_origin = MenuOrigin::Title;
        menu.gallery_columns = 3;
        let counts = WalkCounts {
            gallery: 5,
            timeline: 0,
        };
        let route = plan(
            &menu,
            counts,
            MenuFocus::GalleryCard(0),
            MenuFocus::GalleryCard(4),
        )
        .expect("the last card is reachable");
        assert_eq!(keys(&route), vec![KeyCode::ArrowDown, KeyCode::ArrowDown]);
        assert_eq!(
            plan(&menu, counts, MenuFocus::GalleryCard(0), MenuFocus::SaveTab),
            Err(Unreachable {
                from: MenuFocus::GalleryCard(0),
                searched: 5,
            })
        );
    }
}
