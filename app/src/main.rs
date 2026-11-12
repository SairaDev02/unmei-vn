//! `unmei-play` — play a content pack in the Bevy runtime.
//!
//! Usage: `unmei-play [--dev] [PACK_DIR]` (default `demo-pack` in the folder
//! containing the executable). `-V`/`--version` prints the release version. The
//! launch grammar and the developer help live in [`unmei_app::launch`].
//!
//! Controls: Space/Enter/left-click advance · 1-9 or click choose ·
//! A auto · F11 fullscreen · Esc menu. F5/F9 are developer save/load shortcuts
//! to manual slot 1, gated by an explicit `--dev` run: they are silent no-ops
//! unless dev mode is on (the normal debug/test build, or `--dev` in release).
//! Decisions come from OpenRouter when a key is
//! available — `OPENROUTER_API_KEY`, or the user-level credential file it
//! overrides — and without one every decision point falls back to the pack's
//! author default, so the demo plays fully offline.
//!
//! The session log `<pack-dir>.log` is opened (and truncated) before the pack
//! load and its first line names the release version, so the startup error
//! screen's pointer is always valid (ADR 0008). When the executable's own
//! location is unknown there is no pack to sit beside, so no log is opened and
//! the screen points at none.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use bevy::app::AppExit;
use bevy::prelude::*;
use unmei_app::composition::shipping_app;
use unmei_app::failure::{
    session_log_path_for, LogOpenError, SessionLog, SessionLogResult, StartupFailure,
};
use unmei_app::launch::{self, LaunchRequest};
use unmei_app::{
    credentials, settings, DecisionFactory, KeyCheckChannel, ResolverBuilder, RuntimeConfig,
};
use unmei_engine::{DecisionResolver, DecisionsClient, HttpKeyCheck};

