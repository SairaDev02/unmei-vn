//! The play HUD's spawned chrome: the dialogue band, the help line, the Notice
//! node and the `AUTO`/`SKIP` cues with their shared slot.

use bevy::prelude::*;
use bevy::render::view::Msaa;

use super::super::style::TextFaces;
use super::state::{AutoCue, DialogueProse, DialogueText, HelpLine, NoticeText, SkipCue};
use crate::core::SkipState;
use crate::failure::Notice;
use crate::menu::MenuState;
use crate::settings::SettingsState;

/// Spawn the dialogue band's one text node: the mono speaker prefix as the root
/// text, with the sans prose as a [`TextSpan`] child. Shared by [`setup_ui`] and
/// the tests' fixture, so the two span layouts cannot drift
/// (`docs/title-and-menu.md` § Type system §4).
pub(crate) fn spawn_dialogue(commands: &mut Commands, faces: &TextFaces) -> Entity {
    let mut root = commands.spawn((
        DialogueText,
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(40.0),
            left: Val::Px(48.0),
            // The fixed right inset reserves the cue slot in every cue state,
            // so showing or hiding `AUTO`/`SKIP` never shifts the prose
            // (`docs/runtime-hardening.md` § Text skip §7).
            right: Val::Px(160.0),
            ..default()
        },
        Text::new(""),
        faces.mono(24.0),
        TextColor::from(Color::WHITE),
    ));
    root.with_children(|parent| {
        parent.spawn((
            DialogueProse,
            TextSpan::new(""),
            faces.sans(24.0),
            TextColor::from(Color::WHITE),
        ));
    });
    root.id()
}

pub(crate) fn setup_ui(mut commands: Commands, faces: Res<TextFaces>) {
    // The player draws axis-aligned 2D art and UI only, so `Msaa::Off` is
    // expected to change no edge the look depends on; its benefit is expected
    // and unmeasured.
    commands.spawn((Camera2d, Msaa::Off));
    spawn_dialogue(&mut commands, &faces);
    // The `AUTO` cue: absolutely positioned at the dialogue band's bottom-right,
    // so turning auto on draws over the band's free corner and moves no text.
    // Dim (≈50 % alpha) and off by default; `sync_auto_cue` clears the glyphs
    // when the preference is off.
    commands.spawn((
        AutoCue,
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(CUE_BOTTOM),
            right: Val::Px(CUE_RIGHT),
            ..default()
        },
        Text::new(""),
        faces.mono(CUE_FONT_SIZE),
        TextColor::from(AUTO_CUE_COLOR),
    ));
    // The `SKIP` cue shares that exact slot; the two are mutually exclusive
    // while skip runs, so they never overlap.
    commands.spawn((
        SkipCue,
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(CUE_BOTTOM),
            right: Val::Px(CUE_RIGHT),
            ..default()
        },
        Text::new(""),
        faces.mono(CUE_FONT_SIZE),
        TextColor::from(SKIP_CUE_COLOR),
    ));
    commands.spawn((
        HelpLine,
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            left: Val::Px(16.0),
            ..default()
        },
        Text::new(HELP_LINE),
        faces.mono(14.0),
        TextColor::from(Color::srgba(1.0, 1.0, 1.0, 0.5)),
    ));
    // The Notice: one line, top-right, right-aligned; presentation state only.
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

/// Mirror `play.auto` onto the in-play `AUTO` cue. Reads the same
/// [`SettingsState`] the `A` hotkey and the Settings switch write, so the cue
/// flips in the same frame either path toggles it — and it never depends on the
/// dialogue text, so showing or hiding it moves nothing on screen. The seen skip
/// wins the exclusive slot, so `AUTO` is hidden while it runs
/// (`docs/runtime-hardening.md` § Text skip §7).
pub(crate) fn sync_auto_cue(
    settings: Option<Res<SettingsState>>,
    skip: Option<Res<SkipState>>,
    menu: Option<Res<MenuState>>,
    mut cue: Query<&mut Text, With<AutoCue>>,
) {
    let want = if settings.as_deref().is_some_and(SettingsState::auto)
        && !skip.as_deref().is_some_and(SkipState::is_on)
        && !menu.as_deref().is_some_and(MenuState::title_surface_up)
    {
        "AUTO"
    } else {
        ""
    };
    for mut current in &mut cue {
        if **current != want {
            **current = want.to_string();
        }
    }
}

/// Mirror effective skip onto the in-play `SKIP` cue: one literal `SKIP`
/// whenever the seen toggle is engaged or a force hold is effective, hidden on
/// title surfaces exactly like `AUTO`. The cue follows the effective state, so
/// a disarmed hold shows nothing (`docs/runtime-hardening.md` § Text skip §7).
pub(crate) fn sync_skip_cue(
    skip: Option<Res<SkipState>>,
    menu: Option<Res<MenuState>>,
    mut cue: Query<&mut Text, With<SkipCue>>,
) {
    let want = if skip.as_deref().is_some_and(SkipState::is_on)
        && !menu.as_deref().is_some_and(MenuState::title_surface_up)
    {
        "SKIP"
    } else {
        ""
    };
    for mut current in &mut cue {
        if **current != want {
            **current = want.to_string();
        }
    }
}

/// The in-play help line's fixed text; cleared while the title is up. Names both
/// skip controls — the `Tab` seen toggle and the held `Ctrl` force skip
/// (`docs/runtime-hardening.md` § Text skip §7).
pub(super) const HELP_LINE: &str =
    "Space/Enter/click advance \u{00b7} Tab skip \u{00b7} Ctrl skip all \u{00b7} 1-9 or click choose \u{00b7} A auto \u{00b7} F11 fullscreen \u{00b7} Esc menu";

/// The cue slot's shared geometry, at the 1280x720 reference
/// (`docs/runtime-hardening.md` § Text skip §7).
const CUE_BOTTOM: f32 = 40.0;

const CUE_RIGHT: f32 = 48.0;

const CUE_FONT_SIZE: f32 = 20.0;

/// The seen-skip cue: warm, half-alpha.
pub(crate) const SKIP_CUE_COLOR: Color =
    Color::srgba(230.0 / 255.0, 158.0 / 255.0, 92.0 / 255.0, 0.5);

/// The auto cue: the same face and slot in neutral, half-alpha.
pub(crate) const AUTO_CUE_COLOR: Color =
    Color::srgba(217.0 / 255.0, 217.0 / 255.0, 230.0 / 255.0, 0.5);
