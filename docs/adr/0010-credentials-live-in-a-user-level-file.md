---
title: Credentials live in a user-level file
summary: The OpenRouter key lives in one user-level plain-text file; preferences stay in the pack's settings file.
keywords:
  - credentials
  - settings-file
---

# Credentials live in a user-level file; preferences stay pack-level

The beta trajectory adds the player's OpenRouter API key — the first secret the runtime has ever stored. Every other sidecar the runtime writes is a **sibling of the pack directory**: the four save slots, `<pack-dir>.settings.ron`, `<pack-dir>.log`, and `<pack-dir>.read.ron`. [ADR 0007](0007-settings-file-beside-the-pack.md) explicitly **rejected** a user-level file for preferences on portability grounds. A credential is the value that must not follow that convention: the pack directory has to stay shareable — zipped, attached to a bug report, copied to another machine — without carrying a working key. Two things are hard to reverse once players have files on disk: **where the key lives**, and **whether preferences and credentials share one home**. Build-ready detail — the file format, the path helper, the row's states and the test list — is in [`docs/title-and-menu.md` § Decisions settings](../title-and-menu.md#decisions-settings).

**Decision.** The OpenRouter API key lives in a **user-level plain-text file** holding the API key and nothing else: `%APPDATA%/unmei/credentials` on Windows, `~/.config/unmei/credentials` on Linux (`$XDG_CONFIG_HOME` honoured when set). Preferences **stay pack-level** in `<pack-dir>.settings.ron`; ADR 0007's rejected option remains rejected for them. `OPENROUTER_API_KEY`, when non-empty, **overrides** the stored key rather than replacing it. The file is read at launch (to build the resolver), on every Settings-view open (so a hand-edit is picked up with no watcher), and after every in-app write; `app/` owns the path derivation, the atomic write (Unix `0600`), the read posture and the row. The `unmei` CLI keeps its **env-only** key contract for 0.1.0.

Why the asymmetry, in one line: **preferences are cheap, reproducible, and belong to a pack; a credential is valuable, dangerous to share, and belongs to a person.** A player can restore a mix by dragging three sliders; they cannot un-leak a key that travelled inside a pack directory. ADR 0007's reasoning pays for the settings file's permissiveness — unknown keys ignored, a malformed file falling back to defaults, the next write dropping what it does not know — which is right for a slider and wrong for a secret. The credential file is therefore deliberately outside that permissive contract, and its read path is written so that **no failure message can ever quote the value**.

## Considered options

- **The key in `<pack-dir>.settings.ron`** (the "one preferences file" reading). Rejected: it puts a working credential in the file a player is most likely to paste into a bug report, making "share the pack" and "share my key" the same act; it also inherits the file's lossy rewrite rules (a future build's unknown keys are dropped on the next write), which is exactly the behaviour a secret must not have.
- **The key beside the pack** (`<pack-dir>.key` or similar). Rejected: a portability trap — the pack directory is the unit the project tells players to zip and send, so the key would travel by default; it also needs a pack-relative ignore rule and would sit among files the demo's `.gitignore` already treats as disposable.
- **Env-only, today's posture.** Rejected *for the app*: the 0.1.0 definition of done is a stranger who downloads a zip and pastes their own key in Settings; on Windows that launch path is a double-click, and a shell variable is not reachable from it. The variable is kept as the override — useful for CI, the nightly live tests and developers — not as the only door.
- **The OS keychain** (Windows Credential Manager, Secret Service, macOS Keychain). Rejected for 0.1.0: three platform integrations and their failure modes for a demo shipping on Windows and Linux only; it makes the key invisible to the player who wants to hand-edit or inspect it; and it can put an unlock prompt in front of a game's startup. Recorded as a **revisit trigger** rather than dismissed.
- **Move preferences to user level too, one home for both.** Rejected: ADR 0007 settled pack-level preferences for the project's portable, no-installer posture; sharing a home would make the key live in the same file an unrelated feature churns, and would move every existing player's mix.
- **A user-level `unmei/` directory holding per-pack settings files plus the credential file.** Rejected: more structure than three preferences and one secret need, and it reopens ADR 0007 rather than extending it.
- **A `credentials` file inside the pack directory with a `.gitignore` rule.** Rejected for the same reason as the sibling: ignore rules protect a working copy, not a zip.

## Consequences

- **The pack directory stays shareable**, and the key is the only player state that does not travel with it — moving a pack to another machine leaves the key behind, which is accepted: it is per-user by definition.
- **One more read path at launch** and one more file whose absence is completely normal; a missing credential file is Not set with no warning, never a failure.
- **The standing "keys are env-only" posture is superseded for the app.** `engine/src/decision/client.rs`'s `API_KEY_ENV` doc comment ("never a flag, never a file"), `app/src/main.rs`'s env-only comment and `GLOSSARY.md`'s vocabulary move with the build; `OPENROUTER_API_KEY` keeps its meaning as the override.
- **No engine change**: the credential file is `app/` presentation state, exactly like the settings file, and `engine/` learns no path, no format and no read rule. The one engine addition the plan asks for is the narrow `KeyCheck` seam beside the resolver ([ADR 0009](0009-decision-resolver-seam.md)'s pattern), not a path or a file.
- **Failure routing is inherited, not invented**: a **failed read** is a log-only silent degradation (nothing was lost, play is not blocked); a **failed write** is a Notice (the player's key was not persisted) through the existing funnel ([ADR 0008](0008-player-first-failure-posture.md)).
- **Plain text is part of the decision's cost, and its benefit**: no structured format means no parse failure, so no error type, message or fingerprint path can leak the value — at the price that a future non-key credential would need its own file or a format decision of its own.
- **Revisit triggers.** Reopen this record when **the CLI needs a stored key** (unification of the app's file with the authoring CLI's env-only contract becomes worth its own decision), or when **an OS-keychain requirement appears** (a second secret, a platform or store policy, or a threat model the plain file does not meet). Neither is scheduled.
