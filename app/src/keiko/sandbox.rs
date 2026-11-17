//! The per-take sandbox (spec §4.2): a fresh temp root holding a text-only copy
//! of the pack, the sidecars a take writes, its config dir and its session log.
//!
//! The pack's own `assets/` and `tests/` are never copied: assets are read in
//! place through [`RuntimeConfig::asset_root`](crate::RuntimeConfig) and the
//! cases stay in the original pack, so nothing a take does can write to the
//! user's pack.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// One take's sandbox folder.
#[derive(Debug, Clone)]
pub struct Sandbox {
    /// `<os-temp>/unmei-keiko-<pid>-<nonce>/`.
    root: PathBuf,
    /// `<root>/take-<n>/`.
    take_dir: PathBuf,
    /// `<take_dir>/<pack-name>/` — the copied pack.
    pack_root: PathBuf,
    /// The original pack's `assets/`, read in place.
    asset_root: PathBuf,
    /// `<take_dir>/config/` — the credential config dir.
    config_dir: PathBuf,
}

static NONCE: AtomicU64 = AtomicU64::new(0);

impl Sandbox {
    /// Create the root and its first take folder, and copy the pack's text files
    /// into it.
    pub fn create(original_pack: &Path, take_index: usize) -> io::Result<Self> {
        let nonce = format!(
            "{}-{}",
            std::process::id(),
            NONCE.fetch_add(1, Ordering::Relaxed)
        );
        let root = std::env::temp_dir().join(format!("unmei-keiko-{nonce}"));
        Self::create_in(&root, original_pack, take_index)
    }

    /// Create the sandbox under `root`, so a test can name its own root.
    pub fn create_in(root: &Path, original_pack: &Path, take_index: usize) -> io::Result<Self> {
        let take_dir = root.join(format!("take-{take_index}"));
        let pack_name = original_pack
            .file_name()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("pack"));
        let pack_root = take_dir.join(&pack_name);
        let asset_root = original_pack.join("assets");
        let config_dir = take_dir.join("config");

        fs::create_dir_all(&pack_root)?;
        fs::create_dir_all(&config_dir)?;
        copy_text_pack(original_pack, &pack_root)?;

        Ok(Self {
            root: root.to_path_buf(),
            take_dir,
            pack_root,
            asset_root,
            config_dir,
        })
    }

    /// The copied pack a take plays.
    pub fn pack_root(&self) -> &Path {
        &self.pack_root
    }

    /// The original pack's `assets/` folder, read-only.
    pub fn asset_root(&self) -> &Path {
        &self.asset_root
    }

    /// The take's own config dir, so no credential file is ever touched.
    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    /// The take's session-log path: `<pack-name>.log`, beside the copied pack.
    pub fn session_log_path(&self) -> PathBuf {
        crate::failure::session_log_path_for(&self.pack_root)
    }

    /// The take folder itself, for `--keep` and for a test to inspect.
    pub fn take_dir(&self) -> &Path {
        &self.take_dir
    }

    /// The sandbox root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The path a fixture of this kind lands at.
    pub fn fixture_path(&self, kind: unmei_engine::keiko::FixtureKind) -> PathBuf {
        use unmei_engine::keiko::FixtureKind;
        let pack = self.paths_pack();
        match kind {
            FixtureKind::Slot(slot) => unmei_engine::save_path_for(&pack, slot),
            FixtureKind::LegacySave => unmei_engine::legacy_save_path_for(&pack),
            FixtureKind::ReadLog => unmei_engine::read_log_path_for(&pack),
            FixtureKind::Settings => crate::settings::settings_path_for(&self.pack_root),
            FixtureKind::Credential => self.config_dir.join("unmei").join("credentials"),
            FixtureKind::EnvKey => self.take_dir.join("env-key"),
        }
    }

    /// A pack handle carrying only the sandbox root, for the sidecar path
    /// producers (they read `root` alone).
    pub fn paths_pack(&self) -> unmei_engine::Pack {
        unmei_engine::Pack {
            manifest: unmei_engine::Manifest {
                name: String::new(),
                entry: String::new(),
                tagline: None,
                title_backdrop: None,
                scenes: Vec::new(),
            },
            state: Default::default(),
            scenes: Default::default(),
            npcs: Default::default(),
            root: self.pack_root.clone(),
        }
    }

    /// Remove the take folder unless the run asked to keep it, then the root
    /// when it is empty. Returns the kept folder's path.
    ///
    /// Removal is retried briefly: a task-pool thread can still be flushing the
    /// take's last log line on Windows, and a folder whose file is still open
    /// cannot be removed. A leftover is always empty when this gives up.
    pub fn finish(&self, keep: bool) -> Option<PathBuf> {
        if keep {
            return Some(self.take_dir.clone());
        }
        for _ in 0..20 {
            let _ = fs::remove_dir_all(&self.take_dir);
            if fs::remove_dir_all(&self.root).is_ok() || !self.root.exists() {
                return None;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        None
    }
}

/// Copy every file of a pack except `assets/` and `tests/`, recursively.
fn copy_text_pack(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == "assets" || name == "tests" {
            continue;
        }
        let source = entry.path();
        let target = to.join(&name);
        if entry.file_type()?.is_dir() {
            copy_text_pack(&source, &target)?;
        } else {
            fs::copy(&source, &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sandbox_copies_the_text_pack_and_not_its_assets_or_tests() {
        let source = tempfile::tempdir().unwrap();
        let pack = source.path().join("demo-pack");
        fs::create_dir_all(pack.join("scenes")).unwrap();
        fs::create_dir_all(pack.join("assets/backgrounds")).unwrap();
        fs::create_dir_all(pack.join("tests")).unwrap();
        fs::write(pack.join("pack.ron"), "(name: \"x\")").unwrap();
        fs::write(pack.join("scenes/a.unmei"), "Narrator: hi\n").unwrap();
        fs::write(pack.join("assets/backgrounds/a.png"), b"png").unwrap();
        fs::write(pack.join("tests/a.keiko"), "keiko 1\n").unwrap();

        let root = source.path().join("sandbox");
        let sandbox = Sandbox::create_in(&root, &pack, 1).unwrap();
        assert_eq!(sandbox.pack_root(), root.join("take-1/demo-pack"));
        assert!(sandbox.pack_root().join("scenes/a.unmei").exists());
        assert!(sandbox.pack_root().join("pack.ron").exists());
        assert!(!sandbox.pack_root().join("assets").exists());
        assert!(!sandbox.pack_root().join("tests").exists());
        assert_eq!(
            sandbox.session_log_path(),
            root.join("take-1/demo-pack.log"),
            "sidecars land beside the copied pack"
        );
        assert_eq!(sandbox.asset_root(), pack.join("assets"));
        assert!(sandbox.config_dir().is_dir());

        sandbox.finish(false);
        assert!(!root.join("take-1").exists());
    }

    #[test]
    fn a_kept_sandbox_stays_and_reports_its_path() {
        let source = tempfile::tempdir().unwrap();
        let pack = source.path().join("demo-pack");
        fs::create_dir_all(&pack).unwrap();
        fs::write(pack.join("pack.ron"), "(name: \"x\")").unwrap();

        let sandbox = Sandbox::create_in(&source.path().join("root"), &pack, 2).unwrap();
        let kept = sandbox.finish(true).expect("a kept path");
        assert!(kept.exists());
        assert_eq!(kept, source.path().join("root/take-2"));

        fs::remove_dir_all(source.path().join("root")).unwrap();
    }
}
