//! The plugin's fonts: the faces the manifest declares, the license each one
//! names, and the faces a typography token may use (spec §11, §15 rules 8 and
//! 11).
//!
//! A font file is recognised by its extension alone, and a declaration is the
//! only evidence that it may ship, so this pass reads no font bytes and judges
//! no license's terms: whether a license permits redistribution is the author's
//! statement, and no font header carries it. The five faces the player embeds
//! are addressable by family and weight without shipping one (R19, R20), and
//! every other face a typography token names must be declared and present, so
//! the player never asks for a font no file carries.

use crate::finding::{rules, Finding, Location};

use super::manifest::{self, FontDecl, Manifest};
use super::source::{extension, Source};
use super::tokens::{self, Tokens};

/// One of the five faces the player embeds (R19, R20).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmbeddedFace {
    pub family: &'static str,
    pub weight: u16,
}

/// `IBM Plex Sans` at 400, 600 and 700, and `Fira Mono` at 400 and 500: the
/// faces a typography token may name without the plugin shipping one.
pub const EMBEDDED_FACES: &[EmbeddedFace] = &[
    EmbeddedFace {
        family: "IBM Plex Sans",
        weight: 400,
    },
    EmbeddedFace {
        family: "IBM Plex Sans",
        weight: 600,
    },
    EmbeddedFace {
        family: "IBM Plex Sans",
        weight: 700,
    },
    EmbeddedFace {
        family: "Fira Mono",
        weight: 400,
    },
    EmbeddedFace {
        family: "Fira Mono",
        weight: 500,
    },
];

/// Whether one family and weight is an embedded face. A face is the pair, so a
/// shipped `IBM Plex Sans` at another weight is not embedded.
pub fn embedded(family: &str, weight: u16) -> bool {
    EMBEDDED_FACES
        .iter()
        .any(|face| face.family == family && face.weight == weight)
}

/// The extensions that make a plugin file a font (spec §2).
const FONT_EXTENSIONS: [&str; 6] = ["ttf", "otf", "ttc", "woff", "woff2", "eot"];

/// Rule 8's three declaration rules and rule 11's face rule, in one pass: a
/// font finding always names the file or field the author wrote.
pub(super) fn check(
    source: &dyn Source,
    manifest: &Manifest,
    tokens: &Tokens,
    findings: &mut Vec<Finding>,
) {
    let manifest_file = source.display(manifest::FILE);
    for (index, font) in manifest.fonts.iter().enumerate() {
        declaration(source, index, font, &manifest_file, findings);
    }
    undeclared(source, manifest, findings);
    faces(source, manifest, tokens, findings);
}

/// One declaration's license: a named license, and a text file that is present
/// and readable. Either half missing is the same problem as an undeclared font
/// file, because both leave a shipped face with no stated license.
fn declaration(
    source: &dyn Source,
    index: usize,
    font: &FontDecl,
    file: &str,
    findings: &mut Vec<Finding>,
) {
    let face = format!("`{}` at weight {}", font.family, font.weight);
    if font.license.trim().is_empty() {
        findings.push(Finding::new(
            file,
            Location::Field(format!("fonts[{index}].license")),
            rules::UI_FONT_LICENSE,
            format!("{face} names no license"),
        ));
    }
    let path = format!("fonts[{index}].license_file");
    if !source
        .paths()
        .iter()
        .any(|entry| entry == &font.license_file)
    {
        findings.push(Finding::new(
            file,
            Location::Field(path),
            rules::UI_FONT_LICENSE,
            format!("{face} has no license text at `{}`", font.license_file),
        ));
    } else if let Err(reason) = source.read(&font.license_file) {
        findings.push(Finding::new(
            file,
            Location::Field(path),
            rules::UI_FONT_LICENSE,
            format!(
                "cannot read the license text `{}`: {reason}",
                font.license_file
            ),
        ));
    }
}

/// Every font file the folder holds is named by a declaration. The comparison
/// is exact, so a declaration whose case differs from the file's name would
/// still fail on the case-sensitive filesystems the player supports.
fn undeclared(source: &dyn Source, manifest: &Manifest, findings: &mut Vec<Finding>) {
    for path in source.paths() {
        if !is_font(path) || manifest.fonts.iter().any(|font| font.file == *path) {
            continue;
        }
        findings.push(Finding::new(
            source.display(path),
            Location::File,
            rules::UI_FONT_LICENSE,
            format!("`{path}` is a font file that no declaration in plugin.ron names"),
        ));
    }
}

/// Every typography token names a face the player can draw. A declaration ships
/// a face only when the file it names is in the folder, so a token cannot pass
/// on a declaration whose font nobody has. The finding sits at the token's
/// `face`, the field that names the pair, and its message carries the weight
/// that made the pair unknown.
fn faces(source: &dyn Source, manifest: &Manifest, tokens: &Tokens, findings: &mut Vec<Finding>) {
    let file = source.display(tokens::FILE);
    let shipped: Vec<(&str, u16)> = manifest
        .fonts
        .iter()
        .filter(|font| source.paths().iter().any(|path| path == &font.file))
        .map(|font| (font.family.as_str(), font.weight))
        .collect();
    for (name, token) in &tokens.typography {
        if embedded(&token.face, token.weight)
            || shipped
                .iter()
                .any(|(family, weight)| *family == token.face && *weight == token.weight)
        {
            continue;
        }
        findings.push(Finding::new(
            file.clone(),
            Location::Field(format!("typography.{name}.face")),
            rules::UI_FONT_UNKNOWN,
            format!(
                "`{}` at weight {} is neither an embedded face nor declared in plugin.ron",
                token.face, token.weight
            ),
        ));
    }
}

fn is_font(path: &str) -> bool {
    extension(path).is_some_and(|extension| {
        FONT_EXTENSIONS
            .iter()
            .any(|known| known.eq_ignore_ascii_case(extension))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The five faces and no others: a weight of an embedded family that the
    /// player does not embed is not addressable.
    #[test]
    fn only_the_five_embedded_faces_are_embedded() {
        assert_eq!(EMBEDDED_FACES.len(), 5);
        for (family, weight) in [
            ("IBM Plex Sans", 400),
            ("IBM Plex Sans", 600),
            ("IBM Plex Sans", 700),
            ("Fira Mono", 400),
            ("Fira Mono", 500),
        ] {
            assert!(embedded(family, weight), "{family} at {weight}");
        }
        assert!(!embedded("IBM Plex Sans", 500));
        assert!(!embedded("Fira Mono", 700));
        assert!(!embedded("Inter", 400));
        assert!(!embedded("ibm plex sans", 400));
    }

    /// The extension alone decides, in either case, at any depth.
    #[test]
    fn a_font_file_is_recognised_by_its_extension() {
        assert!(is_font("fonts/x.ttf"));
        assert!(is_font("a/b/c/x.WOFF2"));
        assert!(!is_font("fonts/readme.txt"));
        assert!(!is_font("fonts/license"));
        assert!(!is_font("tokens.ron"));
    }
}
