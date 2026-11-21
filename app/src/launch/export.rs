//! The `--export-ui` mode on `unmei-play` (UI plugin spec §1).
//!
//! Export is a mode of its own: it takes no operand, refuses `--dev`, and joins
//! the exclusive group that `--check`, `--preview` and the Keiko modes form. It
//! writes the embedded reference plugin into the pack's `ui/` folder as an
//! author's starting point ([`crate::export_ui`]).
//!
//! The grammar never touches the filesystem, so the one rule that needs it — the
//! folder must be absent or empty, because an export never mixes the reference
//! plugin into an author's own files — is a separate step that runs where the
//! pack root is resolved ([`export_target`]).

use std::path::{Path, PathBuf};

use super::{KeikoMode, UsageError};

/// The pack-relative folder a plugin lives in (ui-plugin spec §2). The refusal
/// builds its path from this one name and the export writes into that path, so
/// the folder checked empty is the folder written.
const UI_FOLDER: &str = "ui";

/// The export half of one launch: whether `--export-ui` was asked for.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ExportArgs {
    requested: bool,
}

impl ExportArgs {
    pub fn request(&mut self) {
        self.requested = true;
    }

    /// Fold the flag into the launch. One mode per invocation, so `--check`,
    /// `--preview`, a Keiko mode or `--dev` alongside it is a usage error.
    pub fn finish(
        &self,
        dev: bool,
        check: bool,
        keiko: Option<&KeikoMode>,
    ) -> Result<bool, UsageError> {
        if !self.requested {
            return Ok(false);
        }
        if dev {
            return Err(UsageError::new("--dev cannot be combined with --export-ui"));
        }
        if check {
            return Err(UsageError::new(
                "--export-ui cannot be combined with --check; choose one mode",
            ));
        }
        if let Some(mode) = keiko {
            return Err(UsageError::new(format!(
                "--export-ui cannot be combined with {}; choose one mode",
                mode.flag()
            )));
        }
        Ok(true)
    }
}

/// The folder an export writes into, or the usage error that refuses it. A pack
/// with no `ui/` or an empty one is writable; a `ui/` holding anything stays
/// exactly as the author left it (spec §1).
///
/// The pack folder itself must already exist: export creates the plugin folder
/// and the surfaces under it, never the pack, so a mistyped path makes nothing
/// (spec §1, rule 32).
pub fn export_target(pack_root: &Path) -> Result<PathBuf, UsageError> {
    if !pack_root.is_dir() {
        return Err(UsageError::new(format!(
            "--export-ui needs an existing pack folder, and {} is not one; check the path",
            pack_root.display()
        )));
    }
    let ui = pack_root.join(UI_FOLDER);
    if !ui.exists() {
        return Ok(ui);
    }
    if !ui.is_dir() {
        return Err(occupied(&ui));
    }
    let mut entries = std::fs::read_dir(&ui).map_err(|error| {
        // A folder that cannot be read cannot be shown empty, so it is refused
        // like an occupied one; the reason says which case it is.
        UsageError::new(format!(
            "--export-ui cannot read {}: {error}; fix its permissions, or delete or rename it to start over",
            ui.display()
        ))
    })?;
    if entries.next().is_some() {
        return Err(occupied(&ui));
    }
    Ok(ui)
}

fn occupied(ui: &Path) -> UsageError {
    UsageError::new(format!(
        "--export-ui needs a pack with no ui/ folder or an empty one, and {} is not empty; delete or rename it to start over",
        ui.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `ui/` that is absent or empty is writable and named; one holding a file
    /// or a folder is refused, and the refusal creates nothing.
    #[test]
    fn only_an_absent_or_empty_folder_is_an_export_target() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let pack = dir.path().join("pack");
        std::fs::create_dir(&pack).expect("the pack folder");

        assert_eq!(export_target(&pack).expect("no ui/"), pack.join(UI_FOLDER));

        let ui = pack.join(UI_FOLDER);
        std::fs::create_dir(&ui).expect("an empty ui/");
        assert_eq!(export_target(&pack).expect("an empty ui/"), ui);

        std::fs::write(ui.join("plugin.ron"), "(format_version: 1)").expect("an author's file");
        let error = export_target(&pack).expect_err("a non-empty ui/");
        assert!(error.to_string().contains("not empty"), "{error}");
        assert!(
            error.to_string().contains(&ui.display().to_string()),
            "the refusal names the folder: {error}"
        );

        // A file where the folder belongs is refused the same way: nothing can
        // be written under it.
        let other = dir.path().join("other");
        std::fs::create_dir(&other).expect("a second pack folder");
        std::fs::write(other.join(UI_FOLDER), "not a folder").expect("a file named ui");
        assert!(export_target(&other).is_err());
    }

    /// A pack folder that does not exist is a usage error, and the check leaves
    /// the filesystem exactly as it found it (spec §1, rule 32).
    #[test]
    fn a_missing_pack_folder_is_refused_and_creates_nothing() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let missing = dir.path().join("typoed-pack");

        let error = export_target(&missing).expect_err("an absent pack folder");
        assert!(
            error.to_string().contains(&missing.display().to_string()),
            "the refusal names the path: {error}"
        );
        assert!(!missing.exists(), "nothing is created for a mistyped path");
    }
}
