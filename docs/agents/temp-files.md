---
title: Scratch files, prototype branches and their assets
summary: Where disposable scratch lives, how to delete it by explicit path, and how prototype code and assets stay on a retained branch off main.
keywords:
  - repository-upkeep
  - prototype
  - agent-process
---
# Scratch files and prototype branches

Disposable scratch is used and then deleted without being committed. Prototype code and assets are committed only on their own prototype branch and never merged to `main`. Test fixtures and the demo pack are neither: they are maintained files on `main`.

## Scratch files

Keep scratch out of tracked paths. Use a directory outside the clone, or a directory inside it that your clone's `.git/info/exclude` lists, so that `git add` cannot stage it. Do not add a personal scratch entry to the tracked `.gitignore`.

Typical scratch:

- issue and pull request bodies passed to `gh --body-file` (see [github.md](github.md));
- transient tool output, such as a fetched page or a JSON response;
- a large block of content destined for a tracked file. Write it to a scratch file with a file tool and append it, because a very long inline heredoc can be cut off and leave a half-written block behind.

Use a fresh, uniquely named file for each body you post. A reused file can post an earlier draft.

## Deletion

Delete this task's scratch before you finish. Delete by explicit path, one named target at a time. Never delete recursively: no `rm -rf`, no `Remove-Item -Recurse`. Name each file and each empty directory you remove.

Leave scratch you did not create, or whose owner you do not know, for the person you work for to decide on. An ignored or old file is not automatically disposable.

## Prototype branches

A **prototype** is a throwaway artifact made to answer a design question, such as a stub, a rough UI or a comparison sheet. It lives on its own branch, `prototype/<issue>-<name>`, and its code, screenshots and stills stay under `app/prototype/` on that branch.

- `main` never merges the branch, and no `app/prototype/` path reaches `main`. [CONTRIBUTING.md § Local repository invariant checks](../../CONTRIBUTING.md#local-repository-invariant-checks) enforces the second rule in CI.
- The branch is kept and stays pushed, because the issue that decided the question links to it as its source.

## Done

- This task's scratch is removed. Any scratch you left for someone else to decide on is named in your report.
- `git ls-tree -r main -- app/prototype/` prints nothing.
- Prototype commits sit on their pushed `prototype/<issue>-<name>` branch.
