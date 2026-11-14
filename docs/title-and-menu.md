---
title: Title and menu — the start screen, Gallery and Timeline contracts
summary: The title-screen contracts — Decisions settings, the title's states and layout, the type system, the Gallery and the Timeline.
keywords:
  - decisions
  - credentials
  - presentation-surface
  - gallery
  - timeline
  - player-input
---
# Title and menu — build-ready spec

**Status (2026-10-14).** This document is where the beta start menu and the player's key map were settled, written as a plan and now built — every section's build has shipped; what the sections explicitly put out of scope stays out of scope. The **Decisions settings** and the **Title** shipped first, then the **Title layout**'s backdrop and tagline, the **Type system**, the **Gallery** and the **Timeline**. Sections are appended as their questions resolve; amendments remain dated and in place. Bevy claims are checked against the vendored 0.19.1 sources.

Sections so far:

- **Decisions settings** — settled 2026-10-01.
- **Title** — settled 2026-10-01.
- **Title layout** — settled 2026-10-02.
- **Type system** — settled 2026-10-04.
- **Timeline** — settled 2026-10-08 (behaviour), prototype-validated: roomier layout A.
- **Gallery** — settled 2026-10-05, prototype-validated; layout B, the framed-card grid.
- **Default look** — settled 2026-11-14, prototype-validated: variant E, the play frame, title and Esc menu for the default install. Not built.

