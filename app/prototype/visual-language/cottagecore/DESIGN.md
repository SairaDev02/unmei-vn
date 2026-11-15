---
version: alpha
name: Unmei Cottagecore
description: PROTOTYPE mood B for the default look (variant E). Slow, handmade warmth after StyleShift's Cottagecore mood. Linen and parchment, sage and dusty rose, a handwritten hand and pressed flowers.
colors:
  primary: "#55694A"
  secondary: "#9E524D"
  tertiary: "#C6913F"
  sage: "#8FA382"
  linen: "#F3EBDD"
  linen-deep: "#E6D8C0"
  parchment: "#FBF6EC"
  bark: "#3E3328"
  bark-soft: "#6E5F4E"
typography:
  prose:
    fontFamily: Lora
    fontSize: 23px
    fontWeight: 400
    lineHeight: 1.65
  narration:
    fontFamily: Lora
    fontSize: 22px
    fontWeight: 400
    lineHeight: 1.65
  name:
    fontFamily: Caveat
    fontSize: 30px
    fontWeight: 700
    lineHeight: 1
  label:
    fontFamily: Lora
    fontSize: 13px
    fontWeight: 400
    lineHeight: 1.4
    letterSpacing: 0.02em
  code:
    fontFamily: Fira Mono
    fontSize: 11px
    fontWeight: 500
    lineHeight: 1.4
  wordmark:
    fontFamily: Caveat
    fontSize: 76px
    fontWeight: 700
    lineHeight: 0.9
  title-item:
    fontFamily: Lora
    fontSize: 21px
    fontWeight: 400
    lineHeight: 1.3
  choice:
    fontFamily: Lora
    fontSize: 21px
    fontWeight: 500
    lineHeight: 1.35
  menu-entry:
    fontFamily: Lora
    fontSize: 30px
    fontWeight: 400
    lineHeight: 1.25
  body:
    fontFamily: Lora
    fontSize: 17px
    fontWeight: 400
    lineHeight: 1.5
rounded:
  sm: 6px
  card: 18px
  box: 24px
  full: 999px
spacing:
  frame-inset: 72px
  box-pad: 34px
  stitch: 7px
  sm: 8px
  md: 16px
  lg: 28px
components:
  dialogue-box:
    backgroundColor: "{colors.parchment}"
    textColor: "{colors.bark}"
    rounded: "{rounded.box}"
    padding: "{spacing.box-pad}"
  stitch:
    backgroundColor: "{colors.bark-soft}"
    width: 1px
  narration:
    backgroundColor: "{colors.parchment}"
    textColor: "{colors.bark-soft}"
  speaker-tag:
    backgroundColor: "{colors.parchment}"
    textColor: "{colors.secondary}"
    rounded: "{rounded.sm}"
    padding: 6px
  choice:
    backgroundColor: "{colors.parchment}"
    textColor: "{colors.bark}"
    rounded: "{rounded.card}"
    padding: 16px
  choice-focus:
    backgroundColor: "{colors.linen-deep}"
    textColor: "{colors.bark}"
  choice-badge:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.parchment}"
    rounded: "{rounded.full}"
    size: 32px
  pressed-flower:
    backgroundColor: "{colors.parchment}"
    textColor: "{colors.secondary}"
    size: 26px
  sprig:
    backgroundColor: "{colors.sage}"
    size: 30px
  pin:
    backgroundColor: "{colors.tertiary}"
    size: 16px
  title-letter:
    backgroundColor: "{colors.parchment}"
    textColor: "{colors.bark}"
    rounded: "{rounded.card}"
    width: 500px
  title-item-focus:
    backgroundColor: "{colors.parchment}"
    textColor: "{colors.secondary}"
  menu-page:
    backgroundColor: "{colors.linen}"
    textColor: "{colors.bark}"
  menu-entry-active:
    backgroundColor: "{colors.linen}"
    textColor: "{colors.primary}"
  slot-row:
    backgroundColor: "{colors.parchment}"
    textColor: "{colors.bark}"
    rounded: "{rounded.card}"
  slot-action:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.parchment}"
    rounded: "{rounded.full}"
---

# Unmei Cottagecore

PROTOTYPE. Mood B of three, laid over the settled variant E layout (`docs/title-and-menu.md` § Default look). Inspired by StyleShift's Cottagecore mood (styleshift.design/styles/cottagecore). Prototype rule: no purple.

