//! The play HUD's own spawned chrome: the camera and the Notice node.
//!
//! The frame itself — the dialogue box, the tab, the prose, the markers, the
//! options and the quick row — is the plugin's `play` surface, drawn by
//! [`super::frame`]. Only what is not a surface stays here (spec §5: the Notice
//! and the error screen are not surfaces).

use bevy::prelude::*;
use bevy::render::view::Msaa;

use super::state::NoticeText;
use crate::failure::Notice;
use crate::presentation::style::TextFaces;

pub(crate) fn setup_ui(mut commands: Commands, faces: Res<TextFaces>) {
    // The player draws axis-aligned 2D art and UI only, so `Msaa::Off` is
    // expected to change no edge the look depends on; its benefit is expected
    // and unmeasured.
    commands.spawn((Camera2d, Msaa::Off));
    commands.spawn((
        NoticeText,
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            right: Val::Px(48.0),
            ..default()
        },
        TextLayout::justify(Justify::Right),
        Text::new(""),
        faces.sans(16.0),
        TextColor::from(Color::srgba(0.95, 0.85, 0.8, 0.85)),
    ));
}

/// Mirror the [`Notice`] resource onto its one text node.
pub(crate) fn sync_notice(notice: Res<Notice>, mut text: Query<&mut Text, With<NoticeText>>) {
    let want = notice.text.clone().unwrap_or_default();
    for mut current in &mut text {
        if **current != want {
            **current = want.clone();
        }
    }
}
