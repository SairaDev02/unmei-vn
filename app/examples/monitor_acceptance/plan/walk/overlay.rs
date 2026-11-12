//! The walk's decision-wait, overlay and Settings steps.

use crate::prelude::*;

pub(crate) fn overlay_steps() -> Vec<Step> {
    vec![
        // --- the decision wait -----------------------------------------------
        Step::new("decision: advance onto the decide point held by the gate")
            .at(0, |world, harness| tap_key(world, harness, KeyCode::Space))
            .at(8, |world, harness| tap_key(world, harness, KeyCode::Space))
            .at(16, |world, harness| tap_key(world, harness, KeyCode::Space))
            .settle(34)
            .check(|world, harness| {
                let wait = wait_of(world);
                println!("  wait: {wait:?}");
                harness.report.check(
                    "the runner is parked on the decision wait",
                    matches!(wait_kind(world), Some(Wait::Decision { .. })),
                );
            }),
        Step::new("decision: F3 over a live decision wait changes nothing")
            .at(0, |world, harness| {
                open_drawer(world, harness);
                note_wait(world, harness);
            })
            .at(3, park_focus)
            .shot(4, "05-decision-wait")
            .settle(7)
            .check(|world, harness| {
                check_panel(world, harness, "decision");
                check_hud_unchanged(world, harness);
            }),
        Step::new("decision: the request is untouched and closing does not answer it")
            .at(0, |world, harness| tap_key(world, harness, KeyCode::Space))
            .settle(6)
            .check(check_recorded_wait),
        Step::new("decision: the gated answer lands once Monitor is closed")
            .at(0, close_drawer)
            .at(5, |_world, harness| harness.gate.release("warm"))
            .settle(14)
            .check(|world, harness| {
                let wait = wait_of(world);
                println!("  after release: {wait:?}");
                harness.report.check(
                    format!("the released answer landed ({wait:?})"),
                    !matches!(wait_kind(world), Some(Wait::Decision { .. })),
                );
            }),
        // --- the overlay and Settings ---------------------------------------
        Step::new("menu: Esc opens the overlay, F3 opens Monitor over it")
            .at(0, |world, harness| tap_key(world, harness, KeyCode::Escape))
            .at(6, open_drawer)
            .shot(10, "06-overlay")
            .settle(12)
            .check(|world, harness| {
                let menu = world.resource::<MenuState>();
                harness.report.check(
                    "the overlay is up under the drawer",
                    menu.open && menu.view == MenuView::Slots,
                );
                check_panel(world, harness, "overlay");
                check_underlying_unchanged(world, harness);
            }),
        Step::new("menu: Esc under Monitor dismisses only the drawer")
            .at(0, close_drawer)
            .settle(6)
            .check(|world, harness| {
                harness
                    .report
                    .check("Monitor closed", !crate::harness::snap(world).drawer_open());
                let menu = world.resource::<MenuState>();
                harness.report.check(
                    "the overlay stayed open",
                    menu.open && menu.view == MenuView::Slots,
                );
            }),
        Step::new("settings: reach the view and open Monitor over it")
            .at(0, |world, _harness| {
                world.resource_mut::<MenuState>().focus = MenuFocus::SettingsButton;
            })
            .at(4, |world, harness| tap_key(world, harness, KeyCode::Enter))
            .at(12, open_drawer)
            .shot(18, "07-settings")
            .settle(20)
            .check(|world, harness| {
                let menu = world.resource::<MenuState>();
                harness.report.check(
                    "the Settings view is up under the drawer",
                    menu.open && menu.view == MenuView::Settings,
                );
                check_panel(world, harness, "settings");
                check_underlying_unchanged(world, harness);
            }),
        Step::new("settings: a slider drag under Monitor changes nothing")
            .at(0, |world, harness| {
                let snap = crate::harness::snap(world);
                let track = snap.nodes.iter().find(|node| {
                    node.shape.contains("w=Px(220.0)") && node.shape.contains("h=Px(10.0)")
                });
                match track {
                    Some(track) => {
                        harness.before_value = Some(
                            world
                                .resource::<unmei_app::settings::SettingsState>()
                                .settings
                                .audio
                                .master,
                        );
                        crate::harness::move_cursor(world, track.position + track.size / 2.0);
                        world.write_message(MouseButtonInput {
                            button: MouseButton::Left,
                            state: ButtonState::Pressed,
                            window: Entity::PLACEHOLDER,
                        });
                    }
                    None => harness
                        .report
                        .fail("a Settings slider track is rendered under the drawer"),
                }
            })
            .at(6, |world, _harness| {
                let snap = crate::harness::snap(world);
                if let Some(track) = snap.nodes.iter().find(|node| {
                    node.shape.contains("w=Px(220.0)") && node.shape.contains("h=Px(10.0)")
                }) {
                    crate::harness::move_cursor(
                        world,
                        Vec2::new(
                            track.position.x + track.size.x * 0.9,
                            track.position.y + 5.0,
                        ),
                    );
                }
            })
            .at(12, |world, _harness| {
                release_button(world, MouseButton::Left)
            })
            .settle(18)
            .check(|world, harness| {
                let after = world
                    .resource::<unmei_app::settings::SettingsState>()
                    .settings
                    .audio
                    .master;
                harness.report.check(
                    format!("the slider drag was inert under Monitor ({after})"),
                    Some(after) == harness.before_value,
                );
            }),
        Step::new("settings: the key row enters edit mode")
            .at(0, close_drawer)
            .at(5, |world, _harness| {
                world.resource_mut::<MenuState>().focus = MenuFocus::KeyRow;
            })
            .at(9, |world, harness| tap_key(world, harness, KeyCode::Enter))
            .settle(15)
            .check(|world, harness| {
                let menu = world.resource::<MenuState>();
                harness
                    .report
                    .check("the key row is editing", menu.key_editing);
            }),
        Step::new("settings: type a dummy draft through the real text path")
            .at(0, |world, harness| {
                for character in "sk-acceptance-dummy-0001".chars() {
                    crate::harness::typed(world, &character.to_string());
                    // The synthetic press must be released: a key left down
                    // would be quarantined as held across the Monitor
                    // transition and would block every later app-input path.
                    harness
                        .queued
                        .push(Queued::Key(crate::harness::key_for(character)));
                }
            })
            // Seed a nontrivial caret and selection through the real held-Shift
            // + arrow path, on frames where the modifier state has settled.
            .at(3, |world, _harness| {
                send_logical(world, Key::Shift, KeyCode::ShiftLeft, ButtonState::Pressed)
            })
            .at(5, |world, _harness| {
                for _ in 0..3 {
                    send_logical(
                        world,
                        Key::ArrowLeft,
                        KeyCode::ArrowLeft,
                        ButtonState::Pressed,
                    );
                    send_logical(
                        world,
                        Key::ArrowLeft,
                        KeyCode::ArrowLeft,
                        ButtonState::Released,
                    );
                }
            })
            .at(6, |world, _harness| {
                send_logical(world, Key::Shift, KeyCode::ShiftLeft, ButtonState::Released)
            })
            .at(7, |world, harness| {
                harness.draft_edit_before = draft_selection(world);
            })
            .settle(8)
            .check(|world, harness| {
                let draft = crate::harness::snap(world).draft().unwrap_or_default();
                println!("  draft: {draft:?}");
                harness.report.check(
                    "the draft is typed into the field",
                    draft == "sk-acceptance-dummy-0001",
                );
                println!("  draft edit: {:?}", harness.draft_edit_before);
                harness.report.check(
                    "a nontrivial caret and selection are seeded",
                    harness.draft_edit_before
                        == Some(("sk-acceptance-dummy-0001".to_string(), 21, 21..24)),
                );
            }),
        Step::new("settings: F3 suspends the draft without committing it")
            .at(0, |world, harness| {
                note_draft(world, harness);
                open_drawer(world, harness);
            })
            .shot(6, "08-settings-draft")
            .settle(10)
            .check(|world, harness| {
                check_panel(world, harness, "settings draft");
                check_underlying_unchanged(world, harness);
                harness.report.check(
                    "the draft, caret and selection are suspended unchanged",
                    draft_selection(world) == harness.draft_edit_before,
                );
            }),
        Step::new("settings: Enter under Monitor cannot commit the draft")
            .at(0, |world, harness| tap_key(world, harness, KeyCode::Enter))
            .settle(8)
            .check(|world, harness| {
                let snap = crate::harness::snap(world);
                harness.report.check(
                    "the draft is unchanged under Monitor",
                    snap.draft() == harness.draft_before,
                );
                let menu = world.resource::<MenuState>();
                harness
                    .report
                    .check("the field is still editing", menu.key_editing);
            }),
        Step::new("paste: a clipboard paste under Monitor never reaches the draft")
            .at(0, |world, harness| {
                *world.resource_mut::<unmei_app::credentials::ClipboardPort>() =
                    unmei_app::credentials::ClipboardPort::Memory(Some(
                        "sk-pasted-dummy-value".to_string(),
                    ));
                paste(world, harness);
            })
            .settle(10)
            .check(|world, harness| {
                harness.report.check(
                    "the paste under Monitor did not reach the draft",
                    crate::harness::snap(world).draft() == harness.draft_before,
                );
            }),
        Step::new("paste: a fresh paste after close is the ordinary path again")
            .at(0, |world, harness| {
                close_drawer(world, harness);
            })
            .at(8, |world, harness| {
                harness.draft_edit_restored = draft_selection(world);
            })
            // Collapse the restored selection to the line end so the fresh paste
            // appends, exactly as the pre-selection walk expected.
            .at(9, |world, _harness| {
                send_logical(world, Key::End, KeyCode::End, ButtonState::Pressed);
                send_logical(world, Key::End, KeyCode::End, ButtonState::Released);
            })
            .settle(8)
            .at(10, paste)
            .settle(18)
            .check(|world, harness| {
                let draft = crate::harness::snap(world).draft().unwrap_or_default();
                println!("  draft after paste: {draft:?}");
                harness.report.check(
                    "the caret and selection survived the close",
                    harness.draft_edit_restored == harness.draft_edit_before,
                );
                harness.report.check(
                    "the fresh paste applied after Monitor released the frame",
                    draft == "sk-acceptance-dummy-0001sk-pasted-dummy-value",
                );
            }),
        Step::new("settings: closing restores the draft unharmed")
            .settle(10)
            .check(|world, harness| {
                let menu = world.resource::<MenuState>();
                harness
                    .report
                    .check("the field is still editing after close", menu.key_editing);
                harness.report.check(
                    "the draft survived the close and the fresh paste",
                    crate::harness::snap(world).draft().unwrap_or_default()
                        == "sk-acceptance-dummy-0001sk-pasted-dummy-value",
                );
            }),
        Step::new("settings: a fresh Esc discards the suspended draft")
            .at(0, |world, harness| tap_key(world, harness, KeyCode::Escape))
            .settle(10)
            .check(|world, harness| {
                let menu = world.resource::<MenuState>();
                harness
                    .report
                    .check("Esc left edit mode", !menu.key_editing);
                harness.report.check(
                    "Esc discarded the draft",
                    crate::harness::snap(world)
                        .draft()
                        .unwrap_or_default()
                        .is_empty(),
                );
            }),
        Step::new("settings: the environment row renders its read-only state")
            .at(0, |world, _harness| {
                world.resource_mut::<CredentialsState>().row = KeyRow::FromEnvironment {
                    fingerprint: "sk-acc\u{2026}0001".to_string(),
                };
            })
            .at(5, open_drawer)
            .shot(12, "09-settings-environment")
            .settle(18)
            .check(|world, harness| {
                harness.report.check(
                    "the environment row renders its read-only state",
                    rendered_surface_text(world).contains("From environment"),
                );
                check_panel(world, harness, "settings environment");
            }),
        Step::new("settings: the Checking row renders its in-flight state")
            .at(0, close_drawer)
            .at(5, |world, _harness| {
                world.resource_mut::<CredentialsState>().row = KeyRow::Checking;
            })
            .at(10, open_drawer)
            .shot(18, "10-settings-checking")
            .settle(24)
            .check(|world, harness| {
                harness.report.check(
                    "the Checking row renders its in-flight state",
                    rendered_surface_text(world).contains("Checking\u{2026}"),
                );
                check_panel(world, harness, "settings checking");
            }),
    ]
}
