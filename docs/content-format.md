---
title: Pack content format — structured data, dialogue DSL and decision configs
summary: The pack's content contracts — RON structured data, the line-oriented dialogue DSL, decision configs, transitions, manifest and state.
keywords:
  - content-format
  - content-pack
  - dialogue-dsl
  - decision-point
  - decisions
---
# unmei content format

**Status:** settled · **Scope:** the vertical slice.
**Amended:** conditionals added to §3 and fixed state-key kinds in §6 — see [ADR 0004](adr/0004-conditionals-in-the-dialogue-dsl.md); `noul`/`score` decision configs added to §1/§4 — see [ADR 0005](adr/0005-noul-and-score-question-types.md); optional manifest `tagline` and `title_backdrop` added to §2/§6 in the title-screen polish pass; the gallery's accepted-limitation line added to §2 from the gallery-entries resolution; the optional ordered manifest `scenes` list added to §2/§6, 2026-10-08; the `.unmei` encoding rule added to §2, 2026-11-06, because no rule covered editing a scene file's text; the encoding rule widened to every pack text file, with a leading byte order mark allowed and ignored, in §2, 2026-11-06, because a byte order mark turned the first comment line into a say-line; the NPC-id stem rule added to §2, and the GLOSSARY term "Decision point" adopted in §3/§4, 2026-11-06; `pack.glossary.md` added to the pack text files under the encoding and byte order mark rule in §2, 2026-11-06, because it still rejected, or passed on, a leading byte order mark.

Three decisions define the slice's content: **RON** for all structured data, a **line-oriented dialogue DSL** (`.unmei` files) for scenes, and a **per-NPC pack layout**. This document is the working spec; the example files under `examples/demo-pack/` are load-bearing — they double as parse fixtures for the parser and the CLI.

## 1. Structured data: RON

