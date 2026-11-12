//! The `unmei` binary: the authoring CLI for Unmei content packs.
//!
//! `unmei author npc <NPC_ID> --question <QID>` generates or replaces one NPC
//! decision config through any OpenAI-compatible chat endpoint. It validates
//! the config against the whole content pack, then writes it, or prints it
//! with `--dry-run`. Scene text, state and other NPCs stay untouched.
//!
//! `unmei author draft <SCENE_ID>` drafts one run of prose in an existing
//! scene. A run is a block of consecutive say-lines (`Speaker: text` lines),
//! and `--list` prints the runs of the scene.
//!
//! Authoring is offline in one sense: it happens in this tool, never while the
//! game runs. A command that calls the model still reaches the chat endpoint
//! over the network. The repository's `docs/author-cli.md` § 1 Command surface
//! and `docs/author-cli-drafting.md` § 1 Command surface own the full command
//! surface.

use std::io::Write;
use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand, ValueEnum};
use unmei::draft;
use unmei::provider::{resolve_provider, ConfigInput, OpenAiChat};
use unmei::runner::{self, DraftCommand, NpcCommand, Terminal};
use unmei::target::{CheckedPack, NpcTarget, SceneTarget, TargetError};
use unmei_engine::{load_pack, read_pack_text, QuestionType};

mod mask;

#[derive(Parser)]
#[command(name = "unmei", about = "unmei engine authoring CLI", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Authoring tools: generate an NPC decision config or draft scene prose
    /// with an OpenAI-compatible chat model.
    Author(AuthorArgs),
}

#[derive(Args)]
struct AuthorArgs {
    #[command(subcommand)]
    command: AuthorCommand,
}

