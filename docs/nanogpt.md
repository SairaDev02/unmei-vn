---
title: NanoGPT — requirements for replacing OpenRouter as the provider
summary: Requirements for moving the runtime's Decisions calls and the CLI's authoring calls from OpenRouter to NanoGPT, the one supported provider.
keywords:
  - decisions
  - credentials
  - authoring-cli
  - terminal-mask
  - content-format
  - external-dependency
---
# NanoGPT — requirements

**Status (2026-11-16).** Requirements reviewed on 2026-11-15 (three rounds, 12 questions), with a follow-up round on default models on 2026-11-16 (four questions); the decision pin moved to `typesafe/jev-1.14` on 2026-11-16. Requirements accepted 2026-11-15 (PR merged). **Built** on branch `feat/19-nanogpt` (2026-11-16). By decision (N21), no separate spec follows: this page is precise enough to build from, and the build amended each contract named in [Contracts this will amend](#10-contracts-this-will-amend). The decision record is [ADR 0015](adr/0015-nanogpt-is-the-one-provider.md). This work takes priority over all other pending development, including the Keiko spec and the plugin family specs.

**What this is.** Unmei calls an LLM service in two places. At play time the runtime resolves each decision point through OpenRouter's alpha Decisions API (`POST https://openrouter.ai/api/alpha/decisions`). Offline, the `unmei author` CLI drafts prose and generates decision configs through an OpenAI-compatible chat endpoint, which defaults to `https://openrouter.ai/api`. OpenRouter has become a liability on cost and on content screening. **NanoGPT** replaces it as the one provider for both.

**What NanoGPT offers (checked against its docs on 2026-11-15).**

- A decisions endpoint, `POST https://api.nano-gpt.com/api/v1/decisions` ([decisions](https://docs.nano-gpt.com/api-reference/endpoint/decisions.md)). It takes the same `{ model, state, questions }` body and returns the same typed `choice`, `noul` and `score` answers that the engine already decodes. The differences:
  - The root schema rejects unknown fields (`additionalProperties: false`).
  - A question can come back as `{ "type": "refusal" }`.
  - A `score` question takes 2 to 10 levels.
  - `usage` carries `estimated_cost` unless the request sends `include_usage: true`, which returns the recorded `cost`.
  - Only decision models answer. `GET /api/v1/decision-models` listed 20 on 2026-11-16, among them `typesafe/jev-1.13` (the demo pack's pin today) and `typesafe/jev-1.14`, both at $0.042 per million input tokens. They are not in `/api/v1/models`.
  - Billing counts input tokens only.
- An OpenAI-compatible chat endpoint, `POST https://api.nano-gpt.com/api/v1/chat/completions`, that accepts `response_format` with a strict `json_schema` and the `provider` object, including `require_parameters` ([chat completions](https://docs.nano-gpt.com/api-reference/endpoint/chat-completion.md), [migrating from OpenRouter](https://docs.nano-gpt.com/migrate-from-openrouter.md)). Model ids use the same `vendor/model` form as OpenRouter. About half of OpenRouter's catalog matches exactly.
- Bearer authentication. Keys look like `sk-nano-<uuid>`, and older keys are a plain UUID. A missing or invalid key gets `401`. After repeated rejections NanoGPT answers `429` with `code: "authentication_rate_limited"` and a `Retry-After` of up to 15 minutes ([authentication](https://docs.nano-gpt.com/authentication.md), [errors](https://docs.nano-gpt.com/api-reference/miscellaneous/error-handling.md)). No endpoint is labelled as a key check.
- No moderation by default. NanoGPT's moderation is paid, opt-in per request, screens input only, and is not listed for the decisions endpoint ([inline moderation](https://docs.nano-gpt.com/api-reference/miscellaneous/inline-moderation.md)). The pinned model may still decline, which shows as a refusal.
- Privacy: prompts and conversations are not stored by default, and NanoGPT does not train on input or output. Upstream model providers retain data under their own terms ([privacy policy](https://nano-gpt.com/legal/privacy-policy)).

---

## 1. Provider

- **N1 — NanoGPT replaces OpenRouter.** NanoGPT is the one provider Unmei supports, defaults to, tests and documents, for both the runtime and the CLI. OpenRouter is not kept beside it as a second provider.
- **N2 — One concrete client.** The runtime keeps one concrete Decisions client, now pointed at NanoGPT, behind the existing [decision resolver seam](adr/0009-decision-resolver-seam.md). This is a replacement, not a second provider, so ADR 0009's second-provider trigger does not fire and no provider abstraction is added.
- **N3 — The overrides stay, unsupported.** `UNMEI_DECISIONS_URL` and the CLI's `base_url` (flag and `unmei.author.ron`) keep working as escape hatches to another compatible endpoint. Unmei neither supports nor tests what they point at, and N10's key refusal still applies.

## 2. Runtime decisions

- **N4 — The endpoint.** The default Decisions endpoint becomes `https://api.nano-gpt.com/api/v1/decisions`. The request body stays `{ model, state, questions }`, with one question per call and the state filtered to the config's `state_keys`.
- **N5 — Actual cost.** Every Decisions request sends `include_usage: true`, so the cost the session log and the Monitor show is the recorded charge, not an estimate. No other field is added to the request (N18).
- **N6 — A refusal takes the fallback.** A `{ "type": "refusal" }` answer lands on the author's fallback, like any unusable answer. It has its own categorical diagnostic, "the model refused the question", so the session log and the Monitor tell a refusal apart from a broken response. Like any fallback, it does not refresh the autosave, and the player sees no Notice.
- **N7 — The demo pin becomes `typesafe/jev-1.14`.** The demo pack's NPC pins `typesafe/jev-1.14` in place of `typesafe/jev-1.13`, and the docs that quote the demo's pin follow. A test that uses a model id only as an arbitrary string need not change. The build confirms with one live call that NanoGPT answers the pin. If the call fails, the build stops and brings the pin back to the maintainer rather than choosing a replacement model.
- **N8 — Everything else is unchanged.** The landing rules, the fallback posture, the timeouts, the categorical diagnostics, the model-pin provenance rule, the no-redirect rule and the held-answer and cancellation behavior stay as they are.

## 3. Keys

- **N9 — Environment variables.** The runtime's override of the credential file becomes `NANOGPT_API_KEY`. The CLI reads `UNMEI_API_KEY`, then `NANOGPT_API_KEY`. `OPENROUTER_API_KEY` is no longer read by either.
- **N10 — An OpenRouter key is never sent.** No Unmei request carries a key that starts with `sk-or-`. This holds for the credential file, `NANOGPT_API_KEY` and `UNMEI_API_KEY`, and for the CLI whatever `base_url` points at. Sending an OpenRouter key would hand an OpenRouter credential to a third party.
- **N11 — The key row names an OpenRouter key.** A stored or environment key that starts with `sk-or-` shows a new key row state, **Invalid, an OpenRouter key**, which asks for a NanoGPT key. The runtime builds no resolver from it, so decision points take the fallback. The file is not changed or deleted. A newly entered `sk-or-` key is refused before it is written.
- **N12 — The CLI refuses an OpenRouter key.** The CLI stops before any network call when the key it resolves starts with `sk-or-`. The message names the variable and asks for a NanoGPT key.
- **N13 — The terminal mask follows the variables.** The CLI's [terminal mask](adr/0014-cli-diagnostics-and-terminal-credential-safety.md) covers exactly the two variables the CLI reads, `UNMEI_API_KEY` and `NANOGPT_API_KEY`.

## 4. Key check

- **N14 — A read-only check that spends nothing.** The key row's ping uses a NanoGPT call that reads and spends no balance. The build picks the endpoint after a live test: `GET /api/v1/api-keys/current` first, `GET /api/v1/usage` as the alternative. The verdicts keep today's mapping: `2xx` is valid, `401` or `403` is rejected (**Invalid, rejected by NanoGPT**), and anything else, including the `429` for too many failed attempts, a timeout or a network failure, is not checked. The 5-second timeout and the rule that the key endpoint does not follow `UNMEI_DECISIONS_URL` stay.

## 5. Authoring CLI

- **N15 — The default endpoint.** The CLI's built-in `base_url` becomes `https://api.nano-gpt.com/api`. The CLI still appends `/v1/chat/completions`, and there is still no default model.
- **N16 — The structured request stays.** Structured drafting keeps sending a strict `json_schema` `response_format` with `provider.require_parameters: true`, which NanoGPT accepts. The fallback for an endpoint that cannot honor structured output is unchanged.
- **N17 — Packs keep their file.** A pack whose `unmei.author.ron` names another `base_url` keeps it (N3). The CLI does not rewrite or warn about the file.

## 6. Content and privacy

- **N18 — Unmei sends no screening or routing options.** No `moderation` header, no `provider.zdr`, no `data_collection` and no provider routing go with a Decisions request. Screening is whatever the pinned model does, and a refusal takes the fallback (N6). Data handling is governed by the player's own NanoGPT account settings.
- **N19 — The player docs state what is sent.** The Guide's live-decisions page states that each request sends the NPC's instructions, its criteria and the state map filtered to `state_keys`, and that NanoGPT bills input tokens only. It links NanoGPT's privacy policy.

## 7. Pack format

- **N20 — At most ten levels.** A `score` decision config needs two to ten levels. The loader rejects more than ten, so `unmei check` and the CLI's validation gate report it, and a pack never plays a `score` question that always falls back.

## 8. Process and release

- **N21 — Requirements, then build.** No separate spec. The build PR amends the contracts in §10 and lands the glossary changes in §11.
- **N22 — No dedicated release.** The migration merges to `main` and ships with a later release. That release's notes tell players to get a NanoGPT key and authors to update `UNMEI_API_KEY`. *(Amended 2026-11-16: the maintainer chose a dedicated release, 0.2.0, with this migration as its only change. It is 0.2.0, not 0.1.1, because new variants on the public `unmei-engine` enums `DecisionError` and `KeyVerdict` break compatibility. Its notes still carry the key instructions above.)*
- **N23 — Priority.** This work goes before the Keiko spec and the plugin family specs.

### Done when (for the build)

- The runtime resolves the demo pack's decision point through NanoGPT, which is shown by the `#[ignore]`d live tests run once with a NanoGPT key.
- `cargo test --workspace` passes, with tests for N6 (a refusal answer), N10 to N12 (an `sk-or-` key from each source is never sent, and the key row and the CLI say why), N13, N14's status mapping, N20, N25's precedence (flag, file, built-in; replace leaves the pin alone) and N26's note.
- The example authoring model passes N28's live call.
- No contract, Guide page, archive README, crate README or rustdoc names OpenRouter as the provider. A mention that records history, such as an ADR's context, may stay.

## 9. Default models

The runtime and the CLI have no built-in model today. An NPC with no `model` pin never calls the model and takes the fallback. The CLI stops before any request when no authoring model is set. Yet `unmei author npc --create` writes every new NPC with no pin, so an NPC the CLI creates stays offline until the author edits the file, and nothing tells the author.

- **N24 — No runtime default.** The runtime has no built-in decision model. An NPC with no pin in the pack's data never calls the model, as today. A shipped pack plays the same across engine upgrades, and "no pin" keeps its meaning: this NPC is decided offline.
- **N25 — `--create` writes a pin.** `unmei author npc --create` writes a `model` pin into the new NPC file. The pin comes from, highest first: a new `--decision-model <MODEL>` flag; a new optional `decision_model` field in `unmei.author.ron`; the CLI's built-in default decision model, `typesafe/jev-1.14` (the demo pack's pin, under N7's live check). The default applies only at creation and only in the CLI. Once written, the pin is the author's data, and the runtime never fills one in. Replacing a config in an existing NPC never adds, changes or removes its pin. An author who wants an offline NPC deletes the line.
- **N26 — The CLI notes a missing pin.** Wherever the CLI loads and reports on the whole pack, today `unmei author draft --list` and the validation gate before a write, it reports each NPC that has decision configs but no pin. The report is a note that never fails the command, because an offline NPC is valid. The plugin family's planned `--check` ([C1](plugin-family.md#5-pack-wide-check-and-preview)) takes the same note when it is specified.
- **N27 — No authoring default.** The authoring model keeps no built-in default, as today: the CLI stops before any request when neither `--model` nor `unmei.author.ron` names one. The prose style and how willingly a model writes fiction are the author's choice. The demo pack ships no `unmei.author.ron`.
- **N28 — A verified example.** Every authoring model id that the Guide, the CLI contract and the archive READMEs give as an example is a NanoGPT chat model. The build confirms with a live call that NanoGPT serves it and honors a strict `json_schema` `response_format`, so a first-time author does not land on the free-text fallback.

## 10. Contracts this will amend

The build amended each of these in place on 2026-11-16, with a dated pointer. This list is the plan as accepted and stays as it was.

- [Decisions settings](title-and-menu.md#decisions-settings): the credential file holds a NanoGPT key; `NANOGPT_API_KEY`; the key check endpoint (N14); the new state **Invalid, an OpenRouter key** (N11); copy that names OpenRouter; the shape-check test values.
- [Provider configuration](author-cli.md#4-provider-configuration): the default `base_url`, the key variables, the `sk-or-` refusal, the `decision_model` field and the example model (N9, N12, N15, N25, N28).
- [Command surface and write unit](author-cli.md#1-command-surface): the `--decision-model` flag and the pin `--create` writes (N25). Its line saying `--create` writes an "optional `model`" describes a field the code never writes today; N25 makes it true.
- [Validation gate](author-cli.md#3-validation-gate-before-any-write) and [drafting](author-cli-drafting.md): the missing-pin note (N26).
- [Drafting provider](author-cli-drafting.md): the pointer to `author-cli.md` §4 and its variable names.
- [Decision configs](content-format.md#4-decision-configs-and-decision-points): the ten-level limit (N20), the refusal in the fallback list (N6), the example's pin (N7) and, beside the optional `model`, that the runtime has no default (N24).
- [Provider seam](runtime-hardening.md#provider-seam): the concrete client is NanoGPT's (N2).
- [Architecture](architecture.md): the data flow and contract map entries that name OpenRouter.
- [Keiko K19](keiko.md#4-decisions-sandbox-and-faults): "a key check that cannot reach OpenRouter" becomes "cannot reach NanoGPT".
- [Behavioral protection](testing-behavioral-protection.md) and [human smoke checklist](testing-human-smoke.md): the entries that name OpenRouter or its variable.
- ADRs [0009](adr/0009-decision-resolver-seam.md), [0010](adr/0010-credentials-live-in-a-user-level-file.md) and [0014](adr/0014-cli-diagnostics-and-terminal-credential-safety.md): already amended by this page's PR with a pointer to ADR 0015. The build changes no ADR.
- `LANGUAGE.md` P1 and P2: the live-decisions page's job and the player's assumed lack of a key name NanoGPT.
- The Guide (`docs/guide/`), `README.md`, `packaging/player-README.md`, `packaging/author-README.md`, `engine/README.md`, `cli/README.md` and the root rustdoc in `engine/src/lib.rs`.
- `examples/demo-pack/npcs/ace.ron` and the Monitor acceptance fixture that copies it: the pin (N7).
- `GLOSSARY.md`: the changes in §11.

## 11. Glossary changes

The build applies these. The repository adds glossary terms with the behavior they describe, not with requirements.

- **Intro:** NPC responses are chosen at play time by NanoGPT Decisions.
- **Provider** (new, under Decisions): The LLM service aggregator Unmei calls: NanoGPT, for both the runtime's Decisions calls and the CLI's authoring. Unmei supports one provider. An endpoint override may point at another compatible service, but Unmei does not support or test that. _Avoid_: backend, vendor, aggregator (in prose), OpenRouter (the retired provider).
- **Credential file:** holds the player's NanoGPT API key; `NANOGPT_API_KEY` overrides it; a key in OpenRouter's `sk-or-` format, from the file or the variable, is never sent.
- **Key row:** the states become Not set / Set / Set, not checked / Checking… / Invalid / Invalid, an OpenRouter key / Invalid, rejected by NanoGPT / From environment.
- **Terminal mask:** a non-empty `UNMEI_API_KEY` or `NANOGPT_API_KEY` value.
- **Score question:** an ordered list of two to ten levels.
- **Fallback:** used when the Decisions call fails, answers unusably or the model refuses the question.

## 12. Left to the build

- The key check endpoint, after the live test (N14).
- The key row's copy for the new state, the Not set placeholder, the env note and the fingerprint's shape for an `sk-nano-` key, within N11.
- The CLI's refusal message text (N12) and the missing-pin note's text (N26).
- Whether the decoder reads `cost` alone or falls back to `estimated_cost` when `cost` is absent (N5).

### How the build resolved each item (2026-11-16)

- **Key check endpoint (N14).** `GET https://api.nano-gpt.com/api/v1/api-keys/current`, 5-second timeout. Any `2xx` is valid, `401` or `403` is rejected, and anything else, including `429`, is not checked.
- **Key row copy (N11).** The Not set placeholder is `Paste or type a NanoGPT key`. A set key reads `Set — sk-nano-…a1b2`, and a rejected key reads `Invalid — rejected by NanoGPT`. The new states are `Invalid — an OpenRouter key; enter a NanoGPT key` and, for the environment variable, `From environment — Invalid, an OpenRouter key`. A stored or typed `sk-or-` key leaves the field empty and editable; a typed one is cleared and never written, with no ping and no resolver, and the file is untouched. The env note reads `NANOGPT_API_KEY overrides the stored key. Unset it and relaunch to use the stored key.` The fingerprint is the fixed `sk-nano-` prefix when the key has it, otherwise the first 6 characters or fewer, then `…`, then the last 4. A commit runs trim, shape check, then the OpenRouter check.
- **Messages (N6, N10, N12, N26).** A refusal takes the fallback with the diagnostic ``the model refused the question `<id>` ``. An OpenRouter-format key never reaches the network, with the diagnostic `an OpenRouter key is never sent`. The CLI stops before any network call on a resolved `sk-or-` key, with a message that names the variable and asks for a NanoGPT key. The missing-pin note, on stderr, reads ``note: npc `<id>` has decision configs but no model pin, so its decision points always take the fallback``. It comes from `unmei author draft --list` and from the validation gate of `author npc` and `author draft`, once per NPC sorted by id, and never fails the command.
- **Cost (N5).** Requests send `include_usage: true`, and the logged cost is `usage.cost`, the recorded charge. The decoder reads `cost` only and does not fall back to `estimated_cost`.
- **CLI details (N15, N25, N28).** The default `base_url` is `https://api.nano-gpt.com/api`, with `/v1/chat/completions` appended, and there is no default authoring model. `--decision-model <MODEL>` works on `unmei author npc` with `--create` only. `decision_model` is an optional field of `unmei.author.ron`. Precedence is flag, then file, then the built-in `typesafe/jev-1.14`. `--create` alone writes the pin, replace never adds, changes or removes it, and an empty value is an error. The example authoring model is `deepseek/deepseek-v4.1-flash`, a NanoGPT chat model listed with structured-output support.
- **Variables (N9, N13).** The runtime override is `NANOGPT_API_KEY`. The CLI reads `UNMEI_API_KEY`, then `NANOGPT_API_KEY`. The terminal mask covers exactly those two.
- **Pack format (N20).** A `score` config takes 2 to 10 levels, and more is a load error. The demo pack pins `typesafe/jev-1.14` (N7).
- **Live checks (2026-11-16).** Run once with a NanoGPT key:
  - N14: `GET /api/v1/api-keys/current` returned `200` for a valid key, and the account balance was the same before and after the call.
  - N28: `deepseek/deepseek-v4.1-flash` answered a strict `json_schema` request with schema-valid JSON, and `unmei author draft --dry-run` on the demo pack exited 0 without the free-text fallback.
  - N7: the engine's live test landed on `warm` through `typesafe/jev-1.14`. The app's live test resolved the demo's decision point through the async wait to `decision -> warm via typesafe/jev-1.14`. That test had stopped working before this build because it never started play from the title; the build fixed it to enter play through New Game.

## 13. Out of scope

Keeping OpenRouter as a supported provider; a pack-level default decision pin in `pack.ron` that NPCs inherit; checking a pin against NanoGPT's list of decision models; a player's choice of decision model; a provider abstraction or a provider plugin; NanoGPT's subscription endpoint and billing modes; NanoGPT's BYOK, web search, caching, moderation and zero-data-retention options; migrating a stored OpenRouter key; changing a pack's `unmei.author.ron`; choosing a new demo pin unless N7's live call fails.
