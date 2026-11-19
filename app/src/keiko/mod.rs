//! Keiko: end-to-end testing from the player's side (spec §1, §8, §10).
//!
//! Each entry point replays a case on the headless shipping player in its own
//! sandbox, prints an observation and returns the run's exit code. The case
//! grammar and its static checks live in `unmei_engine::keiko`; this module is
//! everything that needs the player.

pub mod assert;
pub mod assets;
pub mod controls;
pub mod decide;
pub mod explore;
pub mod finding;
pub mod observe;
mod observe_text;
pub mod output;
pub mod preview;
pub mod render;
pub mod sandbox;
pub mod sha256;
pub mod steps;
pub mod take;

use std::path::{Path, PathBuf};

use serde_json::json;
use unmei_engine::keiko::{parse_case, validate, Case, Facts, Finding, StepLine};
use unmei_engine::Pack;

use crate::launch::{Format, KeikoMode};

/// Run one Keiko mode. Returns the process exit code (spec §10.3).
pub fn run(mode: &KeikoMode, pack_root: &Path, keep: bool, format: Format) -> u8 {
    let loaded = match unmei_engine::load_pack(pack_root) {
        Ok(loaded) => loaded,
        Err(errors) => {
            let detail = errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ");
            let finding = Finding::new(
                pack_root.display().to_string(),
                String::new(),
                unmei_engine::keiko::rules::CASE_REFERENCE,
                format!("pack: {detail}"),
            );
            if output::is_json(format) {
                println!(
                    "{}",
                    json!({ "findings": [output::finding_json(&finding)] })
                );
            } else {
                output::print_finding(&finding);
            }
            return output::FAIL;
        }
    };
    let pack = loaded.pack;

    match mode {
        KeikoMode::Take { ids, live } => {
            let live = match live_client(*live) {
                Ok(live) => live,
                Err(message) => {
                    eprintln!("unmei-play: {message}");
                    return output::USAGE;
                }
            };
            take(pack_root, &pack, ids, keep, format, None, live)
        }
        KeikoMode::Step { id, step } => append_step(pack_root, &pack, id, step, keep, format),
        // `--observe` reports one case with the same document as `--step`
        // (spec §10.1), not under "takes".
        KeikoMode::Observe { id } => observe_one(pack_root, &pack, id, keep, format),
        KeikoMode::Explore { budget, from } => {
            explore(pack_root, &pack, *budget, from.as_deref(), keep, format)
        }
        // The preview is a mode of its own beside the Keiko group (spec §9).
        KeikoMode::Preview(args) => preview::run(args, pack_root, keep, format),
    }
}

/// The `--observe` run: one case, reported as one document.
fn observe_one(pack_root: &Path, pack: &Pack, id: &str, keep: bool, format: Format) -> u8 {
    let path = pack_root.join("tests").join(format!("{id}.keiko"));
    if !path.is_file() {
        eprintln!("unmei-play: no case `{id}` at {}", path.display());
        return output::FAIL;
    }
    let mut run = run_one(pack_root, pack, id, &path, keep, None, None, 1);
    run.case = id.to_string();
    if output::is_json(format) {
        println!(
            "{}",
            json!({
                "result": if run.result.is_ok() { "pass" } else { "fail" },
                "failure": run.result.as_ref().err().map(output::finding_json),
                "observation": run.observation.to_json(),
            })
        );
    } else {
        output::print_case(&run);
    }
    if run.result.is_ok() {
        output::PASS
    } else {
        output::FAIL
    }
}

/// The `--take` run: every named case, or every case under `tests/`.
#[allow(clippy::too_many_arguments)]
fn take(
    pack_root: &Path,
    pack: &Pack,
    ids: &[String],
    keep: bool,
    format: Format,
    extra: Option<StepLine>,
    live: Option<take::Live>,
) -> u8 {
    let files = match case_files(pack_root, ids) {
        Ok(files) => files,
        Err(message) => {
            eprintln!("unmei-play: {message}");
            return output::FAIL;
        }
    };
    if files.is_empty() {
        if !ids.is_empty() {
            eprintln!("unmei-play: no case named by the given ids");
            return output::FAIL;
        }
        println!("no cases under tests/");
        return output::PASS;
    }

    let mut runs = Vec::new();
    for (index, (id, path)) in files.into_iter().enumerate() {
        // Takes are numbered within the run, so each gets its own
        // `take-<n>` folder under one sandbox root (spec §4.2).
        let mut run = run_one(
            pack_root,
            pack,
            &id,
            &path,
            keep,
            extra.as_ref(),
            live.clone(),
            index + 1,
        );
        run.case = id;
        if output::is_json(format) {
            runs.push(run);
        } else {
            output::print_case(&run);
            runs.push(run);
        }
    }
    if output::is_json(format) {
        println!("{}", output::takes_json(&runs));
    }
    if runs.iter().all(|run| run.result.is_ok()) {
        output::PASS
    } else {
        output::FAIL
    }
}

