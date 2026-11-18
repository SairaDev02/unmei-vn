//! Pack load and the boot park: the one place `Runtime` is created.

use bevy::prelude::*;
use unmei_engine::runner::RunnerError;
use unmei_engine::save::StagingState;
use unmei_engine::{load_pack, ScriptRunner};

use crate::failure::{self, DefensiveHold, Failure, FailureRow, Notice, StartupFailure};
use crate::menu::MenuState;
use crate::Runtime;
use crate::RuntimeConfig;

/// Load the pack and start the runner. A bad pack no longer exits: record the
/// canonicalised path, the whole error list and how the path was chosen for the
/// startup error screen, report it once (row 1), and leave `Runtime` absent so
/// the play path stays shut.
pub(crate) fn setup_pack(
    mut commands: Commands,
    config: Res<RuntimeConfig>,
    mut notice: ResMut<Notice>,
    mut hold: ResMut<DefensiveHold>,
    mut menu: ResMut<MenuState>,
    mut read_log: ResMut<crate::read_log::ReadLogState>,
) {
    let loaded = match load_pack(&config.pack_root) {
        Ok(loaded) => loaded,
        Err(errors) => {
            let pack_root = config
                .pack_root
                .canonicalize()
                .unwrap_or_else(|_| config.pack_root.clone());
            let detail = errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n");
            failure::report(
                Failure::new(
                    FailureRow::PackLoad,
                    format!("pack `{}`:\n{detail}", pack_root.display()),
                ),
                &mut notice,
                &mut hold,
            );
            commands.insert_resource(StartupFailure::Pack {
                pack_root,
                errors,
                defaulted: config.pack_origin == crate::launch::PackOrigin::Defaulted,
            });
            return;
        }
    };
    for warning in &loaded.warnings {
        // Row 2: a real but non-fatal pack warning — logged, never shown.
        failure::report(
            Failure::new(FailureRow::PackWarning, warning.to_string()),
            &mut notice,
            &mut hold,
        );
    }

    let pack_root = loaded.pack.root.clone();
    let asset_root = config.asset_root.clone();
    let mut runtime = Runtime {
        runner: ScriptRunner::new(loaded.pack),
        staging: StagingState::default(),
        pack_root,
        asset_root,
    };

    // The boot run: carry the entry scene to its first wait and apply its
    // leading commands, so the staging behind the title is the opening frame.
    // It deliberately does **not** arm an autosave or dispatch a Decision —
    // those are `drive_script`'s jobs and stay gated behind the title.
    if let Err(error) = park_at_entry(&mut runtime) {
        // Row 13 — defensive: the entry scene's first run never parks. The
        // title still shows; the hold keeps play from stepping the runner.
        failure::report_runner_call(error, &mut notice, &mut hold);
    }

    // Read the log exactly once, after the pack is known good. A rejected pack
    // returned above and never reads or writes the file.
    crate::read_log::load_once(runtime.runner.pack(), &mut read_log, &mut notice, &mut hold);

    // The first frame after a successful load is the title; it reads the slots
    // once for Continue's line and enablement. Only this boot entry announces
    // the title: a later `Exit to title` returns to a screen already shown.
    menu.enter_title(runtime.runner.pack(), runtime.runner.scene_id());
    crate::session_lines::title_ready();

    commands.insert_resource(runtime);
    commands.insert_resource(super::DecisionChannel {
        resolver: config.decisions.clone(),
    });
}

/// Rebuild the runner at the pack's entry scene with its declared initial
/// state, run it once to its first wait, and apply the leading staging. Shared
/// by the boot load and New Game's reset, so the two cannot drift; it arms no
/// autosave and dispatches no Decision. A run that never parks
/// ([`RunnerError::WaitFreeCycle`]) applies no staging and is returned for the
/// caller to report as row 13.
pub(crate) fn park_at_entry(runtime: &mut Runtime) -> Result<(), RunnerError> {
    let pack = runtime.runner.pack().clone();
    runtime.runner = ScriptRunner::new(pack);
    runtime.staging = StagingState::default();
    let outcome = runtime.runner.run()?;
    runtime.staging.apply_all(&outcome.staged);
    Ok(())
}
