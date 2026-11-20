//! Reading one plugin file for the hand-walked passes: its bytes, its UTF-8
//! text and its RON value.
//!
//! Every plugin file that is read as RON — the manifest, the tokens and each
//! surface tree — comes through here, so a read or syntax failure reads the same
//! whichever file it came from. All three are file-level findings, never a field
//! path.

use crate::finding::{rules, Finding, Location};

use super::source::Source;

/// RON's own guard against a stack overflow from a deeply nested document. Its
/// default, 128, is below what a surface tree at the format's depth cap needs,
/// so a tree one node too deep would read as invalid RON instead of the cap
/// finding the spec names. 256 parses well past the cap and still bounds an
/// untrusted file; anything deeper is `ui.parse` for exceeding it.
const RECURSION_LIMIT: usize = 256;

pub(super) fn read_ron(
    source: &dyn Source,
    path: &str,
    what: &str,
    findings: &mut Vec<Finding>,
) -> Option<ron::Value> {
    let file = source.display(path);
    let bytes = match source.read(path) {
        Ok(bytes) => bytes,
        Err(reason) => {
            findings.push(Finding::new(
                file,
                Location::File,
                rules::UI_FILE_READ,
                format!("cannot read the {what}: {reason}"),
            ));
            return None;
        }
    };
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => {
            findings.push(Finding::new(
                file,
                Location::File,
                rules::UI_PARSE,
                format!("the {what} is not UTF-8"),
            ));
            return None;
        }
    };
    let options = ron::Options::default().with_recursion_limit(RECURSION_LIMIT);
    match options.from_str(&text) {
        Ok(value) => Some(value),
        Err(error) => {
            findings.push(Finding::new(
                file,
                Location::File,
                rules::UI_PARSE,
                format!("invalid RON: {error}"),
            ));
            None
        }
    }
}
