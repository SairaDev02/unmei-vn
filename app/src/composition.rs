//! Shared shipping plugin selection for the binary and strict schedule gate.
//!
//! Both constructors return fresh, unrun apps. One private `DefaultPlugins`-
//! derived owner preserves upstream order and owns the production overrides and
//! coverage-preserving headless delta. Process inputs and resource construction
//! remain caller-owned; neither constructor runs frames or decides `AppExit`.

use std::path::Path;

use bevy::app::PluginGroup;
use bevy::asset::AssetPlugin;
use bevy::image::TextureAtlasPlugin;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::window::WindowPlugin;

use crate::failure::{app_log_layer, SessionLogResult};
use crate::{RuntimeConfig, UnmeiPlugin};

struct Selection {
    group: bevy::app::PluginGroupBuilder,
}

impl Selection {
    fn upstream() -> Self {
        Self {
            group: DefaultPlugins.build(),
        }
    }

    fn shipping_overrides(self, window: Window, asset_root: &Path) -> Self {
        Self {
            group: self
                .group
                .set(LogPlugin {
                    custom_layer: app_log_layer,
                    ..default()
                })
                .set(AssetPlugin {
                    file_path: asset_root.to_string_lossy().into_owned(),
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(window),
                    ..default()
                }),
        }
    }

    // Supported Windows/Linux features guarantee these slots in the locked
    // upstream group. `disable` preserves their ordering positions.
    //
    // Exclude extras the curated gate never installed: in particular Light
    // registers gizmo systems even without GizmoPlugin's required assets.
    // Sprite/Animation must stay excluded to keep the private upstream
    // IgnoreAmbiguitiesPlugin's acceptance branches inactive. Audit its source
    // and the slot/feature census on dependency upgrades.
    fn headless_subtraction(self) -> Self {
        Self {
            group: self
                .group
                .disable::<LogPlugin>()
                .disable::<bevy::app::PanicHandlerPlugin>()
                .disable::<bevy::app::TerminalCtrlCHandlerPlugin>()
                .disable::<bevy::a11y::AccessibilityPlugin>()
                .disable::<bevy::transform::TransformPlugin>()
                .disable::<bevy::diagnostic::DiagnosticsPlugin>()
                .disable::<bevy::winit::WinitPlugin>()
                .disable::<bevy::render::RenderPlugin>()
                .disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>()
                .disable::<bevy::core_pipeline::CorePipelinePlugin>()
                .disable::<bevy::post_process::PostProcessPlugin>()
                .disable::<bevy::anti_alias::AntiAliasPlugin>()
                .disable::<bevy::sprite::SpritePlugin>()
                .disable::<bevy::sprite_render::SpriteRenderPlugin>()
                .disable::<bevy::ui_render::UiRenderPlugin>()
                .disable::<bevy::gltf::GltfPlugin>()
                .disable::<bevy::pbr::PbrPlugin>()
                .disable::<bevy::gilrs::GilrsPlugin>()
                .disable::<bevy::animation::AnimationPlugin>()
                .disable::<bevy::gizmos::GizmoPlugin>()
                .disable::<bevy::gizmos_render::GizmoRenderPlugin>()
                // Text re-adds ClipboardPlugin when absent, retaining the
                // curated gate's relative order and upstream prerequisites.
                .disable::<bevy::clipboard::ClipboardPlugin>()
                .disable::<bevy::mesh::MeshPlugin>()
                .disable::<bevy::camera::CameraPlugin>()
                .disable::<bevy::light::LightPlugin>()
                .disable::<bevy::world_serialization::WorldSerializationPlugin>()
                .disable::<bevy::scene::ScenePlugin>()
                .disable::<bevy::state::app::StatesPlugin>(),
        }
    }

    fn with_atlas(self) -> Self {
        Self {
            // Sprite normally nests atlas support. Supply it at the upstream
            // slot before Text rather than silently dropping gate coverage.
            group: self
                .group
                .add_before::<bevy::text::TextPlugin>(TextureAtlasPlugin),
        }
    }

    fn with_headless_asset_root(self, asset_root: &Path) -> Self {
        Self {
            group: self.group.set(AssetPlugin {
                file_path: asset_root.to_string_lossy().into_owned(),
                ..default()
            }),
        }
    }

    fn finish(self, app: &mut App) {
        app.add_plugins(self.group);
    }
}

/// Build a fresh, unrun shipping app with the supplied configuration/log result,
/// configured upstream plugins, kira audio, startup insertions and UnmeiPlugin.
///
/// The caller resolves all process/environment/secret inputs and opens the log.
/// `install_startup` must only insert already-constructed resources: no I/O,
/// plugin installation or RuntimeConfig replacement. This is a documented
/// caller contract, not type-enforced closure policing.
pub fn shipping_app(
    config: RuntimeConfig,
    window: Window,
    session_log: SessionLogResult,
    install_startup: impl FnOnce(&mut App),
) -> App {
    let asset_root = config.pack_root.join("assets");
    let mut app = App::new();
    app.insert_resource(config);
    match session_log {
        SessionLogResult::Opened(log) => {
            app.insert_resource(log);
        }
        SessionLogResult::Unwritable(error) => {
            app.insert_resource(error);
        }
        SessionLogResult::NotAttempted => {}
    }
    Selection::upstream()
        .shipping_overrides(window, &asset_root)
        .finish(&mut app);
    app.add_plugins(bevy_kira_audio::AudioPlugin);
    install_startup(&mut app);
    app.add_plugins(UnmeiPlugin);
    app
}

/// Build a fresh, unrun headless schedule app from the shared selection.
///
/// Renderer/window backend/global logger and their dependants are excluded.
/// This app is update-only, without a runtime schedule runner; the caller sets
/// strict ScheduleBuildSettings, executes frames and owns assertions.
pub fn schedule_app(config: RuntimeConfig) -> App {
    let asset_root = config.pack_root.join("assets");
    let mut app = App::new();
    app.insert_resource(config);
    Selection::upstream()
        .with_headless_asset_root(&asset_root)
        .headless_subtraction()
        .with_atlas()
        .finish(&mut app);
    app.add_plugins(bevy_kira_audio::AudioPlugin);
    app.add_plugins(UnmeiPlugin);
    app
}
