//! One take: a sandbox, a fresh windowless app, a manual clock, the settle rule
//! and a panic boundary (spec §4).
//!
//! A take executes one case from its first line to its last. It never touches
//! the original pack, the user's sidecars or a credential file outside its own
//! sandbox.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use unmei_engine::keiko::{Case, Expect, Fault, KeyCheckFault, Step};
use unmei_engine::Pack;

use super::assert::evaluate;
use super::decide::KeikoResolver;
use super::finding::{rules, Finding, Location};
use super::observe::{observe, LogLine, Observation};
use super::sandbox::Sandbox;
use super::steps::{apply, Applied, FRAME};
use crate::failure::{SessionLog, SessionLogResult};
use crate::{Runtime, RuntimeConfig};

mod support;
use support::*;

/// A live run's client, built from the environment key only.
#[derive(Clone)]
pub struct Live {
    pub key: String,
    pub endpoint: String,
}

/// Everything one take needs.
pub struct TakeRequest<'a> {
    pub case: &'a Case,
    pub case_file: String,
    pub original_pack: &'a Path,
    pub pack: &'a Pack,
    pub take_index: usize,
    pub keep: bool,
    pub live: Option<Live>,
    /// The folder a caller chooses for the frames, with each file named by its
    /// step alone (the preview, spec §9.1). `None` keeps §13's folder beside the
    /// real pack and its ordinal names.
    pub frames: Option<PathBuf>,
}

/// What one take produced.
pub struct TakeResult {
    pub outcome: Result<(), Finding>,
    pub observation: Observation,
    /// One observation per settled step, in order.
    pub records: Vec<Observation>,
    /// Every funnel line the take wrote.
    pub log: Vec<LogLine>,
    pub kept: Option<PathBuf>,
    pub live: bool,
}

/// Run one take.
pub fn run(request: TakeRequest) -> TakeResult {
    run_with(
        request,
        #[cfg(test)]
        Vec::<NoExtraPlugin>::new(),
    )
}

