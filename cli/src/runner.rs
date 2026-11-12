//! The real command runner both shipping commands share: resolved inputs,
//! `AuthorChat`, borrowed terminal I/O and the actual session/driver/attempt
//! composition plus the existing atomic writer. It returns a typed outcome
//! instead of exiting, so main keeps only parsing, resolution, runtime and
//! exit wiring (author-cli contract §5, drafting contract §10).

use std::collections::HashSet;
use std::path::Path;

use unmei_engine::content::Npc;
use unmei_engine::{Pack, QuestionType};

use crate::draft;
use crate::gen::{self, GenerateInput, GenerationSession};
use crate::provider::{AuthorChat, ProviderError};
use crate::splice::{apply_candidate, write_atomic, CreateMode};
use crate::target::{CheckedPack, NpcTarget, SceneTarget, TargetError};
pub use crate::terminal::Terminal;
use crate::terminal::{
    describe_criteria, discarded, draft_preview, eof_discard, input, print_exhausted, read_line,
    read_paragraph, report_draft, warn_if_unsteered,
};

/// What the runner did. `Discarded` still printed its existing
/// acknowledgement; both variants leave main to exit 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerOutcome {
    Completed,
    Discarded,
}

/// Why the runner stopped. Every variant's `Display` is the exact text main
/// prints after its `✖ ` prefix.
#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    #[error(transparent)]
    Target(#[from] TargetError),
    #[error("generation failed: {0}")]
    Npc(#[from] gen::GenerationError),
    #[error("provider error: {0}")]
    DraftProvider(ProviderError),
    #[error(
        "all {limit} attempts failed validation — nothing was written",
        limit = gen::RETRY_LIMIT
    )]
    DraftExhausted,
    #[error("{0}")]
    Message(String),
    #[error("splice failed: {0}")]
    Splice(#[from] crate::splice::SpliceError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Everything `unmei author npc` needs once its flags are parsed and the pack
/// is loaded.
pub struct NpcCommand<'a, C: AuthorChat> {
    pub pack: &'a Pack,
    pub pack_root: &'a Path,
    pub npc_id: &'a str,
    pub question: &'a str,
    pub question_type: QuestionType,
    pub create: bool,
    pub brief: Option<&'a str>,
    pub interactive: bool,
    pub dry_run: bool,
    pub client: &'a C,
    pub model: &'a str,
}

/// Everything `unmei author draft` needs once its flags are parsed, the pack is
/// loaded and the run is resolved.
pub struct DraftCommand<'a, C: AuthorChat> {
    pub pack: &'a Pack,
    pub pack_root: &'a Path,
    pub scene_id: &'a str,
    pub run: &'a draft::Run,
    pub scene_text: &'a str,
    pub scene_path: &'a Path,
    pub allowed_speakers: &'a HashSet<String>,
    pub brief: Option<&'a str>,
    pub interactive: bool,
    pub dry_run: bool,
    pub client: &'a C,
    pub model: &'a str,
}

/// `unmei author npc` (author-cli contract §3/§5/§6).
pub async fn run_npc<C: AuthorChat>(
    command: NpcCommand<'_, C>,
    terminal: &mut Terminal<'_>,
) -> Result<RunnerOutcome, RunnerError> {
    // Terminal target preflight, before any warning, prompt, session, attempt
    // or provider work (contract §10). The check repeats the generation core's
    // own preflight; either boundary may be called on its own.
    NpcTarget::new(command.npc_id)?;
    CheckedPack::new(command.pack)?;

    warn_if_unsteered(
        command.pack_root,
        command.brief,
        "generation proceeds from pack context alone; add a brief for intent",
        terminal,
    )?;

    if command.interactive {
        npc_interactive(&command, terminal).await
    } else {
        npc_one_shot(&command, terminal).await
    }
}

