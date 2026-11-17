---
title: Reading the glossary and ADRs before exploring code
summary: "How to use GLOSSARY.md and docs/adr while exploring: use the glossary vocabulary and surface a contradiction with an existing ADR."
keywords:
  - domain-modeling
  - agent-process
---
# Domain docs

This page says how to use the repository's domain documentation while you explore the code.

## Before exploring, read these

- [`GLOSSARY.md`](../../GLOSSARY.md) at the repository root.
- The records in [`docs/adr/`](../adr/) that touch the area you are about to work in.

The repository has one context, so it has one glossary.

## Layout

```
/
├── GLOSSARY.md
├── docs/adr/   numbered architecture decision records
├── engine/     the unmei-engine crate: parser, pack loader, decision layer, headless runner
├── cli/        the unmei crate: the authoring CLI
└── app/        the unmei-app crate: the Bevy player, unmei-play
```

[`docs/architecture.md`](../architecture.md) owns the crate boundaries and maps each contract to the page that owns it.

## Use the glossary's vocabulary

When your output names a domain concept, such as in an issue title, a proposal, a hypothesis or a test name, use the term as `GLOSSARY.md` defines it. Do not drift to a synonym that its _Avoid_ list names.

If the concept you need is not in the glossary, either you are inventing language the project does not use, or there is a real gap. Reconsider the first. Report the second.

When your change adds a term to the model, add it to `GLOSSARY.md` in the same change.

## Flag ADR conflicts

If your output contradicts an existing ADR, say so explicitly instead of silently overriding it:

> _Contradicts ADR 0008 (player-first failure posture), but worth reopening because…_
