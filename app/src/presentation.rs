//! Presentation assembly: the one menu-tree rebuild transaction, the
//! title/overlay door dispatch and the single focus repaint.
//!
//! The coordinator alone chooses which surface the tree mounts: it owns the one
//! structural rebuild transaction and the title/overlay door dispatch, and the
//! overlay it builds dispatches its own sub-view content, so no leaf decides for
//! itself. Each surface's realization state stays with its owner behind concrete
//! operations, and the shared asset watch and style support sit beneath them
//! ([ADR 0013](../../docs/adr/0013-presentation-surface-partition.md)):
//! [`play`], [`title`], [`overlay`], [`settings_view`], [`gallery`] and
//! [`timeline`] over [`assets`] and [`style`]. The Monitor drawer's widget
//! syncs stay the reader's ([the reading-anchor
//! decision](../../docs/runtime-hardening.md#reading-anchor--decided-seam-2026-10-13));
//! this module introduces no second reader or metric formula.

pub(crate) mod assets;
pub(crate) mod gallery;
pub(crate) mod overlay;
pub(crate) mod play;
pub(crate) mod settings_view;
pub(crate) mod style;
pub(crate) mod timeline;
pub(crate) mod title;

use bevy::prelude::*;
use bevy::ui::ScrollPosition;
use unmei_engine::save::SlotInfo;

use crate::credentials::CredentialsState;
use crate::menu::{self, MenuAction, MenuState, MenuView};
use crate::settings::SettingsState;
use crate::PresentationSet;
use crate::Runtime;

use assets::AssetWatch;
use gallery::build_title_gallery;
use overlay::build_overlay;
use settings_view::build_title_settings;
use style::{TextFaces, FOCUS};
use timeline::TimelineBody;
use title::build_title;

/// Marks the whole overlay tree, so one despawn clears it.
#[derive(Component)]
pub(crate) struct MenuRoot;

/// The structural signature of the last-built overlay. Focus is deliberately
/// *not* part of it: focus changes only repaint a border, never rebuild the tree
/// (which would drop the buttons' `Interaction` state mid-click).
#[derive(Resource, Default, PartialEq, Clone)]
pub(crate) struct MenuBuilt {
    open: bool,
    view: Option<MenuView>,
    mode: Option<menu::MenuMode>,
    confirm: Option<menu::ConfirmKind>,
    picker: Option<menu::LegacyPicker>,
    slots: Vec<SlotInfo>,
    legacy_importable: bool,
    /// The title's Continue line, so a re-entry with a different autosave or
    /// scene repaints the wordmark's foot row.
    continue_line: String,
    continue_enabled: bool,
    settings_origin: Option<menu::MenuOrigin>,
    /// The gallery's door, so the back label repaints when a different door
    /// opens it.
    gallery_origin: Option<menu::MenuOrigin>,
    /// Whether the still view is up, so entering or leaving it swaps the grid
    /// for the one contain-fit entry and its caption strip.
    gallery_still: bool,
}

fn focus_border(menu: &MenuState, action: MenuAction) -> Color {
    if menu::action_for_focus(menu.focus) == action {
        FOCUS
    } else {
        Color::NONE
    }
}

/// Build or tear down the overlay whenever its structure changes.
#[allow(clippy::too_many_arguments)]
pub(crate) fn sync_menu_ui(
    mut commands: Commands,
    faces: Res<TextFaces>,
    menu: Res<MenuState>,
    runtime: Option<Res<Runtime>>,
    settings: Option<Res<SettingsState>>,
    credentials: Option<Res<CredentialsState>>,
    asset_server: Option<Res<AssetServer>>,
    read_log: Option<Res<crate::read_log::ReadLogState>>,
    bodies: Query<&ScrollPosition, With<TimelineBody>>,
    mut watch: Option<ResMut<AssetWatch>>,
    mut built: ResMut<MenuBuilt>,
    roots: Query<Entity, With<MenuRoot>>,
) {
    let signature = MenuBuilt {
        open: menu.open,
        view: Some(menu.view),
        mode: Some(menu.mode),
        confirm: menu.confirm,
        picker: menu.picker,
        slots: menu.slots.clone(),
        legacy_importable: menu.legacy_importable,
        continue_line: menu.continue_line.clone(),
        continue_enabled: menu.continue_enabled,
        settings_origin: Some(menu.settings_origin),
        gallery_origin: Some(menu.gallery_origin),
        gallery_still: menu.gallery_still,
    };
    if *built == signature {
        return;
    }

    // Opening or cancelling a confirm rebuilds the overlay. Only that change
    // carries the Timeline body's offset across the rebuild; any other rebuild
    // starts at the top (`docs/title-and-menu.md` § Timeline §3, `### Layout`).
    // The capture and the restore share this one run: the old body is still in
    // the world while the new tree is spawned through deferred commands.
    let mut without_confirm = signature.clone();
    without_confirm.confirm = built.confirm;
    let restored_scroll = if without_confirm == *built && built.confirm != signature.confirm {
        bodies.iter().next().map(|position| position.0)
    } else {
        None
    };

    for entity in &roots {
        commands.entity(entity).despawn();
    }
    if menu.open {
        if let Some(runtime) = runtime {
            let watch = watch.as_deref_mut();
            match menu.view {
                MenuView::Title => build_title(
                    &mut commands,
                    &faces,
                    &menu,
                    &runtime,
                    asset_server.as_deref(),
                    watch,
                ),
                MenuView::Settings if menu.settings_origin == menu::MenuOrigin::Title => {
                    build_title_settings(
                        &mut commands,
                        &faces,
                        &menu,
                        &runtime,
                        settings.as_deref(),
                        credentials.as_deref(),
                        asset_server.as_deref(),
                        watch,
                    );
                }
                MenuView::Gallery if menu.gallery_origin == menu::MenuOrigin::Title => {
                    build_title_gallery(
                        &mut commands,
                        &faces,
                        &menu,
                        &runtime,
                        asset_server.as_deref(),
                        watch,
                    );
                }
                _ => build_overlay(
                    &mut commands,
                    &faces,
                    &menu,
                    &runtime,
                    settings.as_deref(),
                    credentials.as_deref(),
                    asset_server.as_deref(),
                    watch,
                    read_log.as_deref(),
                    restored_scroll,
                ),
            }
        }
    }
    *built = signature;
}

/// Repaint the focus highlight on every control. Focus is always visible: the
/// focused control gets an accent border in every state, including inside an
/// open confirmation.
pub(crate) fn sync_menu_focus(
    menu: Res<MenuState>,
    mut buttons: Query<(&MenuAction, &mut BorderColor)>,
) {
    if !menu.open {
        return;
    }
    for (action, mut border) in &mut buttons {
        let color = focus_border(&menu, *action);
        if border.top != color {
            *border = BorderColor::all(color);
        }
    }
}

/// Register the presentation assembly.
///
/// Every owner registers its own group — its systems with their intrinsic run
/// conditions and ordering edges; the composition below keeps the `Update`
/// phase order the app has always had. The app-level set chain and the
/// ownership-gated edit drain stay in `lib.rs`.
pub(crate) fn register(app: &mut App) {
    app.init_resource::<MenuBuilt>().add_systems(
        Update,
        (sync_menu_ui, sync_menu_focus)
            .chain()
            .in_set(PresentationSet),
    );
    play::register(app);
    title::register(app);
    gallery::register(app);
    timeline::register(app);
    settings_view::register(app);
}
