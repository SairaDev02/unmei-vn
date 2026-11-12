//! The walk's reading-fallback and retained-tail style steps.

use crate::prelude::*;

pub(crate) fn history_steps() -> Vec<Step> {
    vec![
        // --- nearest-earlier fallback ----------------------------------------
        Step::new("anchor: a tall middle pair and 1,000 more events")
            .at(0, |world, harness| {
                // The widened view is up from the flip-back check.
                let _ = world;
                log::info!(target: "unmei::acceptance::seq", "sequence event 0999");
                log::info!(
                    target: "bevy_acceptance_middle",
                    "{}",
                    "A deliberately tall middle fixture that spans several estimated display \
                     lines so one wheel notch from the oldest retained row lands inside it, \
                     giving the filter fallback a preceding match to find. "
                        .repeat(4)
                );
                log::info!(target: "bevy_acceptance_middle_b", "the middle pair's second row");
                harness.emit_to = harness.emit_from + 997;
            })
            .tick(drain_and_record)
            .settle(24)
            .check(|world, harness| {
                let (status, _) = status_of(world);
                let Some(status) = status else {
                    harness.report.fail("the status footer renders");
                    return;
                };
                println!("  status: {}", status.text);
                harness.report.check(
                    format!("the middle pair is retained ({})", status.retained),
                    status.retained == 1000,
                );
            }),
        Step::new("anchor: Home in Show everything, then page onto the middle pair")
            .at(0, |world, harness| {
                let snap = crate::harness::snap(world);
                let unmei_only = snap
                    .status()
                    .is_some_and(|status| status.contains("showing unmei:: only"));
                drop(snap);
                if unmei_only {
                    tap_control(world, harness, "Show everything");
                }
            })
            .at(8, |world, harness| tap_key(world, harness, KeyCode::Home))
            .at(16, |world, _harness| wheel_over_body(world, -1.0))
            .settle(24)
            .check(|world, harness| {
                let (_, snap) = status_of(world);
                let rows = rendered_row_marks(&snap);
                let top = snap.top_visible_row().expect("a top visible row");
                let top_mark = RowMark {
                    target: top.target(&snap),
                    body: top.body(&snap),
                };
                let index = rows.iter().position(|mark| *mark == top_mark);
                let target = top_mark.target.clone();
                println!("  paged onto: {target:?} / {:?}", top_mark.body);
                harness.rows_before_flip = rows;
                harness.anchor_index = index;
                // The anchor sits in the middle pair; the both-sides property
                // is read from the retained history, not the rendered window
                // (which starts at the anchor). The fallback step then verifies
                // the newer-first identity against the rendered rows.
                harness.report.check(
                    "the paged anchor is a non-unmei event with matches on both sides",
                    !target.starts_with("unmei::")
                        && retained_matches_bracket_the_middle_pair(world),
                );
            }),
        Step::new("anchor: the both-sides filter falls back to the first newer match")
            .at(0, |world, harness| tap_control(world, harness, "Show everything"))
            .settle(20)
            .check(|world, harness| {
                let (status, snap) = status_of(world);
                let top = snap.top_visible_row().expect("a top visible row");
                let target = top.target(&snap);
                let body = top.body(&snap);
                println!("  after switch: {target:?} / {body:?}");
                let expected = expected_fallback(harness);
                println!("  fallback rule: {}", expected.rule);
                harness.report.check(
                    format!("the {} fallback landed on the documented row", expected.rule),
                    target == expected.target && body == expected.body,
                );
                harness.report.check(
                    "the both-sides anchor exercises the newer-first rule",
                    expected.rule == "first-newer",
                );
                if let Some(status) = status {
                    harness
                        .report
                        .check("the fallback kept follow off", !status.following);
                }
            }),
        // --- the earlier-only fallback (newer-first cannot apply) --------------
        Step::new("anchor: a non-matching tail with only earlier matches")
            .at(0, |world, harness| {
                log::info!(target: "unmei::acceptance::seq", "the earlier-only match marker");
                for index in 0..20 {
                    log::info!(target: "bevy_acceptance_tail", "tail noise {index}");
                }
                let snap = crate::harness::snap(world);
                let unmei_only = snap
                    .status()
                    .is_some_and(|status| status.contains("showing unmei:: only"));
                drop(snap);
                if unmei_only {
                    tap_control(world, harness, "Show everything");
                }
            })
            .settle(20)
            .at(0, |world, harness| tap_key(world, harness, KeyCode::End))
            .at(8, |world, _harness| wheel_over_body(world, 1.0))
            .settle(20)
            .check(|world, harness| {
                let (_, snap) = status_of(world);
                let top = snap.top_visible_row().expect("a top visible row");
                let target = top.target(&snap);
                println!("  earlier-only anchor: {target:?} / {:?}", top.body(&snap));
                harness.report.check(
                    "the earlier-only anchor is a non-unmei tail event",
                    !target.starts_with("unmei::"),
                );
                harness.report.check(
                    "no matching event survives newer than the earlier-only anchor",
                    !rendered_row_marks(&snap)
                        .iter()
                        .any(|mark| mark.target.starts_with("unmei::"))
                        && retained_has_no_match_after_the_marker(world),
                );
            }),
        Step::new("anchor: the earlier-only case falls back to the nearest earlier match")
            .at(0, |world, harness| tap_control(world, harness, "Show everything"))
            .settle(20)
            .check(|world, harness| {
                let (status, snap) = status_of(world);
                let top = snap.top_visible_row().expect("a top visible row");
                let target = top.target(&snap);
                let body = top.body(&snap);
                println!("  after switch: {target:?} / {body:?}");
                harness.report.check(
                    "the anchor fell back to the nearest earlier matching event",
                    target.starts_with("unmei::")
                        && body.contains("the earlier-only match marker"),
                );
                if let Some(status) = status {
                    harness
                        .report
                        .check("the fallback kept follow off", !status.following);
                }
            }),
        Step::new("expiry: Home, then continued arrivals evict the anchor")
            .at(0, |world, harness| {
                tap_key(world, harness, KeyCode::Home);
                harness.emit_to = harness.emit_from + 120;
            })
            .tick(drain_and_record)
            .settle(26)
            .check(|world, harness| {
                let (status, snap) = status_of(world);
                let Some(status) = status else {
                    harness.report.fail("the status footer renders");
                    return;
                };
                println!("  status: {}", status.text);
                harness.report.check(
                    "an evicted anchor is acknowledged and follow stays off",
                    status.expired && !status.following,
                );
                harness.report.check(
                    format!("retention kept evicting oldest-first ({})", status.evicted),
                    status.evicted >= 1000,
                );
                let top = snap.top_visible_row().expect("a top visible row");
                harness.report.check(
                    "the clamped anchor is a surviving matching event",
                    top.target(&snap).starts_with("unmei::"),
                );
            }),
        Step::new("expiry: deliberate End resumes following and clears the acknowledgement")
            .at(0, |world, harness| tap_key(world, harness, KeyCode::End))
            .settle(8)
            .check(|world, harness| {
                let (status, _) = status_of(world);
                let Some(status) = status else {
                    harness.report.fail("the status footer renders");
                    return;
                };
                harness.report.check("End resumed following", status.following);
                harness
                    .report
                    .check("End cleared the expiry acknowledgement", !status.expired);
            }),
        // --- the style fixtures at the retained tail ------------------------
        Step::new("styles: tone, escape, Unicode and truncation fixtures arrive")
            .at(0, |_world, harness| {
                fixture::emit_styles();
                log::info!(target: "bevy_acceptance_newest", "the newest retained event is filtered out of the default view");
                harness.emit_to = harness.emit_from;
            })
            .settle(12)
            .check(|world, harness| {
                check_style_fixtures(world, harness);
                let (status, snap) = status_of(world);
                let Some(status) = status else {
                    harness.report.fail("the status footer renders");
                    return;
                };
                println!("  status: {}", status.text);
                harness.report.check(
                    format!("the truncated counter rose once ({})", status.truncated),
                    status.truncated == 1,
                );
                harness.report.check(
                    "the newest retained event is not marked while it is filtered out",
                    marked_rows(&snap).is_empty(),
                );
                harness
                    .report
                    .check("no event was lost by the fixture load", status.lost == 0);
            }),
        Step::new("styles: the newest retained marker appears under Show everything")
            .at(0, |world, harness| tap_control(world, harness, "Show everything"))
            .settle(8)
            .check(|world, harness| {
                let (_, snap) = status_of(world);
                let marked = marked_rows(&snap);
                println!(
                    "  marked rows: {:?}",
                    marked
                        .iter()
                        .map(|row| row.body(&snap))
                        .collect::<Vec<_>>()
                );
                harness.report.check(
                    "exactly the newest retained row carries the marker",
                    marked.len() == 1
                        && marked[0].body(&snap).contains("the newest retained event is filtered out"),
                );
            }),
    ]
}
