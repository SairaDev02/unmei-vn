Prototype for a demo-pack mood on top of variant E: branch [`prototype/8-vn-visual-language`](https://github.com/SairaDev02/unmei-vn/tree/prototype/8-vn-visual-language) at commit 8ff3fdedd4516070e43d50ef8a6aa4e4a386c1a8. It branches from `prototype/8-vn-ui-redesign`.

Question: which mood (colour, type, shape, depth, ornament) should the demo pack wear over the settled variant E layout? E's geometry is not reopened.

Scope: a mood is for the **demo pack only**. The default look of a default install stays the neutral variant E, so authors can have their own agent develop their pack's UI. Two things gate any choice:

- **The demo pack's story rewrite.** A mood has to follow the story's genre and setting. These three were fitted to the current story (the letter, the school gate, Ace's decision).
- **A per-pack styling mechanism.** It does not exist yet: `pack.ron` has no field for a pack's own look, and every colour and font is fixed in `app/`. Adding one changes the pack format.

Contents:

- `app/prototype/visual-language/{gothic,cottagecore,aero}/DESIGN.md`: three moods in the [DESIGN.md](https://github.com/google-labs-code/design.md) format, based on [StyleShift's mood styles](https://styleshift.design/styles). Each passes `designmd lint` with no errors or warnings. Prototype rule: no purple (no hue 250–330°).
- `app/prototype/vn-visual.html`: one mockup that reads the three files and renders them on E's title, play frame and Esc menu.
- The first round (Washi, Dusk, Pop) was rejected as too generic. It stays in the history at d3d2852.

To run it, check out the branch, run `python -m http.server 8137` from the repository root and open `http://localhost:8137/app/prototype/vn-visual.html?variant=A` (`A` Gothic, `B` Cottagecore, `C` Aero). It needs a network connection: the moods' fonts load from Google Fonts. Press T for the token panel.

No verdict yet. Other costs for any winner: one to three OFL fonts shipped with the demo pack, and Cottagecore's tilts need UI rotation in Bevy 0.19, which is not yet confirmed.
