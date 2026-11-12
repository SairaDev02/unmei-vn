//! The materialized log rows: the display window, the grid and the newest marker.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy::prelude::*;
use bevy::text::{LineBreak, LineHeight, TextLayout};
use bevy::ui::UiRect;
use bevy::window::PrimaryWindow;

use crate::monitor::{MonitorHistory, MonitorLine};
use crate::monitor_reader::Metrics;
use crate::presentation::style::TextFaces;

use super::components::{
    MonitorEmptyState, MonitorLocatingState, MonitorReaderNode, MonitorRow, MonitorRowPart,
};
use super::geometry::{
    level_tone, log_font, reader_columns, reader_metrics, LOG_LINE_HEIGHT, NEWEST_INSET,
    NEWEST_WASH, ROW_COLUMN_GAP, ROW_ELAPSED_WIDTH, ROW_LEVEL_WIDTH, ROW_LINE_PX, ROW_TARGET_WIDTH,
    ROW_VERTICAL_PADDING, TEXT_WORK_BUDGET, TONE_FAINT, TONE_INFO, TONE_TEXT, TONE_WARN,
};
use super::projection::{newest_retained, reader_frame, MonitorProjection};
use super::state::{MonitorPanel, MonitorWork};

/// Sync the materialized rows to the reader's window over the current-predicate
/// projection: the newest [`DISPLAY_ROWS`] while following, or a
/// [`DISPLAY_ROWS`]-tall window starting at the stable anchor while reading.
///
/// The body clips at its own edges, so following needs no scrolling; a reading
/// window bottom-aligns a tail shorter than the viewport through the gap spacer
/// and shifts the whole rows group up by the anchor's within-row offset. Rows
/// that survive a window change keep their materialized text; only the missing
/// ones are spawned, each inserted at its place in the window so a scroll or a
/// jump never reorders the display.
#[allow(clippy::too_many_arguments)]
pub(crate) fn sync_panel_rows(
    mut commands: Commands,
    faces: Option<Res<TextFaces>>,
    panel: Res<MonitorPanel>,
    projection: Res<MonitorProjection>,
    history: Option<Res<MonitorHistory>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut work: ResMut<MonitorWork>,
    mut rows: Query<(Entity, &MonitorRow, &mut Node), Without<MonitorReaderNode>>,
    mut reader_nodes: Query<(Entity, &mut Node, &MonitorReaderNode)>,
    empties: Query<Entity, With<MonitorEmptyState>>,
    locating: Query<Entity, With<MonitorLocatingState>>,
) {
    if !panel.open {
        return;
    }
    let Some(faces) = faces else {
        return;
    };

    let metrics = reader_metrics(reader_columns(&windows));
    let frame = reader_frame(&projection, history.as_deref(), metrics);
    let view = panel.reader.view(&frame);
    // The drawer materializes the reader's visible range; the reader never
    // clones or retains the rows itself.
    let window: Vec<Arc<MonitorLine>> = frame.rows.range(view.range.clone()).cloned().collect();
    let anchored = view.anchor.is_some() && !view.pending && !view.following;

    // The reader chrome: a short anchored tail bottom-aligns through the gap
    // spacer, and the within-row offset shifts the rows group as one. The
    // reader derives both in display lines; the drawer only converts to px.
    let shift = if anchored {
        view.shift_lines * ROW_LINE_PX
    } else {
        0.0
    };
    let gap = if anchored {
        view.spacer_lines * ROW_LINE_PX
    } else {
        0.0
    };

    let mut rows_content = None;
    for (entity, mut node, part) in &mut reader_nodes {
        match part {
            MonitorReaderNode::Viewport => {
                node.justify_content = if anchored {
                    JustifyContent::FlexStart
                } else {
                    JustifyContent::FlexEnd
                };
            }
            MonitorReaderNode::Spacer => {
                node.height = Val::Px(gap);
            }
            MonitorReaderNode::Rows => {
                node.top = Val::Px(-shift);
                rows_content = Some(entity);
            }
            MonitorReaderNode::Thumb => {
                node.height = Val::Percent(view.thumb_size * 100.0);
                node.top = Val::Percent(view.thumb_top * 100.0);
            }
        }
    }
    let Some(rows_content) = rows_content else {
        return;
    };

    let newest_retained = newest_retained(history.as_deref());
    let window_by_sequence: HashMap<u64, &Arc<MonitorLine>> =
        window.iter().map(|line| (line.sequence, line)).collect();
    let existing: HashMap<u64, Entity> = rows
        .iter()
        .map(|(entity, row, _)| (row.0, entity))
        .collect();
    let desired: HashSet<u64> = window.iter().map(|line| line.sequence).collect();

    // A surviving row keeps its admitted text, but its imposed height is a
    // function of the current column width, so a resize/rewrap must recompute
    // it or the shift, spacer and clamp would disagree with the drawn row.
    for (_, row, mut node) in &mut rows {
        let Some(line) = window_by_sequence.get(&row.0) else {
            continue;
        };
        let line = *line;
        if line.truncated {
            continue;
        }
        let want = Val::Px(metrics.height_units(line) * ROW_LINE_PX);
        if node.height != want {
            node.height = want;
        }
    }

    // Despawn rows that left the display window.
    for (sequence, entity) in &existing {
        if !desired.contains(sequence) {
            commands.entity(*entity).despawn();
        }
    }

    // Show the empty state only for a genuinely empty view; a pending refresh
    // shows a locating placeholder instead of the newest current-predicate rows.
    if window.is_empty() {
        if view.pending {
            for entity in &empties {
                commands.entity(entity).despawn();
            }
            if locating.is_empty() {
                commands.entity(rows_content).with_children(|content| {
                    content.spawn((
                        MonitorLocatingState,
                        state_label(&faces, "locating your position…"),
                    ));
                });
            }
            return;
        }
        for entity in &locating {
            commands.entity(entity).despawn();
        }
        if empties.is_empty() {
            commands.entity(rows_content).with_children(|content| {
                content.spawn((
                    MonitorEmptyState,
                    state_label(&faces, panel.view.empty_label()),
                ));
            });
        }
        return;
    }
    for entity in &empties {
        commands.entity(entity).despawn();
    }
    for entity in &locating {
        commands.entity(entity).despawn();
    }

    let mut present: HashSet<u64> = existing
        .keys()
        .copied()
        .filter(|sequence| desired.contains(sequence))
        .collect();
    for (position, line) in window.iter().enumerate() {
        if present.contains(&line.sequence) {
            // The row's payload is immutable once admitted; only the marker
            // moves, and `sync_panel_row_markers` owns that repaint.
            continue;
        }
        if work.text_ops >= TEXT_WORK_BUDGET {
            break;
        }
        // The insert index counts only the already-present earlier rows, so a
        // partial materialization still keeps the display in window order.
        let index = window[..position]
            .iter()
            .filter(|line| present.contains(&line.sequence))
            .count();
        let newest = Some(line.sequence) == newest_retained;
        spawn_row(
            &mut commands,
            rows_content,
            index,
            &faces,
            line,
            metrics,
            newest,
        );
        present.insert(line.sequence);
        work.text_ops += 1;
    }
}

