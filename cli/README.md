# unmei

The authoring CLI for [Unmei](https://github.com/SairaDev02/unmei-vn), a visual-novel engine. It
generates the NPC decision configs of a content pack (`unmei author npc`) and
drafts scene prose (`unmei author draft`). Both commands use any
OpenAI-compatible chat endpoint. The CLI writes plain `.ron` and `.unmei` files
that you can read and edit by hand.

A content pack is the directory that holds everything one game needs. The
Guide's [pack format overview](https://github.com/SairaDev02/unmei-vn/blob/v0.1.0/docs/guide/format-overview.md) maps its
parts.

## Install

To install the CLI, run:

```sh
cargo install --locked unmei
```

To list the commands, run `unmei --help`. To print the release you are running,
run `unmei --version`.

This route needs a Rust toolchain and works on any platform Cargo supports,
including macOS, which has no archive. The Guide's
[install page](https://github.com/SairaDev02/unmei-vn/blob/v0.1.0/docs/guide/install-the-cli.md) covers the CLI archives for
Windows x64 and Linux x64, which need no Rust toolchain.

## API key

A command that calls the model needs an API key for the chat endpoint. Before
you run such a command, set the key in the environment: `UNMEI_API_KEY`, with
`OPENROUTER_API_KEY` as the fallback when `UNMEI_API_KEY` is unset or empty. The
CLI reads the key from the environment only. A command that does not call the
model needs no key, such as `unmei author draft <SCENE_ID> --list` or `--help`.

The `.ron` and `.unmei` files that `unmei` writes need no network to read, edit
or play.

## Documentation

The Guide's [home page](https://github.com/SairaDev02/unmei-vn/blob/v0.1.0/docs/guide/README.md) routes authors to the pages
for installing the CLI, writing a first pack and authoring with the CLI.

## License

MIT, as stated in `LICENSE`.

## Library

The package also contains a library beside the binary. The library is an
implementation detail of the binary and makes no stability promise. See the
crate documentation on docs.rs.
