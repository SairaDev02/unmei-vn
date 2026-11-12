//! Real-Bevy rendered acceptance walk for the Monitor drawer.
//!
//! This is the assembled-product gate the browser prototype cannot provide. It
//! boots the **shipping** [`UnmeiPlugin`] on a real window and GPU at 1280×720,
//! drives a purpose-built fixture pack through every runtime surface with real
//! input messages, and measures the real text metrics, layout and palette of
//! the drawer on each one. Screenshots land in `target/monitor-acceptance/`.
//!
//! Four modes:
//!
//! ```sh
//! cargo run -p unmei-app --example monitor_acceptance                  # surfaces + fixtures
//! cargo run -p unmei-app --example monitor_acceptance -- --mode hold   # defensive hold
//! cargo run -p unmei-app --example monitor_acceptance -- --mode broken # startup error (exits 1)
//! cargo run -p unmei-app --example monitor_acceptance -- --mode gate   # the dev gate
//! ```
//!
//! `--dev` resolves the effective value through the shipping launch parser (so
//! a release run exercises the binary's own force-on mapping); `--no-dev`
//! applies the explicit construction-time off override.
//!
//! The walk uses **no credentials and no network**: the one `decide` point
//! answers through a gated resolver the walk releases itself, exactly like the
//! headless suites' fake. Every fixture event is a real `log` record through the
//! installed `LogPlugin` subscriber, so it travels the production Monitor
//! capture layer.
//!
//! Human confirmation of this walk was recorded on 2026-10-12: a person
//! watched the walk and exercised the manual live drawer pass over every action.
//! The printed measurements and screenshots are the reproducible evidence
//! beside that confirmation.
//!
//! The `broken` mode deliberately ends with the app's own non-zero startup-error

mod acts;
mod anchor;
mod checks;
mod fixture;
mod harness;
mod input;
mod mode;
mod plan;
mod prelude;
mod rows;
mod settings;
mod status;
mod style;
mod walk;

use std::sync::Arc;

use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::window::WindowResolution;

use unmei_app::failure::{app_log_layer, session_log_path_for, SessionLog};
use unmei_app::{RuntimeConfig, UnmeiPlugin};
use unmei_engine::DecisionResolver;

use input::release_button;
use mode::{parsed_dev_mode, GatedResolver, Mode};
use walk::{Harness, Queued};

fn main() -> AppExit {
    let mode = Mode::parse();
    let gate = GatedResolver::new();
    let (pack, resolver): (std::path::PathBuf, Option<Arc<dyn DecisionResolver>>) = match mode {
        Mode::Broken => (fixture::write_broken_pack(), None),
        Mode::Walk => (fixture::write_pack(), Some(Arc::new(gate.clone()))),
        Mode::Hold | Mode::Gate => (fixture::write_pack(), None),
    };

    // `--dev` resolves through the **shipping launch parser**, so a release run
    // of this example exercises the binary's own force-on mapping. Without it,
    // the walk modes use the explicit construction-time override (the drawer
    // must exist to be walked), while `--mode gate` deliberately leaves the
    // build's own `cfg!(debug_assertions)` default in place.
    let force_dev = std::env::args().any(|arg| arg == "--dev");
    let force_off = std::env::args().any(|arg| arg == "--no-dev");
    let dev_mode: Option<bool> = if force_dev {
        Some(parsed_dev_mode())
    } else if force_off {
        // The explicit construction-time override, off, in any build. Only
        // meaningful for `--mode gate`; the walk modes need the drawer.
        Some(false)
    } else if mode == Mode::Gate {
        None
    } else {
        Some(true)
    };

    let mut app = App::new();
    let mut config = RuntimeConfig::new(pack.clone(), resolver);
    if let Some(dev_mode) = dev_mode {
        config = config.with_dev_mode(dev_mode);
    }
    app.insert_resource(config);
    // The session log beside the pack, exactly as the entry point opens it —
    // release version on the first line included — so the startup error
    // screen's pointer is truthful and the log can be read back as evidence.
    let log_path = session_log_path_for(&pack);
    match SessionLog::open(log_path.clone(), &unmei_app::launch::version_text()) {
        Ok(log) => {
            app.insert_resource(log);
        }
        Err(error) => {
            eprintln!("WARN: the session log could not be opened: {error}");
        }
    }
    app.add_plugins(
        DefaultPlugins
            .set(LogPlugin {
                custom_layer: app_log_layer,
                ..default()
            })
            .set(bevy::asset::AssetPlugin {
                file_path: pack.join("assets").to_string_lossy().into_owned(),
                ..default()
            })
            .set(bevy::window::WindowPlugin {
                primary_window: Some(Window {
                    resolution: WindowResolution::new(1280, 720).with_scale_factor_override(1.0),
                    ..default()
                }),
                ..default()
            }),
    )
    .add_plugins(bevy_kira_audio::AudioPlugin)
    .add_plugins(UnmeiPlugin)
    .insert_resource(Harness::new(mode, gate, dev_mode))
    .add_systems(Update, walk.after(unmei_app::RuntimeCoreSet));
    app.run()
}

/// The walk itself: an exclusive system so every check can read the whole
/// world's laid-out UI tree.
fn walk(world: &mut World) {
    let mut harness = world
        .remove_resource::<Harness>()
        .expect("the walk owns its harness resource");
    if !harness.finished {
        step_once(world, &mut harness);
    }
    let done = harness.finished;
    let passed = harness.report.failures.is_empty();
    world.insert_resource(harness);
    if done {
        world.write_message(if passed {
            AppExit::Success
        } else {
            AppExit::error()
        });
    }
}

fn step_once(world: &mut World, harness: &mut Harness) {
    // Releases queued by the previous frame's taps.
    for queued in std::mem::take(&mut harness.queued) {
        match queued {
            Queued::Key(code) => harness::release(world, code),
            Queued::Button(button) => release_button(world, button),
        }
    }

    let Some(step) = harness.plan.get(harness.step).cloned() else {
        finish(harness);
        return;
    };

    if harness.frame == 0 {
        println!("\n[{:02}] {}", harness.step, step.name);
    }
    for (frame, act) in step.acts.clone() {
        if harness.frame == frame {
            act(world, harness);
        }
    }
    if let Some(tick) = step.tick {
        tick(world, harness);
    }
    if let Some((frame, name)) = step.shot {
        if harness.frame == frame {
            let path = harness::screenshot(world, name);
            println!("  shot: {}", path.display());
        }
    }

    harness.frame += 1;
    if harness.frame < step.settle {
        return;
    }
    // The gate holds the check (and the step) while a bounded progressive pass
    // is still running. A hard frame cap keeps a broken gate reporting a
    // failure instead of hanging the walk.
    if let Some(until) = step.until {
        if harness.frame < 600 && !until(world, harness) {
            return;
        }
    }

    if let Some(check) = step.check {
        check(world, harness);
    }
    harness.step += 1;
    harness.frame = 0;
}

fn finish(harness: &mut Harness) {
    if harness.finished {
        return;
    }
    harness.finished = true;
    if !harness.report.finish() {
        eprintln!("\nthe rendered acceptance walk failed");
    }
}

// ---------------------------------------------------------------------------
// Physical input helpers
// ---------------------------------------------------------------------------
