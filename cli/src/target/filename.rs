//! The lexical filename policy for authoring ids and its one terminal refusal
//! (author-cli contract §10).
//!
//! Nothing here is trimmed, renamed, case-folded or Unicode-normalized: a check
//! is pure policy over an id, a [`FileKind`] and an explicitly selected
//! [`Platform`], so each platform's verdict is testable on either host.
//!
//! [`TargetError`] is the one shared terminal refusal for an authoring target.
//! `Display` carries the kind, the debug-escaped offending id and a useful
//! reason. It never carries credentials or an absolute staged path.

use std::fmt;

/// The fixed file kind behind an authoring target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    /// `npcs/<id>.ron`.
    Npc,
    /// `scenes/<id>.unmei`.
    Scene,
}

impl FileKind {
    /// The loader's fixed extension for this kind, without the dot.
    pub(super) fn extension(self) -> &'static str {
        match self {
            FileKind::Npc => "ron",
            FileKind::Scene => "unmei",
        }
    }

    /// The pack subdirectory this kind lives in.
    pub(super) fn subdirectory(self) -> &'static str {
        match self {
            FileKind::Npc => "npcs",
            FileKind::Scene => "scenes",
        }
    }

    /// The noun used in a refusal message.
    fn label(self) -> &'static str {
        match self {
            FileKind::Npc => "NPC",
            FileKind::Scene => "Scene",
        }
    }
}

/// The filename/path semantics a check is evaluated against.
///
/// Production selects [`Platform::native`]; the pure matrix evaluates both so
/// each platform's policy is explicit and testable on either host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Platform {
    Windows,
    Linux,
}

impl Platform {
    /// The platform the running build authors for.
    pub(super) const fn native() -> Self {
        if cfg!(windows) {
            Platform::Windows
        } else {
            Platform::Linux
        }
    }

    fn separators(self) -> &'static [char] {
        match self {
            Platform::Windows => &['/', '\\'],
            Platform::Linux => &['/'],
        }
    }
}

/// Why an id cannot become one ordinary fixed-extension filename.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilenameFault {
    Empty,
    Nul,
    Separator(char),
    ReservedCharacter(char),
    Control(char),
    AlternateStream,
    ReservedDevice(String),
    NotOneComponent,
    StemMismatch,
}

impl fmt::Display for FilenameFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FilenameFault::Empty => write!(f, "the id is empty"),
            FilenameFault::Nul => write!(f, "the id contains a NUL character"),
            FilenameFault::Separator(c) => write!(f, "the id contains the path separator `{c}`"),
            FilenameFault::ReservedCharacter(c) => write!(
                f,
                "the completed Windows filename contains the reserved character `{c}`"
            ),
            FilenameFault::Control(c) => {
                write!(
                    f,
                    "the id contains the control character U+{:04X}",
                    *c as u32
                )
            }
            FilenameFault::AlternateStream => write!(
                f,
                "the id contains `:`, which names a Windows alternate data stream"
            ),
            FilenameFault::ReservedDevice(name) => {
                write!(f, "`{name}` is a reserved Windows device name")
            }
            FilenameFault::NotOneComponent => {
                write!(
                    f,
                    "the completed filename is not one ordinary path component"
                )
            }
            FilenameFault::StemMismatch => write!(
                f,
                "the completed filename does not round-trip to the id as its stem"
            ),
        }
    }
}

/// The one shared terminal refusal for an unsafe authoring target.
///
/// `Display` carries the kind, the debug-escaped offending id and a useful
/// reason. It never carries credentials or an absolute staged path.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TargetError {
    #[error("unsafe {} target {id:?}: {fault}", .kind.label())]
    UnsafeFilename {
        kind: FileKind,
        id: String,
        fault: FilenameFault,
    },
    #[error(
        "no scene {id:?} in the pack — drafting and scene validation require an existing scene"
    )]
    AbsentScene { id: String },
}

/// Check an id for one fixed-extension filename on `platform`.
///
/// This is the pure policy: it depends on no host state beyond the id and the
/// explicitly selected platform, so both variants are testable on either OS.
/// Empty and NUL ids refuse everywhere. Linux refuses `/` and accepts ordinary
/// `\`/`:`; Windows refuses both separators, root/drive/UNC/device prefixes,
/// stream `:`, the reserved characters and controls, and case-insensitive
/// device basenames including extension-bearing and COM/LPT superscript-digit
/// forms. The completed filename must be one ordinary component whose
/// loader-style stem is exactly the id.
pub(super) fn check_filename(
    id: &str,
    kind: FileKind,
    platform: Platform,
) -> Result<(), TargetError> {
    let fail = |fault: FilenameFault| {
        Err(TargetError::UnsafeFilename {
            kind,
            id: id.to_string(),
            fault,
        })
    };

    if id.is_empty() {
        return fail(FilenameFault::Empty);
    }
    if id.contains('\0') {
        return fail(FilenameFault::Nul);
    }

    match platform {
        Platform::Linux => {
            if id.contains('/') {
                return fail(FilenameFault::Separator('/'));
            }
        }
        Platform::Windows => {
            for c in id.chars() {
                match c {
                    '/' | '\\' => return fail(FilenameFault::Separator(c)),
                    ':' => return fail(FilenameFault::AlternateStream),
                    '<' | '>' | '"' | '|' | '?' | '*' => {
                        return fail(FilenameFault::ReservedCharacter(c))
                    }
                    c if (c as u32) < 0x20 => return fail(FilenameFault::Control(c)),
                    _ => {}
                }
            }
            if let Some(device) = reserved_device(id) {
                return fail(FilenameFault::ReservedDevice(device));
            }
        }
    }

    let filename = format!("{id}.{}", kind.extension());
    if !is_one_ordinary_component(&filename, platform) {
        return fail(FilenameFault::NotOneComponent);
    }
    if loader_stem(&filename) != Some(id) {
        return fail(FilenameFault::StemMismatch);
    }
    Ok(())
}

