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

    /// The slice-2 render selection (spec §13): the shipping plugin group with
    /// no window backend, the renderer pinned to the fallback (software)
    /// adapter, and rendering unpipelined so a readback belongs to the frame the
    /// take asked for. A take supplies its own image target and never presents.
    fn rendered(self) -> Self {
        let backends = wgpu::Backends::from_env().unwrap_or(wgpu::Backends::all());
        Self {
            group: self
                .group
                .disable::<bevy::winit::WinitPlugin>()
                .disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>()
                .disable::<bevy::gilrs::GilrsPlugin>()
                // As in the windowless take, the global logger stays out: the
                // take's session log is the funnel's own file, and the
                // renderer's console lines would otherwise reach stderr and
                // that file.
                .disable::<LogPlugin>()
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: bevy::window::ExitCondition::DontExit,
                    close_when_requested: false,
                    ..default()
                })
                .set(bevy::render::RenderPlugin {
                    render_creation: bevy::render::settings::RenderCreation::Automatic(Box::new(
                        bevy::render::settings::WgpuSettings {
                            backends: Some(backends),
                            force_fallback_adapter: true,
                            ..default()
                        },
                    )),
                    synchronous_pipeline_compilation: true,
                    ..default()
                }),
        }
    }

    /// A take opens no audio device (spec §4.1). Bevy's own audio plugin
    /// opens the default output when it builds, and on a machine without one
    /// ALSA prints straight to stderr. The player's audio is Kira's, which a
    /// take leaves out too, so nothing in the player needs this plugin.
    fn without_audio_device(self) -> Self {
        Self {
            group: self.group.disable::<bevy::audio::AudioPlugin>(),
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

    /// The take's log: the session-log layer that follows the current take's
    /// sandbox file, reinstalled where `headless_subtraction` removed the
    /// console-only default.
    fn with_keiko_log(self) -> Self {
        Self {
            group: self.group.set(LogPlugin {
                custom_layer: crate::failure::keiko_log_layer,
                ..default()
            }),
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
    let asset_root = config.asset_root.clone();
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

/// Build a fresh, unrun Keiko take app (spec §4.1): the headless selection plus
/// the take's own session log, sandbox config, clipboard, key check, resolver
/// factory and manual clock. It has no audio device: `UnmeiPlugin` registers the
/// play surface's channel resources and the track decoder without
/// `bevy_kira_audio`'s plugin, so the mix and staging bookkeeping run and
/// nothing sounds.
///
/// It is deliberately not a wrapper around [`schedule_app`]: a take needs the
/// session-log layer that the strict schedule gate leaves out, and the real
/// image loader that a windowless app otherwise never registers (`ImagePlugin`
/// only preregisters it, and a renderer supplies the real one in play).
pub fn keiko_app(
    config: RuntimeConfig,
    session_log: SessionLogResult,
    clipboard: crate::credentials::ClipboardPort,
    key_check: crate::KeyCheckChannel,
    decision_factory: crate::DecisionFactory,
    asset_faults: std::sync::Arc<crate::keiko::assets::AssetFaults>,
) -> App {
    let mut app = take_app(
        config,
        session_log,
        clipboard,
        key_check,
        decision_factory,
        asset_faults,
    );
    let asset_root = app.world().resource::<RuntimeConfig>().asset_root.clone();
    Selection::upstream()
        .headless_subtraction()
        .without_audio_device()
        .with_atlas()
        .with_headless_asset_root(&asset_root)
        .with_keiko_log()
        .finish(&mut app);
    // A windowless app never registers the real image loader (`ImagePlugin`
    // only preregisters it, and a renderer supplies it in play).
    app.register_asset_loader(bevy::image::ImageLoader::new(
        bevy::image::CompressedImageFormats::NONE,
    ));
    app.add_plugins(UnmeiPlugin);
    app
}

/// Build a take app that draws (spec §13): the player's full plugin selection,
/// minus the window backend, with the renderer pinned to the software
/// rasterizer and rendering itself unpipelined, so a readback belongs to the
/// frame the take asked for.
///
/// The caller checks [`crate::keiko::render::available`] first: without a
/// fallback adapter Bevy's renderer panics, so a take that cannot draw keeps
/// today's windowless app and fails its `frame` and `click` steps instead.
pub fn keiko_app_rendered(
    config: RuntimeConfig,
    session_log: SessionLogResult,
    clipboard: crate::credentials::ClipboardPort,
    key_check: crate::KeyCheckChannel,
    decision_factory: crate::DecisionFactory,
    asset_faults: std::sync::Arc<crate::keiko::assets::AssetFaults>,
) -> App {
    let mut app = take_app(
        config,
        session_log,
        clipboard,
        key_check,
        decision_factory,
        asset_faults,
    );
    let asset_root = app.world().resource::<RuntimeConfig>().asset_root.clone();
    Selection::upstream()
        .rendered()
        .without_audio_device()
        .with_headless_asset_root(&asset_root)
        .with_keiko_log()
        .finish(&mut app);
    app.add_plugins(UnmeiPlugin);
    app
}

/// The parts every take app shares: the sandbox asset source, the config, the
/// session log and the take's own ports, all inserted before the selection so
/// the asset plugin sees the reader that carries the fault inventory.
fn take_app(
    config: RuntimeConfig,
    session_log: SessionLogResult,
    clipboard: crate::credentials::ClipboardPort,
    key_check: crate::KeyCheckChannel,
    decision_factory: crate::DecisionFactory,
    asset_faults: std::sync::Arc<crate::keiko::assets::AssetFaults>,
) -> App {
    let asset_root = config.asset_root.clone();
    let mut app = App::new();
    // The Keiko asset reader replaces the default source, so the two asset
    // faults act where the player reads its files and nowhere else.
    let mut sources = bevy::asset::io::AssetSourceBuilders::default();
    let reader_root = asset_root.clone();
    sources.insert(
        bevy::asset::io::AssetSourceId::Default,
        bevy::asset::io::AssetSourceBuilder::new(move || {
            Box::new(crate::keiko::assets::KeikoAssetReader::new(
                reader_root.clone(),
                asset_faults.clone(),
            ))
        }),
    );
    app.insert_resource(sources);
    app.insert_resource(config);
    match session_log {
        SessionLogResult::Opened(log) => {
            crate::failure::set_keiko_log(Some(log.writer()));
            app.insert_resource(log);
        }
        SessionLogResult::Unwritable(error) => {
            crate::failure::set_keiko_log(None);
            app.insert_resource(error);
        }
        SessionLogResult::NotAttempted => crate::failure::set_keiko_log(None),
    }
    app.insert_resource(clipboard)
        .insert_resource(key_check)
        .insert_resource(decision_factory)
        // The take's own funnel recorder: only this app's failure lines land
        // in it, whatever else runs in the process.
        .init_resource::<crate::failure::TapLog>();
    app
}

/// Build a fresh, unrun headless schedule app from the shared selection.
///
/// Renderer/window backend/global logger and their dependants are excluded.
/// This app is update-only, without a runtime schedule runner; the caller sets
/// strict ScheduleBuildSettings, executes frames and owns assertions.
pub fn schedule_app(config: RuntimeConfig) -> App {
    let asset_root = config.asset_root.clone();
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
