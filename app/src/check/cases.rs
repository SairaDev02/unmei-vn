//! The pack's Keiko cases, statically checked without a take (§7).

use std::path::Path;

use unmei_engine::keiko::{parse_case, Finding};
use unmei_engine::Pack;

/// Every `.keiko` file under `tests/`, `tests/findings/` included, in byte order
/// of its relative path.
///
/// Each case contributes the parser's one finding, if any, and its parse
/// result. `pack` is `None` when the pack failed to load: the reference checks
/// need the loaded pack's facts, so then only the parser runs.
pub fn findings(pack_root: &Path, pack: Option<&Pack>) -> Vec<Finding> {
    let Ok(files) = crate::keiko::case_files(pack_root, &[]) else {
        // The cases folder could not be walked; no case file exists to report.
        return Vec::new();
    };
    let mut findings = Vec::new();
    for (id, path) in files {
        let file = format!("tests/{id}.keiko");
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                findings.push(Finding::new(
                    file,
                    String::new(),
                    unmei_engine::keiko::rules::CASE_SYNTAX,
                    format!("cannot read the case: {error}"),
                ));
                continue;
            }
        };
        match parse_case(&file, &text) {
            Ok(case) => {
                if let Some(pack) = pack {
                    findings.extend(crate::keiko::validate_case(pack_root, pack, &case, &file));
                }
            }
            Err(finding) => findings.push(finding),
        }
    }
    findings
}