/// Case-insensitive Windows device basenames, including the documented
/// extension-bearing and COM/LPT superscript-digit forms. The fixed extension
/// does not turn `NUL` into a regular file.
const RESERVED_DEVICES: &[&str] = &[
    "CON",
    "PRN",
    "AUX",
    "NUL",
    "COM1",
    "COM2",
    "COM3",
    "COM4",
    "COM5",
    "COM6",
    "COM7",
    "COM8",
    "COM9",
    "LPT1",
    "LPT2",
    "LPT3",
    "LPT4",
    "LPT5",
    "LPT6",
    "LPT7",
    "LPT8",
    "LPT9",
    "COM\u{00B9}",
    "COM\u{00B2}",
    "COM\u{00B3}",
    "LPT\u{00B9}",
    "LPT\u{00B2}",
    "LPT\u{00B3}",
];

/// The device basename an id resolves to, if any: the part before the first
/// dot, compared case-insensitively.
fn reserved_device(id: &str) -> Option<String> {
    let base = id.split('.').next().unwrap_or(id);
    let upper = base.to_uppercase();
    RESERVED_DEVICES
        .iter()
        .find(|device| **device == upper)
        .map(|device| (*device).to_string())
}

/// Whether the completed filename is one ordinary component under `platform`.
///
/// Separators are already refused per platform, so this guards the normalized
/// component traps (`"."`, `".."`, any embedded separator) without relying on
/// the host's `Path`, which would apply the wrong platform's separators.
fn is_one_ordinary_component(filename: &str, platform: Platform) -> bool {
    !filename.is_empty()
        && filename != "."
        && filename != ".."
        && !filename.contains(platform.separators())
}

