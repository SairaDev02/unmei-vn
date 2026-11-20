//! The take's setup seams: the resolver, the exact frame count, the
//! `setting` header applier and the key check. Kept apart from `take.rs` so
//! each file stays under the source-size limit.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use unmei_engine::keiko::{Case, Fault, FixtureKind, KeyCheckFault};

use bevy::prelude::*;

use super::super::decide::{self as decide, KeikoResolver};
use super::super::finding::{rules, Finding, Location};
use super::super::observe::{log_lines_from, observe};
use super::Sandbox;
use super::{settle, TakeRequest, TakeState};
use crate::keiko::{landing_action, question_of};
use std::path::{Path, PathBuf};
use unmei_engine::keiko::WriteTarget;

pub(super) fn build_resolver(request: &TakeRequest, clock: Arc<AtomicU64>) -> Arc<KeikoResolver> {
    let decisions = request
        .case
        .headers
        .iter()
        .filter_map(|line| match &line.header {
            unmei_engine::keiko::Header::Decide { point, action } => {
                Some((question_of(point), landing_action(action)))
            }
            _ => None,
        });
    let faults: Vec<Fault> = request
        .case
        .headers
        .iter()
        .filter_map(|line| match &line.header {
            unmei_engine::keiko::Header::Fault { fault } => Some(fault.clone()),
            _ => None,
        })
        .collect();
    let resolver = KeikoResolver::new(decisions, faults).with_clock(clock);
    match &request.live {
        Some(live) => Arc::new(resolver.with_live(decide::live_client(&live.endpoint, &live.key))),
        None => Arc::new(resolver),
    }
}

/// How many timed frames `duration` lasts: `ceil(nanos * 60 / 1e9)`, in exact
/// integer arithmetic so `wait 1s` is 60 frames rather than 61 (spec §3.4).
pub fn frames_for(duration: Duration) -> u64 {
    let nanos = duration.as_nanos() * 60;
    nanos.div_ceil(u128::from(super::NANOS_PER_FRAME)) as u64
}

/// Write one `setting` header into the in-memory settings, in the file's RON
/// form, so the player's own validation and normalisation apply at load.
pub(super) fn apply_setting(
    settings: &mut crate::settings::Settings,
    key: &str,
    value: &str,
) -> Result<(), String> {
    let Some((_, kind)) = crate::settings::SETTING_SCHEMA
        .iter()
        .find(|(name, _)| *name == key)
    else {
        return Err(format!("unknown settings key `{key}`"));
    };
    // The kind is checked by the table, so a toggle cannot take a number.
    match kind {
        crate::settings::SettingKind::Number if value.parse::<f32>().is_err() => {
            return Err(format!("`{value}` is not a value for `{key}`"));
        }
        crate::settings::SettingKind::Toggle if value.parse::<bool>().is_err() => {
            return Err(format!("`{value}` is not a value for `{key}`"));
        }
        _ => {}
    }
    let invalid = || format!("`{value}` is not a value for `{key}`");
    match key {
        "audio.master" => settings.audio.master = value.parse().map_err(|_| invalid())?,
        "audio.music" => settings.audio.music = value.parse().map_err(|_| invalid())?,
        "audio.sfx" => settings.audio.sfx = value.parse().map_err(|_| invalid())?,
        "audio.muted" => settings.audio.muted = value.parse().map_err(|_| invalid())?,
        "play.text_speed" => settings.play.text_speed = value.parse().map_err(|_| invalid())?,
        "play.auto" => settings.play.auto = value.parse().map_err(|_| invalid())?,
        "play.fullscreen" => settings.play.fullscreen = value.parse().map_err(|_| invalid())?,
        other => return Err(format!("unknown settings key `{other}`")),
    }
    Ok(())
}

/// A key check that follows the case's declared fault, or answers `Unreachable`
/// when none is declared (spec §4.1, §7.2).
pub(super) struct KeikoKeyCheck {
    verdict: KeyCheckFault,
    released: Arc<std::sync::atomic::AtomicBool>,
}

