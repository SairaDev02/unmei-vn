---
title: Install the Unmei authoring CLI
summary: Get the CLI from its archive or with cargo install --locked unmei, set the API key environment variable and give the CLI a model to talk to.
keywords:
  - authoring-cli
  - credentials
  - content-pack
---
# Install the Unmei authoring CLI

This page installs the `unmei` CLI, then gives it an API key and an authoring model. `unmei` is the
authoring tool for a [content pack](../../GLOSSARY.md#content--packs) on your disk. It drafts scene
prose and generates NPC decision configs through any OpenAI-compatible chat endpoint, validates the
whole pack before it writes and leaves every other file untouched.

## Before you start

- A terminal.
- For the Cargo route only: a Rust toolchain. The archive route needs no Rust.
- An API key for the chat endpoint. The default endpoint is OpenRouter, and
  [`live-decisions.md` § Get an OpenRouter key](live-decisions.md#get-an-openrouter-key) shows how to
  get one. Installing the CLI and checking its version need no key.
- The ID of a chat model that the endpoint serves, such as `openai/gpt-4o-mini`.

Do the sections that follow in order.

## Two ways to get `unmei`

Choose one route.

- **The CLI archive**: a Windows x64 or Linux x64 download.
  1. Download the archive for your platform from [releases](https://github.com/SairaDev02/unmei-vn/releases), for the
     release you are using.
  2. Extract it. There is nothing to install: you run `unmei` from the extracted folder.

  The archive also carries `demo-pack/`, which the Guide's authoring pages use. On Windows the executable is
  unsigned, and the `README` inside the archive covers the warning screens. The Guide's
  [Smart App Control section](troubleshooting.md#smart-app-control) covers the case where Windows
  refuses to start an unsigned program without a prompt.
- **`cargo install --locked unmei`**: needs a Rust toolchain and works on any platform Cargo
  supports. This is the route for macOS, which has no archive.
  Cargo installs no demo pack. To follow the Guide's authoring pages, take `demo-pack/` from either
  CLI archive: the pack is the same in both.

  ```sh
  cargo install --locked unmei
  ```

## Check that it runs

To check the install, open a terminal in the extracted folder (archive route) or in any folder
(Cargo route). The command needs no key. For release 0.1.0 it prints:

```console
$ unmei --version
unmei 0.1.0
```

Neither shell searches the current folder for programs by default. When you run the archive's
executable from its folder, type `.\unmei` in PowerShell or `./unmei` in `sh` where these pages show
`unmei`.

Every command also takes `-h` / `--help`. The full command surface and its rules are
[`docs/author-cli.md` § 1 Command surface](../author-cli.md#1-command-surface). The drafting
subcommand's own surface is
[`docs/author-cli-drafting.md` § 1 Command surface](../author-cli-drafting.md#1-command-surface).

## Set the API key

The CLI reads the key from the environment only, never from a flag or a file, so a key cannot end up
committed with a pack. In the terminal where you run the CLI, set the variable:

```powershell
$env:UNMEI_API_KEY = "sk-or-..."
```

```sh
export UNMEI_API_KEY=sk-or-...
```

The variable lasts for that terminal session: closing the terminal forgets it. Set it again in each
new terminal before a command that calls the model.

`OPENROUTER_API_KEY` works too, and `UNMEI_API_KEY` wins when both are set. The precedence, the exact
names and what a missing key does are in
[`docs/author-cli.md` § 4 Provider configuration](../author-cli.md#4-provider-configuration).

## Give it a model

There is no default authoring model. The authoring model is the chat model that generates decision
configs and drafts prose. Give the CLI one in either of two ways:

- Pass `--model <MODEL>` on the command.
- Put the model in an optional `unmei.author.ron` at the pack root, the folder that holds
  `pack.ron`, so you do not repeat it:

  ```ron
  // unmei.author.ron — optional, at the pack root
  (
      base_url: "https://openrouter.ai/api", // optional; the CLI appends /v1/chat/completions
      model: "openai/gpt-4o-mini",           // required unless --model is passed: there is no default model
  )
  ```

A flag beats the same setting in that file. Both are in
[`docs/author-cli.md` § 4 Provider configuration](../author-cli.md#4-provider-configuration).

With neither a key nor a model the CLI stops before any request. On the archive route, you can see
this: in the extracted folder, which holds `demo-pack/`, run:

```console
$ unmei author npc ace --question greet --pack demo-pack
✖ no authoring model set — pass --model <id> or set `model` in unmei.author.ron; the authoring model must serve OpenAI chat/completions (decision models like typesafe/jev reject that endpoint)
```

`unmei.author.ron` configures the authoring model, which generates decision configs. The decision
model, which answers decision points during play, is pinned per NPC in `npcs/<id>.ron`, and the two
are deliberately separate. See
[`docs/author-cli.md` § 4 Provider configuration](../author-cli.md#4-provider-configuration) and, for
the per-NPC pin,
[`docs/content-format.md` § 4 Decision configs and decision points](../content-format.md#4-decision-configs-and-decision-points).

## Next

- [`write-your-first-pack.md`](write-your-first-pack.md): copy `demo-pack/` and validate an edit.
- [`author-with-the-cli.md`](author-with-the-cli.md): the worked `author npc` and `author draft`
  flows.
- [`format-overview.md`](format-overview.md): what each part of a pack is.

Guide home: [`README.md`](README.md), with the routes for players, authors and contributors.
