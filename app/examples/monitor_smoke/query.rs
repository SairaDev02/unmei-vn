//! UI-tree reads the probe uses: children, text, layout and row identity.

use crate::prelude::*;

// ---------------------------------------------------------------------------
// Tree walking
// ---------------------------------------------------------------------------

/// Read a fixed-pixel node value, panicking on anything proportional.
pub(crate) fn px(value: Val) -> f32 {
    match value {
        Val::Px(value) => value,
        other => panic!("expected a fixed px value, got {other:?}"),
    }
}

pub(crate) fn only_child(parent: Entity, children: &Query<&Children>) -> Option<Entity> {
    child_list(parent, children).first().copied()
}

pub(crate) fn child_list(parent: Entity, children: &Query<&Children>) -> Vec<Entity> {
    children
        .get(parent)
        .map(|children| children.iter().collect())
        .unwrap_or_default()
}

/// The rendered text of a row's target (column 2) and body (last column), by
/// UI shape rather than by the drawer's crate-internal component types.
pub(crate) fn row_texts(
    row: Entity,
    children: &Query<&Children>,
    texts: &Query<(Entity, &Text)>,
) -> (Option<String>, Option<String>) {
    let parts = child_list(row, children);
    let read = |entity: Option<&Entity>| -> Option<String> {
        let entity = *entity?;
        texts
            .iter()
            .find(|(candidate, _)| *candidate == entity)
            .map(|(_, text)| text.as_str().to_string())
    };
    (read(parts.get(2)), read(parts.last()))
}

pub(crate) fn node_of<'a>(
    nodes: &'a Query<(Entity, &Node, &ComputedNode, &UiGlobalTransform)>,
    entity: Entity,
) -> Option<&'a Node> {
    nodes
        .iter()
        .find(|(candidate, ..)| *candidate == entity)
        .map(|(_, node, ..)| node)
}

pub(crate) fn size_of(
    nodes: &Query<(Entity, &Node, &ComputedNode, &UiGlobalTransform)>,
    entity: Entity,
) -> Vec2 {
    nodes
        .iter()
        .find(|(candidate, ..)| *candidate == entity)
        .map(|(_, _, computed, _)| computed.size())
        .expect("the measured node")
}

pub(crate) fn place(
    nodes: &Query<(Entity, &Node, &ComputedNode, &UiGlobalTransform)>,
    entity: Entity,
) -> (Vec2, Vec2) {
    nodes
        .iter()
        .find(|(candidate, ..)| *candidate == entity)
        .map(|(_, _, computed, transform)| {
            let size = computed.size();
            // `UiGlobalTransform` carries the node's centre.
            (transform.translation - size / 2.0, size)
        })
        .expect("the measured node")
}

/// The `Show everything` control's entity and its physical centre.
pub(crate) fn show_everything_control(
    nodes: &Query<(Entity, &Node, &ComputedNode, &UiGlobalTransform)>,
    texts: &Query<(Entity, &Text)>,
) -> Option<(Entity, Vec2)> {
    let entity = texts
        .iter()
        .find(|(_, text)| text.as_str().contains("Show everything"))
        .map(|(entity, _)| entity)?;
    let (position, size) = place(nodes, entity);
    Some((entity, position + size / 2.0))
}

/// Every four-column grid row entity, top edge first.
pub(crate) fn drawer_row_entities(
    nodes: &Query<(Entity, &Node, &ComputedNode, &UiGlobalTransform)>,
    texts: &Query<(Entity, &Text)>,
    children: &Query<&Children>,
) -> Vec<Entity> {
    rendered_rows(nodes, texts, children)
        .into_iter()
        .map(|row| row.entity)
        .collect()
}

/// The drawer's log body: the panel's flexible direct child.
pub(crate) fn drawer_body(
    roots: &Query<(Entity, &GlobalZIndex)>,
    children: &Query<&Children>,
    nodes: &Query<(Entity, &Node, &ComputedNode, &UiGlobalTransform)>,
) -> Option<(Entity, Vec2, Vec2)> {
    let root = roots
        .iter()
        .find(|(_, index)| index.0 == 100)
        .map(|(entity, _)| entity)?;
    let panel = only_child(root, children)?;
    let body = child_list(panel, children)
        .into_iter()
        .find(|entity| node_of(nodes, *entity).is_some_and(|node| node.flex_grow == 1.0))?;
    let (position, size) = place(nodes, body);
    Some((body, position, size))
}

/// The fixed 10px scrollbar track inside the log body, found by shape.
pub(crate) fn scrollbar_track(
    roots: &Query<(Entity, &GlobalZIndex)>,
    children: &Query<&Children>,
    nodes: &Query<(Entity, &Node, &ComputedNode, &UiGlobalTransform)>,
) -> Option<Entity> {
    let (body, ..) = drawer_body(roots, children, nodes)?;
    child_list(body, children)
        .into_iter()
        .find(|entity| node_of(nodes, *entity).is_some_and(|node| node.width == Val::Px(10.0)))
}