/// The loader-style stem: `Path::file_stem`'s dot rule, independent of host
/// separators. The completed filename already has no separator, so its
/// `file_name` is the whole string.
fn loader_stem(filename: &str) -> Option<&str> {
    if filename == ".." {
        return Some(filename);
    }
    let bytes = filename.as_bytes();
    let rest = bytes.get(1..)?;
    let last_dot = rest.iter().rposition(|byte| *byte == b'.')? + 1;
    Some(&filename[..last_dot])
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    /// One matrix row: the id, and whether each platform accepts it.
    struct Case {
        id: &'static str,
        windows: bool,
        linux: bool,
    }

    fn rejects(case: &Case, platform: Platform) -> bool {
        check_filename(case.id, FileKind::Scene, platform).is_err()
    }

    /// The compact, explicit Windows/Linux policy matrix.
    ///
    /// Both fixed extensions are exercised: the kind only changes the appended
    /// extension, and every completed name is still one ordinary component.
    #[test]
    fn policy_matrix_is_explicit_for_both_platforms() {
        let cases = [
            // Empty and NUL refuse everywhere.
            Case {
                id: "",
                windows: false,
                linux: false,
            },
            Case {
                id: "a\0b",
                windows: false,
                linux: false,
            },
            // Ordinary stems accepted everywhere.
            Case {
                id: "ace",
                windows: true,
                linux: true,
            },
            Case {
                id: ".hidden",
                windows: true,
                linux: true,
            },
            Case {
                id: "act.one",
                windows: true,
                linux: true,
            },
            Case {
                id: "café",
                windows: true,
                linux: true,
            },
            Case {
                id: "act one",
                windows: true,
                linux: true,
            },
            // Deliberate dot-only stems are ordinary extended names, not paths.
            Case {
                id: ".",
                windows: true,
                linux: true,
            },
            Case {
                id: "..",
                windows: true,
                linux: true,
            },
            // Trailing dot/space stems stay eligible: the fixed extension means
            // the completed name does not end in a dot or space.
            Case {
                id: "x.",
                windows: true,
                linux: true,
            },
            Case {
                id: "x ",
                windows: true,
                linux: true,
            },
            // Slash always refuses; Windows also refuses backslash.
            Case {
                id: "a/b",
                windows: false,
                linux: false,
            },
            Case {
                id: "a/./b",
                windows: false,
                linux: false,
            },
            Case {
                id: "a/../b",
                windows: false,
                linux: false,
            },
            Case {
                id: "a\\b",
                windows: false,
                linux: true,
            },
            // Colon: an ADS on Windows, an ordinary Linux filename character.
            Case {
                id: "a:b",
                windows: false,
                linux: true,
            },
            Case {
                id: "C:pack",
                windows: false,
                linux: true,
            },
            // Absolute/root/UNC/device path shapes.
            Case {
                id: "/abs",
                windows: false,
                linux: false,
            },
            Case {
                id: "\\abs",
                windows: false,
                linux: true,
            },
            Case {
                id: "\\\\server\\share",
                windows: false,
                linux: true,
            },
            Case {
                id: "\\\\.\\NUL",
                windows: false,
                linux: true,
            },
            Case {
                id: "\\\\?\\C:\\x",
                windows: false,
                linux: true,
            },
            // Windows reserved characters and controls.
            Case {
                id: "a<b",
                windows: false,
                linux: true,
            },
            Case {
                id: "a>b",
                windows: false,
                linux: true,
            },
            Case {
                id: "a\"b",
                windows: false,
                linux: true,
            },
            Case {
                id: "a|b",
                windows: false,
                linux: true,
            },
            Case {
                id: "a?b",
                windows: false,
                linux: true,
            },
            Case {
                id: "a*b",
                windows: false,
                linux: true,
            },
            Case {
                id: "a\u{01}b",
                windows: false,
                linux: true,
            },
            // Windows device basenames, case-insensitively and with extensions.
            Case {
                id: "CON",
                windows: false,
                linux: true,
            },
            Case {
                id: "con",
                windows: false,
                linux: true,
            },
            Case {
                id: "NUL",
                windows: false,
                linux: true,
            },
            Case {
                id: "nul.ron",
                windows: false,
                linux: true,
            },
            Case {
                id: "AUX.txt",
                windows: false,
                linux: true,
            },
            Case {
                id: "PRN",
                windows: false,
                linux: true,
            },
            Case {
                id: "COM1",
                windows: false,
                linux: true,
            },
            Case {
                id: "lpt9",
                windows: false,
                linux: true,
            },
            // COM/LPT superscript-digit device forms.
            Case {
                id: "COM\u{00B9}",
                windows: false,
                linux: true,
            },
            Case {
                id: "com\u{00B2}.txt",
                windows: false,
                linux: true,
            },
            Case {
                id: "LPT\u{00B3}",
                windows: false,
                linux: true,
            },
        ];

        for case in &cases {
            for kind in [FileKind::Npc, FileKind::Scene] {
                // The kind only changes the appended extension, so the verdict
                // is the same for both; assert that rather than duplicating the
                // matrix.
                assert_eq!(
                    check_filename(case.id, kind, Platform::Windows).is_ok(),
                    case.windows,
                    "windows {:?} {:?}",
                    kind,
                    case.id
                );
                assert_eq!(
                    check_filename(case.id, kind, Platform::Linux).is_ok(),
                    case.linux,
                    "linux {:?} {:?}",
                    kind,
                    case.id
                );
            }
            assert_eq!(
                rejects(case, Platform::Windows),
                !case.windows,
                "windows verdict for {:?}",
                case.id
            );
        }
    }

    /// The completed filename must round-trip to the exact id under the
    /// loader's own stem rule, for the accepted unusual stems.
    #[test]
    fn accepted_completed_names_round_trip_through_the_loader_stem() {
        for id in [
            ".hidden", "act.one", ".", "..", "x.", "x ", "café", "act one",
        ] {
            for kind in [FileKind::Npc, FileKind::Scene] {
                check_filename(id, kind, Platform::native()).expect("accepted");
                let filename = format!("{id}.{}", kind.extension());
                let stem = Path::new(&filename)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default();
                if cfg!(windows) {
                    // Windows hosts also expose the completed name as one
                    // ordinary component.
                    assert_eq!(
                        Path::new(&filename).components().count(),
                        1,
                        "{filename} is one component"
                    );
                }
                assert_eq!(stem, id, "{filename} stem");
            }
        }
    }

    /// The terminal error carries the kind, the escaped id and a reason, and
    /// never a filesystem path.
    #[test]
    fn target_error_escapes_the_id_and_names_the_kind() {
        let err = check_filename("a/b", FileKind::Scene, Platform::native()).unwrap_err();
        let text = err.to_string();
        assert!(text.contains("Scene"), "{text}");
        assert!(text.contains("\"a/b\""), "{text}");
        assert!(text.contains("separator"), "{text}");
        assert!(!text.contains("npcs/"), "no staged path: {text}");

        let missing = TargetError::AbsentScene {
            id: "no_such".to_string(),
        };
        assert!(missing.to_string().contains("\"no_such\""), "{missing}");
    }
}
