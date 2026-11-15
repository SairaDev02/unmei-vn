---
version: alpha
name: Unmei Aero
description: PROTOTYPE demo-pack mood C, over the variant E layout. Optimistic early-2000s gloss after StyleShift's Y2K / Frutiger Aero mood. Sky, water and lime; jelly buttons, gloss highlights and rising bubbles.
colors:
  primary: "#0A66AD"
  secondary: "#7ED321"
  tertiary: "#29C4D9"
  sky: "#3FA9F5"
  lime-light: "#C8F27E"
  ink: "#0D3354"
  mist: "#E6F4FF"
  cloud: "#FFFFFF"
typography:
  prose:
    fontFamily: Nunito
    fontSize: 24px
    fontWeight: 600
    lineHeight: 1.5
  narration:
    fontFamily: Nunito
    fontSize: 22px
    fontWeight: 600
    lineHeight: 1.5
  name:
    fontFamily: Nunito
    fontSize: 17px
    fontWeight: 800
    lineHeight: 1
    letterSpacing: 0.02em
  label:
    fontFamily: Nunito
    fontSize: 12px
    fontWeight: 800
    lineHeight: 1.4
    letterSpacing: 0.08em
  code:
    fontFamily: Fira Mono
    fontSize: 11px
    fontWeight: 500
    lineHeight: 1.4
  wordmark:
    fontFamily: Nunito
    fontSize: 70px
    fontWeight: 800
    lineHeight: 1
    letterSpacing: -0.02em
  title-item:
    fontFamily: Nunito
    fontSize: 20px
    fontWeight: 700
    lineHeight: 1.2
  choice:
    fontFamily: Nunito
    fontSize: 21px
    fontWeight: 700
    lineHeight: 1.3
  menu-entry:
    fontFamily: Nunito
    fontSize: 24px
    fontWeight: 800
    lineHeight: 1.2
  body:
    fontFamily: Nunito
    fontSize: 16px
    fontWeight: 600
    lineHeight: 1.5
rounded:
  md: 18px
  lg: 30px
  full: 999px
spacing:
  frame-inset: 72px
  box-pad: 34px
  sm: 8px
  md: 16px
  lg: 28px
components:
  dialogue-box:
    backgroundColor: "{colors.cloud}"
    textColor: "{colors.ink}"
    rounded: "{rounded.lg}"
    padding: "{spacing.box-pad}"
  gloss:
    backgroundColor: "{colors.cloud}"
    rounded: "{rounded.lg}"
  narration:
    backgroundColor: "{colors.cloud}"
    textColor: "{colors.primary}"
  speaker-plate:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.cloud}"
    rounded: "{rounded.full}"
    padding: 8px
  choice:
    backgroundColor: "{colors.cloud}"
    textColor: "{colors.ink}"
    rounded: "{rounded.full}"
    padding: 16px
  choice-focus:
    backgroundColor: "{colors.secondary}"
    textColor: "{colors.ink}"
  choice-focus-top:
    backgroundColor: "{colors.lime-light}"
    textColor: "{colors.ink}"
  choice-badge:
    backgroundColor: "{colors.tertiary}"
    textColor: "{colors.ink}"
    rounded: "{rounded.full}"
    size: 34px
  bubble:
    backgroundColor: "{colors.sky}"
    size: 18px
  title-card:
    backgroundColor: "{colors.cloud}"
    textColor: "{colors.ink}"
    rounded: "{rounded.lg}"
    width: 470px
  title-item-focus:
    backgroundColor: "{colors.secondary}"
    textColor: "{colors.ink}"
  menu-page:
    backgroundColor: "{colors.mist}"
    textColor: "{colors.ink}"
  menu-entry-active:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.cloud}"
    rounded: "{rounded.full}"
  slot-row:
    backgroundColor: "{colors.cloud}"
    textColor: "{colors.ink}"
    rounded: "{rounded.md}"
  slot-action:
    backgroundColor: "{colors.secondary}"
    textColor: "{colors.ink}"
    rounded: "{rounded.full}"
---

# Unmei Aero

