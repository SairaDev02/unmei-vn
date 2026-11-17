//! The output shapes and exit codes every Keiko mode shares (spec §10).
//!
//! A finding is the pack check's C3 shape — `file`, `path`, `rule`, `message` —
//! so one parser handles a check's findings and a take's alike.

use serde_json::{json, Value};

use super::finding::Finding;
use super::observe::Observation;
use crate::launch::Format;

/// The exit code a run returns.
pub const PASS: u8 = 0;
pub const FAIL: u8 = 1;
pub const USAGE: u8 = 2;

/// One case's outcome, for the take document.
pub struct CaseRun {
    pub case: String,
    pub result: Result<(), Finding>,
    pub observation: Observation,
    pub live: bool,
    /// The kept sandbox folder, when `--keep` was given.
    pub kept: Option<std::path::PathBuf>,
}

impl CaseRun {
    /// The case's take object.
    pub fn to_json(&self) -> Value {
        json!({
            "case": self.case,
            "result": if self.result.is_ok() { "pass" } else { "fail" },
            "live": self.live,
            "failure": self.result.as_ref().err().map(finding_json),
            "observation": self.observation.to_json(),
        })
    }
}

/// A finding as the check family's C3 object.
pub fn finding_json(finding: &Finding) -> Value {
    json!({
        "file": finding.file,
        "path": finding.path,
        "rule": finding.rule,
        "message": finding.message,
    })
}

/// The `--take` document.
pub fn takes_json(runs: &[CaseRun]) -> Value {
    json!({
        "takes": runs.iter().map(CaseRun::to_json).collect::<Vec<_>>(),
    })
}

/// Print one case's text result.
pub fn print_case(run: &CaseRun) {
    if let Some(kept) = &run.kept {
        println!("kept: {}", kept.display());
    }
    match &run.result {
        Ok(()) => {
            println!("PASS {} ({})", run.case, run.observation.surface());
            if run.live {
                println!("  live: nondeterministic, not regression evidence");
            }
        }
        Err(finding) => {
            println!("FAIL {}", run.case);
            println!(
                "  {} {}: {}",
                finding.rule,
                if finding.path.is_empty() {
                    String::new()
                } else {
                    finding.path.clone()
                },
                finding.message
            );
            print!("  observed: {}", indent(&run.observation.to_text()));
        }
    }
}

/// Print one finding in the text form.
pub fn print_finding(finding: &Finding) {
    let path = if finding.path.is_empty() {
        String::new()
    } else {
        format!(" {}", finding.path)
    };
    println!(
        "{}:{} {}: {}",
        finding.file, path, finding.rule, finding.message
    );
}

fn indent(text: &str) -> String {
    text.replace('\n', "\n  ")
}

/// Whether a format is JSON.
pub fn is_json(format: Format) -> bool {
    matches!(format, Format::Json)
}
