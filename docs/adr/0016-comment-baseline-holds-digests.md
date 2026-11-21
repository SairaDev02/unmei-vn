---
status: accepted
title: The comment baseline holds a digest per comment
summary: The comment gate's baseline records each frozen comment's digest, not a per-file count, so a comment swapped for another one fails the gate.
keywords:
  - code-comments
  - repository-upkeep
---

# The comment baseline holds a digest per comment

The comment gate, [`scripts/no-new-comments.py`](../../scripts/no-new-comments.py), first stored one comment count per file in `.comment-baseline.json`. Issue #133 showed that a count cannot tell one comment from another. If you delete a contract and add three lines of narration in the same file, the count stays the same and the gate stays green. The next `--tighten` then freezes the narration.

**Decision.** The baseline maps each file to the digests of its in-scope comments, one digest per comment. A digest is the first 8 hex characters of the SHA-256 of the comment text with its whitespace collapsed. Moving a comment or changing its indentation keeps its digest. The gate fails when a file holds a digest that its entry does not hold, counting repeated digests as separate comments. `--tighten` drops digests that the tree no longer holds, admits a file that the baseline has never counted, and refuses any other new digest. `--tighten --reword PATH` accepts new digests in that one file as long as its count does not rise. The swap then appears as a change to that file's entry in the baseline diff, where review sees it.

## Considered options

- **Keep counts and rely on review for swaps.** Rejected: a swap changes nothing in the baseline, so review has nothing to notice.
- **Store a count and one digest of each file's whole comment set.** Rejected: any change to the file fails the gate, but the gate cannot say which comment changed.
- **Full SHA-256 digests, one per JSON line.** Rejected for size. The threat is an accidental or lazy swap, not a crafted collision, and 32 bits per digest is enough to tell the comments in one file apart.

## Consequences

- **Rewording a comment now takes a deliberate step.** A `SHORTEN` under [comments.md](../agents/comments.md) needs `--tighten --reword PATH`. That step is the point of this design.
- **The baseline is about 47 KB**, above the 20,000-byte size budget for authored files. It has a recorded exception in [`scripts/source-size-exceptions.json`](../../scripts/source-size-exceptions.json). That entry's ceiling must be raised whenever an admitted file makes the baseline grow past it.
- **A collision of 8-hex digests within one file would hide a swap.** With a few dozen comments per file, the chance is negligible.
