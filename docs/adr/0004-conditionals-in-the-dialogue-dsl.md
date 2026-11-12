---
title: Conditionals in the dialogue DSL
summary: The grammar gains flat if/else/endif blocks with no indent stack, and a state key's declared kind becomes fixed.
keywords:
  - dialogue-dsl
  - content-format
---

# Conditionals in the dialogue DSL

The `.unmei` grammar gains `if` / `else` / `endif` blocks, **amending [ADR 0002](0002-line-oriented-dsl-without-conditionals.md)**. 0002 excluded conditionals so that branching stayed player choice + decisions only; that exclusion was explicitly slice-scoped ("revisit conditionals only when a real scene demands them"), and a demand case now exists.

**The demand case is shipped content, not a hypothetical.** `examples/demo-pack/` collects state it cannot act on: `first_meeting.unmei` sets `letters_found` and `met_ace`, and `state.ron` declares `affinity_level`, but no construct branches on any of them — they can only be interpolated into prose or sent to a decision model. Routing a deterministic question through `decide` would spend an API call and a model judgment on a fact the script already holds, add latency, and fail offline. The live `scenes/epilogue.unmei` is the canonical fixture.

The addition is deliberately **flat-scalar flow control only**:

- **Block form.** `if <condition>` … `else` … `endif`. `else` is optional; there is no `elif` — authors nest. Nesting is unbounded, tracked by a depth counter, so 0002's **line-scanner constraint is retained**: indentation stays cosmetic and no indent stack is introduced.
- **Condition.** `<state key> <op> <literal>`, key always on the left. `==` and `!=` apply to all three scalar kinds; `<`, `<=`, `>`, `>=` apply to **numbers only** (bucketed strings are labels, not magnitudes). A bare bool key means `== true`. Literals are the same three kinds `set` accepts.
- **Retained exclusions.** Arithmetic, loops, macros, string operations, `and`/`or` composition, and sub-parsers of any kind all stay out.

## Consequences

- **ADR 0003's rationale is void.** Choice-only decision configs followed from the absence of DSL conditionals; that premise no longer holds. The config schema's non-choice types are a separate decision, recorded in [ADR 0005](0005-noul-and-score-question-types.md).
- **A state key's declared kind becomes fixed.** Because a condition compares a declared key against a literal, `state.ron`'s kind for each key must be authoritative: every `set` must assign the declared kind, and every condition literal must match it. Both are load-time errors. No shipped pack violates this (all five `set` lines in `examples/demo-pack/` match their declared kinds).
- **No jumping past a condition.** A `label` may not appear inside an arm, so every `jump` target remains a top-level position. `choice` option lists stay flat — a conditional cannot appear inside a `choice` block; authors wrap the whole block in an arm instead.
- **`if` / `else` / `endif` are reserved as a line's first token** (lowercase, exact match), so classification stays keyword-first. A line whose first token is one of them must parse as that command, or it is a load-time error. There is no escape syntax; a capitalised `If:` is still a legal speaker, and no line in the demo pack begins with a reserved token.
- **A condition is evaluated once on reach.** A `set` inside an arm affects later conditions, not the block's own test. There is no runtime block stack: a `jump` or `decide` leaving an arm simply leaves the block, because the depth counter exists only at parse time.

## Considered options

- **Defer.** Record a deferral naming the scene that would flip it. Rejected: the demand case is already in shipped content — state collected and unconsumable — not a scene we hope someone writes.
- **`decide` as the substitute.** Route the branch through the decision model. Rejected: API cost and latency, non-determinism for a deterministic question, and it contradicts the engine's logs-and-continues-offline posture.
- **`elif` sugar.** Rejected: a fourth keyword and a parser special case (an `elif` is an `else` holding an `if`); nesting covers the case.
- **Indentation-based blocks.** Rejected: reintroduces the indent stack 0002 deliberately excluded.
- **An expression grammar** (`and`/`or`, arithmetic, string operations). Rejected: exceeds what the demand case needs and reintroduces precedence and parenthesisation parsing.
