---
title: Authoring CLI contract for NPC decision configs
summary: The offline authoring CLI contract for NPC decision configs — command surface, write unit, full-pack validation gate, provider configuration and failure UX.
keywords:
  - authoring-cli
  - pack-validation
  - content-pack
  - credentials
---
# `unmei author` CLI contract

**Status:** settled by the design interview (rounds 1–3 plus confirmation) · **Scope:** the vertical slice.

This is the written contract for the offline authoring CLI: what `unmei author` accepts, generates, validates, and writes. It emits into the `Npc`/`DecisionConfig` schema ([content format §4](content-format.md#4-decision-configs-and-decision-points)); LLM JSON is serde-converted to RON at the CLI boundary ([ADR 0001](adr/0001-ron-for-structured-content.md)). **This is a contract, not an implementation** — the build is written against it.

**Extended by** [`docs/author-cli-drafting.md`](author-cli-drafting.md) — the contract for drafting `.unmei` prose (`unmei author draft`), which lifts the §8 non-goal below.

## 1. Command surface

Two modes over **one shared core** (generate → validate → preview → write). Interactive mode is a guided wrapper around the same pipeline, not a second engine.

```text
unmei author npc <NPC_ID> --question <QID> [OPTIONS]
unmei author npc <NPC_ID> --create --question <QID> [OPTIONS]

Generate or replace one decision config in npcs/<NPC_ID>.ron using an
OpenAI-compatible chat endpoint. Structured configs only — never scene text.

Options:
  --question <QID>     Decision config to generate or replace (required)
  --type <T>           Question type: choice | noul | score (default: choice;
                       on replace, the existing config's type unless overridden)
  --create             Create a new NPC file (identity header + first config)
  --brief <TEXT>       Author intent for this config; optional
  --brief-file <PATH>  Read the brief from a file (for long notes)
  -i, --interactive    Guided session for one config (see §5)
  --pack <DIR>         Pack root (default: cwd, must contain pack.ron)
  --model <MODEL>      Authoring chat model (required unless set in config, §4)
  --base-url <URL>     OpenAI-compatible endpoint (default: https://openrouter.ai/api)
  --dry-run            Print the generated RON; write nothing
  -h, --help
```

- `--question` + no `--create`: read the existing `npcs/<id>.ron`, **add or replace only that question's config**. Everything else in the file — identity, other decisions, comments — passes through untouched (RON comments are a stated reason RON was chosen; regeneration must never erase them).
- `--create`: writes a **new** `npcs/<id>.ron` — identity header plus the one config. Every later config is added by re-running without `--create`. Error if the file already exists.
- Pack discovery: cwd must be a pack root (`pack.ron` present), or `--pack <DIR>` names it.

## 2. Generation unit and write surface

One invocation generates **exactly one decision config** — the `type` (emitted only for `noul`/`score`) / `instructions` / `state_keys` / `criteria` / `fallback` block for one question id — plus, on `--create`, the NPC identity fields (`id`, `name`, optional `model`). The `--type` flag selects the question type; on replace, the target config's existing `type` is inherited unless the flag overrides it, and `--create` defaults to `choice`.

The CLI **never** writes or drafts:

- `.unmei` scene text or dialogue lines (prose DSL is out of the generator's reach for the slice);
- any config other than the named question's;
- `state.ron`, `pack.ron`, or anything under `assets/`.

Output format is RON, converted from the model's JSON by serde before any validation or write.

## 3. Validation gate (before any write)

Every candidate generation must pass **full pack cross-validation** — the same rules the runtime loader enforces at load time, run at generation time so the author fixes a bad config in seconds instead of at game load. The CLI reuses the loader's validation logic; this list is the contract, not a second implementation. Rule 2 is the exception — it applies the loader's own id grammar to candidate fields the loader does not police, where a different spelling either passes the label grammar or fails opaquely as a later label-closure error:

1. **Schema**: the model's JSON parses into `DecisionConfig` for the selected `type` (plain-string `instructions` and criteria descriptions). `criteria` must take the type's shape: a map (choice), an optional both-or-neither `true`/`false` map (noul), or an ordered array of ≥2 levels (score).
2. **Identifier grammar (CLI drafting pre-check)**: every `choice` criterion id and the `choice` fallback must be snake_case — `[a-z0-9_]+`, non-empty, no surrounding whitespace — the grammar the loader applies to the pack's other ids (NPC and question ids, state keys, command ids). The pack format is looser for criteria (it only advises against spaces in action ids), so this refuses a candidate the loader itself accepts, and names the offending id; it adds no loader rule.
3. **Fallback integrity**: `fallback` is present in the type's landing space — a criterion id (choice), `true`/`false` (noul), or a legend index (score).
4. **State keys**: every `state_keys` entry is declared in `state.ron` (declared-before-use). Unchanged by type.
5. **Label closure**: for every `decide <npc>.<question> -> <prefix>` line in the pack's scenes that targets this NPC + question, every label the type can land on exists in the target scene — choice: `<prefix>.<action>` for each criterion id and the fallback; noul: `<prefix>.true` and `<prefix>.false`; score: `<prefix>.0` … `<prefix>.<n-1>`. (If no scene references the config yet, this check is vacuously satisfied.)
6. **`none` advisory**: warn (not error) when a `choice` config's `criteria` lacks a `"none"` action. Not applicable to `noul`/`score`.

**Validation staging amendment (2026-10-13):** the decided [pack RON bridge](content-format.md#pack-ron-bridge--decided-seam-2026-10-13) specifies one engine-owned serializer for the temporary validation pack's `state.ron`, replacing hand-rendered literals in the staging transplant. Staging emits a quoted-key RON map and preserves state values, including quotes and backslashes in strings; it need not preserve source formatting or comments. Delivered, and moved behind the staging owner in `cli/src/staging.rs`, **not permission to write the author's `state.ron`**: §2's protected write surface is unchanged. The shared provider optional-string adapter also preserves existing config compatibility, including rejection of `Some("…")`; provider precedence, credentials and retry behavior do not change.

**Shared lifecycle design (2026-10-14):** [§9](#9-staged-pack-validation--decided-design-2026-10-14) specifies two CLI-internal concrete validators over one private temporary owner, with verbatim engine diagnostics, delivered as §9 records. It follows the landed RON/retry implementations and the separately tracked [target-safety repair](#10-authoring-target-confinement--decided-design-2026-10-14) (design settled 2026-10-14, delivered 2026-10-16); it neither expands the bridge nor changes retry/error classification.

## 4. Provider configuration

The CLI talks to any OpenAI-compatible **chat** endpoint. That authoring model is distinct from the runtime's pinned decision model (the per-NPC `model` field in content); the two are deliberately separate — one is what writes configs, the other is what plays them.

Precedence, highest first:

1. Flags: `--model`, `--base-url`
2. Pack config file: `unmei.author.ron` at the pack root (optional)
3. Built-in defaults: `base_url: "https://openrouter.ai/api"`; **no default model**

```ron
// unmei.author.ron — optional, at the pack root
(
    base_url: "https://openrouter.ai/api", // any OpenAI-compatible base URL; /v1/chat/completions is appended
    model: "openai/gpt-4o-mini",         // authoring chat model; no built-in default
)
```

`unmei.author.ron` is UTF-8, and a single leading byte order mark is allowed and ignored
([content format §2](content-format.md#2-pack-layout)).

**API key: environment only.** Read from `UNMEI_API_KEY`, falling back to `OPENROUTER_API_KEY`. Never accepted from a flag or config file — a key must never live in a file that might get committed. Missing key or missing model ⇒ immediate, clear error naming exactly what to set; no network call is made.

**Categorical provider diagnostics (2026-10-18).** [ADR 0014](adr/0014-cli-diagnostics-and-terminal-credential-safety.md) now applies to the provider: a retained `ProviderError` carries a fixed category only — transport kind, HTTP status, decode shape or the structured-support reason — never the provider body, a URL or the transport/decoder text that produced it, and the credential-bearing `ProviderConfig`/`AuthorFile`/`OpenAiChat` `Debug` is opaque. The `Authorization` header stays sensitive, wire verbosity is explicitly off, and redirects are not followed, so the bearer key cannot travel to a destination the author did not name. Structured-support classification still inspects the response transiently; only the category survives into the retained value and the degradation report. **Terminal masking (2026-10-18):** both non-empty recognized environment values — including the one provider precedence does not select — are snapshotted before argument parsing, and every exact literal occurrence on stdout/stderr is replaced with the fixed marker `[redacted]`: Clap's argument/help rendering, the final `✖ {err}` line, `--list`, generated previews and dry runs, exhaustion/best-output reports, interactive prompts and the `env_logger` sink. Masking deliberately does not depend on the runner's `Terminal`, and a literal split across formatting or write chunks is masked whole (all writers on one stream share that stream's buffer; stdout and stderr buffer independently). This is not a general secret scanner: unknown values, fragments, encodings, paraphrases, OS input echo, a dependency writing straight to stderr rather than through `env_logger` and a recognized value that is itself a substring of the marker are outside the guarantee. Clap's help/argument output keeps its ordinary rendering when no recognized value is configured and is re-rendered through the mask in plain form when one is; a malformed `RUST_LOG` is reported through the masked logger sink rather than `env_filter`'s own stderr warning. Generated files, provider prompts and correction payloads are neither rewritten nor certified secret-free — a masked preview does not make a pack safe to share. The 120-second timeout, retry classification, precedence and env-only sourcing are unchanged.

## 5. Failure, retry, and editing UX

**One-shot:** on validation failure the CLI automatically retries, up to **3 attempts**, feeding the validation errors back into the prompt as corrections. If all attempts fail: print the best raw model output plus all validation errors, **write nothing**, exit with code 1. On success: preview the resulting RON config, then write.

**Interactive (`-i`):** a guided loop over the same core, **one decision config per session** — collect or edit the brief, generate, preview, take free-text feedback ("make `warm` less effusive", "add a guarded middle action") and regenerate, repeat until `accept` (writes, passing the same §3 gate) or `discard` (writes nothing). Run the command again for the next config.

**`--dry-run`:** print the would-be RON, write nothing, no write-side side effects.

**Undo:** packs are version-controlled by standing decision — **git is the undo button**. The CLI keeps no backup files and offers no in-CLI history.

**Shared retry ownership (2026-10-14):** the [decided attempt seam](author-cli-drafting.md#10-attempt-seam-and-retry-ownership--decided-design-2026-10-14) covers NPC generation and prose drafting with one CLI-local typed retry driver, command-specific complete attempts and private sessions. Delivered 2026-10-15; the retry count, prompts, provider behavior and write surface are unchanged. NPC interactive exhaustion currently omits the raw reply, unlike §5's one-shot failure output; the extraction explicitly preserves that distinct output and defers repair rather than silently unifying UX.

## 6. Prompt inputs (what the model sees)

Context is assembled **automatically**; the author never wires it by hand:

- the author's `--brief` / `--brief-file`, when given (**optional** — generation proceeds from pack context alone; the CLI warns when there is *neither* a brief *nor* a `pack.glossary.md`);
- `pack.glossary.md` (pack-local terms), when present;
- the `state.ron` key vocabulary (bucketed-string guidance included — decision models judge, they don't compute);
- the existing config for that question, when replacing;
- on `--create`, nothing pre-existing — the brief and pack context carry it.

The **prompt-engineering wording** (system prompt, schema instruction phrasing, retry-correction composition) is deliberately *not* fixed by this contract — it is the build's experimentation space.

## 7. Worked example

```console
$ cd my-pack
$ export UNMEI_API_KEY=sk-...          # or OPENROUTER_API_KEY
$ unmei author npc mira --create --question farewell \
    --brief "Mira is the town archivist; her farewell should differ when the \
    player has helped her find the lost ledger."
✔ Generated and validated: decisions["farewell"] (attempt 1)
  criteria: cordial, grateful, none · fallback: cordial
  state_keys: ledger_found — all declared in state.ron ✓
  no scene references mira.farewell yet — label check skipped
Wrote npcs/mira.ron
```

Resulting file (exactly what §2 allows — nothing else touched):

```ron
// npcs/mira.ron
(
    id: "mira",
    name: "Mira",
    decisions: {
        "farewell": (
            instructions: "How does Mira say goodbye to the player, given whether they helped her find the lost ledger?",
            state_keys: ["ledger_found"],
            criteria: {
                "cordial": "A polite, professional goodbye; the ledger work made no difference.",
                "grateful": "A warm, personal goodbye; the player found the lost ledger for her.",
                "none": "Neither description fits the state.",
            },
            fallback: "cordial",
        ),
    },
)
```

The same command with `--type noul` or `--type score` emits that type's `criteria` shape (a both-or-neither `true`/`false` map, or an ordered array) with `fallback` typed to match; the §3 gate then checks the type's own label set.

Failure path:

```console
$ unmei author npc mira --question trust --brief "..."
✖ Attempt 3 of 3 failed validation:
  - fallback "shy" is not a criterion id
  - state_keys["mood"] is not declared in state.ron
Best model output printed above. Nothing was written.
```

## 8. Non-goals (this slice)

- Drafting dialogue or scene text (`.unmei`) with the LLM — **now its own contract**: [`docs/author-cli-drafting.md`](author-cli-drafting.md) (`unmei author draft`, prose runs only).
- In-editor or runtime generation — authoring is offline CLI only (standing decision).
- Multi-config batch generation in one invocation — one config per invocation, by design.
- Interactive multi-config sessions — one config per interactive session.

## 9. Staged pack validation — decided design (2026-10-14)

**Decision:** settled 2026-10-14 through Q1–Q8 plus the complete-design/publication confirmation Q9 at interview. **Delivered 2026-10-16** (delivery note below). It follows the actual [pack RON bridge](content-format.md#pack-ron-bridge--decided-seam-2026-10-13), [attempt/retry](author-cli-drafting.md#10-attempt-seam-and-retry-ownership--decided-design-2026-10-14) and separately tracked [target-safety](#10-authoring-target-confinement--decided-design-2026-10-14) implementations (the target-safety design is settled in [§10](#10-authoring-target-confinement--decided-design-2026-10-14) and delivered 2026-10-16), not merely their design resolutions. The existing protected write surfaces remain binding.

**Delivered (2026-10-16).** `cli/src/staging.rs` owns one private `Staging` value — a fresh real `TempDir`, the staged root, the checked transplant, the NPC/Scene candidate placement and the real engine reload — released on every ordinary return. It exposes only the two crate-visible validators `validate_npc_decision` and `validate_scene` over `CheckedPack`/`NpcTarget`/`SceneTarget`; the old `validate_candidate`, `validate_scene_checked` and arbitrary-destination `transplant_pack` are gone. `gen.rs`'s complete generation and drafting attempts call those validators inside the delivered session/driver composition, and the public `validate_edited_scene` keeps its signature as the diagnostic compatibility adapter. Diagnostics are the engine's verbatim `Display` on both paths: the NPC absolute-staged-path replacement is removed and no symmetric scrub, deduplication, ranking or reorder is added. The command-specific NPC extraction/schema/identity rules and outer substring filter, prose layer 1/render/splice, and preview/accept/discard/dry-run remain outside staging. The NPC staged read/parse swallow and substring-filter candidate bypass were repaired on 2026-10-16; the Scene-path I/O-as-corrections classification is preserved unchanged and remains owned by [drafting §10](author-cli-drafting.md#10-attempt-seam-and-retry-ownership--decided-design-2026-10-14).

### Verified baseline and selected depth

Source inspected at the baseline revision: `cli/src/gen.rs:423–477` creates a temporary directory, transplants, prepares/replaces an NPC file, reloads and renders diagnostics; `:1070–1079` repeats the lifecycle/reload/display for edited Scene text. The useful `transplant_pack` implementation (`:481–527`) already materializes manifest/state and copies Scene/NPC source bytes; assets are not needed by this loader. Its state hand-renderer belongs to the pack RON bridge, not a second defect repair here. These are source facts, not executed CLI results.

Q1 chose depth beyond copying: one home for temporary lifetime, candidate placement, reload and diagnostic display. Q2 compared (A) two concrete validators, (B) a one-call closed whole-file byte override, and (C) a restricted one-use staging session. **A wins:** callers need complete validation, not staged access or ordering. B would push the existing staged NPC read/parse into different source/in-memory preparation; C adds lifecycle knowledge without a present caller need. Removing the selected module would spread orchestration and diagnostic knowledge across both callers, whereas removing only `transplant_pack` spreads copying. Retain its useful implementation, incorporating the decided [pack RON bridge](content-format.md#pack-ron-bridge--decided-seam-2026-10-13) serializer once it lands rather than copying or re-solving it.

### Interface, temporary ownership and command-specific preparation

A focused CLI-library-internal module (proposed `cli/src/staging.rs`) exposes two crate-visible concrete functions. Illustrative signatures, not a compiled promise:

```rust
pub(crate) fn validate_npc_decision(
    pack: &CheckedPackPaths<'_>, target: &NpcTarget,
    question: &str, candidate: &DecisionConfig,
) -> Result<(), Vec<String>>;
pub(crate) fn validate_scene(
    pack: &CheckedPackPaths<'_>, target: &SceneTarget, edited: &str,
) -> Result<(), Vec<String>>;
```

**Target-confinement composition refinement (2026-10-14):** these illustrative parameters replace §9's original raw `Pack`/id parameters with [§10's checked inputs](#10-authoring-target-confinement--decided-design-2026-10-14). Target/key/membership refusals occur before attempts; only existing loader/ordinary-I/O corrections reach these vector-returning validators. The raw public Scene helper remains a diagnostic compatibility adapter, not the shipping corrections route. This changes neither the selected two-validator interface shape nor temporary ownership.

- One private owner holds a fresh real `TempDir`, the staged root and all path/write/reload knowledge. A call transplants, prepares one candidate, invokes the real engine loader and releases its temporary owner on every return path. No reuse across attempts or feedback, arbitrary destination/root parameter, caller callback, public staged capability, filesystem/loader trait or second validator. These local-substitutable dependencies are exercised with real temporary directories and the real loader.
- `transplant_pack` moves behind this owner; it is not an externally callable arbitrary-destination staging interface. The optional-manifest/fidelity witnesses move with their distinct real-writer/loader guarantees. Existing public `gen::validate_edited_scene` remains a narrow compatibility adapter; the new module does not require a public export. Unit witnesses cross the concrete internal validators, and external attempts/commands still exercise the real composition.
- A narrow private NPC handler retains membership-based create/replace, the staged create head (`id` as `name`, `model: None`), staged read/parse, insertion under `question` and compact whole-NPC serialization (`gen.rs:433–455`). Keep that staged read-back rather than switching to `pack.npcs` as its content snapshot. Generated optional identity stays in existing extraction/final-write handling (`gen.rs:352–372`, `main.rs:703–738`); staging does not add it to the head. No direct-construction shortcut silently removes failure behavior.
- NPC JSON/schema/identity/content rules and the outer diagnostic filter remain command-specific. Prose layer 1, say-line rendering and `splice_run` stay in its complete attempt; it supplies the fully edited Scene. The final atomic/comment-preserving source writers remain outside staging (`cli/src/splice.rs:36–105`), as do preview, accept/discard and dry-run handling. No broad `gen.rs` partition or model-generation framework.
- Temporary ownership means ordinary `TempDir` best-effort cleanup, not guaranteed deletion after a crash or OS refusal. No new cleanup-error reporting or retry policy. Exact Rust visibility/borrowing/private fixture signatures must compile at implementation time.

### Diagnostics and behavior freeze

Engine `LoadError` owns pack-relative `file`, optional 1-based `line` and useful `message`; `Display` renders `file[:line]: message` (`engine/src/error.rs:9–35`, Scene/NPC construction in `engine/src/pack.rs:264–344`). **Q3 selects verbatim engine Display for both validators.** Remove the NPC replacement of the absolute staged NPC path anywhere in every rendered loader string (`gen.rs:462–475`); Scene already displays verbatim. This is redundant for file locations, but deliberately stops modifying arbitrary matching message substrings: do not claim all possible message bytes remain identical or that such a substring can never occur. No symmetric scrub, new column/diagnostic type, deduplication, ranking or reordering. Warnings remain non-rejections; underlying OS/parser text gains no new byte-stability guarantee.

Staging's NPC validator returns a typed outcome — the engine's loader **content** diagnostics, or a terminal **preparation failure** (settled 2026-10-16) — while the Scene validator keeps its existing correction strings; staging returns no attempt reports. Temporary creation/transplant/serialization/staged read-parse/write failures on the NPC path are that terminal preparation failure, never a correction. It owns no provider/prompt/raw-reply/capability/session/corrections/count/retry state. Both complete command attempts call it inside the real session/driver composition ([drafting §10](author-cli-drafting.md#10-attempt-seam-and-retry-ownership--decided-design-2026-10-14)); that driver remains the sole three-attempt policy owner. Preserve streams/status/prompts/exhaustion and error duplicates except the narrowly intentional diagnostic choice above.

### Safety prerequisite and distinct residuals

- **Target confinement — prerequisite repair, not already guaranteed.** NPC staging joins unchecked `npc_id` (`gen.rs:430–433`); the final writer also joins `npc.id` (`splice.rs:40`). A closed file-kind interface alone cannot confine raw string ids. Shipping prose checks loaded Scene membership (`main.rs:274–278`), but the public Scene helper itself takes unchecked ids. Q4/Q8 authorize a separate human-approved target-safety decision to settle consistent CLI/staging/persisted target rules and witnesses. Its actual repair establishes the confinement baseline before extraction may claim validation writes cannot reach authored paths; no new target policy is silently imposed here. Its checks/refusal timing/platform/lexical-only scope were settled in [§10](#10-authoring-target-confinement--decided-design-2026-10-14) and delivered 2026-10-16.
- **NPC candidate-bypass residual — decided and delivered 2026-10-16.** The staged read/parse used `unwrap_or_default()`/`.ok()` and the outer filter drops strings not matching NPC/question substrings, so a preparation failure could accept a candidate without installing or validating it. That decision reverified the composition at the then-current `HEAD` (the `transplant_pack(...).map_err(...)` call at `gen.rs:527`, whose source reads are `:595`/`:601`; the `unwrap_or_default()`/`.ok()` swallow at `:541–542`; the content-substring filter at `:504–507`) and ran a real temporary-pack/real-loader/complete-attempt witness: an intact pack rejects an undeclared-`state_keys` candidate with `AttemptsExhausted` after three calls, while deleting a source file after the pack load accepts the **same** invalid candidate after one call (and a valid candidate unvalidated), with authored bytes unchanged. Human-confirmed policy (interview, 2026-10-16): installation in a real staged pack plus real-loader validation is a precondition of `Accepted` (the candidate is installed, the real loader runs, and no retained candidate-relevant rejection remains — not whole-pack validity, since the intentional filtering of unrelated pack diagnostics is preserved); a preparation failure is a **terminal infrastructure error** (no retry, no correction, no attempt-budget consumption; stderr `✖ {err}`, exit 1; the interactive `-i` session ends without a feedback prompt); its diagnostic is the verbatim underlying message plus explicit "candidate not installed or validated" context, and the outer substring filter applies only to loader **content** diagnostics, never to a preparation failure. The delivered repair supersedes this section's NPC-specific "retain existing preparation/classification" wording; the general Scene-path classification below is untouched. Independence: not a hidden T repair and no new V prerequisite; if this repair lands before V's staged-lifecycle extraction, V must preserve the new baseline rather than recreate the bypass. **Delivered (2026-10-16).** `validate_npc_decision` now returns a typed `NpcValidationFailure` (`Content` loader diagnostics or a terminal `Preparation` message): the staged read/parse and the transplant/write steps surface a failure instead of defaulting, and the complete NPC attempt propagates it through the shared driver as the terminal `GenerationError::Preparation` — no retry, no correction, no attempt-budget consumption, no interactive feedback prompt. `Accepted` structurally implies the candidate was installed and loader-validated, while the outer substring filter still sees loader content diagnostics only.
- **Staging I/O-as-corrections residual — separate policy repair, still open.** Keep the attempt-retry record's existing classification for the Scene/drafting path, including its budget consumption where errors reach corrections. The candidate-bypass decision deliberately did not decide or generalize this policy: the NPC repair's terminal classification is scoped to NPC staged preparation and does not endorse the Scene-path behavior as permanent. Ownership/pointer remains [drafting §10, Frozen behavior, composition and residuals](author-cli-drafting.md#frozen-behavior-composition-and-residuals). [Drafting §10](author-cli-drafting.md#10-attempt-seam-and-retry-ownership--decided-design-2026-10-14) is the design and implementation record for that classification; neither its closure nor this handoff schedules or repairs it.
- **Pack RON bridge composition:** transplant uses the engine-owned typed state serializer once it lands; semantic state/manifest fidelity, quoted/escaped string/key text, no source `state.ron`/manifest rewrite, one shared actual CLI-writer → engine-loader full-state witness. No duplicate escaping fix, normalization, numeric/key restrictions or whole-pack validation rules.
- **Handoff sequencing:** the staged-validation decision left target confinement open for a separate interview; that decision subsequently settled the policy in §10 and the repair was implemented 2026-10-16. The extraction handoff carried native blocked-by edges to the actual [pack RON bridge](content-format.md#pack-ron-bridge--decided-seam-2026-10-13), [attempt/retry](author-cli-drafting.md#10-attempt-seam-and-retry-ownership--decided-design-2026-10-14) and [target-safety](#10-authoring-target-confinement--decided-design-2026-10-14) implementations, and was delivered 2026-10-16 once all three landed. A closed design record is neither a fake implementation blocker nor evidence of delivery. Candidate-bypass and general I/O-policy repairs remain explicitly separate.

### Required implementation validation — design requirements (delivered)

The witnesses below were implemented in `cli/src/staging.rs`'s test module (unit tests over the production cores, with the module-private real-TempDir-under-test-parent seam) and by the retained `gen_test`/`splice_test`/`cli_test` real attempt and command witnesses; the merge-gate commands and their observed results are recorded with the 2026-10-16 delivery.

1. Both concrete validators use the real temporary pack/loader. Valid NPC/Scene candidates pass; independently invalid candidates produce appropriate engine pack-relative file/message and optional line. Assert a known Scene source line exactly and NPC absence of a line where the engine has none. A deterministic private fixture with a would-be staged-path substring protects verbatim message display, not a blanket no-absolute-text assertion.
2. A narrow module-private construction seam allocates a real `TempDir` under a test-owned parent; production still creates its normal directory. Run the **same validation core**, then inspect that parent after success, loader rejection and portable transplant-read failure (remove a source file after loading `Pack`, before capturing the failure fixture's baseline). Assert protected authored Scene/NPC/state/manifest/assets/other bytes or intentional absence unchanged from immediately before validation. No public root, fake loader/filesystem, global-temp enumeration or global environment mutation. Candidate-write/temp-creation/cleanup failures need established portable real-filesystem fixtures; name unsupported fault cases rather than pretend ordinary cleanup proves them.
3. Retain optional-manifest fields, plain optional values and declared order staging → real-loader witnesses (`gen.rs:1099–1190`); once the pack RON bridge lands, reuse its **one** actual full-state writer/loader witness including quoted/backslash strings, escaped key, line break/Unicode and ordinary scalars. Relocate guarantees as needed without duplicate serializer-only tests or deletion of meaningful engine coverage.
4. Preserve real NPC extraction/full gates, prose layer-1/layer-2/splice/outside-run bytes, optional identity/final comment-preserving writer and existing provider/loopback/binary/protected-file witnesses. Include the real attempt/session/driver/runner composition of [drafting §10](author-cli-drafting.md#10-attempt-seam-and-retry-ownership--decided-design-2026-10-14), not a fake driver. Preserve the current filter/preparation/classification and UX baseline unless separately repaired; helper test migration cannot erase those distinct protections. Existing real Scene witness is `cli/tests/gen_test.rs:686–707`; typed NPC loader rejection is `:458–490`.
5. Choose focused checks as [the testing policy](testing.md) describes: smallest applicable CLI target, then `cargo test --locked -p unmei`; before merge `cargo fmt --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace`, `python3 scripts/check-invariants.py`. Until implementation, only source/document/link/scope checks and `git diff --check` validate this plan. No Cargo/provider/browser/runtime result is claimed here.

No ADR is warranted for this small reversible seam. Content pack, NPC, Scene, Run and State map keep their existing glossary meanings; lifecycle/validator/adapter mechanics introduce no project-specific domain term.

## 10. Authoring-target confinement — decided design (2026-10-14)

**Decision:** settled 2026-10-14 through Q1–Q9 and the complete-design/publication confirmation Q10 at interview. This section is the normative target policy; §9 and the drafting contract link to it rather than owning another policy.

**Delivered (2026-10-16).** `cli/src/target.rs` owns the pure per-platform filename policy and the opaque `NpcTarget`/`SceneTarget` values, `CheckedPack` checks every supplied Scene/NPC key, and both shipping commands, the generation/drafting library entries and the defensive NPC writer consume checked inputs with terminal `TargetError`s. Native filesystem/loader and spawned-binary witnesses ran green on Windows at the delivery revision; the invariant checks and the merge gate own the rest of the record, and native Linux acceptance was completed afterward (the *Native filename verification* note below). No extraction of the staging lifecycle happened here.

**Native filename verification (2026-10-17).** The follow-up audit re-verified the target repair on a Windows host only and recorded native Linux acceptance as BLOCKED: no merge-gate run was recorded for that revision, and the delivery note had cited the `ubuntu-latest` merge gate without a preserved, revision-bound record. An earlier pushed revision did execute the `cfg(unix)` witness on `ubuntu-latest` (`apply_candidate_accepts_backslash_and_colon_ids_on_native_linux ... ok`), so the witness was not never-run — but that log was never cited as evidence, the witness asserted only the file's existence and loader acceptance, and no native Scene-writer witness existed. This verification supplies the missing revision-bound record at the delivery revision, whose native Linux run is a headless CI job (Ubuntu 24.04.5, x86_64 Linux, ext4, at the delivery commit plus the verification branch's evidence job): the real NPC writer and the real drafting runner author the exact `npcs/back\slash.ron`, `npcs/co:lon.ron`, `scenes/back\slash.unmei` and `scenes/co:lon.unmei`; the engine loader round-trips those stems to the ids, the intended edit bytes are the only change and the remaining pack bytes are identical. The paired native Windows witnesses at the same revision accept an ordinary-but-unusual stem and refuse those two constructions before any provider call or write. The two-dialect policy matrix is necessary but is **not** native acceptance, and the Linux half is a headless CI host, which bounds nothing here beyond rendering. No policy, loader grammar or writer behaviour changed.

### Source-backed baseline

The inspected baseline `HEAD`: `gen.rs:234–261` starts NPC provider/retry work without target preflight; `:407–410` filters staging error strings; `:423–477` stages the raw NPC id; `:481–527` copies every raw Pack file key; `:1005–1049` runs prose retries; `:1070–1080` is the unchecked public Scene helper. `main.rs:251–342` requires a loaded Scene for shipping draft/list and constructs the source path; `:611–744` sets up NPC generation before final create/replace writing. `splice.rs:36–72` joins the supplied `Npc.id`; `:83–95` is the generic atomic writer. `engine/src/content.rs:632–643` exposes mutable Pack maps; `engine/src/pack.rs:259–338` derives loaded filename stems and requires NPC identity equality. These are source reads, not executed defect witnesses. Paths without an explicit prefix here are under `cli/src/`.

### Policy and checked interface

- **Q1 — Native-platform filenames, exact spelling.** Confine targets using the current platform's filename/path semantics, not a new portable or snake_case NPC/Scene naming language. Preserve legitimate stems exactly; never trim, sanitize, normalize or rename an id. Acceptance is OS-relative: a Linux-only filename need not be authorable on Windows. This CLI policy does not tighten the engine loader's grammar.
- **Q2 — Lexical direct-child confinement only.** The repair covers target construction in an ordinary author-controlled pack, not filesystem-link/reparse-point redirection, concurrent replacement/writers, multi-user security, crash durability or guaranteed temporary cleanup. A lexically confined path is not a claim that its resolved filesystem object lies beneath the pack. Canonicalization alone would not supply that stronger guarantee, particularly for create targets.

- **Q3 — Complete-filename predicate, not an extra stem grammar.** A non-empty id must produce one ordinary native filename component with the fixed `.ron`/`.unmei` extension and round-trip to the exact input as its filename stem. Inspect raw native separators/prefixes; component normalization alone is insufficient. Windows stream/device/reserved-character forms refuse. Dot-only, hidden, Unicode, spaces, embedded dots and trailing-dot/space stems are not automatically unsafe when the appended extension makes an ordinary exact-round-tripping filename. Length/resource/permission failures remain ordinary I/O.
- **Q4 — Every copied key is checked.** Preflight all supplied `Pack.scenes`/`Pack.npcs` keys before staging/source-path I/O, including manually assembled or mutated Packs. Loader provenance is not an unchecked-key escape hatch. This is filename confinement, not new whole-pack semantic or in-memory identity validation.

- **Q5 — Opaque checked targets.** Select one small CLI target module with concrete NPC/Scene constructors, immutable private filename/kind and fixed subdirectory/extension path construction. Shipping commands and defensive library/writer adapters use the same constructors; authoring joins consume checked values, including transplant keys. Expose only the narrow surface the separate binary needs and retain raw-input compatibility adapters. Reject shared path-returning functions (weaker checked-value flow) and entry guards with unchecked joins (caller convention). Generic `write_atomic` remains an arbitrary-path primitive, not a sandbox.
- **Q6 — Terminal preflight.** After Clap/basic mode syntax, check raw ids before pack/brief/provider configuration. Check supplied-Pack keys before source reads, temporary allocation, prompts or calls. Target refusals are typed terminal errors for generation/drafting/writing, never NPC-filtered corrections, retries, exhaustion or interactive feedback. Preserve draft-session state on preflight refusal. CLI uses its existing outer stderr failure/exit-1 route, with no stdout preview/success, feedback prompt or write. Report kind, escaped offending id and useful reason; no credential or absolute staged-path context. The public Scene helper retains its string-vector result as a compatibility adapter; shipping attempts preflight outside that adapter. This intentionally changes target-error timing/output only; ordinary staging I/O classification stays unchanged.
- **Q7 — Existing Scenes throughout.** All drafting/Scene-validation entries require exact `pack.scenes` membership, including the public Scene helper: a safe absent id refuses instead of adding a staged Scene. This deliberately tightens that helper to match v1 drafting. NPC staging membership still selects create/replace; safe-target final NPC CreateMode/existence behavior and timing are unchanged. Question keys and generated identity equality retain their separate content rules.

- **Q8 — Preflight plus checked inputs, not a new retry classification.** Raw public generation/drafting APIs construct checked targets and a private immutable checked inventory of every Pack file key before their loops. The inventory borrows the Pack immutably and is consumed by the staging owner's transplant (`cli/src/staging.rs`) for both source reads and staged writes; preflight followed by raw-key joins is insufficient. One shared `TargetError` has terminal variants in `GenerationError`, `DraftError` and `SpliceError`. Internal concrete validators consume checked target/inventory inputs and retain `Result<(), Vec<String>>` for loader/ordinary-I/O corrections. This refines §9's illustrative parameters, not its two-validator/private-lifetime ownership. No target error enters §10 of the drafting contract's retry driver. The raw public Scene helper retains its signature/vector result as a diagnostic compatibility adapter (one target/membership refusal diagnostic); shipping uses the checked internal path, not that adapter. No staged root is exposed. Ordering is basic syntax → raw filename check → resolve/load pack → copied-key preflight and Scene membership → existing brief/provider/session work → generation. `--list`, `--dry-run`, interactive and direct library calls obey the policy. Pack/source roots remain caller-selected, not sandboxed.
- **Q9 — Future proof across both native platforms.** Require a compact pure-policy matrix with explicit Windows/Linux selection plus real native filesystem/loader evidence on both OSes. Exercise accepted legitimate stems and unsafe raw targets/copied keys through real validators/helper/NPC create/replace writer; protect pack bytes, outside sentinel and forbidden/temp-file absence. Direct generation/drafting witnesses must return terminal Target (not Accepted/exhaustion), make no provider call or staging artefact and preserve an existing draft session. Spawned-binary witnesses establish configuration-preempting refusal, exit 1, escaped contextual stderr and no stdout/preview/feedback/write; a configured loopback endpoint receives no request. Reuse existing gen/splice/CLI fixtures and meaningful gates, not a fake filesystem/loader or duplicate matrix at every layer. Safe-target success output remains unchanged, including existing raw-id echo: this is not a printing-hygiene repair. Missing native-platform evidence leaves delivery unverified. Focused CLI targets and `cargo test --locked -p unmei`, then the workspace gate before merge, are future requirements, not results today.

### Predicate detail and refusal scope

The pure filename policy takes id, fixed file kind and explicit Windows/Linux platform; production constructors select the native platform. Validate the **complete** filename, inspecting raw separators/prefixes so normalized `components()` cannot hide `a/./b`. Require one exact ordinary `Normal` filename component and `path.file_stem().and_then(|s| s.to_str()) == Some(id)` using the loader's stem rule. Empty ids and NUL refuse. Linux `/` refuses; ordinary `\`/`:` remain permissible. Windows `/`/`\`, prefixes/root/drive/UNC/device paths and `:` (including alternate streams) refuse, as do the native reserved filename characters `< > " | ? *` and controls 1–31. Detect reserved device basenames case-insensitively, including extensions and the documented COM/LPT superscript-digit forms; the fixed extension does not turn `NUL` into a regular file. No arbitrary maximum length, case folding, whitespace trimming or Unicode normalization is imposed. Native rules must be verified during implementation, not inferred from a Linux-only matrix.

A dot-only id is intentionally not treated as a raw path: `..` produces `...ron`/`...unmei`, not a parent component. `.hidden`, `act.one`, Unicode, spaces, and `x.`/`x ` stems remain eligible when their completed filename round-trips exactly and is an ordinary native file; appending the extension means the completed name does not end in a dot/space. An unsafe raw filename refuses before pack discovery/loading, brief-file access or provider configuration; loaded-key/membership checks necessarily follow loading and precede any **target-derived** source read/prompt/temporary staging. Clap/basic mode-syntax precedence remains unchanged. Direct supplied-Pack APIs perform the same preflight without reloading the Pack.

One shared target error supplies NPC/Scene kind, quoted/escaped id and useful reason (including absent Scene membership); library generation/drafting/writer wrappers keep their normal contextual error rendering. The binary uses its existing outer `✖ {err}` stderr/exit-1 route. Refusal must not allocate a staged owner, create a writer's sibling temporary, print preview/success, prompt for feedback or alter draft capability/corrections/remembered-draft state. Error text is contextual and escaped, not a new byte-stability promise for OS/parser diagnostics. Safe-target output stays frozen, including raw-id echoes; printing hygiene is not silently added. Existing safe NPC missing/existing/case-collision outcomes and filesystem existence semantics are not redesigned. `Pack.root`/writer root remain caller-selected source/destination roots, not claims about root provenance or sandboxing.

### Required implementation witnesses

Delivered 2026-10-16; the merge gate below owns the exact commands, and the native Windows and Linux runs are recorded in the 2026-10-17 native filename verification above.

1. Compact pure-policy matrix covers NPC/Scene fixed extensions, empty/NUL, separators/normalized-dot traps/parent-bearing/absolute inputs, Windows relative-drive/absolute-drive/UNC/device/ADS/reserved characters and case-insensitive device basenames (including superscript digits), and exact stem round-trip. Protect the deliberate dot-only/hidden/dotted/Unicode/space and trailing-dot/space-stem acceptance. Linux-only ordinary backslash/colon cases stay native-only positives. Matrix green does not replace native evidence on **both Windows and Linux**.
2. Real temporary source packs and real loader through checked validators, raw public Scene adapter and NPC create/replace writer prove accepted targets write only intended bytes, keeping NPC comments and prose outside-run bytes. Refusal of unsafe raw targets/copied keys and safe-but-absent Scenes leaves protected manifest/state/Scene/NPC/assets/other bytes or absence and an **outside sentinel** unchanged; forbidden paths and sibling temporaries stay absent. A narrow private real-temp-parent fixture seam can support observable staging/cleanup without a public staged root, fake filesystem/loader or global-temp scanning. This repair may add the seam in place; §9 later moves it with the lifetime owner. Root links/races/crash cleanup are not proven.
3. Cross actual `generate_once` and `draft_once`/`draft_with_session` composition with test-local `AuthorChat`, not a fake attempt/driver: unsafe target/key/membership refuses as typed Target, never Accepted or AttemptsExhausted, before calls or staging and without changing a supplied draft session. Include a refusal whose wording would be lost by the old NPC substring filter and prove it never enters that filter. Checked-key consumption must make source/staging escape joins unavailable; no preflight followed by raw transplant keys. The public Scene compatibility vector is diagnostics, not a universal retry classification; shipping calls the checked internal path.
4. Spawned CLI witnesses cover NPC create/replace and representative draft list/dry-run/interactive routes: target refusal preempts missing model/credentials/brief-file/pack configuration, exits 1 with contextual escaped stderr and no stdout/preview/feedback/write. A separately configured loopback provider receives no request. Preserve safe-target output and existing content/provider/exhaustion/no-write behavior; mock-call counts alone are not the refusal witness.
5. Reuse `cli/tests/gen_test.rs`'s `FakeChat`/`StructuredFake`, `validate_edited_scene_accepts_good_and_reports_loader_errors` and session-state witnesses; `cli/tests/splice_test.rs`'s `temp_pack`, `replace_preserves_comments` and named-question preservation; and `cli/tests/cli_test.rs`'s real-temp/loopback `Fixture`, create/refusal and dry-run/outside-run witnesses. Keep the meaningful loader/serializer/provider/binary gates. Do not repeat every policy case at every layer or fold the NPC preparation/filter defect into target tests.
6. Use [the testing policy](testing.md): applicable `cargo test --locked -p unmei --test gen_test`, `--test splice_test`, `--test cli_test`, then `cargo test --locked -p unmei`; before merge `cargo fmt --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace`, `python3 scripts/check-invariants.py`. Missing native-platform evidence leaves delivery unverified.

### Composition, delivery and residuals

The target repair was delivered 2026-10-16 without resolving the target-confinement handoff bookkeeping here, and without extracting staging. The staged-extraction handoff still wires to the pack RON bridge and attempt/retry implementation handoffs and this repair with native blocked-by edges. A closed design record is not an implementation dependency or proof of delivery. The [pack RON bridge](content-format.md#pack-ron-bridge--decided-seam-2026-10-13) owns state serialization; [drafting §10](author-cli-drafting.md#10-attempt-seam-and-retry-ownership--decided-design-2026-10-14) owns retries; [§9](#9-staged-pack-validation--decided-design-2026-10-14) owns the later temporary lifecycle extraction and verbatim engine diagnostics. The checked-input refinement is explicit, not permission to extract staging during the repair.

NPC staged preparation/filter candidate bypass and general staging I/O-as-corrections stay separately scoped as in §9/drafting §10. Neither is repaired by this target-confinement record and no extra extraction prerequisite is added for them. The candidate-bypass repair was delivered 2026-10-16 (installation + real-loader validation as an `Accepted` precondition); the general Scene-path I/O classification remains deferred to [drafting §10](author-cli-drafting.md#10-attempt-seam-and-retry-ownership--decided-design-2026-10-14). Filename rules do not promise symlink/reparse/concurrent-writer/multi-user/crash security, guaranteed TempDir cleanup or generic atomic-writer confinement.

No ADR is warranted for this small reversible CLI repair; existing NPC, Scene, Run and Content pack meanings remain unchanged. Checked targets/inventories are implementation vocabulary, not new glossary concepts.

**Primary path-semantic references:** [Rust `Path`](https://doc.rust-lang.org/std/path/struct.Path.html) documents native separators, prefixes/relative-drive handling, component normalization and canonicalization; [Microsoft filename conventions](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file) documents reserved characters/device basenames and extensions. These ground the policy; the pinned toolchain/native behavior is verified by the implementation's real filesystem/loader and spawned-binary witnesses on each platform.
