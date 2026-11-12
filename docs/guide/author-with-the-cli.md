---
title: Use the authoring CLI
summary: List a scene's runs, draft prose into one run, generate or replace a decision config, preview each with a dry run and go interactive with unmei author.
keywords:
  - authoring-cli
  - dialogue-dsl
  - pack-validation
  - credentials
---
# Use the authoring CLI

These flows use `unmei author` to list the runs in a scene, draft prose into one run and generate
or replace one NPC decision config. They also show what the CLI prints when the key or the model is
missing and when the model's answer is rejected.

## Before you start

- Install `unmei` as in [`install-the-cli.md`](install-the-cli.md) and open a terminal in the folder
  that holds `unmei` and `demo-pack/`, the pack these flows use. On the archive route, type
  `.\unmei` in PowerShell or `./unmei` in `sh` where a command shows `unmei`
  ([`install-the-cli.md` § Check that it runs](install-the-cli.md#check-that-it-runs)).
- To run a command that calls the model, set an API key and give the CLI a model, as in
  [`install-the-cli.md` § Set the API key](install-the-cli.md#set-the-api-key) and
  [`install-the-cli.md` § Give it a model](install-the-cli.md#give-it-a-model). `--list` needs
  neither. The draft and generate commands call the model even with `--dry-run`. The model provider
  is the endpoint set by `--base-url` or by `base_url` in `unmei.author.ron`, and the default is
  OpenRouter ([`docs/author-cli.md` § 4 Provider configuration](../author-cli.md#4-provider-configuration)).
- Use a pack you can restore. A command without `--dry-run` writes into the pack, and the CLI keeps no
  backup files. Copy `demo-pack` first
  ([`write-your-first-pack.md` § 1 Copy the demo pack](write-your-first-pack.md#1-copy-the-demo-pack)),
  or keep the pack under git. The examples keep the name `demo-pack`, so replace it with your copy's
  name.

## What the two subcommands do

`unmei author` has two subcommands:

- `unmei author npc` produces one decision config: the block in an NPC file that defines one
  question that the NPC's decision model answers
  ([`docs/content-format.md` § 4 Decision configs and decision points](../content-format.md#4-decision-configs-and-decision-points)).
- `unmei author draft` produces one run of scene prose. A *say-line* is a line of the form
  `Speaker: text`, and a *run* is a maximal contiguous block of say-lines. Any line that is not a
  say-line, such as a command or a `label`, ends a run
  ([`docs/author-cli-drafting.md` § 2](../author-cli-drafting.md#2-generation-unit-and-write-surface)).

Each subcommand produces a candidate, validates it, previews it and then writes it. The command
surfaces are
[`docs/author-cli.md` § 1 Command surface](../author-cli.md#1-command-surface) and
[`docs/author-cli-drafting.md` § 1 Command surface](../author-cli-drafting.md#1-command-surface).

Each subcommand writes one unit and nothing else: one question's config in `npcs/<id>.ron`, or one
run's say-lines in `scenes/<id>.unmei`. Before it writes, each subcommand runs the whole pack through
the validation that the pack loader applies
([`docs/author-cli.md` § 2](../author-cli.md#2-generation-unit-and-write-surface),
[`docs/author-cli.md` § 3 Validation gate](../author-cli.md#3-validation-gate-before-any-write),
[`docs/author-cli-drafting.md` § 4](../author-cli-drafting.md#4-validation-gate-before-any-write)).

## List the runs in a scene

To see the runs of a scene and the handle of each, run `--list`. It makes no network call, needs no
key or model and writes nothing:

```console
$ unmei author draft reunion --list --pack demo-pack
  start       L7     Narrator: The gate again, hours later. She's still there.
  reply.warm  L14    Ace: You came back! I... hoped you would.
  reply.cold  L19    Ace: Oh. It's you.
  reply.none  L24    Narrator: Ace watches you approach, unreadable.
```

Each line shows a run's handle, its line span (`L7`, or `L7-L10` for a run over several lines) and its first say-line, in scene order. The handle is
what `--from` takes: `start` or a label name, each with an optional `#n` ordinal. When a scene has
several runs under one name, an ordinal addresses the later ones (`#2` for the second)
([`docs/author-cli-drafting.md` § 2](../author-cli-drafting.md#2-generation-unit-and-write-surface)).

## Draft prose into one run

To draft a run, pick its handle from
[List the runs in a scene](#list-the-runs-in-a-scene), then follow these steps in order.

1. Preview the draft with `--dry-run`. The command below calls the model even with `--dry-run`. It
   sends the whole target scene, every NPC file in the pack, `pack.glossary.md` when present, the
   `state.ron` key vocabulary with its current values and your `--brief` to the model provider.
   With the default OpenRouter endpoint, OpenRouter bills the request to your OpenRouter account.
   Another endpoint bills by its own terms
   ([`docs/author-cli-drafting.md` § 3 Context assembly](../author-cli-drafting.md#3-context-assembly-what-the-model-sees)).

   ```sh
   unmei author draft reunion --from reply.warm --brief "Ace is guarded but pleased the player came back." --pack demo-pack --model openai/gpt-4o-mini --dry-run
   ```

   The CLI prints the drafted lines and writes nothing. If validation rejects the draft, see
   [When the model's answer is rejected](#when-the-models-answer-is-rejected).

2. Write the run. Two warnings apply before you run the command. It calls the model again, so it
   sends the same data and incurs the cost again. It also replaces the say-lines of the run in
   `scenes/reunion.unmei`, including any hand edits and `#` comments inside the run, while every
   line outside the run stays byte for byte as it was
   ([`docs/author-cli-drafting.md` § 6 Regeneration policy](../author-cli-drafting.md#6-regeneration-policy)).
   Then run the same command without `--dry-run`.

Optionally, add `-i` to the command in step 2 for a guided session. The session previews the draft,
takes free-text feedback and asks for a new draft until you `accept` or `discard`. `accept` writes
the run, so the warning in step 2 applies. `discard` writes nothing.

For what the model may emit (say-lines only, no control flow, known speakers), what a re-draft
replaces, and how retries work, see
[`docs/author-cli-drafting.md` § 2](../author-cli-drafting.md#2-generation-unit-and-write-surface),
[§ 4](../author-cli-drafting.md#4-validation-gate-before-any-write),
[§ 5](../author-cli-drafting.md#5-failure-retry-and-editing-ux),
[§ 6](../author-cli-drafting.md#6-regeneration-policy) and
[§ 7 Provider and model output](../author-cli-drafting.md#7-provider-and-model-output).

## Generate or replace a decision config

To generate a config for one question of an NPC, or to replace it, follow these steps in order.

1. Preview the config with `--dry-run`. The command below calls the model even with `--dry-run`. It
   sends your `--brief`, `pack.glossary.md` when present, the `state.ron` key vocabulary and, when you
   replace a config, the existing config for that question to the model provider. With the default OpenRouter endpoint, OpenRouter bills the request to your OpenRouter account.
   Another endpoint bills by its own terms
   ([`docs/author-cli.md` § 6 Prompt inputs](../author-cli.md#6-prompt-inputs-what-the-model-sees)).

   ```sh
   unmei author npc ace --question greet --pack demo-pack --model openai/gpt-4o-mini --dry-run
   ```

   The CLI prints the generated RON and writes nothing.

2. Write the config. Two warnings apply before you run the command. It calls the model again, so it
   sends the same data and incurs the cost again. It also adds or replaces only
   `decisions["greet"]` in `npcs/ace.ron`: a `greet` config that already exists is replaced, and the
   rest of the file, comments included, stays as it was
   ([`docs/author-cli.md` § 1 Command surface](../author-cli.md#1-command-surface)).
   Then run the same command without `--dry-run`.

Optionally, add these options to the command:

- `--create` starts a new NPC file. It stops with an error when the file already exists.
- `--type choice|noul|score` picks the question type. The default is `choice`, and a replaced config
  keeps its existing type unless you pass `--type`.
- `--brief` and `--brief-file` carry your intent, the second one from a file.
- `-i` runs the guided session. It previews the config, takes free-text feedback and regenerates
  until you `accept` or `discard`. `accept` writes the config, so the warning in step 2
  applies. `discard` writes nothing.

For the write unit, the shape of each question type, and the fallback and label-closure checks, see
[`docs/author-cli.md` § 2](../author-cli.md#2-generation-unit-and-write-surface) and
[`docs/author-cli.md` § 3](../author-cli.md#3-validation-gate-before-any-write). A full worked run is
[`docs/author-cli.md` § 7 Worked example](../author-cli.md#7-worked-example).

## When the key or the model is missing

A key or a model that is not set is an environment or settings problem, not a problem in the pack.
The CLI stops before it opens a connection and prints what to set.

If no model is set, pass `--model <MODEL>` or set `model` in `unmei.author.ron`:

```console
$ unmei author npc ace --question greet --pack demo-pack
✖ no authoring model set — pass --model <id> or set `model` in unmei.author.ron; the authoring model must serve OpenAI chat/completions (decision models like typesafe/jev reject that endpoint)
```

If no key is set, export `UNMEI_API_KEY` or `OPENROUTER_API_KEY`. The CLI accepts a key only from the
environment, never from a flag or a file:

```console
$ unmei author npc ace --question greet --pack demo-pack --model openai/gpt-4o-mini
✖ no authoring API key — export UNMEI_API_KEY (or OPENROUTER_API_KEY); keys are environment-only and never accepted from a flag or file
```

[`install-the-cli.md`](install-the-cli.md) explains both settings, and
[`docs/author-cli.md` § 4 Provider configuration](../author-cli.md#4-provider-configuration) holds
their rules.

## When the model's answer is rejected

When a candidate fails validation, the CLI makes at most 3 attempts. Between attempts it feeds the
validation errors back to the model as corrections. If the third attempt still fails, the CLI prints
the best raw output and the errors, and it writes nothing, so the pack is unchanged.

A dry run and an interactive session that ends in `discard` also write nothing. To undo an accepted
write, restore the file with git: the CLI keeps no backups
([`docs/author-cli.md` § 5 Failure, retry, and editing UX](../author-cli.md#5-failure-retry-and-editing-ux),
[`docs/author-cli-drafting.md` § 5](../author-cli-drafting.md#5-failure-retry-and-editing-ux)).

## Next

- [`format-overview.md`](format-overview.md): where each file you are editing fits.
- [`write-your-first-pack.md`](write-your-first-pack.md): a pack to run these flows against.
- [`install-the-cli.md`](install-the-cli.md): the archive, the key and the model.

Guide home: [`README.md`](README.md), with the routes for players, authors and contributors.
