# Interface typefaces — provenance

The app's five interface faces are **embedded in the binary** (`include_bytes!`
into `Assets<Font>` in `app/src/presentation/style.rs`), not loaded from a loose
file. The asset root is the *pack's* `assets/` directory (`app/src/main.rs`
sets `AssetPlugin::file_path` from the pack root), so the app's own faces
cannot live there. This directory is the paper trail: the five TTFs, their two
licenses, the license of Bevy's embedded fallback face, and this note. See
`docs/title-and-menu.md` § Type system.

## Faces

### IBM Plex Sans — Regular, SemiBold, Bold

- **Family**: IBM Plex Sans (SIL Open Font License 1.1, with Reserved Font Name
  "Plex"). License text: [`IBMPlexSans-license.txt`](IBMPlexSans-license.txt).
- **Source**: <https://github.com/IBM/plex>, path
  `packages/plex-sans/fonts/complete/ttf/`.
- **Pinned revision**: commit
  [`763c36ef9117782905ae010056dfbe8fd2653a25`](https://github.com/IBM/plex/commit/763c36ef9117782905ae010056dfbe8fd2653a25)
  (`763c36e`, 2026-09-22).

| File | SHA-256 |
| --- | --- |
| `IBMPlexSans-Regular.ttf` | `975dcda37d80f038dcd143c22e33ca2d97a0cc5a929aace1c749153b0fe1afa5` |
| `IBMPlexSans-SemiBold.ttf` | `a20caf8286023a6a7a85e40b1d2a4ae9fc3e3b1f9eda8f4c542dd4986af67bb1` |
| `IBMPlexSans-Bold.ttf` | `9e6c74a889a700d707613d24548fe4ffa6bc59559a0689d2cf9e133bdcdafb2f` |

### Fira Mono — Regular, Medium

- **Family**: Fira Mono (SIL Open Font License 1.1). License text:
  [`FiraMono-OFL.txt`](FiraMono-OFL.txt).
- **Source**: <https://github.com/google/fonts/tree/main/ofl/firamono>, path
  `ofl/firamono/`.
- **Retrieved revision**: commit
  [`d74f7e8db499e84477ad0af4140ad25cf573091c`](https://github.com/google/fonts/commit/d74f7e8db499e84477ad0af4140ad25cf573091c)
  (2026-03-27), the latest commit touching `ofl/firamono`.

| File | SHA-256 |
| --- | --- |
| `FiraMono-Regular.ttf` | `2e00b0cf3106a3d792f35711b7722740b76c02ee7300ea8c60f940584b7a8ac9` |
| `FiraMono-Medium.ttf` | `a5bc1a2db2f65946049de615ecc08603d788eb0c02b520f4ed95f05a5cc58532` |

### Bevy's embedded default face — `FiraMono-subset.ttf`

This face has no file here: `bevy_text` embeds it and the player never names
it. It is still in the binary while `default_font` is on (see the last section),
so its license travels with the player archive as
[`FiraMono-LICENSE`](FiraMono-LICENSE).

- **Family**: Fira Mono (SIL Open Font License 1.1), subset to ASCII by Bevy.
- **Source**: <https://github.com/bevyengine/bevy/blob/v0.19.1/assets/fonts/FiraMono-LICENSE>,
  vendored verbatim from the `v0.19.1` tag (the release the workspace resolves).
- **Why it is vendored**: the published `bevy_text` crate ships only
  `LICENSE-APACHE` and `LICENSE-MIT`, not this OFL text, so a crate-level
  license scan cannot find it.

| File | SHA-256 |
| --- | --- |
| `FiraMono-LICENSE` | `587dd9548304fd421cd254f7550bd101c1250ffc95417f2f7f92d55b44ecbe53` |

## Coverage check

The app renders printable ASCII plus these eight non-ASCII codepoints:

| Codepoint | Glyph | Where | Bundle coverage |
| --- | --- | --- | --- |
| U+00B7 | `·` | the help line, the title legend, the error-screen footer and the Settings/notice separators | covered by every bundled face |
| U+2014 | `—` | the key row's `Set — …` state line, confirm bodies, the `— end of scene —` marker | covered by every bundled face |
| U+2026 | `…` | `Checking…`, `sk-or-…a1b2` fingerprints, `Legacy save — Import…` | covered by every bundled face |
| U+2190 | `←` | the Settings back controls (`← Back to slots` / `← Back to title`) | covered by every bundled face |
| U+2191 | `↑` | the title legend (`↑/↓ move · …`) | covered by every bundled face |
| U+2193 | `↓` | the title legend (`↑/↓ move · …`) | covered by every bundled face |
| U+25B6 | `▶` | the overlay's Resume row | **fallback-only** — absent from all five faces |
| U+2699 | `⚙` | the overlay's `⚙ Settings` segment | **fallback-only** — absent from all five faces |

Verified against the shipped TTFs with `fontTools` (`TTFont.getBestCmap()`):
every face carries U+00B7, U+2014, U+2026, U+2190, U+2191 and U+2193, and
none carries U+25B6 or U+2699. Both fallback-only glyphs therefore resolve
through Bevy's `system_font_discovery` fallback to an OS font, which is
best-effort and platform-dependent; no test asserts a fallback glyph
(`docs/title-and-menu.md` § Type system §1). The test-only `é` (U+00E9) in
`credentials.rs` is not rendered and is excluded.

Bevy's embedded default font stays enabled (`default_font`) only as Bevy's own
safety net. No app string relies on it: every one names a face through
`TextFaces` (`app/src/presentation/style.rs`), one of these five handles.
