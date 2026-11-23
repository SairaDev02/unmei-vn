//! `unmei author draft`: its resolved input, the one-shot and interactive
//! paths, and the existing atomic writer.

use std::collections::HashSet;
use std::path::Path;

use unmei_engine::Pack;

use super::{write_missing_pin_notes, RunnerError, RunnerOutcome, Terminal};
use crate::draft;
use crate::gen;
use crate::provider::AuthorChat;
use crate::splice::write_atomic;
use crate::target::{CheckedPack, SceneTarget};
use crate::terminal::{
    discarded, draft_preview, eof_discard, input, print_exhausted, read_line, read_paragraph,
    report_draft, warn_if_unsteered,
};

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

    // The draft passed the gate: note the unpinned NPCs, once per command.
    write_missing_pin_notes(command.pack, terminal.err)?;
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
    let mut noted = false;
    loop {
        match gen::draft_with_session(&input(command, brief.as_deref()), &mut session).await {
            Ok(outcome) => {
                if !noted {
                    write_missing_pin_notes(command.pack, terminal.err)?;
                    noted = true;
                }
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