#[derive(Subcommand)]
enum AuthorCommand {
    /// Generate or replace one decision config in npcs/<NPC_ID>.ron.
    Npc {
        /// NPC id (filename stem of npcs/<id>.ron).
        npc_id: String,
        /// Decision config to generate or replace.
        #[arg(long)]
        question: String,
        /// Question type: choice | noul | score (default: choice; on replace,
        /// the existing config's type unless overridden).
        #[arg(long = "type", value_enum, value_name = "T")]
        question_type: Option<TypeArg>,
        /// Create a new NPC file (identity header + first config).
        #[arg(long)]
        create: bool,
        /// Author intent for this config; optional.
        #[arg(long)]
        brief: Option<String>,
        /// Read the brief from a file (for long notes).
        #[arg(long)]
        brief_file: Option<PathBuf>,
        /// Guided session for one config.
        #[arg(short, long)]
        interactive: bool,
        /// Pack root (default: cwd, must contain pack.ron).
        #[arg(long)]
        pack: Option<PathBuf>,
        /// Authoring chat model (required unless set in unmei.author.ron).
        #[arg(long)]
        model: Option<String>,
        /// OpenAI-compatible endpoint (default: https://openrouter.ai/api).
        #[arg(long)]
        base_url: Option<String>,
        /// Print the generated RON; write nothing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Draft one run of prose in an existing scene (`--list` to see the runs).
    Draft {
        /// Scene id (filename stem of scenes/<id>.unmei).
        scene_id: String,
        /// The run to address: `start`, a label name, or either with a `#n`
        /// ordinal, exactly as printed by --list.
        #[arg(long, value_name = "ANCHOR", conflicts_with = "list")]
        from: Option<String>,
        /// Print every run in the scene and exit; drafts nothing.
        #[arg(long)]
        list: bool,
        /// Author intent for this run; optional.
        #[arg(long)]
        brief: Option<String>,
        /// Read the brief from a file (for long notes).
        #[arg(long)]
        brief_file: Option<PathBuf>,
        /// Guided session for one run: generate, preview, then accept, discard
        /// or regenerate with free-text feedback.
        #[arg(short, long, conflicts_with = "list")]
        interactive: bool,
        /// Pack root (default: cwd, must contain pack.ron).
        #[arg(long)]
        pack: Option<PathBuf>,
        /// Authoring chat model (required unless set in unmei.author.ron).
        #[arg(long)]
        model: Option<String>,
        /// OpenAI-compatible endpoint (default: https://openrouter.ai/api).
        #[arg(long)]
        base_url: Option<String>,
        /// Print the drafted run; write nothing.
        #[arg(long)]
        dry_run: bool,
    },
}

/// The `--type` flag's value; maps to the engine's [`QuestionType`].
#[derive(Clone, Copy, Debug, ValueEnum)]
enum TypeArg {
    Choice,
    Noul,
    Score,
}

impl From<TypeArg> for QuestionType {
    fn from(value: TypeArg) -> Self {
        match value {
            TypeArg::Choice => QuestionType::Choice,
            TypeArg::Noul => QuestionType::Noul,
            TypeArg::Score => QuestionType::Score,
        }
    }
}

fn main() {
    // Snapshot both recognized credential values before argument parsing,
    // logging or provider resolution can render anything (ADR 0014).
    let mask = mask::Mask::from_env();
    let stdout = mask::masked_stream(&mask, std::io::stdout());
    let stderr = mask::masked_stream(&mask, std::io::stderr());
    mask::init_logging(&stderr);
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => render_parse_error(err, &mask, &stdout, &stderr),
    };
    let result = match cli.command {
        Command::Author(args) => match args.command {
            AuthorCommand::Npc {
                npc_id,
                question,
                question_type,
                create,
                brief,
                brief_file,
                interactive,
                pack,
                model,
                base_url,
                dry_run,
            } => run_author_npc(
                npc_id,
                question,
                question_type,
                create,
                brief,
                brief_file,
                interactive,
                pack,
                model,
                base_url,
                dry_run,
                &stdout,
                &stderr,
            ),
            AuthorCommand::Draft {
                scene_id,
                from,
                list,
                brief,
                brief_file,
                interactive,
                pack,
                model,
                base_url,
                dry_run,
            } => run_author_draft(
                scene_id,
                from,
                list,
                brief,
                brief_file,
                interactive,
                pack,
                model,
                base_url,
                dry_run,
                &stdout,
                &stderr,
            ),
        },
    };
    if let Err(err) = result {
        let mut writer = stderr.clone();
        let _ = writeln!(writer, "✖ {err}");
        drop(writer);
        let _ = stdout.finish();
        let _ = stderr.finish();
        std::process::exit(1);
    }
    let _ = stdout.finish();
    let _ = stderr.finish();
}

