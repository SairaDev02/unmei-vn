//! `unmei-play --export-ui`: the embedded reference plugin written into a pack as
//! an author's starting point (UI plugin spec §1, §11).
//!
//! Every file comes from [`crate::reference_plugin::FILES`], the plugin's one
//! owner, so the exported look is the reference look byte for byte and both meet
//! the same static rules. Whether the folder may be written at all is the
//! launch's rule ([`crate::launch::export_target`]), which runs first.
//!
//! This is the only writer of a pack's `ui/` folder. Like `--check` and the Keiko
//! modes it is not a play session: it opens no window and writes no setting, save
//! slot, read log or session log, and reads no sidecar or credential file
//! (spec §1).
//!
//! A write that fails part-way leaves the files already written behind, because
//! the folder is not staged and renamed. The spec's recovery is then the same as
//! for an occupied `ui/` — delete it and start over — so the failure names the
//! folder rather than leaving the author to guess which one.

use std::path::{Path, PathBuf};

use crate::keiko::output::{FAIL, PASS};
use crate::reference_plugin;

/// Write the reference plugin into `ui` and return the process exit code: 0
/// written, 1 a file could not be written.
pub fn run(ui: &Path) -> u8 {
    match write(ui) {
        Ok(count) => {
            println!("exported {count} files into {}", ui.display());
            PASS
        }
        Err(failure) => {
            eprintln!("unmei-play: {failure}");
            FAIL
        }
    }
}

/// A refused write: the path that failed, its folder and the operating system's
/// reason.
struct WriteFailure {
    path: PathBuf,
    folder: PathBuf,
    error: std::io::Error,
}

impl WriteFailure {
    fn new(path: PathBuf, folder: &Path, error: std::io::Error) -> Self {
        Self {
            path,
            folder: folder.to_path_buf(),
            error,
        }
    }
}

impl std::fmt::Display for WriteFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "could not write {}: {}; delete {} before retrying",
            self.path.display(),
            self.error,
            self.folder.display()
        )
    }
}

fn write(ui: &Path) -> Result<usize, WriteFailure> {
    for (path, text) in reference_plugin::FILES {
        let file = ui.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| WriteFailure::new(parent.to_path_buf(), ui, error))?;
        }
        std::fs::write(&file, text).map_err(|error| WriteFailure::new(file.clone(), ui, error))?;
    }
    Ok(reference_plugin::FILES.len())
}
