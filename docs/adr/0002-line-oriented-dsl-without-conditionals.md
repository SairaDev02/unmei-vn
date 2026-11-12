---
title: Line-oriented dialogue DSL without conditionals
summary: The dialogue DSL is line-oriented with no indent stack; conditionals were excluded, then added by ADR 0004.
keywords:
  - dialogue-dsl
  - content-format
---

# Line-oriented dialogue DSL without conditionals

> **Amended by [ADR 0004](0004-conditionals-in-the-dialogue-dsl.md)**: conditionals were added once a demand case existed. The line-oriented, no-indent-stack constraint below still holds; the conditionals exclusion does not.

Scenes are line-oriented `.unmei` files (one command per line, `Speaker: text` say-lines, cosmetic indentation), not indentation-block, Ink-like weave, or Yarn-node syntax, and the grammar deliberately has **no conditionals** (`if`, conditional jumps, arithmetic). The slice needs exactly four flow pillars (say, player choice, labels/jumps, flat set/refs); a line scanner parses the whole grammar with no indent stack, and excluding conditionals keeps branching = player choice + decisions only.

The exclusion is load-bearing downstream: because nothing can branch on a computed value, decision answers must be action ids — which is why decision configs are choice-only (ADR 0003). Revisit conditionals only when a real scene demands them; `noul`/`score` question types become expressible at the same time.
