//! The walk's gallery, timeline and confirmation steps.

use crate::prelude::*;

pub(crate) fn door_steps() -> Vec<Step> {
    vec![
        // --- gallery and timeline -------------------------------------------
        Step::new("gallery: leave Settings for the slot grid")
            .at(0, close_drawer)
            .at(6, |world, _harness| {
                world.resource_mut::<CredentialsState>().row = KeyRow::NotSet;
            })
            .at(12, |world, harness| {
                tap_key(world, harness, KeyCode::Escape)
            })
            .settle(18)
            .check(|world, harness| {
                let menu = world.resource::<MenuState>();
                println!("  after Esc: open={} view={:?}", menu.open, menu.view);
                harness.report.check(
                    "the overlay is back on the slot grid",
                    menu.open && menu.view == MenuView::Slots,
                );
            }),
        Step::new("gallery: the grid opens under Monitor")
            .at(0, |world, _harness| {
                world.resource_mut::<MenuState>().focus = MenuFocus::GalleryButton;
            })
            .at(5, |world, harness| tap_key(world, harness, KeyCode::Enter))
            .at(16, open_drawer)
            .shot(24, "11-gallery-grid")
            .settle(30)
            .check(|world, harness| {
                let menu = world.resource::<MenuState>();
                println!("  gallery: open={} view={:?}", menu.open, menu.view);
                harness.report.check(
                    "the gallery grid is up under the drawer",
                    menu.open && menu.view == MenuView::Gallery && !menu.gallery_still,
                );
                check_panel(world, harness, "gallery grid");
                check_underlying_unchanged(world, harness);
            }),
        Step::new("gallery: the still opens under Monitor")
            .at(0, close_drawer)
            .at(8, |world, harness| tap_key(world, harness, KeyCode::Enter))
            .at(20, open_drawer)
            .shot(28, "12-gallery-still")
            .settle(36)
            .check(|world, harness| {
                let menu = world.resource::<MenuState>();
                println!("  still: open={} still={}", menu.open, menu.gallery_still);
                harness.report.check(
                    "the still is up under the drawer",
                    menu.open && menu.view == MenuView::Gallery && menu.gallery_still,
                );
                check_panel(world, harness, "gallery still");
                check_underlying_unchanged(world, harness);
            }),
        Step::new("timeline: leave the still and the gallery for the grid")
            .at(0, close_drawer)
            .at(8, |world, harness| tap_key(world, harness, KeyCode::Escape))
            .at(18, |world, harness| {
                tap_key(world, harness, KeyCode::Escape)
            })
            .settle(26)
            .check(|world, harness| {
                let menu = world.resource::<MenuState>();
                println!("  back: open={} view={:?}", menu.open, menu.view);
                harness.report.check(
                    "two Escs leave the still and the gallery for the grid",
                    menu.open && menu.view == MenuView::Slots,
                );
            }),
        Step::new("timeline: the declared order opens under Monitor")
            .at(0, |world, _harness| {
                world.resource_mut::<MenuState>().focus = MenuFocus::TimelineButton;
            })
            .at(5, |world, harness| tap_key(world, harness, KeyCode::Enter))
            .at(16, open_drawer)
            .shot(24, "13-timeline")
            .settle(30)
            .check(|world, harness| {
                let menu = world.resource::<MenuState>();
                println!("  timeline: open={} view={:?}", menu.open, menu.view);
                harness.report.check(
                    "the Timeline view is up under the drawer",
                    menu.open && menu.view == MenuView::Timeline,
                );
                check_panel(world, harness, "timeline");
                check_underlying_unchanged(world, harness);
            }),
        // The overlay door's Settings body is a bounded viewport, so its lower
        // controls and `← Back to slots` are reachable at 1280×720
        // (`docs/title-and-menu.md` § Title layout §5). These run after the
        // reader's anchor checks: opening Settings emits the ambient
        // credential-directory failure row, which is not part of that small
        // history and must not shift its anchor expectations.
        Step::new("settings(overlay door): leave Timeline and open Settings")
            .at(0, close_drawer)
            .at(6, |world, harness| tap_key(world, harness, KeyCode::Escape))
            .at(12, |world, _harness| {
                world.resource_mut::<MenuState>().focus = MenuFocus::SettingsButton;
            })
            .at(16, |world, harness| tap_key(world, harness, KeyCode::Enter))
            .at(26, park_cursor_over_settings)
            .at(30, wheel_settings_to_bottom)
            .settle(38)
            .check(check_settings_wheel_reaches_back),
        Step::new("settings(overlay door): focusing Clear key reveals it")
            .at(0, wheel_settings_to_top)
            .at(6, focus_clear_key)
            .settle(14)
            .check(check_settings_focus_reveals_clear_key),
        Step::new("settings(overlay door): Esc returns to the slot grid")
            .at(0, |world, harness| tap_key(world, harness, KeyCode::Escape))
            .at(10, open_drawer)
            .settle(18)
            .check(|world, harness| {
                let menu = world.resource::<MenuState>();
                harness.report.check(
                    "Esc backed out of the overlay's Settings",
                    menu.open && menu.view == MenuView::Slots,
                );
                harness.report.check(
                    "Monitor reopened over the slot grid",
                    crate::harness::snap(world).drawer_open(),
                );
            }),
        // --- confirmation ----------------------------------------------------
        Step::new("confirm: the title's New Game raises the destructive confirmation")
            .at(0, close_drawer)
            .at(6, |world, _harness| {
                let mut menu = world.resource_mut::<MenuState>();
                menu.open = true;
                menu.view = MenuView::Title;
                menu.focus = MenuFocus::TitleNewGame;
            })
            .at(12, |world, harness| tap_key(world, harness, KeyCode::Enter))
            .settle(18)
            .check(|world, harness| {
                let menu = world.resource::<MenuState>();
                println!(
                    "  confirm: open={} view={:?} focus={:?} confirm={:?}",
                    menu.open, menu.view, menu.focus, menu.confirm
                );
                harness.report.check(
                    "the live session raised a confirmation",
                    matches!(menu.confirm, Some(ConfirmKind::NewGame)),
                );
            }),
        Step::new("confirm: F3 opens Monitor over the confirmation")
            .at(0, open_drawer)
            .at(3, park_focus)
            .shot(8, "14-confirmation")
            .settle(14)
            .check(|world, harness| {
                check_panel(world, harness, "confirmation");
                check_underlying_unchanged(world, harness);
            }),
        Step::new("confirm: Enter under Monitor cannot accept the confirmation")
            .at(0, |world, harness| tap_key(world, harness, KeyCode::Enter))
            .settle(10)
            .check(|world, harness| {
                let menu = world.resource::<MenuState>();
                harness.report.check(
                    "Enter under Monitor did not accept the confirmation",
                    matches!(menu.confirm, Some(ConfirmKind::NewGame)),
                );
            }),
        Step::new("confirm: closing Monitor leaves the confirmation up")
            .at(0, close_drawer)
            .settle(10)
            .check(|world, harness| {
                harness.report.check(
                    "Monitor is closed",
                    !crate::harness::snap(world).drawer_open(),
                );
                let menu = world.resource::<MenuState>();
                harness.report.check(
                    "the confirmation is still up",
                    matches!(menu.confirm, Some(ConfirmKind::NewGame)),
                );
            }),
        Step::new("confirm: a fresh Esc cancels the confirmation")
            .at(0, |world, harness| tap_key(world, harness, KeyCode::Escape))
            .settle(10)
            .check(|world, harness| {
                let menu = world.resource::<MenuState>();
                harness
                    .report
                    .check("Esc cancelled the confirmation", menu.confirm.is_none());
            }),
        // The title door's Settings panel is bounded and internally scrollable,
        // so its lower controls and `← Back to title` are reachable at 1280×720
        // (`docs/title-and-menu.md` § Title layout §5).
        Step::new("settings(title door): the wheel reaches the back control")
            .at(0, open_title_settings)
            .at(10, park_cursor_over_settings)
            .at(14, wheel_settings_to_bottom)
            .settle(22)
            .check(check_settings_wheel_reaches_back),
        Step::new("settings(title door): focusing Clear key reveals it")
            .at(0, wheel_settings_to_top)
            .at(6, focus_clear_key)
            .settle(14)
            .check(check_settings_focus_reveals_clear_key),
        Step::new("settings(title door): Esc returns to the title")
            .at(0, |world, harness| tap_key(world, harness, KeyCode::Escape))
            .settle(8)
            .check(|world, harness| {
                let menu = world.resource::<MenuState>();
                harness.report.check(
                    "Esc backed out of the title's Settings",
                    menu.open && menu.view == MenuView::Title,
                );
            }),
    ]
}