/// Argument/setup rendering is outside the runner's [`Terminal`]. With any
/// recognized value configured, a Clap error or help renders through the mask in
/// plain form; with none, nothing can leak, so Clap keeps its own rendering.
fn render_parse_error(
    err: clap::Error,
    mask: &mask::Mask,
    stdout: &mask::MaskedStream<std::io::Stdout>,
    stderr: &mask::MaskedStream<std::io::Stderr>,
) -> ! {
    if mask.is_empty() {
        let _ = err.print();
    } else if err.use_stderr() {
        let mut writer = stderr.clone();
        let _ = writer.write_all(err.render().to_string().as_bytes());
    } else {
        let mut writer = stdout.clone();
        let _ = writer.write_all(err.render().to_string().as_bytes());
    }
    let _ = stdout.finish();
    let _ = stderr.finish();
    std::process::exit(err.exit_code());
}

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error("{0}")]
    Message(String),
    #[error("pack failed to load: {0}")]
    Pack(String),
    #[error("{0}")]
    Draft(#[from] draft::UnknownRun),
    #[error(transparent)]
    Target(#[from] TargetError),
    #[error("{0}")]
    Runner(#[from] runner::RunnerError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Resolve the pack root: `--pack`, or the cwd, which must contain `pack.ron`.
fn resolve_pack_root(pack: Option<PathBuf>) -> Result<PathBuf, CliError> {
    let pack_root = pack.unwrap_or_else(|| std::env::current_dir().unwrap());
    if !pack_root.join("pack.ron").exists() {
        return Err(CliError::Message(format!(
            "no pack.ron in {} — run from a pack root or pass --pack <DIR>",
            pack_root.display()
        )));
    }
    Ok(pack_root)
}

/// Load and validate the pack through the shared engine loader, collapsing its
/// error list into one display string.
fn load_pack_or_report(pack_root: &Path) -> Result<unmei_engine::LoadedPack, CliError> {
    load_pack(pack_root).map_err(|errors| {
        CliError::Pack(
            errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
        )
    })
}

/// Resolve the optional brief from `--brief` / `--brief-file` (flags shared
/// with the rest of the author CLI). The flag wins when both are given.
fn resolve_brief(
    brief: Option<String>,
    brief_file: Option<PathBuf>,
) -> Result<Option<String>, CliError> {
    match (brief, brief_file) {
        (Some(brief), _) => Ok(Some(brief)),
        (None, Some(path)) => Ok(Some(std::fs::read_to_string(&path).map_err(|e| {
            CliError::Message(format!("cannot read --brief-file {path:?}: {e}"))
        })?)),
        (None, None) => Ok(None),
    }
}

/// The async runtime main constructs and hands to the runner; the runner
/// itself never builds one.
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime builds")
}

/// `unmei author draft` — one-shot and interactive `-i` drafting (drafting
/// contract §1–§7).
///
/// Resolution stays here — the pack, the addressed run, the brief, the
/// provider — and the command itself runs through [`runner::run_draft`].
#[allow(
    clippy::too_many_arguments,
    reason = "the `draft` subcommand's parsed flags, threaded down its one call path"
)]
fn run_author_draft(
    scene_id: String,
    from: Option<String>,
    list: bool,
    brief: Option<String>,
    brief_file: Option<PathBuf>,
    interactive: bool,
    pack: Option<PathBuf>,
    model: Option<String>,
    base_url: Option<String>,
    dry_run: bool,
    stdout: &mask::MaskedStream<std::io::Stdout>,
    stderr: &mask::MaskedStream<std::io::Stderr>,
) -> Result<(), CliError> {
    if list == from.is_some() {
        return Err(CliError::Message(
            "give exactly one of --list (show the scene's runs) or --from <ANCHOR> (address one run)"
                .into(),
        ));
    }

    // Terminal preflight ordering (contract §10): the raw filename check comes
    // before pack/brief/provider work; the copied-key inventory and exact Scene
    // membership come straight after loading, before the brief is read.
    let raw_target = SceneTarget::new(&scene_id)?;
    let pack_root = resolve_pack_root(pack)?;
    let loaded = load_pack_or_report(&pack_root)?;
    let checked = CheckedPack::new(&loaded.pack)?;
    let target = checked.require_scene(raw_target.id())?.clone();

    let scene = loaded
        .pack
        .scenes
        .get(&scene_id)
        .expect("membership was checked above");

    let runs = draft::runs(scene);

    if list {
        let mut out = stdout.clone();
        write!(out, "{}", draft::render_list(&runs))?;
        out.flush()?;
        return Ok(());
    }

    let address = from.expect("--from is set when --list is absent");
    let run = draft::resolve(&runs, &scene_id, &address)?.clone();

    // An empty `--brief`/`--brief-file` is no brief at all. The `-i` session
    // collects one from the author before its first regeneration when none is
    // given.
    let brief = resolve_brief(brief, brief_file)?.filter(|b| !b.trim().is_empty());

    let provider = resolve_provider(ConfigInput {
        pack_root: &pack_root,
        model: model.as_deref(),
        base_url: base_url.as_deref(),
    })
    .map_err(CliError::Message)?;

    // The original bytes: the splice replaces only the run region, so every
    // line outside it survives byte-for-byte. The path is the checked target's
    // fixed `scenes/<id>.unmei` construction, never a raw string join.
    let scene_path = target.path_in(&pack_root);
    let scene_text = read_pack_text(&scene_path)?;

    let allowed = draft::allowed_speakers(scene, &loaded.pack);
    let client = OpenAiChat::new(provider.base_url.clone(), provider.api_key.clone());

    let command = DraftCommand {
        pack: &loaded.pack,
        pack_root: &pack_root,
        scene_id: &scene_id,
        run: &run,
        scene_text: &scene_text,
        scene_path: &scene_path,
        allowed_speakers: &allowed,
        brief: brief.as_deref(),
        interactive,
        dry_run,
        client: &client,
        model: &provider.model,
    };

    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut out = stdout.clone();
    let mut err = stderr.clone();
    let mut terminal = Terminal {
        input: &mut input,
        out: &mut out,
        err: &mut err,
    };
    let result = runtime().block_on(runner::run_draft(command, &mut terminal));
    // Main finishes the shared buffers once its own final error is written, so a
    // held suffix survives the runner/main boundary.
    out.flush()?;
    err.flush()?;
    result?;
    Ok(())
}

