//! The per-take sandbox (spec §4.2): a fresh temp root holding a text-only copy
//! of the pack, the sidecars a take writes, its config dir and its session log.
//!
//! The pack's own `assets/` and `tests/` are never copied, nor is a `.git/`: assets are read in
//! place through [`RuntimeConfig::asset_root`](crate::RuntimeConfig) and the
//! cases stay in the original pack, so nothing a take does can write to the
//! user's pack.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// The first free `unmei-keiko-<pid>-<n>` root under `base` from `nonce` on,
/// created on the spot so no two takes can pick it. A recycled process id can
/// leave a kept sandbox behind, and a take that reused it would boot with
/// another run's slots, read log or settings instead of a clean one (spec §4.2).
fn fresh_root_in(base: &Path, mut nonce: u64) -> io::Result<PathBuf> {
    loop {
        let root = base.join(format!("unmei-keiko-{}-{nonce}", std::process::id()));
        match fs::create_dir(&root) {
            Ok(()) => return Ok(root),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => nonce += 1,
            Err(error) => return Err(error),
        }
    }
}

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
        Self::create_under(&std::env::temp_dir(), original_pack, take_index)
    }

    /// [`Sandbox::create`] with its root under `base`.
    fn create_under(base: &Path, original_pack: &Path, take_index: usize) -> io::Result<Self> {
        let root = fresh_root_in(base, NONCE.fetch_add(1, Ordering::Relaxed))?;
        Self::create_in(&root, original_pack, take_index).inspect_err(|_| {
            let _ = fs::remove_dir_all(&root);
        })
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
            FixtureKind::Credential => {
                crate::credentials::credentials_path_in(&self.credential_dir())
            }
            FixtureKind::EnvKey => self.take_dir.join("env-key"),
        }
    }

    /// The config directory the credential reader is pointed at: the take's
    /// own `unmei/` folder, so `credentials_path_in` lands on the fixture and
    /// never on the developer's real `%APPDATA%`/`~/.config` (spec §7.1).
    pub fn credential_dir(&self) -> PathBuf {
        self.config_dir.join("unmei")
    }

    /// The `env-key` fixture's text, when the case wrote one.
    pub fn env_key(&self) -> Option<String> {
        std::fs::read_to_string(self.fixture_path(unmei_engine::keiko::FixtureKind::EnvKey))
            .ok()
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty())
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
    /// cannot be removed. When it gives up, the sandbox root stays with
    /// whatever `remove_dir_all` could not delete, such as the locked file, and
    /// its path is written to stderr.
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
        eprintln!(
            "unmei-play: could not remove the take sandbox {}",
            self.root.display()
        );
        None
    }
}

/// Copy every file of a pack except `assets/`, `tests/` and `.git/`,
/// recursively. Each file must pass
/// [`check_pack_file`](unmei_engine::check_pack_file), so a symlink or an
/// oversized file fails the copy rather than reaching outside the pack.
fn copy_text_pack(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == "assets" || name == "tests" || name == ".git" {
            continue;
        }
        let source = entry.path();
        let target = to.join(&name);
        if entry.file_type()?.is_dir() {
            copy_text_pack(&source, &target)?;
        } else {
            copy_pack_file(&source, &target)?;
        }
    }
    Ok(())
}

/// Copy one pack file into the sandbox once it passes
/// [`check_pack_file`](unmei_engine::check_pack_file), naming the file in the
/// error.
pub(crate) fn copy_pack_file(source: &Path, target: &Path) -> io::Result<()> {
    unmei_engine::check_pack_file(source)
        .map_err(|error| io::Error::new(error.kind(), format!("{}: {error}", source.display())))?;
    fs::copy(source, target)?;
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

    /// A leftover sandbox at the next root's own name — what a recycled process
    /// id leaves after `--keep` — must be stepped over, not reused.
    #[test]
    fn a_take_never_reuses_an_existing_sandbox_root() {
        let start = NONCE.fetch_add(1, Ordering::Relaxed);
        let taken =
            std::env::temp_dir().join(format!("unmei-keiko-{}-{start}", std::process::id()));
        fs::create_dir_all(taken.join("take-1")).unwrap();
        fs::write(taken.join("take-1/old.slot1.save.ron"), "stale").unwrap();

        let root = fresh_root_in(&std::env::temp_dir(), start).unwrap();
        assert_ne!(root, taken, "a fresh root is never an existing one");
        assert_eq!(
            fs::read_dir(&root).unwrap().count(),
            0,
            "and it starts empty"
        );
        fs::remove_dir_all(&root).unwrap();
        fs::remove_dir_all(&taken).unwrap();
    }

    #[test]
    fn a_failed_pack_copy_leaves_no_sandbox_folder() {
        let base = tempfile::tempdir().unwrap();
        let missing = base.path().join("no-such-pack");
        let watched = base.path().join("temp");
        fs::create_dir(&watched).unwrap();

        assert!(Sandbox::create_under(&watched, &missing, 1).is_err());
        assert_eq!(
            fs::read_dir(&watched).unwrap().count(),
            0,
            "no unmei-keiko folder is left behind"
        );
    }

    #[test]
    fn a_sandbox_skips_a_git_folder() {
        let source = tempfile::tempdir().unwrap();
        let pack = source.path().join("demo-pack");
        fs::create_dir_all(pack.join(".git/objects")).unwrap();
        fs::write(pack.join("pack.ron"), "(name: \"x\")").unwrap();
        fs::File::create(pack.join(".git/objects/big.pack"))
            .unwrap()
            .set_len(unmei_engine::PACK_FILE_LIMIT + 1)
            .unwrap();

        let sandbox = Sandbox::create_in(&source.path().join("root"), &pack, 1).unwrap();
        assert!(sandbox.pack_root().join("pack.ron").exists());
        assert!(!sandbox.pack_root().join(".git").exists());
    }

    #[test]
    fn a_pack_file_over_the_limit_fails_the_sandbox() {
        let source = tempfile::tempdir().unwrap();
        let pack = source.path().join("demo-pack");
        fs::create_dir_all(pack.join("scenes")).unwrap();
        fs::File::create(pack.join("scenes/a.unmei"))
            .unwrap()
            .set_len(unmei_engine::PACK_FILE_LIMIT + 1)
            .unwrap();

        let error = Sandbox::create_in(&source.path().join("root"), &pack, 1).unwrap_err();
        assert!(error.to_string().contains("limit"), "{error}");
        assert!(!source
            .path()
            .join("root/take-1/demo-pack/scenes/a.unmei")
            .exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_in_the_pack_fails_the_sandbox() {
        let source = tempfile::tempdir().unwrap();
        let outside = source.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret"), "host file").unwrap();
        for link in ["notes.txt", "linked-folder"] {
            let pack = source.path().join(format!("pack-{link}"));
            fs::create_dir_all(&pack).unwrap();
            let target = if link == "notes.txt" {
                outside.join("secret")
            } else {
                outside.clone()
            };
            std::os::unix::fs::symlink(target, pack.join(link)).unwrap();

            let root = source.path().join(format!("root-{link}"));
            assert!(Sandbox::create_in(&root, &pack, 1).is_err(), "{link}");
            assert!(!root.join(format!("take-1/pack-{link}/{link}")).exists());
        }
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
