//! The Keiko flags on `unmei-play`: the mutually exclusive testing modes, their
//! modifiers and the case-id shape (spec §1).
//!
//! The grammar lives beside the player's launch grammar and shares its token
//! loop; this module owns the Keiko half — what each flag may combine with, and
//! what a complete Keiko request means. The preview is one of the group's modes
//! and carries its own half in [`PreviewArgs`].

use std::ffi::OsString;
use std::path::Path;

use super::preview::PreviewArgs;
use super::UsageError;

/// The modes one invocation may choose: four Keiko testing modes and the
/// preview. One per invocation, and each excludes the others.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeikoMode {
    /// Run every named case, or every case under `tests/` when none is named.
    Take { ids: Vec<String>, live: bool },
    /// Replay a case, apply one step, and append it only if it passed.
    Step { id: String, step: String },
    /// Replay a case and print the observation without writing.
    Observe { id: String },
    /// Walk the pack's choices and landing keys within a step budget.
    Explore { budget: u32, from: Option<String> },
    /// Draw a line or a scene into an output folder (spec §9).
    Preview(PreviewArgs),
}

impl KeikoMode {
    /// The rules that need the canonical pack root rather than the token list:
    /// a preview never writes into the pack it reads (spec §1). The grammar
    /// touches no filesystem, so this step runs where the root is resolved.
    pub fn check_paths(&self, pack_root: &Path) -> Result<(), UsageError> {
        match self {
            KeikoMode::Preview(args) => args.refuse_inside_pack(pack_root),
            _ => Ok(()),
        }
    }
}

/// The output format every mode shares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    #[default]
    Text,
    Json,
}

/// What a bare token means while a mode is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bare {
    /// A take id.
    TakeId,
    /// A `--step` or `--observe` operand.
    Value,
    /// Not an operand, so the token is `PACK_DIR` or an error.
    None,
}

/// Which mode flag was seen, before its operands are complete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Take,
    Step,
    Observe,
    Explore,
    Preview,
}

/// The Keiko half of one launch, accumulated token by token.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct KeikoArgs {
    kind: Option<Kind>,
    take_ids: Vec<String>,
    step_args: Vec<String>,
    observe_arg: Option<String>,
    budget: Option<u32>,
    from: Option<String>,
    live: bool,
    keep: bool,
    format: Format,
    preview: PreviewArgs,
}

impl KeikoArgs {
    /// What a bare token becomes while this mode is open.
    pub fn bare_target(&self) -> Bare {
        match self.kind {
            Some(Kind::Take) => Bare::TakeId,
            Some(Kind::Step) if self.step_args.len() < 2 => Bare::Value,
            Some(Kind::Observe) if self.observe_arg.is_none() => Bare::Value,
            _ => Bare::None,
        }
    }

    /// Take a bare token as the current mode's operand.
    pub fn push_value(&mut self, value: OsString) -> Result<(), UsageError> {
        let value = value
            .into_string()
            .map_err(|_| UsageError::new("a Keiko operand must be valid Unicode"))?;
        match self.kind {
            Some(Kind::Step) => {
                self.step_args.push(value);
                Ok(())
            }
            Some(Kind::Observe) => {
                self.observe_arg = Some(value);
                Ok(())
            }
            Some(Kind::Take) => {
                validate_id(&value)?;
                self.take_ids.push(value);
                Ok(())
            }
            Some(Kind::Explore) => Err(UsageError::new("`--explore` takes no ids")),
            Some(Kind::Preview) => Err(UsageError::new(
                "`--preview` takes its folder after the flag",
            )),
            None => Err(UsageError::new(format!("unexpected argument '{value}'"))),
        }
    }

    fn set_kind(&mut self, kind: Kind) -> Result<(), UsageError> {
        if self.kind.is_some() {
            return Err(UsageError::new(
                "choose one mode: --take, --step, --observe, --explore or --preview",
            ));
        }
        self.kind = Some(kind);
        Ok(())
    }

    /// `--take` and its bare ids.
    pub fn take(&mut self) -> Result<(), UsageError> {
        self.set_kind(Kind::Take)
    }

    pub fn step(&mut self) -> Result<(), UsageError> {
        self.set_kind(Kind::Step)
    }

    pub fn observe(&mut self) -> Result<(), UsageError> {
        self.set_kind(Kind::Observe)
    }

    pub fn explore(&mut self) -> Result<(), UsageError> {
        self.set_kind(Kind::Explore)
    }

    /// `--preview DIR`.
    pub fn preview(&mut self, dir: std::path::PathBuf) -> Result<(), UsageError> {
        self.set_kind(Kind::Preview)?;
        self.preview.preview(dir);
        Ok(())
    }