/// One-shot NPC generation: generate → preview/write, retries inside the
/// driver; exhaustion prints the last raw reply labelled `best` plus every
/// error, writes nothing and returns a nonzero-status error.
async fn npc_one_shot<C: AuthorChat>(
    command: &NpcCommand<'_, C>,
    terminal: &mut Terminal<'_>,
) -> Result<RunnerOutcome, RunnerError> {
    let input = GenerateInput {
        pack: command.pack,
        npc_id: command.npc_id,
        question: command.question,
        question_type: command.question_type,
        brief: command.brief,
        client: command.client,
        model: command.model,
    };
    let mut session = GenerationSession::new();
    let payload = match gen::generate_with_session(&input, &mut session).await {
        Ok(outcome) => outcome.payload,
        Err(gen::GenerationError::AttemptsExhausted {
            raw_outputs,
            errors,
        }) => {
            print_exhausted(&raw_outputs, &errors, terminal)?;
            return Err(RunnerError::Npc(gen::GenerationError::AttemptsExhausted {
                raw_outputs,
                errors,
            }));
        }
        Err(gen::GenerationError::Target(e)) => return Err(RunnerError::Target(e)),
        Err(e @ (gen::GenerationError::Provider(_) | gen::GenerationError::Preparation(_))) => {
            return Err(RunnerError::Npc(e))
        }
    };

    finish_npc(command, payload, terminal)
}

/// The guided NPC `-i` session: brief → generate → preview → feedback /
/// accept / discard — one config per session, over the same complete attempt
/// and driver as one-shot. Exhaustion takes feedback rather than failing the
/// process; `accept` is refused until a validated candidate exists.
async fn npc_interactive<C: AuthorChat>(
    command: &NpcCommand<'_, C>,
    terminal: &mut Terminal<'_>,
) -> Result<RunnerOutcome, RunnerError> {
    let brief = match command.brief {
        Some(brief) => Some(brief.to_string()),
        None => {
            writeln!(
                terminal.out,
                "Brief for decisions[\"{}\"] (end with an empty line):",
                command.question
            )?;
            terminal.out.flush()?;
            Some(read_paragraph(terminal)?)
        }
    };

    let mut session = GenerationSession::new();

    loop {
        let input = GenerateInput {
            pack: command.pack,
            npc_id: command.npc_id,
            question: command.question,
            question_type: command.question_type,
            brief: brief.as_deref(),
            client: command.client,
            model: command.model,
        };

        match gen::generate_with_session(&input, &mut session).await {
            Ok(outcome) => {
                let rendered = ron::ser::to_string_pretty(
                    &outcome.payload.config,
                    ron::ser::PrettyConfig::default(),
                )
                .map_err(|e| RunnerError::Message(e.to_string()))?;
                writeln!(
                    terminal.out,
                    "--- preview (decisions[\"{}\"], not yet written) ---",
                    command.question
                )?;
                writeln!(terminal.out, "{rendered}")?;
                writeln!(terminal.out, "--- end preview ---")?;
                writeln!(
                    terminal.out,
                    "Type 'accept' to write, 'discard' to write nothing, or free-text feedback to regenerate:"
                )?;
                terminal.out.flush()?;

                let Some(line) = read_line(terminal)? else {
                    return eof_discard(terminal);
                };
                let trimmed = line.trim();
                if trimmed.eq_ignore_ascii_case("accept") {
                    return finish_npc(command, outcome.payload, terminal);
                }
                if trimmed.eq_ignore_ascii_case("discard") {
                    return discarded(terminal);
                }
                if trimmed.is_empty() {
                    continue;
                }
                session.set_feedback(Some(trimmed.to_string()));
            }
            Err(gen::GenerationError::AttemptsExhausted { errors, .. }) => {
                writeln!(terminal.err, "✖ all attempts failed validation:")?;
                for err in &errors {
                    writeln!(terminal.err, "  - {err}")?;
                }
                writeln!(
                    terminal.err,
                    "Give free-text feedback to regenerate, or 'discard' to write nothing:"
                )?;
                terminal.err.flush()?;

                let Some(line) = read_line(terminal)? else {
                    return eof_discard(terminal);
                };
                if line.trim().eq_ignore_ascii_case("discard") {
                    return discarded(terminal);
                }
                session.set_feedback(Some(line));
            }
            Err(gen::GenerationError::Target(e)) => return Err(RunnerError::Target(e)),
            Err(e @ (gen::GenerationError::Provider(_) | gen::GenerationError::Preparation(_))) => {
                return Err(RunnerError::Npc(e))
            }
        }
    }
}

