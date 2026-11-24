//! `ui/plugin.ron`: the manifest's types and the rules its own fields obey
//! (spec §2).
//!
//! The manifest is the plugin's entry point — the file that says which format
//! version the folder is written in — so its rules decide whether the rest of
//! the folder is read at all. Every path it names is confinement-checked here,
//! before a rule or the renderer opens one. Its fonts, images and copy overrides
//! are typed here and judged by the passes that own them; `numbering` is judged
//! here, because it is a value of this file and not a file of its own.

use std::collections::BTreeMap;

use crate::finding::{rules, Finding, Location};

use super::file::read_ron;
use super::source::{check_relative, Source};

/// The manifest's file, relative to the plugin folder.
pub(super) const FILE: &str = "plugin.ron";

/// The format version this build reads, matched exactly (spec §2).
pub const FORMAT_VERSION: u32 = 1;

/// `ui/plugin.ron`.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct Manifest {
    /// The plugin format's version. Any other value fails the plugin, and the
    /// message names this one.
    pub format_version: u32,
    /// The plugin's display name.
    #[serde(default)]
    pub name: String,
    /// How choice numbers display.
    #[serde(default)]
    pub numbering: Numbering,
    /// The faces the plugin ships, each with its license.
    #[serde(default)]
    pub fonts: Vec<FontDecl>,
    /// The art the plugin ships.
    #[serde(default)]
    pub images: Vec<ImageDecl>,
    /// Copy-key overrides: the plugin's words for a key the copy table owns.
    #[serde(default)]
    pub copy: BTreeMap<String, String>,
}

/// How a choice's number displays (spec §5). The keys stay `1`–`9`.
///
/// The manifest authors one of [`Numbering::STYLES`] as a kebab-case string,
/// because no plugin file authors a RON identifier (slice-1 design note, "RON
/// syntax"), and a style that is not one of the five is reported at the
/// manifest's own `numbering` field rather than at the file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Numbering {
    #[default]
    Arabic,
    RomanUpper,
    RomanLower,
    AlphaUpper,
    AlphaLower,
}

