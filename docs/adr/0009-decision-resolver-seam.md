---
title: One narrow decision-resolver seam without a vendor abstraction
summary: The provider stays a concrete OpenRouter client; one narrow DecisionResolver trait abstracts the resolved landing.
keywords:
  - decisions
  - ownership
---

# Decision resolver seam: no vendor abstraction, one narrow injection point

> **Implemented.** The original rationale and consequences below record planning-time context, including the then-uncompiled signature and build ordering. The amendment at the end of this record records the shipped seam; the original plan-only wording is not its current status.

The [harden-the-runtime map](../runtime-hardening.md) charted the Decisions provider as research-then-decide, and the provider research returned the evidence: **drift risk LOW**. The follow-up decision turned that into a verdict, and the evidence pointed two ways at once, so the record carries two decisions. Build-ready detail — the trait's shape, its home, the app landing, the build ordering and the test list — is in [`docs/runtime-hardening.md` § Provider seam](../runtime-hardening.md#provider-seam).

**Decision.** The vendor abstraction **abstains**; one narrow injection point is **cut now**.

- **No `DecisionBackend`.** The Decisions client stays a single hardwired OpenRouter implementation (`engine/src/decision/client.rs`). Nothing abstracts the wire: `DecisionRequest`, the typed `Answer` enum, `Decision` and `DecisionError` stay concrete, and no second implementation is written. `UNMEI_DECISIONS_URL` (the endpoint override), the per-NPC pinned model id and the pure `resolve_or_fallback` already cover the realistic breaks — a provider-agnostic trait buys nothing today that they do not.
- **A `DecisionResolver` trait is cut**, in `engine/`'s always-compiled `decision` module, abstracting the *resolved landing*: `resolve(...)` — one call per parked question, the author fallback already applied — returning `ResolvedDecision`. `DecisionsClient` is the real implementation, behind the existing `network` feature; the app holds `Option<Arc<dyn DecisionResolver>>`, and tests implement the trait locally with a fake. The name is deliberate: an earlier provider-agnostic `DecisionBackend` was declined twice, and this is not that trait.
- **The revisit trigger is recorded** — a real alpha break, a second provider, an offline mode, or a many-NPC batching story. A **cache** is not on the list: [`decisions-scale.md` §4](../decisions-scale.md) rejects caching outright.

**Why.** The asymmetry is **testability, not portability**. `cli/src/provider.rs` already runs the authoring side behind `pub trait AuthorChat` with four test-local fakes, while `app/` holds a concrete `Option<DecisionsClient>`: every deterministic in-process test passes `None`, which routes to the author fallback, and the only test that passes a real client is the `#[ignore]`d live network test — so no reproducible app test can drive a model answer, a held answer, or a cancelled request. Three rules the map has already settled need exactly those paths: [Menu overlay](../runtime-hardening.md#menu-overlay) §6's answer held while the Esc overlay is open, §6's load-cancels-the-request, and [Runtime failure posture](../runtime-hardening.md#runtime-failure-posture)'s defensive hold with its Notice. Cutting the seam is cheap and additive; leaving those three rules specified but undrivable in-process is how they ship broken.

The split is the honest reading of the evidence. The provider research found nothing that argues for restructuring the client before an actual break — and nothing that makes an untestable decision pipeline acceptable. Two different questions, two different answers.

**Considered options.**

- **Cut the full provider-agnostic `DecisionBackend` trait now** (the other horn). Rejected: it contradicts the earlier twice-declined ruling on the strength of evidence that says the opposite, and a wire-level abstraction would have one implementation and no second consumer.
- **Wait on both** — no seam, the three rules stay untestable in-process. Rejected: it trades a cheap additive trait for three rules exercisable only against the live API, and two `#[ignore]`d live tests are the repo's entire network coverage.
- **A transport-only seam** (`decide(...) -> Result<Decision, DecisionError>`, with the app still calling the pure `resolve_or_fallback`). Rejected: thinner than the app's actual need, and it leaks `DecisionError` into every call site. The resolved-landing shape is precisely the app's question — *resolve this parked question, fallback included*. Accepted cost: a fake implementation bypasses `resolve_or_fallback`, which keeps its own engine unit tests.
- **Adding `async-trait`** for an `async fn` in the trait. Rejected: a boxed-future signature on the trait object needs no new dependency, for a trait with one method.
- **Making `RuntimeCorePlugin` generic over the backend** instead of `dyn`. Rejected: it genericizes the app's public plugin API and every system touching the resource, to avoid one `Box::pin`.
- **Unifying `AuthorChat` with the new resolver.** Rejected: offline CLI authoring is a different job — a different endpoint, a different auth env, a different wire shape, structured output with a batched retry loop, and no in-fiction fallback (its failure is a CLI message, not a script branch) — and it lives in `cli/`, which `engine/` cannot depend on.
- **Treating the in-fiction author fallback as a second backend.** Rejected: `Fallback::Action`/`Bool`/`Index` is a config value the pure `resolve_or_fallback` already applies. It has no address, no request and no failure mode of its own.
- **Shipping a fake backend in `engine/` behind a test-support feature.** Rejected: `cli/`'s precedent is test-local fakes with none shipped, and this fake is too coupled to the app's timing rules to be a useful general-purpose one.

## Consequences

- **`engine/` gains one public trait and one implementation** and stays Bevy-free and runtime-free. `app/` gains one `Arc<dyn>` in `RuntimeConfig` and keeps `None` as the offline posture, so no existing behaviour changes.
- **The three rules become buildable test-first.** The menu overlay's held-answer and load-cancel rules and the failure posture's defensive hold are writable in-process, with no network and no key — and the seam is a prerequisite for them, so it is the build effort's first step rather than a later refinement.
- **The signature is a design sketch.** The section gives it one canonical home and marks its exact lifetime/`Send` bounds as to be validated at first compile, because they interact with how the app spawns the task and with the held-answer and cancellation rules. A plan-only record cannot compile it.
- **A fake is not a stub: it must be jointly controllable** — able to withhold completion and to observe that a dropped task was dropped — or it re-proves only what `None` already proves. That requirement belongs to the plan, not to the build.
- **The provider-agnostic path stays closed** until one of the four triggers fires. The trigger is recorded here and in the section, and `NEXT-STEPS.md`'s two `DecisionBackend` mentions pointed at it instead of offering the trait *(that file was deleted 2026-10-08; this record and the section are the trigger's surviving home)*.
- **Nothing is implemented.** The map is plan-only: this record and the section specify the seam; a later build effort writes it.

## Amendment (2026-10-14, seam implemented)

**Implemented.** The hardening build cut the seam this record decided: `DecisionResolver` lives in `engine/`'s always-compiled `decision` module, `DecisionsClient` implements it behind the existing `network` feature, and the app holds `Option<Arc<dyn DecisionResolver>>` in `RuntimeConfig`, with `None` keeping the offline posture. The original rationale and consequences — including the design-sketch lifetime/`Send` bounds, first-build ordering and **"Nothing is implemented"** wording — describe this record's planning date, not the current state; the three rules the seam was cut for — [Menu overlay](../runtime-hardening.md#menu-overlay)'s held answer and load-cancels-the-request, and [Runtime failure posture](../runtime-hardening.md#runtime-failure-posture)'s defensive hold — are now driven in-process by a jointly controllable fake (`app/src/tests.rs`). The decision, its rejected alternatives and the four revisit triggers above are unchanged.
