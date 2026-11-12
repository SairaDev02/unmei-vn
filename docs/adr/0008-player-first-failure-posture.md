---
title: Player-first failure posture
summary: Failures are classified by what the player gets and can do next, with one narrow Notice and no failure without a log line.
keywords:
  - failure-posture
  - diagnostics
---

# Player-first failure posture: silent degradation, a Notice for loss, and no failure without a log line

The [harden-the-runtime map](../runtime-hardening.md) charted a player-first failure posture in prose: a bad pack gets a readable load-time error, a bad save gets a clear message and the game continues, a missing asset is silence or a placeholder, a malformed pack stops with a readable error. That prose was then turned into an inventory of every failure a player can hit, and the enumeration forced two decisions that are hard to reverse once players have a build: **what a player is told when something breaks**, and **how far a failure is allowed to stop play**. Build-ready detail — the row-by-row inventory, the Notice's mechanics, the error-screen contents, the log file, the test list — is in [`docs/runtime-hardening.md` § Runtime failure posture](../runtime-hardening.md#runtime-failure-posture).

**Decision.** A failure is classified by what the player gets and what they can do next, not by what went wrong. Five postures cover the runtime:

- **Quit-with-message** — a pack that does not load. Nothing can be recovered, so play stops.
- **Continue-with-message** (refuse-and-continue) — an action fails and the player is told: a manual save or autosave that could not be written, a settings flush that could not be written, a load that was refused, a legacy import that failed, an unreadable slot. The action does not happen; play continues.
- **Continue-silently** — a missing or undecodable background, sprite, or track; no audio device; a pack warning; a malformed settings file; a `<pack-dir>.log` that cannot be written. Logged, never shown.
- **Continue-with-fallback** — any Decisions failure (30 s timeout, transport or status error, unusable answer, or no client or pinned model at all). The script takes the branch the author wrote as the config's `fallback`.
- **Defensive** — the three branches the loader's validation makes unreachable (a `decide` with no matching NPC/question, an answer with no label, a runner call that fails). Specified anyway, because the cost of being wrong is a frozen screen: an `error!`, a Notice that says so, and `Esc` as the recovery.

Two rules follow. **Nothing fails without a log line**, under one target (`unmei::failure`), at `error!` when player data is lost or the case is a defect and `warn!` for a silent degradation; the pack-adjacent `<pack-dir>.log` carries the whole session stream, truncated per launch, flushed per line, so the same failure is readable on screen and on disk. And **"game continues" always means the runner reaches the next wait** — a failure never leaves a parked wait without an affordance.

The player-facing surface is one **Notice**: a single transient line, top-right, roughly five seconds of unpaused time, replaced rather than queued, never consuming input. It means exactly one thing — *something you expected to persist did not*, or *the game cannot move* — and it carries the same short string the log line opens with.

