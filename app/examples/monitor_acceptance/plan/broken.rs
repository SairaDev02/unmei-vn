//! The startup-error plan.

use crate::prelude::*;

pub(crate) fn broken_plan() -> Vec<Step> {
    vec![
        Step::new("broken: the startup error screen records the whole error list")
            .settle(14)
            .check(|world, harness| {
                let Some(failure) = world.get_resource::<StartupFailure>() else {
                    harness.report.fail("the startup failure is recorded");
                    return;
                };
                let StartupFailure::Pack { errors, .. } = failure else {
                    harness.report.fail("a broken pack is a pack failure");
                    return;
                };
                let lines = failure.screen_lines();
                println!("  errors: {}  screen lines: {}", errors.len(), lines.len());
                harness.report.check(
                    format!("the whole error list is preserved ({})", errors.len()),
                    errors.len() >= fixture::BROKEN_ERRORS,
                );
                harness.report.check(
                    format!("the screen caps at 20 lines plus the pointer ({})", lines.len()),
                    lines.len() == 21 && lines[20].contains("more"),
                );
                harness.report.check(
                    "no runtime was built",
                    world.get_resource::<Runtime>().is_none(),
                );
            }),
        Step::new("broken: F3 opens over the startup error without a Runtime")
            .at(0, open_drawer)
            .shot(6, "30-startup-error")
            .settle(12)
            .check(|world, harness| {
                let snap = crate::harness::snap(world);
                harness
                    .report
                    .check("the drawer is open", snap.drawer_open());
                check_panel(world, harness, "startup error");
                check_underlying_unchanged(world, harness);
                // The recorded overlap: the drawer covers the lower tail of the
                // error list at the reference size. The walk measures it and
                // proves the readable part is not silently redesigned.
                let Some(geometry) = snap.geometry() else {
                    harness.report.fail("the drawer panel is rendered");
                    return;
                };
                match error_list_bounds(&snap) {
                    Some((top, bottom)) => {
                        println!(
                            "  error list y {top:.1}..{bottom:.1}; drawer top {:.1}",
                            geometry.y
                        );
                        harness
                            .report
                            .check("the error list starts above the drawer", top < geometry.y);
                        harness.report.check(
                            "the drawer covers the error list's lower tail (the recorded overlap)",
                            bottom > geometry.y,
                        );
                    }
                    None => harness.report.fail("the error list is rendered"),
                }
                // The failure log carries the whole list.
                let root = world.resource::<RuntimeConfig>().pack_root.clone();
                let name = unmei_app::failure::session_log_file_name(&root);
                let log = root.join(format!("../{name}"));
                let log = log.canonicalize().unwrap_or(log);
                match std::fs::read_to_string(&log) {
                    Ok(text) => {
                        harness.report.check(
                            format!("{name} carries the failure rows"),
                            text.contains("nowhere_0") && text.contains("nowhere_24"),
                        );
                        let numbered = (0..25)
                            .filter(|index| text.contains(&format!("`nowhere_{index}`")))
                            .count();
                        harness.report.check(
                            format!("the failure log keeps all 25 numbered rows ({numbered})"),
                            numbered == 25,
                        );
                        harness.report.check(
                            "the failure log keeps the escaped row too",
                            text.contains(fixture::ESCAPED_BROKEN_MARKER),
                        );
                    }
                    Err(error) => harness
                        .report
                        .fail(format!("the session log is readable: {error}")),
                }
            }),
        Step::new("broken: the whole retained startup diagnostic is drawn in-window")
            .settle(2)
            .check(|world, harness| {
                read_visible_rows(world, harness);
                let joined = harness.read_bodies.join("\n");
                let numbered = (0..25)
                    .filter(|index| joined.contains(&format!("`nowhere_{index}`")))
                    .count();
                harness.report.check(
                    format!("all 25 numbered startup messages are drawn in-window ({numbered})"),
                    numbered == 25,
                );
                harness.report.check(
                    "the escaped startup target rendered escaped in-window",
                    joined.contains(r"nowhere_escaped_\u{202e}bidi\\nmarker")
                        && !joined.contains('\u{202e}'),
                );
                harness.report.check(
                    "the retained startup diagnostic is not truncated (it fits the 8,192-byte cap)",
                    !joined.contains(fixture::TRUNCATED_SUFFIX),
                );
                harness.report.check(
                    "the reader starts following the latest event",
                    crate::harness::snap(world)
                        .status()
                        .is_some_and(|status| status.contains("following latest")),
                );
            }),
        Step::new("broken: the log-body wheel leaves following with a reading anchor")
            .at(0, |world, _harness| wheel_over_body(world, 1.0))
            .settle(8)
            .check(|world, harness| {
                read_visible_rows(world, harness);
                let status = crate::harness::snap(world).status().unwrap_or_default();
                harness.report.check(
                    "the wheel switches the viewer to reading older lines",
                    status.contains("reading older lines"),
                );
                harness.report.check(
                    "the wheel leaves a reading anchor",
                    world
                        .resource::<unmei_app::drawer::MonitorPanel>()
                        .anchor()
                        .is_some(),
                );
            }),
        Step::new("broken: Home moves the reader to the oldest startup message")
            .at(0, |world, harness| tap_key(world, harness, KeyCode::Home))
            .until(projection_settled)
            .check(|world, harness| {
                read_visible_rows(world, harness);
                let status = crate::harness::snap(world).status().unwrap_or_default();
                harness.report.check(
                    "Home keeps the viewer reading older lines",
                    status.contains("reading older lines"),
                );
                let offset = world
                    .resource::<unmei_app::drawer::MonitorPanel>()
                    .anchor()
                    .map_or(f32::NAN, |anchor| anchor.offset_lines);
                harness.report.check(
                    format!("Home anchors at the oldest line (offset {offset:.1})"),
                    offset == 0.0,
                );
            }),
        Step::new("broken: PageDown pages on to the bottom of the retained diagnostic")
            .at(0, |world, harness| tap_key(world, harness, KeyCode::PageDown))
            .until(projection_settled)
            .check(|world, harness| {
                read_visible_rows(world, harness);
                let status = crate::harness::snap(world).status().unwrap_or_default();
                harness.report.check(
                    "paging to the bottom resumes following the latest event",
                    status.contains("following latest"),
                );
            }),
        Step::new("broken: End restores following the latest event")
            .at(0, |world, harness| tap_key(world, harness, KeyCode::End))
            .until(projection_settled)
            .check(|world, harness| {
                read_visible_rows(world, harness);
                let status = crate::harness::snap(world).status().unwrap_or_default();
                harness.report.check(
                    "End restores following the latest event",
                    status.contains("following latest"),
                );
            }),
        Step::new("broken: the whole capped list was read in-window across the reader controls")
            .settle(2)
            .check(|_world, harness| {
                let joined = harness.read_bodies.join("\n");
                for index in 0..25 {
                    harness.report.check(
                        format!("startup message nowhere_{index} was readable in-window"),
                        joined.contains(&format!("`nowhere_{index}`")),
                    );
                }
                harness.report.check(
                    "the escaped startup target stayed readable in-window",
                    joined.contains(r"nowhere_escaped_\u{202e}bidi\\nmarker"),
                );
            }),
        Step::new("broken: Enter under Monitor cannot quit the error screen")
            .at(0, |world, harness| tap_key(world, harness, KeyCode::Enter))
            .settle(10)
            .check(|world, harness| {
                let exited = world
                    .get_resource::<bevy::ecs::message::Messages<AppExit>>()
                    .is_some_and(|exits| !exits.is_empty());
                harness.report.check(
                    "Enter under Monitor did not quit the error screen",
                    !exited,
                );
            }),
        Step::new("broken: Esc dismisses Monitor and leaves the surface intact")
            .at(0, close_drawer)
            .settle(8)
            .check(|world, harness| {
                harness
                    .report
                    .check("Monitor closed", !crate::harness::snap(world).drawer_open());
                harness.report.check(
                    "the startup failure is still recorded",
                    world.get_resource::<StartupFailure>().is_some(),
                );
            }),
        Step::new("broken: let the screenshot pipeline drain")
            .settle(120)
            .check(|_world, harness| {
                harness.report.finish();
                println!(
                    "EXPECTED: this mode now lets the broken pack's own quit key exit the app with code 1."
                );
            }),
        Step::new("broken: fresh input restores the app's own non-zero error exit")
            .at(0, |world, harness| tap_key(world, harness, KeyCode::Enter))
            .settle(30),
        // Reached only if the fresh Enter did *not* quit: the run is wrong and
        // must not be read as a pass.
        Step::new("broken: the error screen never quit")
            .settle(2)
            .check(|_world, harness| {
                harness
                    .report
                    .fail("a fresh Enter did not restore the error screen's own quit");
            }),
    ]
}
