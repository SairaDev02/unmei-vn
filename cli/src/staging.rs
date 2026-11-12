//! The one private owner of the temporary candidate-pack lifecycle
//! (author-cli contract §9).
//!
//! [`staged_pack`] owns the `TempDir`, the transplant, the candidate placement,
//! the reload and its release; [`validation`] owns the two crate-visible
//! validators and their failure vocabulary. Each module states the rules it
//! owns.

mod staged_pack;
mod validation;

pub(crate) use validation::{validate_npc_decision, validate_scene, NpcValidationFailure};
