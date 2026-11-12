# Unmei

Unmei is a Rust visual-novel engine where the response of a non-player character (NPC) can be
decided at play time. This page explains the idea and points a player, an author and a contributor
to where to start.

A scene's [decision point](GLOSSARY.md#decisions) (a `decide` line) asks the
[OpenRouter](https://openrouter.ai) Decisions API to judge the [state map](GLOSSARY.md#scripts)
against that NPC's authored criteria. An authored [fallback](GLOSSARY.md#decisions) keeps the
[content pack](GLOSSARY.md#content--packs) playable offline. A model deciding at play time is
optional: [live decisions](docs/guide/live-decisions.md) covers the OpenRouter API key, where it goes
and what a request costs.

**0.1.0 is a beta.** Formats can change in any 0.x release
([`docs/release-certification.md` § Compatibility during 0.x](docs/release-certification.md#compatibility-during-0x-decided-2026-11-10)). An older version refuses a save that a newer version
wrote ([`docs/runtime-hardening.md` § Save slots](docs/runtime-hardening.md#save-slots)). The pack loader
refuses a pack that declares a field it does not know
([`docs/content-format.md` § 6 Manifest and state](docs/content-format.md#6-manifest-and-state)).

The archives support two platforms: **Windows x64** and **Linux x64**. Each release publishes a
player archive and a CLI archive for each of them on [releases](https://github.com/SairaDev02/unmei-vn/releases). There
is no macOS archive and no installer, and the Windows executables are not signed.

## Play

On Linux, the player needs a working Vulkan driver and a few system libraries.
[`packaging/player-README.md` § Linux requirements](packaging/player-README.md#linux-requirements)
lists them.

To play the demo:

1. Download the player archive for your platform from [releases](https://github.com/SairaDev02/unmei-vn/releases).
2. Extract the whole archive into a folder of its own.
3. Start `unmei-play` (on Windows, `unmei-play.exe`).

[Play the Unmei demo](docs/guide/play-the-demo.md) walks through the archive, the Windows warning for
the unsigned beta, the first launch and saving and loading in the app. The `README` inside the
player archive is the offline copy. It also covers Linux requirements, live decisions, saves and
upgrading, and reporting a problem.

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

## Write

The authoring CLI, `unmei`, comes as a CLI archive in the same release as the player archive, or from Cargo (Rust's
package manager). Cargo needs a Rust toolchain, and on macOS, which has no archive, it is the only
route. To install the CLI with Cargo, run:

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

### Run from source

Before you build from source, run the environment doctor to check the prerequisites.
[`CONTRIBUTING.md` § Local environment doctor](CONTRIBUTING.md#local-environment-doctor) describes
each check.

```powershell
python scripts/doctor.py
```

```sh
python3 scripts/doctor.py
```

To play the repository's demo pack from source, run:

```sh
cargo run -p unmei-app -- examples/demo-pack
```

Pass the pack path after `--`: `cargo run` builds the executable into `target/`, which has no
`demo-pack` beside it, so the player cannot find its default pack.

To list the runs of the demo pack's `first_meeting` scene, run the authoring CLI's `--list`.
It needs no API key, makes no network call and writes nothing
([`docs/author-cli-drafting.md` § 1 Command surface](docs/author-cli-drafting.md#1-command-surface)):

```sh
cargo run -p unmei -- author draft first_meeting --list --pack examples/demo-pack
```

Unmei has three crates: the engine, the authoring CLI and the player. The engine does not depend on
[Bevy](https://bevyengine.org), which the player is built on.
[`docs/architecture.md`](docs/architecture.md) owns the crates' roles, their boundaries and the two
data flows.

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

## AI use in this project

- **Development:** most of the code, tests and documentation were generated with AI coding agents
  under the maintainer's direction, and the maintainer reviewed all of it before it was merged.
- **Play time:** with your own OpenRouter key, a model picks one of the actions the author wrote for
  a decision point, and it writes no text
  ([`docs/guide/live-decisions.md` § What a live decision sends](docs/guide/live-decisions.md#what-a-live-decision-sends)).
  With no key, play uses the authored fallback.
- **Authoring:** the `unmei` CLI can draft scene text with a model. It prints the draft before it writes
  it to the scene
  ([`docs/author-cli-drafting.md` § 5 Failure, retry and editing UX](docs/author-cli-drafting.md#5-failure-retry-and-editing-ux)).
  The interactive session writes only on accept.

The maintainer is responsible for everything in this repository.

## License

[MIT](LICENSE)
