---
title: RON for the pack's structured content
summary: Pack structured data is RON, with LLM-facing JSON converted at the author CLI boundary.
keywords:
  - content-format
  - content-pack
  - authoring-cli
---

# RON for structured content

Pack structured data (manifest, state, NPC definitions, decision configs) is [RON](https://github.com/ron-rs/ron), not TOML, JSON, or YAML. Content is hand-edited and serde-round-tripped by the author CLI, and RON is the only candidate that is simultaneously Rust/serde-native (Bevy's home format), comment-friendly, and expressive enough for tagged, shallowly-nested action-list shapes. LLM-facing JSON is converted at the CLI boundary: what models emit reliably and what authors maintain are intentionally different points in the pipeline.

## Considered options

- **TOML** — familiar, but nested per-option criteria maps get clumsy and there is no enum story.
- **JSON** — what LLMs emit best, but no comments; poor as hand-edited authored content.
- **YAML** — hand-editable, but indentation traps and the weakest Rust serde story.
