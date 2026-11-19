//! `unmei-play --check`: one pack-wide report without a play session (spec
//! §1, §7–§8).
//!
//! The check loads the pack exactly as the boot does, adds every finding from
//! the pack's Keiko cases, and reports them in §3's order. It opens no window
//! and reads no sidecar or credential file. When the pack fails to load, its
//! findings and the case parser's findings are all that can be reported: the
//! reference checks need the loaded pack, and the asset and flow rules belong to
//! the check's later halves.
//!
//! The findings list is built here, once, so a rule family added later pushes
//! its findings into the same list and inherits the one sort and one output.

mod cases;
mod report;

use std::path::Path;

use crate::launch::Format;

/// Run the check. Returns the process exit code: 0 with no error finding, 1
/// with at least one. A usage error returns 2 before this function runs.
pub fn run(pack_root: &Path, format: Format) -> u8 {
    let mut findings = Vec::new();
    let pack = match unmei_engine::load_pack(pack_root) {
        Ok(loaded) => {
            findings.extend(loaded.warnings);
            Some(loaded.pack)
        }
        Err(errors) => {
            findings.extend(errors);
            None
        }
    };
    findings.extend(cases::findings(pack_root, pack.as_ref()));

    let (errors, _) = report::print(&mut findings, format);
    if errors == 0 {
        crate::keiko::output::PASS
    } else {
        crate::keiko::output::FAIL
    }
}
