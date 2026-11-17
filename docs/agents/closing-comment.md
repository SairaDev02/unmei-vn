---
title: Closing comment for an implementation issue
summary: "The hand-off comment on an implementation issue at close: what landed, the acceptance walk, the validation output and the deferred work."
keywords:
  - issue-tracker
  - agent-process
  - verification
---
# Closing comment

The **closing comment** is the hand-off left on an implementation issue when its work lands: what landed, the acceptance walk, the validation output and what was deferred. A reader gets the result from one comment instead of from `git log`.

Write it after the work is committed. When a pull request closes the issue, post the comment on the issue once the pull request merges. Do not push only to finish the hand-off: push when the person you work for asks you to.

## Steps

1. **Gather the evidence.** Collect the commit SHA and the diff stat, the issue's acceptance criteria and its out-of-scope list, the real output of the validation commands and any review finding that changed the code.

   Also search for code comments that promised this issue's work. With `<n>` as the issue number, run `rg -n '#<n>\b' --glob '*.rs'` and `rg -ni 'later slice|not built yet|when it lands' --glob '*.rs'`. Correct or delete every hit that this work has made false.

   *Done when* every claim you plan to make has a SHA, a command, a file or an issue line behind it, and no comment still promises finished work.

2. **Draft the skeleton.** Copy the four headings of the [template](#template), in order.

   *Done when* all four headings are present and none is empty.

3. **Fill "What landed".** Write one bullet per file or area, naming the behavior it adds rather than its mechanics. Open with the SHA and the branch.

   *Done when* every changed file whose behavior a player, an author or the next contributor can observe has a bullet, and the SHA is exact.

4. **Walk the acceptance criteria.** Copy the issue's criteria word for word, one checklist line each, in the issue's order. Tick a box only where a test or a command exercises the criterion. Leave the rest unticked and say what is missing.

   *Done when* the number of lines equals the number of criteria, with no paraphrase and no omission.

5. **Record the validation.** List the exact commands and the result you observed, including new test counts.

   *Done when* every command can be copied and run, and its result is the one you saw.

6. **Record the deferrals and residuals.** A **deferral** is work the issue explicitly excluded: quote the exclusion. A **residual** is a gap, approximation or deviation the work leaves behind. Give each one a follow-up.

   *Done when* every difference between the work and the issue is either gone or listed here.

7. **Post.** Run `gh issue comment <n> --body-file <path>`. If the issue is still open and the work is complete, run `gh issue close <n>`.

   *Done when* the comment URL is returned and the issue reads `CLOSED`.

## Template

````
Implemented in `<sha>` on `<branch>`.

## What landed

- **`path/to/file`**: the behavior it adds.

## Acceptance criteria

- [x] <criterion, copied word for word from the issue>
- [ ] <criterion>: <what is missing>

## Validation

- `cargo test --locked --workspace`: …
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: clean.
- `cargo fmt --all -- --check`: clean.

## Deferrals and residuals

- **<name>**: deferral, "<quotation from the issue>"; <follow-up>.
- **<name>**: residual, <what it leaves>; <follow-up>.
````

## The reply report

End every implementation task with a short report to the person you work for, in three parts:

- **What landed:** the files or areas, named by the behavior they add.
- **Validation:** the exact commands you ran and the result you observed.
- **What remains:** deferrals, residuals and the next step, if any.

Keep it to a handful of bullets. The closing comment is the long form.
