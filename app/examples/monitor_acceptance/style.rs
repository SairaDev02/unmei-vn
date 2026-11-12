//! The tone, escape, Unicode, target and truncation fixture checks.

use crate::prelude::*;

pub(crate) fn check_style_fixtures(world: &mut World, harness: &mut Harness) {
    let snap = crate::harness::snap(world);
    let rows = snap.rows();
    harness.report.check(
        format!("the log body materialized rows ({})", rows.len()),
        !rows.is_empty(),
    );

    for (needle, tone, name) in [
        (fixture::WARN_FIXTURE, TONE_WARN, "WARN"),
        (fixture::ERROR_FIXTURE, TONE_ERROR, "ERROR"),
        (fixture::SHORT_FIXTURE, TONE_INFO, "INFO"),
    ] {
        let Some(row) = row_with_body(&snap, needle) else {
            harness
                .report
                .fail(format!("the {name} fixture row is rendered"));
            continue;
        };
        let color = row.level_color(&snap);
        harness.report.check(
            format!(
                "the {} fixture renders {} in the {name} tone ({color:?})",
                name,
                row.level(&snap)
            ),
            row.level(&snap) == name && color == Some(tone),
        );
    }

    // The escaped/control/Unicode fixtures render as one logical line each.
    for (needle, name) in [
        (fixture::ESCAPED_FIXTURE, "escaped"),
        (fixture::UNICODE_FIXTURE, "Unicode"),
    ] {
        match row_with_body(&snap, needle) {
            Some(row) => {
                let body = row.body(&snap);
                harness.report.check(
                    format!("the {name} fixture stays one logical row"),
                    !body.contains('\n') && !row.truncated,
                );
            }
            None => harness
                .report
                .fail(format!("the {name} fixture row is rendered")),
        }
    }
    if let Some(row) = row_with_body(&snap, fixture::ESCAPED_FIXTURE) {
        let body = row.body(&snap);
        harness.report.check(
            "the escaped fixture keeps \\n literal and never a real newline",
            body.contains("\\n") && !body.contains('\n'),
        );
        harness.report.check(
            "the escaped fixture escapes the bidi control",
            body.contains("\\u{202e}") || body.contains("\\u{202E}"),
        );
    }

    // The oversized fixture collapses the grid with a visible suffix.
    let truncated = snap
        .rows()
        .into_iter()
        .find(|row| row.truncated && row.body(&snap).contains(fixture::TRUNCATED_SUFFIX));
    match truncated {
        Some(row) => {
            harness.report.check(
                "the oversized fixture collapses to one warm line",
                snap.color(row.parts[0]) == Some(TONE_WARN),
            );
        }
        None => harness
            .report
            .fail("the oversized fixture renders a visible [truncated] row"),
    }

    // The long module path renders whole and wraps inside the fixed target
    // column (B5): no ordinary display ellipsis, and the row grows past one
    // line.
    let long_target = snap.rows().into_iter().find(|row| {
        !row.truncated
            && snap
                .text(row.parts[2])
                .is_some_and(|text| text.contains(fixture::TARGET_TAIL))
    });
    match long_target {
        Some(row) => {
            let target = snap.text(row.parts[2]).unwrap_or_default().to_string();
            let widths = column_widths(&snap, &row);
            harness.report.check(
                format!("the long target renders whole ({target:?})"),
                target.starts_with("unmei::acceptance::a_deliberately_long_module_path")
                    && target.ends_with(fixture::TARGET_TAIL)
                    && !target.starts_with('\u{2026}'),
            );
            harness.report.check(
                "the long target fits its 224px column",
                widths
                    .get(2)
                    .is_some_and(|width| *width <= ROW_TARGET_WIDTH + 1.5),
            );
            harness.report.check(
                "the long target row grew past one line",
                row.height > SINGLE_LINE_ROW * 1.5,
            );
        }
        None => harness
            .report
            .fail("the long-target fixture renders a whole target"),
    }

    // The grid columns of a normal single-line row, and the settled row metrics.
    if let Some(row) = row_with_body(&snap, fixture::SHORT_FIXTURE) {
        let widths = column_widths(&snap, &row);
        println!("  short fixture row widths: {widths:?}");
        harness.report.near(
            "elapsed column",
            widths.first().copied().unwrap_or_default(),
            ROW_ELAPSED_WIDTH,
            2.5,
        );
        harness.report.near(
            "level column",
            widths.get(1).copied().unwrap_or_default(),
            ROW_LEVEL_WIDTH,
            2.5,
        );
        harness.report.near(
            "target column",
            widths.get(2).copied().unwrap_or_default(),
            ROW_TARGET_WIDTH,
            2.5,
        );
        harness
            .report
            .near("single-line row height", row.height, SINGLE_LINE_ROW, 1.0);
        let gaps: Vec<f32> = row
            .parts
            .windows(2)
            .filter_map(|pair| {
                let left = snap.node(pair[0])?;
                let right = snap.node(pair[1])?;
                Some((right.position.x - (left.position.x + left.size.x)).abs())
            })
            .collect();
        harness.report.check(
            format!("the grid gaps are 10px ({gaps:?})"),
            gaps.iter().all(|gap| (*gap - ROW_COLUMN_GAP).abs() < 1.5),
        );
        let padding = snap
            .node(row.entity)
            .map(|node| node.shape.clone())
            .unwrap_or_default();
        harness.report.check(
            "the row carries 1px vertical padding",
            padding.contains("top: Px(1.0)") && padding.contains("bottom: Px(1.0)"),
        );
    } else {
        harness
            .report
            .fail("the short INFO fixture renders a normal grid row");
    }
}
