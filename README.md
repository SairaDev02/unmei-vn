# Unmei

[![Release](https://shieldcn.dev/github/SairaDev02/unmei-vn/release.svg)](https://github.com/SairaDev02/unmei-vn/releases)
[![CI](https://shieldcn.dev/github/SairaDev02/unmei-vn/ci.svg)](https://github.com/SairaDev02/unmei-vn/actions)
[![License](https://shieldcn.dev/github/SairaDev02/unmei-vn/license.svg)](LICENSE)
[![Rust 1.98](https://shieldcn.dev/badge/rust-1.98-orange.svg?logo=rust)](rust-toolchain.toml)
[![Platforms: Windows and Linux](https://shieldcn.dev/badge/platform-Windows_%7C_Linux-blue.svg)](https://github.com/SairaDev02/unmei-vn/releases)

Unmei is a Rust visual-novel engine. In Unmei, the response of a non-player character (NPC) can
be decided at play time.

## Start here

| You want to | Go to |
| --- | --- |
| Write a content pack with an AI coding agent | [Write with an AI coding agent](#write-with-an-ai-coding-agent) |
| Play the demo | [Play](#play) |
| Write a content pack by hand | [Write by hand](#write-by-hand) |
| Set up Unmei for development with an AI coding agent | [Set up with an AI coding agent](#set-up-with-an-ai-coding-agent) |
| Build Unmei from source or change it | [Contribute](#contribute) |

## Write with an AI coding agent

Use this section to have your own AI coding agent set up Unmei on your system, so you can start
writing a [content pack](GLOSSARY.md#content--packs): the folder that holds everything one game
needs to play. The agent installs the authoring CLI,
`unmei`, and gives you a copy of the demo pack to change.

The agent gets the CLI over the network, copies the demo pack and writes an `AGENTS.md` note in
the folder you start it in.

1. Copy this setup prompt into the agent. The prompt points the agent at the Guide pages and
   tells it which steps to leave to you.

   ```text
   Set up Unmei authoring in this folder. I am writing a content pack for the Unmei visual-novel engine.

   1. Install the unmei CLI by following https://github.com/SairaDev02/unmei-vn/blob/v0.2.0/docs/guide/install-the-cli.md. If a Rust toolchain is installed, use the Cargo route. Otherwise, download the CLI archive for this platform from https://github.com/SairaDev02/unmei-vn/releases and from no other source. If Windows blocks the unsigned executable, stop and tell me. Do not work around the warning.
   2. Get demo-pack/ as that page describes. On the Cargo route, it comes from a CLI archive. Ask me for a name for my pack, copy demo-pack/ to a new folder with that name and edit only the copy.
   3. Add a note to AGENTS.md in this folder, beside the pack and not inside it. Create the file if it does not exist. The note says: before you edit the pack, read the AGENT-GUIDE.md inside the pack folder.
   4. Never read, ask for or write a NanoGPT API key. Before any step that needs a key, stop and point me to https://github.com/SairaDev02/unmei-vn/blob/v0.2.0/docs/guide/live-decisions.md#get-a-nanogpt-key.
   ```

2. Ask the agent to change the pack:

   ```text
   In the first_meeting scene of my pack, add a third option to the choice block that hands the letter back, and add the label it points to. Then validate the whole pack and show me the result.
   ```

## How a decision works

- A scene can hold a [decision point](GLOSSARY.md#decisions): a `decide` line.
- A decision point asks the [NanoGPT](https://nano-gpt.com) Decisions API to judge the
  [state map](GLOSSARY.md#scripts) against that NPC's authored criteria.
- An authored [fallback](GLOSSARY.md#decisions) keeps the
  [content pack](GLOSSARY.md#content--packs) playable offline.
- A model deciding at play time is optional. [Live decisions](docs/guide/live-decisions.md)
  covers the NanoGPT API key, where it goes and what a request costs.

## Beta limits

**0.2.0 is a beta.** Three limits apply:

- Formats can change in any 0.x release
  ([`docs/release-certification.md` § Compatibility during 0.x](docs/release-certification.md#compatibility-during-0x-decided-2026-11-10)).
- An older version refuses a save that a newer version wrote
  ([`docs/runtime-hardening.md` § Save slots](docs/runtime-hardening.md#save-slots)).
- The pack loader refuses a pack that declares a field it does not know
  ([`docs/content-format.md` § 6 Manifest and state](docs/content-format.md#6-manifest-and-state)).

## Platforms

- **Windows x64 and Linux x64:** each release publishes a player archive and a CLI archive for
  each of them on [releases](https://github.com/SairaDev02/unmei-vn/releases).
- **macOS:** there is no macOS archive.
  [Write by hand](#write-by-hand) covers the CLI on macOS.
- **Not offered:** an installer or signed Windows executables.

## Play

Before you start, on Linux only: the player needs a working Vulkan driver and a few system
libraries.
[`packaging/player-README.md` § Linux requirements](packaging/player-README.md#linux-requirements)
lists them.

To play the demo:

1. Download the player archive for your platform from [releases](https://github.com/SairaDev02/unmei-vn/releases).
2. Extract the whole archive into a folder of its own.
3. Start `unmei-play` (on Windows, `unmei-play.exe`).

For more help:

- [Play the Unmei demo](docs/guide/play-the-demo.md) walks through the archive, the Windows
  warning for the unsigned beta, the first launch and saving and loading in the app.
- The `README` inside the player archive is the offline copy. It also covers Linux requirements,
  live decisions, saves and upgrading, and reporting a problem.

### Controls

In play, the controls are:

| Keys | Action |
|---|---|
| Space, Enter or left-click | Advance the line |
| 1–9, or click a choice | Pick a choice |
| Tab | Skip lines you have already read |
| Hold Ctrl | Skip every line, read or not |
| A | Auto mode on or off |
| F11 | Fullscreen |
| Esc | Menu overlay: Save, Load, Timeline, Gallery, Settings |

These keys are the player control contract, fixed by
[`docs/runtime-hardening.md` § Player comfort](docs/runtime-hardening.md#player-comfort),
[§ Text skip](docs/runtime-hardening.md#text-skip) and
[§ Menu overlay](docs/runtime-hardening.md#menu-overlay).

## Write by hand

The authoring CLI is `unmei`. Get it by one of two routes:

- **CLI archive:** in the same release as the player archive.
- **Cargo** (Rust's package manager): needs a Rust toolchain. On macOS, which has no archive, it
  is the only route. To install the CLI with Cargo, run:

  ```sh
  cargo install --locked unmei
  ```

The author pages of the [Guide](docs/guide/README.md) cover the rest:

- [Install the Unmei authoring CLI](docs/guide/install-the-cli.md): the two routes, the API key
  environment variable and the model the CLI talks to.
- [Write your first pack](docs/guide/write-your-first-pack.md): copy the demo pack, change a
  line, add a choice and validate the result.
- [Format overview](docs/guide/format-overview.md): what each file in a pack is, and the section
  that owns it.
- [Author with the CLI](docs/guide/author-with-the-cli.md): the worked `author npc` and
  `author draft` flows.

## Contribute

### Set up with an AI coding agent

To get a clone ready for development with an AI coding agent, copy this prompt into the agent. The
agent clones the repository and downloads Rust crates over the network, and it asks you before it
installs anything or changes your Git configuration.

```text
Set up the Unmei repository for development in this folder. Clone https://github.com/SairaDev02/unmei-vn.git and work from the repository root, in a POSIX shell (on Windows, Git Bash). Run python3 scripts/doctor.py (use python if there is no python3 command), and for each WARN or FAIL line, show me the command or document the doctor prints and ask me before you install anything or change any Git configuration, including git config core.hooksPath .githooks. Then read AGENTS.md and CONTRIBUTING.md and follow them for the rest of the session. Run cargo test --locked --workspace once and report what passed and what failed. Do not change any file until I name the accepted issue to work on, and never read, ask for or write a NanoGPT API key.
```

### Run from source

1. Run the environment doctor to check the prerequisites.
   [`CONTRIBUTING.md` § Local environment doctor](CONTRIBUTING.md#local-environment-doctor)
   describes each check.

   ```powershell
   python scripts/doctor.py
   ```

   ```sh
   python3 scripts/doctor.py
   ```

2. Play the repository's demo pack:

   ```sh
   cargo run -p unmei-app -- examples/demo-pack
   ```

   Pass the pack path after `--`. `cargo run` builds the executable into `target/`, which has no
   `demo-pack` beside it, so without the path the player cannot find its default pack.

3. Optional: list the runs of the demo pack's `first_meeting` scene with the authoring CLI's
   `--list`. It needs no API key, makes no network call and writes nothing
   ([`docs/author-cli-drafting.md` § 1 Command surface](docs/author-cli-drafting.md#1-command-surface)).

   ```sh
   cargo run -p unmei -- author draft first_meeting --list --pack examples/demo-pack
   ```

Unmei has three crates: the engine, the authoring CLI and the player. The engine does not depend
on [Bevy](https://bevyengine.org), which the player is built on.
[`docs/architecture.md`](docs/architecture.md) owns the crates' roles, their boundaries and the
two data flows.

### Read order

Read these in order:

1. This file
2. [`GLOSSARY.md`](GLOSSARY.md): the vocabulary each area uses
3. [`docs/architecture.md`](docs/architecture.md): the crates, the data flows and the contract
   map
4. [`CONTRIBUTING.md`](CONTRIBUTING.md): the contribution workflow and the local checks a change must pass
5. [`docs/INDEX.md`](docs/INDEX.md): the index of every contract and decision record
6. [`docs/adr/`](docs/adr/): the decision records, for the area you are changing

## Community

- [`CONTRIBUTING.md`](CONTRIBUTING.md): how to report an issue and propose a change
- [`SECURITY.md`](SECURITY.md): how to report a vulnerability and which versions get fixes
- [Issue tracker](https://github.com/SairaDev02/unmei-vn/issues): where work in progress is tracked

<a href="https://github.com/SairaDev02/unmei-vn/issues">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://shieldcn.dev/chart/github/issues/SairaDev02/unmei-vn.svg?theme=blue&amp;mode=dark">
    <img alt="Line chart of the total number of issues opened on Unmei over time" src="https://shieldcn.dev/chart/github/issues/SairaDev02/unmei-vn.svg?theme=blue&amp;mode=light">
  </picture>
</a>

## AI use in this project

- **Development:** most of the code, tests and documentation were generated with AI coding agents
  under the maintainer's direction. The maintainer reviewed all of it before it was merged.
- **Play time:** with your own NanoGPT key, a model picks one of the actions the author wrote for
  a decision point. The model writes no text
  ([`docs/guide/live-decisions.md` § What a live decision sends](docs/guide/live-decisions.md#what-a-live-decision-sends)).
  With no key, play uses the authored fallback.
- **Authoring:** the `unmei` CLI can draft scene text with a model. It prints the draft before it
  writes it to the scene
  ([`docs/author-cli-drafting.md` § 5 Failure, retry and editing UX](docs/author-cli-drafting.md#5-failure-retry-and-editing-ux)).
  The interactive session writes only on accept.

The maintainer is responsible for everything in this repository.

## License

[MIT](LICENSE)
