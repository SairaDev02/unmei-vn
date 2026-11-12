//! The player-facing startup error screen.

use bevy::prelude::*;

use super::super::style::TextFaces;
use super::state::ErrorScreen;
use crate::failure::StartupFailure;

/// Draw the startup error screen for a pack that would not load — or for an
/// executable whose own location is unknown. Warnings never reach it. The intro
/// line is sans; the pack path, the detail lines and the quit footer are mono
/// (`docs/title-and-menu.md` § Type system §2).
pub(crate) fn draw_error_screen(
    mut commands: Commands,
    faces: Res<TextFaces>,
    failure: Option<Res<StartupFailure>>,
    existing: Query<Entity, With<ErrorScreen>>,
) {
    let Some(failure) = failure else {
        return;
    };
    if !existing.is_empty() {
        return;
    }

    let mut machine = String::new();
    if let Some(pack_root) = failure.pack_root() {
        machine.push_str(&format!("{}\n\n", pack_root.display()));
    }
    for line in failure.screen_lines() {
        machine.push_str(&line);
        machine.push('\n');
    }
    machine.push_str("\nEsc · Enter · Space · click to quit");
    let text_color = TextColor::from(Color::srgb(0.9, 0.9, 0.9));

    commands
        .spawn((
            ErrorScreen,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                left: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                padding: UiRect::all(Val::Px(32.0)),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgb(0.06, 0.06, 0.08)),
        ))
        .with_children(|screen| {
            screen.spawn((Text::new(failure.intro()), faces.sans(16.0), text_color));
            screen.spawn((Text::new(machine), faces.mono(16.0), text_color));
        });
}
