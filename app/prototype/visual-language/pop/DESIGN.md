---
version: alpha
name: Unmei Pop
description: PROTOTYPE visual language C for the default look (variant E). Sticker-sheet pop, thick ink outlines, hard offset shadows, candy accents.
colors:
  primary: "#FF5C97"
  secondary: "#3FCFB0"
  tertiary: "#FFD45C"
  ink: "#2B2540"
  ink-soft: "#5B5470"
  cream: "#FFF6EC"
  cream-deep: "#FFE6CF"
  white: "#FFFFFF"
typography:
  prose:
    fontFamily: IBM Plex Sans
    fontSize: 24px
    fontWeight: 400
    lineHeight: 1.5
  narration:
    fontFamily: IBM Plex Sans
    fontSize: 22px
    fontWeight: 400
    lineHeight: 1.5
  name:
    fontFamily: IBM Plex Sans
    fontSize: 17px
    fontWeight: 700
    lineHeight: 1
    letterSpacing: 0.06em
  label:
    fontFamily: Fira Mono
    fontSize: 12px
    fontWeight: 500
    lineHeight: 1.4
    letterSpacing: 0.14em
  wordmark:
    fontFamily: IBM Plex Sans
    fontSize: 72px
    fontWeight: 700
    lineHeight: 0.95
    letterSpacing: -0.035em
  title-item:
    fontFamily: IBM Plex Sans
    fontSize: 19px
    fontWeight: 700
    lineHeight: 1.2
  choice:
    fontFamily: IBM Plex Sans
    fontSize: 20px
    fontWeight: 600
    lineHeight: 1.3
  menu-entry:
    fontFamily: IBM Plex Sans
    fontSize: 28px
    fontWeight: 700
    lineHeight: 1.2
  body:
    fontFamily: IBM Plex Sans
    fontSize: 16px
    fontWeight: 400
    lineHeight: 1.5
rounded:
  sm: 10px
  md: 18px
  lg: 26px
  full: 999px
spacing:
  frame-inset: 72px
  box-pad: 34px
  stroke: 3px
  drop: 6px
  sm: 8px
  md: 16px
  lg: 28px
components:
  dialogue-box:
    backgroundColor: "{colors.cream}"
    textColor: "{colors.ink}"
    rounded: "{rounded.lg}"
    padding: "{spacing.box-pad}"
  narration:
    backgroundColor: "{colors.cream}"
    textColor: "{colors.ink-soft}"
  speaker-plate:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.ink}"
    rounded: "{rounded.full}"
    padding: 8px
  choice:
    backgroundColor: "{colors.white}"
    textColor: "{colors.ink}"
    rounded: "{rounded.md}"
    padding: 16px
  choice-focus:
    backgroundColor: "{colors.tertiary}"
    textColor: "{colors.ink}"
  choice-badge:
    backgroundColor: "{colors.secondary}"
    textColor: "{colors.ink}"
    rounded: "{rounded.full}"
    size: 28px
  title-item:
    backgroundColor: "{colors.cream}"
    textColor: "{colors.ink}"
    rounded: "{rounded.full}"
  title-item-focus:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.ink}"
  menu-page:
    backgroundColor: "{colors.cream-deep}"
    textColor: "{colors.ink}"
  menu-entry-active:
    backgroundColor: "{colors.secondary}"
    textColor: "{colors.ink}"
    rounded: "{rounded.full}"
  slot-row:
    backgroundColor: "{colors.white}"
    textColor: "{colors.ink}"
    rounded: "{rounded.md}"
  slot-action:
    backgroundColor: "{colors.tertiary}"
    textColor: "{colors.ink}"
    rounded: "{rounded.full}"
---

# Unmei Pop

PROTOTYPE. One of three visual languages laid over the settled variant E layout (`docs/title-and-menu.md` § Default look). Geometry is variant E's; only colour, type, shape, depth and ornament change.

## Overview

A sticker sheet. Every surface is a die-cut sticker: a cream or white shape, a thick ink outline and a hard ink shadow offset down-right. Accents are candy colours that each mean one thing. It is loud, friendly and clearly a game, which suits a demo pack whose job is to look approachable on first launch.

## Colors

- **Pink (`primary`, #FF5C97):** the speaker plate and the focused title row.
- **Mint (`secondary`, #3FCFB0):** numbers and the active menu entry: "this is where you are".
- **Butter (`tertiary`, #FFD45C):** focus on choices and slot actions: "press this".
- **Ink (`ink`, #2B2540):** all text, every outline and every shadow. There are no grey borders.
- **Soft ink (`ink-soft`, #5B5470):** narration and captions.
- **Cream (`cream`, #FFF6EC) / Deep cream (`cream-deep`, #FFE6CF) / White (`white`):** sticker faces and the menu page.

## Typography

Bundled faces only. Plex Sans Bold carries the personality: a heavy, tight wordmark, bold speaker names and bold title rows. Reading text stays Regular for comfort; choices are SemiBold so they read as buttons.

## Layout

Variant E's geometry is unchanged. The `stroke` (3px) and `drop` (6px) spacing tokens are the outline width and the shadow offset of every sticker.

## Elevation & Depth

One elevation, everywhere: `BoxShadow` offset 6px right and 6px down, zero blur, ink. Focus raises a sticker by moving it 2px up-left and growing the shadow to 8px, so it reads as lifted off the sheet. Nothing is translucent.

## Shapes

Big, friendly radii: 26px on the box, 18px on choices and slots, full pills on plates, title rows and menu entries. Number badges are circles.

## Components

- **Dialogue box:** cream, 3px ink outline, 26px radius, hard shadow.
- **Speaker plate:** a pink pill with an ink outline, sitting on the box's top edge.
- **Choices:** white stickers with a mint number circle. Focus turns the face butter and lifts it.
- **Title:** no veil. Each row is its own cream pill sticker, so it reads over any art; the wordmark is cream with a hard ink text shadow. Focused row: pink and lifted.
- **Esc menu:** the contents page on deep cream; entries are pills, the active one mint; slots are white stickers with butter action pills.
- **Deciding:** three ink dots bouncing (offset loop) in a white bubble.

## Do's and Don'ts

- Do outline every surface in ink at 3px. A sticker without an outline looks unfinished.
- Do keep shadows hard. A blurred shadow breaks the language.
- Don't use more than two accent colours in one view.
- Don't put ink text on pink below 17px bold.

## Bevy feasibility

Solid fills, 3px borders, corner radius, unblurred offset `BoxShadow`, `TextShadow`, offset loops. Bundled fonts only.