/// The take's one body. A test build adds extra plugins to the composed app
/// before its first frame, which is how a lib test reaches the rules a shipping
/// take cannot install (spec §15 rules 23 and 24). The parameter does not exist
/// in a non-test build, so the player and `unmei-play` gain no fault hook.
fn run_with(request: TakeRequest, #[cfg(test)] plugins: Vec<impl Plugin>) -> TakeResult {
    let sandbox = match Sandbox::create(request.original_pack, request.take_index) {
        Ok(sandbox) => sandbox,
        Err(error) => {
            return TakeResult {
                outcome: Err(Finding::new(
                    request.case_file,
                    Location::File,
                    rules::CASE_REFERENCE,
                    format!("cannot create the take sandbox: {error}"),
                )),
                observation: super::observe::empty(),
                records: Vec::new(),
                log: Vec::new(),
                kept: None,
                live: false,
            }
        }
    };

    let setup = prepare(&sandbox, request.case, request.original_pack);
    if let Err(finding) = setup {
        let kept = sandbox.finish(request.keep);
        return TakeResult {
            outcome: Err(finding),
            observation: super::observe::empty(),
            records: Vec::new(),
            log: Vec::new(),
            kept,
            live: false,
        };
    }

    let log = SessionLog::open(sandbox.session_log_path(), &crate::launch::version_text());
    let session_log = match log {
        Ok(log) => SessionLogResult::Opened(log),
        Err(error) => SessionLogResult::Unwritable(crate::failure::LogOpenError {
            path: sandbox.session_log_path(),
            message: error.to_string(),
        }),
    };

    let clock = Arc::new(AtomicU64::new(0));
    let resolver = build_resolver(&request, clock.clone());
    let live_marked = resolver.live_marked.clone();
    let key_check_release = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let key_check = crate::KeyCheckChannel {
        checker: Some(Arc::new(KeikoKeyCheck::new(
            &request.case.headers,
            key_check_release.clone(),
        ))),
    };
    let faults: Vec<Fault> = request
        .case
        .headers
        .iter()
        .filter_map(|line| match &line.header {
            unmei_engine::keiko::Header::Fault { fault } => Some(fault.clone()),
            _ => None,
        })
        .collect();
    let asset_faults = Arc::new(super::assets::AssetFaults::of(faults.iter()));
    // The launch credential read is the read-only seam `main` uses, pointed at
    // the take's own config dir: an `env-key` fixture wins, then a `credential`
    // fixture, and an absent file is the offline posture (spec §7.1).
    let launch = crate::credentials::resolve_launch(
        Some(&sandbox.credential_dir()),
        sandbox.env_key().as_deref(),
        |key| {
            let endpoint = request
                .live
                .as_ref()
                .map(|live| live.endpoint.clone())
                .unwrap_or_else(|| unmei_engine::DEFAULT_ENDPOINT.to_string());
            super::decide::live_client(&endpoint, key)
        },
    );
    let config = RuntimeConfig::new(
        sandbox.pack_root().to_path_buf(),
        Some(resolver.clone() as Arc<dyn unmei_engine::DecisionResolver>),
    )
    .with_dev_mode(request.case.dev())
    .with_asset_root(sandbox.asset_root().to_path_buf());

    let mut state = TakeState {
        observation: super::observe::empty(),
        records: Vec::new(),
        log_cursor: 0,
        all_log: Vec::new(),
        clock,
        timed: Arc::new(AtomicU64::new(0)),
        live: live_marked.clone(),
        resolver,
        key_check_release,
        render: None,
    };

    // The app owns the open session-log file, and Kira's backend (in every app
    // that has one) holds the audio device. Both must be gone before the
    // sandbox is deleted, or Windows refuses to remove a folder whose file is
    // still open. A take installs no audio backend at all (spec §4.1).
    // Only a case that draws pays for the renderer, and only a machine with a
    // fallback adapter can build one at all (spec §13).
    let rendered = super::render::draws(request.case) && super::render::available();
    let outcome = {
        let mut app = if rendered {
            crate::composition::keiko_app_rendered(
                config,
                session_log,
                crate::credentials::ClipboardPort::Memory(None),
                key_check,
                crate::DecisionFactory { builder: None },
                asset_faults,
            )
        } else {
            crate::composition::keiko_app(
                config,
                session_log,
                crate::credentials::ClipboardPort::Memory(None),
                key_check,
                crate::DecisionFactory { builder: None },
                asset_faults,
            )
        };
        // The launch read is preloaded, so the sandbox's credential fixture and
        // the `env-key` fixture are what the app sees (spec §7.1).
        app.insert_resource(crate::credentials::StartupCredentials {
            state: launch.state,
            warning: launch.warning,
        });
        #[cfg(test)]
        for plugin in plugins {
            app.add_plugins(plugin);
        }
        #[cfg(test)]
        support::apply_test_hook(&mut app);
        if rendered {
            // A rendered app's plugins are finished before its first frame, as
            // `App::run` does: the renderer's resources are unpacked there.
            app.finish();
            // Frames land beside the real pack unless the caller chose a folder,
            // so they outlive the take without `--keep` (spec §13).
            let (frames, naming) = match &request.frames {
                Some(dir) => (dir.clone(), super::render::Naming::Exact),
                None => (
                    super::render::frames_beside(request.original_pack, &request.case_file),
                    super::render::Naming::Ordinal,
                ),
            };
            state.render = Some(super::render::Rendered::new(&mut app, frames, naming));
        }
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            boot(&mut app, &mut state, &request)?;
            check_focus(&app, &request, Location::File)?;
            for step in &request.case.steps {
                step_once(&mut app, &mut state, &request, &step.step, step.line)?;
                check_focus(&app, &request, Location::Line(step.line))?;
            }
            // A key still held when the take ends is released.
            release_held(&mut app);
            Ok::<(), Finding>(())
        }))
        .unwrap_or_else(|payload| {
            let message = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|text| text.to_string()))
                .unwrap_or_else(|| "a panic with no message".to_string());
            Err(Finding::new(
                request.case_file.clone(),
                Location::File,
                rules::PANIC,
                message,
            ))
        });
        // The observation is rebuilt once more so a failing step reports the
        // last frame it actually saw.
        state.observation = observe(&mut app, Vec::new(), state.live.load(Ordering::Relaxed));
        outcome
    };

    let kept = sandbox.finish(request.keep);
    TakeResult {
        outcome,
        observation: state.observation,
        records: state.records,
        log: state.all_log,
        kept,
        live: state.live.load(Ordering::Relaxed),
    }
}

pub(super) struct TakeState {
    observation: Observation,
    records: Vec<Observation>,
    log_cursor: usize,
    all_log: Vec<LogLine>,
    clock: Arc<AtomicU64>,
    timed: Arc<AtomicU64>,
    live: Arc<std::sync::atomic::AtomicBool>,
    resolver: Arc<KeikoResolver>,
    key_check_release: Arc<std::sync::atomic::AtomicBool>,
    /// The rendered composition, when the case asks for a frame or a click and
    /// the machine has a software rasterizer (spec §13).
    render: Option<super::render::Rendered>,
}

