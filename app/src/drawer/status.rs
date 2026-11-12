//! The panel-local status footer.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::monitor::{MonitorCapture, MonitorHistory};

use super::components::MonitorStatusText;
use super::geometry::{reader_columns, reader_metrics};
use super::projection::{reader_frame, MonitorProjection};
use super::state::MonitorPanel;

/// The panel-local status footer: the active view, the retained/shown readout,
/// the three saturating counters, the truthful in-progress and unavailable
/// flags, the reader state and the close/cycle hint. Status is presentation
/// text, never a retained line, a tracing event or a Notice.
pub(crate) fn sync_panel_status(
    panel: Res<MonitorPanel>,
    projection: Res<MonitorProjection>,
    history: Option<Res<MonitorHistory>>,
    capture: Option<Res<MonitorCapture>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut status: Query<&mut Text, With<MonitorStatusText>>,
) {
    if !panel.open {
        return;
    }
    let metrics = reader_metrics(reader_columns(&windows));
    let frame = reader_frame(&projection, history.as_deref(), metrics);
    let view = panel.reader.view(&frame);
    let retained = history
        .as_deref()
        .map(MonitorHistory::retained)
        .unwrap_or(0);
    let counters = capture
        .as_deref()
        .map(MonitorCapture::counters)
        .unwrap_or_default();
    let mut parts = vec![
        panel.view.label().to_string(),
        format!("retained {retained} · shown {}", projection.shown()),
        format!(
            "evicted {} · lost {} · truncated {}",
            counters.evicted, counters.lost, counters.truncated
        ),
    ];
    if projection.is_updating() {
        parts.push("updating view".to_string());
    }
    if capture.as_deref().is_some_and(MonitorCapture::unavailable) {
        parts.push("capture unavailable".to_string());
    }
    if view.expired {
        // Panel-local acknowledgement of retention loss: never a row, tracing
        // event, Notice or failure class.
        parts.push("older lines expired".to_string());
    }
    parts.push(
        match view.following {
            true => "following latest",
            false => "reading older lines",
        }
        .to_string(),
    );
    parts.push("F3/Esc close · Tab cycles controls".to_string());
    let want = parts.join(" · ");
    for mut text in &mut status {
        if text.as_str() != want {
            **text = want.clone();
        }
    }
}
