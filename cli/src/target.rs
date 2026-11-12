//! Opaque checked authoring targets and the checked Pack-key inventory
//! (author-cli contract §10).
//!
//! [`filename`] owns the lexical policy and its terminal refusal, [`checked`]
//! the target types and the checked inventory; each module states the rules it
//! owns.
//!
//! Confinement here is lexical direct-child construction in an
//! author-controlled directory; it is not a symlink/reparse-point, race,
//! multi-user or crash-safety sandbox.

mod checked;
mod filename;

pub use checked::{CheckedPack, NpcTarget, SceneTarget};
pub use filename::{FileKind, FilenameFault, TargetError};
