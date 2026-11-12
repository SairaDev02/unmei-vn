<!-- New issues start as `needs-triage`; a maintainer moves them through the triage labels listed in CONTRIBUTING.md § How to report an issue -->

## Summary

Closes #

<!-- Required: the accepted issue this pull request implements. A pull request is accepted only against an issue labelled `ready-for-agent` or `ready-for-human`; CONTRIBUTING.md § Contribution paths states what acceptance requires. -->

<!-- What does this pull request change, and why? -->

**Acceptance criteria:**

<!-- Link the issue's criteria or state the expected outcome directly. -->

## Changes

-

**Implementation considerations:**

<!-- Non-obvious assumptions or architectural trade-offs; `none` is valid. -->

## Validation

<!-- The exact commands you ran and the result you observed — pass/fail, counts, or output. Write `not run` with the reason for any check you skipped. A checked box below is a claim, not proof a command ran; this section is where a command's evidence goes. CONTRIBUTING.md § Local pre-commit checks says which local checks to run before you push. Keep this section short. -->

- `<command>` — `<observed result>`
- `not run` — `<check skipped, and why>`

**Observable behavior verified:**

<!-- What a player or caller can now see or do, and the test or walk that made it happen. Write "N/A, docs-only" when nothing user-visible changed. -->

**Untested risks, blockers, or skipped checks:**

<!-- None, or what remains unverified and why: environmental blockers, credentials, platform, an unrun mode. -->

## Checklist

- [ ] The linked issue is labelled `ready-for-agent` or `ready-for-human`
- [ ] The diff contains only what the issue requires
- [ ] Tests included or updated for behavior changes (or reason stated)
- [ ] If this change alters a contract, update the pages that present it in this PR. The "Presented in" column of the [contract map](../docs/architecture.md#contract-map) lists them, and "—" means no page presents the contract.
- [ ] Uses domain vocabulary from `GLOSSARY.md`
- [ ] No contradictions with existing [ADRs](../docs/adr/), or each contradiction is called out in the Summary