The Settings view these sections extend — its shape, its pause doctrine and its focus conventions — is [Menu overlay](runtime-hardening.md#menu-overlay)'s and is inherited, not re-decided here.

**Input qualification (2026-10-11).** All underlying title/Settings/Gallery/still/Timeline/confirmation input rules and their validation cases below apply only when [Monitor → Interaction](runtime-hardening.md#interaction) does not own or claim that input. While Monitor is open, opening/closing, or quarantining an unreleased control, they cannot also navigate, activate, type or scroll. Esc closes Monitor only, preserving the underlying pause and surface. Monitor's F3 is reachable over all of them; this is a developer-only viewer, not a new player legend/Settings row or change to their pause/session contracts.

---

## Decisions settings

Settled 2026-10-01. Design record: [ADR 0010](adr/0010-credentials-live-in-a-user-level-file.md). **Built.**

This section covers the **Decisions group** in the Settings view and the **credential file** behind it: where the player's OpenRouter key is stored, how it is entered, validated, overridden and cleared, and what the player is told. The Settings view itself ([Menu overlay](runtime-hardening.md#menu-overlay) §5–6), the audio and play groups ([Audio settings](runtime-hardening.md#audio-settings), [Player comfort](runtime-hardening.md#player-comfort)) and the failure posture ([Runtime failure posture](runtime-hardening.md#runtime-failure-posture)) are inherited. The title screen's layout and the Continue/New Game semantics are the title sections'. The `unmei` CLI keeps its env-only key contract for 0.1.0 ([ADR 0010](adr/0010-credentials-live-in-a-user-level-file.md)).

### 1. File and format

- **The credential file** lives in a user-level directory: `%APPDATA%/unmei/credentials` on Windows, `~/.config/unmei/credentials` on Linux (`$XDG_CONFIG_HOME` is honoured when set). It is the one sidecar that is **not** a pack sibling, so a pack directory can be zipped and shared without carrying the key.
- **Plain text, the key only.** A read is the whole file, trimmed of surrounding ASCII whitespace; a trailing newline is fine; an empty or whitespace-only file is **Not set**. There is no schema, no parse step, no unknown-key tolerance question, and no error path that can quote the value. Chosen over RON deliberately: `ron`'s parse errors display the offending input, which could echo the key into `<pack-dir>.log`, and "the key is never logged" is a standing rule.
- **Path derivation is one shared helper**: a pure `credentials_path_in(config_dir: &Path) -> PathBuf` plus `default_config_dir() -> PathBuf` (Windows `%APPDATA%`; Unix `$XDG_CONFIG_HOME`, else `$HOME/.config`). Tests call the pure function with a `tempfile::TempDir`; **no test ever reads or writes the real `%APPDATA%`/`~/.config`**, and nothing mutates process-global environment (`std::env::set_var` is racy across the harness's parallel tests). The `credentials` file name is the helper's, not a caller's.
- **Atomic write**: a temp sibling then a rename, exactly as `app/src/settings.rs` and `write_save` do — `credentials.tmp` beside the target. On Unix the temp file is created **`0600`** (`OpenOptionsExt::mode`) with `create_new(true)`, so the key is private from its first byte and there is no chmod-after window; a stale `credentials.tmp` left by a crashed write is removed best-effort first, so a leftover can never block a later write (or smuggle a pre-created file's mode); a failed rename removes the temp best-effort. The `unmei/` directory is created **`0700`** (Unix) on the first write that needs it.
- **Windows**: no extra call. `%APPDATA%` is already ACL'd to the account, and the temp-sibling + rename supplies the atomicity. No `windows-sys` ACL is attempted.
- **Lifecycle**: no file until the first **successful** write. A load never writes; opening the Settings view never writes; Clear deletes the file and leaves the directory. Not set means no file, or an empty one, treated identically.
- **Read moments**: at launch (to build the resolver), on **every Settings-view open** (to pick up a hand-edit), and after every in-app write. A hand-edit is therefore picked up the next time the view is opened — no watcher, no timer, no live reload while the view is open. Reading never rewrites the file.

### 2. Precedence and application

- `OPENROUTER_API_KEY`, when it is **non-empty after trimming**, wins over the stored key. An empty or whitespace-only variable counts as unset.
- The stored file is still read at launch when the variable is set (cheap, and it keeps the read moments uniform) but is never used to build a resolver, and the row is read-only. A stored key stays on disk untouched.
- **Launch resolution order**: a non-empty variable → a resolver built from it; else a stored value that passes §3's shape check → a resolver built from it; else `None` — the offline posture, every decision point taking its authored fallback. A stored value that fails the shape check logs once and builds no resolver, so a known-bad key cannot produce a 401 per decision.
- **A key entered in the app applies in the same session — no relaunch.** The resolver is rebuilt when the row's verdict reaches **Set** (Accepted, or Unreachable → `not checked`); it is **not** rebuilt on Invalid. This is the one use of ADR 0009's resolver seam outside the play path.
- **Clear** returns the resolver to `None` and the row to Not set.

### 3. Validation

- **Shape check — local, free, no network.** After trimming, a key must be non-empty, printable ASCII, contain no internal whitespace or control characters, and be at least 16 characters. It rejects obvious non-secrets (prose, a pasted path carrying a newline) with a precise message without a round-trip; any OpenRouter key of any current or future shape passes it, because **the ping is the only authority on whether a key works**.
- **Ping.** On commit, **after the file is written**, `GET https://openrouter.ai/api/v1/key` with `Authorization: Bearer <key>` and a **5-second** timeout. The URL is a module constant beside the decisions endpoint and is deliberately **independent of `UNMEI_DECISIONS_URL`**: that override exists because the alpha decisions prefix can move, while `/api/v1/key` is the stable `/api/v1` surface on the same origin ([OpenRouter limits](https://openrouter.ai/docs/api_reference/limits)). There is **no standing ping** at launch or on view-open.
- **Three outcomes.** `200` → **Accepted**; `401`/`403` → **Rejected**; anything else — transport error, timeout, any other status — → **Unreachable**. A network outage is never called a bad key.
- **A rejected key stays on disk.** The row shows Invalid for the session and the resolver is **not** rebuilt: play keeps whatever resolver it had (the offline posture if there was none), so nothing spams a 401 per decision. The verdict is live, not persisted — a relaunch reads the (shape-valid) file and shows Set, and play degrades log-only, exactly as the play-time rule says.
- **Testability is a seam, not a mock network.** Next to the resolver: `KeyCheck`, one async method returning `Accepted`/`Rejected`/`Unreachable`; a pure `verdict_from_status(status) -> KeyVerdict` (engine-internal — `KeyVerdict` is the exported type, the mapping is not); a real `reqwest` implementation behind the existing `network` feature; and `Option<Arc<dyn KeyCheck>>` in the app (`None` in offline tests). This mirrors [ADR 0009](adr/0009-decision-resolver-seam.md) and is what makes `Checking…` → Set/Invalid and the resolver rebuild drivable in-process.
- **Never logged.** No log line, Notice, error screen or fingerprint rendered anywhere contains the key. The only rendering is the fingerprint of §4.1.

### 4. The Decisions group

- **Heading `Decisions`**, the **last** group in the Settings view, after Audio and Play, in both Settings surfaces (opened from the title and from the Esc overlay). Its position makes it the last stop of the `↑`/`↓` walk.
- **Controls, in walk order**: the **key row** (state line + field), **`Copy path`**, **`Clear key`**.
- **No key state gates play.** Continue and New Game stay enabled with no key, a malformed key or a rejected key; the only consequence is the authored fallback plus a log line (silent degradation, per the failure posture).
- Opening the group writes nothing. Only a commit, a Clear, or a successful view-open hand-edit read changes anything.

#### 4.1 The key row's states

| State | State line | Field |
| --- | --- | --- |
| Not set | `Not set` | empty, editable, placeholder `Paste or type an OpenRouter key` |
| Set | `Set — sk-or-…a1b2` | empty, editable |
| Checking… | `Checking…` | input disabled while the ping runs |
| Set, ping unreachable | `Set — sk-or-…a1b2 · not checked (see the log)` | empty, editable |
| Invalid (shape failure) | `Invalid — does not look like a key` | the typed text is kept, so the player can correct it |
| Invalid (rejected) | `Invalid — rejected by OpenRouter` | empty, editable |
| From environment | `From environment — sk-or-…a1b2` + the env note | not editable; `Clear key` disabled |
| Not set, unreadable file | `Not set` (the reason is in the log) | empty, editable |

- The **fingerprint** is the first ≤6 characters of the key, then `…`, then its last 4 — never more of the key than that.
- The **env note** reads `OPENROUTER_API_KEY overrides the stored key. Unset it and relaunch to use the stored key.`
- The **path line** reads `Stored in <path>` and is followed by a `Copy path` control, which writes that path to the clipboard through the existing `bevy_clipboard` resource (`bevy/system_clipboard` is enabled, so it is the real OS clipboard). Activation changes the control's label to `Copied` for 1.5 s; there is no new surface and no Notice — `Copied` is a label, not a message.
- While the env key is present the stored key is never used and never rewritten; the row is read-only and `Clear key` is disabled.

#### 4.2 Field mechanics

- **The field never holds the stored key.** It starts empty whenever it is rendered; committing **replaces** the stored key; the raw key is never rendered after commit. This sidesteps the widget's missing masking (`EditableText` is headless — no masking, no placeholder) and leaves "replace" as the only operation.
- **Focus model.** The row is a walk stop. `↑`/`↓` move the walk only while the row is **not** being edited, so the focused widget's own arrow handling cannot trap the walk. `Enter` or a click enters editing; while editing, `Enter` commits and returns to the walk, `Esc` leaves the field and discards, and clicking elsewhere also discards. **Monitor exception** *(amended 2026-10-11)*: opening Monitor suspends rather than discards the draft/caret/selection; its Esc/clicks/paste cannot commit, discard or type behind it. Close restores a still-editable field after activation release, otherwise clears the invalidated draft and uses safe current focus, without credential reads/writes/checks ([Interaction](runtime-hardening.md#interaction)). `Esc`'s existing "one key, four moments" gains exactly one step, and only while the field is being edited: the first `Esc` leaves the field, the next backs out of Settings.
- **Paste.** A pasted value is trimmed of surrounding ASCII whitespace before it enters the field, and the field's filter rejects newlines, so a paste carrying a trailing newline is not dropped whole by the widget's all-or-nothing filter.
- **A commit runs in this order**: shape check → on failure the row shows `Invalid — does not look like a key`, nothing is written, and no ping runs → otherwise write the file atomically → show `Checking…` → ping → resolve the verdict:
  - **Accepted** → `Set`, resolver rebuilt;
  - **Rejected** → `Invalid — rejected by OpenRouter`, file kept, resolver untouched;
  - **Unreachable** → `Set — … · not checked`, resolver rebuilt.
- **Clear** uses the existing one-confirm pattern — title `Clear key?`, detail “The stored key is deleted from <path>. You can paste it again at any time.”, buttons `Yes` / `Cancel` — then deletes the file, returns the row to Not set and the resolver to `None`.
- **A failed write** is not a row state: the key was not stored, so the row is left as it was, no ping runs, and the failure goes through the existing funnel — `error!` with the path and the IO error, and the Notice `Could not save the key`.

### 5. Failure posture

- **Credential read failures are silent degradations.** Missing → Not set, no log line. Unreadable (IO or permissions) → Not set plus `warn!`, bytes untouched. Content failing the shape check → the row shows Invalid plus `warn!`, bytes untouched, never auto-deleted. No **Notice**: nothing was lost and play is not blocked, so the Notice's single meaning does not apply.
- **A config directory that cannot be derived** (no `%APPDATA%`, no `$HOME`, no `$XDG_CONFIG_HOME`) behaves as an unreadable file on read — Not set plus `warn!` naming the path it could not resolve — and as a failed write on write.
- **A failed write is the one Notice**: the player's key was not persisted. It joins the failure inventory as a new `Continue-with-message` row; the hardening spec's table is amended when the build lands, not here.
- **Nothing in this section ever stops play**, and the startup error screen is not involved.

### 6. Validation rules (the build's test list)

- `credentials_path_in(dir)` is `dir/credentials` — the file name is added by the helper, never by a caller.
- `default_config_dir()` derives Windows `%APPDATA%\unmei` and Unix `$XDG_CONFIG_HOME/unmei` / `$HOME/.config/unmei` from **given** environment values, with no test mutating the process environment.
- A pack with no credential file: Not set, no warning, and no file created by loading, by opening the Settings view, or by opening the group.
- A successful write creates the file; the temp sibling is written and renamed, `credentials.tmp` is gone afterwards, and on Unix the file mode is `0600` and the `unmei/` directory is `0700`.
- A write that fails (read-only directory, IO error) leaves no partial credential file, removes the temp best-effort, leaves the row as it was, runs no ping, logs `error!` and notices `Could not save the key`.
- Read forms: a trailing newline and surrounding whitespace are trimmed; a whitespace-only file is Not set; a value with an internal newline or space fails the shape check and shows Invalid.
- An unreadable file (permissions or IO) → Not set, `warn!`, bytes byte-identical afterwards.
- Shape check table: `""` and whitespace → reject; `sk-or-v1-` + 60 chars → accept; a value with a space or a newline → reject; a 15-character value → reject; a non-ASCII value → reject; a 400-character printable value → accept (the ping decides).
- `verdict_from_status`: `200` → Accepted; `401`/`403` → Rejected; `429`/`500`/`0` (transport) → Unreachable.
- The write happens **before** the ping: a ping that never answers still leaves the key on disk.
- A rejected ping keeps the file, shows `Invalid — rejected by OpenRouter`, and leaves the resolver exactly as it was (a fake resolver proves no swap).
- An accepted ping, and an unreachable ping, each rebuild the resolver from the new key; a relaunch with the stored key builds one at startup.
- A shape failure writes nothing and never pings (a fake `KeyCheck` proves zero calls).
- Clear confirms once, deletes the file, returns the row to Not set and the resolver to `None`; cancelling changes nothing; Clear is disabled while the env variable is present.
- Precedence: a non-empty `OPENROUTER_API_KEY` wins, the row is read-only with the fingerprint, the stored bytes are untouched, and no resolver is built from the file; an empty or whitespace-only variable is treated as unset and the stored key is used.
- Launch resolution order, asserted with a fake `DecisionResolver` and a temp directory: env → stored (shape-valid) → `None`, with a shape-invalid stored value logging once and building `None`.
- In-session apply: commit a key, get `Set`, then run a decision — the fake resolver is the one built from that key, with no relaunch.
- Hand-edit pickup: write a different value into the file behind the app, then open the Settings view — the row updates and the resolver is rebuilt.
- Focus: `↑`/`↓` walk the Settings controls while the row is not editing; `Enter` enters editing; `↑`/`↓` do not walk while editing; `Enter` commits and returns to the walk; the first `Esc` leaves the field and discards (file and row unchanged), the second backs out of Settings.
- One widget-chain test drives `KeyboardInput` (and a paste through the in-process clipboard) through `MinimalPlugins + InputPlugin + InputFocusPlugin + InputDispatchPlugin + ClipboardPlugin + EditableTextInputPlugin` and asserts the field's value — the recipe verified against the vendored Bevy 0.19.1 sources. No test touches the developer's OS clipboard.
- A pasted `…\n` reaches the field as the trimmed key, not as nothing (the all-or-nothing filter's known trap).
- `Copy path` writes the path string to the clipboard resource and its label returns from `Copied` to `Copy path`.
- No key ever appears in a log line: drive a shape failure with a distinctive value and assert the session log contains neither it nor its fingerprint beyond §4.1's four characters.
- The group is the last stop of the walk in both Settings surfaces, and Continue/New Game are enabled in every key state (Not set, Set, Invalid, From environment).

### Amendments

Amendments to this section are dated and made **in place**, with a note here saying what changed; later sections are appended to this document and never rewrite this one. The vocabulary this section introduces — **Credential file**, **Decisions group**, **Key row** — is in [`GLOSSARY.md`](../GLOSSARY.md#settings).

- **2026-10-14**: the settled banner above now reads **Built** — the group and the credential file shipped. No rule changed.

### Out of scope for this section

The Settings view's shape, pause doctrine, card/slider visuals and focus conventions ([Menu overlay](runtime-hardening.md#menu-overlay) — inherited); the Audio and Play groups and their Resets; the title screen's layout, art and the Continue/New Game/exit-to-menu semantics (the title sections); the CLI's key contract (env-only for 0.1.0, [ADR 0010](adr/0010-credentials-live-in-a-user-level-file.md)); OS keychain storage; per-pack or per-profile key scoping; live file watching while the view is open; a masked or reveal-toggle field render; key rotation or multiple keys; and any change to `engine/`'s decisions wire format. The failure inventory itself lives in [Runtime failure posture](runtime-hardening.md#runtime-failure-posture) — this section adds one row to it at build time.

---

## Title

Settled 2026-10-01. **Built.** The title's layout, art and backdrop are **§ Title layout**'s.

This section covers the **title screen**: what is true while it is up, what each of its five buttons does, how it reaches Settings and how Settings returns, what the player is told, and what a New Game resets. The pause doctrine it freezes the play path with is [Menu overlay](runtime-hardening.md#menu-overlay) §6's, inherited rather than re-decided; the Settings view, its groups and its walk are [Menu overlay](runtime-hardening.md#menu-overlay) §5 and [Decisions settings](#decisions-settings)'s.

### 1. The boot gate

- **The title is a gated boot state, overlay-style** — not a Bevy `States` migration. It is a third `MenuView` (`Title`) on the existing `MenuState`, so the Esc overlay's freeze predicate (`menu_closed`) freezes the play path while the title is up, with no second gate and no second resource. `Esc` on the title is a no-op branch of the existing Esc chain: the title is not a door to the overlay, and Esc is not a way off the title.
- **The pack loads first.** Pack load, the settings read and the credential read are unchanged; a pack that will not load reaches the [error screen](runtime-hardening.md#4-the-startup-error-screen) and the title never appears. **The first frame after a successful load is the title** — there is no live-play frame and no loading surface.
- **The runner is parked, not started.** The boot `run()` has already carried the entry scene to its first wait (say, choice, decision or end of script) and applied its leading commands, so the staging behind the title is the entry scene's opening frame. The title never advances the runner, and entering and leaving Settings changes nothing about its position.
- **The freeze is literal** — exactly [Menu overlay](runtime-hardening.md#menu-overlay) §6's set: no advance or choice input, no `drive_script`, no text-reveal progress, no auto dwell, no Notice countdown, no `F5`/`F9`, no skip step *(added 2026-10-08)*, and a runner position that stays byte-identical while the title is up.
- **The title is silent.** The entry scene's BGM commands are staged but do not sound; music starts when play is entered — from the top, since the audio channel carries no resume position — stops on `Exit to title`, and starts again when Continue resumes. This is the one rule where the title is not simply a freeze of the play path.
- **The title is static.** No entrance, hover, press or exit animation, no fade and no ambient motion: every change on the title is an instant cut, as on every menu surface today. Motion was consciously rejected for the title; if a motion language is ever defined it is one pass across the title, the overlay and Settings together, in the real-art visual pass — never a title-only animation. This is a visual rule, standing beside — not instead of — "the title is silent".
- **The play hotkeys are inert.** `A`, `F11`, `Tab`, `Ctrl` and the dev save/load keys are ignored while the title is up, **but dev-enabled F3 can toggle Monitor without leaving the title** ([Interaction](runtime-hardening.md#interaction); amended 2026-10-11), as they are under the Esc overlay; they resume on entering play. *(Amended 2026-10-08: the skip keys join the list.)*
- **The title is not the overlay.** No Resume, no slot grid, no Save/Load toggle, no Esc toggle; the overlay's own rules ([Menu overlay](runtime-hardening.md#menu-overlay) §6) are otherwise unchanged.

### 2. The title's states and Continue

- **Five buttons, in this order**: `Continue`, `New Game`, `Settings`, `Gallery` and `Quit`. `Gallery` opens the gallery ([Gallery](#gallery)) *(amended 2026-10-05)*; it is no longer a disabled row with a caption. No key state gates play: a missing, malformed or rejected credential leaves Continue and New Game enabled ([Decisions settings](#decisions-settings) §4's rule).
- **A live session is play entered this launch** — set the moment play is entered (Continue or New Game), never persisted, cleared only by app exit. The boot-time parked runner is *not* a live session, so the title has exactly two states: a live session exists, or it does not.
- **Continue's precedence**: a live session → resume it exactly where it is; otherwise → load the **autosave**, the only slot the game writes itself.
- **The live path is the Esc resume, one level up.** The frozen runner, the reveal clock, the auto dwell and the Notice resume from where they stopped; an in-flight decision keeps running while the title is up, and an answer that arrives is held and lands on resume — a held *model* answer writes the autosave exactly once, a held fallback writes none, as under the overlay.
- **The autosave path is the existing load path.** A parked decision-wait re-fires the `decide` line; a refused load raises the existing load Notice; a load writes no autosave; entering play sets the live-session flag.
- **One line beside Continue describes what Continue will do**, in three states: a live session → the runner's current scene id alone (`reunion`), because a live session has no `saved_at`; otherwise a loadable autosave → the scene id and the app-formatted local time from `SlotInfo::Ok` (`reunion · 12 Oct 2026 14:03`, `—` when `saved_at` is `None`) — the same fields and formatting as the overlay's card; otherwise → the reason line below.
- **Continue is disabled, never enabled-then-refusing, when the autosave is unusable.** No file → `No autosave yet`. An autosave classified `Unreadable` → the engine's own `SlotInfo::Unreadable { reason }` string, verbatim, never paraphrased — one source of truth with the overlay's card. The reason line's placement is § Title layout's.
- **Slot metadata is read once when the title is entered** (boot, and again on every exit to title), matching the overlay's per-open read. A slot that changes between that read and the press is the load path's problem: it refuses with the existing Notice, and the title refreshes its metadata — which can flip Continue to disabled.
- **Focus** starts on Continue; when Continue is disabled it starts on New Game. Disabled controls stay in the `↑`/`↓` walk, so their reason and caption stay readable, and activating one does nothing. Focus is not remembered across entries.

### 3. New Game

- **New Game starts play immediately when there is no session to lose** (the boot state). With a **live session** it confirms once, and nothing else in this section asks a second time.
- **What it resets**: the runner to the fresh-boot parked state (the entry scene, `pc 0`, the pack's initial state map, the entry scene's commands re-applied from the top), the staging state, an in-flight decision request (cancelled), a held answer (discarded), a defensive hold, the auto dwell, the reveal, `PendingAutosave` (disarmed — a write armed by the discarded session must never capture the new one) and the Notice. Audio, visuals and the reveal are forced to replay, exactly as a load forces them.
- **What it leaves untouched**: the three manual slots, the **autosave** (overwritten at the next scene crossing, choice resolve or model-answered decision resolve after New Game — never by the click), the settings file, the credential file, and the read log (its own section owns that rule; New Game deliberately does not reset what the player has read).
- **It writes no autosave and reads no slot.** The reset is load-like, not a load.
- **Accepted consequence, recorded**: after New Game followed by Quit, a relaunch has no live session, so Continue loads the autosave — the previous playthrough, if the new one never crossed a scene. New Game deletes nothing, and writing the autosave at the click would clobber the checkpoint instead.
- New Game enters play, so the live-session flag is set and the title closes.

### 4. Settings from the title

- The `Settings` button opens **the same Settings view the Esc overlay shows** — one view, two doors: audio, play and the Decisions group, the same groups, the same walk, the same Esc back-out. The door is recorded when the view is opened (`origin: Slots | Title`), so:
  - the back control reads `← Back to title` on the title's door and stays `← Back to slots` on the overlay's;
  - `Esc` backs out of Settings to the title, then does nothing on the title;
  - the title's Settings has no Resume control.
- **`Exit to title` appears in the Settings left rail only when the door is the overlay** — there is no live session to leave when the door is the title. It joins Resume as a session-scoped rail control, so the Decisions group keeps its settled position as the last group and the last stop of the walk.
- Leaving Settings still flushes the settings file (the existing dirty-state rule); the title itself writes nothing.

### 5. Exit to title

- **It is the Esc pause, one level up.** The overlay closes, the title is shown, and the live session — position, reveal, dwell, Notice, held answer, in-flight request — is kept exactly as the Esc overlay keeps it. Nothing is written; the autosave is untouched.
- **It confirms once**: title `Exit to the title?`, detail “Your session is kept — Continue resumes it where you left off.”, buttons `Exit` / `Cancel`. It does **not** reuse the Load card's wording: no progress is lost and no decision is cancelled, so that sentence would be false in all three of its clauses. That sentence belongs to `Quit`.
- **Continue from the title resumes the same live session** — no slot read, no request re-fired, no answer dropped. A held answer is applied on resume exactly once.

### 6. Copy

The strings this section fixes. Placement, type sizes and the backdrop are § Title layout's.

| Where | String |
| --- | --- |
| Rail control (Settings, overlay door) | `Exit to title` |
| Exit confirm | `Exit to the title?` · “Your session is kept — Continue resumes it where you left off.” · `Exit` / `Cancel` |
| New Game confirm | `Start a new game?` · “The session in progress is discarded. Your saved slots and the autosave are untouched.” · `Start` / `Cancel` |
| Quit confirm | `Quit?` · “Unsaved progress is lost and any decision in flight is cancelled.” · `Quit` / `Cancel` |
| Continue's reason, no autosave | `No autosave yet` |
| Continue's reason, unreadable autosave | the engine's `SlotInfo::Unreadable { reason }` string, verbatim |
| Gallery | label `Gallery`; no caption — the row opens the gallery *(amended 2026-10-05)* |
| Settings back control | `← Back to title` (title door) / `← Back to slots` (overlay door) |
| Title legend | `↑/↓ move · Enter select · click to choose` |

The in-play help line is unchanged by this section. The `Quit` sentence is the Load card's; this section adopts it as the app's one destructive-confirm sentence, and the sentence `Exit to title` must never use.

### 7. Validation rules (the build's test list)

- The first frame after a successful pack load shows the title: `Runtime` exists, the runner is parked at the entry scene's first wait, and `drive_script`, advance input, `A`, `F11`, `F5`, `F9` and `Esc` all leave the position byte-identical.
- Boot's leading commands are applied — the staging behind the title is the entry scene's opening frame.
- No BGM sounds while the title is up; entering play starts it from the top, `Exit to title` stops it, and Continue starts it again.
- With `text_speed > 0`, no reveal progresses while the title is up, and the auto dwell does not accumulate.
- A pack that will not load reaches the error screen and never shows the title.
- Continue is disabled with `No autosave yet` on a save-free pack; an unreadable autosave disables it with the engine's reason verbatim, identical to the string the overlay's card shows.
- Continue with a loadable autosave loads it (position matches the slot), writes nothing, and sets the live-session flag.
- The Continue line's three states: live session → scene id alone; autosave → scene id plus the formatted local time (`—` for `saved_at: None`); unusable → the reason.
- Continue's autosave path onto a decision-wait slot re-fires the `decide` line; a slot that turns unreadable between the metadata read and the press refuses with the existing Notice and refreshes the metadata, flipping Continue to disabled.
- With a live session, Continue resumes it: position, reveal clock and any held answer are unchanged by the time spent on the title, and no slot is read.
- An answer arriving while the title is up is held; resuming applies it and writes the autosave exactly once (model) or not at all (fallback).
- Metadata is read once per entry: a slot written behind the app after the title is up does not change the line until the title is re-entered.
- Opening, closing or continuing with nothing outstanding writes no autosave.
- The Settings door opens the Settings view with `origin = Title`; back-out returns to the title; the overlay's door returns to the slots grid; the back control's label follows the door.
- The title's Settings rail has no Resume; `Exit to title` renders from the overlay door and is absent from the title door.
- `Esc` in the title's Settings returns to the title; `Esc` on the title does nothing, and never opens the overlay.
- Leaving Settings flushes a dirty settings file; opening the title's Settings creates no file.
- The Decisions group is the last walk stop from both doors, and Continue and New Game stay enabled in every key state.
- With no live session, New Game starts play with no confirm; with one, it confirms once, and cancelling changes nothing.
- New Game's reset: the runner is back at the entry scene with `pc 0`, staging re-applied, pending/held/hold/auto/reveal cleared, `PendingAutosave` disarmed, the Notice cleared, and audio/visual replay forced.
- An in-flight request is cancelled and a held answer discarded by New Game, so neither lands in the new session.
- New Game writes no slot and no settings change; the first crossing after it writes the autosave.
- The old autosave survives New Game + Quit + relaunch, and that relaunch's Continue loads it.
- Manual slots and the credential file are byte-identical after a New Game.
- `Exit to title` confirms once; cancelling changes nothing; confirming closes the overlay, shows the title and leaves position, reveal, dwell, Notice and any held answer byte-identical, writing nothing.
- `Exit to title` followed by Continue resumes the live session — no second `decide` call, no dropped answer, no slot read.
- `Quit` confirms once with a live session and not at boot; its detail is the Load card's sentence verbatim; cancelling changes nothing; confirming exits, and quitting with a held answer writes no autosave.
- Focus starts on Continue (New Game when Continue is disabled) at boot and on every re-entry; disabled controls stay focusable; activating one is a no-op; digits and `Tab` do nothing on the title.
- The legend, the three confirms, both reason lines and the door-aware back label render exactly §6's strings *(amended 2026-10-05)*: the `Gallery` row carries no caption.
- Every pre-existing app test reaches play through the real New Game press and passes unchanged; the title group drives the frozen app and asserts that nothing on the play path moves across frames; production code carries no test-only title bypass.

### Residual: the live-session flag

The live session is this section's one piece of new state, and four rules branch on it — Continue's precedence, New Game's confirm, Quit's confirm, and whether `Exit to title` is offered. Each branch is a bug surface if the flag is stale or read at the wrong moment: Continue resurrecting a playthrough the player believes they replaced, a confirm that appears with nothing to lose (or is missing with something to lose), or a rail control that lingers on the title's door. The build should compute it once (`live_session(state) -> bool`), use that predicate in all four places, and assert each of them in §7; the flag stays launch-scoped, is never persisted, and a relaunch always starts from the autosave.

### Amendments

Amendments to this section are dated and made **in place**, with a note here saying what changed; later sections are appended to this document and never rewrite this one. The vocabulary this section introduces — **Title screen**, **Live session** — is in [`GLOSSARY.md`](../GLOSSARY.md#pause--menu).

- **2026-10-04**: §1 gains **The title is static** — the motion deferral is consciously rejected, and any future motion is the real-art pass's, across the title, the overlay and Settings.
- **2026-10-05**: the `Gallery` row is enabled and caption-free — §2, §6 and §7 are amended in place, and the Out-of-scope line below now points at [Gallery](#gallery) instead of the disabled button.
- **2026-10-14**: the settled banner above now reads **Built** — the section shipped. No rule changed.

### Out of scope for this section

The title's layout, art and backdrop — including what shows behind the title and whether a frozen in-play HUD is visible (**§ Title layout** in this document); the gallery itself ([Gallery](#gallery) is its section; this section ships the row that opens it); the read log ([Read log](runtime-hardening.md#read-log)) and the Timeline ([Timeline](#timeline)); *(Amended 2026-10-08: this supersedes the booked “read log and chapter timeline (their own section)” pointer; Timeline is the surface name, and “chapter” is retired.)* the in-play help line's wording; `engine/` and the `unmei` CLI (`ScriptRunner::new` and the existing load surface are enough — this section changes no format, no wire contract and no engine behaviour); window size/position memory; gamepad support and input rebinding; quit-to-desktop from the Esc overlay (there is none); and the CI check this plan pulled forward.

---

## Title layout

Settled 2026-10-02. Prototype-validated: the clickable stub lives on a throwaway prototype branch at `app/prototype/title.html` (out of `main`), three layouts over one title model. The **title card** (variant B) won; the rail-and-dossier (A) and the command bar (C) remain comparison-only. **Built.**

*(Amended 2026-11-14: §1, §2's art bullet, §4, §5, §7 and §8's layout tests are superseded by [Default look](#default-look) §3, which moves the title to a left column and draws a veil over pack art. §3's wordmark source, §6 and the title's behaviour stand.)*

This section covers only the title's **look**: its layout, its backdrop, its wordmark, where Continue's reason line and the legend sit, and where the Settings view and the Gallery row live *(amended 2026-10-05)*: the Gallery caption is retired with the row's disabled state. What the title *is* — the boot gate, the five buttons' semantics, Continue's precedence, focus, the confirms — is [Title](#title)'s and is inherited, not re-decided. Where a look rule would need a behaviour rule, this section flags [Title](#title) rather than inventing one. The panel, focus-ring and tone vocabulary is [Menu overlay](runtime-hardening.md#menu-overlay)'s.

### 1. Shape

```
┌───────────────────────────────────────────────────────────────┐
│                                                               │
│                          unmei demo                           │
│                          a demo pack                          │
│                                                               │
│                ┌─────────────────────────────┐                │
│                │  ▶ Continue                 │                │
│                │  ✦ New Game                 │                │
│                │  ⚙ Settings                 │                │
│                │  ▦ Gallery                  │                │
│                │  ────────────────────────── │                │
│                │  ⏻ Quit                     │                │
│                └─────────────────────────────┘                │
│     reunion · 12 Oct 2026 14:03    ↑/↓ move · Enter select …  │
└───────────────────────────────────────────────────────────────┘
```

- **One centred column, and nothing else.** No left rail, no top bar, no slot grid, no frozen play frame. The column is about 520px wide at 1280×720 (`min(520px, 92%)`) and vertically centred.
- **The button panel reuses the overlay's vocabulary** — the same panel fill, hairline border, corner radius and per-row hover/focus treatment as [Menu overlay](runtime-hardening.md#menu-overlay) §1's cards — so the title reads as the same game without repeating the rail.
- **Five rows in [Title](#title) §2's order**: `Continue`, `New Game`, `Settings`, `Gallery`, `Quit`. A hairline rule separates `Quit` from the other four.
- **Focus is the overlay's ring** — the same `FOCUS` orange 2px border the card grid uses, never a fill change alone. Focus starts where [Title](#title) §2 says (Continue, or New Game when Continue is disabled), and disabled rows stay in the walk.
- **The foot row** spans the column: Continue's reason line left-aligned with the panel, the legend right-aligned opposite it.

| Part | Where | Content |
| --- | --- | --- |
| Wordmark | top of the column, centred | the pack's manifest `name` |
| Tagline | directly under the wordmark | optional, pack-supplied (§3) |
| Panel | the column | the five rows, in order |
| Reason line | foot row, left | [Title](#title) §2's three strings |
| Legend | foot row, right | [Title](#title) §6's fixed string |

### 2. Backdrop

- **A flat neutral gradient by default**: one radial from about `#1b2230` at the upper middle to `#0b0d12` at the edges and corners. No motif, no frame, no HUD; a pack's `title_backdrop` replaces it per the art bullet below, and without one it stands as-is.
- **Nothing of the play path is drawn behind the title — no frame, no dialogue box, no scene chip — in any of the four states**, at boot or on any re-entry after `Exit to title`. The runner's position is untouched ([Title](#title) §1: it stays parked at the entry scene's first wait, and entering and leaving Settings changes nothing); the title simply never draws it. This is the visible half of "the title is silent", and it is what makes it impossible for the title to show a say-line the player has not reached.
- **Why neutral, not the dimmed freeze**: the title's job is to read as *not a play frame*. A dimmed freeze of the entry scene reads as a paused game with a line on screen; the Esc overlay already owns that reading. The overlay's freeze doctrine is unchanged — the runner is frozen either way — it is only the drawing that differs.
- **Pack art replaces the gradient when the manifest's `title_backdrop` is set** *(amended 2026-10-04)*: a full-window cover draw — scaled to fill the window, cropping the overflow — **exactly as authored, with no veil, tint or overlay**, over the gradient underlay (invisible beneath opaque art, blending under alpha art). The gradient remains the default for packs without the field. Contrast of the wordmark, tagline and foot row over pack art is the **pack's responsibility**; this section makes no readability guarantee. The same backdrop stands behind Settings opened from the title. A declared-but-missing or undecodable file is a silent degradation — the gradient drawn, `warn!` once — and its failure-inventory row is [row 25](runtime-hardening.md#2-the-inventory), not restated here ([Decisions settings](#decisions-settings)'s precedent).
- **Build-time consequence**: no `bg`, `sprite` or HUD draw may run behind the title, and the staged frame's first draw is the frame that enters play.

### 3. Wordmark

- **The wordmark is the pack's manifest `name`** (`unmei demo` in the mock), the largest text on the surface, centred at the top of the column.
- **The wordmark's face is IBM Plex Sans Bold** *(added 2026-10-04)*: the project now ships its fonts, so the interface's default face no longer stands in. The faces, the weights and the app-wide sans/mono split are [Type system](#type-system)'s.
- **A tagline slot sits directly under it** — one small letterspaced uppercase line in the overlay's accent tone. It renders **only when a tagline exists**; with none, the wordmark sits alone and the spacing above the panel is unchanged, so there is never an empty gap.
- **The tagline's source is the manifest's `tagline`** *(amended 2026-10-04)*: natural-case copy, trimmed and single-line, with no `{var}` interpolation — the field's contract is [Manifest and state](content-format.md#6-manifest-and-state)'s. The title renders it **uppercased** in the slot, exactly the prototype's `.wordmark small` treatment; the demo pack's copy is `a demo pack`. [Title](#title) §6's copy table is unchanged by this section.
- **No state readout.** The mock's chip (`AUTOSAVE PRESENT`, `LIVE FROZEN SESSION`) was the prototype's own state display, not a design element: the title's state is [Title](#title) §2's Continue line and nothing else, so a chip would say it twice in save-system vocabulary the player never otherwise sees. With the chip gone, that line is the *only* state surface — see §4's note.

### 4. The reason line and the legend

- **Continue's reason line sits directly under the button panel**, left-aligned with it, in mono. Its strings and its three states are [Title](#title) §2's; this section only places them.
- **Tone follows which state it explains**: muted while Continue is enabled, the warm warn tone when the line explains a disabled Continue. The string is always the same single line — only the colour changes.
- **The legend is [Title](#title) §6's fixed string**, right-aligned in the same row, in the faint tone. It is a legend, not a Notice and not a per-row caption: it appears once, at the bottom of the column.
- **Flagged, not fixed here.** With the chip dropped, §2's live-session form ("the runner's current scene id alone", no `saved_at`) is the *entire* visible difference between resuming a live session and loading the autosave. That is §2's rule and stands; if it proves too quiet in play, the fix is a date-and-note amendment to [Title](#title) §2 — a new element on the title is not the fix, and this section does not add one.

### 5. Settings from the title

- The title's door opens the **same Settings view** as the overlay's ([Title](#title) §4, [Menu overlay](runtime-hardening.md#menu-overlay) §5): Audio, Play and the Decisions group, the same walk.
- **Placement**: the same centred panel at the column's width, top-anchored inside the frame and internally scrollable when the groups exceed it; the back control — `← Back to title`, [Title](#title) §4's door-aware label — sits at the bottom of the panel.
- **No rail, no Resume, no `Exit to title`** on this door ([Title](#title) §4). The wordmark and the five rows are gone while Settings is open; returning restores them with focus re-started per [Title](#title) §2.

### 6. Gallery's row

- `Gallery` renders as a normal, enabled row in the panel, in the same position and with the same treatment as the other four, and activating it opens the gallery ([Gallery](#gallery)) *(amended 2026-10-05)*: the disabled row and its `coming in a later build` caption are retired, and the panel's line-up closes over the freed line.
- The row takes the same `FOCUS` ring, the same pointer behaviour and the same walk stop as every other row; it is an ordinary row in every respect.

### 7. Reference proportions (1280×720)

The mock's numbers, as the build's starting point; exact pixels may be tuned within these proportions.

| Part | Size / treatment |
| --- | --- |
| Wordmark | ~46px IBM Plex Sans Bold, tight tracking; the manifest `name` |
| Tagline | ~11px mono, uppercase, ~0.34em tracking, accent tone |
| Column | `min(520px, 92%)`, centred; panel padding ~10px |
| Button rows | ~16px, ~12px×14px padding, ~2px row gap |
| Reason line / legend | ~12px mono; reason muted or warm-warn, legend faint |
| Focus | 2px `FOCUS` orange border (the overlay's ring) |
| Backdrop | radial `#1b2230` → `#0b0d12`, centred ~50%/28% |
| Settings panel | `min(720px, 100%)`, top-anchored ~52px, internal scroll |

### 8. Validation rules (the build's test list)

- At 1280×720 the wordmark, the five rows, the reason line and the legend fit without scrolling in all four title states — no autosave, loadable autosave, live session, unreadable autosave — including the longest `SlotInfo::Unreadable` reason.
- The first frame after a successful pack load draws the backdrop — the neutral gradient, or the pack's `title_backdrop` art when declared — and the title; **no** play frame, dialogue box or scene chip is drawn while the title is up.
- Re-entering the title after `Exit to title` draws the same backdrop, and no part of the frozen live session shows behind it.
- The wordmark renders the pack's manifest `name`. *(Testable today.)*
- The title's faces follow [Type system](#type-system): the wordmark is Plex Sans Bold; the five row labels are sans; the tagline, reason line and legend are mono (Fira Mono Medium) *(amended 2026-10-05)*: the Gallery caption is gone.
- A pack with neither `tagline` nor `title_backdrop` renders exactly today's title: the gradient, no tagline line, no spacing shift. *(Build.)*
- A pack declaring `tagline` renders it uppercased directly under the wordmark; a 48-character tagline fits at 1280×720 without scrolling or reflow. *(Build.)*
- A `tagline` over 48 characters loads with one warning and still renders — no error, no Notice. *(Build.)*
- An empty or whitespace-only `tagline` renders no line; a newline or control character in it is a `pack.ron` load error. *(Build.)*
- `title_backdrop: "<id>"` draws `assets/backgrounds/<id>.png` full-window, scaled to cover, **unmodified** — no veil, tint or overlay — and no part of the play frame is drawn with it. *(Build.)*
- A declared backdrop whose file is missing or undecodable logs one `warn!` and draws the gradient; the title still shows, no Notice, play is unaffected. *(Build.)*
- A malformed `title_backdrop` id is a `pack.ron` load error; an unknown manifest field stays a load error. *(Build.)*
- Settings opened from the title keeps the same backdrop. *(Build.)*
- The demo pack declares `tagline` and `title_backdrop` and loads with no errors or warnings. *(Build.)*
- The title's state is rendered nowhere but Continue's line: no chip, badge or second status line.
- Focus is the `FOCUS` orange ring, on Continue at boot, on New Game when Continue is disabled, and on every re-entry; disabled rows stay focusable and activating them is a no-op.
- The reason line renders [Title](#title) §2's three strings, muted when Continue is enabled and warm-warn when it explains a disabled Continue.
- The legend renders exactly [Title](#title) §6's string, once, right-aligned in the foot row.
- **`Gallery` opens the gallery** *(amended 2026-10-05)*: the row renders enabled with no caption, stays in the walk, and both keyboard and pointer activate it.
- Settings from the title renders the shared groups with `← Back to title` at the bottom, and no Resume or `Exit to title` control; `Esc` returns to the title.
- Keyboard and pointer produce the same focus and the same activation on every row, with the ring always visible.

### Amendments

Amendments to this section are dated and made **in place**, with a note here saying what changed; later sections are appended and never rewrite this one.

- **2026-10-04**: the backdrop and tagline deferrals are settled — the manifest carries `title_backdrop` and `tagline`; §2, §3 and §8 are amended in place, and the pack-format contract is [§6 of the content format](content-format.md#6-manifest-and-state). The wordmark's typeface, animation/transitions and sub-1280 layouts remain deferred.
- **2026-10-04**: the typeface, motion and sub-1280 deferrals are settled — §3 names the wordmark's face, §7's wordmark row follows it, §8 gains its face test, and the Out-of-scope lines are rewritten. The type system is [Type system](#type-system)'s; the minimum window size is [runtime-hardening § Fullscreen](runtime-hardening.md#5-fullscreen)'s.
- **2026-10-05**: the `Gallery` row is enabled and its caption retired — the intro, §1's mock, §6, §7's caption row and §8 are amended in place. The gallery itself is [Gallery](#gallery).
- **2026-10-14**: the settled banner above now reads **Built** — the look shipped. No look rule changed.
- **2026-11-14**: the default look's redesign supersedes §1, §2's art bullet (the no-veil rule), §4, §5, §7 and §8's layout tests; the pointer above the intro names them. The owner is [Default look](#default-look) §3.

### Out of scope for this section

The five buttons' semantics, focus order, confirms and copy, and the missing live-session note above ([Title](#title) — §4 of this section flags it there); the Settings groups' contents and walk ([Menu overlay](runtime-hardening.md#menu-overlay), [Decisions settings](#decisions-settings)); the gallery itself; any behaviour at a decision-wait or under the Esc overlay; the wordmark's typeface and every other face rule ([Type system](#type-system) — settled); animation, transitions and title audio ([Title](#title) §1's "the title is static" and "the title is silent" both stand; any motion belongs to the real-art pass, never a title-only animation); and window-size-specific layouts below 1280×720 (the minimum supported window size is 1280×720 — [runtime-hardening § Fullscreen](runtime-hardening.md#5-fullscreen)).

---

## Type system

Settled 2026-10-04 — the title-polish typeface deferral. **Built.**

This section covers the faces every surface renders with and the split between them: the title, the in-play dialogue band and HUD, the Esc overlay with its Settings view and the gallery, the confirm dialogs and the startup error screen. Placement and sizes are the owning section's; this section only says which face renders them. The Bevy claims are checked against the vendored 0.19.1 sources.

### 1. The faces

- **IBM Plex Sans** is the app's prose and control face, in three weights: **Regular**, **SemiBold** and **Bold**. Source: [IBM/plex](https://github.com/IBM/plex), `packages/plex-sans/fonts/complete/ttf/`, pinned at commit `763c36e`; SIL OFL 1.1, and the license text ships with the faces.
- **Fira Mono** is the annotation and machine-value face: **Medium** for every string the split sends to mono — today's embedded face is Fira Mono Medium, so the weight is unchanged — with **Regular** shipped as the available lighter face. Source: [Google Fonts `ofl/firamono`](https://github.com/google/fonts/tree/main/ofl/firamono); SIL OFL 1.1, and the license text ships with the faces.
- **Both families are embedded in the binary**, not loaded as loose files: the asset root is the pack's `assets/` directory (set in `app/src/main.rs`), so the app's own faces cannot live in that source. `app/assets/fonts/` holds the five TTFs, both license files and a provenance note (family, source, pinned revision).
- **Bevy's `system_font_discovery` feature is enabled** as the fallback: a codepoint the bundled faces lack resolves to an OS font. This is best-effort and platform-dependent — the demo's copy is fully covered by the bundle, while non-Latin scripts and the overlay's `▶` (U+25B6, the Resume row) depend on the machine's fonts. No test asserts a fallback glyph.
- **Bevy's embedded default font is not the app's look.** Every app string names a bundled face; the `default_font` feature stays on only as Bevy's own safety net.

### 2. The split

One rule: **prose and control labels take the sans; annotations and machine values take the mono.**

| Surface | Sans (Plex) | Mono (Fira Mono Medium) |
| --- | --- | --- |
| Title | the wordmark (Bold); the five row labels | the tagline; Continue's reason line; the legend |
| Gallery | the heading; the empty-state line | the card labels; the still caption; the grid and still legends |
| Dialogue band | the say-line prose; the choice option text | the speaker prefix (an inline span on the same line); the help line; the `AUTO` cue |
| Esc overlay | the pack name; the rail and row labels; the slot card's label and action label; confirm titles and bodies | the scene line; the slot card's kind, status, detail and saved-at |
| Settings | the panel title (SemiBold); the row labels | the group headers; every control value (steps, percentages, switch states); the key row's state line, the key field and the path line |
| Startup error screen | the intro line | the pack path; the error lines; the quit footer |
| Notices | the Notice text | — |

*(Amended 2026-11-14: the Title, Dialogue band and Esc overlay rows are replaced by [Default look](#default-look) §6's rows.)*

The error screen's mono half is an **inferred** application of the rule (the intro was settled as sans, the machine lines were not); it is flagged here so a reviewer can overturn it cheaply. The slot card's split was settled directly (2026-10-04).

### 3. Weights

- The **wordmark** is Plex Sans **Bold** — the ~46px bold § Title layout §7 asks for.
- The **sans headings** — the Settings panel title, the overlay's pack name, the confirm-dialog titles, the gallery heading — are **SemiBold**.
- Every other sans string is **Regular**; every mono string is Fira Mono **Medium**. Fira Mono Regular ships available, but no rule currently uses it.

### 4. Mixed strings

Two surfaces hold prose and machine text in one node; each renders as `TextSpan` children of the one text node, every span carrying its own `TextFont` and `TextColor` explicitly rather than inheriting:

- *(Superseded 2026-11-14 by [Default look](#default-look) §6: the name tab and the prose are separate nodes.)* **the dialogue say-line** — the root text is the mono speaker prefix (`Ace: `) and one span is the sans prose; the reveal updates the prose span;
- **the slot card** — sans spans for the label and the action label, mono spans for kind, status, detail and saved-at.

### 5. Validation rules (the build's test list)

- Every text entity the app builds names a bundled face; none falls back to Bevy's default font handle.
- The wordmark is Plex Sans Bold; the tagline, reason line and legend are Fira Mono Medium.
- The say-line's speaker prefix and prose are two spans in one text node with different faces, and the reveal updates the prose span.
- The slot card's label and action label are sans while kind, status, detail and saved-at are mono.
- The gallery's heading is sans SemiBold like the Settings panel title; its card labels, still caption and both legends are mono; its empty-state line is sans prose.
- The Settings panel title is SemiBold; its group headers and control values are mono; its row labels are sans.
- The error screen's intro is sans while its pack path, error lines and quit footer are mono.
- A headless UI world renders with the embedded faces without a pack asset root; the fonts load and the handles are distinct.
- No test asserts a system-fallback glyph.

### Amendments

Amendments to this section are dated and made **in place**, with a note here saying what changed.

- **2026-10-04**: the section is new — the typeface deferral settled with the two families, the split, the weights and the mixed-string spans.
- **2026-10-05**: the gallery joins §1's surface list, §2's table, §3's heading list and §5's tests, and the Title row's Gallery-caption references (§2, §4, §5) are retired with the row's caption ([Title layout](#title-layout) §6). The split and the weights are unchanged.
- **2026-10-14**: the settled banner above now reads **Built** — the faces and the split shipped. No face, split or weight rule changed.
- **2026-11-14**: §2's Title, Dialogue band and Esc overlay rows and §4's dialogue bullet are superseded by [Default look](#default-look) §6. The faces and weights are unchanged.

### Out of scope for this section

Font-size or legibility controls (a recorded [player-comfort non-goal](runtime-hardening.md#player-comfort)); a per-pack font surface; italics, small caps, or any weight beyond the three sans faces and the two mono faces; glyph-by-glyph fallback behaviour on any given OS; motion (the title is static, [Title](#title) §1); and shipping the license files with packaged binaries (the packaging trajectory's — that work inherits the OFL obligation to carry both license texts alongside the binary).

---

## Gallery

Behavior settled 2026-10-05. **Built.** `### Entries` (the declared-reference model) and `### Layout` (the grid's geometry, card art and scaling) sit below; the `### Behavior` subsection below fixes the surface's behavior only. The gallery is a fourth `MenuView` on the pause surface (`Title | Slots | Settings | Gallery`): the pause doctrine, the card vocabulary and the focus conventions it inherits are [Menu overlay](runtime-hardening.md#menu-overlay)'s and are not re-decided here, and the failure posture it sits under is [Runtime failure posture](runtime-hardening.md#runtime-failure-posture)'s and [ADR 0008](adr/0008-player-first-failure-posture.md)'s.

### Behavior

Settled 2026-10-05.

This subsection covers the gallery as a surface: how it is opened and left, what happens when a card is activated, and what it shows when there is nothing — or nothing loadable — to show. What it lists, and how entries are labelled and ordered, is `### Entries`'s; the grid's geometry, its card art and its scaling are `### Layout`'s.

#### 1. The two doors

- **The title's `Gallery` row opens it.** The row is an ordinary row in [Title](#title) §2's five — enabled, caption-free, with the same ring and the same activation as its neighbours ([Title layout](#title-layout) §6 records the amendment) — and opening it leaves the title's backdrop standing behind the gallery ([Title layout](#title-layout) §2's backdrop rule is unchanged).
- **The Esc overlay's top bar carries the second door**: a `Gallery` control immediately left of `Settings`, rendering only while the slot grid is up. In that view `Tab` cycles Save → Load → Timeline → Gallery → Settings → Save; while the Timeline, gallery or Settings is up the entry controls are absent and the cycle narrows to Save → Load, exactly as the Settings view already narrows it. *(Amended 2026-10-08: Timeline joins the Slots top bar immediately before Gallery; the gallery's doors are unchanged.)* The overlay shape it amends is [Menu overlay](runtime-hardening.md#menu-overlay) §1.
- **Neither view is a door to the other.** Settings carries no gallery control and the gallery carries no Settings control.
- **Opening is write-free.** Opening, walking and closing the gallery writes nothing and reads no slot: no settings flush, no `list_slots`, no save file, no autosave.

#### 2. Available at every parked wait

- The gallery opens at say-wait, choice-wait, decision-wait with the request in flight, with an answer already held, and at scene end — the reach [Menu overlay](runtime-hardening.md#menu-overlay) §6 gives the overlay, from both doors (the title's door exists only while the title is up).
- It cannot disturb the session: activating a card can at most show a still (§5), so opening changes neither the runner's position nor the in-flight request, and a completed answer stays held until resume. The freeze is the open pause surface's existing set — no advance or choice input, no `drive_script`, no reveal, no auto dwell, no Notice countdown, no `F5`/`F9`, the hotkeys inert.

#### 3. Back to origin

- The door is recorded when the gallery opens (`origin: Slots | Title`, the value [Title](#title) §4 already uses for Settings) and every way out returns to it: the back control is door-aware — `← Back to slots` from the overlay's door, `← Back to title` from the title's door — and it is a click/`Esc` affordance, not an arrow stop, exactly as Settings' is.
- `Esc` backs out one level per press: a still closes to the grid; the grid closes to the origin; from the overlay's grid the next press closes the overlay (Resume), and with the title as origin the chain ends there ([Title](#title) §1).

#### 4. The grid and its walk

- Cards are walked with `↑`/`↓`/`←`/`→` in the entry order the `### Entries` rules fix, and the walk does not wrap. Which arrow reaches which card in a multi-column grid — the row-versus-neighbour mapping for `↑`/`↓` — is `### Layout`'s to fix; this subsection fixes only the order, the no-wrap rule and the activation keys. Focus is always visible: when the grid scrolls, the focused card is scrolled into view. `Enter`, `Space` and a click activate the focused card; digit keys do nothing here (`1`–`3` stay the slot grid's shortcut).
- Focus starts on the first card at every open, and opening a still leaves the walk on that card. Focus is one value: the grid's focused card and the still's current entry are the same thing, so they cannot drift apart, and closing a stepped still returns to the entry it was showing (§5).

#### 5. The still view

- Activating a card opens the **still view**: that entry alone, contain-fit so the whole image is visible and a transparent-background sprite pose stays transparent, over the gallery's own backdrop (§6) *(amended 2026-10-05)*: `### Layout` §6 reserves a foot strip for the caption, so the image is contain-fit in the window above that strip rather than across the whole window. Its caption is the entry's label, verbatim the card's label.
- `←`/`→` browse to the previous and next entry in the entry order, wrapping at both ends; the caption and the grid's focus follow the step.
- `↑`, `↓`, `Esc` and a click each close back to the grid, focused on the entry the still was showing. The still has no other controls.
- The still is inert: it never calls the runner, never writes a slot, the autosave or the settings file, and never starts audio. It is a hold, not an action.

#### 6. Backdrop, empty pack and failed art

- **The backdrop is the door's.** From the title's door the gallery stands on the title's backdrop — the pack's `title_backdrop` when declared, the neutral gradient otherwise — exactly as Settings does ([Title layout](#title-layout) §2); from the overlay's door it stands on the overlay's dimmed, blurred freeze, like the slot grid. The still view draws over that same backdrop; stepping between stills changes nothing behind them.
- **An empty pack** — one that declares no entries — opens the gallery with the single line `No images in this pack`: no grid, no error, no Notice. Silent degradation, per [ADR 0008](adr/0008-player-first-failure-posture.md).
- **A card whose art is missing or undecodable** draws the placeholder frame and the entry's label, and the gallery still opens; activating it opens a still showing the same frame and label. The failure rides the existing [failure funnel](runtime-hardening.md#5-the-failure-funnel) as **rows 3/4** by path kind (`BackgroundAsset` / `SpriteAsset`): `warn!` once per path, no Notice, and a path the play path has already reported is not reported again. **The inventory does not move** — no new row, no change to `FailureRow::ALL` and no change to its table-driven test.
- **The placeholder** is what a card — and its still — draws when no decoded image is available: while the file is still loading and forever after a failed load. The label is drawn either way.

#### 7. Copy

The strings this subsection fixes. Placement, sizes and the card art are `### Layout`'s.

| Where | String |
| --- | --- |
| Title row | `Gallery` — no caption |
| Overlay control | `Gallery` |
| Heading | `Gallery` |
| Back control | `← Back to slots` (overlay door) · `← Back to title` (title door) |
| Grid legend | `↑/↓/←/→ move · Enter view · click to open` |
| Still caption | the entry's label, verbatim |
| Still legend | `←/→ browse · ↑/↓ or Esc or click to close` |
| Empty state | `No images in this pack` |

#### 8. Validation rules (the build's test list)

- The title's `Gallery` row is enabled and caption-free; activating it opens the gallery with `origin = Title`; the five-row order of [Title](#title) §2 is unchanged.
- The overlay's top bar renders `Gallery` immediately left of `Settings` only while the slot grid is up; `Tab` cycles Save → Load → Timeline → Gallery → Settings → Save there, and Save → Load in the Timeline, gallery and Settings views. *(Amended 2026-10-08: validation follows the new Slots cycle.)*
- The overlay's door opens the gallery with `origin = Slots`; the back control reads `← Back to slots` from it and `← Back to title` from the title's door.
- The gallery opens at every parked wait kind — say, choice, decision with the request in flight, a held answer, scene end — and leaves `runner.wait()`, `pc`, the in-flight request and a held answer unchanged.
- Opening, walking and closing the gallery writes nothing and reads no slot: no slot file, no autosave, no settings flush, no `list_slots` call.
- `Esc` unwinds exactly one level per press: still → grid → origin; from the overlay's grid the next press closes the overlay; with the title as origin the chain ends at the title, where `Esc` is inert.
- Settings offers no gallery control and the gallery offers no Settings control, from either door.
- Focus starts on the first card at every open; the walk follows the entry order and does not wrap; a focus moved past the visible rows scrolls the focused card into view; `Enter`, `Space` and a click activate the focused card; a digit does nothing.
- Activating a card opens the still for that entry, with the entry's label as its caption.
- The still's `←`/`→` step through the entry order and wrap at both ends; the caption follows the step; `↑`, `↓`, `Esc` and a click each close to the grid, focused on the entry the still was showing; no other input does anything.
- The still calls no runner method and writes no slot, autosave or settings file.
- A card whose declared file is missing or undecodable draws the placeholder frame and still opens a still; exactly one `warn!` logs per path under rows 3/4, no Notice appears, and a path already reported by the play path logs no second line.
- A pack that declares no entries shows the single line `No images in this pack`, with no grid, no error and no Notice.
- The backdrop behind the gallery is the door's — the pack's `title_backdrop` or the gradient from the title's door, the overlay's dimmed freeze from the overlay's door — and is unchanged by stepping or closing a still.
- The still's image is contain-fit: both axes fit the window above `### Layout` §6's caption strip, the aspect is preserved, and a transparent-background sprite stays transparent (the exact maths and its test are `### Layout`'s; this list asserts the behavior).
- §7's strings render exactly: the caption-free title row, the overlay control, the heading, both back labels, both legends, the still caption and the empty-state line.
- The heading follows [Type system](#type-system)'s split as the Settings panel title does (sans SemiBold); the card labels, the still caption and both legends are mono; the empty-state line is sans prose.

### Entries

Settled 2026-10-05. **Built.**

This subsection fixes what the gallery lists and how each entry is labelled and ordered. The surface that shows them — the two doors, the grid, the still and the states — is `### Behavior`'s above; the grid's geometry, its card art and its scaling are `### Layout`'s. Every entry stands for an image the pack itself declares, and the accepted limitation that the gallery shows nothing else is stated once, in [the content format](content-format.md#2-pack-layout), and is not restated here.

#### 1. What an entry is

- **An entry is one declared image reference.** Each distinct `bg <id>` line yields one background entry; each distinct `sprite <npc_id> <pose>` line yields one sprite entry; the manifest's optional `title_backdrop` yields one background entry under its own id (§3).
- **A sprite entry is the `(npc_id, pose)` pair, not the NPC.** `ace · neutral` and `ace · happy` are two entries, because they are two images; an NPC with three poses yields three.
- **Images only.** A `hide <npc_id>` line and every `play`/`stop` reference yield no entry. The demo pack references three audio ids (`music_morning`, `music_evening`, `sfx_paper_rustle`) and ships a fourth, unreferenced `.ogg`; none of them appears.
- **Entries are a set.** The same background id or `(npc_id, pose)` pair referenced in many scenes, or in both arms of an `if`, is one entry. The dedupe keys are the background id and the `(npc_id, pose)` pair, and the manifest's `title_backdrop` shares the background id space.
- **Scope.** Entries come from the loaded pack's declarations only — one pack, no cross-pack list. What that leaves out is the content format's accepted-limitation line ([§2](content-format.md#2-pack-layout)), cited in this subsection's intro and not restated here.

#### 2. Labels

- **The label is the entry's identity.** It is the card's label and, verbatim, the still view's caption (`### Behavior` §5, §7).
- **A background entry's label is its raw id** (`school_gate`); **a sprite entry's is `npc_id · pose`** — the two ids joined by a space, U+00B7 `·` and a space (`ace · neutral`). Ids are `[a-z0-9_]+` ([content format §6](content-format.md#6-manifest-and-state)), so no label can collide with another and the separator cannot occur inside an id.
- **No name is invented.** No display-name metadata exists, so nothing title-cases, prettifies or truncates an id.
- **The label belongs to the entry, not the surface**: the engine produces it with the entry (§4), and the grid and the still both render that one string.

#### 3. Order

- **One list: backgrounds first, then sprites, each group sorted alphabetically.** Backgrounds sort by id; sprites sort by `(npc_id, pose)`, NPC first and pose second.
- **The manifest's `title_backdrop` joins the background group** and sorts by its id like any scene background, with no marker and no second group. A backdrop id a scene's `bg` also uses is already one entry (§1), so it stays one card.
- **The order is total and deterministic, and is computed from the pack's own data** — never from iteration order. `Pack.scenes` is a `HashMap`, so the derivation sorts; the same pack yields the same order on every run and in every build.
- **This is the walk order.** `### Behavior` §4 walks the cards in it, §5 browses the stills in it, and the order does not change while the gallery is open.

#### 4. The derivation seam

- **The engine enumerates; the app resolves.** `Pack::gallery_entries()` returns the ordered entries, so one producer owns what an entry is, its label and its order. The app maps each entry's kind to its asset path and to the failure row `### Behavior` §6 already fixes — a background entry to `background_asset_path` (`backgrounds/<id>.png`) and `BackgroundAsset`, a sprite entry to the sprite path producer (`sprites/<npc_id>/<pose>.png`) and `SpriteAsset`.
- **The sprite path gains the producer the background path already has.** `background_asset_path` is the one producer of `bg`'s path rule, and `sprite_asset_path` the producer of `sprites/<npc_id>/<pose>.png` beside it in `app/src/presentation/assets.rs`; the gallery must not become a second inline copy, so the play path and the gallery both call it.
- **`GalleryEntry` carries the kind and its ids and yields its label, but no path** — the engine resolves no asset paths today, and the app's producers stay the single source of the path rules.
- **Nothing is scanned and nothing is cached.** The entries are derived from the loaded `Pack`; there is no `assets/` walk and no cross-pack cache.

#### 5. Validation rules (the build's test list)

- The demo pack's entry list is exactly `school_gate`, then `ace · happy`, then `ace · neutral` — backgrounds first, each group alphabetical, `happy` before `neutral`.
- `bg school_gate` declared in three scenes and as the manifest's `title_backdrop` yields one background entry.
- `sprite ace neutral` declared four times yields one sprite entry; `sprite ace happy` yields a second.
- The same background id in both arms of an `if` yields one entry, as does the same pose in both arms.
- A `hide <npc_id>` line and every `play`/`stop` reference yield no entry: no audio id from the demo pack appears in the list.
- A pack whose `title_backdrop` id no scene's `bg` declares yields that background entry — the backdrop-only fixture — and a pack whose backdrop id a scene also declares yields the one merged entry, with no marker.
- The order rule holds independently of scene-file enumeration order: permuting the scene files' names does not change the list, and two calls to `Pack::gallery_entries()` return the same list.
- A background entry's label is its raw id; a sprite entry's is `npc_id · pose` with the U+00B7 separator; the still's caption equals the card's label.
- An entry whose file is absent from `assets/` still appears and still opens (its failed art is `### Behavior` §6's, rows 3/4).
- A pack declaring no `bg`, `sprite` or `title_backdrop` yields no entries — `### Behavior` §6's `No images in this pack` state.
- `Pack::gallery_entries()` returns kinds, ids and each entry's label, and never an asset path; the app resolves a background entry to `backgrounds/<id>.png` via `background_asset_path` and a sprite entry to `sprites/<npc_id>/<pose>.png` via `sprite_asset_path` — one producer each, and the play path calls the same two.
- No entry's label contains a character outside `[a-z0-9_ ·]`, and no two entries share a label.

### Layout

Settled 2026-10-05. Prototype-validated: the throwaway stub lives on a prototype branch at `app/prototype/gallery.html` (out of `main`), three layouts over one gallery model; the **framed-card grid** (variant B) won, and the contact sheet (A) and the preview-and-rail (C) remain comparison-only. **Built.**

This subsection fixes the gallery's **look** only: its shape, the card and its art, the grid's geometry at 1280×720, the arrow mapping the walk uses, the placeholder, the still's caption and the backdrop treatment. What the surface is and does — the two doors, its availability, the back-out, the walk's order and keys, the empty and failed states and the copy — is `### Behavior`'s; what it lists is `### Entries`'s; the panel, card and focus vocabulary it reuses is [Menu overlay](runtime-hardening.md#menu-overlay)'s; the faces are [Type system](#type-system)'s.

#### 1. Shape

- **One surface, both doors.** A header row over the grid — or over the empty-state line. From the title's door the surface spans the window; from the overlay's door it is the overlay's main column, so the rail and the top bar stand unchanged ([Menu overlay](runtime-hardening.md#menu-overlay) §1). The header carries the door-aware back control leftmost, the `Gallery` heading beside it, and the grid legend right-aligned in the same row.
- **Only the grid scrolls.** The header stays put; the body scrolls when the cards exceed it and the focused card is scrolled into view (`### Behavior` §4).
- **The empty state is the same header over one line**: the grid legend is not drawn and `No images in this pack` is centred where the grid would be (`### Behavior` §6).
- The still replaces the grid and the header inside the same surface (§6).

#### 2. The card

- **The gallery reuses the overlay's card/panel treatment, scaled for art** — the same panel fill, hairline border and corner radius as [Menu overlay](runtime-hardening.md#menu-overlay) §1's slot cards — and adds no second card vocabulary. The card is a framed panel: an art box on top, a footer line under it.
- **The label is the entry's, verbatim** (`### Entries` §2): left-aligned in the footer under the art, in the mono label tone — the card's only text. No kind, no status, no invented display name.
- **A missing or undecodable image draws the placeholder frame**: the same 4:3 art box with a faint 1px solid outline *(amended 2026-10-07)*: Bevy 0.19's UI has no dashed-border primitive, so the card's box and the still's art box each draw the same faint outline, the closest the engine offers, and the label still renders in the footer. No marker, no glyph and no Notice — silent degradation ([ADR 0008](adr/0008-player-first-failure-posture.md)); activating it opens the still on the same frame and label (`### Behavior` §6).

#### 3. Card art and scaling

- **Every entry is drawn contain-fit, and no pixel is cropped.** The scale is `min(box_w / image_w, box_h / image_h)`; the drawn size is the image's natural size times that scale, centred in the art box, so the aspect is preserved and the box's overflowing axis shows a letterbox of the card's own fill.
- **One rule for both kinds.** Backgrounds and sprites scale identically: a 1280×720 background letterboxes top and bottom in the 4:3 box, a 256×512 transparent sprite pillarboxes. The letterbox is never tinted, filled or cropped, and there is no checkerboard, veil or per-kind rule.
- **The rule is the inverse of the shipped cover maths.** `cover_size` (used by the title backdrop) scales to fill; this is the same computation with `min`, so the build gains one `contain_size` producer beside it and the still calls the same one (§6).
- **Transparency stays transparent**: a sprite's unmarked pixels draw nothing of their own, so the card fill shows through in the grid and the backdrop and scrim show through in the still.

#### 4. Grid geometry at 1280×720

- **Auto-fill, three across on both doors.** The grid is `repeat(auto-fill, minmax(300px, 1fr))` with 20px row and column gaps. At 1280×720 that is three cards across whether the surface is the full window or the overlay's main column, so the line-up does not change with the door; the last row is left-aligned and may be short.
- **It scrolls rather than shrinks.** A pack with more cards than fit scrolls the body vertically; the focused card is scrolled into view and the header never moves.

| Part | Size / treatment |
| --- | --- |
| Grid | `minmax(300px, 1fr)` auto-fill · 20px gap · 3 across at 1280×720 |
| Card | the overlay's framed panel, ~10px padding, 2px border |
| Art box | 4:3 · ~360×270 from the title's door · ~291×218 from the overlay's |
| Label | mono, under the art, left-aligned in the footer |
| Focus | 2px `FOCUS` border, no other change |
| Scrim | radial `rgba(5, 7, 10, 0.34)` → `rgba(5, 7, 10, 0.74)`, title door only |

#### 5. Focus and the walk

- **Focus is the overlay's `FOCUS` ring and nothing else**: the focused card's 2px border turns orange and the card is otherwise unchanged — no fill change, no brightness change, no second selection treatment ([Title layout](#title-layout) §7's ring, reused on cards).
- **`←`/`→` step one card**, `↑`/`↓` step one row in the same column — by the grid's current column count (three at 1280×720) — and when the destination row has no card in that column, a short last row, the move clamps to the grid's last entry. The walk does not wrap, and `Enter`, `Space` and a click activate the focused card (`### Behavior` §4).

#### 6. The still

- **The still is the same surface, zoomed**: the header and grid give way to one entry on the door's backdrop and scrim, contain-fit in the window above a foot caption strip.
- **A foot strip holds the caption.** The image is contain-fit (§3) in the area above it; the caption — the entry's label, verbatim — is centred under the image on the backdrop, and `### Behavior` §7's still legend sits centred under the caption in the faint tone. The caption is never drawn over the art.
- **Nothing else changes behind it**: the backdrop and the scrim stand as they were, and `←`/`→` change only the image and the caption (`### Behavior` §5).

#### 7. Backdrop and scrim

- **The backdrop is the door's** (`### Behavior` §6). From the title's door — the pack's `title_backdrop` when declared, the neutral gradient otherwise — the gallery draws a **subtle dark scrim** between the backdrop and the grid or still, so the heading, the labels and both legends hold contrast over pack art. The stub's numbers: a radial from `rgba(5, 7, 10, 0.34)` at the middle to `rgba(5, 7, 10, 0.74)` at the edges. This is a gallery-local rule: [Title layout](#title-layout) §2 leaves contrast over pack art to the pack, and the title's own surfaces keep that rule.
- **The overlay's door adds none**: that door already stands on the dimmed, blurred freeze ([Menu overlay](runtime-hardening.md#menu-overlay) §1).
- **The scrim darkens the backdrop only** — never a card's art and never the still's image — and it stands unchanged while the still steps (`### Behavior` §5).

#### 8. Validation rules (the build's test list)

- At 1280×720 the grid lays out exactly three cards across from both doors, with a 20px gap; the header and legend render without scrolling.
- With a 19-entry pack the grid scrolls, and after every arrow move the focused card's rect is inside the body's visible rect.
- Contain, not cover: a 1280×720 background in the 4:3 art box draws box-width wide, `box_w × 720 / 1280` tall and centred, with equal letterbox above and below; a 256×512 sprite draws box-height tall, `box_h × 256 / 512` wide and centred, with equal pillarbox. In both, `drawn_w / drawn_h == natural_w / natural_h`, `drawn_w <= box_w` and `drawn_h <= box_h`.
- A transparent sprite's unmarked pixels are not filled or tinted: in the grid the card fill shows through, in the still the backdrop and scrim; no checkerboard is drawn anywhere.
- The focused card differs from an unfocused one only by its 2px `FOCUS` border — no fill, brightness or size delta — and the ring is drawn on the first card at every open.
- The walk: `←`/`→` move ±1; `↑`/`↓` move by the grid's current column count when that entry exists, else clamp to the grid's last entry; the first card's `↑`/`←` and the last card's `↓`/`→` are no-ops (no wrap).
- A missing-art card draws the 4:3 placeholder frame — a faint 1px solid outline *(amended 2026-10-07)* — with its label in the footer and no marker; activating it opens a still with the same frame and label.
- The still draws its image contain-fit above a foot strip holding the entry's label, verbatim, and the still legend; `←`/`→` change only the image and the caption.
- From the title's door the scrim is drawn between the backdrop and the surface; from the overlay's door it is absent; the scrim never covers a card's art or the still's image.
- An empty pack draws the header without the grid legend and the single line `No images in this pack`; no card and no grid is drawn.
- The card follows [Menu overlay](runtime-hardening.md#menu-overlay) §1's card vocabulary — fill, hairline border, radius — scaled for art and a footer line; the gallery adds no second card treatment.

### Amendments

Amendments to this section are dated and made **in place**, with a note here saying what changed; later `###` subsections are appended and never rewrite another's.

- **2026-10-05**: the section and `### Behavior` are new — the two doors, the one-level back-out, the still view, the empty-pack and failed-art states, and the copy. [Title](#title) §2/§6/§7, [Title layout](#title-layout) (intro, §1, §6, §7, §8) and [Type system](#type-system) §1/§2/§4/§5 are amended in place wherever they described the disabled row; [Menu overlay](runtime-hardening.md#menu-overlay) §1 gains the top-bar control and the gallery view. No failure row is added: failed card art is rows 3/4, and `FailureRow::ALL` does not move.
- **2026-10-05**: `### Entries` is new — the declared-reference entry model, its labels, its order and the derivation seam. The manifest's `title_backdrop` is an entry, merged into the background of the same id (the recommendation to exclude it is not taken); the sprite path rule is factored into a `sprite_asset_path` producer so the gallery does not fork it; the accepted limitation is added once to [content format §2](content-format.md#2-pack-layout). No other subsection is rewritten.
- **2026-10-05**: `### Layout` is new — the framed-card grid (variant B), the contain scaling rule, the overlay's card vocabulary reused, the 4:3 art box with a footer label and the dashed placeholder, the auto-fill geometry, the clamped same-column walk, the still's foot caption strip and the title-door scrim. `### Behavior` §6's backdrop *source* is unchanged — the scrim is a gallery-local look rule and [Title layout](#title-layout) §2's contrast line stays the title's. `### Behavior` §5 is amended in place: its "full-window" line and its still-fit validation bullet now name `### Layout` §6's caption strip. The section's intro is amended in place: `### Entries` and `### Layout` have landed, so it no longer says they will. No other subsection is rewritten.
- **2026-10-07**: `### Layout` §2's and §8's "dashed" placeholder border is reconciled with the build. Bevy 0.19's UI exposes no dashed-border primitive (`grep` finds none in `bevy_ui`), so the placeholder frame is a faint 1px solid `Outline` drawn on the card's 4:3 art box and on the still's art box — the closest the engine offers, and separate from the card panel's 2px `BorderColor` hairline. The frame's behaviour is unchanged: it shows while the file loads and after a failed load, and the entry's label renders in the footer either way. No failure row moves.
- **2026-10-08**: Behavior §1 and §8's validation bullet gain Timeline before Gallery in the Slots Tab cycle; Timeline joins the sub-views retaining Save → Load. Gallery's two doors and its own behaviour are unchanged.
- **2026-10-14**: the four settled banners above (`## Gallery`, `### Behavior`, `### Entries`, `### Layout`) now read **Built** — the viewer shipped. No behaviour, entry or layout rule changed.

---

## Timeline

Settled 2026-10-08, four interview rounds including the shared-understanding confirmation. **Built.** This section consumes [Read log](runtime-hardening.md#read-log)'s visited set, [Manifest and state](content-format.md#6-manifest-and-state)'s declared scene order and [Text skip](runtime-hardening.md#text-skip)'s discontinuity rules. Geometry and visual treatment are recorded below in [Layout](#layout-1), folded from the approved layout prototype; the preceding numbered subsections own behaviour.

### 1. Home, reach and back-out

- **A fifth `MenuView`, `Timeline`**, alongside `Title | Slots | Settings | Gallery`, on the existing pause gate; no new Bevy state or freeze predicate.
- **One door: the Esc overlay's Slots top bar.** The `Timeline` control sits immediately before `Gallery`. While Slots is up, `Tab` cycles **Save → Load → Timeline → Gallery → Settings → Save**. Timeline, Gallery and Settings entry controls appear only there; sub-views keep the existing Save → Load cycle. The title's five rows stay unchanged. Neither title-origin Settings nor Gallery opens Timeline, and Timeline has no direct Settings/Gallery door.
- The heading is **`Timeline`**, with **`← Back to slots`** and the overlay's Resume affordance. `Esc` cancels an open confirm first, then returns Timeline to Slots, then resumes play. The back control is a click/`Esc` affordance rather than a card-arrow stop, following Gallery. Returning to Slots follows its existing back-out focus convention (first slot card); it reads no slot metadata again.
- **Available at every wait the overlay reaches**: say, choice, decision with a request in flight, an answer already held, and scene end. There is no decision-specific jump gate.
- **Browsing is a presentation pause, not a discontinuity.** Opening, walking, cancelling a confirm and backing out do not change the runner, staging or settings, cancel a request, discard a held answer, mark anything read/visited or write any file. No slot read or settings flush is added. A running request may complete and become held as usual; it is not applied until play resumes. The inherited pause set freezes script/input, reveal, auto/skip dwell, Notice countdown and play hotkeys. Only an accepted jump replaces the old position.
- **The door stays enabled without declared entries.** Absent `scenes` and `scenes: []` show `This pack has no Timeline.` under the heading, with no cards or card focus; Back and Resume still work. “No Timeline” in the manifest contract means no entries, not a derived order or a hidden navigation control.

### 2. Entries, cards and focus

- The engine exposes **`Pack::timeline_entries()`**, a read-only accessor yielding scene ids in the manifest's declared order; absent/empty order yields no ids. This is enumeration, not UI data: no visited join, lock state, card, Bevy type, directory scan or `HashMap` sort. The app joins those ids with the launch's in-memory read log and renders them. The manifest field and its preservation are the scenes-list build's responsibility; this accessor must preserve that order exactly.
- **Every declared scene has one card.** Existing-but-unlisted scenes have none, even when visited or currently live. Listed-but-missing scenes and duplicate declarations are load errors under the manifest spec, not placeholder cards. The read log is the visited authority, not save-slot metadata or this playthrough's path: previous launches' visits, a loaded/copied save's restored scene and force-skip crossings all count. The boot park behind the title does not.
- **Visited card:** its raw scene id as label, selectable. **Unvisited card:** `???`, locked, no scene id, title or thumbnail exposed. No ordinal, timestamp, completion percentage or Current badge is added. These are text cards; their geometry is [Layout](#layout-1)'s.
- Locked cards stay in the arrow walk, with visible focus, but `Enter`, `Space` and click are inert: no confirm, jump or Notice. The current scene, if declared and visited, is selectable too: it restarts from the top behind the same confirm.
- **Initial focus on every open is the first selectable card in declared order, otherwise the first declared card.** No remembered focus. A current unlisted scene and an entirely locked declared list are valid: the first card is focused and all activations are inert. An empty list has no card focus.
- `↑`/`↓`/`←`/`→` walk the declared card order, **without wrapping**; the focused card is scrolled into view. [Layout](#layout-1) fixes the two-column mapping: horizontal ±1, vertical ±2, clamped to first/last (including deliberate diagonal end clamping). `Enter`, `Space` and click activate; digits do nothing. Confirm cancellation leaves focus and scroll on the target card.

### 3. One confirm and fixed copy

- **Every selectable target confirms once**, including a same-scene restart. The only door is in a live session, so there is no boot/no-session exception. A locked card never reaches this confirm.
- The title names the raw target id. The first detail sentence is the Load card's destructive sentence verbatim; the next two sentences are the human-selected state-carry explanation. “Current game state” here means the carried state map, not a historical snapshot, a saved slot or a promise that the target's commands cannot change it.
- Cancel, or `Esc` on the confirm, returns to the same card with the old session intact; requests continue/answers are held under the normal pause rule. No second confirm and no slot operation.

| Where | String |
| --- | --- |
| Door / heading | `Timeline` |
| Back | `← Back to slots` |
| Explanatory line | `Choose a visited scene to start it from the beginning with your current state.` |
| Visited / locked label | raw scene id / `???` |
| Empty state | `This pack has no Timeline.` |
| Confirm title | `Jump to <scene id>?` |
| Confirm detail | `Unsaved progress is lost and any decision in flight is cancelled. The scene restarts from the beginning. Current game state is retained.` |
| Confirm buttons | `Jump` / `Cancel` |
| Walk legend | `↑/↓/←/→ move · Enter/Space select · Esc back · click to choose` |

### 4. Jump and landing

- **A scene-from-top teleport, not rewind.** Use the existing runner seam `restore_position(target_scene, 0, current_state, None)`. The id names the scene directly, not a label lookup; a local label with the same name must not intercept selection. There is no new runner jump rule, historical state per scene, save read or fresh-default-state reset.
- **Carry current state and current staging** (background, visible sprites and BGM). Run the target from the top normally and apply its staged commands to that carried staging until the next wait. Leading `set`, flow, `bg`, `sprite` and BGM commands may change what was carried. If the target does not replace an old sprite or BGM, it stays; replaying the scene does not reconstruct its original first-visit frame.
- **Successful replacement cleanup**, before the new play path can run: cancel/drop the old pending decision task; discard any held answer; release a defensive hold; disarm old `PendingAutosave`; reset reveal, `AutoDwell`, skip dwell and old arrival status; clear seen skip and disarm force skip (a held Ctrl needs release and a fresh press in play); clear the old Notice; force audio/visual/UI replay like a load. Keep settings, including auto and text speed, manual saves, the autosave and the read log's existing knowledge. Clearing old arrival status does not skip computing the new park's status before its normal mark.
- **Close the overlay and enter play**, parked at the next normal wait, with no extra advance spent on it. Auto's preference survives, but its dwell starts fresh after full reveal. A leading decision issues its own new request normally, never the old request's result. Leading authored jumps/choices/decisions and scene end obey the ordinary runner/app pipeline.
- **Restore refusal is defensive**, not a new error surface: validate/restore before destructive cleanup, preserve the old runner/staging and pending/held session, remain on Timeline, and use the existing failure funnel (error log, Notice and defensive hold). Loader validation makes a declared missing scene unreachable in normal play. No new failure-inventory row is introduced.

### 5. Writes, read marks and accepted consequences

- **The selection itself writes no autosave**, including a same-scene restart. Disarming the old pending write prevents it from capturing the replacement position. An ordinary authored crossing encountered while running the target, a later choice resolve or model-answered decision resolve still has its normal autosave effect; the selection's no-write rule is not a suppression of script events.
- **Accepted consequence:** jump → Quit before any normal autosave event → relaunch → Continue loads the pre-jump autosave, not the Timeline landing. Manual slots are untouched.
- **Ordinary marks only.** The selected scene is already visited, so its visit mark normally inserts nothing. Any subsequent scenes actually crossed become visited by the usual rule. A displayed say-park evaluates arrival status then marks its own authored line normally; no whole-scene mark, completion mark or unseen-line mark is invented by selection. Force skip stays disarmed at landing.
- **Every confirmed landing is a scene-entry read-log flush opportunity**, including a same-scene restart, after that frame's parks settle. Dirty-gated, independent of autosave; if there is no new knowledge it writes nothing. A failed flush takes the existing read-log row 26's log-only posture and dirty retry rule. Browsing/cancellation adds no flush opportunity.

### 6. Validation rules (the build's test list)

- `Pack::timeline_entries()` returns the declared ids in exactly authored order; absent/empty returns empty; a loaded unlisted scene is absent. A label sharing a declared scene id cannot redirect Timeline selection.
- Slots shows Timeline before Gallery and `Tab` cycles Save → Load → Timeline → Gallery → Settings → Save. Sub-views keep Save → Load; no Timeline control appears on title or title-origin Settings/Gallery. Title still has five rows.
- Open Timeline from say, choice, in-flight decision, held answer and scene end: runner/staging unchanged; play input, reveal, auto/skip dwell and Notice countdown frozen; request may finish into held state but never applies. Opening/walking/backing out/cancelling reads no slot and writes no file.
- Visited ids render verbatim and select; unvisited entries render only `???`, retain visible focus and activate inertly under Enter/Space/click. Existing-but-unlisted entries never render. A copied save and prior-launch visits unlock their declared cards; merely looking at the boot title does not.
- On each open, focus starts at the first selectable entry, or first declared entry when all are locked. Closing/reopening forgets the previous focus. Current unlisted + all declared locked is usable with inert cards and Back/Resume.
- Arrow walk includes locked cards, does not wrap, scrolls focus into view, and digits do nothing; geometry/neighbour tests follow [Layout](#layout-1)'s additional rules. Same-scene selection confirms and restarts normally.
- Absent/empty order keeps the enabled door and renders the exact empty copy with no cards/focus and functioning Back/Resume; no order is derived.
- Confirm title names the target and detail/buttons match §3 exactly. Cancel/Esc leaves position and staging intact, preserves pending/held under pause doctrine, writes nothing and returns to the same focused card; subsequent Esc returns to Slots, then resumes.
- A target with no leading state/staging changes keeps the carried state, background, sprites and BGM; a target with leading changes applies them normally. The runner starts at scene pc 0/no wait, not the historical first-visit state or pack defaults; the first normal wait is not extra-advanced.
- Successful jump cancels pending and clears held so an old result never lands; releases hold, disarms stale pending autosave, clears Notice, resets reveal/auto/skip clocks and arrival status, forces replay, and closes the overlay. Auto/text speed/settings and manual slots are unchanged.
- Held Ctrl cannot force-skip the landing until released/repressed in play; seen skip is off. Arrival status is newly computed before the landing mark, never carried from the discarded park.
- Leading decide creates only the target's new request; a leading authored cross-scene jump retains normal autosave/visit behaviour; a target ending immediately resumes into normal scene end.
- Selection with no qualifying script event writes no autosave, even with a stale pending flag beforehand; the old autosave remains byte-identical. Jump + Quit + relaunch Continue loads it. A subsequent normal crossing/choice/model resolve writes as usual.
- Landing marks only its displayed say-line (if any), never unseen lines or whole scenes. Different-scene landing and same-scene restart each dirty-flush after parks settle; an already-known landing with no dirty knowledge writes nothing. Failure of this flush uses row 26, no Notice, and leaves dirty set.
- A forced restore refusal preserves the old session before cleanup, keeps Timeline open and follows the existing error/Notice/defensive-hold posture; no new surface or inventory row.
- Every copy string in §3 renders verbatim. Back-out returns to Slots' first card without refreshing metadata. Runtime play tests still pass through the real pause/jump input path; no test-only bypass.

### Layout

Settled 2026-10-08 by the layout prototype, folded by the Timeline build. **Roomier variant A won** after human confirmation. Primary source: the throwaway prototype's `app/prototype/timeline.html` stub, which stays out of `main`. B/C remain comparison-only; never merge the prototype or its assets into main. This is a structure/spacing/walk reference, not a guarantee of pixel-identical Bevy rendering. Behaviour, copy, first-selectable focus and jump semantics remain §1–§6's.

#### Reference proportions (1280×720)

| Part | Approved size / treatment |
| --- | --- |
| Frame / backdrop | Supported minimum 1280×720; inherited dimmed/blurred frozen play frame, no new backdrop |
| Rail | 240px wide, 20px padding, 10px gaps; existing pack/current-scene/Resume controls |
| Main | 24px padding, 16px gaps; heading, explanation, internally scrolling card body, legend |
| Grid | Two equal columns, **136px-high cards / 16px gaps**, 18px card padding; about three full rows before scrolling |
| Card width | Distribute available body width; about 486px without scrollbar, 478.5px measured in Chrome with a 15px scrollbar—**not a fixed card width requirement** |
| Heading / explanation / legend | 24px Plex Sans SemiBold / 16px Plex Sans Regular / 12px Fira Mono Medium |
| Card labels | 18px Fira Mono Medium; raw visited id or `???`; long ids wrap rather than truncate |
| Fill / label tones | Card `#1f212b`; visited `#e6e6eb`; locked `rgba(204,204,217,.35)` |
| Focus | Reserved 2px `#e69e5c` border, identical on visited and locked cards; no size or fill change |
| Confirm | Existing overlay confirm vocabulary, 420px panel, 20px padding, 12px gaps; 22px SemiBold title, 15px regular sans detail; black 45% dimmer; destructive Jump `#7a2929`; exact §3 copy |
| Motion | Instant changes; no animation or new surface |

Locked focus deliberately communicates **progression and relative position**, never completion: visited means entered, not completed. The card exposes only `???`; no ordinal, timestamp, Current badge, art or completion percentage. Empty order retains §1's enabled door, explanation and Back/Resume, without cards/card focus; use §3's empty copy.

#### Walk and scrolling

- Horizontal arrows move index ±1; vertical arrows move ±2. Clamp the result to `[0, last]`, never wrap, include locked cards and scroll the entire focused card into view. End clamping may move diagonally; this is intentionally not a strict-column no-op rule.
- Opening still focuses the first selectable card, otherwise the first declared. Cancel retains target focus and scroll; returning/reopening follows §1/§2, not a layout-local remembered focus.
- Header, explanation, rail and legend remain outside the card body's scroll. Long lists scroll internally rather than increasing window height. Long ids wrap within card width; build checks must verify representative long ids remain legible within the fixed height.
- In-play cue geometry belongs only to [Text skip §7](runtime-hardening.md#7-cue-and-labels): Timeline introduces no SKIP cue or alternate placement. The prototype's shaded dialogue sample is staging, not a required production skin.

#### Validation rules (the build's test list)

- At 1280×720, render two equal-width columns with 136px cards, 16px gaps and 18px padding; header/explanation/legend and rail controls fit outside the scroll body. Card width follows available body space rather than a fixed browser measurement.
- Mixed, all-locked and first-locked/later-visited fixtures retain §2's opening focus; locked focus is the same reserved 2px orange ring as visited focus, without size/fill change or identity exposure.
- A 30-entry fixture and representative long raw ids wrap without truncation; every focused card including the last is fully visible after scrolling.
- Arrow moves use ±1/±2 and clamp to first/last without wrap, including deliberate diagonal clamps at incomplete/end rows; locked entries stay in the walk and activating them is inert.
- Cancel/Esc from the one confirm restores identical target focus and scroll, old position/staging and file bytes; verify §3's exact copy and the inherited panel proportions/tones in Bevy.
- Absent/empty order renders the enabled door and exact empty copy without cards/card focus; Back/Resume remain usable.
- Labels, heading, explanation and legend use the specified sans/mono roles, sizes and tones; no extra badge, completion value, new art or motion appears. Confirm and jump behaviour remain production-path tests, not prototype mock assertions.

### Amendments and scope

- **2026-10-08, build-spec synthesis**: Layout records roomier prototype A, approved geometry/tones, locked-focus rationale, clamped ±1/±2 walk and Bevy validation. Intro/index and §2/§6 point here instead of deferring geometry to the now-resolved prototype. The two prerequisite wording flags below are resolved by dated Read log/Text skip amendments, without changing their normative lifecycle.
- **2026-10-08**: this section and the Sections-so-far entry land; Title's booked read-log/chapter pointer is superseded in place. Gallery's top-bar cycle, Menu overlay §1/§5/§6 and Read log §6 are amended in place to name the Timeline and its scene-entry flush opportunity. The glossary gains Timeline and the retired chapter alias.
- **Not part of this behaviour contract:** feature implementation, card geometry/visuals (now recorded in Layout), historical checkpoint rewind, save-format changes, settings/CLI changes, runner jump-rule changes, gamepad or rebinding. **2026-10-08 clarification:** the prerequisite New Game “unchanged log” and absent-log “whole launch” wording flags are resolved by the build-spec synthesis: retain existing knowledge with ordinary entry additions/flushes, and initialize empty with later same-launch revisits eligible. Timeline still consumes the same monotone-visit and mark-at-park rules.
- **2026-10-14**: the settled banner above now reads **Built** — the surface and the overlay selection shipped. No rule changed.
- **2026-11-14**: the Timeline view moves into the right column of the Esc menu's contents page, and its rail, Back control and Resume give way to the page's left column ([Default look](#default-look) §4). The cards, walk, confirm and jump rules are unchanged.

---

## Default look

Settled 2026-11-14. **Not built.** Prototype-validated: the clickable stub lives on the throwaway branch `prototype/8-vn-ui-redesign` at `app/prototype/vn-ui.html` (out of `main`), with four layouts and one mix over one VN model. The **mix, variant E**, won: variant A's play frame, choices and title with variant B's contents page as the Esc menu. A–D remain on that branch as the comparison record.

This section is the one owner of the default look of the play frame, the title and the Esc menu, for the improved demo pack and every default install. Where an earlier layout rule disagrees, this section wins, and that rule carries a dated pointer here. Behaviour — the title's states and confirms, the pause doctrine, the skip and auto rules, the slot actions, the Timeline and Gallery contracts — is unchanged unless a numbered point below says otherwise. Faces follow [Type system](#type-system) as amended by §6.

### 1. Play frame

```
┌───────────────────────────────────────────────────────────────┐
│                                                      Notice   │
│                                                               │
│                           (sprite)                            │
│    ┌─────┐                                                    │
│ ┌──┤ Ace ├───────────────────────────────────────────────┐    │
│ │  └─────┘                                               │    │
│ │  A letter? Ah... you saw that.                         │    │
│ │                                                      ▼ │    │
│ │  Save  Load  Auto A  Skip Tab  Log           Menu Esc  │    │
│ └────────────────────────────────────────────────────────┘    │
└───────────────────────────────────────────────────────────────┘
```

- **The dialogue box** spans the frame's width less a 72px side inset, sits 28px above the bottom edge and is 178px tall, so its top edge is at y 514 at the 1280×720 reference. Fill: a vertical gradient from `rgba(16,18,26,.95)` to `rgba(10,11,16,.97)`, a 1px `rgba(255,255,255,.14)` hairline, a 14px corner radius and a soft drop shadow. Sprites stand behind it.
- **The name tab** overlaps the box's top edge at its left (28px in, 20px above), filled with the `FOCUS` orange and carrying the say-line's speaker in dark text. **Narration has no tab**: a say-line with speaker `Narrator` renders its prose alone, in the dim tone, as [Say-lines and narration](content-format.md#say-lines-and-narration) already requires ("no speaker sprite/name plate"). The prototype's italics are not adopted, because [Type system](#type-system) ships no italic face.
- **The prose** is 25px with a 1.5 line height, inside 40px side padding. The reveal, its speed and its pause rules are [Player comfort](runtime-hardening.md#player-comfort) §3's. A blinking `▼` advance marker shows at the box's right once the line is fully revealed.
- **At a decision-wait** the box shows the existing `<npc_id> is deciding…` line in the `FOCUS` tone with an animated three-dot marker, and no tab. **At end of scene** it shows the existing `— end of scene —` line.
- **The quick row** runs along the bottom of the box, 12px mono in the faint tone, in this order: `Save`, `Load`, `Auto A`, `Skip Tab`, `Log`, then `Menu Esc` right-aligned. Each control is clickable, and a click on the row never also advances the line:
  - `Save` and `Load` open the Esc menu at that section, from any parked state where `Esc` opens it.
  - `Auto A` and `Skip Tab` act exactly as `A` and `Tab` do, including where those keys are inert ([Player comfort](runtime-hardening.md#player-comfort) §4, [Text skip](runtime-hardening.md#text-skip)).
  - `Log` opens the **Backlog**, a new surface that is not yet specified. Until the Backlog is settled and built, the row has no `Log` control and the other controls close over its place.
  - `Menu Esc` opens the Esc menu as `Esc` does.
- **The mode indicator is the lit label.** `Auto A` renders in the `FOCUS` tone while auto is on; `Skip Tab` renders in the `FOCUS` tone whenever skip is effective, the held-`Ctrl` force skip included. The exclusivity rule stands: while skip runs, `Auto A` is not lit. This replaces the separate `AUTO`/`SKIP` corner cue.
- **There is no help line.** The quick row's key labels, the `▼` marker and the numbered choice buttons carry the keys in play; `Space`/`Enter`/left-click, `Ctrl` and `F11` are not shown in play. The README's controls table is unchanged.
- **The Notice** keeps its top-right slot (`top: 12px`, `right: 48px`).
- The box, the tab and the quick row are hidden on title surfaces, exactly as the dialogue band is today.

### 2. Player choices

- At a choice-wait the options render as **numbered buttons stacked in the centre of the frame**: 560px wide, starting 150px from the top, 14px apart, each with 16px×22px padding, a `rgba(14,16,22,.88)` fill, a 1px hairline and a 10px radius. The option text is 21px; the option's number sits in a small mono badge outlined in the `FOCUS` tone. Hover and keyboard focus take the `FOCUS` ring.
- **The question stays on screen.** The box keeps showing the say-line the player read before the choice, fully revealed, with its tab and without the `▼` marker. This is presentation state only: the app remembers the last say-line it rendered. No `Wait` changes and nothing is saved, so after a load onto a choice-wait the box is empty.
- `1`–`9` and clicks pick exactly as today.

### 3. Title

- **The layout is a left column, not a centred one.** The column starts 104px from the left edge and 150px from the top and is 420px wide. From the top: the wordmark (64px Plex Sans Bold, tight tracking), the tagline (12px mono, uppercase, 0.34em tracking, accent tone, 14px above and 46px below), the five rows in [Title](#title) §2's order, then Continue's reason line 26px below the rows.
- **Rows are text-only**, 22px, with no panel behind them. The focused row is white with a 12px×2px `FOCUS` bar to its left; unfocused rows are in the muted tone, and a disabled row is dimmed but stays in the walk. `Quit` sits 14px below the other four, in place of the hairline rule.
- **The legend** keeps [Title](#title) §6's string and sits at the frame's bottom right (40px in, 26px up), in mono.
- **A veil is drawn over the backdrop.** Over the pack's `title_backdrop` and over the default gradient alike, the title draws a left-to-right gradient from `rgba(8,9,13,.86)` at the left edge through `rgba(8,9,13,.55)` at 38% to transparent at 62%. The wordmark, rows and reason line stay readable over any art, so their contrast is no longer the pack's responsibility alone. The art is otherwise drawn as authored.
- **Settings from the title** opens the contents page of §4 on its Settings section, over the same backdrop and veil instead of the page fill, with only the `Settings` entry and `← Back to title` in the left column. The Gallery's title door keeps its current layout.

### 4. Esc menu: the contents page

```
┌──────────────────────┬────────────────────────────────────────┐
│ CONTENTS             │ Save your place — you are in reunion   │
│                      │                                        │
│ Save ·               │ SLOT 1    reunion             Overwrite│
│ Load                 │           12 Nov 2026 14:03            │
│ Timeline             │ SLOT 2    —                   Save here│
│ Gallery              │ SLOT 3    Unreadable          Overwrite│
│ Settings             │ AUTOSAVE  first_meeting       Load-only│
│                      │                                        │
│ ▶ Resume reading     │                                        │
│ Exit to title        │                                        │
└──────────────────────┴────────────────────────────────────────┘
```

- **One opaque full-frame page** replaces the overlay's rail, top bar, card grid and dimmed freeze: a `#0d0e12` fill with a 420px left column and a right column, divided by a hairline. The play frame is not drawn behind it. The pause doctrine is unchanged ([Menu overlay](runtime-hardening.md#menu-overlay) §6).
- **The left column** holds a small mono `CONTENTS` label, then the sections as large SemiBold entries in order (`Save`, `Load`, `Timeline`, `Gallery`, `Settings`), with the current one white and marked with an orange `·` and the others in the faint tone. At the column's foot sit `▶ Resume reading` in the `FOCUS` tone and `Exit to title`. The page replaces the rail, so the pack name and the current scene move into the right column's heading.
- **Save and Load** show the slots as a ledger in the right column: a heading line (`Save your place — you are in <scene>` or `Return to a page — you are in <scene>`), then one row per slot in `Slot::ALL` order. A row holds the slot label (mono), the scene id or status over the saved-at line, and the action at the row's right, in the `FOCUS` tone or the muted tone when disabled. The ledger shows exactly the fields of [Menu overlay](runtime-hardening.md#menu-overlay) §2 and the actions of §3; the legacy entry becomes a final ledger row, in save mode only, under §4's rules.
- **Timeline, Gallery and Settings** render their current views in the right column. Their own rails, back controls and Resume give way to the left column. This section does not redesign their interiors.
- **The walk.** `Tab` cycles the sections exactly as the top bar did (Save → Load → Timeline → Gallery → Settings, and Save → Load inside the sub-views). In Save and Load, `↑`/`↓` walk the ledger rows, the legacy row included, and `1`–`3` focus a manual slot; `←`/`→` do nothing there. `Enter`/`Space` activate, focus wears the `FOCUS` ring, and pointer and keyboard are equal. `Esc` keeps its four moments, so it also resumes from the slots. Settings, Gallery and Timeline keep their own walks.
- **Exit to title moves** from the Settings view's rail to the contents page's foot, so every section reaches it, with [Title](#title) §5's confirm.

### 5. Monitor clearance

The box's top edge rises to y 514, so the dev-only Monitor drawer moves up to keep the dialogue visible: the drawer spans **y 212–500 (288 tall)**, a **220px bottom reservation**, and its log body shrinks to about 221px, about 11 single-line rows. Everything else in [The panel](runtime-hardening.md#the-panel-q1q2-verdict-a) stands.

### 6. Faces

[Type system](#type-system) §2's split holds, with these rows replacing its Title, Dialogue band and Esc overlay rows:

| Surface | Sans (Plex) | Mono (Fira Mono Medium) |
| --- | --- | --- |
| Dialogue box | the name tab (SemiBold); the say-line prose; the deciding line; the choice option text | the quick row; the choice number badges |
| Title | the wordmark (Bold); the five row labels | the tagline; Continue's reason line; the legend |
| Contents page | the section entries (SemiBold); Resume and Exit to title; the ledger heading and scene id | the `CONTENTS` label; the slot labels; the saved-at and status lines |

The say-line is no longer a mixed string: the tab and the prose are separate nodes, so [Type system](#type-system) §4's dialogue bullet no longer applies.

### 7. Validation rules (the build's test list)

- At 1280×720 the box, tab, prose, `▼` marker and quick row render at §1's geometry; long prose wraps inside the box; narration shows no tab.
- Every quick-row control does exactly what its key does, and a click on the row never advances the line; with the Backlog unbuilt, the row has no `Log` control.
- `Auto A` and `Skip Tab` light and unlight in the same frame as the auto preference and effective skip, held `Ctrl` included, and `Auto A` is unlit while skip runs.
- At a choice-wait the previous say-line stays in the box, fully revealed and without `▼`; after a load onto a choice-wait the box is empty.
- The title renders §3's left column and veil over pack art and over the default gradient, and fits in all four title states, including the longest `SlotInfo::Unreadable` reason.
- The contents page opens on Save from `Esc`, `Menu Esc` and the `Save` control, and on Load from the `Load` control; `Tab`, the ledger walk and the digits behave as §4 says; `Esc` keeps its four moments.
- The Monitor drawer measures y 212–500 and intersects neither the box nor the Notice.
- Every string's face follows §6.

### Amendments

- **2026-11-14**: the section is new. It supersedes, each with a dated pointer in place: [Title layout](#title-layout) §1, §2's art bullet, §4, §5, §7 and §8's layout tests; [Type system](#type-system) §2's Title, Dialogue band and Esc overlay rows and §4's dialogue bullet; the Timeline's [Layout](#layout-1) rail; [Menu overlay](runtime-hardening.md#menu-overlay) §1 and §6's grid walk; [Player comfort](runtime-hardening.md#player-comfort) §4's `AUTO` cue and §6's help line; [The Notice](runtime-hardening.md#3-the-notice)'s placement reason; [Text skip](runtime-hardening.md#text-skip) §7's cue, help-line and dialogue-reservation bullets; and [The panel](runtime-hardening.md#the-panel-q1q2-verdict-a)'s drawer numbers.

### Out of scope for this section

The Backlog itself (a new surface, to be specified in its own section); the interiors of the Settings, Gallery and Timeline views; motion beyond the `▼` and three-dot loops; gamepad support and input rebinding; any `engine/` change; and window sizes below 1280×720.
