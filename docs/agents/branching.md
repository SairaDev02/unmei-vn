---
title: Branch lifecycle from creation to deletion
summary: How work branches are named and worked, the cleanup after a squash merge, the retained branch kinds and the test for deleting a branch safely.
keywords:
  - repository-upkeep
  - agent-process
---
# Branch lifecycle

`main` changes only through a pull request, and pull requests here are **squash-merged**. A merged branch therefore never becomes an ancestor of `main`. Every rule below follows from that.

## Create

- Branch from a freshly fetched `origin/main`: run `git fetch origin`, then `git switch -c <name> origin/main`.
- Use one branch per issue, named `<type>/<issue>-<slug>`. The type is `feat`, `fix`, `docs`, `chore`, `ci`, `test` or `refactor`, and the slug is a few lowercase words. This is the short, descriptive name that [CONTRIBUTING.md § Development workflow](../../CONTRIBUTING.md#development-workflow) asks for, with the issue number added.
- Work built across several tickets gets one **integration branch**, `spec/<issue>-<slug>`. Its tickets merge into that branch, and only that branch opens the pull request to `main`.
- A prototype branch, `prototype/<issue>-<name>`, follows [temp-files.md](temp-files.md). It stays pushed for as long as an issue links to it.

## Work

- When the work ends, the main checkout is back on an up-to-date `main` with no changes.
- Delegated or parallel work runs in its own worktree. Remove the worktree and its scratch branch once the branch it produced is pushed or merged.
- Commit locally. Push when the person you work for asks you to.

## After the merge

1. Run `git switch main`, then `git pull --ff-only origin main`.
2. Delete the local branch with `git branch -D <name>`, once its tip is the merged pull request's head or `main` holds all of its content. `git branch -d` refuses here, because a squash merge leaves no ancestry.
3. GitHub deletes the remote branch on merge. A ticket's branch on an integration branch goes once the integration branch merges.

A squash-merged branch is finished. A commit that still has to land goes on a fresh branch from `origin/main`, carried over with `git cherry-pick`.

## Safe-deletion test

A branch is **safe** to delete when every commit it holds is reachable from somewhere else:

- `origin/main`, by ancestry or equal content (`git cherry origin/main <branch>` marks equivalent patches);
- a pushed remote branch (`git branch -r --contains <tip>`);
- the head of a merged pull request (`gh pr list --state merged --json headRefName,headRefOid`).

Ask before you delete any other branch: one with unpushed commits, abandoned unmerged work or an `archive/` name. A branch that `main` cites, and a retained prototype branch, stay.

Deleting a remote branch needs approval each time.