pub(super) fn write_target_path_root(pack_root: &std::path::Path, target: WriteTarget) -> PathBuf {
    let pack = unmei_engine::Pack {
        manifest: unmei_engine::Manifest {
            name: String::new(),
            entry: String::new(),
            tagline: None,
            title_backdrop: None,
            scenes: Vec::new(),
        },
        state: Default::default(),
        scenes: Default::default(),
        npcs: Default::default(),
        root: pack_root.to_path_buf(),
    };
    match target {
        WriteTarget::Slot(slot) => unmei_engine::save_path_for(&pack, slot),
        WriteTarget::ReadLog => unmei_engine::read_log_path_for(&pack),
        WriteTarget::Settings => crate::settings::settings_path_for(pack_root),
        WriteTarget::Credential => crate::credentials::credentials_path_in(
            &pack_root.join("..").join("config").join("unmei"),
        ),
    }
}

impl KeikoKeyCheck {
    pub(super) fn new(
        headers: &[unmei_engine::keiko::HeaderLine],
        released: Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        let verdict = headers
            .iter()
            .find_map(|line| match &line.header {
                unmei_engine::keiko::Header::Fault {
                    fault: Fault::KeyCheck { verdict },
                } => Some(*verdict),
                _ => None,
            })
            .unwrap_or(KeyCheckFault::Unreachable);
        Self { verdict, released }
    }
}

impl unmei_engine::KeyCheck for KeikoKeyCheck {
    fn check<'a>(
        &'a self,
        _key: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = unmei_engine::KeyVerdict> + Send + 'a>>
    {
        let immediate = match self.verdict {
            KeyCheckFault::Accepted => Some(unmei_engine::KeyVerdict::Accepted),
            KeyCheckFault::Rejected => Some(unmei_engine::KeyVerdict::Rejected),
            KeyCheckFault::Unreachable => Some(unmei_engine::KeyVerdict::Unreachable),
            // A held verdict answers only when `release-key-check` runs.
            KeyCheckFault::Hold => None,
        };
        if let Some(verdict) = immediate {
            return Box::pin(async move { verdict });
        }
        let released = self.released.clone();
        Box::pin(std::future::poll_fn(move |context| {
            if released.load(Ordering::Relaxed) {
                std::task::Poll::Ready(unmei_engine::KeyVerdict::Unreachable)
            } else {
                context.waker().wake_by_ref();
                std::task::Poll::Pending
            }
        }))
    }
}

/// Release every key the case left held, so the next take starts clean.
pub(super) fn release_held(app: &mut App) {
    let held: Vec<KeyCode> = app
        .world()
        .resource::<ButtonInput<KeyCode>>()
        .get_pressed()
        .copied()
        .collect();
    for key in held {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(key);
    }
}

/// Build the observation and accumulate the take's log lines.
pub(super) fn observe_step(app: &mut App, state: &mut TakeState) {
    let (lines, cursor) = log_lines_from(app, state.log_cursor);
    state.log_cursor = cursor;
    state.all_log.extend(lines.iter().cloned());
    state.observation = observe(app, lines, state.live.load(Ordering::Relaxed));
}

pub(super) fn settle_with_step(
    app: &mut App,
    state: &mut TakeState,
    request: &TakeRequest,
    line: usize,
) -> Result<(), Finding> {
    let mut settled = settle(app, state, request);
    if let Err(finding) = &mut settled {
        finding.path = Location::Line(line);
    }
    settled
}

pub(super) fn write_target_path(sandbox: &Sandbox, target: WriteTarget) -> PathBuf {
    write_target_path_root(sandbox.pack_root(), target)
}

/// The temp sibling the target's own writer creates first: where a
/// `write-blocked` obstruction goes, and the path `unblock` removes. Sidecars
/// are written through `sidecar::write_atomic` (`<path>.ron.tmp`); the
/// credential file's writer uses `<path>.tmp` (`credentials::write`), so the two
/// cannot share one rule.
pub(super) fn blocked_write_temp(path: PathBuf, target: WriteTarget) -> PathBuf {
    match target {
        WriteTarget::Credential => path.with_extension("tmp"),
        _ => path.with_extension("ron.tmp"),
    }
}

