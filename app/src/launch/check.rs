//! The `--check` mode on `unmei-play` (spec §1).
//!
//! Check is a mode of its own: it takes no operand, joins the exclusive group
//! the Keiko modes form, refuses `--dev`, and shares only `--format` with them.

use super::UsageError;

/// The check half of one launch: whether `--check` was asked for.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CheckArgs {
    requested: bool,
}

impl CheckArgs {
    pub fn request(&mut self) {
        self.requested = true;
    }

    /// Whether `--check` was asked for.
    pub fn requested(&self) -> bool {
        self.requested
    }

    /// Fold the flag into the launch. `--check` is one mode per invocation, so
    /// another mode or `--dev` alongside it is a usage error. The Keiko-only
    /// modifiers are rejected by [`super::keiko::KeikoArgs::finish`].
    pub fn finish(&self, dev: bool, keiko_mode: bool) -> Result<bool, UsageError> {
        if !self.requested {
            return Ok(false);
        }
        if dev {
            return Err(UsageError::new("--dev cannot be combined with --check"));
        }
        if keiko_mode {
            return Err(UsageError::new(
                "--check cannot be combined with a Keiko mode; choose one mode",
            ));
        }
        Ok(true)
    }
}
