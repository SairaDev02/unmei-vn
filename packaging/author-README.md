# Unmei @@VERSION@@ (beta): author CLI

This page takes you from the extracted CLI archive to a first run of `unmei`, the authoring CLI for
Unmei content packs. `unmei` drafts scene prose and generates NPC decision configs through any
OpenAI-compatible chat endpoint (OpenRouter by default), validates the whole pack before every write
and leaves every other file untouched
([`docs/author-cli.md` § 3 Validation gate](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/author-cli.md#3-validation-gate-before-any-write),
[§ 2 Generation unit and write surface](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/author-cli.md#2-generation-unit-and-write-surface)).
You do not need Rust or Cargo.

> **Beta.** The pack format and the authoring config can change in any 0.x release
> ([`docs/release-certification.md` § Compatibility during 0.x](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/release-certification.md#compatibility-during-0x-decided-2026-11-10)).

## Before you start

- A Windows x64 or Linux x64 machine. For macOS, see [Other ways to install](#other-ways-to-install).
- For a command that calls the model only: an API key for your endpoint ([step 4 of Set up](#set-up)) and a
  model ID from its provider.

## Set up

1. Extract the CLI archive into a folder you can write to.

   On Windows, `unmei.exe` is unsigned. If SmartScreen shows **Windows protected your PC**, choose
   **More info**, then **Run anyway**. Where Windows 11's **Smart App Control** is on, it can block an
   unsigned program outright, with no prompt. The Guide's
   [Smart App Control section](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/guide/troubleshooting.md#smart-app-control) covers this case.
2. Open a terminal in the extracted folder, the one that holds `unmei` (`unmei.exe` on Windows) and
   `demo-pack/`.
3. Confirm that the CLI runs. This check needs no key. Where the commands on this page show `unmei`,
   type `.\unmei` in PowerShell and `./unmei` in a Linux shell.

   ```console
   $ unmei --version
   unmei @@VERSION@@
   ```
4. To run a command that calls the model, set your API key as an environment variable.
   `--version`, `--help` and `unmei author draft <SCENE_ID> --list` work without a key.

   ```powershell
   $env:UNMEI_API_KEY = "sk-or-..."
   ```

   ```sh
   export UNMEI_API_KEY=sk-or-...
   ```

   The `sh` line also works on macOS with `cargo install`. The CLI never reads a key from a flag or a
   file, so a key cannot end up committed to your pack. The variable lasts for that terminal session,
   so set it again in each terminal you open.

   `OPENROUTER_API_KEY` works too. When both are set, the CLI uses `UNMEI_API_KEY`
   ([`docs/author-cli.md` § 4 Provider configuration](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/author-cli.md#4-provider-configuration)).

## First commands, against the bundled demo pack

The first command is offline.

1. List the prose runs in a scene. A [run](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/GLOSSARY.md#scripts)
   is a block of consecutive say-lines. The command loads and checks the pack, needs no key, makes no
   network call and writes nothing
   ([`docs/author-cli-drafting.md` § 1 Command surface](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/author-cli-drafting.md#1-command-surface)).
   A clean run prints only the list and exits with code 0:

   ```console
   $ unmei author draft first_meeting --list --pack demo-pack
     start       L7     Narrator: Morning light. A folded letter on the ground, half under a shoe.
     start#2     L10    Ace: You're not from around here, are you?
     ask_letter  L21    Ace: A letter? Ah... you saw that.
     silence     L28    Ace: ...Fine, don't talk. See if I care.
   ```

   A failure prints a `✖` line and exits with code 1.
2. Preview a regenerated decision config without writing it.

   This command needs the key from step 4 of Set up and a model. It calls the model endpoint, even
   with `--dry-run`, and sends your `--brief`, `pack.glossary.md` when present, the `state.ron`
   vocabulary and, when replacing, the existing config for that question
   ([`docs/author-cli.md` § 6 Prompt inputs](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/author-cli.md#6-prompt-inputs-what-the-model-sees)).
   With the default OpenRouter endpoint, OpenRouter bills the request to your OpenRouter
   account. Another endpoint bills by its own terms.

   ```sh
   unmei author npc ace --question greet --pack demo-pack --model <MODEL> --dry-run
   ```

   Replace `<MODEL>` with a model ID from your provider, for example `openai/gpt-4o-mini`. There is
   no default authoring model
   ([`docs/author-cli.md` § 4 Provider configuration](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/author-cli.md#4-provider-configuration)).
   The command prints the generated config and then `--dry-run: nothing written`.

Every command accepts `--help`.
[`docs/author-cli.md` § 5 Failure, retry, and editing UX](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/author-cli.md#5-failure-retry-and-editing-ux)
owns the `✖` line and the exit codes.

To avoid repeating `--model` and `--base-url`, you can save them in an optional `unmei.author.ron` at
your pack root:

```ron
// unmei.author.ron — optional, at the pack root
(
    base_url: "https://openrouter.ai/api", // optional; the CLI appends /v1/chat/completions
    model: "openai/gpt-4o-mini",           // authoring chat model; there is no default
)
```

When neither `--base-url` nor `base_url` is set, the CLI uses OpenRouter (`https://openrouter.ai/api`).
You must give a model: pass `--model` or set `model` in `unmei.author.ron`.

## Your own pack

Copy `demo-pack/` and edit the copy. The parts of a pack are
([`docs/content-format.md` § 2 Pack layout](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/content-format.md#2-pack-layout)):

- `pack.ron` is the manifest
  ([§ 6 Manifest and state](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/content-format.md#6-manifest-and-state)).
- `scenes/*.unmei` hold the script
  ([§ 3 Dialogue DSL](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/content-format.md#3-dialogue-dsl-unmei)).
- `npcs/*.ron` hold the NPCs and their decision configs
  ([§ 4 Decision configs and decision points](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/content-format.md#4-decision-configs-and-decision-points)).
- `state.ron` declares the state map
  ([§ 6 Manifest and state](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/content-format.md#6-manifest-and-state)).
- `assets/` holds the art and audio.

To play your pack, pass its folder to the player, which is a separate download from
[releases](https://github.com/SairaDev02/unmei-vn/releases). Run the player from its own folder and replace `<PACK_DIR>`
with the path of your pack folder:

```powershell
.\unmei-play <PACK_DIR>
```

```sh
./unmei-play <PACK_DIR>
```

The Guide has step-by-step pages:
[Install the Unmei authoring CLI](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/guide/install-the-cli.md),
[Write your first Unmei pack](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/guide/write-your-first-pack.md) and
[Use the authoring CLI](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/guide/author-with-the-cli.md), which covers `author npc` and
`author draft`. The Guide's [home page](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/guide/README.md) links the content format overview.

If an AI coding agent edits your pack, point it at `demo-pack/AGENT-GUIDE.md`.

## Other ways to install

If you have a Rust toolchain, `cargo install --locked unmei` installs the CLI on any platform Cargo
supports. macOS has no archive, so this is its only route. There is no installer.

## Reporting a problem

Report bugs at https://github.com/SairaDev02/unmei-vn/issues, with what `unmei --version` prints and the command you ran.

## Licenses

Unmei is MIT-licensed. The text is in `LICENSE`. Third-party code notices are in
`THIRD-PARTY-LICENSES`.
