---
title: Scaling Decisions to many NPCs
summary: The deferral of batching and prefetch for many-NPC Decisions calls, the rejected cache, the flip trigger and the constraints already known.
keywords:
  - decisions
  - decision-point
---
# Scaling Decisions to many NPCs

**Status:** **deferred** — settled · **Scope:** the runtime Decisions layer.

The `NEXT-STEPS.md` "Many NPCs deciding at once — batching or caching Decisions calls" deferral (that file was deleted 2026-10-08) resolves as a **recorded deferral**: no batching, no prefetch, no cache. This doc is the trigger and the constraints a future effort starts from; it is not a design to build against today.

## 1. No demand today

The reference content is `examples/demo-pack/`, and it contains **one NPC** (`ace`) with **one decision** (`ace.greet`, used once, in `scenes/reunion.unmei`). Nothing in the pack places two Decisions calls in one playthrough, let alone in one beat. Per this effort's demand-grounding rule, a strategy with no demand case does not get designed ahead of one.

**Flip trigger:** a scene — or consecutive scenes in one playthrough — with `decide` points for **two or more different NPCs**. That is when decision latency can stack before or between player input, and when grouping requests could plausibly pay.

## 2. The flip scene, sketched

Proposed, **spec text only** — never added to the live pack by this effort (this plan's fixture rule). It would also need a proposed `npcs/kai.ron` carrying a `greet` config shaped like Ace's.

```text
# scenes/group_confrontation.unmei — PROPOSED, NOT in the live pack.
# Flip scene: two NPCs decide in the same playthrough.

bg school_gate
play music_evening

sprite ace neutral
sprite kai neutral
Narrator: Two figures at the gate — Ace, and a friend she never mentioned.

decide ace.greet -> ace_reply

label ace_reply.warm
Ace: You came back. I hoped you would.
jump kai_turn

label ace_reply.cold
Ace: Oh. It's you.
jump kai_turn

label ace_reply.none
Narrator: Ace watches you approach, unreadable.
jump kai_turn

label kai_turn
decide kai.greet -> kai_reply

label kai_reply.warm
Kai: So you're the one she keeps talking about.
jump end_scene

label kai_reply.cold
Kai: ...Do I know you?
jump end_scene

label kai_reply.none
Narrator: Kai studies you in silence.
jump end_scene

label end_scene
stop music_evening
```

## 3. If it flips: the constraints already known

Carried from the published Decisions request shape, so a future effort does not re-derive them:

- **Batching is same-model-only.** The Decisions request puts `model` at the top level, sibling to `questions`; there is no per-question `model`. Only questions pinned to the same model id can share a request. (Packs currently pin one model "by repetition", so a multi-NPC scene will often share one — but that is a content fact, not a guarantee the engine may assume.)
- **A shared `state` costs precision.** `state` is also request-level, so one batch forces the **union** of every question's `state_keys` into every question's view. The engine's per-question `state_keys` filter has no wire equivalent, and the decision-model guidance is explicit that irrelevant state hurts accuracy. Batching is a precision trade, not a free win.
- **No documented question cap.** The published ceiling is the model context window and the endpoint's body limit (`413 PayloadTooLargeResponseError`), not a count.
- **Per-question fallback fits inside a batch.** Answers come back keyed and independently `type`-tagged; the existing author-fallback posture can be applied per question. A missing key or unexpected `type` is already specified as an error.
- **Observability must be client-side.** `usage` is one object per response and `usage.cost` cannot be split per question. Log each question id beside the response's `model` and cost.
- **Latency scaling is unmeasured.** Docs say nothing about 1 → N questions in one request; that needs a measurement, not a doc claim, before any batching decision is treated as settled.

## 4. Cache: rejected

Caching is **rejected**, not deferred. Decisions answers are non-deterministic judgments over whatever state the request carried; replaying a stored answer presents a stale judgment as a current one, which is dishonest. The endpoint offers no idempotency or cache surface either, so any cache would be wholly client-side — and the honesty objection stands regardless.

No other part of the strategy is settled: batching, prefetch/scheduling, and partial-failure handling all wait on the trigger. Implementation is out of this effort's scope in any case (plan, don't do).
