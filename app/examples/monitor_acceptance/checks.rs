//! The drawer, surface and HUD checks several plans share.

use crate::prelude::*;

pub(crate) fn window_size(world: &mut World) -> (f32, f32) {
    let mut query = world.query_filtered::<&Window, With<PrimaryWindow>>();
    let window = query.single(world).expect("the primary window");
    (
        window.resolution.physical_width() as f32,
        window.resolution.physical_height() as f32,
    )
}

pub(crate) fn check_title_up(world: &mut World, harness: &mut Harness) {
    let menu = world.resource::<MenuState>();
    harness.report.check(
        "the title is up before any walk step",
        menu.open && menu.view == MenuView::Title,
    );
    harness.report.check(
        "Monitor starts closed",
        !crate::harness::snap(world).drawer_open(),
    );
}

/// The approved drawer geometry for the current window (`D10`).
pub(crate) fn check_panel(world: &mut World, harness: &mut Harness, label: &str) {
    let (width, height) = window_size(world);
    let snap = crate::harness::snap(world);
    let Some(geometry) = snap.geometry() else {
        harness
            .report
            .fail(format!("{label}: the drawer panel is not rendered"));
        return;
    };
    println!(
        "  {label} panel: x {:.1} y {:.1} w {:.1} h {:.1} (header {:.1} body {:.1} footer {:.1})",
        geometry.x,
        geometry.y,
        geometry.width,
        geometry.height,
        geometry.header,
        geometry.body,
        geometry.footer
    );
    harness
        .report
        .near("panel x", geometry.x, PANEL_SIDE_INSET, POSITION_TOLERANCE);
    harness.report.near(
        "panel width",
        geometry.width,
        width - 2.0 * PANEL_SIDE_INSET,
        SIZE_TOLERANCE,
    );
    harness.report.near(
        "panel y",
        geometry.y,
        height - PANEL_BOTTOM_RESERVATION - PANEL_HEIGHT,
        POSITION_TOLERANCE,
    );
    harness.report.near(
        "panel height",
        geometry.height,
        PANEL_HEIGHT,
        SIZE_TOLERANCE,
    );
    harness.report.near(
        "panel right inset",
        width - (geometry.x + geometry.width),
        PANEL_SIDE_INSET,
        POSITION_TOLERANCE,
    );
    harness.report.near(
        "bottom reservation",
        height - (geometry.y + geometry.height),
        PANEL_BOTTOM_RESERVATION,
        POSITION_TOLERANCE,
    );
    harness.report.near(
        "header height",
        geometry.header,
        HEADER_HEIGHT,
        POSITION_TOLERANCE,
    );
    harness
        .report
        .near("log body height", geometry.body, 293.0, SIZE_TOLERANCE);
    harness.report.near(
        "footer height",
        geometry.footer,
        FOOTER_HEIGHT,
        POSITION_TOLERANCE,
    );

    // No scrim: the drawer must not dim the surface underneath.
    if let Some(root) = snap.drawer_root() {
        let background = snap
            .node(root)
            .and_then(|node| node.background)
            .unwrap_or(Color::NONE);
        harness.report.check(
            "the drawer root carries no scrim",
            background.alpha() == 0.0,
        );
    }

    // The header controls, left to right.
    let controls = snap.header_controls();
    let labels: Vec<String> = controls.iter().map(|(label, _)| label.clone()).collect();
    println!("  {label} header: {labels:?}");
    let order: Vec<Option<usize>> = ["Monitor", "Show everything", "Follow latest", "Close"]
        .iter()
        .map(|needle| labels.iter().position(|label| label.contains(needle)))
        .collect();
    harness.report.check(
        "the header reads Monitor · Show everything · Follow latest · Close",
        order.iter().all(Option::is_some)
            && (0..order.len()).all(|index| order[index] == Some(index)),
    );
}

pub(crate) fn check_underlying_unchanged(world: &mut World, harness: &mut Harness) {
    let after = crate::harness::snap(world).underlying();
    let before = harness.before.clone().unwrap_or_default();
    harness.report.check(
        format!("the surface underneath is unmoved ({} nodes)", before.len()),
        before == after,
    );
    if before != after {
        for (index, line) in after.iter().enumerate() {
            if before.get(index) != Some(line) {
                println!("    after[{index}]:  {line}");
                if let Some(old) = before.get(index) {
                    println!("    before[{index}]: {old}");
                }
                break;
            }
        }
    }
}

pub(crate) fn check_hud_unchanged(world: &mut World, harness: &mut Harness) {
    let after = crate::harness::snap(world).hud();
    let before = harness.hud_before.clone().unwrap_or_default();
    harness
        .report
        .check("the in-play HUD is unmoved and undimmed", before == after);
    if before != after {
        println!("    before: {before:?}");
        println!("    after:  {after:?}");
    }
}

pub(crate) fn check_recorded_wait(world: &mut World, harness: &mut Harness) {
    let now = wait_of(world);
    harness.report.check(
        format!("the runner did not move while Monitor owned the frame ({now:?})"),
        now == harness.wait_before,
    );
}

/// The top and bottom of the error screen's rendered line list.
pub(crate) fn error_list_bounds(snap: &Snap) -> Option<(f32, f32)> {
    let list: Vec<&crate::harness::NodeSnap> = snap
        .nodes
        .iter()
        .filter(|node| {
            snap.text(node.entity)
                .is_some_and(|text| text.contains("nowhere_") || text.contains("and 5 more"))
        })
        .collect();
    let top = list.iter().map(|node| node.position.y).reduce(f32::min)?;
    let bottom = list
        .iter()
        .map(|node| node.position.y + node.size.y)
        .reduce(f32::max)?;
    Some((top, bottom))
}

// ---------------------------------------------------------------------------
// The walk plans
// ---------------------------------------------------------------------------