## Overview

Slow beauty and handmade warmth. The interface is a letter written at a kitchen table: parchment and linen, a pen hand for names, pressed flowers in the corners. It is imperfect on purpose: corners are uneven, cards sit a degree off true, as if placed by hand. It matches the demo's opening, a folded letter on the ground, and makes the default install feel personal rather than technical.

## Colors

No pure colours: everything is slightly faded, warm and aged, as if dyed with natural pigments.

- **Sage (`primary`, #55694A):** what the player can act on: choice numbers, slot actions, the active menu entry.
- **Dusty rose (`secondary`, #9E524D):** people and feeling: speaker names, pressed flowers, the focused title row.
- **Ochre (`tertiary`, #C6913F):** the pin on the title letter; nothing else.
- **Light sage (`sage`, #8FA382):** sprig ornaments, never text.
- **Parchment (`parchment`, #FBF6EC) / Linen (`linen`, #F3EBDD) / Deep linen (`linen-deep`, #E6D8C0):** the box, cards, the menu page and focus.
- **Bark (`bark`, #3E3328) / Soft bark (`bark-soft`, #6E5F4E):** reading text, narration, stitching. No black anywhere.
- **Banned:** purple, lavender, any hue between 250° and 330°.

## Typography

- **Reading and chrome:** Lora, a warm book serif with a soft brush feel, with generous leading (1.65) so the text breathes.
- **Hand:** Caveat for the wordmark, speaker names and choice letters. It looks like ink from a pen.
- **Labels:** Lora at 13px in sentence case. No spaced capitals: they would feel clinical here.
- **Codes:** Fira Mono stays for engine strings.
- Neither Lora nor Caveat is bundled. Both are SIL OFL 1.1 and would be bundled under `app/assets/fonts/` if this mood wins. The mockup loads them from Google Fonts.

## Layout

Variant E's geometry is unchanged. Choice letters (a, b) replace numbers in the badge; the keys stay 1–9.

## Elevation & Depth

Paper on a table: a soft, warm, slightly blurred shadow (`0 6px 18px`, bark at 22%) under every card. No glow, no hard edges.

## Shapes

Organic and uneven. The box uses four different corner radii (18/28/16/30px, around `rounded.box`), and cards around `rounded.card`. The speaker tag, the title letter and the choices are rotated by 1–2 degrees, alternating, so the layout looks placed by hand. Badges and slot actions are round, like buttons sewn on.

## Components

- **Dialogue box:** parchment with uneven corners, a hairline edge, and a stitched inner rule inset 7px (a nested 1px bordered node at 35% opacity). A pressed flower (❀, rose) sits on the top-right corner and a sprig (⚘, sage) on the bottom-left.
- **Speaker tag:** a small parchment tag tilted −2° on the box's top edge, the name in rose Caveat.
- **Choices:** recipe cards with a sage letter button (a, b), alternately tilted ±0.6°. Focus straightens the card, fills it deep linen and adds a rose underline.
- **Title:** the column is written on a parchment letter, 500px wide, tilted −1.2° and held by an ochre pin. The art around it is untouched: no veil. The wordmark is handwritten; the focused row is rose with a pressed flower before it.
- **Esc menu:** the contents page on linen; the left column ends in a large faded sprig (⚘); the active entry is sage with a flower. Slots are recipe cards with round sage action buttons.
- **Deciding:** three flowers bloom in turn (opacity loop) and "Ace is thinking it over" in italic Lora.
- **Advance mark:** a small rose flower.

## Do's and Don'ts

- Do keep everything slightly off: uneven corners, slight tilts. A perfectly square layout loses the mood.
- Do keep handwriting for names and short marks only. Never handwrite a full line of dialogue.
- Don't use pure white, pure black or saturated colour.
- Don't use purple or lavender.
- Don't use hard shadows or glow.

## Bevy feasibility

Solid fills, 1px borders, nested bordered nodes, per-corner `BorderRadius`, a blurred `BoxShadow`, and opacity loops. The tilts need `UiTransform` rotation (added in Bevy 0.17); confirm it is present in 0.19 before building. If not, drop the tilts and keep the uneven corners. It needs two new bundled fonts.
