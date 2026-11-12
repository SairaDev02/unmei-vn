---
title: The Unmei Guide
summary: Find the Guide page for playing the demo, writing a pack, contributing to Unmei or fixing a failed launch or load.
keywords:
  - content-pack
  - authoring-cli
  - decisions
  - persistence
  - failure-posture
---
# The Unmei Guide

Pick the page for your task: playing the demo, writing a pack, contributing or fixing a problem.

If you use a release, follow these pages as they stood at that release's tag. Each page links the
contract that owns a rule instead of restating it. A contract is the glossary, a spec or a decision
record, and the [`docs/architecture.md` § Contract map](../architecture.md#contract-map) lists them.
When a page and a contract differ, trust the contract.

## Play

- [`play-the-demo.md`](play-the-demo.md): choose and extract the player archive for your platform,
  launch the demo, and save and load from the menu overlay that Esc opens.
- [`live-decisions.md`](live-decisions.md): add an OpenRouter key so a model chooses how Ace, the
  demo's non-player character (NPC), greets you, with what each request sends and costs. This is
  optional: with no key, the demo plays end to end. Read `play-the-demo.md` first.

## Write

A content pack is the folder that holds everything one game needs to play
([`GLOSSARY.md` § Content & packs](../../GLOSSARY.md#content--packs)). The authoring command-line
tool (the CLI) works on a pack.

- [`install-the-cli.md`](install-the-cli.md): get the CLI from its archive or with Cargo, set the API
  key environment variable and give the CLI a model.
- [`write-your-first-pack.md`](write-your-first-pack.md): copy the demo pack, change a scene line, add
  a choice, validate the pack and play it. Read `install-the-cli.md` first.
- [`format-overview.md`](format-overview.md): what each part of a pack is, and the section of the
  content format that owns it.
- [`author-with-the-cli.md`](author-with-the-cli.md): the worked `unmei author npc` and
  `unmei author draft` flows.

## Contribute

- [`docs/architecture.md`](../architecture.md): the three crates and the boundary each keeps, the two
  data flows and the contract map from every contract to the pages that present it.
- [`CONTRIBUTING.md`](../../CONTRIBUTING.md): how to report an issue, propose a change and run the
  local checks.
- [`README.md` § Run from source](../../README.md#run-from-source): run the player and the CLI from
  source.

## Something is wrong

- [`troubleshooting.md`](troubleshooting.md): a Linux launch that opens no window, Windows blocking
  the unsigned player, a pack or a save that will not load, where the session log is and what to
  attach to a bug report.
