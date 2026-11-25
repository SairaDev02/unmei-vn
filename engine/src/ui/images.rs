//! The plugin's images: every declared image is a PNG (spec §11, §15 rule 39).
//!
//! An image is judged by its declaration and its header, never by a decoder:
//! the name the manifest wrote must end `.png`, in either case as the font
//! extensions are read, and the file's first eight bytes must be the PNG
//! signature. A plugin therefore fails at the check for the same reason the
//! player would skip the texture at boot, so the check and the boot agree on
//! every image (rule 25). The image cap is [`super::caps`]'s: it measures the
//! header of every PNG in the folder, declared or not.

use crate::finding::{rules, Finding, Location};

use super::manifest::Manifest;
use super::source::{extension, Source};

/// The eight bytes every PNG starts with, and the signature the cap reads.
pub(super) const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// Whether a manifest path names a PNG.
pub(super) fn is_png(path: &str) -> bool {
    extension(path).is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
}

/// Rule 39: every image the manifest declares has a `.png` name and PNG bytes.
/// A declared file the folder does not hold is left to the renderer, which logs
/// and skips it: the format rule speaks about the name and the bytes an author
/// shipped, and a missing file has neither.
pub(super) fn check(source: &dyn Source, manifest: &Manifest, findings: &mut Vec<Finding>) {
    for image in &manifest.images {
        let file = image.file.as_str();
        if !is_png(file) {
            findings.push(finding(
                source,
                file,
                format!(
                    "`{file}` is not a PNG; every plugin image has a `.png` name and PNG bytes"
                ),
            ));
            continue;
        }
        let Ok(bytes) = source.read(file) else {
            continue;
        };
        if !bytes.starts_with(&PNG_SIGNATURE) {
            findings.push(finding(
                source,
                file,
                format!("`{file}` is named `.png` but its bytes are not PNG"),
            ));
        }
    }
}

fn finding(source: &dyn Source, file: &str, message: String) -> Finding {
    Finding::new(
        source.display(file),
        Location::File,
        rules::UI_IMAGE_FORMAT,
        message,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The extension alone decides, in either case, and nothing else is a PNG.
    #[test]
    fn a_png_is_recognised_by_its_extension() {
        assert!(is_png("ornament/arch.png"));
        assert!(is_png("a/b/c/arch.PNG"));
        assert!(!is_png("ornament/arch.jpg"));
        assert!(!is_png("ornament/arch"));
        assert!(!is_png("png"));
    }
}
