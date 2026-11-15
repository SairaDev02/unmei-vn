---
version: alpha
name: Unmei Gothic
description: PROTOTYPE demo-pack mood A, over the variant E layout. Dark romance after StyleShift's Gothic mood, with its violet removed. Stone, candlelight and crimson; lancet arches and Roman numerals.
colors:
  primary: "#8E1B24"
  primary-bright: "#A3232E"
  candle: "#D9B76E"
  gold: "#C9A45C"
  void: "#0B0908"
  stone: "#1C1715"
  stone-raised: "#2A221E"
  parchment: "#E8DCC6"
  ash: "#9A8F80"
typography:
  prose:
    fontFamily: Cormorant Garamond
    fontSize: 28px
    fontWeight: 500
    lineHeight: 1.45
    letterSpacing: 0.005em
  narration:
    fontFamily: Cormorant Garamond
    fontSize: 26px
    fontWeight: 500
    lineHeight: 1.45
  name:
    fontFamily: Cinzel
    fontSize: 15px
    fontWeight: 600
    lineHeight: 1
    letterSpacing: 0.22em
  label:
    fontFamily: Cinzel
    fontSize: 11px
    fontWeight: 500
    lineHeight: 1.4
    letterSpacing: 0.24em
  code:
    fontFamily: Fira Mono
    fontSize: 11px
    fontWeight: 500
    lineHeight: 1.4
    letterSpacing: 0.1em
  wordmark:
    fontFamily: UnifrakturMaguntia
    fontSize: 72px
    fontWeight: 400
    lineHeight: 1
  title-item:
    fontFamily: Cinzel
    fontSize: 17px
    fontWeight: 500
    lineHeight: 1.3
    letterSpacing: 0.16em
  choice:
    fontFamily: Cormorant Garamond
    fontSize: 24px
    fontWeight: 600
    lineHeight: 1.3
  menu-entry:
    fontFamily: Cormorant Garamond
    fontSize: 34px
    fontWeight: 500
    lineHeight: 1.2
  body:
    fontFamily: Cormorant Garamond
    fontSize: 19px
    fontWeight: 500
    lineHeight: 1.4
rounded:
  none: 0px
  arch: 230px
  arch-menu: 174px
spacing:
  frame-inset: 72px
  box-pad: 34px
  inner-rule: 6px
  sm: 8px
  md: 16px
  lg: 28px
components:
  dialogue-box:
    backgroundColor: "{colors.stone}"
    textColor: "{colors.parchment}"
    rounded: "{rounded.none}"
    padding: "{spacing.box-pad}"
  dialogue-frame:
    backgroundColor: "{colors.gold}"
    width: 1px
  inner-rule:
    backgroundColor: "{colors.primary}"
    width: 1px
  narration:
    backgroundColor: "{colors.stone}"
    textColor: "{colors.ash}"
  speaker-plate:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.candle}"
    rounded: "{rounded.none}"
    padding: 8px
  choice:
    backgroundColor: "{colors.stone}"
    textColor: "{colors.parchment}"
    rounded: "{rounded.none}"
    padding: 16px
  choice-focus:
    backgroundColor: "{colors.primary-bright}"
    textColor: "{colors.parchment}"
  choice-badge:
    backgroundColor: "{colors.stone-raised}"
    textColor: "{colors.candle}"
    rounded: "{rounded.none}"
    size: 34px
  title-lancet:
    backgroundColor: "{colors.stone}"
    textColor: "{colors.parchment}"
    rounded: "{rounded.arch}"
    width: 460px
  title-item-focus:
    backgroundColor: "{colors.stone}"
    textColor: "{colors.candle}"
  menu-page:
    backgroundColor: "{colors.void}"
    textColor: "{colors.parchment}"
  menu-entry-active:
    backgroundColor: "{colors.void}"
    textColor: "{colors.candle}"
  slot-row:
    backgroundColor: "{colors.stone}"
    textColor: "{colors.parchment}"
  slot-action:
    backgroundColor: "{colors.stone}"
    textColor: "{colors.gold}"
  deciding-flame:
    backgroundColor: "{colors.candle}"
    size: 12px
---

# Unmei Gothic