    /// `--scene ID`.
    pub fn set_scene(&mut self, id: &str) -> Result<(), UsageError> {
        self.preview.set_scene(id)
    }

    /// `--at SCENE:LINE`.
    pub fn set_at(&mut self, value: &str) -> Result<(), UsageError> {
        self.preview.set_at(value)
    }

    /// `--state FILE`.
    pub fn set_state(&mut self, path: std::path::PathBuf) {
        self.preview.set_state(path);
    }

    /// `--pick N,N,...`.
    pub fn set_picks(&mut self, value: &str) -> Result<(), UsageError> {
        self.preview.set_picks(value)
    }

    pub fn live(&mut self) -> Result<(), UsageError> {
        self.live = true;
        Ok(())
    }

    pub fn keep(&mut self) {
        self.keep = true;
    }

    /// The keep flag, for the finished launch.
    pub fn keeping(&self) -> bool {
        self.keep
    }

    /// The output format, for the finished launch.
    pub fn output_format(&self) -> Format {
        self.format
    }

    pub fn set_format(&mut self, value: &str) -> Result<(), UsageError> {
        self.format = match value {
            "text" => Format::Text,
            "json" => Format::Json,
            other => {
                return Err(UsageError::new(format!(
                    "--format takes `text` or `json`, not '{other}'"
                )))
            }
        };
        Ok(())
    }

    pub fn set_budget(&mut self, value: &str) -> Result<(), UsageError> {
        self.budget = Some(
            value
                .parse::<u32>()
                .map_err(|_| UsageError::new("--budget takes a whole number of steps"))?,
        );
        Ok(())
    }

    pub fn set_from(&mut self, value: &str) -> Result<(), UsageError> {
        validate_id(value)?;
        self.from = Some(value.to_string());
        Ok(())
    }

    /// Fold the accumulated flags into one request, or explain why they cannot
    /// be combined. `check` says whether `--check` was also requested, where a
    /// mode-only modifier is a usage error and `--format` alone is valid.
    pub fn finish(self, dev: bool, check: bool) -> Result<Option<KeikoMode>, UsageError> {
        // The preview's own flags belong to its mode alone, whether or not some
        // other mode was named (spec §1).
        if self.kind != Some(Kind::Preview) {
            for (given, flag) in [
                (self.preview.scene.is_some(), "--scene"),
                (self.preview.at.is_some(), "--at"),
                (self.preview.state.is_some(), "--state"),
                (!self.preview.picks.is_empty(), "--pick"),
            ] {
                if given {
                    return Err(UsageError::new(format!(
                        "{flag} is valid only with --preview"
                    )));
                }
            }
        }
        let Some(kind) = self.kind else {
            if check {
                for (given, flag) in [
                    (self.live, "--live"),
                    (self.keep, "--keep"),
                    (self.budget.is_some(), "--budget"),
                    (self.from.is_some(), "--from"),
                ] {
                    if given {
                        return Err(UsageError::new(format!(
                            "{flag} is valid only with a Keiko mode"
                        )));
                    }
                }
                return Ok(None);
            }
            for (given, flag) in [
                (self.live, "--live"),
                (self.keep, "--keep"),
                (self.format != Format::Text, "--format"),
                (self.budget.is_some(), "--budget"),
                (self.from.is_some(), "--from"),
            ] {
                if given {
                    return Err(UsageError::new(format!(
                        "{flag} needs a Keiko mode (--take, --step, --observe or --explore)"
                    )));
                }
            }
            return Ok(None);
        };
        if dev {
            return Err(UsageError::new(if kind == Kind::Preview {
                "--dev cannot be combined with --preview"
            } else {
                "--dev cannot be combined with a Keiko mode; a case sets dev mode itself"
            }));
        }
        if check && kind == Kind::Preview {
            return Err(UsageError::new(
                "--check cannot be combined with --preview; choose one mode",
            ));
        }
        if self.live && kind != Kind::Take {
            return Err(UsageError::new("--live is valid only with --take"));
        }
        if kind != Kind::Explore && (self.budget.is_some() || self.from.is_some()) {
            return Err(UsageError::new(
                "--budget and --from are valid only with --explore",
            ));
        }
        let mode = match kind {
            Kind::Take => KeikoMode::Take {
                ids: self.take_ids,
                live: self.live,
            },
            Kind::Step => {
                let [id, step] = <[String; 2]>::try_from(self.step_args)
                    .map_err(|_| UsageError::new("--step needs an ID and one STEP"))?;
                validate_id(&id)?;
                KeikoMode::Step { id, step }
            }
            Kind::Observe => {
                let id = self
                    .observe_arg
                    .ok_or_else(|| UsageError::new("--observe needs an ID"))?;
                validate_id(&id)?;
                KeikoMode::Observe { id }
            }
            Kind::Explore => KeikoMode::Explore {
                budget: self.budget.unwrap_or(DEFAULT_BUDGET),
                from: self.from,
            },
            Kind::Preview => KeikoMode::Preview(
                self.preview
                    .finish()?
                    .ok_or_else(|| UsageError::new("--preview needs a folder"))?,
            ),
        };
        Ok(Some(mode))
    }
}