/// Apply the case's fixtures and faults to the sandbox before boot (spec §7).
pub(super) fn prepare(sandbox: &Sandbox, case: &Case, original_pack: &Path) -> Result<(), Finding> {
    let tests_dir = original_pack.join("tests");
    let fail =
        |message: String| Finding::new("case", Location::File, rules::CASE_REFERENCE, message);

    for line in &case.headers {
        if let unmei_engine::keiko::Header::Fixture { kind, path } = &line.header {
            let source = tests_dir.join(path);
            let destination = sandbox.fixture_path(*kind);
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| fail(format!("fixture `{path}`: {error}")))?;
            }
            std::fs::copy(&source, &destination)
                .map_err(|error| fail(format!("fixture `{path}`: {error}")))?;
        }
    }

    // `setting` headers: written into the sandbox settings file before boot, on
    // top of any `fixture settings` the case copied.
    let settings_headers: Vec<(&str, &str)> = case
        .headers
        .iter()
        .filter_map(|line| match &line.header {
            unmei_engine::keiko::Header::Setting { key, value } => {
                Some((key.as_str(), value.as_str()))
            }
            _ => None,
        })
        .collect();
    if !settings_headers.is_empty() {
        let mut settings = crate::settings::load(sandbox.pack_root()).settings;
        for (key, value) in settings_headers {
            apply_setting(&mut settings, key, value).map_err(&fail)?;
        }
        crate::settings::save(sandbox.pack_root(), &settings)
            .map_err(|error| fail(format!("setting: {error}")))?;
    }

    for line in &case.headers {
        let unmei_engine::keiko::Header::Fault { fault } = &line.header else {
            continue;
        };
        match fault {
            Fault::WriteBlocked { target } => {
                let path = write_target_path(sandbox, *target);
                let temp = blocked_write_temp(path, *target);
                std::fs::create_dir_all(&temp)
                    .map_err(|error| fail(format!("write-blocked: {error}")))?;
            }
            Fault::PackFileMissing { path } => {
                let _ = std::fs::remove_file(sandbox.pack_root().join(path));
            }
            Fault::PackFile { path, fixture } => {
                let source = tests_dir.join(fixture);
                let destination = sandbox.pack_root().join(path);
                if let Some(parent) = destination.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|error| fail(format!("pack-file: {error}")))?;
                }
                std::fs::copy(&source, &destination)
                    .map_err(|error| fail(format!("pack-file `{fixture}`: {error}")))?;
            }
            Fault::CredentialUnreadable => {
                let path = sandbox.fixture_path(FixtureKind::Credential);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                std::fs::create_dir_all(&path)
                    .map_err(|error| fail(format!("credential-unreadable: {error}")))?;
            }
            Fault::AssetMissing { .. }
            | Fault::AssetCorrupt { .. }
            | Fault::DecisionError { .. }
            | Fault::DecisionSlow { .. }
            | Fault::DecisionHold { .. }
            | Fault::KeyCheck { .. } => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every key the schema publishes applies, so the validator's table and
    /// the applier cannot drift.
    #[test]
    fn every_schema_key_applies() {
        for (key, kind) in crate::settings::SETTING_SCHEMA {
            let value = match kind {
                crate::settings::SettingKind::Number => "0.5",
                crate::settings::SettingKind::Toggle => "true",
            };
            let mut settings = crate::settings::Settings::default();
            assert!(
                apply_setting(&mut settings, key, value).is_ok(),
                "{key} did not apply"
            );
        }
    }

    #[test]
    fn wait_frames_are_exact() {
        assert_eq!(frames_for(Duration::from_secs(1)), 60);
        assert_eq!(frames_for(Duration::from_secs(2)), 120);
        assert_eq!(frames_for(Duration::from_millis(250)), 15);
        assert_eq!(frames_for(Duration::from_millis(980)), 59);
        assert_eq!(frames_for(Duration::from_nanos(1)), 1);
    }
}
