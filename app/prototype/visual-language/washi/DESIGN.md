---
version: alpha
name: Unmei Washi
description: PROTOTYPE visual language A for the default look (variant E). Paper and ink over the scene, one vermilion seal colour.
colors:
  primary: "#B83A24"
  on-primary: "#FBF7F0"
  ink: "#1E1B18"
  ink-muted: "#5E554C"
  paper: "#F4EEE3"
  paper-deep: "#E9E0D0"
  rule: "#CDBFAA"
  shade: "#1E1B182E"
typography:
  prose:
    fontFamily: Shippori Mincho
    fontSize: 25px
    fontWeight: 400
    lineHeight: 1.6
    letterSpacing: 0.01em
  narration:
    fontFamily: Shippori Mincho
    fontSize: 23px
    fontWeight: 400
    lineHeight: 1.6
  name:
    fontFamily: IBM Plex Sans
    fontSize: 16px
    fontWeight: 600
    lineHeight: 1
    letterSpacing: 0.16em
  label:
    fontFamily: Fira Mono
    fontSize: 12px
    fontWeight: 500
    lineHeight: 1.4
    letterSpacing: 0.24em
  wordmark:
    fontFamily: Shippori Mincho
    fontSize: 60px
    fontWeight: 600
    lineHeight: 1
    letterSpacing: 0.04em
  title-item:
    fontFamily: Shippori Mincho
    fontSize: 22px
    fontWeight: 400
    lineHeight: 1.3
  choice:
    fontFamily: Shippori Mincho
    fontSize: 21px
    fontWeight: 400
    lineHeight: 1.35
  menu-entry:
    fontFamily: Shippori Mincho
    fontSize: 32px
    fontWeight: 400
    lineHeight: 1.2
  body:
    fontFamily: IBM Plex Sans
    fontSize: 16px
    fontWeight: 400
    lineHeight: 1.5
rounded:
  none: 0px
  sm: 2px
  full: 999px
spacing:
  frame-inset: 72px
  box-pad: 34px
  inner-rule: 6px
  sm: 8px
  md: 16px
  lg: 28px
components:
  dialogue-box:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.ink}"
    rounded: "{rounded.sm}"
    padding: "{spacing.box-pad}"
  dialogue-rule:
    backgroundColor: "{colors.rule}"
    width: 1px
  narration:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.ink-muted}"
  speaker-plate:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.on-primary}"
    rounded: "{rounded.none}"
    padding: 8px
  choice:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.ink}"
    rounded: "{rounded.none}"
    padding: 16px
  choice-focus:
    backgroundColor: "{colors.paper-deep}"
    textColor: "{colors.ink}"
  choice-badge:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.on-primary}"
    rounded: "{rounded.full}"
    size: 26px
  title-sheet:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.ink}"
    width: 520px
  title-item-focus:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.primary}"
  menu-page:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.ink}"
  menu-entry-active:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.primary}"
  slot-row:
    backgroundColor: "{colors.paper-deep}"
    textColor: "{colors.ink}"
    rounded: "{rounded.none}"
  shadow:
    backgroundColor: "{colors.shade}"
---

# Unmei Washi

PROTOTYPE. One of three visual languages laid over the settled variant E layout (`docs/title-and-menu.md` § Default look). Geometry is variant E's; only colour, type, shape, depth and ornament change.

## Overview

A printed book laid over a photograph. Every surface the player reads is opaque paper with ink on it; the scene stays as the art was drawn. One colour carries meaning: a vermilion seal for who is speaking, what is focused and what can be chosen. The mood is quiet, literary and slightly formal, which suits slow reading and the demo's letter motif.

## Colors

- **Seal vermilion (`primary`, #B83A24):** the speaker plate, choice numbers, focus and the active menu entry. Never a fill larger than a plate.
- **On seal (`on-primary`, #FBF7F0):** text on vermilion only.
- **Ink (`ink`, #1E1B18):** all reading text.
- **Faded ink (`ink-muted`, #5E554C):** narration, quick row, captions.
- **Paper (`paper`, #F4EEE3):** the dialogue box, choices, title sheet and menu page.
- **Deep paper (`paper-deep`, #E9E0D0):** slot rows and the focused choice.
- **Rule (`rule`, #CDBFAA):** hairlines and the box's inner rule.
- **Shade (`shade`, ink at 18%):** the one flat drop under paper.

## Typography

Reading text is a Mincho serif, so the box reads as a printed page. Chrome (speaker name, body copy) stays in the bundled IBM Plex Sans; codes and the quick row stay in Fira Mono.

- Shippori Mincho is **not bundled**. Adopting Washi means bundling it (SIL OFL 1.1) next to Plex and Fira under `app/assets/fonts/`. The mockup falls back to Yu Mincho, then Georgia.
- Narration is the same face at 23px in faded ink, never italic (no italic face is bundled, and synthetic italic looks broken in Mincho).
- Speaker names are uppercase Plex SemiBold with wide tracking.

## Layout

Variant E's geometry is unchanged: the 72px frame inset, the 178px box at y 514, the 560px centred choice stack, the title's left column at x 104 and the 420px contents column. Washi adds a 6px inner rule inside the dialogue box and the choices.

## Elevation & Depth

Paper sits on the scene with one flat, unblurred 2px shade below it. No glows, no gradients on paper. Depth inside a page is made with hairlines, not shadows.

## Shapes

Square corners (2px on the box only, so the edge reads as cut paper rather than a screen). The choice badge is the one circle: a seal.

## Components

- **Dialogue box:** paper, 2px radius, a 1px `rule` border and a second 1px rule inset 6px (a child node with its own border). Text in ink.
- **Speaker plate:** a vermilion rectangle on the box's top edge, square corners, uppercase Plex.
- **Choices:** paper slips with the same double rule; the number is a vermilion seal circle. Focus fills deep paper and draws a 4px vermilion bar on the left edge.
- **Title:** an opaque paper sheet, 520px wide and flush left, carries the wordmark and rows; the art to its right is untouched. This is Washi's answer to the open "no veil" conflict: the art is covered, not darkened. The focused row has a vermilion dash and vermilion text.
- **Esc menu:** the contents page in paper and ink; the active entry is vermilion with a seal dot.
- **Deciding:** three ink dots and the line in faded ink; no colour change.

## Do's and Don'ts

- Do keep vermilion to marks and plates. A vermilion panel stops reading as a seal.
- Do keep reading text on opaque paper. Never set ink on art.
- Don't add blur, gradients or glow to paper.
- Don't use the Mincho face below 16px; use Plex for small text.

## Bevy feasibility

Solid fills, 1px borders, a nested bordered node for the inner rule, one unblurred `BoxShadow`, corner radius. Needs one new bundled font.