/// The shared body of a panel-local placeholder line ([`MonitorEmptyState`] and
/// [`MonitorLocatingState`]): one faint logical line at the same inset.
fn state_label(faces: &TextFaces, label: &str) -> impl Bundle {
    (
        Text::new(label),
        log_font(faces),
        TextColor::from(TONE_FAINT),
        LineHeight::RelativeToFont(LOG_LINE_HEIGHT),
        Node {
            padding: UiRect::axes(Val::Px(14.0), Val::Px(0.0)),
            ..default()
        },
    )
}

fn spawn_row(
    commands: &mut Commands,
    content: Entity,
    index: usize,
    faces: &TextFaces,
    line: &MonitorLine,
    metrics: Metrics,
    newest: bool,
) {
    // A character-wrapping row's automatic minimum height is its min-content
    // height (every character on its own line), which taffy would otherwise
    // adopt as the row height. Give the row its real laid-out height instead,
    // from the same per-column metric the reader uses. A byte-truncated event
    // keeps its preserved single warm line and its own natural height.
    let mut node = Node {
        width: Val::Percent(100.0),
        border: UiRect::left(Val::Px(2.0)),
        padding: UiRect::axes(Val::Px(12.0), Val::Px(ROW_VERTICAL_PADDING)),
        column_gap: Val::Px(ROW_COLUMN_GAP),
        ..default()
    };
    if !line.truncated {
        node.height = Val::Px(metrics.height_units(line) * ROW_LINE_PX);
    }
    let row = commands
        .spawn((
            MonitorRow(line.sequence),
            node,
            BackgroundColor(if newest { NEWEST_WASH } else { Color::NONE }),
            BorderColor::all(if newest { NEWEST_INSET } else { Color::NONE }),
        ))
        .id();
    commands.entity(content).insert_children(index, &[row]);
    commands.entity(row).with_children(|row| {
        if line.truncated {
            // An oversized event collapses the grid into one warm-toned line
            // with the visible `[truncated]` suffix; the message wraps inside
            // the row instead of running past the panel.
            row.spawn((
                MonitorRowPart::Body,
                Text::new(line.text.clone()),
                log_font(faces),
                TextColor::from(TONE_WARN),
                LineHeight::RelativeToFont(LOG_LINE_HEIGHT),
                Node {
                    flex_grow: 1.0,
                    min_width: Val::Px(0.0),
                    ..default()
                },
            ));
            return;
        }
        let mut part = |part: MonitorRowPart, value: String, color: Color, width: Option<f32>| {
            let mut node = Node {
                flex_shrink: 0.0,
                ..default()
            };
            match width {
                Some(width) => node.width = Val::Px(width),
                // The message/fields column takes the remaining space
                // and may shrink below its content width, so a long
                // message wraps instead of running off the panel
                // (`min_width: auto` would clamp it to the widest
                // word).
                None => {
                    node.flex_grow = 1.0;
                    node.flex_shrink = 1.0;
                    node.min_width = Val::Px(0.0);
                }
            }
            let mut entity = row.spawn((
                part,
                Text::new(value),
                log_font(faces),
                TextColor::from(color),
                LineHeight::RelativeToFont(LOG_LINE_HEIGHT),
                // Machine-value rows wrap at any character, so a long unbroken
                // target or message folds inside its column instead of running
                // over the next one, and the laid-out line count matches the
                // reader's analytical row metric.
                TextLayout::linebreak(LineBreak::AnyCharacter),
                node,
            ));
            if part == MonitorRowPart::Elapsed {
                // The fixed-width elapsed field is right-aligned and never
                // wraps, so this deliberately replaces the character-wrap
                // layout set above.
                entity.insert(TextLayout::justify(Justify::Right));
            }
        };
        part(
            MonitorRowPart::Elapsed,
            format!("{}ms", line.elapsed_ms),
            TONE_FAINT,
            Some(ROW_ELAPSED_WIDTH),
        );
        part(
            MonitorRowPart::Level,
            line.level.to_string(),
            level_tone(line.level),
            Some(ROW_LEVEL_WIDTH),
        );
        part(
            MonitorRowPart::Target,
            line.target_display.clone(),
            TONE_INFO,
            Some(ROW_TARGET_WIDTH),
        );
        part(
            MonitorRowPart::Body,
            line.body().to_string(),
            TONE_TEXT,
            None,
        );
    });
}

/// Repaint every window row's newest marker from the retained tail, so the
/// marker moves with a new arrival and disappears when the newest retained event
/// is filtered out.
pub(crate) fn sync_panel_row_markers(
    panel: Res<MonitorPanel>,
    projection: Res<MonitorProjection>,
    history: Option<Res<MonitorHistory>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut rows: Query<(&MonitorRow, &mut BackgroundColor, &mut BorderColor)>,
) {
    if !panel.open {
        return;
    }
    let metrics = reader_metrics(reader_columns(&windows));
    let frame = reader_frame(&projection, history.as_deref(), metrics);
    let view = panel.reader.view(&frame);
    let window: Vec<u64> = frame
        .rows
        .range(view.range.clone())
        .map(|line| line.sequence)
        .collect();
    let newest_retained = newest_retained(history.as_deref());
    for (row, mut background, mut border) in &mut rows {
        if !window.contains(&row.0) {
            continue;
        }
        let newest = Some(row.0) == newest_retained;
        let wash = if newest { NEWEST_WASH } else { Color::NONE };
        if background.0 != wash {
            background.0 = wash;
        }
        let inset = if newest { NEWEST_INSET } else { Color::NONE };
        if border.left != inset {
            *border = BorderColor::all(inset);
        }
    }
}
