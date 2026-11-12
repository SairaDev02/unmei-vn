//! The default-pack rule: `demo-pack` in the folder that holds the executable.
//!
//! Split from the grammar so [`super::parse`] stays free of OS calls: the one OS
//! fact the rule needs — the executable's own folder — is taken as a parameter
//! here, which is also what makes the rule testable ([`resolve_pack`]).

use std::path::{Path, PathBuf};

/// The pack played when no `PACK_DIR` is given, in the folder that holds the
/// executable — in every build, never the working directory. The one spelling
/// resolution, the help text and the error-screen hint share.
pub const DEFAULT_PACK_NAME: &str = "demo-pack";

/// A pack to play, and how its path was chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPack {
    /// The pack directory, not yet canonicalized. Canonicalization is a separate
    /// best-effort step so resolution stays free of filesystem calls.
    pub pack_root: PathBuf,
    /// Which rule produced [`ResolvedPack::pack_root`].
    pub origin: PackOrigin,
}

impl ResolvedPack {
    /// The pack path with best-effort canonicalization; the original is
    /// retained when canonicalization fails (a missing or racing path is still
    /// the app's problem to report at load, not a usage error).
    pub fn canonical_pack_root(&self) -> PathBuf {
        self.pack_root
            .canonicalize()
            .unwrap_or_else(|_| self.pack_root.clone())
    }
}

/// Which rule chose the pack: the positional argument, or the default beside the
/// executable. The error screen's hint line shows only for
/// [`PackOrigin::Defaulted`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackOrigin {
    /// The positional `PACK_DIR` was given.
    Explicit,
    /// The positional was absent, so [`DEFAULT_PACK_NAME`] beside the executable
    /// was used.
    Defaulted,
}

/// No `PACK_DIR` was given and the executable's folder is unknown, so the
/// default pack cannot be derived. Reported as the no-executable error screen
/// with no session log, which has no pack to sit beside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutableUnknown;

/// The folder that holds this executable — the one OS fact the default-pack rule
/// needs. It uses each platform's own `current_exe` semantics with no extra
/// canonicalization, and there is no working-directory fallback: `None` means
/// the no-executable outcome, not a guess.
pub fn executable_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
}

/// Apply the default-pack rule to a parsed launch.
///
/// An explicit `PACK_DIR` is kept exactly as given and never consults the
/// executable. An absent one becomes `<exe_dir>/demo-pack`; when the executable's
/// folder is unknown ([`ExecutableUnknown`]) there is nothing to play.
pub fn resolve_pack(
    pack_dir: Option<PathBuf>,
    exe_dir: Option<PathBuf>,
) -> Result<ResolvedPack, ExecutableUnknown> {
    match pack_dir {
        Some(pack_root) => Ok(ResolvedPack {
            pack_root,
            origin: PackOrigin::Explicit,
        }),
        None => exe_dir
            .map(|exe_dir| ResolvedPack {
                pack_root: exe_dir.join(DEFAULT_PACK_NAME),
                origin: PackOrigin::Defaulted,
            })
            .ok_or(ExecutableUnknown),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_pack_is_demo_pack_in_the_injected_executable_folder() {
        let resolved = resolve_pack(None, Some(PathBuf::from("/games/unmei-play-0.1.0"))).unwrap();
        assert_eq!(
            resolved.pack_root,
            PathBuf::from("/games/unmei-play-0.1.0/demo-pack")
        );
        assert_eq!(resolved.origin, PackOrigin::Defaulted);
    }
    #[test]
    fn an_explicit_pack_dir_is_unchanged_and_ignores_the_executable() {
        let with_exe = resolve_pack(
            Some(PathBuf::from("my-pack")),
            Some(PathBuf::from("/games")),
        )
        .expect("an explicit pack needs no executable");
        assert_eq!(with_exe.pack_root, PathBuf::from("my-pack"));
        assert_eq!(with_exe.origin, PackOrigin::Explicit);

        // The executable location is only needed for the default rule, so an
        // explicit PACK_DIR still resolves when it is unknown.
        let without_exe = resolve_pack(Some(PathBuf::from("my-pack")), None)
            .expect("an explicit pack needs no executable");
        assert_eq!(without_exe, with_exe);
    }
    #[test]
    fn an_unknown_executable_folder_is_the_no_executable_outcome() {
        assert_eq!(resolve_pack(None, None), Err(ExecutableUnknown));
    }
    #[test]
    fn the_pack_root_canonicalizes_but_a_missing_path_is_kept() {
        let existing = ResolvedPack {
            pack_root: PathBuf::from("."),
            origin: PackOrigin::Explicit,
        };
        assert_eq!(
            existing.canonical_pack_root(),
            std::fs::canonicalize(".").unwrap()
        );

        let missing = ResolvedPack {
            pack_root: PathBuf::from("definitely/not/a/real/pack"),
            origin: PackOrigin::Defaulted,
        };
        assert_eq!(
            missing.canonical_pack_root(),
            PathBuf::from("definitely/not/a/real/pack")
        );
    }
}
