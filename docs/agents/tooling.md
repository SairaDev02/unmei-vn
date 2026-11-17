---
title: Tool obligations, supported fallbacks and delegation
summary: Which kind of tool supplies each obligation, the supported fallback when it is unavailable, and the question, task-list and delegation rules.
keywords:
  - agent-tooling
  - agent-process
---
# Tooling: obligations and fallbacks

Read this page when a task depends on a tool, when you ask the person you work for a set of questions, when you track a multi-step task or when you delegate work. It separates the **obligation** from the **tool**. A tool is one way to supply the evidence, not the evidence itself. Use the tool when your runtime has it. When it does not, use the fallback below. If no fallback supplies the required evidence, report the blocker and do not declare success.

Testing, comment, domain and recovery rules live on their own pages. This page does not restate them.

## Obligations and their fallbacks

| Obligation | Tool, where the runtime has one | Fallback |
| --- | --- | --- |
| A round of questions the person answers | a structured question tool | Ask the same questions in conversation, with your recommendation for each. |
| A task list for multi-step work | the runtime's task tracker | Keep the same items and status changes in your replies. |
| Delegating an independent part of the work | a subagent tool | Do the work yourself. Say which you did. |
| Evidence that the player renders or responds correctly | the player itself | [`docs/testing.md` § Shipped Path](../testing.md#shipped-path) owns this evidence. |

A missing tool never relaxes a validation or safety requirement. Missing rendered, interaction or external-API evidence stays a named blocker. The `gh` CLI has no supported substitute: without it, report the blocker instead of improvising another route.

Unmei ships a native Bevy player, so a browser driver proves nothing about it.

## Questions

Put each decision as one question, with your recommended option first and a reason for it. Ask a batch of related questions together, not one at a time. Without a question tool, ask the same questions in conversation.

## Task list

- Keep a task list on multi-step work. A one-step edit skips it.
- Write one item per outcome you can call done, such as "the workspace tests pass", not one per activity, such as "explore the loader".
- Mark an item in progress when you start it, and done the moment its outcome holds, not in a batch at the end.
- Give a task you discover mid-work its own item. Delete an item that stops mattering.

## Delegation

Delegate when the work splits into independent parts, such as parallel reads, a research question or an isolated review, or when a step needs a context of its own. Work that runs in one pass stays with you.

Make the delegated task self-contained. Name the outcome, the constraints and where the result goes, because the delegate cannot see your conversation. Give a delegate that only checks or researches a read-only tool set, so it reports findings instead of patching what it checks.

Read the delegate's report and inspect the artifacts it points to before you trust it. A delegate's completion is not verification. Report the delegation, and the verification you performed, in your end-of-task report ([closing-comment.md § The reply report](closing-comment.md#the-reply-report)).
