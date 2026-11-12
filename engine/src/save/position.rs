//! The one semantic validity owner for a decodable save: the additive version
//! floor and the position checks a saved value must pass before it may load
//! (`docs/runtime-hardening.md` § Save slots §3).

use crate::content::{Line, Pack};
use crate::runner::{RunnerError, Wait};

use super::{SaveError, SaveFile, SAVE_VERSION};

/// The additive version floor (ADR 0006): accept `1..=SAVE_VERSION`, reject `0`
/// and anything newer than this build.
pub(super) fn check_version(save: &SaveFile) -> Result<(), SaveError> {
    if save.version == 0 || save.version > SAVE_VERSION {
        return Err(SaveError::Version {
            found: save.version,
            expected: SAVE_VERSION,
        });
    }
    Ok(())
}

/// The semantic checks a decodable save must pass before it may load
/// (`docs/runtime-hardening.md` § Save slots §3): the version is accepted, the
/// scene exists, `pc` is inside it, `pc == 0` carries no wait, every state key
/// is declared, and the parked wait matches the kind of the line at `pc - 1`.
///
/// The single owner of that rule: [`read_slot`](super::slots::read_slot) calls it while classifying and
/// [`SaveFile::restore`] calls it against the destination runner's actual pack
/// before mutation. Acceptance is only ever for the checked value against the
/// pack it was checked against; it carries no authority over a later edit or a
/// different destination.
pub(super) fn validate_position(pack: &Pack, save: &SaveFile) -> Result<(), SaveError> {
    check_version(save)?;
    let Some(scene) = pack.scenes.get(&save.scene_id) else {
        return Err(RunnerError::UnknownScene {
            scene: save.scene_id.clone(),
        }
        .into());
    };
    if save.pc > scene.lines.len() {
        return Err(semantic_error(format!(
            "position {} is past the end of scene `{}` ({} lines)",
            save.pc,
            save.scene_id,
            scene.lines.len()
        )));
    }
    for key in save.state.keys() {
        if !pack.state.contains_key(key) {
            return Err(semantic_error(format!(
                "state key `{key}` is not declared by `state.ron`"
            )));
        }
    }
    if let Some(wait) = &save.wait {
        if save.pc == 0 {
            return Err(semantic_error("a wait is parked at position 0".to_string()));
        }
        let matches = scene
            .lines
            .get(save.pc - 1)
            .is_some_and(|line| wait_matches_line(wait, line));
        if !matches {
            return Err(semantic_error(format!(
                "the parked wait does not match line {}",
                save.pc - 1
            )));
        }
    }
    Ok(())
}

/// A semantic failure has no dedicated [`SaveError`] variant; it rides
/// [`SaveError::Decode`] so the error type stays unchanged.
fn semantic_error(reason: String) -> SaveError {
    SaveError::Decode(reason)
}

fn wait_matches_line(wait: &Wait, line: &Line) -> bool {
    matches!(
        (wait, line),
        (Wait::Say { .. }, Line::Say { .. })
            | (Wait::Choice { .. }, Line::Choice(_))
            | (Wait::Decision { .. }, Line::Decide { .. })
    )
}
