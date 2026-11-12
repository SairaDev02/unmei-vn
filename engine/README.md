# unmei-engine

`unmei-engine` is the shared core of [Unmei](https://github.com/SairaDev02/unmei-vn), a visual-novel
engine. It loads and validates content packs, parses `.unmei` scenes and runs
them, with no rendering, windowing or audio dependency of its own. Both the
Bevy player and the authoring CLI are built on this crate.

## What it does

- Loads a content pack and validates it as a whole: manifest, state map, scenes
  and NPC decision configs. Asset files are not checked for existence.
- Runs a scene: say-lines, player choices, transitions and decision points.
- Reads and writes save slots, and a read log of what the player has read.
- Resolves decision points through the OpenRouter Decisions API in the `network`
  feature, which is off by default. docs.rs builds this crate with the feature
  on.

## Documentation

- [The crate documentation on docs.rs](https://docs.rs/unmei-engine): the API
  reference, including the `network` feature and the 0.x stability notes.
- [The repository](https://github.com/SairaDev02/unmei-vn).
- [The Guide's home page](https://github.com/SairaDev02/unmei-vn/blob/v0.1.0/docs/guide/README.md).

## License

MIT, as stated in `LICENSE`.