/// Boot the app and settle on the first surface (spec §4.3), then honour a
/// `start` header by reaching the scene top through the Timeline's own restart
/// seam (spec §12).
fn boot(app: &mut App, state: &mut TakeState, request: &TakeRequest) -> Result<(), Finding> {
    frame(app, &state.timed, &state.clock, Duration::ZERO);
    settle(app, state, request)?;
    if request.case.start_scene().is_some() {
        start_at_scene(app, state, request)?;
        settle(app, state, request)?;
    }
    Ok(())
}

/// The focus check after boot and after every step (spec §4.3).
fn check_focus(app: &App, request: &TakeRequest, path: Location) -> Result<(), Finding> {
    match super::controls::hidden_focus(app.world()) {
        None => Ok(()),
        Some(message) => Err(Finding::new(
            request.case_file.clone(),
            path,
            rules::FOCUS_HIDDEN,
            message,
        )),
    }
}

/// `start <scene>` with the `state` overrides: enter play, then replace the
/// position through the one restart recipe the Timeline uses.
fn start_at_scene(
    app: &mut App,
    state: &mut TakeState,
    request: &TakeRequest,
) -> Result<(), Finding> {
    let fail = |message: String| {
        Finding::new(
            request.case_file.clone(),
            Location::File,
            rules::CASE_REFERENCE,
            message,
        )
    };
    let scene = request
        .case
        .start_scene()
        .expect("a start header")
        .to_string();

    // Enter play through the title's own New Game, so the live session the menu
    // keeps is real.
    let mut frame_run = |app: &mut App| frame(app, &state.timed, &state.clock, Duration::ZERO);
    super::steps::apply(app, &Step::NewGame, &mut frame_run, &state.resolver, None)
        .map_err(|finding| fail(finding.message))?;
    settle(app, state, request)?;

    let mut start_state = request.pack.state.clone();
    for (key, value) in request.case.state_overrides() {
        let parsed = unmei_engine::keiko::parse_state_value(value)
            .ok_or_else(|| fail(format!("`{value}` is not a state value for `{key}`")))?;
        start_state.insert(key.to_string(), parsed);
    }
    let scene_for_start = scene.clone();
    let outcome =
        app.world_mut()
            .run_system_once(move |mut session: crate::session::SessionInput| {
                let mut transitions = session.transitions();
                matches!(
                    transitions.restart_scene_with_state(&scene_for_start, start_state.clone()),
                    crate::session::RestartOutcome::Restarted
                )
            });
    match outcome {
        Ok(true) => Ok(()),
        _ => Err(fail(format!("scene `{scene}` could not be started"))),
    }
}

/// Apply one step, settle where the spec says to, and evaluate an assertion.
fn step_once(
    app: &mut App,
    state: &mut TakeState,
    request: &TakeRequest,
    step: &Step,
    line: usize,
) -> Result<(), Finding> {
    // The two fault-step releases act on the take's own fault state before the
    // step is applied like any other.
    match step {
        Step::ReleaseKeyCheck => state.key_check_release.store(true, Ordering::Relaxed),
        Step::Unblock(target) => {
            let pack_root = app
                .world()
                .get_resource::<Runtime>()
                .map(|runtime| runtime.pack_root.clone());
            if let Some(pack_root) = pack_root {
                let path = write_target_path_root(&pack_root, *target);
                let _ = std::fs::remove_dir_all(blocked_write_temp(path, *target));
            }
        }
        _ => {}
    }
    let resolver = state.resolver.clone();
    let mut frame_run = |app: &mut App| frame(app, &state.timed, &state.clock, Duration::ZERO);
    let applied =
        apply(app, step, &mut frame_run, &resolver, state.render.as_mut()).map_err(|finding| {
            let mut finding = finding;
            finding.file = request.case_file.clone();
            finding.path = Location::Line(line);
            finding
        })?;
    match applied {
        Applied::Input => settle_with_step(app, state, request, line)?,
        Applied::Wait(duration) => {
            for _ in 0..frames_for(duration) {
                frame(app, &state.timed, &state.clock, FRAME);
            }
            settle_with_step(app, state, request, line)?;
        }
        Applied::Assert(expect) => {
            observe_step(app, state);
            let drawn = state.render.is_some();
            evaluate(&expect, &state.observation, &state.all_log, app, drawn).map_err(
                |finding| {
                    let mut finding = finding;
                    finding.file = request.case_file.clone();
                    finding.path = Location::Line(line);
                    finding
                },
            )?;
        }
    }
    Ok(())
}

