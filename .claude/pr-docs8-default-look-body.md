## Summary

Part of #8, step 1: the spec amendments. The Bevy build is step 2 and closes the issue, so this PR does not.

Settles the default look that the prototype chose (variant E) as one new section, `docs/title-and-menu.md` § Default look, and marks every layout rule it supersedes with a dated pointer. Nothing is built: the section's banner reads **Not built**, and the code still matches the old rules.

```diff
 docs/title-and-menu.md
   § Title layout        §1, §2 art bullet (no veil), §4, §5, §7, §8 layout tests → superseded
   § Type system         §2 Title / Dialogue band / Esc overlay rows, §4 dialogue bullet → superseded
   § Timeline › Layout   rail → superseded
+  § Default look        new: play frame, choices, title, contents page, Monitor clearance, faces, tests
 docs/runtime-hardening.md
   § Menu overlay        §1 layout, §6 grid walk → superseded
   § Player comfort      §4 AUTO cue, §6 help line → superseded
   § Runtime failure     §3 Notice placement reason → amended (the slot stands)
   § Text skip           §7 cue, help line, dialogue reservation → superseded
   § Monitor › The panel drawer y 212–572 → y 212–500
```

The look, by surface:

```text
play frame     dialogue box (y 514–692) + name tab + ▼ marker
               quick row: Save · Load · Auto A · Skip Tab · [Log, once the Backlog exists] · Menu Esc
               lit Auto A / Skip Tab replace the AUTO/SKIP corner cue; no help line
choices        numbered buttons centred; the question stays in the box
title          left column over a veil on the pack art
Esc menu       opaque contents page: sections left, slot ledger right, Resume / Exit to title at the foot
```

**Acceptance criteria:** from #8, "The amended specs describe E and record the date and the reason for each change." The build criteria in #8 (layout at 1280×720, controls unchanged) belong to step 2.

## Changes

- `docs/title-and-menu.md`: new § Default look (§1–§7, Amendments, Out of scope); the Sections-so-far entry; dated pointers and Amendments entries in § Title layout, § Type system and § Timeline.
- `docs/runtime-hardening.md`: dated pointers in § Menu overlay (§1, §6 and its Amendments), § Player comfort §4 and §6, § The Notice, § Text skip §7 and § The panel; the Sections-so-far entry.

**Implementation considerations:**

- Decisions taken with the maintainer: the Log control waits for a separate Backlog spec, and until then the quick row has no Log control; the quick-row labels carry the keys and replace the help line; the lit labels replace the corner cue; the Monitor drawer moves up rather than covering the box; the title gets a veil, which reverses Title layout §2's no-veil rule.
- Calls made in the draft, open to review: narration has no italics (Type system ships no italic face; it uses the dim tone instead); the question at a choice is presentation state only, so after a load onto a choice-wait the box is empty; `Exit to title` moves from the Settings rail to the contents page's foot; Settings from the title opens the contents page with only its Settings entry; the Gallery's title door keeps its current layout.
- **Merge danger:** a two-way door. The change is text only and reverts cleanly; no code, format or save data reads it. Blast radius: specs. The pages that present these contracts (`docs/guide/play-the-demo.md`, `packaging/player-README.md`) still describe the shipped card grid, the Settings route to `Exit to title` and the grid's arrow keys. They are correct for 0.1.0 and should change in the build PR, when the new look ships.
- A mismatch found, not fixed here: `docs/guide/play-the-demo.md` says the title opens "with the first scene's opening frame frozen behind it", while § Title layout §2 says no play frame is drawn behind the title.

## Validation

- `python scripts/docs.py check`: passes (54 tracked Markdown paths classified, 3 index files fresh, 299 relative links resolve).
- `python scripts/check-invariants.py --rules eol`: passes, no CRLF.
- `docs.py` does not check links inside the specs, so each new anchor (`#default-look`, `#the-panel-q1q2-verdict-a`, `#3-the-notice`, `#layout-1`, `content-format.md#say-lines-and-narration` and the section anchors) was checked against the headings' slugs with a one-off script: all resolve.
- `not run`: cargo tests, fmt and Clippy, because no code changed.

**Observable behavior verified:** N/A, docs-only.

**Untested risks, blockers, or skipped checks:** the geometry comes from the HTML prototype (`prototype/8-vn-ui-redesign`, `app/prototype/vn-ui.html`). It uses only primitives Bevy 0.19 UI draws, but the Bevy rendering is unproven until the build.

## Checklist

- [ ] The linked issue is labelled `ready-for-agent` or `ready-for-human` (#8 has no triage label yet)
- [x] The diff contains only what the issue requires
- [x] Tests included or updated for behavior changes (or reason stated): no behavior change; § Default look §7 lists the build's tests
- [ ] If this change alters a contract, update the pages that present it in this PR: deferred to the build PR, because the presenting pages describe what ships and nothing new ships here
- [x] Uses domain vocabulary from `GLOSSARY.md`
- [x] No contradictions with existing [ADRs](../docs/adr/), or each contradiction is called out in the Summary: ADR 0013's one-owner-per-surface partition is untouched
