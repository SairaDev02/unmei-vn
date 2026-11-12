---
title: "Decision points: judgment in data, flow in script"
summary: Judgment config lives in the NPC file and flow in one decide line; the choice-only clause was later reversed.
keywords:
  - decision-point
  - decisions
  - dialogue-dsl
---

# Decision points: judgment in data, flow in script, choice-only

> **Amended by [ADR 0005](0005-noul-and-score-question-types.md)**: the choice-only clause is reversed — configs support `noul` and `score` alongside `choice`, and every type lands the script on `<prefix>.<landing key>`. The core below — judgment in data, flow in script, one `decide` line, an author-named fallback — stands.

A Decisions decision is split across the two content kinds: judgment config (instructions, criteria, fallback, state filter) lives in the NPC's structured file — exactly what the offline author CLI generates and edits — while flow lives in the scene script as one greppable `decide <npc>.<question> -> <prefix>` line whose chosen action id X jumps to label `<prefix>.X`. Configs support `choice` questions only in the slice: without DSL conditionals there is nothing a `noul`/`score` answer could branch to, so the schema carries dead types. A model `none` answer is a successful answer (jumps to `<prefix>.none`); the author-defined `fallback` action fires only on API failure or unusable response, keeping the engine's logs-and-continues-offline posture.

## Consequences

- The CLI's generated files never embed scene knowledge (no jump tables in NPC files).
- Required `<prefix>.<action>` labels are validated at pack load: a missing label is a parse error, not a runtime surprise.
- Multi-question NPCs (several entries in `decisions`) work with no extra grammar.
