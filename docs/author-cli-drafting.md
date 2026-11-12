---
title: Authoring CLI contract for drafting dialogue prose
summary: The drafting contract for one say-line run in an existing scene — generation unit, two-layer validation gate, provider fallback and regeneration policy.
keywords:
  - authoring-cli
  - dialogue-dsl
  - pack-validation
  - content-pack
---
# `unmei author draft` — LLM dialogue drafting contract

**Status:** settled by the design interview (rounds 1–5 plus confirmation) · **Scope:** v1 — the first drafting cut.

**Built (2026-10-14).** Sections §1–§9 are implemented: run addressing and `--list`, the one-shot draft, structured output with its retry and free-text fallback and the interactive session all shipped. §10's attempt seam was implemented 2026-10-15; its own status marker and the residuals below remain binding.

This is the written contract for drafting `.unmei` prose with the authoring model: what `unmei author draft` accepts, generates, validates, and writes. It lifts the [§8 non-goal](author-cli.md#8-non-goals-this-slice) of [`docs/author-cli.md`](author-cli.md) — *drafting dialogue or scene text with the LLM* — and its drafting rules follow the research on LLM drafting of Unmei prose, with the demand case in [`examples/demo-pack/`](../examples/demo-pack/). It shares the provider, retry, and preview/write core with the rest of the author CLI; **this is a contract, not an implementation** — the builds implemented against it.

## 1. Command surface

One-shot and interactive modes over the **same** generate → validate → preview → write core as [`author-cli.md`](author-cli.md) §1/§5.

```text
unmei author draft <SCENE_ID> --from <ANCHOR> [OPTIONS]
unmei author draft <SCENE_ID> --list

Draft one run of prose in scenes/<SCENE_ID>.unmei using an OpenAI-compatible
chat endpoint. Say-lines only — never control flow, never commands.
Existing scenes only: the anchors are authored by hand, and drafting a scene
outside the pack (or a new scene) is out of v1's reach.

Options:
  --from <ANCHOR>      The run to draft, as shown by --list: `start`, a label
                       name, or that named anchor with a `#n` ordinal (see §2)
  --list               Print every run in the scene and exit; drafts nothing
  --brief <TEXT>       Author intent for this run; optional
  --brief-file <PATH>  Read the brief from a file (for long notes)
  -i, --interactive    Guided session for one run (see §5)
  --pack <DIR>         Pack root (default: cwd, must contain pack.ron)
  --model <MODEL>      Authoring chat model (required unless set in config)
  --base-url <URL>     OpenAI-compatible endpoint (default: https://openrouter.ai/api)
  --dry-run            Print the drafted run; write nothing
  -h, --help
```

- `--from` is **required** unless `--list` is given. The scene must already exist and parse; the anchors it names are authored by hand (§2).
- `--list` prints each run's handle, its line span, and its first say-line, in scene order — the source of truth for `--from`:

  ```console
  $ unmei author draft first_meeting --list
    start       L7     Narrator: Morning light. A folded letter on the ground, half under a shoe.
    start#2     L10    Ace: You're not from around here, are you?
    ask_letter  L21    Ace: A letter? Ah... you saw that.
    silence     L28    Ace: ...Fine, don't talk. See if I care.
  ```

  `--list` needs no API key, makes no network call and writes nothing: it loads the pack, checks it
  and reads the scene.

- `--brief` / `--brief-file`, `--pack`, `--model`, `--base-url`, and the key handling are exactly [`author-cli.md`](author-cli.md) §1/§4.
- `--dry-run` prints the would-be run and writes nothing — no write-side side effects.

## 2. Generation unit and write surface

**A run is the unit.** A run is a *maximal contiguous block of say-lines*: the first say-line after a non-say-line line (or the scene start) through the last say-line before the next non-say-line line (or the scene end). Blank and comment lines *between* two say-lines sit inside the run; any other line ends it.

- **What bounds a run:** every non-say-line line — any command (`bg`, `sprite`, `hide`, `play`, `stop`, `set`), any flow line (`label`, `jump`, `choice`/`end`, `decide`, `if`/`else`/`endif`) — and the scene start/end. Commands therefore *split* a passage into several runs, and are never inside one.
- **Addressing (`--from`):** a run's handle is its **enclosing named anchor** — the innermost construct that contains it and carries a name: `start` (the scene top level) or a `label` — plus an ordinal (`#2` for the second run under that anchor) when several runs fall under it. Anonymous constructs add no handle: a run inside an `if`/`else` arm, or one after an `endif`, belongs to the nearest enclosing `label` (or `start`), and the ordinal counts runs in scene order within that anchor. (`choice`/`decide` never enclose a run — `choice` bodies hold only options, `decide` is one line — so the only anonymous container is `if`/`else`/`endif`.) `first_meeting`'s two pre-`choice` lines are `start` and `start#2` because `sprite ace neutral` splits them; `reunion`'s branches are `reply.warm`, `reply.cold`, `reply.none`, each a `label` directly enclosing one run.
- **One invocation drafts exactly one run** — the say-lines of that run, and nothing else.

**v1 emits prose lines only.** Every generated line must be a say-line (`Speaker: text`). The CLI **never** generates:

- control flow: `if` / `else` / `endif`, `choice` / `end`, `label`, `jump`, `decide`;
- commands: `bg`, `sprite`, `hide`, `play`, `stop`, `set`.

**Label structure is author-provided, never model-proposed.** The devising of branches, labels, and decision points is the author's hand-authored skeleton; the model only fills prose into the run the author names. This is the boundary that keeps the drafting contract from reopening the conditional grammar ([ADR 0004](adr/0004-conditionals-in-the-dialogue-dsl.md)) or the decision-config schema (see [`author-cli.md`](author-cli.md) §2).

**Write surface:** a successful draft writes **in place** to `scenes/<SCENE_ID>.unmei`, replacing only the run region — its say-lines plus any interstitial blank or comment lines (§6) — while every line outside it survives byte-for-byte; it never touches `state.ron`, `pack.ron`, `npcs/`, or `assets/`, and never creates a scene. The write is atomic (temp file + rename), as the RON splice is — a validated draft must land whole.

## 3. Context assembly (what the model sees)

Context is assembled **automatically**, mirroring [`author-cli.md`](author-cli.md) §6; the author never wires it by hand:

- the **whole target scene text** — every run and every command, so the model sees the flow it is writing into;
- every `npcs/*.ron` in the pack — because any declared NPC id is an allowed speaker (§7), the cast vocabulary is pack-wide;
- `pack.glossary.md` (pack-local terms), when present;
- the `state.ron` key vocabulary, with its current values and bucketed-string guidance (decision models judge, they don't compute);
- the author's `--brief` / `--brief-file`, when given (**optional** — the CLI warns when there is neither a brief *nor* a `pack.glossary.md`).

**Prior scenes are deliberately out of scope for v1.** The target scene plus glossary and brief carry scene-local coherence; cross-scene "story so far" is a cost/context escalation that waits on demand (and only pays for itself once a pack clears the provider's prompt-cache minimum). The **prompt-engineering wording** (system prompt, schema instruction phrasing, retry-correction composition) is the build's experimentation space, exactly as §6 says for configs.

## 4. Validation gate (before any write)

Every candidate run must pass a **two-layer** gate. The CLI reuses the loader's validation logic; this list is the contract, not a second implementation.

1. **Layer 1 — generated lines are say-lines.** Every line the model produced must parse as a say-line: a single `: ` separator, non-empty speaker with no `:`, text on one line with no embedded newline. A generated line whose first token is a keyword (`bg`, `sprite`, `hide`, `play`, `stop`, `set`, `label`, `jump`, `choice`, `end`, `decide`, `if`, `else`, `endif`) is a **rejection** — this is the check that enforces prose-only, because the shared loader would otherwise accept such a line as a real command. A draft that yields no say-lines is also a rejection (nothing to write).
2. **Layer 2 — the shared loader.** The edited scene text re-parses under the engine's `.unmei` parser and the pack re-cross-validates, the same posture as [`author-cli.md`](author-cli.md) §3. This is a regression guard: the splice must not have broken the surrounding scene, and the pack's existing rules must still hold.

**Validation staging amendment (2026-10-13):** Layer 2 uses the same staging transplant as decision-config generation (`cli/src/staging.rs`). The decided [pack RON bridge](content-format.md#pack-ron-bridge--decided-seam-2026-10-13) requires that transplant to serialize temporary state through the engine, preserving string/key text with RON quoting and escaping instead of hand-rendering; it has landed. It changes only staging bytes, never the authored `state.ron` or §2's write surface; diagnostics and retry policy below stay unchanged. The quote/backslash CLI-writer → engine-loader regression is shared across both staging consumers, not two copies of the same test.

**Diagnostics** are the loader's existing `LoadError` — pack-relative file, 1-based line, message — fed back as corrections. There is **no column** and no change to the loader; the drafting layer adds no new diagnostic type. (The drafting research pointed at Yarn Spinner's file+line+column `Diagnostic`; file+line is what the shipped engine carries, and a column is a possible later refinement, not part of this contract.)

**Retry:** validation failure runs [`author-cli.md`](author-cli.md) §5's loop — up to **3 attempts**, feeding the errors back as corrections. Provider errors (an unreachable endpoint, or an unsupported `response_format`) surface immediately rather than being retried as content problems (see §7).

**Capability-error clarification (2026-10-14):** §7's automatic free-text fallback is the exception to the preceding sentence's unsupported-`response_format` wording and supersedes that wording. `NoStructuredSupport` triggers capability negotiation inside the same attempt, not a content correction or an extra validation attempt; every other provider error terminates immediately. This reconciles the existing contract with the shipped behavior, not a new fallback policy. The [decided attempt seam](#10-attempt-seam-and-retry-ownership--decided-design-2026-10-14) preserves this distinction.

## 5. Failure, retry, and editing UX

**One-shot:** generate → gate → on success, preview the drafted run, then write. On failure after 3 attempts: print the best raw model output plus all validation errors, **write nothing**, exit with code 1.

**Interactive (`-i`):** a guided loop over the same core, **one run per session** — collect or edit the brief, generate, preview, take free-text feedback ("make Ace more guarded", "shorter, three lines") and regenerate, repeat until `accept` (writes, passing the same §4 gate) or `discard` (writes nothing). Run the command again — `--list` then `--from` — for the next run.

**`--dry-run`:** print the would-be run, write nothing, no write-side side effects.

**Undo:** git is the undo button ([`author-cli.md`](author-cli.md) §5). The CLI keeps no backups and offers no in-CLI history.

## 6. Regeneration policy

**The run is the regeneration unit.** Re-drafting a run replaces its say-lines **wholesale**:

- every line outside the run — control flow, other runs, commands, and the `#` comments above and below it — survives **byte-for-byte**;
- hand edits and `#` comments **inside** the run are part of the regenerated unit and are replaced.

This is the RON comment-preservation lesson ([`author-cli.md`](author-cli.md) §2, `cli/src/splice.rs`) ported to `.unmei`: `author npc --question` rewrites only the named config block, so comments everywhere else survive; `author draft --from` rewrites only the named run, so everything around it survives. It is also why the run is a *maximal say-line block* (§2): the splice is then a pure line-range replacement, and a command inside a passage can never be at risk. Git is the undo for anything the run took with it.

## 7. Provider and model output

**Provider: [`author-cli.md`](author-cli.md) §4, unchanged.** Drafting reads the same `unmei.author.ron`, the same `--model`/`--base-url` flags, and the same `UNMEI_API_KEY` → `OPENROUTER_API_KEY` precedence; there is **no separate drafting-model setting**, and still no built-in default model. One authoring model serves both `author npc` and `author draft`, chosen per invocation by `--model`. (The runtime's pinned per-NPC decision model stays deliberately separate — §4.)

**Output shape.** The CLI asks for JSON-schema structured output: one object of say-line items, rendered to `.unmei` text by the CLI —

```json
{ "LINES": [ { "speaker": "Ace", "text": "Who goes there?" } ] }
```

— so newlines, colons, and escaping stay in JSON strings and the model stays out of surface syntax. The CLI re-serialises the items to `Speaker: text` lines. Speakers are validated per the rule below.

**`require_parameters: true`.** Support for `response_format`/`json_schema` is **per endpoint, not per model**, and can change over time; `require_parameters: true` routes a request only to endpoints that can honour it, and an unsupported model returns an actionable error naming the lack of support. *Re-verified live at contract time against the drafting research's snapshot: structured outputs remain per-endpoint, `require_parameters: true` is the documented routing posture, `strict: true` is not a universal guarantee, and Response Healing exists for non-streaming `json_schema` requests.*

**Fallback.** Because §4 pins no default model, the CLI cannot know a model's capability up front. It sends the structured request first; if the endpoint errors for lack of structured-output support, it **falls back automatically** to free-text generation + parse/gate/retry — the layered shape research 0003 §3 recommends. A free-text fallback is not a licence for control flow: the §4 gate applies identically. The degradation report names the provider's **fixed category** (rejected `response_format`, no supporting endpoint, or an unknown `provider` request argument) rather than echoing the refused response body (§4, [ADR 0014](adr/0014-cli-diagnostics-and-terminal-credential-safety.md)).

**Speaker latitude.** A drafted line's speaker must be `Narrator`, a speaker already used in the target scene, or an id with an `npcs/<id>.ron`. Any other name is a §4 rejection — `Narrator` is the canonical narration speaker (`content-format.md` §3), and the restriction keeps `Ace`/`ACE`/`Ace ` from becoming three speakers whose sprite references silently stop matching.

## 8. Worked example

```console
$ cd my-pack
$ export UNMEI_API_KEY=sk-...
$ unmei author draft first_meeting --from start#2 \
    --brief "Ace is guarded; the player is a stranger at the school gate."
✔ Generated and validated: first_meeting `start#2` (attempt 1)
  1 say-line · speaker Ace ✓ · no control flow ✓
Wrote scenes/first_meeting.unmei (run replaced; 1 line changed)
```

The model returned:

```json
{ "LINES": [ { "speaker": "Ace", "text": "Who goes there? You don't look like you're from the school." } ] }
```

and `scenes/first_meeting.unmei` changed in exactly one place — the run's say-line, between its untouched `sprite ace neutral` above and `choice` below:

```unmei
sprite ace neutral
Ace: Who goes there? You don't look like you're from the school.

choice
  "Ask about the letter" -> ask_letter
  "Stay silent" -> silence
end
```

Failure path:

```console
$ unmei author draft reunion --from reply.warm --brief "..."
✖ Attempt 3 of 3 failed validation:
  - line 14: generated line `sprite ace happy` is a command, not a say-line
  - line 15: unknown variable `{affinity}` in say-line
Best model output printed above. Nothing was written.
```

## 9. Non-goals (this cut)

- **Control flow of any kind** — `if`/`choice`/`label`/`jump`/`decide` generation, and any command generation. v1 drafts prose lines only (§2). Reopening this reopens the conditional grammar and the decision-config schema.
- **New scenes and hand-authoring by the model** — drafting fills a run in an existing, parsing scene; it never creates a scene or a label.
- **Story-so-far / prior-scene context** — out of v1 (§3); revisit with demand.
- **A separate drafting-model setting** — provider is §4, unchanged (§7).
- **Column-precise diagnostics** — corrections are the loader's file+line `LoadError` (§4).
- **In-editor or runtime generation** — authoring is offline CLI only (standing decision).
- **Multi-run batch generation in one invocation, and interactive multi-run sessions** — one run per invocation, one run per interactive session, by design.

## 10. Attempt seam and retry ownership — decided design (2026-10-14)

**Decision:** settled 2026-10-14 by human confirmation through Q1–Q14 and the complete-design/publication confirmation Q15. **Delivered 2026-10-15:** `cli/src/retry.rs` owns the typed driver and the command-specific complete attempts live in `gen.rs`, with the real runner in `runner.rs`. The three-attempt contract and existing write surfaces remain binding.

### Verified baseline and scope

One design spans two commands. In the inspected baseline, `cli/src/gen.rs:234`'s `generate_once`, `extract_config` (`:314`) and `validate_candidate` (`:423`) generate NPC decision configs; `cli/src/main.rs:765`'s `interactive_loop` holds their feedback/corrections locals. Prose uses `draft_attempt` (`gen.rs:946`), `draft_with_session` (`:1005`) and `draft_session` (`main.rs:365`), which already uses `DraftSession` (`main.rs:412`). This decision covers **both commands**, without putting NPC state into a prose-only session or changing either command's prompts or content rules.

`draft_attempt` currently stops at layer 1; `draft_with_session` splices and performs layer 2 inside its retry loop. `generate_once` has a second automatic retry loop. `print_exhausted` (`main.rs:882`) prints the last raw reply under `--- best model output ---`, followed by `--- validation errors ---` and every accumulated error, to stderr. "Best" does not imply quality ranking. NPC interactive exhaustion (`main.rs:808–824`) instead discards the raw replies and prints its own errors/feedback prompt. These source references describe the pre-extraction baseline, not future locations.

### Attempt interface and ownership

- A small CLI-local `retry` module (proposed `cli/src/retry.rs`) owns the typed attempt report, statically dispatched attempt-adapter seam and fixed policy. Command-specific mechanisms/sessions remain in `gen.rs`; this is not permission for a broad file split, provider-portability abstraction, framework or new dependency.
- An attempt takes the command's immutable generation inputs (pack, target, brief, model and `AuthorChat`), read-only prompt/session context and current corrections, plus narrowly mutable prose capability state. It owns prompt assembly, provider interaction, extraction and **complete validation**, including prose rendering/splice and shared-loader layer 2. It never writes the author's files.
- Illustrative return shape: `Result<AttemptReport<T>, ProviderError>`, with `AttemptReport { raw, outcome: Accepted(T) | Rejected(Vec<String>) }`; the report outcome is a new type, distinct from the existing crate-public `AttemptOutcome`. Prose `T` holds gated say-lines and the fully validated edited scene; NPC `T` holds the validated config and optional identity. `Rejected` means eligible for content retry; `Err` means stop immediately. Raw text on either accepted/rejected candidates is returned unchanged, not interpreted by the driver.
- The existing public `AttemptOutcome::Rejected` has terminal-looking wrapper matches in `main.rs:759,805`, but `generate_once` constructs only accepted success or an error. Do not turn those unreachable shipping branches into a terminal-content route: the new report's rejection lives inside the retry driver. Existing externally visible APIs need not change merely to introduce the internal seam.
- Prose `NoStructuredSupport` is handled **inside the attempt**: flip capability mode/reason in its session and request plain text through the same gate. That mode/reason survives later attempts and feedback regenerations, including when the plain call in the same attempt returns a terminal provider error. NPC generation gains neither capability fallback nor structured requests.
- One automatic attempt is one prompt/provider/extraction/full-gate path; same-turn structured-to-plain negotiation is part of that attempt. A rejected candidate spends one of three attempts regardless of HTTP-call count. A provider error terminates even after earlier content rejections; it is not converted into corrections or exhaustion. Current staging-I/O classification remains unchanged below.

The driver alone owns the limit **3**, numbering, per-regeneration raw/error history, correction replacement after rejection, success clearing and exhaustion. It returns a validated payload with successful attempt index (and command-specific fallback detail as today), or exhaustion with raw replies and all validation errors in encounter order. It does not assemble prompts, parse content, splice scenes, print failure output, take human feedback or write files. The command runner owns failure rendering; the driver supplies its history unchanged.

### Session state and shipping composition

- Keep private-field `DraftSession`; introduce command-specific `GenerationSession` for NPC feedback/corrections. Each embeds a private corrections-only `RetryState`. This is the one persistent corrections owner, not a second list beside session fields. Feedback remains command-local; prose alone retains capability state/reason and its last accepted draft.
- The driver borrows `RetryState` for automatic correction updates/clearing. Attempt adapters borrow corrections and prompt context read-only, with only capability state mutable. After driver success, the prose command core remembers the accepted draft before returning; NPC retains its current empty assistant-marker behavior, not new last-config memory. Exact Rust signatures/borrowing are implementation-time compile risks, not a compiled API promised by this record.
- Automatic retry uses the **latest rejection's** corrections. After exhaustion and human feedback, the session operation replaces them with **all accumulated errors**, as both current interactive paths do. Success clears stale corrections before the next feedback regeneration. Per-regeneration count/raw/error history resets; session feedback/capability/accepted-prose context persists as today.
- Main supplies feedback through session operations and does not retain parallel feedback/corrections locals. Brief collection remains command UX. One-shot invokes the same core with a fresh command session; interactive invokes it with the continuing session.
- Extract a narrow concrete command runner accepting resolved command inputs, `AuthorChat` and borrowed terminal input/stdout/stderr. It calls the **real** session/driver/attempt composition and existing atomic writer, not a test-only duplicate. Main retains Clap, configuration/environment/provider construction, async-runtime construction and process-exit wiring, and shipping main must invoke this runner.
- The runner returns typed `Completed`/`Discarded` or the existing typed error rather than exiting the process. Discard/EOF still prints `Discarded — nothing written.` to stdout and exits 0 through main; failures retain their existing status. No generic filesystem, service container or fake driver in composition tests.

### Frozen behavior, composition and residuals

Preserve exact current stdout/stderr bytes and destinations, status, preview/prompts, brief handling, feedback/blank-input behavior, accept/discard/EOF/dry-run, atomic write and unrelated-file preservation. Prose cannot accept after exhaustion until a validated draft exists. One-shot exhaustion still exits with code 1, prints the last raw reply labelled "best" and **all errors in order including duplicates**, and writes nothing; interactive exhaustion remains feedback/discard rather than an automatic process failure. Do not rank raw replies, deduplicate diagnostics or clean up output during extraction.

- **Staging I/O residual:** the staging owner and the public `validate_edited_scene` adapter map temporary-directory/copy/write errors into corrections, so they consume the validation budget. Preserve that classification; terminal infrastructure-error handling requires a separate decision/repair, not a silent refactor change.
- **NPC interactive output residual:** preserve its current raw-output omission and distinct error rendering; changing this difference from `author-cli.md` §5's one-shot output requires separate repair. This extraction does not unify exhaustion UX.
- **Pack RON bridge composition:** both real attempts continue to use the shared staging path and the landed [engine-owned pack RON bridge](content-format.md#pack-ron-bridge--decided-seam-2026-10-13). Keep quoted/escaped state fidelity, shared loader diagnostics and no authored `state.ron` write. Neither candidate changes the other's policy; the shared CLI-writer → engine-loader regression remains one witness, not duplicated tests.
- **Staged-validation composition (2026-10-14, delivered 2026-10-16):** the [shared staged-validation design](author-cli.md#9-staged-pack-validation--decided-design-2026-10-14) selects two CLI-internal concrete validators over one private temporary owner. Prose layer 1/render/splice stays inside its complete attempt; layer 2 calls that owner and keeps the existing public Scene helper as a compatibility adapter. Staging owns no session/retry/corrections/provider state and uses the pack RON bridge's actual serializer from the moment it landed. Engine diagnostics become verbatim for both paths (deliberate removal of NPC substring surgery, no symmetric scrub); ordinary I/O classification for the Scene path remains unchanged and separately owned, while the NPC preparation/filtering candidate-bypass was decided and delivered on 2026-10-16; [author-cli §9](author-cli.md#9-staged-pack-validation--decided-design-2026-10-14) owns the confirmed policy (installation + real-loader validation as an `Accepted` precondition; preparation failure terminal; the substring filter reaches loader **content** diagnostics only). That repair is scoped to NPC staged preparation and does not decide the general Scene-path I/O-as-corrections policy, which stays deferred to this record's frozen behavior below. The extraction was delivered on 2026-10-16 after the actual pack RON bridge and attempt/retry implementations and the separately tracked [target-safety repair](author-cli.md#10-authoring-target-confinement--decided-design-2026-10-14) (design settled in [author-cli §10](author-cli.md#10-authoring-target-confinement--decided-design-2026-10-14), delivered 2026-10-16) landed; the design records were not treated as implementation dependencies. The shared fidelity witness remains one, alongside real complete-attempt/command evidence.
- **Target composition (2026-10-14):** [author-cli §10](author-cli.md#10-authoring-target-confinement--decided-design-2026-10-14) is the normative target contract, delivered 2026-10-16. Shipping/raw library entries preflight before attempt/driver/provider work; internal staging consumes opaque checked targets and an immutable checked Pack-path inventory. Terminal target/membership refusal never becomes content corrections, filtering, exhaustion or feedback, so this driver's provider/content policy and ordinary I/O classification remain unchanged. The public Scene helper retains its raw/vector diagnostic adapter but now requires existing-Scene membership; shipping calls the checked internal validators. Preserve safe-target output/retry/session behavior. This is the separately approved target repair, not a hidden attempt-retry or staged-validation policy change, and it was delivered before staged extraction.
- **Other exclusions:** provider credential/error-retention posture and RON pretty-print configurations remain outside this attempt seam. No new ADR is warranted for this small reversible extraction; attempt/retry/adapter/session mechanics add no project-specific glossary concept.

### Required implementation validation

1. **Policy seam:** test-local `ScriptedAttempt` returns typed rejection/success; assert validated payload, successful attempt index, latest correction inputs and correction clearing. Exhaustion preserves all three raw replies and all errors in order/duplicates. Test-local `ProviderFailAttempt` returns a terminal provider error with a forbidden/sentinel later success, proving it cannot become corrections, exhaustion or another attempt, including after a previous rejection. Observe returned values and session state, not chat-call counts alone.
2. **Real attempts:** retain `FakeChat`, `StructuredFake` and test-local `DownChat` over `AuthorChat`. Directly test the complete prose seam: layer-1 rejection, layer-2 loader rejection with diagnostics, accepted edited scene and untouched bytes outside the run, and provider error classification. Test NPC extraction/full-pack validation likewise. Capability rejection followed by invalid plain output then valid plain output is success on attempt 2, not 3; later feedback stays plain. Also test capability mode/reason retention when the fallback plain call fails terminally. No real API or credentials required.
3. **Real composition:** direct command-runner tests inject chat/terminal I/O, not attempt/driver fakes. Cover exhaustion → feedback → validated preview → accept (all exhausted corrections carried, success clears them, write only on accept), and terminal provider failure (no subsequent feedback prompt/write). Preserve discard/EOF/dry-run and accept-after-exhaustion safety. Shipping main must use this same runner.
4. **Binary witnesses:** retain loopback HTTP plus spawned-binary tests for real config/provider/runtime/exit wiring. Add independently captured, reviewed baseline fixtures for exact stdout/stderr bytes, last-raw selection, complete errors and status; assert protected pack/scene bytes on exhausted validation, provider failure and no-write modes. Pin NPC interactive's distinct existing output rather than silently fixing it. Do not derive expected output by calling the new renderer under test.
5. **Coverage migration:** replace only policy-only chat-count assertions whose distinct protection now crosses the typed seam. Keep protocol fields, prompts/context, fallback/session persistence, real loader/splice, protected-file and binary witnesses. A mock-call assertion alone is not evidence of the intended contract ([the testing page](testing.md)).
6. **Checks when implemented:** begin with the smallest applicable CLI test target, then `cargo test --locked -p unmei`; before merge run the workspace gate in [the testing page](testing.md). Exact lifetime bounds, borrowing and runner integration must compile. Until implementation, only document/source/link/scope checks and `git diff --check` validate this record; no Cargo, provider, browser or runtime result is claimed.
