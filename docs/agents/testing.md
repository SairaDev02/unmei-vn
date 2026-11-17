---
title: Pointer to the testing policy for agents
summary: Where the test-selection policy lives, which skill routes a change to its smallest check, and how an agent reports what it verified.
keywords:
  - verification
  - agent-process
---
# Testing: pointer

[`docs/testing.md`](../testing.md) is the testing authority. It owns what to test in each crate, how to fake an external dependency, the validation gates, the shipped-path evidence and the acceptance rule. Read it before you choose or write a test. This page restates none of it.

## Where the rest lives

- **Routing a change to a check:** the `unmei-verify` skill at [`.agents/skills/unmei-verify/SKILL.md`](../../.agents/skills/unmei-verify/SKILL.md) names the smallest existing check that can prove a change. It adds no policy, no suite and no CI step.
- **Reporting the result:** [closing-comment.md § The reply report](closing-comment.md#the-reply-report) holds the shape of the end-of-task report.

## Agent-only notes

- A claim rests on a command you ran in this session, with its result. Do not cite an earlier pass as a current observation.
- Report the checks that ran, their results and the behavior left unverified.