Three further choices ride with the posture. A pack that does not load gets an **in-app error screen** (the pack path, up to twenty `file:line: message` lines, then a pointer at the pack's session log (`<pack-dir>.log`), `Esc` to quit) instead of `stderr` plus `std::process::exit`, because the only player who can read `stderr` is not the player this posture is for. A Decisions call keeps its existing **30-second timeout and no retry**: the fallback is authored and in-fiction, and a retry would double latency and cost to replace a branch the author already wrote. And a save slot must satisfy **cheap semantic checks** on top of [ADR 0006](0006-save-slots-on-disk-format-and-version-policy.md)'s decode/version/IO/scene rule — `pc` inside the scene, no wait parked at `pc == 0`, every state key declared in `state.ron`, and the wait's kind matching the line that produced it — because `restore_position` validates only the scene id, so a decodable slot from an older revision of the pack, or one copied in from another, otherwise restores a position that no longer describes the script.

**Why.** The bar is *a friend can play the demo pack comfortably*, and a failure has exactly two useful outcomes: the player can keep going, or the author can fix it. Each class is routed to whichever of those it can serve, and to no one else. Telling the player that a sprite failed to load, or that a decision fell back, serves neither — it interrupts the scene and leaks machinery the authored fallback exists to hide. Telling them that an autosave failed serves the first: an unannounced failed write is unrecoverable player data lost silently, which is the one thing "player-first" cannot mean. The Notice is deliberately narrow for the same reason: a surface that reports everything stops meaning anything, and the silence of the fallback branch is a feature of the content, not an accident of the runtime.

**Considered options.**

- **Fail fast: panic on internal errors.** Rejected. The three unreachable branches are unreachable by the loader's validation, so the alternative to a specified hold is a crash at a moment no author ever anticipated — for cases where an `error!`, a Notice and the existing menu cover the player's entire need.
- **A modal error dialog for player-facing failures.** Rejected: it adds a second pause surface and contradicts the presentation pause, which deliberately never stops the world under the player. The Notice carries the same information without stopping anything.
- **A Notice for every degradation.** Rejected: it inverts the fallback posture (the player is meant never to learn a model was consulted), turns one missing sprite with several staging commands into a wall of text, and decays the Notice's single meaning.
- **Log-only for save failures** (ADR 0006's original posture, recorded in [Runtime hardening — Save slots §4](../runtime-hardening.md#save-slots)). Rejected: silent loss of a checkpoint is the failure a player most needs to hear about, and it is also the one they can act on — save again, or fix the directory.
- **A version or content fingerprint in `SaveFile` to detect a stale or foreign slot by content.** Rejected here: it is a save-format change with its own version-policy consequences, while the cheap structural checks catch the realistic case — a slot from an older revision of the same pack where `pc` has drifted. Recorded as a possible later step if content edits that keep positions valid turn out to bite.
- **`stderr` plus `exit(1)` for a bad pack** (the slice's dev posture). Rejected: the message exists but reaches nobody who launched the game from a file manager, which is exactly the launch path hardening is for.
- **Shortening the Decisions timeout, or retrying once.** Rejected. The provider research put alpha drift at LOW and latency low; the timeout already exists because "the fallback posture needs a failure, not a stall". A retry trades a written, in-fiction branch for doubled latency and cost.
- **A session log in the OS log directory, or no log file at all.** Rejected: the first needs a platform-path dependency and a cleanup policy for a demo-scale need; the second leaves a friend who double-clicks the binary unable to keep or send anything, which is the whole reason `<pack-dir>.log` exists beside the pack with the saves and settings.

## Consequences

- **Silent means silent.** An author cannot tell from the screen that a sprite is missing or a decision fell back; that is what the log file is for. Every failure still logs, so nothing is lost — only unshown.
- **The worst-case stall is 30 seconds** of `… is deciding…`, with no retry. The menu stays available throughout, because `Esc` never cancels the request, so the player is never blocked from pausing, saving or quitting.
- **The Notice may repeat.** A read-only pack directory fails every autosave and every settings flush, and each failure notices again rather than going quiet. Accepted over per-session bookkeeping, which would let a stale message imply success.
- **One string per failure**, shared by the Notice and the log, rewords the runtime's existing per-site log messages; the machine detail (path, IO error, engine error `Display`) stays in the log as a suffix.
- **`<pack-dir>.log` joins the sibling files.** The pack directory stays pristine, and one more artefact appears beside it, truncated at each launch — so a failure that happened in an earlier session is gone. Rotation and retention are recorded as fog on the map.
- **This record tightens ADR 0006's unreadable rule** (the semantic slot checks) and **supersedes the log-only posture** for a failed autosave (`docs/runtime-hardening.md` § Save slots §4) and for an unwritable settings file (§ Audio settings §7). Those sections, ADR 0006 and `GLOSSARY.md` were amended in place when this record landed: § Save slots §3–4 and §7, § Audio settings §7–8, a dated amendment note on ADR 0006, and the five terms — Notice, Defensive failure, Silent degradation, Unreadable slot, Error screen.
- No `engine/` error type is unified or added: the four typed errors and `LoadError`'s `Vec` are the seam as they stand, and `engine/` stays Bevy-free. The app gains one failure module, one `Notice` resource and no `unwrap`, `expect`, `panic!` or `process::exit` in a play path.

## Amendment (2026-10-01)

The log file this record describes is `<pack-dir>.log` — the pack-sibling derivation, which the **Consequences** section already used. The earlier working name `unmei.log` in this record's body and in [`docs/runtime-hardening.md`](../runtime-hardening.md) referred to the same file; it has been corrected in place, and the error screen's pointer now derives the name from the same path helper that writes the file rather than formatting a fixed string, so the name a player reads and the file that exists cannot diverge again. No posture, failure class or routing above changes.

## Amendment (2026-10-11)

The **Consequences** bullet "the machine detail (path, IO error, engine error `Display`) stays in the log as a suffix" no longer holds for the Decisions transport. [Monitor → Log stream](../runtime-hardening.md#log-stream) makes credential safety a prerequisite of the Monitor viewer, and the credential-surface review showed that a `DecisionError`'s raw transport text, response body, echoed model or echoed score legend can reflect a credential into the fallback reason, the console and `<pack-dir>.log`. The credential-safety decision therefore makes every `DecisionError` `Display` categorical and restricts the response model to the requested pin (else the fixed `unverified response model`). Every other row keeps its machine-detail suffix; no posture, failure class, target, level or routing changes, and the appraisal of "one string per failure" is intact — the short string is unchanged, only the Decisions machine suffix is bounded.
