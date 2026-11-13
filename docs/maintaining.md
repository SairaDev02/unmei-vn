---
title: Maintaining Unmei
summary: How the maintainer gates a pull request, triages an issue, invites a trusted reporter, closes a stale thread, sends each saved reply and reviews this policy.
keywords:
  - repository-upkeep
  - issue-triage
  - verification
---
# Maintaining Unmei

This page covers the maintainer's side of the process that [CONTRIBUTING.md](../CONTRIBUTING.md) describes: how a pull request passes, how an issue is triaged, when a reporter is invited to contribute, when a stale thread closes and which saved reply answers each case.

[CONTRIBUTING.md](../CONTRIBUTING.md) states what a contributor must do. This page states what the maintainer does.

## The two pull-request gates

A pull request passes both gates, in this order, before it is merged. Both gates judge the submission: the diff, the tests and the stated evidence. Neither asks who or what produced it. The maintainer's own pull requests pass both gates. The repository ruleset requires no approval, so Gate 2 is the maintainer's own step rather than a check that GitHub enforces.

### Gate 1: the contribution rules

Gate 1 is a checklist about the pull request that needs no reading of the code. Every item must hold, and a failed item closes the pull request with that item cited.

- It links an issue labelled `ready-for-agent` or `ready-for-human`.
- It changes only what the issue requires.
- The required checks pass. These are the jobs in [`.github/workflows/check.yml`](../.github/workflows/check.yml).
- It includes a test that fails without the change, or states the reason no test applies.
- The description states what the author ran and observed, and names any check the author did not run.
- The pull request template is filled in.

### Gate 2: the maintainer's judgment

Gate 2 is the maintainer's own judgment of the change:

- the change is correct, including the edge cases the issue names;
- it follows the existing patterns;
- it adds no dependency, public interface or stored format that the issue did not approve;
- the maintainer can explain the change without the author present;
- the maintainer is content to own the code if the author is gone.

A pull request that fails the last item is declined even when the other items pass.

## Checking the evidence

When Gate 1 passes and the Validation section is in doubt, check the evidence and do not question the author:

1. Run one command from the Validation section and compare its result with the one stated.
2. Keep the change on another branch, because the revert overwrites it.
3. Revert the fix and keep the test.
4. Run the test and confirm it fails.

A result that does not match closes the pull request with the checklist item cited.

## Triage

