---
title: noul and score question types land on labels
summary: Decision configs gain noul and score question types, and every typed answer still lands the script on a label.
keywords:
  - decisions
  - decision-point
  - content-format
---

# noul and score question types land on labels

Decision configs gain the other two Decisions question types — `noul` (a yes/no judgment) and `score` (an ordered level) — **amending [ADR 0003](0003-decision-points-judgment-in-data-flow-in-script.md)**. 0003 restricted configs to `choice` because, without DSL conditionals, only an action id could branch the script; that premise is void since [ADR 0004](0004-conditionals-in-the-dialogue-dsl.md) added conditionals, so this decision reverses the choice-only clause. 0003's core stands: judgment in data (what the CLI edits), flow in script, one greppable `decide` line, and an author-named fallback.

**Typed answers land on labels, not state.** Every `decide <npc>.<question> -> <prefix>` still means exactly one thing — jump to a label — whatever the question type:

- `choice` → `<prefix>.<action id>` (unchanged);
- `noul` → `<prefix>.true` / `<prefix>.false`, from `noul >= 0.5`;
- `score` → `<prefix>.<legend index>`, the `0`-based position of the level, from the highest entry in the response's `probabilities`.

The alternative — writing a bool or number into the state map for a following `if` to test — was rejected. A `decide` prefix is a label and may contain dots, while state keys are `[a-z0-9_]+`, so the prefix cannot name the key and the syntax would need a second naming surface; and it would give `->` two meanings depending on `type`. Landing on labels keeps the mechanic uniform, keeps typed answers out of `state.ron`, and still lets an author retain a value by writing `set` inside the landed branch. The ordered `score` legend is itself the bucketing, so no raw, uncalibrated magnitude (research 0001 §4) ever reaches content.

**Demand case (prospective).** Typed questions were unlocked by [ADR 0004](0004-conditionals-in-the-dialogue-dsl.md), which [ADR 0002](0002-line-oriented-dsl-without-conditionals.md) had already paired with them (`noul`/`score` "become expressible at the same time"). ADR 0004's proposed `scenes/epilogue.unmei` is the *conditionals* thread's fixture, not a typed-question one, and no pack change exercises a `noul` or `score` question yet — the pack's one decision point (`decide ace.greet -> reply`) is a `choice`, and a live fixture cannot exist until the loader parses the new shapes. The change is additive, so it lands without a scene; the first scene that needs the model to judge a bool or a level is its prospective demand case.

## Consequences

- **`type` on `DecisionConfig`** — `"choice"` (the default when the key is absent), `"noul"`, or `"score"`. Existing choice-only files, including `examples/demo-pack/npcs/ace.ron`, parse and behave identically; the CLI emits `type` only for non-choice configs, so regenerated choice files stay byte-identical.
- **One polymorphic `criteria` key, shape set by `type`.** `choice`: a map of action id → description. `noul`: an optional map with both-or-neither `"true"`/`"false"` (absent = instruction-only). `score`: an ordered array of level descriptions, lowest first, at least two levels. This mirrors the Decisions wire, which keeps one `criteria` field per question.
- **`fallback` is required and typed to the landing space** — a criterion id, `true`/`false`, or a legend index — and still fires on API failure, timeout, missing or unusable answer, or unexpected `type`. It jumps to `<prefix>.<fallback>`.
- **Label closure is derived per type** and stays a load-time error: `choice` needs every criterion id plus the fallback; `noul` both `<prefix>.true` and `<prefix>.false`; `score` `<prefix>.0` through `<prefix>.<n-1>`.
- **The `"none"` advisory stays choice-only.** Typed questions have no no-match answer — a `false` or a low level is a real answer, not a failure — so the advisory is not broadened.
- **Typed answers declare no `state.ron` keys.** `state_keys` filtering is unchanged.

## Considered options

- **Store the answer in state.** Rejected above: prefix/key mismatch, two meanings for `->`, and a second landing mechanic to learn.
- **Author-named score levels** (`<prefix>.high`) by making score criteria `(id, description)` pairs. Rejected: an id that is never sent to the model, and a second shape on top of the wire's plain description array. Positional legend indices are the wire's own keys.
- **Threshold or round the score scalar** (`round(score)`). Rejected: the float is not a calibrated magnitude; the model's discrete `probabilities` is its own bucketing, and argmax is deterministic.
- **Per-config thresholds / bucketing knobs.** Rejected: more schema that invites tuning against an uncalibrated number.
- **An abstain level for typed questions.** Rejected: not part of the verified wire contract; authors who need "unclear" use a `choice`.
