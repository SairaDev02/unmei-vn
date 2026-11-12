//! The runner's terminal surface: the injected reader/writer pair, the guided
//! input helpers, and the warning, exhaustion and success rendering shared by
//! one-shot and `-i` (author-cli contract §3/§5, drafting contract §5/§7).

use std::collections::HashSet;
use std::io::{BufRead, Write};
use std::path::Path;

use crate::draft;
use crate::gen;
use crate::provider::AuthorChat;
use crate::runner::{DraftCommand, RunnerError, RunnerOutcome};

/// The borrowed terminal the runner reads feedback from and renders through.
/// Both streams and the source are injected so direct composition tests can
/// exercise the real runner without a process.
pub struct Terminal<'a> {
    pub input: &'a mut dyn BufRead,
    pub out: &'a mut dyn Write,
    pub err: &'a mut dyn Write,
}

/// The per-regeneration drafting inputs; the session owns everything that
/// persists across regenerations. `brief` is passed separately because the
/// interactive session may have collected or edited it after resolution.
pub(crate) fn input<'a, 'b, C: AuthorChat>(
    command: &'b DraftCommand<'a, C>,
    brief: Option<&'b str>,
) -> gen::DraftInput<'b, C> {
    gen::DraftInput {
        pack: command.pack,
        scene_id: command.scene_id,
        run: command.run,
        scene_text: command.scene_text,
        allowed_speakers: command.allowed_speakers,
        brief,
        client: command.client,
        model: command.model,
    }
}

/// A closed stdin unwinds the guided session writing nothing.
pub(crate) fn eof_discard(terminal: &mut Terminal<'_>) -> Result<RunnerOutcome, RunnerError> {
    writeln!(terminal.out, "Input ended — nothing written.")?;
    Ok(RunnerOutcome::Discarded)
}

/// `discard` (or EOF): nothing is written, the scene is left exactly as
/// authored.
pub(crate) fn discarded(terminal: &mut Terminal<'_>) -> Result<RunnerOutcome, RunnerError> {
    writeln!(terminal.out, "Discarded — nothing written.")?;
    Ok(RunnerOutcome::Discarded)
}

/// §3/§6: context is automatic, but warn when there is neither a brief nor a
/// glossary to steer the command.
pub(crate) fn warn_if_unsteered(
    pack_root: &Path,
    brief: Option<&str>,
    tail: &str,
    terminal: &mut Terminal<'_>,
) -> Result<(), RunnerError> {
    if brief.is_none() && !pack_root.join("pack.glossary.md").exists() {
        writeln!(
            terminal.err,
            "warning: no --brief and no pack.glossary.md — {tail}"
        )?;
    }
    Ok(())
}

/// One line of input, trimmed of the newline. `None` on EOF — the guided
/// session treats a closed stdin as "discard" (write nothing, exit clean).
pub(crate) fn read_line(terminal: &mut Terminal<'_>) -> Result<Option<String>, RunnerError> {
    let mut line = String::new();
    let bytes = terminal.input.read_line(&mut line)?;
    if bytes == 0 {
        return Ok(None);
    }
    Ok(Some(line))
}

/// Multi-line paragraph read: ends on a blank line (interactive brief).
pub(crate) fn read_paragraph(terminal: &mut Terminal<'_>) -> Result<String, RunnerError> {
    let mut lines = Vec::new();
    loop {
        match read_line(terminal)? {
            None => break,
            Some(line) => {
                if line.trim().is_empty() {
                    break;
                }
                lines.push(line.trim_end().to_string());
            }
        }
    }
    Ok(lines.join("\n"))
}

/// Print the best raw output + every validation error before failing (§5).
pub(crate) fn print_exhausted(
    raw_outputs: &[String],
    errors: &[String],
    terminal: &mut Terminal<'_>,
) -> Result<(), RunnerError> {
    if let Some(last) = raw_outputs.last() {
        writeln!(terminal.err, "--- best model output ---\n{last}")?;
    }
    writeln!(terminal.err, "--- validation errors ---")?;
    for err in errors {
        writeln!(terminal.err, "  - {err}")?;
    }
    Ok(())
}

/// The drafted run, framed for the preview (§5), shared by one-shot and `-i`.
pub(crate) fn draft_preview(
    scene_id: &str,
    run: &draft::Run,
    outcome: &gen::DraftOutcome,
) -> String {
    let mut out = format!("--- draft of `{}` in {scene_id} ---\n", run.handle);
    for line in &outcome.lines {
        out.push_str(&line.render());
        out.push('\n');
    }
    out.push_str("--- end draft ---\n");
    out
}

/// The post-write summary shared by one-shot and an accepted `-i` regeneration.
pub(crate) fn report_draft(
    scene_id: &str,
    run: &draft::Run,
    outcome: &gen::DraftOutcome,
    terminal: &mut Terminal<'_>,
) -> Result<(), RunnerError> {
    writeln!(
        terminal.out,
        "✔ Generated and validated: {scene_id} `{}` (attempt {})",
        run.handle, outcome.attempt
    )?;
    if let Some(reason) = &outcome.fallback_detail {
        writeln!(
            terminal.out,
            "  structured output unsupported by the endpoint: {reason} — used the free-text fallback"
        )?;
    }
    writeln!(
        terminal.out,
        "  {} say-line(s) · run {} · speakers: {}",
        outcome.lines.len(),
        run.span(),
        speakers_of(&outcome.lines)
    )?;
    writeln!(terminal.out, "Wrote scenes/{scene_id}.unmei")?;
    Ok(())
}

/// A stable, comma-separated list of the speakers in a drafted run.
pub(crate) fn speakers_of(lines: &[draft::SayLine]) -> String {
    let speakers: HashSet<String> = lines.iter().map(|l| l.speaker.clone()).collect();
    draft::speaker_list(&speakers)
}

/// A one-line summary of a generated config's criteria, per question type
/// (the `✔ Generated and validated` line).
pub(crate) fn describe_criteria(config: &unmei_engine::content::DecisionConfig) -> String {
    if let Some(map) = config.choice_criteria() {
        let mut ids: Vec<&String> = map.keys().collect();
        ids.sort();
        return ids
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ");
    }
    if let Some(levels) = config.score_criteria() {
        return format!("{} levels", levels.len());
    }
    match &config.criteria {
        unmei_engine::content::Criteria::Noul(Some(_)) => "true, false".to_string(),
        unmei_engine::content::Criteria::Noul(None) => "instruction-only".to_string(),
        _ => String::new(),
    }
}