An issue arrives carrying `needs-triage`. Triage ends in one of three ways: accept the issue, ask for the information it lacks, or close it as `wontfix`. [CONTRIBUTING.md § Contribution paths](../CONTRIBUTING.md#contribution-paths) states what acceptance requires and which labels it applies.

An issue opened through the [question form](../.github/ISSUE_TEMPLATE/question.yml) is answered in the issue and then closed. It is not accepted as work. When the answer shows a defect or a request, the maintainer asks the reporter to open a bug report or a feature request.

For a bug report or a feature request, triage reads these signals:

- a version or commit, which the [bug form](../.github/ISSUE_TEMPLATE/bug_report.yml) requires;
- steps that reproduce the problem, and the behavior the reporter expected instead;
- the environment the problem appeared in;
- for a request, the problem it states and the solution it proposes, which the
  [feature form](../.github/ISSUE_TEMPLATE/feature_request.yml) requires.

When a signal is missing, ask the one question that decides acceptance, apply `needs-info`, and send the information reply in [Saved replies](#saved-replies).

### Pull-request signals

The maintainer reads six signals from a pull request. Who or what produced the submission is not one of them. [CONTRIBUTING.md § Development workflow](../CONTRIBUTING.md#development-workflow) holds the rules a pull request follows.

| Signal | Low-quality sign | High-quality sign |
| --- | --- | --- |
| Problem | The change addresses a problem the issue does not state. | The change solves the problem the linked issue states. |
| Scope | The diff carries refactors, reformatting or features the issue did not ask for. | Every changed file serves the issue's acceptance criteria. |
| Description | The Summary section reports the change and no observed behavior. | The description states the cause, the change and the observed result. |
| Tests | No test, and no stated reason for its absence. | A test that fails without the change, or a stated reason that no test applies. |
| Conventions | A new dependency, stored format or public name the issue did not approve. | The existing patterns and domain vocabulary, with no new dependency. |
| Response to review | A requested change is missing, or a push answers the review with no explanation. | Each requested change is made, or the reason it is not is stated. |

## The trust ladder

Trust rises by what a reporter has already produced. The ladder has three rungs, and each rung is a decision the maintainer makes rather than a setting.

| Rung | Who reaches it | What it allows |
| --- | --- | --- |
| 1 | Anyone | Opens an issue through the [bug form](../.github/ISSUE_TEMPLATE/bug_report.yml), the [feature form](../.github/ISSUE_TEMPLATE/feature_request.yml) or the [question form](../.github/ISSUE_TEMPLATE/question.yml). |
| 2 | A reporter whose two issues were accepted without a `needs-info` round | The maintainer may invite the reporter to open pull requests. |
| 3 | An invited collaborator | Opens pull requests against accepted issues, through [the same two gates](#the-two-pull-request-gates). |

## The closing timeouts

- An issue that carries `needs-info` and receives no answer from the reporter for 14 days is closed.
- A pull request whose requested changes have not arrived for 7 days is closed. A review question that goes unanswered starts no timeout.

Close a thread at its timeout with the saved reply for the case in [Saved replies](#saved-replies).

## The three-month policy review

Every 3 months, review this page and the [CONTRIBUTING.md](../CONTRIBUTING.md) rules it depends on. Review it earlier when any of these changes:

- the label set that triage uses;
- an issue or pull request form;
- the jobs that Gate 1 runs;
- the pull-request access setting.

## Saved replies

Keep one saved reply per case, with this wording. Replace each `<placeholder>` with the value for that thread, and keep the sentence that cites the rule.

**Acceptance reply.** Sent when an issue is accepted. Replace `<label>` with `ready-for-agent` or `ready-for-human`, the label that was applied.

```text
Accepted as `<label>`. The issue states the problem, the acceptance criteria and the out-of-scope work, so the work can proceed against it. `CONTRIBUTING.md` § Contribution paths states what acceptance requires.
```

**Information reply.** Sent with the single question from [Triage](#triage), and it applies `needs-info`. Replace `<question>` with the question whose answer decides whether the issue is accepted.

```text
More information is needed before this issue can be triaged, so it carries `needs-info`. <question>

The issue closes after 14 days without an answer, and a new issue is welcome when the information turns up. `docs/maintaining.md` § Triage states the one-question rule.
```

**Decline reply.** Sent when the issue will not be actioned, and it applies `wontfix`. Replace `<reason>` with the reason the issue is not actioned.

```text
Closed as `wontfix`: this issue will not be actioned. <reason>

You are welcome to maintain the idea in a fork. `CONTRIBUTING.md` § Contribution paths states which issues this repository accepts.
```

**Outside pull-request reply.** Sent to a pull request from someone who is not an invited collaborator.

```text
Thanks for the pull request. Pull requests here are accepted only from invited collaborators, and only against an accepted issue, which carries `ready-for-agent` or `ready-for-human`. `CONTRIBUTING.md` § Contribution paths states both rules.
```

**Invitation reply.** Sent with the invitation at the second rung of [The trust ladder](#the-trust-ladder).

```text
Your last two issues were accepted without a `needs-info` round, so you are invited to open pull requests against accepted issues. `CONTRIBUTING.md` § Contribution paths states what an accepted issue carries. § Development workflow states the rules a pull request follows.
```

**Expired-changes reply.** Sent when a pull request closes because the changes a review requested did not arrive within 7 days.

```text
This pull request is closed because the changes the review requested did not arrive within 7 days. A new pull request is welcome when the changes are ready. `CONTRIBUTING.md` § Development workflow states the rules a pull request follows.
```