impl Numbering {
    /// Each style with the spelling a manifest authors, in the order a message
    /// names them. This is the one list: [`Self::STYLES`] and [`Self::parse`]
    /// both derive from it.
    const ALL: [(Numbering, &'static str); 5] = [
        (Numbering::Arabic, "arabic"),
        (Numbering::RomanUpper, "roman-upper"),
        (Numbering::RomanLower, "roman-lower"),
        (Numbering::AlphaUpper, "alpha-upper"),
        (Numbering::AlphaLower, "alpha-lower"),
    ];

    /// The five authored spellings, in the order a message names them.
    pub const STYLES: [&'static str; 5] = [
        Numbering::ALL[0].1,
        Numbering::ALL[1].1,
        Numbering::ALL[2].1,
        Numbering::ALL[3].1,
        Numbering::ALL[4].1,
    ];

    /// The style `authored` names.
    pub fn parse(authored: &str) -> Option<Numbering> {
        let index = Numbering::ALL
            .iter()
            .position(|(_, style)| *style == authored)?;
        Some(Numbering::ALL[index].0)
    }

    /// How the one-based `number` displays in this style, for a choice's number
    /// badge. The keys that pick it stay `1`–`9` whatever the style (spec §5), so
    /// a roman badge reads `IV` while `4` still chooses it. A zero is shown as
    /// the first of the style, because no option is numbered zero.
    pub fn label(self, number: usize) -> String {
        let number = number.max(1);
        match self {
            Numbering::Arabic => number.to_string(),
            Numbering::RomanUpper => roman(number),
            Numbering::RomanLower => roman(number).to_lowercase(),
            Numbering::AlphaUpper => alpha(number),
            Numbering::AlphaLower => alpha(number).to_lowercase(),
        }
    }
}

/// `number` in upper-case roman numerals, the subtractive form a reader expects
/// (`IV`, `IX`, `XL`).
fn roman(number: usize) -> String {
    const PLACES: [(usize, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut left = number;
    let mut out = String::new();
    for (value, glyph) in PLACES {
        while left >= value {
            out.push_str(glyph);
            left -= value;
        }
    }
    out
}

/// `number` in upper-case letters, one letter for 1–26 and two for 27–52
/// (`A`, `Z`, `AA`), the spreadsheet convention a reader recognises.
fn alpha(number: usize) -> String {
    let mut left = number;
    let mut out = String::new();
    while left > 0 {
        let letter = (left - 1) % 26;
        out.insert(0, char::from(b'A' + letter as u8));
        left = (left - 1) / 26;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::Numbering;

    /// The five styles' first three numbers, and the two the roman and letter
    /// forms spell differently from a digit (spec rule 22).
    #[test]
    fn a_choice_number_displays_in_each_style() {
        let shown = |style: &str, number: usize| Numbering::parse(style).unwrap().label(number);
        assert_eq!(
            (1..=3).map(|n| shown("arabic", n)).collect::<Vec<_>>(),
            ["1", "2", "3"]
        );
        assert_eq!(
            (1..=3).map(|n| shown("roman-upper", n)).collect::<Vec<_>>(),
            ["I", "II", "III"]
        );
        assert_eq!(
            (1..=3).map(|n| shown("roman-lower", n)).collect::<Vec<_>>(),
            ["i", "ii", "iii"]
        );
        assert_eq!(shown("roman-upper", 4), "IV");
        assert_eq!(shown("roman-upper", 9), "IX");
        assert_eq!(shown("roman-upper", 14), "XIV");
        assert_eq!(
            (1..=3).map(|n| shown("alpha-upper", n)).collect::<Vec<_>>(),
            ["A", "B", "C"]
        );
        assert_eq!(
            (1..=3).map(|n| shown("alpha-lower", n)).collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
        assert_eq!(shown("alpha-upper", 26), "Z");
        assert_eq!(shown("alpha-upper", 27), "AA");
        assert_eq!(shown("arabic", 0), "1", "no option is numbered zero");
    }
}

impl<'de> serde::Deserialize<'de> for Numbering {
    fn deserialize<D>(deserializer: D) -> Result<Numbering, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let authored: String = serde::Deserialize::deserialize(deserializer)?;
        Numbering::parse(&authored).ok_or_else(|| serde::de::Error::custom(not_a_style(&authored)))
    }
}

/// One shipped face: a family, its weight, its file and the license it comes
/// under with the file that carries the license text.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct FontDecl {
    pub family: String,
    pub weight: u16,
    pub file: String,
    pub license: String,
    pub license_file: String,
}

/// One shipped image: a file under the plugin folder.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct ImageDecl {
    pub file: String,
}

/// Read and judge `ui/plugin.ron`, returning the manifest the later passes
/// read. `None` means the manifest failed, so the folder is never read on.
pub(super) fn check(source: &dyn Source, findings: &mut Vec<Finding>) -> Option<Manifest> {
    let file = source.display(FILE);
    if !source.paths().iter().any(|path| path == FILE) {
        findings.push(Finding::new(
            file,
            Location::File,
            rules::UI_FILE_READ,
            "the plugin folder has no plugin.ron",
        ));
        return None;
    }
    // A missing file is the folder's own message above; every other read or
    // parse failure is the reader's, exactly as it is for the other files.
    let value = read_ron(source, FILE, "plugin manifest", findings)?;

    // `numbering` is one of five authored strings, and the typed parse can only
    // report a style it does not know at the file. The raw value is read first
    // so the finding names the field the author wrote.
    if !check_numbering(&value, &file, findings) {
        return None;
    }

    let manifest: Manifest = match value.into_rust() {
        Ok(manifest) => manifest,
        Err(error) => {
            findings.push(Finding::new(
                file,
                Location::File,
                rules::UI_PARSE,
                format!("invalid RON: {error}"),
            ));
            return None;
        }
    };

    // A version this build does not read says nothing about the rest of the
    // file's shape, so no later rule runs on it.
    if manifest.format_version != FORMAT_VERSION {
        findings.push(Finding::new(
            file,
            Location::Field("format_version".to_string()),
            rules::UI_VERSION,
            format!(
                "format_version is {}; this build reads version {FORMAT_VERSION}",
                manifest.format_version
            ),
        ));
        return None;
    }

    confine_named_paths(&manifest, &file, findings);
    Some(manifest)
}

/// Whether the manifest's `numbering`, when it is there at all, names one of
/// the five styles. A value of another type is the same failure: the format
/// authors a string.
fn check_numbering(value: &ron::Value, file: &str, findings: &mut Vec<Finding>) -> bool {
    let ron::Value::Map(fields) = value else {
        return true;
    };
    let written = fields
        .iter()
        .find(|(field, _)| matches!(field, ron::Value::String(name) if name == "numbering"))
        .map(|(_, value)| value);
    let message = match written {
        None => return true,
        Some(ron::Value::String(authored)) if Numbering::parse(authored).is_some() => return true,
        Some(ron::Value::String(authored)) => not_a_style(authored),
        Some(_) => format!(
            "numbering is not a string; the five styles are `{}`",
            Numbering::STYLES.join("`, `")
        ),
    };
    findings.push(Finding::new(
        file,
        Location::Field("numbering".to_string()),
        rules::UI_PARSE,
        message,
    ));
    false
}

fn not_a_style(authored: &str) -> String {
    format!(
        "`{authored}` is not a numbering style; the five styles are `{}`",
        Numbering::STYLES.join("`, `")
    )
}

/// Every path the manifest names is checked before anything reads one, so an
/// escape is reported where the author wrote it.
fn confine_named_paths(manifest: &Manifest, file: &str, findings: &mut Vec<Finding>) {
    let font_paths = manifest.fonts.iter().enumerate().flat_map(|(index, font)| {
        [
            (format!("fonts[{index}].file"), font.file.as_str()),
            (
                format!("fonts[{index}].license_file"),
                font.license_file.as_str(),
            ),
        ]
    });
    let image_paths = manifest
        .images
        .iter()
        .enumerate()
        .map(|(index, image)| (format!("images[{index}].file"), image.file.as_str()));

    for (path, named) in font_paths.chain(image_paths) {
        if let Err(reason) = check_relative(named) {
            findings.push(Finding::new(
                file,
                Location::Field(path),
                rules::UI_PATH_ESCAPE,
                reason,
            ));
        }
    }
}