/// Parse, validate and replay one case.
#[allow(clippy::too_many_arguments)]
fn run_one(
    pack_root: &Path,
    pack: &Pack,
    id: &str,
    path: &Path,
    keep: bool,
    extra: Option<&StepLine>,
    live: Option<take::Live>,
    take_index: usize,
) -> output::CaseRun {
    let case_file = format!("tests/{id}.keiko");
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            return invalid(id, format!("cannot read the case: {error}"));
        }
    };
    let mut case = match parse_case(&case_file, &text) {
        Ok(case) => case,
        Err(finding) => {
            return output::CaseRun {
                case: id.to_string(),
                result: Err(finding),
                observation: observe::empty(),
                live: false,
                kept: None,
            }
        }
    };
    if let Some(extra) = extra {
        case.steps.push(extra.clone());
    }
    let findings = validate_case(pack_root, pack, &case, &case_file);
    if !findings.is_empty() {
        return output::CaseRun {
            case: id.to_string(),
            result: Err(findings[0].clone()),
            observation: observe::empty(),
            live: false,
            kept: None,
        };
    }

    let result = take::run(take::TakeRequest {
        case: &case,
        case_file: case_file.clone(),
        original_pack: pack_root,
        pack,
        take_index,
        keep,
        live,
        frames: None,
    });
    output::CaseRun {
        case: id.to_string(),
        result: result.outcome,
        observation: result.observation,
        live: result.live,
        kept: result.kept,
    }
}

fn invalid(id: &str, message: String) -> output::CaseRun {
    output::CaseRun {
        case: id.to_string(),
        result: Err(Finding::new(
            format!("tests/{id}.keiko"),
            String::new(),
            unmei_engine::keiko::rules::CASE_SYNTAX,
            message,
        )),
        observation: observe::empty(),
        live: false,
        kept: None,
    }
}

/// Every static check a case must pass before a take (spec §2.2, §10.2).
pub fn validate_case(pack_root: &Path, pack: &Pack, case: &Case, case_file: &str) -> Vec<Finding> {
    let settings = crate::settings::setting_keys();
    let keys = steps::key_names();
    let tests_dir = pack_root.join("tests");
    let facts = Facts {
        pack,
        settings_keys: &settings,
        key_names: &keys,
        tests_dir: Some(&tests_dir),
    };
    validate(case, case_file, &facts)
}

/// The case files a run names, as `(id, path)` pairs in byte order of their
/// relative paths (spec §1). The check asks for every case with no ids.
pub(crate) fn case_files(
    pack_root: &Path,
    ids: &[String],
) -> Result<Vec<(String, PathBuf)>, String> {
    let tests = pack_root.join("tests");
    if ids.is_empty() {
        let mut files = Vec::new();
        collect_cases(&tests, &tests, &mut files)?;
        files.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
        return Ok(files);
    }
    let mut out = Vec::new();
    for id in ids {
        let path = tests.join(format!("{id}.keiko"));
        let text =
            std::fs::read_to_string(&path).map_err(|error| format!("case `{id}`: {error}"))?;
        let _ = text;
        out.push((id.clone(), path));
    }
    Ok(out)
}

fn collect_cases(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) -> Result<(), String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(()),
    };
    for entry in entries {
        let entry = entry.map_err(|error| format!("tests/: {error}"))?;
        let path = entry.path();
        if entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_dir()
        {
            collect_cases(root, &path, out)?;
            continue;
        }
        if path
            .extension()
            .is_none_or(|extension| extension != "keiko")
        {
            continue;
        }
        let relative = path.strip_prefix(root).unwrap_or(&path);
        let mut id = relative.to_string_lossy().replace('\\', "/");
        id.truncate(id.len().saturating_sub(".keiko".len()));
        out.push((id, path));
    }
    Ok(())
}