All structured content — manifest, state, NPC definitions — is [RON](https://github.com/ron-rs/ron). Rationale: Rust/serde-native, Bevy's home format, comments and trailing commas, and tagged variants that map the Decisions wire shapes directly. The author CLI accepts LLM JSON and serde-converts to RON before writing; the format authors hand-edit and the format LLMs emit are deliberately different files in the pipeline.

Decision configs carry a **`type`** — `choice` (the default when the key is absent), `noul`, or `score` — and support all three Decisions question types. The original choice-only cut followed from the absence of DSL conditionals (an answer had to be an action id to land on a script label); [ADR 0004](adr/0004-conditionals-in-the-dialogue-dsl.md) added conditionals and [ADR 0005](adr/0005-noul-and-score-question-types.md) reverses the restriction. `state_keys` filtering (see §4) is unchanged.

## 2. Pack layout

```text
my-pack/
├── pack.ron          # manifest: name + entry scene (+ optional tagline, title_backdrop, scenes)
├── state.ron         # declared initial state
├── pack.glossary.md  # optional: pack-local term list for the author CLI
├── AGENT-GUIDE.md    # optional: notes for an AI agent editing the pack; never loaded
├── scenes/
│   ├── first_meeting.unmei
│   └── reunion.unmei
├── npcs/
│   └── ace.ron
└── assets/
    ├── backgrounds/  # bg <id> or title_backdrop: "<id>" -> assets/backgrounds/<id>.png
    ├── sprites/      # sprite <id> <pose> -> assets/sprites/<id>/<pose>.png
    └── audio/        # play <id>     -> assets/audio/<id>.ogg
```

Scene id = scene filename stem. An NPC id = NPC filename stem: `npcs/ace.ron` holds `id: "ace"`, and a file whose `id` does not match its stem fails to load. Pack text files are UTF-8: `pack.ron`, `state.ron`, `scenes/*.unmei`, `npcs/*.ron` and `pack.glossary.md`. A file the loader reads that is not valid UTF-8 fails to load. A single leading U+FEFF byte order mark is allowed and ignored, so the file reads exactly as it would without one. Any other U+FEFF is part of the text, and a write by the author CLI drops the mark. A pack is a directory in exactly this shape; the runtime/CLI validate it on load.

`AGENT-GUIDE.md` is optional documentation, like this page is for an author: no loader reads it, so it adds no content and changes no validation. The demo pack ships one for an AI coding agent working on a pack.

**The gallery shows declared references only** *(added 2026-10-05)*: the in-app gallery lists exactly the images the pack's own files reference — each distinct `bg <id>`, each distinct `sprite <npc_id> <pose>`, and the manifest's `title_backdrop` (merged into the background entry of the same id) — and never scans `assets/`. An image present in `assets/` that no pack file references is invisible to the gallery.

## 3. Dialogue DSL (`.unmei`)

Line-oriented: one command per line, `#` comments, indentation is cosmetic. A line is a **say-line** iff it matches `Speaker: text`; every other non-blank line must start with a keyword. Ambiguity with a literal colon in text is resolved by requiring a single colon surrounded by spaces: `Ace: Hi there: ok.` reads as say-line "Ace" / text "Hi there: ok."

### Commands

| Command | Form | Notes |
|---|---|---|
| Background | `bg <id>` | → `assets/backgrounds/<id>.png` |
| Sprite | `sprite <npc_id> <pose>` | → `assets/sprites/<npc_id>/<pose>.png` |
| Hide sprite | `hide <npc_id>` | |
| Play sound | `play <sound_id>` | → `assets/audio/<sound_id>.ogg` (music and sfx share the namespace) |
| Stop sound | `stop <sound_id>` | takes the same id `play` took |
| Set variable | `set <name> = <literal>` | literal is string / number / bool |
| Player choice | `choice` / `end` block | options: `"<text>" -> <target>` inside the block |
| Label | `label <name>` | names may contain `.` (e.g. `reply.warm`) |
| Jump | `jump <name>` | see transitions (§5) |
| Conditional | `if <condition>` … `else` … `endif` | block form; `else` optional; see “Conditionals” below |
| Decision point | `decide <npc_id>.<question> -> <prefix>` | see §4 |
| Say-line | `<Speaker>: <text>` | `{var}` interpolation in text |

Grammar deliberately **excluded**: arithmetic, loops, macros, string operations, `and`/`or` composition, sub-parsers of any kind. (Conditionals were excluded in the slice and have since been added — see “Conditionals” below and [ADR 0004](adr/0004-conditionals-in-the-dialogue-dsl.md). The `noul`/`score` question types are a decision-config change, not a grammar one; they too were slice-excluded and have since been added — see §4 and [ADR 0005](adr/0005-noul-and-score-question-types.md).)

### Variables

`set name = "bucket"` · `set name = 3` · `set name = true`. Three literal types — bucketed strings (the recommended pattern: `affinity_level: "high"` not raw counters), simple numbers, bool flags. References: `{name}` in say-line text. Branching comes from player `choice`, `decide`, and conditionals (below); state is also collected and sent to Decisions. **A key's declared kind in `state.ron` is fixed** — every `set` must assign the declared kind, and a mismatch is a load-time error.

### Conditionals

An `if` / `else` / `endif` block plays one of two line groups based on the state map, with no decision call:

```unmei
if letters_found > 0
  Ace: You kept it.
else
  Ace: Nothing, then.
endif
```

- **Block form.** `if <condition>` opens; `else` (optional) and `endif` close. Nesting is unbounded — an `if` may appear inside an arm — and is tracked by a depth counter, so indentation stays cosmetic. A stray `else`/`endif`, a second `else`, or an `endif` without a matching `if` is a load-time error.
- **Condition.** `<state key> <op> <literal>`, key always on the left.
  - `==` / `!=` on all three scalar kinds; `<` / `<=` / `>` / `>=` on **numbers only** (strings are compared for equality only).
  - A bare bool key means `== true` (`met_ace` ≡ `met_ace == true`).
  - `<literal>` is the same three kinds `set` accepts: `"string"`, number, `true`/`false`.
  - The key must be declared in `state.ron` (same rule as `set` and `{var}`), and the literal must match the key's declared kind. Both are load-time errors, as is a bare non-bool key or an ordering operator on a bool.
- **Control flow.** Arms may contain any command, including a whole `choice`/`end` block and a `decide` line. A `label` may **not** appear inside an arm — every `jump` target stays a top-level position, so no jump can enter a block past its condition. `choice` option lists stay flat; to condition an option, wrap the whole block in an arm.
- **Evaluation.** The condition is evaluated once when the line is reached; the untaken arm's lines do not execute. A `set` inside an arm affects later conditions, not the block's own test. Control that leaves an arm (a `jump` or a `decide`) simply leaves the block — there is no runtime block stack, because the depth counter exists only at parse time.
- **Reserved words.** `if`, `else`, and `endif` are reserved as a line's first token (lowercase, exact match), so keyword-first classification is unchanged. A line whose first token is one of them must parse as that command; otherwise it is a load-time error. There is no escape syntax in this cut — `If only she had known: yes.` remains a legal say-line, because `If` is not the keyword.

**Live fixture.** The demand case is an epilogue gated on state the pack already collects. `scenes/epilogue.unmei` is reached by a `jump epilogue` at the end of `reunion.unmei`'s `end_reunion`:

```unmei
# scenes/epilogue.unmei — scene id: epilogue
# Reached via `jump epilogue` from reunion. Branches on state the pack already
# collects: string equality (`affinity_level`), numeric ordering (`letters_found`,
# one nesting level) and a bare bool (`met_ace`).

bg school_gate
play music_evening

if affinity_level == "high"
  Narrator: She lingers by the gate, in no hurry to leave.
  if letters_found > 0
    Ace: Keep it. It suits you.
  else
    Ace: Next time, bring me something.
  endif
else
  Narrator: She is already walking away.
endif

if met_ace
  Narrator: You will see her again.
endif

stop music_evening
```

It exercises string equality, numeric ordering, a bare bool, and one nesting level, using only keys declared in `state.ron` and set by `first_meeting.unmei`. The live demo pack ships this scene, so a playthrough reaches it and plays the arm its collected state selects.

**Choice option targets resolve exactly like `jump`**: local label first, then a scene file. (The jump rules in §5 are the single resolution rule for both.)

### Say-lines and narration

`Ace: You seem {affinity_level} on trust today.` Unknown variable in `{…}` = parse error (catch typos at load, not at players). Narration is a say-line with speaker `Narrator:` (plain style in the runtime; no speaker sprite/name plate).

## 4. Decision configs and decision points

Judgment lives in data (what the author CLI edits); flow lives in script. An NPC file (`npcs/<id>.ron`):

```ron
// npcs/ace.ron
(
    id: "ace",
    name: "Ace",
    model: "typesafe/jev-1.13",   // optional; packs pin one model by repetition for now
    decisions: {
        "greet": (
            instructions: "How does Ace greet the player, given their relationship and the scene?",
            state_keys: ["affinity_level", "met_ace"],  // state filter: only these keys go in the request
            criteria: {
                "warm": "An affectionate, friendly greeting fitting a close relationship.",
                "cold": "A distant, guarded greeting fitting a strained relationship.",
                "none": "Neither fits the state.",
            },
            fallback: "cold",   // action used when the Decisions call fails or answers unusably
        ),
    },
)
```

Two further questions in the same map, showing the typed shapes:

```ron
        "is_hostile": (
            type: "noul",
            instructions: "Is the player acting hostile toward Ace, given the state?",
            state_keys: ["affinity_level", "met_ace"],
            criteria: {
                "true": "Openly hostile or dismissive.",
                "false": "Neutral or friendly.",
            },
            fallback: false,
        ),
        "trust": (
            type: "score",
            instructions: "How much does Ace trust the player right now?",
            state_keys: ["affinity_level", "letters_found"],
            criteria: [
                "No trust at all.",
                "Wary but willing to talk.",
                "Complete trust.",
            ],
            fallback: 0,
        ),
```

Schema (`Npc`, `DecisionConfig`):
- `id`, `name`: strings; `model`: optional string (pinned model id — the mitigation for the alpha API's model risk; the response `model` is logged at decision time). Authored as a **plain RON string** (`model: "id"` or omitted) — never RON's `Some("id")` wrapping.
- `decisions`: map of question id → config. Each has:
  - `type`: `"choice"` (the default when the key is absent), `"noul"`, or `"score"` — the Decisions question type. The CLI omits the key for `choice`, so existing configs and regenerated choice files are unchanged.
  - `instructions`: string. (The Decisions wire format also accepts JSON structures here; the slice's CLI emits and validates strings only.)
  - `state_keys`: list of state-map keys to send. Research guidance: filter to what the question needs.
  - `criteria`, whose shape follows `type` (one key, mirroring the wire):
    - `choice`: map of **action ids** → description string. Include `"none"` whenever nothing may fit.
    - `noul`: **optional** map; when present, exactly the keys `"true"` and `"false"` — either key alone is a load-time error. Omitted means the model judges from `instructions` alone.
    - `score`: **ordered** array of level descriptions, lowest first, at least two levels. The wire's `legend` echoes it.
  - `fallback`: **required**, typed to the type's landing space — a criterion id (`choice`), `true`/`false` (`noul`), or a legend index (`score`). Used on API failure, timeout, missing or unusable answer, or unexpected type; the engine logs and continues offline.

**The decision point:** `decide ace.greet -> reply` in a scene script:
1. Engine gathers the NPC's config, filters `state` to `state_keys`, and calls the Decisions API with the type's wire shape.
2. The answer produces one **landing key**, and the script jumps to `label <prefix>.<landing key>`:
   - `choice`: the chosen action id X — including a model `"none"` answer, which is a successful answer, not a failure.
   - `noul`: `true` when the response's `noul` probability is `>= 0.5`, else `false`.
   - `score`: the `0`-based legend index with the highest value in the response's `probabilities` (a tie resolves to the lowest index; a probability key outside the legend can never land). An answer with no usable in-legend probability is unusable and takes the fallback. The scalar `score` and echoed `legend` are logged, not used for landing (scores are not calibrated magnitudes).
3. On failure/fallback — API failure, timeout, missing or unusable answer, unexpected `type`, or a landing key outside the config — jumps to `label <prefix>.<fallback>`.
4. Required labels, all **load-time parse errors** when missing:
   - `choice`: `<prefix>.<action id>` for every criterion id **and** the fallback;
   - `noul`: `<prefix>.true` and `<prefix>.false` (the fallback is one of the two);
   - `score`: `<prefix>.0` … `<prefix>.<n-1>` (the fallback index is one of them).
5. `fallback` is validated against the landing space above; criteria ids and score levels need no separate identity beyond it.
6. The `"none"` criterion — and the author CLI's advisory for a missing one — applies to `choice` only. Typed questions have no no-match answer: a `false` or a low level is a real answer, and an unusable response takes the fallback path.
7. Multi-word ids: avoid spaces in action ids; criteria descriptions may be prose.

**Many NPCs deciding at once is deferred** — the demo pack has one NPC and one decision point, so nothing exercises two Decisions calls in one playthrough. Batching, prefetch, and the shared-`state` constraint are recorded, with the scene that would flip them, in [`decisions-scale.md`](decisions-scale.md).

## 5. Transitions

Labels are **scene-local**. `jump <name>`:
1. resolves in the current scene; if absent,
2. loads `scenes/<name>.unmei` from its top (scene id = filename stem).

Precedence: the local label always wins — if a label and a scene file share a name, `jump` stays in-scene. (Avoid shadowing scene names with labels; the loader may warn.)

No `call`/`return`, no arguments, no label parameters. The demo: `first_meeting` ends `jump reunion` → `reunion.unmei` plays → its `end_reunion` jumps to the `epilogue` scene.

## 6. Manifest and state

```ron
// pack.ron — minimal by decision
(
    name: "unmei demo",
    entry: "first_meeting",
)

// state.ron — the declared initial state
(
    affinity_level: "low",
    met_ace: false,
    letters_found: 0,
)
```

- **Manifest**: `name` and `entry` are required; `tagline`, `title_backdrop` and `scenes` are optional. Deliberately minimal, and version fields were declined (the distribution/packaging fog owns versioning later).
  - `tagline` — optional natural-case copy for the title's tagline slot. Trimmed; empty or whitespace-only means none; a single line (newlines and control characters are load-time errors); `{var}` interpolation is deliberately not allowed. The title renders it **uppercased**. A load-time **warning** past 48 characters (the length the 1280×720 reference column fits) nudges the author; it is never an error and never a Notice.
  - `title_backdrop` — an optional asset id `[a-z0-9_]+`, resolved exactly like `bg`: `assets/backgrounds/<id>.png`. The title draws it full-window, scaled to cover, **exactly as authored** — no veil, tint or overlay — over the neutral gradient underlay, which remains the default when the field is absent and the fallback when the file is missing or undecodable (silent degradation: `warn!` once, no Notice).
  - `scenes` — an **optional ordered list of scene ids** (filename stems, no `.unmei`), the pack's declared play order and the only order any consumer reads:
    ```ron
    scenes: ["first_meeting", "reunion", "epilogue"]
    ```
    Absent and empty (`scenes: []`) are **equivalent**: both mean no declared order, the pack loads and plays exactly as today, and it simply has no timeline. Nothing is derived — there is no alphabetical fallback. The authored order survives load and the author CLI's staging round-trip, while `Pack.scenes` stays an unordered map; the manifest list is the sole order source. Ids are matched exactly (case-sensitive, byte-for-byte stems, no trimming) and validated by existence, exactly as `entry` is.
    - A listed id with no loaded scene is a **load error** (`pack.ron`, no line) naming the id and listing the available scene ids, worded "not among loaded scenes" so it stays true when the file exists but failed to parse. A listed scene that exists but fails to parse reports **both** that parse error and this one — the entry check's existing double-report, stated here as intended.
    - An id listed more than once is a **load error** naming it, one error per repeated id. Existence and duplicate checks are independent, so an id both missing and repeated reports both. The checks accumulate diagnostics rather than failing fast, so one load reports every `scenes` problem.
    - A scene file that exists but is **not listed** still loads and is reachable by `jump`, **with a warning** (`pack.ron`, one per unlisted scene) — it is absent from the timeline, which lists declared scenes only.
    - The **entry scene** left out of a non-empty list warns once; the dedicated warning replaces the generic unlisted warning for that scene (a pack whose entry scene is missing outright already errors). Neither the entry check nor the unlisted warning runs when the field is absent or empty.
    - Every diagnostic is emitted **sorted by scene id**, so output and tests are deterministic.
  - The optional fields are authored as **plain RON values** — `tagline` and `title_backdrop` as strings, `scenes` as a list — or omitted, never RON's `Some("…")` wrapping — and a malformed value is a load-time error. A build older than the fields rejects a pack that declares them (unknown fields are load errors), accepted under the declined-versioning stance.
- `state.ron` is the state vocabulary: every `set` target, every condition's state key, and every `state_keys` entry must be declared here — undeclared keys are load-time errors. Each key's kind (string / number / bool) is **fixed**: every `set` and every condition literal must match it, or it is a load-time error. It doubles as the save-schema anchor for the save/load scope.
- State values are **flat scalars only**: string, number, or bool. Nested values (maps, sequences) are load-time errors — the state map serializes directly into the Decisions request's `state` object. The canonical authored syntax remains the RON struct form `(key: value, …)` as shown; the loader also accepts the RON map form `{"key": value, …}`. State keys are pack-defined, not a closed set of struct fields; unknown schema fields in the manifest and NPC definitions are load-time errors.
- Scene/asset ids are filename stems; labels `[a-zA-Z0-9_.]+`; speakers are free text up to the first `: ` (say-line rule).
- State keys use `snake_case` ids `[a-z0-9_]+` **by authoring convention**. The state-map loader currently requires string keys, not that identifier subset; the bridge change below preserves that tolerance, with no trimming or normalization. This does not expand which spellings the dialogue DSL can reference.

### Pack RON bridge — decided seam (2026-10-13)

**Planning resolution:** human-confirmed. This paragraph records the decision as it stood on 2026-10-13; it named a future implementation slice, and the slice was delivered — see *Implementation status* at the end of this section. RON (ADR 0001), the state value kinds and fixed declared kinds (ADR 0004) remain unchanged.

#### Ownership and interface

One focused **`engine/src/pack_ron.rs`** module owns the pack's RON conversion rules. `StateValue` and the other domain types stay in `engine/src/content.rs`. The module is available as `unmei_engine::pack_ron` and exposes:

```rust
pub fn decode_state(src: &str) -> Result<HashMap<String, StateValue>, String>;
pub fn encode_state(state: &HashMap<String, StateValue>) -> Result<String, String>;
```

It also exposes narrow, named optional-string Serde adapters: a content adapter with `deserialize`/`serialize` hooks, and a provider adapter with a `deserialize` hook. Their names must make the compatibility distinction clear. Both delegate to one private optional-string conversion implementation; callers do not receive a configurable general-purpose decoding policy.

Behind the interface sit the generic `ron::Value` parsing, state-map shape and string-key checks, one scalar conversion, and the single type-name table. Raw-value conversion helpers and that table are **not public**. `StateValue::Deserialize` delegates to the internal scalar conversion rather than retaining its own three-variant match. `StateValue::Serialize` remains the scalar writer, used by `encode_state` through ordinary RON serialization of the typed map; no second hand-renderer is introduced.

`load_pack` routes its whole-state decode through `decode_state`, removing its separate scalar decode and type-name copy. File reads/writes, pack-relative `LoadError` context, error accumulation/early-return behavior, and full pack validation remain with their existing callers. The bridge performs **no filesystem IO and no whole-pack validation**. It is not a new persistence abstraction for save slots or read logs.

#### Compatibility and quoting

- **Optional strings:** content's NPC `model` and manifest `tagline`/`title_backdrop` retain acceptance of plain strings, unit `()`, `None`, and `Some("…")`; CLI provider `base_url`/`model` retain acceptance of plain strings, unit and `None`, and rejection of `Some("…")`. Omitted fields still default to absent. Canonical authored fields and serialized content remain plain strings or omission; legacy wrapper tolerance is not the authoring recommendation. `cli/src/provider.rs` adopts the shared provider adapter, not a copied match or the more permissive content adapter.
- **State reads:** preserve both existing struct and map forms. Non-string keys and non-scalar values still fail; no new key-shape restrictions are added.
- **State writes:** `encode_state` emits a RON **map with quoted string keys**, using the engine's `StateValue::Serialize`. Keys and string values are serialized as strings, never inserted into RON through interpolation. Quotes and backslashes use RON escaping (for example `\"` and `\\`); line breaks, control characters and Unicode use valid RON string encoding. Decoding must preserve the exact string text, not literal escape spellings. There is no key trimming, normalization or escape pre-processing.
- **Staging fidelity:** semantic, not byte-for-byte. Braces, numeric spelling, whitespace, entry order and comments may differ in the temporary pack; exact formatting and deterministic key order are not promised. The canonical authored struct convention is unchanged, and no migration or rewrite of existing source packs is performed.
- **Numbers and duplicate entries:** retain the current RON-number-to-`f64` conversion and RON map behavior. This slice adds no rounding/coercion, finite-number requirement or duplicate-key diagnostics. No new numeric policy is implied by serializer spelling changes.
- **Diagnostics:** preserve the engine-owned messages for an invalid root, non-string key and invalid scalar at a named state key, as well as the existing `StateValue` and optional-string error prefixes. `decode_state` preserves the `invalid RON: …` distinction; `pack.rs` adds `state.ron` context once. The private type names remain shared. Dependency-generated parser details and OS-generated IO text are not new byte-stability promises.

#### Delivery and validation

The shared bridge and escaping fix ship in **one implementation slice**. The staging owner (`cli/src/staging.rs`) calls `encode_state` for its temporary `state.ron`; both decision-config generation and prose drafting already share that staging path. The CLI still never rewrites the author's `state.ron`, `pack.ron` or other protected files as a consequence of validation. General RON pretty-print configuration, retry policy, pack/save validation ownership and sidecar persistence policies are outside this slice.

The implementation handoff must include minimal deterministic tests protecting these observable boundaries:

1. **Required defect regression:** load a real source pack, stage it through the CLI's actual staging transplant (`cli/src/staging.rs`), then reload through the real engine loader and assert the entire state map retains its values. Include a string containing **both a quote and a backslash**, a line break/Unicode, a quoted non-identifier string key requiring escaping, and ordinary bool/finite-number scalars. Assert the source `state.ron` bytes remain unchanged. This must fail with the old hand-renderer, not merely test RON in isolation.
2. Protect both accepted state read forms, plus invalid-RON syntax (retaining the `invalid RON: …` distinction), actionable wrong-root, non-string-key and named invalid-scalar errors; preserve the existing scalar Serde boundary without duplicating equivalent loader tests.
3. Protect the optional-string compatibility distinction at the content and provider consumers: plain strings/absence remain usable, content retains `Some(string)` tolerance, and provider config rejects it. Reuse existing fixtures where they cover these behaviors.
4. Keep decision-generation and prose-drafting validation coverage meaningful: neither path may misclassify a valid state string as a candidate error. Reuse current command/retry/non-destructive-write tests rather than add redundant helper-level assertions.

Run focused engine/CLI tests first (`cargo test --locked -p unmei-engine`, `cargo test --locked -p unmei`), then `cargo fmt --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace`. These are future implementation gates, not validation already performed by the planning session.

No new ADR is needed: this preserves the existing on-disk format and compatibility while consolidating conversion and fixing staging serialization. The existing **State map** glossary term remains accurate; the bridge is implementation vocabulary, not a new domain concept.

**Implementation status (2026-10-14; owner location updated 2026-10-16).** The bridge and the staging fix have landed. `engine/src/pack_ron.rs` owns `decode_state`/`encode_state` and the content/provider optional-string adapters; `load_pack` and `StateValue`'s `Deserialize` route through the one private scalar conversion, and `cli/src/staging.rs`'s private staging owner writes the staged `state.ron` with `encode_state` (moved there from `cli/src/gen.rs::transplant_pack`), so both authoring commands stage through the engine serializer. The shared writer-to-loader fidelity witness is `staging::tests::staging_preserves_the_whole_state_map_and_authored_bytes` (relocated with the owner; it fails against the old hand-renderer). The planning prose above is unchanged and now records shipped behavior.

## 7. Example pack

`examples/demo-pack/` in this repo holds a complete working example (three scenes, one NPC with a `greet` decision, minimal assets). The files are canonical: the parser and the CLI are validated against them.

- [`examples/demo-pack/pack.ron`](../examples/demo-pack/pack.ron)
- [`examples/demo-pack/state.ron`](../examples/demo-pack/state.ron)
- [`examples/demo-pack/pack.glossary.md`](../examples/demo-pack/pack.glossary.md)
- [`examples/demo-pack/scenes/first_meeting.unmei`](../examples/demo-pack/scenes/first_meeting.unmei)
- [`examples/demo-pack/scenes/reunion.unmei`](../examples/demo-pack/scenes/reunion.unmei)
- [`examples/demo-pack/scenes/epilogue.unmei`](../examples/demo-pack/scenes/epilogue.unmei)
- [`examples/demo-pack/npcs/ace.ron`](../examples/demo-pack/npcs/ace.ron)
- `examples/demo-pack/assets/` — shipped stand-in art/audio (flat-shape PNGs + synthesized OGGs); replace by filename to upgrade. See its README.
