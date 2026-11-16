---
status: accepted
title: NanoGPT is the one provider
summary: NanoGPT replaces OpenRouter for the runtime's Decisions calls and the CLI's authoring; an OpenRouter-format key is never sent.
keywords:
  - decisions
  - credentials
  - authoring-cli
  - external-dependency
---

# NanoGPT is the one provider

Unmei called OpenRouter in two places: the runtime's Decisions calls (`POST /api/alpha/decisions`) and the `unmei author` CLI's OpenAI-compatible chat calls. By 2026-11-15 OpenRouter had become a liability on cost and on content screening. NanoGPT offers a decisions endpoint with the same request body and typed answers, the demo pack's pinned model, and an OpenAI-compatible chat endpoint. It also has no moderation on by default, stores no prompts by default and bills decisions on input tokens only. Requirements and the build-ready detail are in [`docs/nanogpt.md`](../nanogpt.md).

**Decision.** NanoGPT **replaces** OpenRouter as the one provider Unmei supports, defaults to, tests and documents, for both the runtime and the CLI. The runtime keeps one concrete Decisions client, now NanoGPT's, behind the [decision resolver seam](0009-decision-resolver-seam.md). This is a replacement, not a second provider, so ADR 0009's trigger does not fire. The endpoint overrides stay as unsupported escape hatches. The key variables become `NANOGPT_API_KEY` (and `UNMEI_API_KEY` for the CLI). **No Unmei request ever carries a key that starts with `sk-or-`**, whatever its source and whatever endpoint an override names. Unmei sends no moderation, data-retention or routing options and leaves screening to the pinned model and the player's NanoGPT account.

## Considered options

- **Add NanoGPT beside OpenRouter as a second supported provider.** Rejected: it fires ADR 0009's second-provider trigger and reopens the provider abstraction that ADR declined, for a provider the project is leaving. Two providers also double the live tests, the key-row copy and the docs.
- **A provider abstraction or a provider plugin now.** Rejected for the same reason: one provider has one implementation. The overrides already cover a compatible endpoint.
- **Send a stored OpenRouter key to NanoGPT and let it fail with `401`.** Rejected: it hands an OpenRouter credential to a third party, and the player sees "rejected" with no reason.
- **Move the key to a new credential file and ignore the old one.** Rejected: the old key stays on disk unread, and nothing tells the player why decisions stopped working. Refusing an `sk-or-` key keeps one file and names the cause in the key row.
- **Opt into NanoGPT's moderation or zero data retention.** Rejected: moderation is paid, screens input only and is not listed for decisions. Zero data retention covers only some decision models, not the demo's Jev pin, so requiring it would rule that pin out.

## Consequences

- **Existing players lose live decisions until they enter a NanoGPT key.** Their stored OpenRouter key shows as **Invalid, an OpenRouter key**, and decision points take the author's fallback, as they do with no key. The file is never deleted automatically.
- **The `sk-or-` refusal is a prefix check, not a key-format check.** A key in any other shape is still sent and judged by the provider, so a future NanoGPT key format needs no change.
- **A refusal is a new failure category** that lands on the fallback with its own diagnostic, and the pack format caps a `score` legend at ten levels to match NanoGPT.
- **The provider-agnostic path stays closed.** ADR 0009's triggers are unchanged, and a later return to OpenRouter or a second provider would reopen them.
