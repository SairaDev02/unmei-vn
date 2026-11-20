//! The `--preview` mode on `unmei-play` (spec §1, §9).
//!
//! Preview is a mode of its own: it draws a line or a whole scene into an
//! output folder, joins the exclusive group the check and the Keiko modes form,
//! refuses `--dev`, and shares only `--format` with them. `--scene`, `--at`,
//! `--state` and `--pick` belong to it alone.
//!
//! The grammar never touches the filesystem, so the one rule that needs it —
//! the output folder must not lie inside the pack — is a separate step that
//! runs where the pack root is canonical ([`PreviewArgs::refuse_inside_pack`]).

use std::path::{Path, PathBuf};

use super::UsageError;

/// The preview half of one launch, accumulated token by token.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreviewArgs {
    /// The output folder, `--preview DIR`. Empty when the mode was not asked
    /// for; a preview never writes into an empty-named folder.
    pub dir: PathBuf,
    /// `--scene ID`: one frame at every park the scene meets.
    pub scene: Option<String>,
    /// `--at SCENE:LINE`: one frame at the park that line produces.
    pub at: Option<(String, u32)>,
    /// `--state FILE`: a RON map in `state.ron`'s form.
    pub state: Option<PathBuf>,
    /// `--pick N,N,...`: the 1-based option to take at each successive choice.
    pub picks: Vec<usize>,
}

impl PreviewArgs {
    /// `--preview DIR`.
    pub fn preview(&mut self, dir: PathBuf) {
        self.dir = dir;
    }

    /// `--scene ID`. A scene id is a filename stem, and the generated case
    /// writes it as one bare token, so whitespace cannot be expressed.
    pub fn set_scene(&mut self, id: &str) -> Result<(), UsageError> {
        self.scene = Some(scene_id(id)?);
        Ok(())
    }

    /// `--at SCENE:LINE`.
    pub fn set_at(&mut self, value: &str) -> Result<(), UsageError> {
        let (scene, line) = value
            .rsplit_once(':')
            .ok_or_else(|| UsageError::new("--at takes SCENE:LINE"))?;
        let line = line
            .parse::<u32>()
            .map_err(|_| UsageError::new("--at takes SCENE:LINE"))?;
        self.at = Some((scene_id(scene)?, line));
        Ok(())
    }

    /// `--state FILE`.
    pub fn set_state(&mut self, path: PathBuf) {
        self.state = Some(path);
    }

    /// `--pick N,N,...`: one 1-based option number per choice.
    pub fn set_picks(&mut self, value: &str) -> Result<(), UsageError> {
        let mut picks = Vec::new();
        for piece in value.split(',') {
            let number = piece
                .parse::<usize>()
                .ok()
                .filter(|number| *number > 0)
                .ok_or_else(|| {
                    UsageError::new("--pick takes a comma-separated list of option numbers")
                })?;
            picks.push(number);
        }
        self.picks = picks;
        Ok(())
    }

    /// Whether `--preview` was given at all.
    pub fn requested(&self) -> bool {
        !self.dir.as_os_str().is_empty()
    }

    /// The finished request, or `None` when `--preview` was not given. The two
    /// selectors exclude each other (spec §1).
    pub fn finish(&self) -> Result<Option<Self>, UsageError> {
        if !self.requested() {
            return Ok(None);
        }
        if self.scene.is_some() && self.at.is_some() {
            return Err(UsageError::new(
                "--scene and --at exclude each other; choose one selector",
            ));
        }
        Ok(Some(self.clone()))
    }

    /// Refuse an output folder inside the pack, so a preview never writes into
    /// the pack it reads (spec §1). `pack_root` is already canonical, so only
    /// the folder is resolved: an existing path is canonicalized, and a missing
    /// one through its nearest existing ancestor, so this creates nothing.
    pub fn refuse_inside_pack(&self, pack_root: &Path) -> Result<(), UsageError> {
        if resolve(&self.dir).starts_with(pack_root) {
            return Err(UsageError::new(format!(
                "--preview {} is inside the pack folder {}; a preview never writes into the pack it reads",
                self.dir.display(),
                pack_root.display()
            )));
        }
        Ok(())
    }
}

/// A scene id as the generated case carries it: non-empty and free of the
/// whitespace that would split one case token into two.
fn scene_id(id: &str) -> Result<String, UsageError> {
    if id.is_empty() || id.chars().any(char::is_whitespace) {
        return Err(UsageError::new(format!(
            "'{id}' cannot be a scene id: the preview writes it as one case token"
        )));
    }
    Ok(id.to_string())
}

/// A folder's path with every existing prefix canonicalized, so a folder that
/// does not exist yet still compares against a canonical pack root. A relative
/// path resolves against the working directory, and `..` and links resolve the
/// way the platform resolves them.
fn resolve(path: &Path) -> PathBuf {
    let mut missing: Vec<std::ffi::OsString> = Vec::new();
    let mut current = path.to_path_buf();
    loop {
        if let Ok(mut base) = current.canonicalize() {
            while let Some(component) = missing.pop() {
                base.push(component);
            }
            return base;
        }
        match (current.parent(), current.file_name()) {
            (Some(parent), Some(name)) => {
                missing.push(name.to_os_string());
                current = if parent.as_os_str().is_empty() {
                    PathBuf::from(".")
                } else {
                    parent.to_path_buf()
                };
            }
            // Nothing of the path exists, or it has no parent to resolve.
            _ => return path.to_path_buf(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_selectors_are_parsed_and_exclude_each_other() {
        let mut args = PreviewArgs::default();
        args.preview(PathBuf::from("out"));
        args.set_at("first_meeting:19").expect("a selector");
        assert_eq!(args.at, Some(("first_meeting".to_string(), 19)));
        assert!(args.set_at("first_meeting").is_err());
        assert!(args.set_at("first_meeting:sideways").is_err());
        assert!(args.set_scene("two words").is_err());

        let mut both = PreviewArgs::default();
        both.preview(PathBuf::from("out"));
        both.set_scene("a").expect("a scene");
        both.set_at("a:1").expect("a line");
        assert!(both.finish().is_err(), "the selectors exclude each other");

        assert!(PreviewArgs::default()
            .finish()
            .expect("no preview")
            .is_none());
    }

    #[test]
    fn picks_are_one_based_numbers() {
        let mut args = PreviewArgs::default();
        args.set_picks("2,1").expect("a pick list");
        assert_eq!(args.picks, [2, 1]);
        for bad in ["0", "2,0", "2,", "", "two"] {
            assert!(args.set_picks(bad).is_err(), "{bad:?}");
        }
    }

    /// A folder inside the pack is refused; one beside it is not, and neither
    /// check creates the folder.
    #[test]
    fn the_output_folder_may_not_lie_inside_the_pack() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let pack = dir.path().join("pack");
        std::fs::create_dir_all(&pack).expect("the pack folder");
        let pack = pack.canonicalize().expect("a canonical pack");

        let mut args = PreviewArgs::default();
        args.preview(pack.join("frames"));
        let error = args.refuse_inside_pack(&pack).expect_err("inside the pack");
        assert!(error.to_string().contains("inside the pack"), "{error}");
        assert!(!pack.join("frames").exists(), "the refusal creates nothing");

        args.preview(pack.join("..").join("out"));
        args.refuse_inside_pack(&pack).expect("beside the pack");

        args.preview(dir.path().join("out"));
        args.refuse_inside_pack(&pack).expect("outside the pack");
    }
}
