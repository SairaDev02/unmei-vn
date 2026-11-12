//! The probe: drive the real drawer and measure every reference number.

use crate::prelude::*;

#[allow(clippy::too_many_arguments)]
/// Drive a handful of real frames: press F3 through `KeyboardInput`, measure,
/// click a control through the real cursor path, then walk the older-line
/// reader with a real wheel, page keys, a filter change and a scrollbar drag,
/// widen to the fullscreen reference and prove the same event stayed anchored.
///
/// Each paging key is held for exactly one frame and its page is measured one
/// frame later, so the held-key cadence has no frame in which it could add a
/// page, and the drawn rows have settled before anything is read. The walk then
/// admits a real arrival under the held anchor and widens to the fullscreen
/// reference, proving the same event stays the drawn top-visible row.
pub(crate) fn probe(
    mut state: Local<Probe>,
    mut exit: MessageWriter<AppExit>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    nodes: Query<(Entity, &Node, &ComputedNode, &UiGlobalTransform)>,
    texts: Query<(Entity, &Text)>,
    layouts: Query<(Entity, &TextLayoutInfo)>,
    children: Query<&Children>,
    roots: Query<(Entity, &GlobalZIndex)>,
    backgrounds: Query<&BackgroundColor>,
    rows: Query<&MonitorRow>,
    panel: Res<MonitorPanel>,
    history: Option<Res<MonitorHistory>>,
    mut presses: MessageWriter<KeyboardInput>,
    mut clicks: MessageWriter<MouseButtonInput>,
    mut wheels: MessageWriter<MouseWheel>,
    mut settings: ResMut<unmei_app::settings::SettingsState>,
) {
    state.frame += 1;
    let delay = frame_delay();
    if !delay.is_zero() {
        std::thread::sleep(delay);
    }
    match state.frame {
        // Let the pack load, the title build and the renderer warm up. Emit real
        // diagnostics into the dev capture — numbered events first so the oldest
        // anchored event is a normal grid row, then the bidi short fixture, the
        // long wrapped body and the long target — snapshot the HUD, then open
        // Monitor.
        5 => {
            emit_smoke_events();
            state.hud_before = Some(snapshot_hud(&nodes));
            presses.write(KeyboardInput {
                key_code: KeyCode::F3,
                logical_key: Key::Unidentified(NativeKey::Unidentified),
                state: ButtonState::Pressed,
                text: None,
                repeat: false,
                window: Entity::PLACEHOLDER,
            });
        }
        6 => {
            presses.write(KeyboardInput {
                key_code: KeyCode::F3,
                logical_key: Key::Unidentified(NativeKey::Unidentified),
                state: ButtonState::Released,
                text: None,
                repeat: false,
                window: Entity::PLACEHOLDER,
            });
        }
        // Layout, text metrics and the first rows have settled.
        30 => {
            report(
                "1280x720",
                &windows,
                &nodes,
                &texts,
                &layouts,
                &children,
                &roots,
                &backgrounds,
            );
            let hud_after = snapshot_hud(&nodes);
            compare_hud(
                state
                    .hud_before
                    .as_ref()
                    .expect("the pre-open HUD snapshot"),
                &hud_after,
            );
            // Click Show everything through a real cursor position and a real
            // mouse button; `ui_focus_system` hit-tests it.
            let (control, center) = show_everything_control(&nodes, &texts)
                .expect("the Show everything control is rendered");
            state.clicked_control = Some(control);
            if let Some(mut window) = windows.iter_mut().next() {
                window.set_physical_cursor_position(Some(center.as_dvec2()));
            }
            clicks.write(MouseButtonInput {
                button: MouseButton::Left,
                state: ButtonState::Pressed,
                window: Entity::PLACEHOLDER,
            });
        }
        32 => {
            clicks.write(MouseButtonInput {
                button: MouseButton::Left,
                state: ButtonState::Released,
                window: Entity::PLACEHOLDER,
            });
            let control = state.clicked_control.expect("the clicked control");
            let label = texts
                .iter()
                .find(|(entity, _)| *entity == control)
                .map(|(_, text)| text.as_str().to_string())
                .expect("the control still renders its label");
            if label != "[x] Show everything" {
                eprintln!(
                    "FAILED: the real pointer click did not toggle the filter; label is {label:?}"
                );
                std::process::exit(1);
            }
            println!("pointer click: {label}");
            // Start the reader walk: the wheel is hit-tested to the log body, so
            // park the cursor at its centre and roll it older.
            let (body, position, size) =
                drawer_body(&roots, &children, &nodes).expect("the log body is rendered");
            if let Some(mut window) = windows.iter_mut().next() {
                window.set_physical_cursor_position(Some((position + size / 2.0).as_dvec2()));
            }
            wheels.write(MouseWheel {
                unit: MouseScrollUnit::Line,
                x: 0.0,
                y: 9.0,
                window: Entity::PLACEHOLDER,
                phase: TouchPhase::Moved,
            });
            println!("reader: wheel over the log body at {body:?}");
        }
        // The wheel left following and anchored the top visible event.
        34 => {
            if reader_following(&texts) {
                eprintln!("FAILED: the log-body wheel did not leave following");
                std::process::exit(1);
            }
            let anchor = panel.anchor().expect("the wheel left a stable anchor");
            let (_, position, _) =
                drawer_body(&roots, &children, &nodes).expect("the log body is rendered");
            let row = top_visible_row(&nodes, &texts, &children, position.y)
                .expect("an anchored visible row");
            check_drawn_anchor("wheel anchor", &row, anchor, &rows);
            check_anchor_top("wheel anchor", row.top, row.height, position.y);
            state.anchor = Some(anchor.sequence);
            println!("reader: wheel anchored {anchor:?} ({:?})", row.label());
            // A filter change while reading a matching event must keep it.
            let (control, center) = show_everything_control(&nodes, &texts)
                .expect("the Show everything control is rendered");
            state.clicked_control = Some(control);
            if let Some(mut window) = windows.iter_mut().next() {
                window.set_physical_cursor_position(Some(center.as_dvec2()));
            }
            clicks.write(MouseButtonInput {
                button: MouseButton::Left,
                state: ButtonState::Pressed,
                window: Entity::PLACEHOLDER,
            });
        }
        36 => {
            clicks.write(MouseButtonInput {
                button: MouseButton::Left,
                state: ButtonState::Released,
                window: Entity::PLACEHOLDER,
            });
            // The filter refresh settled: following stays off and the matching
            // event is kept.
            if reader_following(&texts) {
                eprintln!("FAILED: the filter change silently enabled follow");
                std::process::exit(1);
            }
            let anchor = panel.anchor().expect("the reader keeps its anchor");
            let (_, position, _) =
                drawer_body(&roots, &children, &nodes).expect("the log body is rendered");
            let row = top_visible_row(&nodes, &texts, &children, position.y)
                .expect("a refreshed visible row");
            if Some(anchor.sequence) != state.anchor {
                eprintln!(
                    "FAILED: the filter refresh moved the matching anchor to {anchor:?} ({:?}), recorded sequence {:?}",
                    row.label(),
                    state.anchor
                );
                std::process::exit(1);
            }
            println!("reader: filter refresh kept the matching anchor");
            check_drawn_anchor("refreshed anchor", &row, anchor, &rows);
            // A fresh PgUp pages a full viewport older. The key is released the
            // frame after the press, so the held-key cadence has no frame in
            // which it could add an unaccounted page.
            presses.write(KeyboardInput {
                key_code: KeyCode::PageUp,
                logical_key: Key::Unidentified(NativeKey::Unidentified),
                state: ButtonState::Pressed,
                text: None,
                repeat: false,
                window: Entity::PLACEHOLDER,
            });
        }
        37 => {
            presses.write(KeyboardInput {
                key_code: KeyCode::PageUp,
                logical_key: Key::Unidentified(NativeKey::Unidentified),
                state: ButtonState::Released,
                text: None,
                repeat: false,
                window: Entity::PLACEHOLDER,
            });
        }
        // The page applied last frame and its rows are materialized, so the
        // reader's anchor and the drawn top-visible row can be compared.
        38 => {
            let anchor = panel.anchor().expect("the page left a stable anchor");
            if Some(anchor.sequence) == state.anchor {
                eprintln!("FAILED: PgUp did not move the reading position");
                std::process::exit(1);
            }
            let (_, position, _) =
                drawer_body(&roots, &children, &nodes).expect("the log body is rendered");
            let row = top_visible_row(&nodes, &texts, &children, position.y)
                .expect("a paged visible row");
            check_drawn_anchor("paged anchor", &row, anchor, &rows);
            check_anchor_top("paged anchor", row.top, row.height, position.y);
            state.anchor = Some(anchor.sequence);
            println!("reader: PgUp paged to {anchor:?} ({:?})", row.label());
        }
        // Drag the real scrollbar thumb to the bottom of its track.
        39 => {
            let track = scrollbar_track(&roots, &children, &nodes)
                .expect("the scrollbar track is rendered");
            let (track_position, track_size) = place(&nodes, track);
            if let Some(mut window) = windows.iter_mut().next() {
                window.set_physical_cursor_position(Some(
                    (track_position + track_size / 2.0).as_dvec2(),
                ));
            }
            clicks.write(MouseButtonInput {
                button: MouseButton::Left,
                state: ButtonState::Pressed,
                window: Entity::PLACEHOLDER,
            });
        }
        41 => {
            // Slide the dragged thumb to the bottom of the track.
            let track = scrollbar_track(&roots, &children, &nodes)
                .expect("the scrollbar track is rendered");
            let (track_position, track_size) = place(&nodes, track);
            if let Some(mut window) = windows.iter_mut().next() {
                window.set_physical_cursor_position(Some(
                    (track_position + Vec2::new(track_size.x / 2.0, track_size.y)).as_dvec2(),
                ));
            }
        }
        42 => {
            clicks.write(MouseButtonInput {
                button: MouseButton::Left,
                state: ButtonState::Released,
                window: Entity::PLACEHOLDER,
            });
        }
        // The bottom of the travel is a deliberate bottom arrival.
        44 => {
            if !reader_following(&texts) {
                eprintln!("FAILED: the scrollbar drag to the bottom did not resume following");
                std::process::exit(1);
            }
            println!("reader: scrollbar drag to the bottom resumed following");
            // Page older to a mid-row anchor, then widen to the fullscreen
            // reference and prove that same event stays on top through rewrap.
            // The key is released the next frame, so this is exactly one page.
            presses.write(KeyboardInput {
                key_code: KeyCode::PageUp,
                logical_key: Key::Unidentified(NativeKey::Unidentified),
                state: ButtonState::Pressed,
                text: None,
                repeat: false,
                window: Entity::PLACEHOLDER,
            });
        }
        45 => {
            presses.write(KeyboardInput {
                key_code: KeyCode::PageUp,
                logical_key: Key::Unidentified(NativeKey::Unidentified),
                state: ButtonState::Released,
                text: None,
                repeat: false,
                window: Entity::PLACEHOLDER,
            });
        }
        // The page applied last frame and its rows are materialized: record the
        // reader's own anchored event, prove the drawn top-visible row is that
        // event, then widen to the fullscreen reference.
        46 => {
            let anchor = panel
                .anchor()
                .expect("the page left the reader a stable anchor");
            let (_, position, _) =
                drawer_body(&roots, &children, &nodes).expect("the log body is rendered");
            let row = top_visible_row(&nodes, &texts, &children, position.y)
                .expect("a paged visible row");
            check_drawn_anchor("paged anchor", &row, anchor, &rows);
            check_anchor_top("paged anchor", row.top, row.height, position.y);
            state.anchor = Some(anchor.sequence);
            println!(
                "reader: paged to a mid-row anchor {anchor:?} ({:?})",
                row.label()
            );
            // Go borderless fullscreen through the **shipping Settings toggle**,
            // so the mode the app's own `sync_window_mode` derives is the one
            // measured. Poking `window.mode` directly does not survive: the
            // settings sync puts the stored `play.fullscreen` value back on the
            // window every frame, and only the resolution change used to stick.
            settings.settings.play.fullscreen = true;
        }
        // A real arrival under the held anchor: the retained row set changes
        // while the reader is anchored, which is the state once suspected of
        // moving the reader's anchor. The arrival must neither move the reader
        // nor take the top of the display.
        47 => {
            log::info!(target: "unmei::smoke::seq", "{ARRIVAL_MESSAGE}");
        }
        48 => {
            let anchor = panel
                .anchor()
                .expect("the arrival left the reader's anchor alone");
            if Some(anchor.sequence) != state.anchor {
                eprintln!(
                    "FAILED: an arrival under the anchor moved the reader to {anchor:?}, recorded sequence {:?}",
                    state.anchor
                );
                std::process::exit(1);
            }
            let (_, position, _) =
                drawer_body(&roots, &children, &nodes).expect("the log body is rendered");
            let row = top_visible_row(&nodes, &texts, &children, position.y)
                .expect("a visible row after the arrival");
            check_drawn_anchor("arrival anchor", &row, anchor, &rows);
            println!("reader: an arrival kept the anchored event on top");
        }
        // A frame later the arrival's admission is observable, so the step above
        // is not vacuous: the row set really changed under the held anchor.
        49 => {
            let retained = history
                .as_deref()
                .map(MonitorHistory::retained)
                .unwrap_or(0);
            let admitted = history.as_deref().is_some_and(|history| {
                history
                    .lines()
                    .iter()
                    .any(|line| line.body().contains(ARRIVAL_MESSAGE))
            });
            if !admitted {
                eprintln!(
                    "FAILED: the arrival under the held anchor never reached the retained history"
                );
                std::process::exit(1);
            }
            println!(
                "reader: the arrival was admitted (retained {retained}) under the held anchor"
            );
        }
        // The wider frame has reflowed: the same event is still the reader's
        // anchor and still the drawn top-visible row, the fixed-px chrome is
        // unchanged and the HUD never moved.
        55 => {
            if reader_following(&texts) {
                eprintln!("FAILED: widening silently resumed following");
                std::process::exit(1);
            }
            let anchor = panel
                .anchor()
                .expect("the reader still holds its anchored event");
            let (_, position, _) =
                drawer_body(&roots, &children, &nodes).expect("the log body is rendered");
            let row = top_visible_row(&nodes, &texts, &children, position.y)
                .expect("a rewrapped visible row");
            if Some(anchor.sequence) != state.anchor {
                eprintln!(
                    "FAILED: the fullscreen rewrap moved the anchor to {anchor:?} ({:?}), recorded sequence {:?}",
                    row.label(),
                    state.anchor
                );
                std::process::exit(1);
            }
            check_drawn_anchor("rewrapped anchor", &row, anchor, &rows);
            check_anchor_top("rewrapped anchor", row.top, row.height, position.y);
            check_reader_status(&texts);
            println!(
                "reader: the same event stayed anchored across the fullscreen rewrap ({:?})",
                row.label()
            );
            // Return to the newest events so the fixture rows are on screen for
            // the wide-frame geometry check.
            presses.write(KeyboardInput {
                key_code: KeyCode::End,
                logical_key: Key::Unidentified(NativeKey::Unidentified),
                state: ButtonState::Pressed,
                text: None,
                repeat: false,
                window: Entity::PLACEHOLDER,
            });
        }
        56 => {
            presses.write(KeyboardInput {
                key_code: KeyCode::End,
                logical_key: Key::Unidentified(NativeKey::Unidentified),
                state: ButtonState::Released,
                text: None,
                repeat: false,
                window: Entity::PLACEHOLDER,
            });
        }
        60 => {
            if !reader_following(&texts) {
                eprintln!("FAILED: End did not resume following after the rewrap check");
                std::process::exit(1);
            }
            report(
                "fullscreen",
                &windows,
                &nodes,
                &texts,
                &layouts,
                &children,
                &roots,
                &backgrounds,
            );
            exit.write(AppExit::Success);
        }
        _ => {}
    }
}
