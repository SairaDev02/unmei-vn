---
title: GitHub issue reads and edits through the gh CLI
summary: Why a bare gh issue view can fail, the field sets that work, and the REST route for editing a body after creation.
keywords:
  - issue-tracker
  - agent-tooling
---
# GitHub issues through the `gh` CLI

## Always pass `--json` with explicit fields

Without `--json`, `gh issue view` asks the GraphQL API for the issue's `projectCards`. GitHub retired Projects (classic), so on `gh` releases that still request that field the call fails with a `Projects (classic) is being deprecated` error and returns no data.

Passing `--json` switches the command to the field path you name and stops the request for `projectCards`. The rule is uniform: **name the fields you want**.

| Command | Without `--json` | With `--json` |
| --- | --- | --- |
| `gh issue view` | deprecation error, no data | works |
| `gh issue view --comments` | same error | works |
| `gh issue list` | works | works |

## Field sets that work

Read one issue, with its body and comments:

```sh
gh issue view <number> --json number,title,state,author,labels,assignees,milestone,body,comments
```

List issues:

```sh
gh issue list --state all --search <term> --json number,title,state,labels,comments
```

Start from the nearest set and add to it. `--json` rejects a field name it does not know, so a typo fails loudly instead of dropping data. When you need a raw field that `gh` does not model, `gh api` reaches it directly.

## Editing after creation

`gh pr edit` can fail the same way, and it has no `--json` form to avoid the field. Use the REST endpoint instead:

```sh
gh api -X PATCH repos/<owner>/<repo>/pulls/<n> -F body=@<body-file>
gh api -X PATCH repos/<owner>/<repo>/issues/comments/<id> -F body=@<body-file>
```

- Use `-F body=@<file>`, not `-f body="$(cat <file>)"`. The file form keeps line breaks and non-ASCII punctuation that an inline argument can mangle. [temp-files.md](temp-files.md) says where the body file lives.
- The comment `<id>` is the numeric id in the comment URL. Read the published body first with `gh api repos/<owner>/<repo>/issues/comments/<id> --jq .body`, so the new text starts from what is actually there.
- Creating still works through the plain commands: `gh pr create --body-file <file>` and `gh issue comment <n> --body-file <file>`.
- Read a pull request on the field path: `gh pr view <n> --json number,title,state,isDraft,baseRefName,headRefName,mergeable,url,body`.

## Labels

A new issue gets a triage label. [triage-labels.md](triage-labels.md) maps the triage roles to label strings, and [issue-tracker.md](issue-tracker.md) covers creating, listing and labeling issues.

## Done

`gh issue view --json <fields>` or `gh issue list --json <fields>` returns without the deprecation error, and the named fields carry the data the task needed. An edit that `gh pr edit` cannot make returns the updated object's `html_url` through `gh api -X PATCH … -F body=@<file>`.
