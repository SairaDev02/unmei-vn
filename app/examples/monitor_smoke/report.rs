//! The walk's report lines and its one numeric check.

use crate::prelude::*;
use crate::query::px;

#[allow(clippy::too_many_arguments)]
pub(crate) fn report(
    label: &str,
    windows: &Query<&mut Window, With<PrimaryWindow>>,
    nodes: &Query<(Entity, &Node, &ComputedNode, &UiGlobalTransform)>,
    texts: &Query<(Entity, &Text)>,
    layouts: &Query<(Entity, &TextLayoutInfo)>,
    children: &Query<&Children>,
    roots: &Query<(Entity, &GlobalZIndex)>,
    backgrounds: &Query<&BackgroundColor>,
) {
    let window = windows.iter().next().expect("the primary window");
    let (width, height) = (
        window.resolution.physical_width() as f32,
        window.resolution.physical_height() as f32,
    );
    println!("[{label}] window: {width}x{height}");

    let root = roots
        .iter()
        .find(|(_, index)| index.0 == 100)
        .map(|(entity, _)| entity)
        .expect("the stacked drawer root exists");
    // The drawer root must not dim the HUD behind it: no scrim, no fill.
    if let Ok(background) = backgrounds.get(root) {
        let alpha = background.0.alpha();
        if alpha > 0.0 {
            eprintln!("FAILED: the drawer root dims the scene (alpha {alpha})");
            std::process::exit(1);
        }
    }
    println!("[{label}] root scrim: none");
    let panel = only_child(root, children).expect("the panel node");
    let panel_children = child_list(panel, children);
    let (header, body, status) = (
        panel_children.first().copied().expect("the header"),
        panel_children
            .iter()
            .copied()
            .find(|entity| node_of(nodes, *entity).is_some_and(|node| node.flex_grow == 1.0))
            .expect("the log body"),
        panel_children
            .iter()
            .copied()
            .find(|entity| node_of(nodes, *entity).is_some_and(|node| node.height == Val::Px(13.0)))
            .expect("the status footer"),
    );

    let (position, size) = place(nodes, panel);
    let (x, y, w, h) = (position.x, position.y, size.x, size.y);
    println!("[{label}] panel: x {x:.1} y {y:.1} w {w:.1} h {h:.1}");
    // Fixed px anchoring: 24px side insets, 360px height, 148px bottom
    // reservation at every size (1232px at 1280, 1872px at 1920).
    check("panel left", x, 24.0, POSITION_TOLERANCE);
    check("panel top", y, height - 148.0 - 360.0, POSITION_TOLERANCE);
    check("panel width", w, width - 48.0, SIZE_TOLERANCE);
    check("panel height", h, 360.0, SIZE_TOLERANCE);
    check(
        "panel right inset",
        width - (x + w),
        24.0,
        POSITION_TOLERANCE,
    );
    check(
        "bottom reservation",
        height - (y + h),
        148.0,
        POSITION_TOLERANCE,
    );

    let header_height = node_of(nodes, header).expect("the header").height;
    let body_size = size_of(nodes, body);
    let footer_height = node_of(nodes, status).expect("the footer").height;
    println!(
        "[{label}] header {header_height:?}  body {:.1}  footer {footer_height:?}",
        body_size.y
    );
    check("header height", px(header_height), 52.0, POSITION_TOLERANCE);
    check("log body height", body_size.y, 293.0, 3.0);
    check("footer height", px(footer_height), 13.0, POSITION_TOLERANCE);

    let rows = drawer_row_entities(nodes, texts, children);
    println!("[{label}] rows: {}", rows.len());
    if rows.is_empty() {
        eprintln!("FAILED: the real capture produced no rendered row");
        std::process::exit(1);
    }
    let width_of = |part: Entity| size_of(nodes, part).x;
    // The text pipeline's own laid-out glyph size: the honest measure of whether
    // a column's text fits its box (wrapped/shortened) or overflows it.
    let glyph_width = |part: Entity| {
        layouts
            .iter()
            .find(|(entity, _)| *entity == part)
            .map_or(0.0, |(_, layout)| layout.size.x)
    };
    // The laid-out glyph height, so the imposed row height can be checked
    // against the text's own extent rather than against the drawer's formula.
    let glyph_height = |part: Entity| {
        layouts
            .iter()
            .find(|(entity, _)| *entity == part)
            .map_or(0.0, |(_, layout)| layout.size.y)
    };

    // Long text wraps inside the panel: no row may run past the log body, and
    // the fixed columns plus the flexible message column must fill the row
    // exactly (the message column takes the remaining space instead of its
    // content width).
    let (body_position, body_size) = place(nodes, body);
    let body_edge = body_position.x + body_size.x;
    let mut tall = 0.0_f32;
    let mut short_row = None;
    let mut long_body_row = None;
    let mut long_target_row = None;
    for row in &rows {
        let (position, size) = place(nodes, *row);
        tall = tall.max(size.y);
        if position.x + size.x > body_edge + 1.5 {
            eprintln!(
                "FAILED: a row overflows the log body ({:.1} > {body_edge:.1})",
                position.x + size.x
            );
            std::process::exit(1);
        }
        let (target, row_body) = row_texts(*row, children, texts);
        let parts = child_list(*row, children);
        if parts.len() == 4 {
            let columns: f32 = parts.iter().map(|part| width_of(*part)).sum();
            let available = size.x - 2.0 - 24.0 - 3.0 * 10.0;
            if (columns - available).abs() > 1.5 {
                eprintln!("FAILED: the grid columns span {columns:.1} of {available:.1} available");
                std::process::exit(1);
            }
            // The imposed row height is the reader's analytical row metric. It
            // is correct only if it equals the text's own laid-out extent plus
            // the 1px top/bottom padding — the independent verification of the
            // "actual laid-out row dimensions" equivalent.
            let text_height = parts
                .iter()
                .map(|part| glyph_height(*part))
                .fold(0.0_f32, f32::max);
            if (size.y - (text_height + 2.0)).abs() > 2.5 {
                eprintln!(
                    "FAILED: row height {:.1} does not match its laid-out text {:.1}",
                    size.y, text_height
                );
                std::process::exit(1);
            }
        }
        let message = row_body.as_deref().map(str::trim).unwrap_or_default();
        match (message, target.as_deref()) {
            ("monitor smoke fixture", _) => short_row = Some(*row),
            (body, _) if body.contains("monitor-smoke-wrapped-message") => {
                long_body_row = Some(*row)
            }
            (_, Some(target)) if target.contains("the_target_column_tail") => {
                long_target_row = Some(*row)
            }
            _ => {}
        }
    }
    println!("[{label}] tallest row: {tall:.1}");

    // Fixture 1: the short line stays one line.
    let short_row = short_row.unwrap_or_else(|| {
        eprintln!("FAILED: the short fixture row is missing");
        std::process::exit(1)
    });
    check(
        "single-line row height",
        size_of(nodes, short_row).y,
        SINGLE_LINE_ROW,
        1.0,
    );

    // Fixture 2: a long message wraps inside its column, so its laid-out
    // glyphs fit the column width and its row grows past one line.
    let long_body_row = long_body_row.unwrap_or_else(|| {
        eprintln!("FAILED: the long-body fixture row is missing");
        std::process::exit(1)
    });
    let long_body_parts = child_list(long_body_row, children);
    let body_column = width_of(long_body_parts[3]);
    let body_row_height = size_of(nodes, long_body_row).y;
    println!(
        "[{label}] long message: glyphs {:.1} in a {body_column:.1} column, row {body_row_height:.1}",
        glyph_width(long_body_parts[3])
    );
    if glyph_width(long_body_parts[3]) > body_column + 1.5 {
        eprintln!(
            "FAILED: the long message overflows its column ({:.1} > {body_column:.1})",
            glyph_width(long_body_parts[3])
        );
        std::process::exit(1);
    }
    if body_row_height < SINGLE_LINE_ROW * 1.5 {
        eprintln!("FAILED: the long message did not wrap ({body_row_height:.1}px)");
        std::process::exit(1);
    }

    // Fixture 3: the whole escaped target renders and wraps inside the fixed
    // 224px target column instead of being shortened under an ellipsis (B5).
    let long_target_row = long_target_row.unwrap_or_else(|| {
        eprintln!("FAILED: the long-target fixture row is missing");
        std::process::exit(1)
    });
    let long_target_parts = child_list(long_target_row, children);
    let target_column = width_of(long_target_parts[2]);
    let long_target = texts
        .iter()
        .find(|(entity, _)| *entity == long_target_parts[2])
        .map_or_else(String::new, |(_, text)| text.as_str().to_string());
    let long_target_glyphs = layouts
        .iter()
        .find(|(entity, _)| *entity == long_target_parts[2])
        .map_or(Vec2::ZERO, |(_, layout)| layout.size);
    println!(
        "[{label}] long target: {long_target:?} glyphs {:.1}x{:.1} in a {target_column:.1} column",
        long_target_glyphs.x, long_target_glyphs.y
    );
    if long_target.starts_with('\u{2026}')
        || !long_target.starts_with("unmei::smoke::a_deliberately_long_module_path")
        || !long_target.ends_with("the_target_column_tail")
    {
        eprintln!("FAILED: the long target was not rendered whole: {long_target:?}");
        std::process::exit(1);
    }
    if glyph_width(long_target_parts[2]) > target_column + 1.5 {
        eprintln!(
            "FAILED: the long target overflows its column ({:.1} > {target_column:.1})",
            glyph_width(long_target_parts[2])
        );
        std::process::exit(1);
    }
    if long_target_glyphs.y < SINGLE_LINE_ROW * 1.5 {
        eprintln!(
            "FAILED: the long target did not wrap ({:.1}px tall)",
            long_target_glyphs.y
        );
        std::process::exit(1);
    }
    if size_of(nodes, long_target_row).y < SINGLE_LINE_ROW * 1.5 {
        eprintln!(
            "FAILED: the long-target row did not grow ({:.1}px)",
            size_of(nodes, long_target_row).y
        );
        std::process::exit(1);
    }

    // The short fixture's grid: elapsed / LEVEL / target / message+fields.
    let parts = child_list(short_row, children);
    if parts.len() != 4 {
        eprintln!("FAILED: the grid row has {} columns", parts.len());
        std::process::exit(1);
    }
    check("elapsed column", width_of(parts[0]), 72.0, SIZE_TOLERANCE);
    check("level column", width_of(parts[1]), 46.0, SIZE_TOLERANCE);
    check("target column", width_of(parts[2]), 224.0, SIZE_TOLERANCE);
    let target = texts
        .iter()
        .find(|(entity, _)| *entity == parts[2])
        .map(|(_, text)| text.as_str().to_string())
        .expect("the target column text");
    println!("[{label}] target: {target:?}");
    if !target.contains("\\u{202e}") {
        eprintln!("FAILED: the bidi target was not escaped in the rendered grid: {target:?}");
        std::process::exit(1);
    }

    println!("[{label}] geometry check complete");
}

pub(crate) fn check(name: &str, actual: f32, expected: f32, tolerance: f32) {
    let delta = (actual - expected).abs();
    println!("  {name}: {actual:.1} (expected {expected:.1} ±{tolerance:.1})");
    if delta > tolerance {
        eprintln!("FAILED: {name} is {actual:.1}, expected {expected:.1} ±{tolerance:.1}");
        std::process::exit(1);
    }
}