/// `unmei author npc` — one-shot and interactive `-i` generation (author-cli
/// contract §1–§6).
///
/// Resolution stays here — the pack, the question type, the brief, the
/// provider — and the command itself runs through [`runner::run_npc`].
#[allow(
    clippy::too_many_arguments,
    reason = "the `npc` subcommand's parsed flags, threaded down its one call path"
)]
fn run_author_npc(
    npc_id: String,
    question: String,
    question_type: Option<TypeArg>,
    create: bool,
    brief: Option<String>,
    brief_file: Option<PathBuf>,
    interactive: bool,
    pack: Option<PathBuf>,
    model: Option<String>,
    base_url: Option<String>,
    dry_run: bool,
    stdout: &mask::MaskedStream<std::io::Stdout>,
    stderr: &mask::MaskedStream<std::io::Stderr>,
) -> Result<(), CliError> {
    // Terminal preflight ordering (contract §10): Clap owns the basic mode
    // syntax; the raw filename check precedes pack/brief/provider work, and
    // the copied-key inventory is checked before the brief and the provider.
    let _target = NpcTarget::new(&npc_id)?;
    let pack_root = resolve_pack_root(pack)?;
    let loaded = load_pack_or_report(&pack_root)?;
    let _checked = CheckedPack::new(&loaded.pack)?;

    let brief = resolve_brief(brief, brief_file)?;

    let provider = resolve_provider(ConfigInput {
        pack_root: &pack_root,
        model: model.as_deref(),
        base_url: base_url.as_deref(),
    })
    .map_err(CliError::Message)?;

    // §1: --type wins; on replace, inherit unless overridden.
    let question_type = match question_type {
        Some(flag) => QuestionType::from(flag),
        None => loaded
            .pack
            .npcs
            .get(&npc_id)
            .and_then(|npc| npc.decisions.get(&question))
            .map(|config| config.question_type)
            .unwrap_or_default(),
    };

    let client = OpenAiChat::new(provider.base_url.clone(), provider.api_key.clone());

    let command = NpcCommand {
        pack: &loaded.pack,
        pack_root: &pack_root,
        npc_id: &npc_id,
        question: &question,
        question_type,
        create,
        brief: brief.as_deref(),
        interactive,
        dry_run,
        client: &client,
        model: &provider.model,
    };

    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut out = stdout.clone();
    let mut err = stderr.clone();
    let mut terminal = Terminal {
        input: &mut input,
        out: &mut out,
        err: &mut err,
    };
    let result = runtime().block_on(runner::run_npc(command, &mut terminal));
    // Main finishes the shared buffers once its own final error is written, so a
    // held suffix survives the runner/main boundary.
    out.flush()?;
    err.flush()?;
    result?;
    Ok(())
}
