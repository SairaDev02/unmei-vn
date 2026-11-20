//! `unmei-play --check`: one pack-wide report without a play session (spec
//! §1, §6–§8).
//!
//! The check loads the pack exactly as the boot does, adds the pack's flow lint,
//! the referenced asset files it does not have or cannot decode, and every
//! finding from its Keiko cases, then reports them in §3's order. It opens no
//! window and reads no sidecar or credential file. When the pack fails to load,
//! its findings and the case parser's findings are all that can be reported: the
//! flow lint, the asset rules and the reference checks all need the loaded pack,
//! which a failed load does not return.
//!
//! The findings list is built here, once, so a rule family added later pushes
//! its findings into the same list and inherits the one sort and one output.

mod cases;
mod decode;
pub(crate) mod report;

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
    if let Some(pack) = pack.as_ref() {
        findings.extend(unmei_engine::flow::findings(pack));
        // §5: the engine lists every asset the pack's files reference and
        // reports the ones that are not there; the player decodes the rest.
        findings.extend(pack.missing_asset_findings());
        findings.extend(decode::findings(pack));
    }
    findings.extend(cases::findings(pack_root, pack.as_ref()));

    let (errors, _) = report::print(&mut findings, format, "check");
    if errors == 0 {
        crate::keiko::output::PASS
    } else {
        crate::keiko::output::FAIL
    }
}
