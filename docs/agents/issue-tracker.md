---
title: GitHub issue tracker operations and conventions
summary: How issues, sub-issues, blocking dependencies and wayfinding maps are created, linked, labeled and closed in this repository.
keywords:
  - issue-tracker
  - issue-triage
  - agent-process
---
# Issue tracker: GitHub

Issues and their acceptance criteria live as GitHub issues in this repository. Use the `gh` CLI for every operation. [CONTRIBUTING.md](../../CONTRIBUTING.md) owns which issues are accepted and who may open a pull request against one.

## Conventions

- **Create an issue:** `gh issue create --title "..." --body-file <file>`.
- **Read an issue:** `gh issue view <number> --json <fields>`, never bare. [github.md](github.md) holds the field sets. Request comments explicitly.
- **List issues:** `gh issue list --state open --json number,title,body,labels,comments`, with `--label` and `--state` filters as needed.
- **Comment on an issue:** `gh issue comment <number> --body-file <file>`.
- **Apply or remove labels:** `gh issue edit <number> --add-label "..."` or `--remove-label "..."`.
- **Close:** post the explanation with `gh issue comment`, then run `gh issue close <number>`. The `--comment` option of `gh issue close` can silently drop the comment on an issue that is already closed. For completed implementation work, follow [closing-comment.md](closing-comment.md).

Before the first issue you create in a session, run `gh repo view --json nameWithOwner` and confirm that the target is `SairaDev02/unmei-vn`. Stop and ask if it differs. A clone or a fork can otherwise select another repository.

## Ticket decomposition

When an issue is split into tickets, link each ticket to its parent as a **native GitHub sub-issue** in the same run. With `gh` 2.94 or later, use `gh issue create --parent <parent> ...` or `gh issue edit <parent> --add-sub-issue <child>`. With an older `gh`, use:

```sh
gh api repos/<owner>/<repo>/issues/<child> --jq .id
gh api --method POST repos/<owner>/<repo>/issues/<parent>/sub_issues -F sub_issue_id=<child-db-id>
```

Read `gh api repos/<owner>/<repo>/issues/<parent>/sub_issues` to confirm the link. A `## Parent` line in the body is not a sub-issue link.

## Blocking dependencies

Use GitHub's native issue dependencies. With `gh` 2.94 or later, add an edge with `gh issue create --blocked-by <n>,<n> ...` or `gh issue edit <child> --add-blocked-by <blocker>`. With an older `gh`, use `gh api --method POST repos/<owner>/<repo>/issues/<child>/dependencies/blocked_by -F issue_id=<blocker-db-id>`. The blocker's database id comes from `gh api repos/<owner>/<repo>/issues/<n> --jq .id`, not from the issue number or its `node_id`. If native dependencies are unavailable, put `Blocked by: #<n>, #<n>` at the top of the child issue body.

Count open blockers with `gh api repos/<owner>/<repo>/issues/<n> --jq .issue_dependencies_summary.blocked_by`. Zero means unblocked. `gh issue view <n> --json blockedBy` can include closed blockers. To count only open ones, add `--jq '[.blockedBy.nodes[] | select(.state == "OPEN")] | length'`.

## Pull requests are not a request surface

Pull requests here come only from invited collaborators and only against an accepted issue. A pull request is therefore not triaged as a feature request. A request arrives as an issue.

GitHub shares one number space across issues and pull requests, so a bare number may be either. Resolve it with `gh pr view <n> --json number,title`, and fall back to `gh issue view <n> --json number,title`.

## Wayfinding operations

A wayfinding map plans work that is not yet clear enough for one accepted issue. The **map** is a single issue, and its **child** issues are the tickets.

- **Map:** one issue labeled `wayfinder:map`, whose body holds Notes, Decisions so far and Fog. Create it with `gh issue create --label wayfinder:map`.
- **Child ticket:** an issue linked to the map as a sub-issue (see [Ticket decomposition](#ticket-decomposition)). Where sub-issues are not enabled, add the child to a task list in the map body and put `Part of #<map>` at the top of the child body. Label it `wayfinder:research`, `wayfinder:prototype`, `wayfinder:grilling` or `wayfinder:task`. A claimed ticket is assigned to the person driving it.
- **Blocking:** follow [Blocking dependencies](#blocking-dependencies). A ticket is unblocked when every blocker is closed.
- **Frontier:** list the map's open children, then drop each one that has an open blocker or an assignee. The first remaining child in map order is next.
- **Claim:** `gh issue edit <n> --add-assignee @me`, as the session's first write.
- **Resolve:** post the answer with `gh issue comment <n>`, close the ticket with `gh issue close <n>`, then add a one-line summary and a link to the map's Decisions so far.

Before you open a research ticket, check the map's Decisions so far. Cite a settled finding instead of researching it again.
