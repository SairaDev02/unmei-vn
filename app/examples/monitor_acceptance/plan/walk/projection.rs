//! The walk's boot, filter-fallback and overflow steps.

use crate::prelude::*;

pub(crate) fn projection_steps() -> Vec<Step> {
    vec![
        // --- boot and the empty projection ---------------------------------
        Step::new("boot: the title is up and Monitor is closed")
            .settle(12)
            .check(|world, harness| {
                check_title_up(world, harness);
                report_platform(world, harness);
            }),
        // A same-frame F3 opening plus a real pointer press on an underlying
        // control: the claim covers the whole frame, so the pointer's own
        // hit-tested activation cannot start play. The check reads the widget's
        // own `Interaction` — the production hit-test result — to prove the
        // press actually reached the row while the menu action was suppressed.
        Step::new("isolation: same-frame F3 and a real New Game press cannot start play")
            .at(0, |world, harness| {
                let snap = crate::harness::snap(world);
                match control_center(&snap, "New Game") {
                    Some(centre) => {
                        println!("  New Game row centre: {centre:?}");
                        crate::harness::move_cursor(world, centre);
                    }
                    None => harness
                        .report
                        .fail("the New Game row is rendered and hittable"),
                }
            })
            // The cursor has settled onto the row, so the press in this frame is
            // the real hit-tested press the widget receives.
            .at(2, |world, harness| {
                open_drawer(world, harness);
                world.write_message(MouseButtonInput {
                    button: MouseButton::Left,
                    state: ButtonState::Pressed,
                    window: Entity::PLACEHOLDER,
                });
                harness.queued.push(Queued::Button(MouseButton::Left));
            })
            // Record the widget's own interaction on the frame the press lands.
            .at(3, |world, harness| {
                harness.pointer_hit =
                    Some(new_game_interaction(world) == Some(Interaction::Pressed));
            })
            .at(8, close_drawer)
            .settle(12)
            .check(|world, harness| {
                let title_up = {
                    let menu = world.resource::<MenuState>();
                    menu.open && menu.view == MenuView::Title
                };
                let hit = harness.pointer_hit == Some(true);
                println!("  New Game press hit-test: {hit}");
                harness.report.check(
                    "Monitor opened and closed over the title",
                    !crate::harness::snap(world).drawer_open(),
                );
                harness.report.check(
                    "the same-frame pointer press did not start a New Game",
                    title_up,
                );
                harness.report.check(
                    "the pointer's real hit-test reached the New Game widget",
                    hit,
                );
                check_underlying_unchanged(world, harness);
            }),
        Step::new("empty: F3 opens on the title with an empty unmei:: view")
            .at(0, open_drawer)
            .shot(4, "01-empty-state")
            .settle(6)
            .check(|world, harness| {
                let snap = crate::harness::snap(world);
                harness
                    .report
                    .check("the drawer is open", snap.drawer_open());
                match snap.empty_label() {
                    Some(label) => harness
                        .report
                        .pass(format!("the empty state renders {label:?}")),
                    None => harness
                        .report
                        .fail("the empty projection renders its local state line"),
                }
                check_panel(world, harness, "empty");
                check_underlying_unchanged(world, harness);
            }),
        Step::new("empty: Esc closes and the title keeps its own keys")
            .at(0, close_drawer)
            .settle(5)
            .check(|world, harness| {
                harness
                    .report
                    .check("Monitor closed", !crate::harness::snap(world).drawer_open());
                let menu = world.resource::<MenuState>();
                harness.report.check(
                    "the title is still up",
                    menu.open && menu.view == MenuView::Title,
                );
            }),
        // --- the small-history filter fallbacks -----------------------------
        Step::new("filter: emit a small history with the drawer open")
            .at(0, |world, harness| {
                open_drawer(world, harness);
                log::info!(target: "bevy_acceptance_early", "a non-unmei event before the first matching fixture");
                harness.emit_to = 200;
            })
            .tick(drain_emissions)
            .settle(16)
            .check(|world, harness| {
                let (status, snap) = status_of(world);
                let Some(status) = status else {
                    harness.report.fail("the status footer renders");
                    return;
                };
                println!("  status: {}", status.text);
                harness.report.check(
                    format!(
                        "the small history fits under the cap without eviction ({})",
                        status.retained
                    ),
                    status.retained >= 200 && status.evicted == 0,
                );
                let rows = rendered_row_marks(&snap);
                harness.rows_before_flip = rows;
                harness
                    .report
                    .check(format!("the drawer rendered rows ({})", harness.rows_before_flip.len()), !harness.rows_before_flip.is_empty());
            }),
        Step::new("filter: Show everything settles over the small history")
            .at(0, |world, harness| tap_control(world, harness, "Show everything"))
            .until(projection_settled)
            .settle(16)
            .check(|world, harness| {
                let (status, _) = status_of(world);
                let Some(status) = status else {
                    harness.report.fail("the status footer renders");
                    return;
                };
                println!("  status: {}", status.text);
                harness.report.check(
                    format!("the widened view projects the whole history ({})", status.shown),
                    status.shown == status.retained && !status.updating,
                );
            }),
        Step::new("filter: Home anchors on the oldest retained non-matching row")
            .at(0, |world, harness| tap_key(world, harness, KeyCode::Home))
            .settle(10)
            .check(|world, harness| {
                let (status, snap) = status_of(world);
                if let Some(status) = status.as_ref() {
                    println!("  status: {}", status.text);
                }
                let rows = rendered_row_marks(&snap);
                harness.anchor_index = snap
                    .top_full_row()
                    .and_then(|row| rows.iter().position(|mark| mark.body == row.body(&snap)));
                harness.rows_before_flip = rows;
                let top = snap.top_visible_row();
                let target = top
                    .as_ref()
                    .map_or_else(String::new, |row| row.target(&snap));
                println!(
                    "  anchor row: {target:?} (index {:?})",
                    harness.anchor_index
                );
                harness.report.check(
                    "Home landed on a rendered row",
                    harness.anchor_index.is_some(),
                );
                harness.report.check(
                    "Home left following without expiring",
                    status.is_some_and(|status| !status.following && !status.expired),
                );
            }),
        Step::new("filter: the flip falls forward to the first newer match")
            .at(0, |world, harness| tap_control(world, harness, "Show everything"))
            .until(projection_settled)
            .settle(16)
            .check(|world, harness| {
                let (status, snap) = status_of(world);
                let top = snap.top_visible_row().expect("a top visible row");
                let (target, body) = (top.target(&snap), top.body(&snap));
                println!("  after switch: {target:?} / {body:?}");
                let expected = expected_fallback(harness);
                println!("  fallback rule: {}", expected.rule);
                harness.report.check(
                    format!("the {} fallback landed on the documented row", expected.rule),
                    target == expected.target && body == expected.body,
                );
                harness.report.check(
                    "the small-history anchor exercises the first-newer rule",
                    expected.rule == "first-newer",
                );
                harness.report.check(
                    "the fallback kept follow off",
                    status.is_some_and(|status| !status.following),
                );
            }),
        Step::new("filter: the matching anchor survives the view flip back")
            .at(0, |world, harness| tap_control(world, harness, "Show everything"))
            .until(projection_settled)
            .settle(16)
            .check(|world, harness| {
                let (_, snap) = status_of(world);
                let top = snap.top_visible_row().expect("a top visible row");
                let target = top.target(&snap);
                let body = top.body(&snap);
                println!("  after flip back: {target:?} / {body:?}");
                harness.report.check(
                    "the matching anchor is the same event under Show everything",
                    target.starts_with("unmei::") && body.contains("sequence event 0000"),
                );
            }),
        // --- the 1,000-line overflow with marked progressive refresh ---------
        Step::new("overflow: close the drawer and accumulate a 1,000-line backlog")
            .at(0, close_drawer)
            .at(6, |_world, harness| {
                harness.emit_to = 999;
            })
            .tick(drain_emissions)
            .settle(22)
            .check(|world, harness| {
                harness.report.check(
                    "Monitor stayed closed while the backlog accumulated",
                    !crate::harness::snap(world).drawer_open(),
                );
                let retained = world
                    .get_resource::<unmei_app::monitor::MonitorHistory>()
                    .map(|history| history.retained())
                    .unwrap_or_default();
                harness.report.check(
                    format!("the hidden capture retained the whole cap ({retained})"),
                    retained == 1000,
                );
            }),
        Step::new("overflow: opening over the backlog marks the bounded refresh")
            .at(0, open_drawer)
            .tick(record_updating)
            .until(projection_settled)
            .settle(20)
            .check(|world, harness| {
                let (status, snap) = status_of(world);
                let Some(status) = status else {
                    harness.report.fail("the status footer renders");
                    return;
                };
                println!("  status: {}", status.text);
                harness.report.check(
                    format!("retained history is capped at 1,000 ({})", status.retained),
                    status.retained == 1000,
                );
                harness.report.check(
                    format!("no event was lost under load ({})", status.lost),
                    status.lost == 0,
                );
                harness.report.check(
                    format!("the progressive refresh was marked ({})", harness.saw_updating),
                    harness.saw_updating,
                );
                harness.report.check(
                    format!("the projection settles on the cap ({})", status.shown),
                    status.shown >= 999 && !status.updating,
                );
                // The reader was left reading older lines before the backlog:
                // its surviving anchor must still be a rendered matching row
                // (or the state must be following / honestly expired).
                let anchored_truthfully = if status.following || status.expired {
                    true
                } else {
                    snap.top_visible_row()
                        .is_some_and(|row| row.target(&snap).starts_with("unmei::"))
                };
                harness
                    .report
                    .check("the reader state stays truthful over the backlog", anchored_truthfully);
                if let Some((thumb, _, track)) = snap.thumb() {
                    harness.report.check(
                        format!("the scrollbar thumb shows the visible share ({thumb:.1}/{track:.1})"),
                        thumb > 0.0 && thumb < track * 0.2,
                    );
                } else {
                    harness.report.fail("the scrollbar thumb is rendered");
                }
                check_panel(world, harness, "overflow");
            }),
    ]
}