/// The settle rule: zero-duration frames until the world is quiet, or 600
/// frames, whichever comes first (spec §4.3).
pub(super) fn settle(
    app: &mut App,
    state: &mut TakeState,
    request: &TakeRequest,
) -> Result<(), Finding> {
    let held_decision = request.case.headers.iter().any(|line| {
        matches!(
            line.header,
            unmei_engine::keiko::Header::Fault {
                fault: Fault::DecisionHold { .. } | Fault::DecisionSlow { .. }
            }
        )
    }) || request.case.headers.iter().any(|line| {
        matches!(
            line.header,
            unmei_engine::keiko::Header::Fault {
                fault: Fault::KeyCheck {
                    verdict: KeyCheckFault::Hold
                }
            }
        )
    });

    let mut previous: Option<Observation> = None;
    let mut quiet = 0u32;
    // A live answer arrives on the network's schedule, so its pending frames
    // wait in real time and do not spend the quiet-frame budget (spec §6).
    let live_deadline = std::time::Instant::now() + LIVE_ANSWER_LIMIT;
    while quiet < 600 {
        frame(app, &state.timed, &state.clock, Duration::ZERO);
        observe_step(app, state);
        let observation = &state.observation;
        let decision_pending = observation.decision.is_some() && !held_decision;
        if decision_pending && request.live.is_some() {
            if std::time::Instant::now() >= live_deadline {
                return Err(Finding::new(
                    request.case_file.clone(),
                    Location::File,
                    rules::UNSETTLED,
                    "the live decision did not answer within 60 s",
                ));
            }
            std::thread::sleep(Duration::from_millis(2));
            previous = None;
            continue;
        }
        let assets_settled = app
            .world()
            .get_resource::<crate::presentation::assets::AssetWatch>()
            .is_none_or(|watch| watch.is_empty());
        let stable = previous
            .as_ref()
            .is_some_and(|previous| previous.same_world(observation));
        quiet += 1;
        if !decision_pending && assets_settled && stable {
            state.records.push(observation.clone());
            return Ok(());
        }
        previous = Some(observation.clone());
    }
    Err(Finding::new(
        request.case_file.clone(),
        Location::File,
        rules::UNSETTLED,
        "the world did not become quiet within 600 frames",
    ))
}

/// How long a live answer may take before the take gives up (spec §6).
const LIVE_ANSWER_LIMIT: Duration = Duration::from_secs(60);

/// One frame of the take's clock. Only this function moves time.
fn frame(app: &mut App, timed: &AtomicU64, clock: &AtomicU64, delta: Duration) {
    let delta = if delta.is_zero() {
        Duration::ZERO
    } else {
        // 1/60 s is not a whole number of nanoseconds, so the frame's length
        // alternates by one nanosecond: frame n ends at exactly `n * 1e9 / 60`
        // ns, which keeps both the take's clock and Bevy's own `Time` exact.
        // Summing a truncated 16,666,666 ns delta would leave `wait 2s` a
        // nanosecond short of the auto dwell (spec §3.4, §4.3).
        let frames = timed.fetch_add(1, Ordering::Relaxed) + 1;
        let total = frames * NANOS_PER_FRAME / 60;
        let previous = (frames - 1) * NANOS_PER_FRAME / 60;
        clock.store(total, Ordering::Relaxed);
        Duration::from_nanos(total - previous)
    };
    app.insert_resource(TimeUpdateStrategy::ManualDuration(delta));
    app.update();
}

/// One frame of the take's clock in nanoseconds, as `n * 1e9 / 60`.
const NANOS_PER_FRAME: u64 = 1_000_000_000;

/// One step's expectation, exposed for the take's tests.
pub fn expects(case: &Case) -> Vec<&Expect> {
    case.steps
        .iter()
        .filter_map(|line| match &line.step {
            Step::Expect(expect) => Some(expect),
            _ => None,
        })
        .collect()
}

/// The plugin `run`'s shipping call site stands in with: none. A test passes
/// its own to `run_with` instead.
#[cfg(test)]
struct NoExtraPlugin;

#[cfg(test)]
impl Plugin for NoExtraPlugin {
    fn build(&self, _app: &mut App) {}
}

#[cfg(test)]
pub(crate) use support::with_test_hook;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod walk_tests;
