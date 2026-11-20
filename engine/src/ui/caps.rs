//! Spec §7's file-level caps and the image headers they read.
//!
//! A cap is an error naming the cap and the value found, and each one is
//! inclusive: a file at 16 MiB passes and its next byte fails. The folder's
//! count and its totals come from the source's sizes, so an oversized plugin is
//! rejected without loading its files; only a shipped image is opened, because
//! its dimensions sit in its header. The other three caps count what a file
//! holds — 2,000 nodes and depth 32 in one surface tree, 512 characters in one
//! copy override — so they belong to the passes that parse those files
//! (tickets #106 and #107). Their values live here so the format's one cap
//! table has one home; the node pass reads the tree's two.

use crate::finding::{rules, Finding, Location};

use super::source::{extension, Source};

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

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// The image files a plugin may ship, by extension (spec §2's "any other file",
/// named from the manifest).
const IMAGE_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "webp", "bmp"];

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
        if is_image(path) {
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
    let Some((width, height)) = image_size(path, &bytes) else {
        // A header this build cannot measure is no cap finding. A plugin image
        // no decoder reads is the renderer's rule, not this pass's.
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

fn is_image(path: &str) -> bool {
    extension(path).is_some_and(|extension| {
        IMAGE_EXTENSIONS
            .iter()
            .any(|known| known.eq_ignore_ascii_case(extension))
    })
}

/// An image's pixel dimensions from its header, without a decoder. Only the two
/// formats the reference plugin and DESIGN.md's art use are measured; a `webp`
/// or `bmp` file is recognised but carries no header reader here.
fn image_size(path: &str, bytes: &[u8]) -> Option<(u32, u32)> {
    let extension = extension(path)?;
    if extension.eq_ignore_ascii_case("png") {
        png_size(bytes)
    } else if extension.eq_ignore_ascii_case("jpg") || extension.eq_ignore_ascii_case("jpeg") {
        jpeg_size(bytes)
    } else {
        None
    }
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

/// A JPEG's dimensions are in its first start-of-frame segment: height then
/// width, after the two length bytes. The segments before it are skipped by
/// their own length.
fn jpeg_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.get(..2)? != [0xFF, 0xD8] {
        return None;
    }
    let mut offset = 2;
    loop {
        if *bytes.get(offset)? != 0xFF {
            return None;
        }
        let mut marker = *bytes.get(offset + 1)?;
        // `0xFF` may repeat before a marker as fill bytes.
        while marker == 0xFF {
            offset += 1;
            marker = *bytes.get(offset + 1)?;
        }
        offset += 2;
        match marker {
            // Markers with no length of their own.
            0xD8 | 0xD9 | 0x01 | 0xD0..=0xD7 => continue,
            // A start of frame: precision, height, width. C4, C8 and CC hold
            // other tables and follow the length rule below.
            0xC0..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) => {
                let height = u16::from_be_bytes([*bytes.get(offset + 3)?, *bytes.get(offset + 4)?]);
                let width = u16::from_be_bytes([*bytes.get(offset + 5)?, *bytes.get(offset + 6)?]);
                return Some((u32::from(width), u32::from(height)));
            }
            _ => {
                let length = u16::from_be_bytes([*bytes.get(offset)?, *bytes.get(offset + 1)?]);
                // A length counts itself, so anything shorter would not advance
                // and the scan would never end.
                if length < 2 {
                    return None;
                }
                offset += usize::from(length);
            }
        }
    }
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

    /// The frame header sits after a segment the scan must skip by its length.
    #[test]
    fn a_jpeg_header_skips_the_segments_before_the_frame() {
        let mut bytes = vec![0xFF, 0xD8];
        // An APP0 segment of six bytes, length included.
        bytes.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x06, 1, 2, 3, 4]);
        // A start of frame: length, precision, height, width.
        bytes.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 8]);
        bytes.extend_from_slice(&720u16.to_be_bytes());
        bytes.extend_from_slice(&1280u16.to_be_bytes());
        bytes.extend_from_slice(&[0; 8]);
        assert_eq!(jpeg_size(&bytes), Some((1280, 720)));
        assert_eq!(jpeg_size(b"not a jpeg"), None);
    }

    /// A segment whose length cannot advance the scan ends it instead of
    /// looping forever.
    #[test]
    fn a_jpeg_segment_with_no_length_ends_the_scan() {
        let bytes = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(jpeg_size(&bytes), None);
    }

    /// `webp` and `bmp` are recognised image files with no header reader here,
    /// and an extension alone does not make a file an image.
    #[test]
    fn only_the_measured_formats_answer_a_size() {
        assert_eq!(image_size("a/b.webp", &[0; 32]), None);
        assert_eq!(image_size("a/b.bmp", &[0; 32]), None);
        assert_eq!(image_size("plugin.ron", &[0; 32]), None);
        assert!(is_image("ornament/arch.png"));
        assert!(!is_image("fonts/plex.ttf"));
    }
}