/// The exploration step budget when `--budget` is not given.
pub const DEFAULT_BUDGET: u32 = 10_000;

/// A case id: `tests/<id>.keiko`'s stem, or `findings/<stem>` for a saved
/// finding (spec §1).
pub fn validate_id(id: &str) -> Result<(), UsageError> {
    // A saved finding's stem is `<rule>-<hash>`, and a rule id carries dots
    // (`keiko.panic`), so the findings folder accepts a dot in the stem.
    let (stem, allow_dots) = match id.strip_prefix("findings/") {
        Some(stem) => (stem, true),
        None => (id, false),
    };
    let mut chars = stem.chars();
    let first_ok = chars
        .next()
        .is_some_and(|first| first.is_ascii_lowercase() || first.is_ascii_digit());
    let rest_ok = chars.all(|ch| {
        ch.is_ascii_lowercase()
            || ch.is_ascii_digit()
            || ch == '_'
            || ch == '-'
            || (allow_dots && ch == '.')
    });
    if first_ok && rest_ok {
        Ok(())
    } else {
        Err(UsageError::new(format!(
            "'{id}' is not a case id ([a-z0-9][a-z0-9_-]*, or findings/<stem>)"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_case_id_is_a_lowercase_stem_or_a_saved_finding() {
        for id in [
            "a",
            "new-game",
            "case_1",
            "9lives",
            "findings/keiko.panic-ab12cd34",
        ] {
            assert!(validate_id(id).is_ok(), "{id}");
        }
        for id in [
            "",
            "A",
            "-x",
            "_x",
            "findings/",
            "findings/A",
            "a/b",
            "a.b",
            "a b",
        ] {
            assert!(validate_id(id).is_err(), "{id}");
        }
    }

    #[test]
    fn modes_are_mutually_exclusive() {
        let mut args = KeikoArgs::default();
        args.take().unwrap();
        assert!(args.explore().is_err());
        assert_eq!(args.bare_target(), Bare::TakeId);
    }

    #[test]
    fn finish_applies_every_combination_rule() {
        let mut take = KeikoArgs::default();
        take.take().unwrap();
        take.push_value(OsString::from("a")).unwrap();
        assert_eq!(
            take.finish(false, false).unwrap(),
            Some(KeikoMode::Take {
                ids: vec!["a".into()],
                live: false
            })
        );

        let mut live = KeikoArgs::default();
        live.observe().unwrap();
        live.live().unwrap();
        assert!(live.finish(false, false).is_err(), "--live needs --take");

        let mut budget = KeikoArgs::default();
        budget.take().unwrap();
        budget.set_budget("5").unwrap();
        assert!(
            budget.finish(false, false).is_err(),
            "--budget needs --explore"
        );

        let mut dev = KeikoArgs::default();
        dev.take().unwrap();
        assert!(
            dev.finish(true, false).is_err(),
            "--dev conflicts with a mode"
        );

        let mut step = KeikoArgs::default();
        step.step().unwrap();
        step.push_value(OsString::from("a")).unwrap();
        assert_eq!(step.bare_target(), Bare::Value);
        step.push_value(OsString::from("advance 2")).unwrap();
        assert_eq!(
            step.finish(false, false).unwrap(),
            Some(KeikoMode::Step {
                id: "a".into(),
                step: "advance 2".into()
            })
        );

        let mut explore = KeikoArgs::default();
        explore.explore().unwrap();
        assert_eq!(
            explore.finish(false, false).unwrap(),
            Some(KeikoMode::Explore {
                budget: DEFAULT_BUDGET,
                from: None
            })
        );
    }

    #[test]
    fn a_modifier_without_a_mode_is_a_usage_error() {
        let mut keep = KeikoArgs::default();
        keep.keep();
        assert!(keep.finish(false, false).is_err());

        let mut format_with_check = KeikoArgs::default();
        format_with_check.set_format("json").unwrap();
        assert_eq!(format_with_check.finish(false, true).unwrap(), None);

        let mut keep_with_check = KeikoArgs::default();
        keep_with_check.keep();
        assert!(
            keep_with_check.finish(false, true).is_err(),
            "--keep needs a Keiko mode, even beside --check"
        );
    }
}