PROTOTYPE. Mood A of three, laid over the settled variant E layout (`docs/title-and-menu.md` § Default look). Scope: a mood is for the **demo pack only**. The default look stays the neutral variant E, so authors can have their own agent develop their pack's UI. Two things gate any choice: the demo pack's story rewrite (a mood must follow the story's genre and setting), and a per-pack styling mechanism, which does not exist yet (`pack.ron` has no field for a pack's own look). Inspired by StyleShift's Gothic mood (styleshift.design/styles/gothic). Prototype rule: no purple, so the mood's violet is dropped and crimson and candle gold carry it alone.

## Overview

Dark romance. The screen is a candlelit chapel at night: the text appears as light coming out of dark stone. Ornament is architectural (lancet arches, frames, corner bosses) rather than botanical. The type is deliberately old (a blackletter wordmark, Roman inscriptional capitals, a Garamond for reading) so the player slows down. The mood suits a letter, a second meeting and a character whose answer is not known.

## Colors

- **Crimson (`primary`, #8E1B24):** the speaker banner and the inner rule. Velvet, roses: passion held back by the dark.
- **Bright crimson (`primary-bright`, #A3232E):** the focused choice only.
- **Candle (`candle`, #D9B76E):** text on crimson, the focused title row, the active menu entry, the deciding flame.
- **Gold (`gold`, #C9A45C):** frames, corner bosses, slot actions. Reliquary metal, used as line, never as fill.
- **Void (`void`, #0B0908) / Stone (`stone`, #1C1715) / Raised stone (`stone-raised`, #2A221E):** page, panels and numeral tiles. Warm blacks, never neutral grey.
- **Parchment (`parchment`, #E8DCC6) / Ash (`ash`, #9A8F80):** dialogue and narration.
- **Banned:** violet and any purple. Nothing between hue 250° and 330°.

## Typography

- **Wordmark:** UnifrakturMaguntia, a textura blackletter. Title only; never anywhere a player has to read quickly.
- **Reading:** Cormorant Garamond 500. Large (28px), because Cormorant's small x-height reads thin on dark at smaller sizes.
- **Inscriptions:** Cinzel capitals with wide tracking for speaker names, title rows, the quick row and labels: lettering cut into stone.
- **Codes:** Fira Mono stays for engine strings (`ace.greet · in flight`).
- None of UnifrakturMaguntia, Cormorant Garamond or Cinzel is bundled. All three are SIL OFL 1.1 and would ship with the demo pack, not the player, if this mood wins. The mockup loads them from Google Fonts.

## Layout

Variant E's geometry is unchanged. Inside it, Gothic centres text in the title column and the menu's left column, because both sit inside an arch.

## Elevation & Depth

Depth comes from light, not lift. Panels are vertical gradients from stone to void, as if lit by a candle above. The focused choice gets a warm candle glow (`0 0 28px`, candle at 35%). There are no drop shadows: in a dark room, nothing casts a visible shadow downwards.

## Shapes

Hard right angles everywhere except the lancet: a full-height panel with a round-arched top (`rounded.arch`, half its width). The menu's left column repeats the arch. Corner bosses are small gold diamonds.

## Components

- **Dialogue box:** a stone-to-void gradient, a 1px gold frame, a 1px crimson rule inset 6px, and a gold diamond on each corner.
- **Speaker banner:** crimson, square, with gold rules above and below; the name in candle Cinzel capitals, with a fleuron (❧) before it.
- **Choices:** stone slabs framed in gold at 40%. The number is a Roman numeral (I, II) on a raised-stone tile. Focus fills bright crimson and lights the slab with candle glow.
- **Title:** the column sits inside a stone lancet, 460px wide, rising from the bottom edge with a round-arched top and a gold frame. The art outside the arch is untouched: no veil. The focused row is candle, with fleurons on both sides.
- **Esc menu:** the contents page on void; the left column sits inside a smaller arch; the active entry is candle with a crimson diamond. Slots are stone slabs with gold actions in Cinzel.
- **Deciding:** a candle flame (a small radial gradient that flickers in opacity) and "Ace weighs her answer" in italic Cormorant.
- **Advance mark:** a gold ✦ that pulses.

## Do's and Don'ts

- Do keep gold to lines and marks. A gold fill turns it into Art Deco.
- Do use blackletter for the wordmark only.
- Don't use purple or violet.
- Don't use cool greys. Every dark is warm.
- Don't use rounded corners except the arch.

## Bevy feasibility

Linear and radial gradients, 1px borders, nested bordered nodes, a large top-only corner radius for the arch (`BorderRadius` per corner), a coloured blurred `BoxShadow`, and opacity loops. It needs three fonts shipped with the pack.
