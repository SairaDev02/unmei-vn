//! The menu's input glue: one system reads the pointer and the keyboard into a
//! single action and applies it, so a click and a key cannot drift apart.

use bevy::clipboard::Clipboard;
use bevy::ecs::system::SystemParam;
use bevy::input::ButtonInput;
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::text::EditableText;

use crate::core::{DecisionChannel, KeyCheckChannel, PendingKeyCheck};
use crate::credentials::{ClipboardPort, CopyPathLabel, CredentialsState, KeyField};
use crate::session::SessionInput;
use crate::settings::SettingsState;
use crate::RuntimeConfig;

use super::action::{action_for_focus, apply, MenuAction};
use super::key_field::{commit_key, discard_key_field};
use super::labels::focus_track;
use super::navigation::{gallery_browse, walk_pressed, WalkCounts};
use super::persistence::flush_settings;
use super::state::{MenuFocus, MenuState, MenuView};
use super::transition::escape;

/// The Decisions key row's system parameters, bundled so [`menu_input`] stays
/// within Bevy's sixteen-parameter system limit.
#[derive(SystemParam)]
pub(crate) struct KeyRowInput<'w, 's> {
    pub(super) credentials: ResMut<'w, CredentialsState>,
    pub(super) runtime_config: ResMut<'w, RuntimeConfig>,
    pub(super) decision_channel: ResMut<'w, DecisionChannel>,
    pub(super) key_check: Res<'w, KeyCheckChannel>,
    pub(super) pending_key_check: ResMut<'w, PendingKeyCheck>,
    pub(super) clipboard_port: ResMut<'w, ClipboardPort>,
    pub(super) clipboard: Option<ResMut<'w, Clipboard>>,
    pub(super) fields: Query<'w, 's, (Entity, &'static mut EditableText), With<KeyField>>,
    pub(super) input_focus: Option<ResMut<'w, InputFocus>>,
    pub(super) copy_label: ResMut<'w, CopyPathLabel>,
}

/// Read the pointer and the keyboard into one action, then apply it.
///
/// `Esc` is handled first so its four moments always win a frame; a click and an
/// activation key funnel through the same [`apply`], so the two input paths
/// cannot drift.
pub(crate) fn menu_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut interactions: Query<(&Interaction, &MenuAction), Changed<Interaction>>,
    mut menu: ResMut<MenuState>,
    mut session: SessionInput,
    mut settings: ResMut<SettingsState>,
    mut exit: MessageWriter<AppExit>,
    mut key: KeyRowInput,
) {
    let was_open = menu.open;
    let was_settings = menu.view == MenuView::Settings;

    if keys.just_pressed(KeyCode::Escape) {
        escape(
            &mut menu,
            session.pack(),
            &mut key.fields,
            key.input_focus.as_deref_mut(),
        );
    }

    if !menu.open {
        // Esc closed the overlay in this frame: one flush for the change that
        // was still pending.
        if was_open {
            flush_settings(&mut settings, &mut session);
        }
        return;
    }

    let mut clicked = None;
    for (interaction, action) in &mut interactions {
        if *interaction == Interaction::Pressed {
            clicked = Some(*action);
        }
    }

    // The still is modal: the overlay's chrome stands but is inert while it is
    // up, so a click anywhere on the still closes it and a click on any other
    // control does nothing (`docs/title-and-menu.md` § Gallery `### Behavior`
    // §5).
    if menu.view == MenuView::Gallery && menu.gallery_still {
        clicked = clicked.filter(|action| *action == MenuAction::GalleryStillClose);
    }

    if menu.confirm.is_some() {
        clicked = clicked.filter(|action| {
            matches!(
                action,
                MenuAction::ConfirmAccept | MenuAction::ConfirmCancel
            )
        });
    }

    // Clicking elsewhere while the key field is edited discards the typed text
    // and returns to the walk, then the click is applied normally
    // (`docs/title-and-menu.md` § Decisions settings §4.2).
    if menu.key_editing {
        if let Some(action) = clicked {
            if action != MenuAction::KeyRow {
                discard_key_field(&mut menu, &mut key.fields, key.input_focus.as_deref_mut());
            }
        }
    }

    // The title: `↑`/`↓` walk the five rows, a confirm swaps Yes/No, and the
    // play path stays frozen. `Esc` is already handled above (it cancels a
    // confirm, or does nothing).
    if menu.title_up() {
        if let Some(focus) = walk_pressed(&menu, WalkCounts::default(), &keys) {
            menu.focus = focus;
        }

        let activated = clicked.or_else(|| {
            // Only `Enter` selects on the title: `Space` is advance input and
            // stays inert, exactly as the legend says (`docs/title-and-menu.md`
            // § Title §7).
            keys.just_pressed(KeyCode::Enter)
                .then(|| action_for_focus(menu.focus))
        });
        if let Some(action) = activated {
            apply(
                action,
                &mut menu,
                &mut session,
                &mut settings,
                &mut exit,
                &mut key,
            );
        }
        // Leaving the title's Settings (Esc or the back control) flushes a
        // dirty settings file, exactly as the overlay's door does.
        if was_settings && menu.view != MenuView::Settings {
            flush_settings(&mut settings, &mut session);
        }
        return;
    }

    // Keyboard navigation: Tab cycles the top bar; arrows walk the grid; digits
    // focus a manual slot; a confirmation swaps Yes/No with Left/Right; the
    // legacy picker walks its three targets. The Settings view deviates: arrows
    // move between controls and adjust the focused slider.
    let counts = match menu.view {
        MenuView::Gallery => WalkCounts {
            gallery: session.pack().gallery_entries().len(),
            timeline: 0,
        },
        MenuView::Timeline => WalkCounts {
            gallery: 0,
            timeline: session.pack().timeline_entries().len(),
        },
        _ => WalkCounts::default(),
    };
    if menu.confirm.is_none() && menu.view == MenuView::Gallery && menu.gallery_still {
        // The still is a hold: `←`/`→` browse the entry order cyclically,
        // `↑`/`↓` close back to the grid, and nothing else moves
        // (`docs/title-and-menu.md` § Gallery `### Behavior` §5).
        if let MenuFocus::GalleryCard(index) = menu.focus {
            let count = session.pack().gallery_entries().len();
            if keys.just_pressed(KeyCode::ArrowLeft) {
                if let Some(next) = gallery_browse(index, count, -1) {
                    menu.focus = MenuFocus::GalleryCard(next);
                }
            } else if keys.just_pressed(KeyCode::ArrowRight) {
                if let Some(next) = gallery_browse(index, count, 1) {
                    menu.focus = MenuFocus::GalleryCard(next);
                }
            } else if keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::ArrowDown) {
                menu.gallery_still = false;
            }
        }
    } else if let Some(focus) = walk_pressed(&menu, counts, &keys) {
        if let (Some(picker), MenuFocus::LegacyTarget(target)) = (menu.picker.as_mut(), focus) {
            picker.target = target;
        }
        menu.focus = focus;
    } else if let Some(track) = (menu.view == MenuView::Settings
        && menu.confirm.is_none()
        && menu.picker.is_none()
        && !menu.key_editing)
        .then(|| focus_track(menu.focus))
        .flatten()
    {
        if keys.just_pressed(KeyCode::ArrowLeft) {
            track.step(&mut settings, -1);
        } else if keys.just_pressed(KeyCode::ArrowRight) {
            track.step(&mut settings, 1);
        }
    }
    if let (None, Some(picker)) = (menu.confirm, menu.picker) {
        menu.focus = MenuFocus::LegacyTarget(picker.target);
    }

    if menu.key_editing {
        // `Enter` commits and returns to the walk; the typed value is kept on a
        // shape or write failure so it can be corrected
        // (`docs/title-and-menu.md` § Decisions settings §4.2).
        if keys.just_pressed(KeyCode::Enter) {
            commit_key(
                &mut menu,
                &mut key.credentials,
                &mut key.fields,
                key.input_focus.as_deref_mut(),
                &mut session,
                key.key_check.checker.as_ref(),
                &mut key.pending_key_check,
            );
        }
    } else {
        let activated = clicked.or_else(|| {
            // The still is a hold: `Enter`/`Space` do nothing there; only the
            // still's own `←`/`→`/`↑`/`↓`, `Esc` and a click act
            // (`docs/title-and-menu.md` § Gallery `### Behavior` §5).
            if menu.view == MenuView::Gallery && menu.gallery_still {
                return None;
            }
            let enter = keys.just_pressed(KeyCode::Enter);
            let space = keys.just_pressed(KeyCode::Space);
            let focus = action_for_focus(menu.focus);
            // `Space` is a text character, so only `Enter` enters the field.
            let activate = if focus == MenuAction::KeyRow {
                enter
            } else {
                enter || space
            };
            activate.then_some(focus)
        });

        if let Some(action) = activated {
            apply(
                action,
                &mut menu,
                &mut session,
                &mut settings,
                &mut exit,
                &mut key,
            );
        }
    }

    // One flush per leave: backing out of Settings to the grid, or closing the
    // overlay from the grid, persists whatever change is pending. Opening the
    // view or merely moving a slider writes nothing.
    let left_settings = was_settings && menu.view != MenuView::Settings;
    let closed = was_open && !menu.open;
    if left_settings || closed {
        flush_settings(&mut settings, &mut session);
    }
}
