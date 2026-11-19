//! The preview's entry: load the pack, build the generated case, probe the
//! rasterizer and run one take that writes its frames into the output folder
//! (spec §9).
//!
//! A preview opens no window, needs no key and never touches the pack's
//! sidecars or the user's credential file: the take engine works in its own
//! sandbox, and the frames folder is the only file the preview writes.

use std::path::Path;

use unmei_engine::keiko::rules;
use unmei_engine::{Finding, StateValue};

use super::{build_case, PreviewRequest};
use crate::check::report;
use crate::keiko::{output, render, take};
use crate::launch::{Format, PreviewArgs};

/// Run the preview. Returns the process exit code: 0 frames written, 1 a
/// finding, 2 a usage error, which returns before this runs (spec §8).
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
    let case = match build_case(&pack, &request) {
        Ok(case) => case,
        Err(finding) => return report_findings(vec![finding], format),
    };
    // §9.4: with no fallback adapter the preview fails before any frame.
    if !render::available() {
        return report_findings(
            vec![Finding::new(
                "preview",
                String::new(),
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

    let result = take::run(take::TakeRequest {
        case: &case,
        case_file: "preview".to_string(),
        original_pack: pack_root,
        pack: &pack,
        take_index: 1,
        keep,
        live: None,
    });
    match result.outcome {
        Ok(()) => output::PASS,
        Err(finding) => report_findings(vec![finding], format),
    }
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
            String::new(),
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
