## Problem

The improved demo pack needs a redesigned visual-novel UI, and the new look also becomes the default for a default Unmei install. The current play frame draws the dialogue as bare text with a help line, the title is one centred column, and the Esc menu is a left rail with a card grid.

## Decision

A clickable prototype compared four layouts of the play frame, the title and the Esc menu over the demo pack's own script and art. Variant **E**, a mix, is final:

- **Play frame:** a dialogue box across the bottom with a speaker name tab (narration has no tab and is set in italics), a row of Save, Load, Auto, Skip, Log and Menu controls inside the box, and a "Ace is deciding…" line in the box while a decision request is in flight.
- **Player choices:** numbered buttons stacked in the centre of the screen, with the question still shown in the dialogue box.
- **Title:** the pack name, tagline and a text-only option list at the left, over a dark gradient on the left side of the pack art.
- **Esc menu:** a full-screen contents page, with the sections (Save, Load, Timeline, Gallery, Settings) listed large on the left, the save slots as a list on the right, and Resume and Exit to title at the bottom left.

Settings, Gallery and Timeline are not redesigned here; they follow the new look later.

## Prototype

The prototype is on the throwaway branch `prototype/<this issue>-vn-ui-redesign` at `app/prototype/vn-ui.html`, out of `main`. A comment on this issue links the exact branch and commit.

## Next steps

1. Amend the settled specs that E replaces, with dated amendments for review: [Title layout](https://github.com/SairaDev02/unmei-vn/blob/main/docs/title-and-menu.md#title-layout), including the rule that pack art is drawn with no veil, and [Menu overlay](https://github.com/SairaDev02/unmei-vn/blob/main/docs/runtime-hardening.md#menu-overlay) with the in-play help line.
2. Build E in the play, title and overlay surfaces against the amended specs.

## Acceptance criteria

- The amended specs describe E and record the date and the reason for each change.
- At 1280×720 the play frame, the title in all four of its states and the Esc menu match the amended specs.
- The player control keys in the README are unchanged.
