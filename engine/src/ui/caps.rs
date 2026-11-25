//! Spec §7's file-level caps and the PNG header the image cap reads.
//!
//! A cap is an error naming the cap and the value found, and each one is
//! inclusive: a file at 16 MiB passes and its next byte fails. The folder's
//! count and its totals come from the source's sizes, so an oversized plugin is
//! rejected without loading its files; only a shipped PNG is opened, because
//! its dimensions sit in its header. The other three caps count what a file
//! holds — 2,000 nodes and depth 32 in one surface tree, 512 characters in one
//! copy override — so they belong to the passes that parse those files
//! (tickets #106 and #107). Their values live here so the format's one cap
//! table has one home; the node pass reads the tree's two.

use crate::finding::{rules, Finding, Location};

use super::images::{is_png, PNG_SIGNATURE};
use super::source::Source;

/// Files in the plugin folder.
pub const MAX_FILES: usize = 256;

/// One file in the plugin folder.
pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

/// Every file in the plugin folder, together.
pub const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;

/// One side of a shipped image.
pub const MAX_IMAGE_SIDE: u32 = 4096;

/// Nodes in one surface tree, the root included.
pub const MAX_NODES: usize = 2_000;

/// Depth of one surface tree, the root counted as 1.
pub const MAX_DEPTH: usize = 32;

/// Characters in one copy override, the smallest of the caps.
pub const MAX_COPY_CHARS: usize = 512;

pub(super) fn check(source: &dyn Source, findings: &mut Vec<Finding>) {
    let paths = source.paths();
    if paths.len() > MAX_FILES {
        findings.push(cap(
            source.display(""),
            format!(
                "the plugin folder holds {} files; the cap is {MAX_FILES}",
                paths.len()
            ),
        ));
    }

    let mut total = 0;
    for path in paths {
        let size = source.size(path);
        if size > MAX_FILE_BYTES {
            findings.push(cap(
                source.display(path),
                format!("the file is {size} bytes; the cap for one file is {MAX_FILE_BYTES}"),
            ));
            continue;
        }
        total += size;
        if is_png(path) {
            check_image(source, path, findings);
        }
        if total > MAX_TOTAL_BYTES {
            findings.push(cap(
                source.display(""),
                format!("the plugin folder holds {total} bytes; the cap is {MAX_TOTAL_BYTES}"),
            ));
            // The folder has already failed, and the rest of it is exactly
            // what the cap says not to read.
            return;
        }
    }
}

fn check_image(source: &dyn Source, path: &str, findings: &mut Vec<Finding>) {
    let bytes = match source.read(path) {
        Ok(bytes) => bytes,
        Err(reason) => {
            findings.push(Finding::new(
                source.display(path),
                Location::File,
                rules::UI_FILE_READ,
                format!("cannot read the image: {reason}"),
            ));
            return;
        }
    };
    let Some((width, height)) = png_size(&bytes) else {
        return;
    };
    if width > MAX_IMAGE_SIDE || height > MAX_IMAGE_SIDE {
        findings.push(cap(
            source.display(path),
            format!("the image is {width}x{height}; the cap is {MAX_IMAGE_SIDE} on each side"),
        ));
    }
}

fn cap(file: String, message: String) -> Finding {
    Finding::new(file, Location::File, rules::UI_CAP, message)
}

/// A PNG's width and height are the IHDR chunk's first eight payload bytes, at
/// offsets 16 and 20 of the file: signature, chunk length, chunk type.
fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let header = bytes.get(..24)?;
    if header[..8] != PNG_SIGNATURE || &header[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes([header[16], header[17], header[18], header[19]]);
    let height = u32::from_be_bytes([header[20], header[21], header[22], header[23]]);
    Some((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_png_header_gives_its_dimensions() {
        let mut bytes = PNG_SIGNATURE.to_vec();
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&4096u32.to_be_bytes());
        bytes.extend_from_slice(&2048u32.to_be_bytes());
        bytes.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(png_size(&bytes), Some((4096, 2048)));
    }

    #[test]
    fn a_file_that_only_looks_like_a_png_has_no_dimensions() {
        let mut bytes = PNG_SIGNATURE.to_vec();
        bytes.extend_from_slice(&[0; 16]);
        assert_eq!(png_size(&bytes), None);
        assert_eq!(png_size(b"not a png at all, not even close"), None);
    }
}
