//! The walk's pointer, reader, fullscreen and play-surface steps.

use crate::prelude::*;

pub(crate) fn input_steps() -> Vec<Step> {
    vec![
        // --- pointer, hold/repeat and focus loss ----------------------------
        Step::new("pointer: the Close control dismisses the drawer")
            .at(0, |world, harness| tap_control(world, harness, "Close"))
            .settle(8)
            .check(|world, harness| {
                harness.report.check(
                    "the pointer activation closed Monitor",
                    !crate::harness::snap(world).drawer_open(),
                );
            }),
        Step::new("pointer: an outside press while Monitor owns input is inert")
            .at(0, |world, harness| {
                open_drawer(world, harness);
            })
            .settle(6)
            .at(7, |world, harness| {
                // A point above the drawer, over whatever surface is underneath.
                let (width, _) = window_size(world);
                crate::harness::move_cursor(world, Vec2::new(width / 2.0, 40.0));
                world.write_message(MouseButtonInput {
                    button: MouseButton::Left,
                    state: ButtonState::Pressed,
                    window: Entity::PLACEHOLDER,
                });
                harness.queued.push(Queued::Button(MouseButton::Left));
            })
            .settle(14)
            .check(|world, harness| {
                let snap = crate::harness::snap(world);
                harness.report.check(
                    "the outside press left Monitor open and the surface untouched",
                    snap.drawer_open(),
                );
                check_underlying_unchanged(world, harness);
            }),
        Step::new("hold: a fresh F3 opens and OS repeats cannot oscillate the drawer")
            .at(0, |world, harness| {
                if crate::harness::snap(world).drawer_open() {
                    close_drawer(world, harness);
                }
            })
            .at(8, |world, _harness| {
                crate::harness::key(world, KeyCode::F3, ButtonState::Pressed);
                // A repeated press is not a fresh press and must not toggle.
                crate::harness::repeat(world, KeyCode::F3);
                crate::harness::repeat(world, KeyCode::F3);
            })
            .settle(18)
            .check(|world, harness| {
                harness.report.check(
                    "repeated F3 presses left the drawer open",
                    crate::harness::snap(world).drawer_open(),
                );
            }),
        Step::new("hold: releasing and pressing F3 once more closes it")
            .at(0, |world, _harness| {
                crate::harness::release(world, KeyCode::F3)
            })
            .at(6, |world, harness| tap_key(world, harness, KeyCode::F3))
            .settle(14)
            .check(|world, harness| {
                harness.report.check(
                    "the fresh press closed Monitor",
                    !crate::harness::snap(world).drawer_open(),
                );
            }),
        Step::new("focus: window focus loss keeps the drawer and its view")
            .at(0, open_drawer)
            .settle(6)
            .check(|world, _harness| {
                let window = primary_window_entity(world);
                world.write_message(WindowFocused {
                    window,
                    focused: false,
                });
            })
            .settle(10)
            .check(|world, harness| {
                let snap = crate::harness::snap(world);
                harness
                    .report
                    .check("the drawer survived focus loss", snap.drawer_open());
                harness.report.check(
                    "focus loss kept the reader state truthful",
                    status_counters(&snap).is_some_and(|status| status.following || status.expired),
                );
            }),
        Step::new("focus: regaining focus does not replay an activation")
            .at(0, |world, _harness| {
                let window = primary_window_entity(world);
                world.write_message(WindowFocused {
                    window,
                    focused: true,
                });
            })
            .settle(10)
            .check(|world, harness| {
                harness.report.check(
                    "regaining focus left the drawer open with no replay",
                    crate::harness::snap(world).drawer_open(),
                );
            }),
        // --- reader: wheel, paging and the scrollbar ------------------------
        Step::new("reader: the log-body wheel leaves following with a top anchor")
            .at(0, |world, _harness| wheel_over_body(world, 9.5))
            .settle(6)
            .check(|world, harness| {
                let (status, snap) = status_of(world);
                if let Some(status) = status {
                    harness
                        .report
                        .check("the wheel left following", !status.following);
                }
                check_anchor_top(harness, world, &snap, "wheel");
                check_anchor_fractional(harness, world, &snap, "wheel");
            }),
        Step::new("reader: PgUp pages a full viewport older")
            .at(0, |world, harness| tap_key(world, harness, KeyCode::PageUp))
            .settle(6)
            .check(|world, harness| {
                let (_, snap) = status_of(world);
                check_anchor_top(harness, world, &snap, "paged");
            }),
        Step::new("reader: a scrollbar drag leaves the reader anchored")
            .at(0, |world, _harness| {
                drag_scrollbar(world, 0.3);
                world.write_message(MouseButtonInput {
                    button: MouseButton::Left,
                    state: ButtonState::Pressed,
                    window: Entity::PLACEHOLDER,
                });
            })
            .settle(8)
            .check(|world, harness| {
                let (status, snap) = status_of(world);
                if let Some(status) = status {
                    harness.report.check(
                        "the fractional scrollbar drag left following",
                        !status.following,
                    );
                }
                check_anchor_top(harness, world, &snap, "dragged");
            }),
        Step::new("reader: the scrollbar drag continues to the bottom and resumes following")
            .at(0, |world, _harness| drag_scrollbar(world, 1.0))
            .at(4, |world, _harness| {
                release_button(world, MouseButton::Left)
            })
            .settle(16)
            .check(|world, harness| {
                let (status, _) = status_of(world);
                if let Some(status) = status {
                    harness.report.check(
                        "the scrollbar drag to the bottom resumed following",
                        status.following,
                    );
                }
            }),
        // --- fullscreen ------------------------------------------------------
        Step::new("fullscreen: the 1920x1080 width contract (1872px drawer)")
            .at(0, |world, _harness| {
                let mut query = world.query_filtered::<&mut Window, With<PrimaryWindow>>();
                if let Ok(mut window) = query.single_mut(world) {
                    window.resolution =
                        WindowResolution::new(1920, 1080).with_scale_factor_override(1.0);
                }
            })
            .shot(8, "02-wide-1920")
            .settle(12)
            .check(|world, harness| {
                check_panel(world, harness, "1920x1080");
            }),
        Step::new("fullscreen: the shipping Settings toggle goes borderless")
            .at(0, |world, _harness| {
                world
                    .resource_mut::<unmei_app::settings::SettingsState>()
                    .settings
                    .play
                    .fullscreen = true;
            })
            .at(4, |world, harness| tap_key(world, harness, KeyCode::F11))
            .shot(14, "03-fullscreen")
            .settle(18)
            .check(|world, harness| {
                let (width, height) = window_size(world);
                println!("  fullscreen window: {width}x{height}");
                check_panel(world, harness, "fullscreen");
                let (status, _) = status_of(world);
                if let Some(status) = status {
                    harness
                        .report
                        .check("the fullscreen frame keeps following", status.following);
                }
                harness.report.check(
                    "F11 is inert while Monitor owns the frame",
                    world
                        .resource::<unmei_app::settings::SettingsState>()
                        .settings
                        .play
                        .fullscreen,
                );
            }),
        Step::new("fullscreen: leave the borderless mode through the same toggle")
            .at(0, |world, _harness| {
                world
                    .resource_mut::<unmei_app::settings::SettingsState>()
                    .settings
                    .play
                    .fullscreen = false;
            })
            .settle(14)
            .check(|world, harness| {
                let (width, height) = window_size(world);
                println!("  windowed again: {width}x{height}");
                harness.report.check(
                    "the shipping toggle left the borderless mode",
                    !world
                        .resource::<unmei_app::settings::SettingsState>()
                        .settings
                        .play
                        .fullscreen,
                );
            }),
        Step::new("fullscreen: back to the 1280x720 reference for the surfaces")
            .at(0, |world, _harness| {
                let mut query = world.query_filtered::<&mut Window, With<PrimaryWindow>>();
                if let Ok(mut window) = query.single_mut(world) {
                    window.resolution =
                        WindowResolution::new(1280, 720).with_scale_factor_override(1.0);
                }
            })
            .settle(14)
            .check(|world, harness| {
                let (width, height) = window_size(world);
                println!("  reference window: {width}x{height}");
                check_panel(world, harness, "reference window");
            }),
        // --- play surfaces ---------------------------------------------------
        Step::new("play: leave the title for the fixture's first say-line")
            .at(0, close_drawer)
            .at(5, |world, harness| tap_key(world, harness, KeyCode::Enter))
            .settle(14)
            .check(|world, harness| {
                let wait = wait_of(world);
                harness.report.check(
                    format!("the runner is parked on a say-line ({wait:?})"),
                    matches!(wait_kind(world), Some(Wait::Say { .. })),
                );
                let menu = world.resource::<MenuState>();
                harness
                    .report
                    .check("New Game left the title for live play", !menu.open);
            }),
        Step::new("play: F3 over the say-line leaves the HUD unmoved")
            .at(0, |world, harness| {
                open_drawer(world, harness);
                note_wait(world, harness);
            })
            .at(3, park_focus)
            .shot(4, "03-play-say")
            .settle(7)
            .check(|world, harness| {
                check_panel(world, harness, "play");
                check_hud_unchanged(world, harness);
            }),
        Step::new("play: Space under Monitor cannot advance the say-line")
            .at(0, |world, harness| tap_key(world, harness, KeyCode::Space))
            .settle(6)
            .check(check_recorded_wait),
        Step::new("play: Esc dismisses only Monitor, then a fresh Space advances")
            .at(0, close_drawer)
            .at(5, |world, harness| {
                note_wait(world, harness);
                tap_key(world, harness, KeyCode::Space);
            })
            .settle(8)
            .check(|world, harness| {
                let now = wait_of(world);
                harness.report.check(
                    format!("a fresh Space advanced the runner ({now:?})"),
                    now != harness.wait_before,
                );
                let menu = world.resource::<MenuState>();
                harness
                    .report
                    .check("Esc never opened the overlay", !menu.open);
            }),
        Step::new("play: advance onto the player-choice wait")
            .at(0, |world, harness| tap_key(world, harness, KeyCode::Space))
            .at(8, |world, harness| tap_key(world, harness, KeyCode::Space))
            .at(16, |world, harness| tap_key(world, harness, KeyCode::Space))
            .at(24, |world, harness| tap_key(world, harness, KeyCode::Space))
            .settle(36)
            .check(|world, harness| {
                let wait = wait_of(world);
                println!("  wait: {wait:?}");
                harness.report.check(
                    "the runner is parked on the choice wait",
                    matches!(wait_kind(world), Some(Wait::Choice { .. })),
                );
            }),
        Step::new("choice: F3 over the choice wait and a digit cannot pick it")
            .at(0, |world, harness| {
                open_drawer(world, harness);
                note_wait(world, harness);
            })
            .at(3, park_focus)
            .shot(4, "04-choice")
            .settle(7)
            .check(|world, harness| {
                check_panel(world, harness, "choice");
                check_hud_unchanged(world, harness);
            }),
        Step::new("choice: the digit under Monitor never chose an option")
            .at(0, |world, harness| tap_key(world, harness, KeyCode::Digit1))
            .settle(6)
            .check(check_recorded_wait),
        Step::new("choice: a fresh digit under no claim picks the option")
            .at(0, close_drawer)
            .at(5, |world, harness| {
                note_wait(world, harness);
                tap_key(world, harness, KeyCode::Digit1);
            })
            .settle(10)
            .check(|world, harness| {
                let now = wait_of(world);
                harness.report.check(
                    format!("the choice moved the runner on ({now:?})"),
                    now != harness.wait_before
                        && !matches!(wait_kind(world), Some(Wait::Choice { .. })),
                );
            }),
    ]
}