/// Render the accepted config and either print the dry run or write it.
fn finish_npc<C: AuthorChat>(
    command: &NpcCommand<'_, C>,
    payload: gen::NpcPayload,
    terminal: &mut Terminal<'_>,
) -> Result<RunnerOutcome, RunnerError> {
    let rendered = ron::ser::to_string_pretty(&payload.config, ron::ser::PrettyConfig::default())
        .map_err(|e| RunnerError::Message(e.to_string()))?;

    if command.dry_run {
        writeln!(terminal.out, "{rendered}")?;
        writeln!(terminal.out, "--dry-run: nothing written")?;
        return Ok(RunnerOutcome::Completed);
    }

    let name = payload
        .identity
        .as_ref()
        .map(|identity| identity.name.clone())
        .unwrap_or_else(|| command.npc_id.to_string());
    let mode = if command.create {
        CreateMode::Create { name: name.clone() }
    } else {
        CreateMode::Replace
    };

    let existing_npc: Npc = if command.create {
        Npc {
            id: command.npc_id.to_string(),
            name,
            model: None,
            decisions: [].into(),
        }
    } else {
        command
            .pack
            .npcs
            .get(command.npc_id)
            .cloned()
            .ok_or_else(|| {
                RunnerError::Message(format!(
                    "npc `{}` not found in the pack — use --create",
                    command.npc_id
                ))
            })?
    };

    apply_candidate(
        command.pack_root,
        &existing_npc,
        command.question,
        &payload.config,
        mode,
    )?;

    writeln!(
        terminal.out,
        "✔ Generated and validated: decisions[\"{}\"]",
        command.question
    )?;
    writeln!(
        terminal.out,
        "  criteria: {} · fallback: {}",
        describe_criteria(&payload.config),
        payload.config.fallback.landing_key()
    )?;
    writeln!(terminal.out, "Wrote npcs/{}.ron", command.npc_id)?;
    Ok(RunnerOutcome::Completed)
}

/// `unmei author draft` (drafting contract §1–§7).
pub async fn run_draft<C: AuthorChat>(
    command: DraftCommand<'_, C>,
    terminal: &mut Terminal<'_>,
) -> Result<RunnerOutcome, RunnerError> {
    // Terminal target preflight, before the interactive brief prompt, any
    // session, attempt or provider work (contract §10).
    let target = SceneTarget::new(command.scene_id)?;
    let checked = CheckedPack::new(command.pack)?;
    checked.require_scene(target.id())?;

    if command.interactive {
        return draft_interactive(&command, terminal).await;
    }
    warn_if_unsteered(
        command.pack_root,
        command.brief,
        "the draft proceeds from scene and cast context alone; add a brief for intent",
        terminal,
    )?;
    draft_one_shot(&command, terminal).await
}

/// One-shot drafting: generate → preview → write (or `--dry-run`), retries
/// inside the driver; exhaustion prints the full history and writes nothing.
async fn draft_one_shot<C: AuthorChat>(
    command: &DraftCommand<'_, C>,
    terminal: &mut Terminal<'_>,
) -> Result<RunnerOutcome, RunnerError> {
    let mut session = gen::DraftSession::new();
    let outcome = match gen::draft_with_session(&input(command, command.brief), &mut session).await
    {
        Ok(outcome) => outcome,
        Err(gen::DraftError::AttemptsExhausted {
            raw_outputs,
            errors,
        }) => {
            print_exhausted(&raw_outputs, &errors, terminal)?;
            return Err(RunnerError::DraftExhausted);
        }
        Err(gen::DraftError::Target(e)) => return Err(RunnerError::Target(e)),
        Err(gen::DraftError::Provider(e)) => return Err(RunnerError::DraftProvider(e)),
    };

    write!(
        terminal.out,
        "{}",
        draft_preview(command.scene_id, command.run, &outcome)
    )?;
    if command.dry_run {
        writeln!(terminal.out, "--dry-run: nothing written")?;
        return Ok(RunnerOutcome::Completed);
    }
    write_atomic(command.scene_path, &outcome.edited_scene)?;
    report_draft(command.scene_id, command.run, &outcome, terminal)?;
    Ok(RunnerOutcome::Completed)
}

