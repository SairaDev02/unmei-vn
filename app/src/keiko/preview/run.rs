//! The preview's entry: load the pack, plan the request, probe the rasterizer
//! and run the takes that write its frames into the output folder (spec §9).
//!
//! A preview opens no window, needs no key and never touches the pack's
//! sidecars or the user's credential file: the take engine works in its own
//! sandbox, and the frames folder is the only file the preview writes.

use std::path::Path;

use unmei_engine::keiko::{rules, Case};
use unmei_engine::{Finding, Location, Pack, StateValue};

use super::states::{Fixture, Outcome};
use super::{plan, Plan, PreviewRequest};
use crate::check::report;
use crate::keiko::{output, render, take};
use crate::launch::{Format, PreviewArgs};

/// Run the preview. Returns the process exit code: 0 frames written and no
/// error finding, 1 a finding, 2 a usage error, which returns before this runs
/// (spec §8, §9.2).
pub fn run(args: &PreviewArgs, pack_root: &Path, keep: bool, format: Format) -> u8 {
    // §9.3: a pack that fails to load reports its findings and writes no frame.
    let pack = match unmei_engine::load_pack(pack_root) {
        Ok(loaded) => loaded.pack,
        Err(errors) => return report_findings(errors, format),
    };
    let request = match request(args) {
        Ok(request) => request,
        Err(finding) => return report_findings(vec![finding], format),
    };
    let plan = match plan(&pack, &request) {
        Ok(plan) => plan,
        Err(finding) => return report_findings(vec![finding], format),
    };
    // §9.4: with no fallback adapter the preview fails before any frame. One
    // probe covers every take in the slate, so no state is misreported as
    // unreachable on a machine that cannot draw at all.
    if !render::available() {
        return report_findings(
            vec![Finding::new(
                "preview",
                Location::File,
                rules::RENDER_UNAVAILABLE,
                "this machine has no wgpu fallback adapter, so the preview cannot render",
            )],
            format,
        );
    }
    if let Err(error) = std::fs::create_dir_all(&args.dir) {
        eprintln!(
            "unmei-play: cannot create the preview folder {}: {error}",
            args.dir.display()
        );
        return output::FAIL;
    }
    match plan {
        Plan::Slate(fixtures) => slate(&pack, pack_root, args, keep, fixtures, format),
        Plan::One(case) => one(&pack, pack_root, args, keep, &case, format),
    }
}

/// One take for a request that names a line or a scene (spec §9.1).
fn one(
    pack: &Pack,
    pack_root: &Path,
    args: &PreviewArgs,
    keep: bool,
    case: &Case,
    format: Format,
) -> u8 {
    match take::run(take_request(pack, pack_root, args, keep, case, 1)).outcome {
        Ok(()) => output::PASS,
        Err(finding) => report_findings(vec![finding], format),
    }
}

/// §9.2's slate: one fresh take per fixture state, and one warning per state the
/// pack cannot reach. A state whose take fails for any other reason keeps the
/// failure an error, so a broken pack, a panic or a missing rasterizer is never
/// reported as a missing state.
fn slate(
    pack: &Pack,
    pack_root: &Path,
    args: &PreviewArgs,
    keep: bool,
    fixtures: Vec<Fixture>,
    format: Format,
) -> u8 {
    let mut findings = Vec::new();
    for (index, fixture) in fixtures.into_iter().enumerate() {
        let name = fixture.name;
        match fixture.outcome {
            Outcome::Unreachable(reason) => findings.push(unreachable(name, &reason)),
            Outcome::Case(case) => {
                match take::run(take_request(pack, pack_root, args, keep, &case, index + 1)).outcome
                {
                    Ok(()) => {}
                    // A step that could not find or activate its control is the one
                    // honest "this take did not reach the state" signal: a
                    // precondition the pack cannot set up (spec §9.2).
                    Err(finding) if unreached(&finding) => {
                        findings.push(unreachable(name, &finding.message));
                    }
                    Err(finding) => findings.push(named(name, finding)),
                }
            }
        }
    }
    let (errors, _) = report::print(&mut findings, format, "preview");
    if errors > 0 {
        output::FAIL
    } else {
        output::PASS
    }
}

/// One take's request: the case, the pack and the folder the frames land in
/// under their fixed names (spec §9.1).
fn take_request<'a>(
    pack: &'a Pack,
    pack_root: &'a Path,
    args: &PreviewArgs,
    keep: bool,
    case: &'a Case,
    take_index: usize,
) -> take::TakeRequest<'a> {
    take::TakeRequest {
        case,
        case_file: "preview".to_string(),
        original_pack: pack_root,
        pack,
        take_index,
        keep,
        live: None,
        frames: Some(args.dir.clone()),
    }
}

/// The warning §9.2 reports for a state the pack cannot reach.
fn unreachable(name: &str, reason: &str) -> Finding {
    Finding::warning(
        "preview",
        Location::File,
        rules::PREVIEW_STATE_UNREACHABLE,
        format!("{name}: {reason}"),
    )
}

/// A take's failure, named for the state whose take it was.
fn named(name: &str, mut finding: Finding) -> Finding {
    finding.message = format!("{name}: {}", finding.message);
    finding
}

/// Whether a take failed because a step could not reach its target, which for
/// the slate is a state the pack cannot set up (spec §9.2).
fn unreached(finding: &Finding) -> bool {
    finding.rule == rules::STEP_NO_TARGET || finding.rule == rules::STEP_DISABLED
}

/// The grammar's request plus `--state`'s file, read here because the parser
/// touches no filesystem (spec §1).
fn request(args: &PreviewArgs) -> Result<PreviewRequest, Finding> {
    let state = match &args.state {
        Some(path) => state_overrides(path)?,
        None => Vec::new(),
    };
    Ok(PreviewRequest {
        scene: args.scene.clone(),
        at: args.at.clone(),
        state,
        picks: args.picks.clone(),
    })
}

/// A `--state` file as the overrides the generated case carries: a RON map in
/// `state.ron`'s form, in key order so the generated case is stable.
fn state_overrides(path: &Path) -> Result<Vec<(String, StateValue)>, Finding> {
    let fail = |message: String| {
        Finding::new(
            "preview",
            Location::File,
            rules::CASE_REFERENCE,
            format!("--state {}: {message}", path.display()),
        )
    };
    let text =
        std::fs::read_to_string(path).map_err(|error| fail(format!("cannot read it: {error}")))?;
    let map = unmei_engine::pack_ron::decode_state(&text).map_err(fail)?;
    let mut state: Vec<(String, StateValue)> = map.into_iter().collect();
    state.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(state)
}

/// Print findings in §8's shape and report the failure.
fn report_findings(mut findings: Vec<Finding>, format: Format) -> u8 {
    report::print(&mut findings, format, "preview");
    output::FAIL
}
