Prototype for the visual language on top of variant E: branch [`prototype/8-vn-visual-language`](https://github.com/SairaDev02/unmei-vn/tree/prototype/8-vn-visual-language) at commit bca372f17dfe03c29e23eb2fabf316008d4ac371. It branches from `prototype/8-vn-ui-redesign`.

Question: which mood (colour, type, shape, depth, ornament) should dress the settled variant E layout? E's geometry is not reopened.

Contents:

- `app/prototype/visual-language/{gothic,cottagecore,aero}/DESIGN.md`: three moods in the [DESIGN.md](https://github.com/google-labs-code/design.md) format, based on [StyleShift's mood styles](https://styleshift.design/styles). Each passes `designmd lint` with no errors or warnings. Prototype rule: no purple (no hue 250–330°).
- `app/prototype/vn-visual.html`: one mockup that reads the three files and renders them on E's title, play frame and Esc menu.
- The first round (Washi, Dusk, Pop) was rejected as too generic. It stays in the history at d3d2852.

To run it, check out the branch, run `python -m http.server 8137` from the repository root and open `http://localhost:8137/app/prototype/vn-visual.html?variant=A` (`A` Gothic, `B` Cottagecore, `C` Aero). It needs a network connection: the moods' fonts load from Google Fonts. Press T for the token panel.

No verdict yet. Open costs for any winner: new bundled OFL fonts (one to three per mood), and Cottagecore's tilts need UI rotation in Bevy 0.19, which is not yet confirmed.
