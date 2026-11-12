//! Shipping-composition schedule gate, driven by the production composition
//! owner.
//!
//! The `PostUpdate` cycle the gate protects only existed once UI, text, input
//! focus and the ownership-gated drain were installed together. Narrower
//! headless harnesses did not construct that combination.
//!
//! This test calls the real [`schedule_app`] constructor shared with the
//! shipping binary, with dev mode on. Its claim is strict executable schedules
//! and real pack startup, not renderer/window-backend or shipping boot proof.
//! Existing third-party ambiguity acceptances remain in [`unmei_app::UnmeiPlugin`];
//! they are not broadened by weakening the gate.

use std::path::PathBuf;

use bevy::ecs::schedule::{LogLevel, ScheduleBuildSettings};
use bevy::prelude::*;

use unmei_app::composition::schedule_app;
use unmei_app::failure::StartupFailure;
use unmei_app::{Runtime, RuntimeConfig};

fn demo_pack() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/demo-pack")
        .canonicalize()
        .expect("the demo pack exists")
}

#[test]
fn the_shipping_composition_builds_clean_schedules_and_runs_frames() {
    let mut app = schedule_app(RuntimeConfig::new(demo_pack(), None).with_dev_mode(true));
    app.configure_schedules(ScheduleBuildSettings {
        ambiguity_detection: LogLevel::Error,
        ..default()
    });

    // Presence checks supplement the executed frames and protect the delta's
    // coverage and private upstream suppression prerequisites.
    assert!(
        !app.is_plugin_added::<bevy::winit::WinitPlugin>(),
        "the window backend must be absent from the headless gate"
    );
    assert!(
        !app.is_plugin_added::<bevy::render::RenderPlugin>(),
        "the renderer must be absent from the headless gate"
    );
    assert!(
        !app.is_plugin_added::<bevy::sprite::SpritePlugin>(),
        "SpritePlugin must stay excluded so the private suppression stays inactive"
    );
    assert!(
        app.is_plugin_added::<bevy::audio::AudioPlugin>(),
        "Bevy audio must be present exactly once from the shared selection"
    );
    assert!(
        app.is_plugin_added::<bevy::image::ImagePlugin>(),
        "image/atlas support must be retained"
    );
    assert!(
        app.is_plugin_added::<bevy::image::TextureAtlasPlugin>(),
        "the atlas support SpritePlugin used to nest must be supplied explicitly"
    );

    for _ in 0..3 {
        app.update();
    }

    assert!(
        app.world().get_resource::<Runtime>().is_some(),
        "the demo pack must load in the shipping composition"
    );
    assert!(
        app.world().get_resource::<StartupFailure>().is_none(),
        "the demo pack must not produce a startup failure"
    );
}