/// The guided drafting `-i` session: brief → generate → preview → feedback /
/// accept / discard — one run per session, over the same complete attempt and
/// driver as one-shot. Exhaustion takes feedback; `accept` is refused until a
/// validated candidate exists.
async fn draft_interactive<C: AuthorChat>(
    command: &DraftCommand<'_, C>,
    terminal: &mut Terminal<'_>,
) -> Result<RunnerOutcome, RunnerError> {
    let brief = match command.brief.filter(|b| !b.trim().is_empty()) {
        Some(current) => {
            writeln!(
                terminal.out,
                "Brief for `{}` in {} (enter to keep, or type a replacement):",
                command.run.handle, command.scene_id
            )?;
            writeln!(terminal.out, "{current}")?;
            terminal.out.flush()?;
            let Some(replacement) = read_line(terminal)? else {
                return eof_discard(terminal);
            };
            let replacement = replacement.trim();
            if replacement.is_empty() {
                Some(current.to_string())
            } else {
                Some(replacement.to_string())
            }
        }
        None => {
            writeln!(
                terminal.out,
                "Brief for `{}` in {} (end with an empty line; empty for none):",
                command.run.handle, command.scene_id
            )?;
            terminal.out.flush()?;
            let collected = read_paragraph(terminal)?;
            if collected.trim().is_empty() {
                None
            } else {
                Some(collected)
            }
        }
    };
    warn_if_unsteered(
        command.pack_root,
        brief.as_deref(),
        "the draft proceeds from scene and cast context alone; add a brief for intent",
        terminal,
    )?;

    let mut session = gen::DraftSession::new();
    loop {
        match gen::draft_with_session(&input(command, brief.as_deref()), &mut session).await {
            Ok(outcome) => {
                write!(
                    terminal.out,
                    "{}",
                    draft_preview(command.scene_id, command.run, &outcome)
                )?;
                // Blank input re-prompts here without regenerating; any other
                // text is feedback for a regeneration.
                loop {
                    writeln!(
                        terminal.out,
                        "Type 'accept' to write, 'discard' to write nothing, or free-text feedback to regenerate:"
                    )?;
                    terminal.out.flush()?;

                    let Some(line) = read_line(terminal)? else {
                        return eof_discard(terminal);
                    };
                    let trimmed = line.trim();
                    if trimmed.eq_ignore_ascii_case("accept") {
                        if command.dry_run {
                            writeln!(terminal.out, "--dry-run: nothing written")?;
                            return Ok(RunnerOutcome::Completed);
                        }
                        write_atomic(command.scene_path, &outcome.edited_scene)?;
                        report_draft(command.scene_id, command.run, &outcome, terminal)?;
                        return Ok(RunnerOutcome::Completed);
                    }
                    if trimmed.eq_ignore_ascii_case("discard") {
                        return discarded(terminal);
                    }
                    if trimmed.is_empty() {
                        continue;
                    }
                    session.set_feedback(Some(trimmed.to_string()));
                    break;
                }
            }
            Err(gen::DraftError::AttemptsExhausted {
                raw_outputs,
                errors,
            }) => {
                // §5: print the best raw output plus every error, then take
                // feedback (the driver left all errors on the session) or
                // discard. Nothing is accepted here — `accept` needs a
                // validated draft.
                print_exhausted(&raw_outputs, &errors, terminal)?;
                loop {
                    writeln!(
                        terminal.err,
                        "Give free-text feedback to regenerate, or 'discard' to write nothing:"
                    )?;
                    terminal.err.flush()?;
                    let Some(line) = read_line(terminal)? else {
                        return eof_discard(terminal);
                    };
                    let trimmed = line.trim();
                    if trimmed.eq_ignore_ascii_case("discard") {
                        return discarded(terminal);
                    }
                    if trimmed.eq_ignore_ascii_case("accept") {
                        writeln!(
                            terminal.err,
                            "nothing has passed validation to accept — give feedback to regenerate, or 'discard'"
                        )?;
                        continue;
                    }
                    if trimmed.is_empty() {
                        continue;
                    }
                    session.set_feedback(Some(trimmed.to_string()));
                    break;
                }
            }
            Err(gen::DraftError::Target(e)) => return Err(RunnerError::Target(e)),
            Err(gen::DraftError::Provider(e)) => return Err(RunnerError::DraftProvider(e)),
        }
    }
}