/// Whether the status footer says the reader is following the newest event.
pub(crate) fn reader_following(texts: &Query<(Entity, &Text)>) -> bool {
    texts
        .iter()
        .any(|(_, text)| text.as_str().contains("following latest"))
}

/// Every four-column grid row with its laid-out top edge and rendered text.
pub(crate) fn rendered_rows(
    nodes: &Query<(Entity, &Node, &ComputedNode, &UiGlobalTransform)>,
    texts: &Query<(Entity, &Text)>,
    children: &Query<&Children>,
) -> Vec<RenderedRow> {
    let text_of = |entity: Entity| {
        texts
            .iter()
            .find(|(candidate, _)| *candidate == entity)
            .map(|(_, text)| text.as_str().to_string())
            .unwrap_or_default()
    };
    let mut rows: Vec<RenderedRow> = nodes
        .iter()
        .filter_map(|(entity, _, computed, transform)| {
            let parts = child_list(entity, children);
            if parts.len() != 4 {
                return None;
            }
            let elapsed = node_of(nodes, parts[0])?;
            if elapsed.width != Val::Px(72.0) {
                return None;
            }
            Some(RenderedRow {
                entity,
                top: transform.translation.y - computed.size().y / 2.0,
                height: computed.size().y,
                message: text_of(parts[3]),
                target: text_of(parts[2]),
            })
        })
        .collect();
    rows.sort_by(|a, b| {
        a.top
            .partial_cmp(&b.top)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    rows
}

/// The top visible event: the first grid row whose bottom edge is below the log
/// body's top edge.
pub(crate) fn top_visible_row(
    nodes: &Query<(Entity, &Node, &ComputedNode, &UiGlobalTransform)>,
    texts: &Query<(Entity, &Text)>,
    children: &Query<&Children>,
    body_top: f32,
) -> Option<RenderedRow> {
    rendered_rows(nodes, texts, children)
        .into_iter()
        .find(|row| row.top + row.height > body_top + 0.5)
}

/// The anchor must be the actual top-visible event: it may be scrolled
/// part-way into its own row (a positive within-row offset) but never pushed
/// below the body top or scrolled fully out.
pub(crate) fn check_anchor_top(label: &str, top: f32, height: f32, body_top: f32) {
    let delta = top - body_top;
    let within_row = body_top - top;
    println!(
        "  {label}: row top {top:.1} vs body top {body_top:.1} (delta {delta:.1}, within-row {within_row:.1} of {height:.1})"
    );
    if delta > 1.0 || within_row >= height {
        eprintln!("FAILED: {label} is {delta:.1}px from the body top (row {height:.1}px tall)");
        std::process::exit(1);
    }
}

/// The drawn top-visible row must be the reader's own anchored event: the
/// reader's anchor is the deliberate reading position, and the row the layout
/// put on top is what the developer sees. Row materialization is deferred to the
/// end of the frame that synced it, so the walk only measures a step on a frame
/// after the reader's last change.
pub(crate) fn check_drawn_anchor(
    label: &str,
    row: &RenderedRow,
    anchor: MonitorAnchor,
    rows: &Query<&MonitorRow>,
) {
    let drawn = rows.get(row.entity).map(|row| row.0).ok();
    if drawn != Some(anchor.sequence) {
        eprintln!(
            "FAILED: {label}: the drawn top-visible row is {:?} at admission sequence {drawn:?}, but the reader's anchor is {anchor:?}",
            row.label()
        );
        std::process::exit(1);
    }
    println!(
        "  {label}: the drawn top-visible row is the reader's anchor {anchor:?} ({:?})",
        row.label()
    );
}

/// The reader's own status must report the settled anchored position, never a
/// lost or re-resolved anchor: the panel-local `older lines expired`
/// acknowledgement and the in-progress `updating view` flag are both absent.
pub(crate) fn check_reader_status(texts: &Query<(Entity, &Text)>) {
    // The close/cycle hint is the status footer's own text, so the check cannot
    // accidentally read a log row that happens to contain the reader's wording.
    let status = texts
        .iter()
        .map(|(_, text)| text.as_str())
        .find(|text| text.contains("F3/Esc close"))
        .unwrap_or_default();
    if !status.contains("reading older lines") {
        eprintln!("FAILED: the reader status does not report the anchored position: {status:?}");
        std::process::exit(1);
    }
    if status.contains("older lines expired") || status.contains("updating view") {
        eprintln!("FAILED: the reader reported a lost or re-resolved anchor: {status:?}");
        std::process::exit(1);
    }
    println!("  reader status: {status}");
}