/// `--step ID STEP`: replay, apply, and append only when it passed (spec §8).
///
/// The candidate file is `existing + STEP`; it is parsed, validated and taken
/// exactly as a case would be, and written back only when the take passed. A
/// missing case starts as its version line, and a header is accepted only while
/// the case has no steps.
fn append_step(
    pack_root: &Path,
    pack: &Pack,
    id: &str,
    step: &str,
    keep: bool,
    format: Format,
) -> u8 {
    let case_file = format!("tests/{id}.keiko");
    let path = pack_root.join(&case_file);
    let existing = std::fs::read_to_string(&path).ok();

    let mut candidate = existing.clone().unwrap_or_else(|| {
        format!(
            "keiko {}
",
            unmei_engine::keiko::CASE_VERSION
        )
    });
    if !candidate.ends_with(NEWLINE) {
        candidate.push(NEWLINE);
    }
    candidate.push_str(step);
    candidate.push(NEWLINE);

    let case = match parse_case(&case_file, &candidate) {
        Ok(case) => case,
        Err(finding) => {
            output::print_finding(&finding);
            return output::FAIL;
        }
    };
    if is_header_line(step) && !case.steps.is_empty() && !existing_has_steps(&existing) {
        // Unreachable: a header line parsed after the first step already failed
        // above. Kept total for the reader.
    }
    if is_header_line(step) && existing_has_steps(&existing) {
        let finding = Finding::new(
            &case_file,
            String::new(),
            unmei_engine::keiko::rules::CASE_HEADER_ORDER,
            "a header line must come before the first step",
        );
        output::print_finding(&finding);
        return output::FAIL;
    }
    let findings = validate_case(pack_root, pack, &case, &case_file);
    if let Some(finding) = findings.first() {
        output::print_finding(finding);
        return output::FAIL;
    }

    let result = take::run(take::TakeRequest {
        case: &case,
        case_file: case_file.clone(),
        original_pack: pack_root,
        pack,
        take_index: 1,
        keep,
        live: None,
        frames: None,
    });
    let run = output::CaseRun {
        case: id.to_string(),
        result: result.outcome,
        observation: result.observation,
        live: result.live,
        kept: result.kept,
    };
    if output::is_json(format) {
        println!(
            "{}",
            json!({
                "result": if run.result.is_ok() { "pass" } else { "fail" },
                "failure": run.result.as_ref().err().map(output::finding_json),
                "observation": run.observation.to_json(),
            })
        );
    } else {
        output::print_case(&run);
    }
    if run.result.is_err() {
        return output::FAIL;
    }
    // A pack with no `tests/` folder yet still accepts its first case (spec §8).
    if let Some(parent) = path.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            eprintln!("unmei-play: cannot create {}: {error}", parent.display());
            return output::FAIL;
        }
    }
    if let Err(error) = std::fs::write(&path, candidate) {
        eprintln!("unmei-play: cannot write {case_file}: {error}");
        return output::FAIL;
    }
    output::PASS
}

/// The one line ending the stepper adds.
const NEWLINE: char = '\n';

/// Whether the file already holds a step, so a header cannot be appended.
fn existing_has_steps(existing: &Option<String>) -> bool {
    existing
        .as_deref()
        .and_then(|text| parse_case("case", text).ok())
        .is_some_and(|case| !case.steps.is_empty())
}

fn is_header_line(step: &str) -> bool {
    let first = step.split_whitespace().next().unwrap_or_default();
    matches!(
        first,
        "decide" | "setting" | "fixture" | "fault" | "dev" | "start" | "state"
    )
}

/// `--explore`: the bounded walk (spec §9).
fn explore(
    pack_root: &Path,
    pack: &Pack,
    budget: u32,
    from: Option<&str>,
    keep: bool,
    format: Format,
) -> u8 {
    let report = explore::run(pack_root, pack, budget, from, keep);
    if output::is_json(format) {
        println!("{}", report.to_json());
    } else {
        report.print();
    }
    if report.findings.is_empty() {
        output::PASS
    } else {
        output::FAIL
    }
}

/// The live client, or the usage message that stops a live run with no key.
fn live_client(live: bool) -> Result<Option<take::Live>, String> {
    if !live {
        return Ok(None);
    }
    let key = std::env::var(crate::credentials::ENV_KEY)
        .ok()
        .filter(|key| !key.trim().is_empty())
        .ok_or_else(|| {
            format!(
                "--live needs {} in the environment; the credential file is never read",
                crate::credentials::ENV_KEY
            )
        })?;
    let endpoint = std::env::var(unmei_engine::decision::client::ENDPOINT_ENV)
        .unwrap_or_else(|_| unmei_engine::DEFAULT_ENDPOINT.to_string());
    Ok(Some(take::Live { key, endpoint }))
}

/// The landing action a `decide` header names, in the question's own terms.
pub fn landing_action(action: &str) -> String {
    action.to_string()
}

/// The question id the decision seam sees: the question alone.
pub fn question_of(point: &str) -> String {
    point
        .split_once('.')
        .map(|(_, question)| question.to_string())
        .unwrap_or_else(|| point.to_string())
}