PROTOTYPE. Mood C of three, laid over the settled variant E layout (`docs/title-and-menu.md` § Default look). Scope: a mood is for the **demo pack only**. The default look stays the neutral variant E, so authors can have their own agent develop their pack's UI. Two things gate any choice: the demo pack's story rewrite (a mood must follow the story's genre and setting), and a per-pack styling mechanism, which does not exist yet (`pack.ron` has no field for a pack's own look). Inspired by StyleShift's Y2K / Frutiger Aero mood (styleshift.design/styles/y2k-aero). Prototype rule: no purple.

## Overview

Optimistic futurism: technology and nature in harmony, before cynicism. Every surface is glossy and full of air, like a jelly button or a water droplet on glass, and the palette is sky, water and new leaves. It suits the demo's school-gate morning and blue sky, and gives the demo a bright, inviting first impression instead of a dark one.

## Colors

Bright and saturated, but always blue-green, never toward violet.

- **Deep sky (`primary`, #0A66AD):** the speaker capsule, narration, the active menu entry.
- **Lime (`secondary`, #7ED321) / Light lime (`lime-light`, #C8F27E):** the two stops of the jelly-button gradient for focus and slot actions.
- **Aqua (`tertiary`, #29C4D9):** the glossy orb behind choice numbers.
- **Sky (`sky`, #3FA9F5):** bubbles and the sky band at the top of the menu.
- **Cloud (`cloud`, #FFFFFF) / Mist (`mist`, #E6F4FF):** surfaces and the menu page.
- **Ink (`ink`, #0D3354):** all text on light surfaces. A navy, never black.
- **Banned:** purple, violet, magenta, any hue between 250° and 330°.

## Typography

- Nunito everywhere: a rounded sans with soft ends and open shapes. It is friendly, not authoritative.
- Weights stay medium to heavy (600 for reading, 800 for names and the wordmark): confident, never thin.
- The wordmark is deep sky with a white highlight below and a soft blue shadow, like embossed plastic on the white title card.
- Codes: Fira Mono stays for engine strings.
- Nunito is not bundled. It is SIL OFL 1.1 and would ship with the demo pack, not the player, if this mood wins. The mockup loads it from Google Fonts.

## Layout

Variant E's geometry is unchanged. Padding is generous, so everything looks inflated.

## Elevation & Depth

Gloss and air:

- **Gloss:** every surface has a highlight on its top half: a white-to-transparent gradient on a nested node inset 3px, rounded like its parent.
- **Lift:** a soft blue shadow (`0 10px 30px`, deep sky at 30%).
- **Focus:** a lime glow (`0 0 22px`, lime at 60%).
- **Bubbles:** small radial-gradient spheres rise slowly behind the box and on the title.

## Shapes

Inflated: 30px on panels, 18px on slot rows, full capsules on plates, choices, title rows, menu entries and actions. Number badges are glossy orbs.

## Components

- **Dialogue box:** a white panel with a cool gradient to mist at the bottom, a top-half gloss, a 2px white rim and a blue lift. Three sky bubbles drift up from its right edge.
- **Speaker capsule:** a deep-sky gradient capsule with its own gloss and white text.
- **Choices:** white jelly capsules with an aqua orb number. Focus turns the capsule into a lime jelly (light lime to lime) with a lime glow.
- **Title:** the column sits on a glossy white card, 470px wide; the art keeps its sky, with a soft sun glint at the top right. No veil. Rows are capsules; the focused row is lime jelly.
- **Esc menu:** the contents page on mist under a sky band; entries are capsules, the active one deep sky. Slots are glossy cards with lime capsule actions.
- **Deciding:** three aqua orbs bounce in turn, with "Ace is deciding…".
- **Advance mark:** a small glossy aqua orb that bounces.

## Do's and Don'ts

- Do give every surface a gloss highlight. A flat surface reads as tired here.
- Do keep the hues between green and blue.
- Don't use purple, violet or magenta.
- Don't use sharp corners or thin type.
- Don't use backdrop blur (Bevy has none). Express "glass" with gloss and gradients.

## Bevy feasibility

Linear and radial gradients, alpha fills, 2px borders, large radii, a blurred coloured `BoxShadow`, `TextShadow`, and offset or opacity loops. No `backdrop-filter`. It needs one font shipped with the pack.
