---
version: alpha
name: Unmei Dusk
description: PROTOTYPE visual language B for the default look (variant E). Twilight glass, soft light, two cool-warm accents.
colors:
  primary: "#A99BFF"
  secondary: "#FF8FB1"
  on-accent: "#1A1433"
  text: "#EEF0FF"
  text-muted: "#A8ABCF"
  surface: "#1D1A44"
  surface-top: "#2C2862"
  night: "#0E0C22"
  horizon: "#4A2F7A"
  edge: "#B4BCFF"
  glow: "#7C6CFF"
typography:
  prose:
    fontFamily: IBM Plex Sans
    fontSize: 24px
    fontWeight: 400
    lineHeight: 1.55
    letterSpacing: 0.01em
  narration:
    fontFamily: IBM Plex Sans
    fontSize: 22px
    fontWeight: 400
    lineHeight: 1.55
    letterSpacing: 0.02em
  name:
    fontFamily: IBM Plex Sans
    fontSize: 15px
    fontWeight: 600
    lineHeight: 1
    letterSpacing: 0.08em
  label:
    fontFamily: Fira Mono
    fontSize: 12px
    fontWeight: 500
    lineHeight: 1.4
    letterSpacing: 0.3em
  wordmark:
    fontFamily: IBM Plex Sans
    fontSize: 76px
    fontWeight: 400
    lineHeight: 1
    letterSpacing: -0.02em
  title-item:
    fontFamily: IBM Plex Sans
    fontSize: 20px
    fontWeight: 400
    lineHeight: 1.3
  choice:
    fontFamily: IBM Plex Sans
    fontSize: 20px
    fontWeight: 400
    lineHeight: 1.3
  menu-entry:
    fontFamily: IBM Plex Sans
    fontSize: 28px
    fontWeight: 400
    lineHeight: 1.25
  body:
    fontFamily: IBM Plex Sans
    fontSize: 16px
    fontWeight: 400
    lineHeight: 1.5
rounded:
  sm: 10px
  md: 16px
  lg: 22px
  full: 999px
spacing:
  frame-inset: 72px
  box-pad: 34px
  sm: 8px
  md: 16px
  lg: 28px
components:
  dialogue-box:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    rounded: "{rounded.lg}"
    padding: "{spacing.box-pad}"
  dialogue-box-top:
    backgroundColor: "{colors.surface-top}"
    textColor: "{colors.text}"
  dialogue-edge:
    backgroundColor: "{colors.edge}"
    width: 1px
  narration:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text-muted}"
  speaker-plate:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.on-accent}"
    rounded: "{rounded.full}"
    padding: 8px
  choice:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    rounded: "{rounded.full}"
    padding: 16px
  choice-focus:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.on-accent}"
  choice-badge:
    backgroundColor: "{colors.secondary}"
    textColor: "{colors.on-accent}"
    rounded: "{rounded.full}"
    size: 26px
  focus-glow:
    backgroundColor: "{colors.glow}"
  title-horizon:
    backgroundColor: "{colors.horizon}"
    textColor: "{colors.text}"
  title-item-focus:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    rounded: "{rounded.full}"
  menu-page:
    backgroundColor: "{colors.night}"
    textColor: "{colors.text}"
  menu-entry-active:
    backgroundColor: "{colors.night}"
    textColor: "{colors.primary}"
  slot-row:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    rounded: "{rounded.md}"
  deciding-orb:
    backgroundColor: "{colors.secondary}"
    size: 14px
---

# Unmei Dusk

PROTOTYPE. One of three visual languages laid over the settled variant E layout (`docs/title-and-menu.md` § Default look). Geometry is variant E's; only colour, type, shape, depth and ornament change.

## Overview

Evening light through glass. Surfaces are deep indigo panels lit from above, with soft coloured light around anything the player can act on. Lavender means "you can act here", rose means "the character is doing something". It is romantic and modern, and it gives the live `decide` wait a natural visual: a glowing orb, as if the character is thinking.

## Colors

- **Lavender (`primary`, #A99BFF):** speaker plate, focus fill, active menu entry, the box's top edge.
- **Rose (`secondary`, #FF8FB1):** character activity: the deciding orb and choice numbers.
- **Deep text (`on-accent`, #1A1433):** text on lavender or rose.
- **Moon (`text`, #EEF0FF) / Haze (`text-muted`, #A8ABCF):** reading text and narration.
- **Surface (`surface`, #1D1A44) and Surface top (`surface-top`, #2C2862):** the two stops of every panel's vertical gradient. Panels sit at 88–94% opacity over the scene.
- **Night (`night`, #0E0C22):** the Esc menu page.
- **Horizon (`horizon`, #4A2F7A):** the title's corner light.
- **Edge (`edge`, #B4BCFF):** panel borders at 28% opacity.
- **Glow (`glow`, #7C6CFF):** shadow colour for focus and the box, at 25–55% opacity.

## Typography

Bundled faces only. Plex Sans Regular everywhere reading happens; weight stays light so the glow carries emphasis instead of bold. The wordmark is large, lowercase and tight. Labels are spaced-out Fira Mono.

## Layout

Variant E's geometry is unchanged. Corners grow: the box takes a 22px radius and choices become full pills.

## Elevation & Depth

Three levels, all made with blurred, coloured `BoxShadow`:

1. Scene (no shadow).
2. Panels: `0 12px 40px` glow at 25%, plus a 1px edge at 28%.
3. Focus: `0 0 24px` glow at 55% and the lavender fill.

Panels are vertical gradients from `surface-top` to `surface`, so they read as lit from above.

## Shapes

Soft and round: 22px on the box, 16px on slot rows, full pills on plates, choices, focused title rows and the deciding orb.

## Components

- **Dialogue box:** gradient glass, 22px radius, 1px edge, panel glow. A 1px lavender line along the top edge (a child node 40% wide, centred) marks it as the active surface.
- **Speaker plate:** a lavender pill on the top edge with deep text.
- **Choices:** glass pills with a rose number. Focus fills lavender and glows.
- **Title:** the veil becomes a radial horizon light behind the column (night at the centre, horizon purple at the rim), so the wordmark sits in coloured light rather than black. It still tints the art's left side, so Dusk does not settle the open "no veil" conflict. Focused row: a glass pill behind the label.
- **Esc menu:** the contents page on night, with a faint horizon light in the left column and glass slot rows.
- **Deciding:** a rose orb pulsing in opacity (a simple loop) next to the line.

## Do's and Don'ts

- Do keep rose for the character's activity. Player affordances are lavender.
- Do let glow mark focus; never use glow on something that is not interactive or live.
- Don't stack more than two glows on screen at once.
- Don't use pure black; night is the darkest value.

## Bevy feasibility

Linear and radial gradients, alpha fills, corner radius, blurred coloured `BoxShadow`, opacity loops. No `backdrop-filter`: the "glass" is opacity, not blur. Bundled fonts only.
