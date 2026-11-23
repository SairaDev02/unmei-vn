//! `unmei author npc`: its resolved input, the one-shot and interactive paths,
//! and the existing atomic writer.

use std::path::Path;

use unmei_engine::content::Npc;
use unmei_engine::{Pack, QuestionType};

use super::{write_missing_pin_notes, RunnerError, RunnerOutcome, Terminal};
use crate::gen::{self, GenerateInput, GenerationSession};
use crate::provider::AuthorChat;
use crate::splice::{apply_candidate, CreateMode};
use crate::target::{CheckedPack, NpcTarget};
use crate::terminal::{
    describe_criteria, discarded, eof_discard, print_exhausted, read_line, read_paragraph,
    warn_if_unsteered,
};

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
    /// The model pin a `--create` writes into the new NPC file (`docs/nanogpt.md`
    /// N25); `None` when not creating. Replace never reads it.
    pub decision_pin: Option<&'a str>,
}

impl<C: AuthorChat> NpcCommand<'_, C> {
    /// The pin a created NPC gets: only `--create` writes one.
    fn create_pin(&self) -> Option<&str> {
        self.decision_pin.filter(|_| self.create)
    }
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
        decision_pin: command.create_pin(),
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

    // The candidate passed the gate: note the unpinned NPCs, once per command.
    write_missing_pin_notes(command.pack, terminal.err)?;
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
    let mut noted = false;

    loop {
        let input = GenerateInput {
            pack: command.pack,
            npc_id: command.npc_id,
            question: command.question,
            question_type: command.question_type,
            brief: brief.as_deref(),
            client: command.client,
            model: command.model,
            decision_pin: command.create_pin(),
        };

        match gen::generate_with_session(&input, &mut session).await {
            Ok(outcome) => {
                if !noted {
                    write_missing_pin_notes(command.pack, terminal.err)?;
                    noted = true;
                }
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
                write_pin_line(command, terminal)?;
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

/// On `--create`, show the pin the new NPC file will carry, so a dry run and a
/// preview display everything that would be written.
fn write_pin_line<C: AuthorChat>(
    command: &NpcCommand<'_, C>,
    terminal: &mut Terminal<'_>,
) -> Result<(), RunnerError> {
    if let Some(pin) = command.create_pin() {
        writeln!(terminal.out, "model pin for the new NPC: {pin:?}")?;
    }
    Ok(())
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
        write_pin_line(command, terminal)?;
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
            model: command.create_pin().map(str::to_string),
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