fn main() -> AppExit {
    // Resolve the launch contract before any startup work: `-h`/`--help`,
    // `-V`/`--version` and usage errors exit here, so they never read
    // credentials/settings, open or truncate the session log, or build the
    // App/window.
    let cli = match launch::parse(std::env::args_os().skip(1)) {
        Ok(LaunchRequest::Help) => {
            print!("{}", launch::help());
            let _ = std::io::stdout().flush();
            return AppExit::Success;
        }
        Ok(LaunchRequest::Version) => {
            println!("{}", launch::version_text());
            let _ = std::io::stdout().flush();
            return AppExit::Success;
        }
        Ok(LaunchRequest::Run(launch)) => launch,
        Err(error) => {
            eprintln!("{error}");
            eprintln!();
            eprint!("{}", launch::USAGE);
            let _ = std::io::stderr().flush();
            return AppExit::from_code(2);
        }
    };

    // The default pack is a separate step from the grammar: it is the one place
    // the executable's own folder is consulted, and it applies `demo-pack`
    // beside it in every build with no working-directory fallback. An explicit
    // PACK_DIR never takes this step.
    let dev_mode = cli.dev_mode();
    let resolved = match launch::resolve_pack(cli.pack_dir, launch::executable_dir()) {
        Ok(resolved) => resolved,
        Err(launch::ExecutableUnknown) => return startup_without_pack(dev_mode),
    };

    // One optional positional PACK_DIR, best-effort canonicalized with the
    // original retained on failure (a missing/broken pack is a startup error,
    // not a usage error).
    let pack_root = resolved.canonical_pack_root();

    // The offline posture is normal, not a fault: no key just means every
    // decision point takes its authored fallback (failure row 10 is logged per
    // decision, through the one funnel). A non-empty `OPENROUTER_API_KEY`
    // overrides the user-level credential file (ADR 0010); a shape-invalid
    // stored value logs once and builds nothing.
    let config_dir = credentials::default_config_dir();
    let env_key = std::env::var(credentials::ENV_KEY).ok();
    // One resolver builder serves both launch resolution and an in-session
    // rebuild after a key commit (`docs/title-and-menu.md` § Decisions settings
    // §2). The endpoint override is captured once.
    let endpoint = std::env::var(unmei_engine::decision::client::ENDPOINT_ENV)
        .unwrap_or_else(|_| unmei_engine::decision::DEFAULT_ENDPOINT.to_string());
    let factory: ResolverBuilder = {
        let endpoint = endpoint.clone();
        Arc::new(move |key: &str| {
            Arc::new(DecisionsClient::new(endpoint.clone(), key)) as Arc<dyn DecisionResolver>
        })
    };
    let credential_launch =
        credentials::resolve_launch(config_dir.as_deref(), env_key.as_deref(), |key| {
            factory(key)
        });

    // Build the runtime config now, and insert it before `DefaultPlugins`: the
    // one app-lifetime dev-mode value must be available before `LogPlugin`
    // installs the custom-layer hook (the later Monitor capture reads it there)
    // and before the runtime plugins read it for the F5/F9 gate.
    let runtime_config = RuntimeConfig::new(pack_root.clone(), credential_launch.resolver)
        .with_dev_mode(dev_mode)
        .with_pack_origin(resolved.origin);

    // Open/truncate the session log *before* the pack load, so the error
    // screen's log pointer is valid even for a broken pack.
    let log_path = session_log_path_for(&pack_root);
    let session_log = match SessionLog::open(log_path.clone(), &launch::version_text()) {
        Ok(log) => SessionLogResult::Opened(log),
        // Reported once at startup as row 21; play continues console-only.
        Err(error) => SessionLogResult::Unwritable(LogOpenError {
            path: log_path,
            message: error.to_string(),
        }),
    };

    // Read the settings once, before the app exists. `play.fullscreen` decides
    // the window mode at creation (no windowed first frame), and the loaded
    // values plus any malformed-file warning ride into the plugin, which emits
    // the warning from its startup system — after `LogPlugin` has installed.
    let loaded = settings::load(&pack_root);
    let window = Window {
        mode: settings::window_mode(loaded.settings.play.fullscreen),
        ..settings::window_config()
    };

    let startup_settings = settings::StartupSettings(loaded);
    let startup_credentials = credentials::StartupCredentials {
        state: credential_launch.state,
        warning: credential_launch.warning,
    };
    // Only the shipping entry point reaches the OS clipboard.
    let clipboard = credentials::ClipboardPort::System;
    // Test worlds keep the hermetic None defaults for both ports.
    let key_check = KeyCheckChannel {
        checker: Some(Arc::new(HttpKeyCheck::new())),
    };
    let decision_factory = DecisionFactory {
        builder: Some(factory),
    };

    let mut app = shipping_app(runtime_config, window, session_log, |app| {
        app.insert_resource(startup_settings)
            .insert_resource(startup_credentials)
            .insert_resource(clipboard)
            .insert_resource(key_check)
            .insert_resource(decision_factory);
    });

    // The logger exists now, so a panic from here on also reaches
    // `<pack-dir>.log`, the file a bug report carries, and not only the console.
    unmei_app::panic_log::install();

    // Returning `AppExit` (which implements `Termination`) is what carries the
    // error screen's non-zero code back to the process — the app never calls a
    // hard process exit.
    app.run()
}

/// Start the one error screen that exists before any pack is known: no
/// `PACK_DIR` was given and this executable cannot find its own folder, so
/// `demo-pack` beside it cannot be derived. The same text goes to `stderr`, no
/// session log is attempted (there is no pack for one to sit beside), and no
/// settings or Decisions work happens — nothing can be played from here.
fn startup_without_pack(dev_mode: bool) -> AppExit {
    eprintln!("{}", launch::NO_EXECUTABLE_MESSAGE);
    let _ = std::io::stderr().flush();

    // Inert: the pack load is skipped because the failure is already recorded,
    // so this path is never read. Settings are preloaded as defaults for the
    // same reason — there is no pack directory to read a file from.
    // The bare name is working-directory-relative on purpose and is never
    // resolved: this path exists only because composition needs a pack root.
    let config =
        RuntimeConfig::new(PathBuf::from(launch::DEFAULT_PACK_NAME), None).with_dev_mode(dev_mode);
    let mut app = shipping_app(
        config,
        settings::window_config(),
        SessionLogResult::NotAttempted,
        |app| {
            app.insert_resource(StartupFailure::NoExecutable)
                .insert_resource(settings::StartupSettings(settings::LoadedSettings {
                    settings: settings::Settings::default(),
                    warning: None,
                }));
        },
    );
    unmei_app::panic_log::install();
    app.run()
}
