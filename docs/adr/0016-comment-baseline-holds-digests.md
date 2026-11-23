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

**Decision.** The baseline maps each file to the digests of its in-scope comments, one digest per comment. A digest is the first 8 hex characters of the SHA-256 of the comment text with its whitespace collapsed. Moving a comment or changing its indentation keeps its digest. The gate fails when a file holds a digest that its entry does not hold, counting repeated digests as separate comments. `--tighten` drops digests that the tree no longer holds and refuses every new digest. `--tighten --reword PATH` accepts new digests in that one file as long as its count does not rise. `--tighten --admit PATH` accepts the comments of a file that the baseline has never counted, including a renamed file, whose old entry drops out in the same run. A new file with no in-scope comments needs no flag. Either way, the change appears in that file's entry in the baseline diff, where review sees it.

**Amended 2026-11-21 (#139).** `--tighten` first admitted a never-counted file with all its comments and no flag, so a new file was the one way in that needed no deliberate step. It now needs `--admit`. Renamed files are not detected by matching digests: `--admit` is the one path for any file new to the baseline.

**Amended 2026-11-23 (#151).** A trailing YAML comment whose whole body is a version tag, such as `# v2.9.3`, is out of scope. Dependabot reads that tag and rewrites it when it bumps the SHA on a pinned `uses:` line, so the comment is tool-maintained in the way a directive is. Counting it made every Dependabot pull request fail the required `fmt, clippy, test` job until a person ran `--reword`, and a push to Dependabot's branch stops Dependabot from rebasing that pull request. The exemption covers YAML files only, and only a comment that trails other text and holds nothing but the tag: an own-line `# v2.9.3`, a tag with any other text and a tag in another file type all still count.

## Considered options

- **Keep counts and rely on review for swaps.** Rejected: a swap changes nothing in the baseline, so review has nothing to notice.
- **Store a count and one digest of each file's whole comment set.** Rejected: any change to the file fails the gate, but the gate cannot say which comment changed.
- **Full SHA-256 digests, one per JSON line.** Rejected for size. The threat is an accidental or lazy swap, not a crafted collision, and 32 bits per digest is enough to tell the comments in one file apart.

## Consequences

- **Rewording a comment now takes a deliberate step.** A `SHORTEN` under [comments.md](../agents/comments.md) needs `--tighten --reword PATH`. That step is the point of this design.
- **The baseline is about 47 KB**, above the 20,000-byte size budget for authored files. It has a recorded exception in [`scripts/source-size-exceptions.json`](../../scripts/source-size-exceptions.json). That entry's ceiling must be raised whenever a file admitted with `--admit` makes the baseline grow past it.
- **A collision of 8-hex digests within one file would hide a swap.** With a few dozen comments per file, the chance is negligible.
