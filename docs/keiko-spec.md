---
title: Keiko — build-ready spec
summary: The build-ready spec for Keiko, Unmei's end-to-end testing framework, covering commands, case format, takes, sandbox, faults, stepping and exploration.
keywords:
  - verification
  - content-pack
  - player-input
  - decisions
  - agent-tooling
  - ci
---
# Keiko — build-ready spec

**Status (2026-11-18).** Slices 1 and 2 are **built**, on the integration branch `spec/28-keiko`. A case may carry `frame` and `click`: both draw the player's own composition at 1280×720 on wgpu's fallback adapter ([`app/src/keiko/render.rs`](../app/src/keiko/render.rs)), and a machine without one fails them with `keiko.render.unavailable`. This page turns the accepted [Keiko requirements](keiko.md) (K1–K31) into a build-ready spec. The decisions §10 of the requirements left open were settled with the maintainer on 2026-11-17 and are recorded where they apply. Each section names the requirement it implements. When this page and the requirements disagree, the requirements win and this page is wrong.

**Slice 3 (specified 2026-11-27, issue #179).** Slice 3 is built on the branch `claude/keiko-improvements-d652c3`. It adds planned routes for `activate` (§3.1), layout assertions (§3.5, §13), the always-on focus check (§4.3), the menu walk (§9.5) and fixture packs for menu states the demo pack cannot reach (§14).

**Slices.** The build ships in three slices. Each slice is complete and mergeable on its own.

| Slice | Contents | Done when |
|---|---|---|
| 1 | Commands (§1), cases (§2–§3), takes (§4), observations (§5), decisions (§6), fixtures and faults (§7), stepping (§8), exploration (§9), output (§10), check and packaging (§11), the shared replay (§12), the gate and the docs (§14). | §15's slice-1 rules pass, and the [requirements' Done when](keiko.md#done-when-for-the-build) holds. |
| 2 | Frames and position clicks on the software rasterizer (§13). | §15's slice-2 rules pass. |
| 3 | Fixture packs E and the new G and L cases (§14), planned routes (§3.1, §3.2), the menu walk (§9.5), the always-on focus check (§4.3) and layout assertions (§3.5, §13), built in that order. | §15's slice-3 rules pass. |

The smoke-checklist migration needs no GPU, so slice 1 meets the requirements' Done when without slice 2.

---

## 1. Commands (K3, K27)

Keiko is hosted by `unmei-play` as flags on the pack. The launch grammar in [`app/src/launch/mod.rs`](../app/src/launch/mod.rs) gains these:

```text
unmei-play [PACK_DIR] --take [ID...] [--live] [--keep] [--format text|json]
unmei-play [PACK_DIR] --step ID STEP [--keep] [--format text|json]
unmei-play [PACK_DIR] --observe ID [--keep] [--format text|json]
unmei-play [PACK_DIR] --explore [--menus] [--budget N] [--from ID] [--keep] [--format text|json]
```

- **One Keiko mode per invocation.** `--take`, `--step`, `--observe` and `--explore` are mutually exclusive. When the plugin family's `--check` and `--preview` are built, they join the same exclusive group.
- **Mode modifiers.** `--live` is valid only with `--take`. `--menus`, `--budget` and `--from` are valid only with `--explore`. `--menus` selects the menu walk (§9.5) in place of the story walk (§9.1). *(Amended 2026-11-27.)* `--keep` and `--format` are valid with every mode. `--dev` is a usage error alongside a Keiko mode, because a case sets dev mode itself (§2.2).
- **Values.** `ID` is a case id: the file stem of `tests/<id>.keiko`, matching `[a-z0-9][a-z0-9_-]*` or `findings/<stem>` (§9.4). `STEP` is one step line (§3), passed as one argument. `--take` with no `ID` runs every case under `tests/`, `tests/findings/` included, in byte order of their relative paths.
- **PACK_DIR.** It resolves exactly as for play ([`resolve_pack`](../app/src/launch/pack.rs)), so the demo pack beside the executable is the default.
- **Usage errors** exit 2 before any startup work, as today.
- **No window.** A Keiko mode never creates a window, never opens the pack's real session log and never reads the user's settings, saves, read log or credential file (K17).
- **Help.** `--help` lists the Keiko flags in a "Testing (Keiko)" group.

Exit codes are listed in §10.3.

## 2. The case file (K6–K8)

A case is `tests/<id>.keiko` in the pack folder, UTF-8, one item per line, with LF or CRLF line endings. The engine's pack loader already reads only the files it knows, so the player ignores `tests/` during play (K6). [§16](#16-amendments) adds the folder to the pack layout.

### 2.1 Shape

```text
keiko 1
# Offline: Ace answers coldly after silence
decide ace.greet cold
setting play.text_speed 0

new-game
advance 2
pick "Stay silent"
expect speaker "Ace"
expect line contains "Fine, don't talk"
expect state affinity_level = "low"
```

- **Line 1 is the version line**, `keiko <n>`. Version 1 is the only version. A missing or unknown version fails the case with `keiko.case.version` and nothing else in the file is parsed (K8).
- **Comments** start with `#` as the first non-blank character. Blank lines are ignored. There are no trailing comments.
- **Header lines** (§2.2) come before the first step. A header line after a step is `keiko.case.header-order`.
- **Steps** (§3) follow, one per line.
- **Tokens** are separated by spaces. A quoted string uses `"`, with `\"` and `\\` as its only escapes. Text matching is exact and case-sensitive after both sides are NFC-normalised.
- **Additive changes keep version 1** (K8): a new step, header, fault or observation field. Removing or changing the meaning of any of them bumps the version.

### 2.2 Header lines

| Header | Meaning | Ref. |
|---|---|---|
| `decide <npc>.<question> <action>` | Sets the landing key of one decision point. `<action>` is one of the question's actions or `fallback`. | K15 |
| `setting <key> <value>` | Writes one value into the sandbox settings file before boot. Keys are the settings file's own (`audio.master`, `audio.music`, `audio.sfx`, `audio.muted`, `play.text_speed`, `play.auto`, `play.fullscreen`), with values in the file's RON form. The player's own validation applies, so an out-of-range value normalises as it would in play. *(Amended 2026-11-18: `audio.muted`, not `audio.mute`, which is the settings file's field name.)* | K18 |
| `fixture <kind> <path>` | Copies a file from the pack's `tests/` folder into the sandbox before boot (§7.1). | K18 |
| `fault <name> [args]` | Declares a fault from the fixed list (§7.2). | K19 |
| `dev` | Starts the player in dev mode. Without it, dev mode is off in every build, so a case behaves the same in a debug and a release binary. | K20 |
| `start <scene-id>` | Starts play at the top of a scene instead of at the title (§12). | K29 |
| `state <key> = <value>` | With `start` only: overrides one initial state value. The value is in `state.ron`'s form. | K29 |

**Static validation.** A header that names an unknown NPC, question, action, settings key, scene or state key fails the case before boot, with the rule ids of §10.2. So does a fixture path that does not exist, or a fault outside the list.

## 3. Steps (K9–K12)

A step is an **intent**, a **raw input**, a **clock move**, an **assertion** or (slice 2) a **frame** request. Intents and raw inputs reach the player only as real input messages written into the world: `ButtonInput` presses and releases, `KeyboardInput` messages for text entry, and the in-memory clipboard port for paste. No step changes runner, menu or save state directly (K9). Every input step is followed by the settle rule (§4.3).

### 3.1 Intents

| Step | Input it produces |
|---|---|
| `new-game` | Activates `title.new-game` (below). |
| `continue` | Activates `title.continue`. |
| `advance [n]` | Presses `Space` `n` times (default 1), settling after each. A line still revealing takes one press to finish revealing, exactly as for a player. `n` is at most 1000. A larger `n` is a `keiko.case.syntax` finding at parse time. *(Amended 2026-11-27: the limit.)* |
| `pick <n>` / `pick "<text>"` | Presses `Digit<n>` for the option with that 1-based index, or with that exact source text. A text that matches no option, or more than one, fails the step with `keiko.step.no-target`. |
| `menu` | Presses `Escape` (opens the overlay, or backs out of the current view). |
| `activate <control>` | Moves focus to the control along its planned route (below), then presses `Enter`. It fails with `keiko.step.no-target` if the control is not on the current surface or no route reaches it, and with `keiko.step.disabled` if it is disabled. *(Amended 2026-11-27: a planned route, not the arrows tried in a fixed order.)* |
| `save <1-3>` | `menu` if the overlay is closed, then activates `overlay.save-tab` and `overlay.slot.<n>`. A confirm that appears is left open for the case to answer. |
| `load <1-3\|autosave>` | The same with `overlay.load-tab`. |
| `auto` | Presses `KeyA`. |
| `skip` | Presses `Tab` (seen skip). |

**Planned routes (added 2026-11-27).** `activate` plans its route before it presses a key:

- **One walk function.** The menu module gains one pure function that maps a surface's walk context, a focus and a key to the focus that key moves to, or to no move. The context covers the view, the confirm, the legacy picker, the door, the Gallery's columns and the entry counts. The player's own menu input calls the same function for every focus move, so the plan and the player cannot disagree. The function holds no world access and changes no state. Moves that change something besides focus, such as a slider step or a still browse, are not focus moves and are never planned.
- **The walk keys** are `ArrowDown`, `ArrowRight`, `ArrowUp`, `ArrowLeft` and `Tab`, in that order.
- **The search.** Keiko searches breadth first from the current focus over the walk keys and takes the shortest key sequence to the target. Between two routes of the same length, it takes the one whose first differing key comes first in the order above. The search ends when no new focus is reachable.
- **The presses.** Keiko presses each key of the route as real input, settling after each (K9, §4.3). After each press, the focus must be the route's next stop. If it is not, the step fails with `keiko.step.no-target` and names the key and the focus the player reached.
- **No route.** When the search exhausts without reaching the target, the step fails with `keiko.step.no-target`, naming the starting focus and the number of stops searched. Nothing is pressed.
- **No fallback.** There is no fixed key order and no `Tab` fallback. `Tab` is planned only where the surface's walk uses it.

### 3.2 Control ids (K10)

A control is named by a stable role id, never by its displayed text. This table is the one owner of these ids. The UI plugin's control names and copy keys reuse them ([§16](#16-amendments)).

| Surface | Ids |
|---|---|
| Title | `title.continue`, `title.new-game`, `title.settings`, `title.gallery`, `title.quit` |
| Overlay | `overlay.save-tab`, `overlay.load-tab`, `overlay.slot.1`, `overlay.slot.2`, `overlay.slot.3`, `overlay.slot.autosave`, `overlay.resume`, `overlay.exit-to-title`, `overlay.settings`, `overlay.gallery`, `overlay.timeline`, `overlay.legacy` |
| Legacy picker | `legacy.target.1`, `legacy.target.2`, `legacy.target.3`, `legacy.cancel` |
| Settings | `settings.back`, `settings.master`, `settings.music`, `settings.sfx`, `settings.mute`, `settings.reset-audio`, `settings.text-speed`, `settings.auto`, `settings.fullscreen`, `settings.reset-play`, `settings.key`, `settings.copy-path`, `settings.clear-key` |
| Gallery | `gallery.card.<n>` (1-based, in entry order) |
| Timeline | `timeline.card.<n>` (1-based, in declared order) |
| Confirm | `confirm.yes`, `confirm.no` |

Each id maps to one `MenuFocus` value in [`app/src/menu/state.rs`](../app/src/menu/state.rs). The build adds the map beside that enum, with a test that every `MenuFocus` variant has exactly one id. A control added later gets an id in the same change (additive, §2.1). The play frame's quick row gets ids when variant E is built.

**The control list (added 2026-11-27).** Each surface has a **control list**: the controls it draws that its walk can focus, plus its pointer-only controls, in walk order. The observation's `controls` field (§5.1), `activate`, the focus check (§4.3) and the menu walk (§9.5) all read this one list.

- **The overlay's sub-views.** The Settings, Gallery and Timeline views opened from the overlay's door draw the overlay's top bar narrowed to Save and Load. Their lists therefore begin with `overlay.save-tab` and `overlay.load-tab`. The same views opened from the title's door draw no top bar, and their lists do not hold these ids.
- **Pointer-only controls.** `settings.back` and `legacy.cancel` are drawn, but no walk key focuses them. A player reaches them by a click or leaves with `Esc`. They stay on their surfaces' lists. `activate` on one fails with `keiko.step.no-target` (§3.1), so a case uses `click` or `menu` instead.
- **Empty lists.** The play surface, the still, the error screen, an empty Gallery and an empty Timeline list no control.

### 3.3 Raw input

| Step | Input |
|---|---|
| `press <key>` | Press in one frame, release in the next. |
| `hold <key>` / `release <key>` | A press or release that stays until the matching step. A key still held when a take ends is released. |
| `type "<text>"` | One `KeyboardInput` message per character, into whatever owns text entry. |
| `paste "<text>"` | Fills the in-memory clipboard port, then presses `Ctrl+V`. The system clipboard is never touched. |

`<key>` is a Bevy `KeyCode` variant name: `Enter`, `Space`, `Escape`, `Tab`, `ControlLeft`, `KeyA`, `Digit1`, `F3`, `ArrowUp`, `PageDown` and so on. Any other name fails with `keiko.case.syntax`.

### 3.4 The clock and fault steps (K4, K19)

| Step | Effect |
|---|---|
| `wait <duration>` | Advances the virtual clock by whole frames of exactly 1/60 s, rounding up. `<duration>` is a decimal number followed by `s` or `ms`. |
| `release-decision <npc>.<question>` | Releases a decision held by the `decision-hold` fault (§7.2). |
| `release-key-check` | Answers a key check held by `key-check hold` (§7.2). |
| `unblock <target>` | Removes a `write-blocked` obstruction (§7.2). |

A fault step for a fault the case did not declare fails with `keiko.case.reference`.

**Limits (added 2026-11-27).** A duration is malformed, and a case with one fails with `keiko.case.syntax` on its line, when it is too large for the runtime's `Duration`, so no value panics the parser. `wait` and the `decision-slow` fault (§7.2) take at most 60 s, the same as the live-answer limit (§6). A longer one is also a `keiko.case.syntax` finding at parse time, so `--check` catches it before any take runs. `wait 60s` parses.

### 3.5 Assertions (K11, K12)

An assertion compares against the current observation (§5). A failing assertion ends the take as failed with `keiko.expect`, and reports the expected value and the observed value. For `expect log` and `expect no-log` the observed value is the log line that answered the assertion, or the nearest one when none matched, rather than the park the take stopped at, which says nothing about what the log holds. *(Amended 2026-11-18.)*

| Step | Passes when |
|---|---|
| `expect surface <name>` | The current surface is `title`, `play`, `menu`, `settings`, `gallery`, `still`, `timeline`, `confirm` or `error`. |
| `expect monitor open\|closed` | The dev Monitor is open or closed. |
| `expect speaker "<text>"\|none` | The name tab shows exactly this text, or nothing. |
| `expect line "<text>"` / `expect line contains "<text>"` | The full say-line (not just the revealed part) is, or contains, the text. |
| `expect revealed` / `expect revealing` | The say-line has finished revealing, or has not. |
| `expect choices "<a>" "<b>" ...` | The options on screen are exactly these, in order. |
| `expect notice "<text>"` / `expect notice contains "<text>"` / `expect notice none` | The Notice shows this text, contains it, or is not shown. |
| `expect control <id> enabled\|disabled\|focused\|absent` | The control is in that condition on the current surface. |
| `expect control <id> label "<text>"` | The control shows exactly this label. |
| `expect control <id> rect <x> <y> <w> <h> [±<t>]` | The control's laid-out node has its top-left corner at (`x`, `y`) and the size `w`×`h`, each within `t` reference pixels. `t` defaults to `0.5`. The node is the one `click` targets (§13), at the 1280×720 reference size. A control with no visible node fails, and the observed value is `absent`. This step makes the take draw (§13). *(Added 2026-11-27.)* |
| `expect bg <id>\|none`, `expect sprite <npc> <pose>\|none`, `expect music <id>\|none` | The staging matches. |
| `expect auto on\|off`, `expect skip seen\|force\|off` | The player's auto or skip mode matches. |
| `expect decision pending\|held\|none` | A decision is waiting for its answer, has an answer held by the paused overlay, or neither. |
| `expect scene <id>`, `expect label <name>\|none` | The runner is in this scene, after this label. |
| `expect state <key> = <value>` | The state map holds this value. |
| `expect read <scene>:<line>` / `expect unread <scene>:<line>` | The read log does or does not hold this line. `<line>` is the 1-based source line of the say-line in its `.unmei` file. |
| `expect slot <1-3\|autosave> empty\|unreadable\|scene <id>` | The slot's status matches. |
| `expect log <level> [row <n>] contains "<text>"` | Since the take began, the take's failure lines hold a line at this level (`error`, `warn`, `info`), optionally from failure row `<n>` of the [failure inventory](runtime-hardening.md#runtime-failure-posture), containing the text. The failure lines are the ones the take's own player logs under `unmei::failure`, not the session log or the sandbox's `<pack>.log`. Every failure row logs at `error` or `warn`, so `info` is accepted but never matches. *(Amended 2026-11-19: the take's failure lines, not the session log, and `info` never matches.)* |
| `expect no-log <level> [row <n>]` | No such failure line since the take began. `expect no-log info` therefore always passes. *(Amended 2026-11-19.)* |

## 4. The take (K2, K4, K17)

A **take** executes one case from its first line to its last, in its own sandbox, in the same process.

### 4.1 The Keiko composition

[`app/src/composition.rs`](../app/src/composition.rs) gains `keiko_app`, a third constructor beside `shipping_app` and `schedule_app`. It uses `schedule_app`'s headless selection, so there is no window, renderer or winit, and adds:

- `TimeUpdateStrategy::ManualDuration`, set by the take before every frame (§4.3).
- `RuntimeConfig` pointing at the sandbox pack, with dev mode from the case and a resolver from §6.
- `ClipboardPort::Memory`, and a `KeyCheckChannel` that follows the `key-check` fault (§7.2), or always answers `Unreachable` when no fault is declared. No take ever reaches the network unless `--live` is given.
- A session log opened on the sandbox's `<pack>.log`. It holds the version header only, because a take installs no global log subscriber. Log assertions read the take app's own funnel recorder, which holds the same `unmei::failure` lines the player writes to its session log. *(Amended 2026-11-19: the take's `<pack>.log` never carried the player's lines.)*
- The Keiko asset reader (§4.2) as the default asset source.
- No audio device. Music and sound effects are observed through staging only.

`UnmeiPlugin` is added unchanged. The shipping player gains no fault hook (K19): every fault in §7.2 acts through the sandbox, the asset source, the resolver or the key check.

### 4.2 The sandbox (K17)

*Decided 2026-11-17: copy the text files, read the assets in place.*

- **Where.** A fresh root per invocation, `<os-temp>/unmei-keiko-<pid>-<nonce>/`, and inside it one folder per take, `take-<n>/`.
- **The sandbox pack.** `take-<n>/<pack-name>/` holds a copy of every file in the pack folder except `assets/` and `tests/`. Sidecars therefore land in `take-<n>/`, beside the sandbox pack, by the normal [sidecar rule](../engine/src/sidecar.rs).
- **Assets.** `RuntimeConfig` gains an `asset_root`, defaulting to `<pack_root>/assets`. `shipping_app`, `schedule_app` and the pack loader's asset checks in [`app/src/core/pack.rs`](../app/src/core/pack.rs) read it instead of joining `assets` themselves. A take sets it to the original pack's `assets/` folder, read-only. The Keiko asset reader wraps Bevy's file reader and applies the asset faults. It is installed only by `keiko_app`.
- **Credentials.** `take-<n>/config/` is the config dir passed to `credentials::resolve_launch`. `NANOGPT_API_KEY` and the endpoint override are never read, except by `--live` (§6).
- **Cleanup.** The take folder is deleted when the take ends, unless `--keep` is given, in which case its path is printed. The root is deleted at exit if it is empty.

### 4.3 Frames and settling (K4)

- **Frame step.** A frame that moves the clock moves it by exactly 1/60 s. Only `wait` moves the clock.
- **The settle rule.** After boot and after every input step, the take runs zero-duration frames until the world is **quiet**: no decision task in flight (unless a hold or slow fault holds it), every asset handle the presentation watches has loaded or failed, and two consecutive frames leave the observation unchanged. A press needs one frame for its release, which settling covers.
- **Boot.** A take first settles on the title, or for a `start` case at the first park (§12).
- **Not settling.** If 600 frames pass without becoming quiet, the take ends as failed with `keiko.unsettled`.
- **The focus check (added 2026-11-27).** After boot and after every step, once the world has settled, the take checks the menu's focus. The take fails with `keiko.focus.hidden` when the surface's control list (§3.2) is not empty and the focus names no control on that list. The finding names the focus and the surface. A surface with an empty list is not checked. On an empty Gallery or Timeline the player has no card focus, whatever card its focus value holds ([Timeline](title-and-menu.md#timeline)), so a card focus there counts as no focus and passes. *(Amended 2026-11-27, in the build: the overlay's empty Timeline lists Save and Load, so its list is not empty.)* The check runs in every take and every mode, so a case never asks for it.
- **Panics.** Each take runs under `catch_unwind`. A panic ends the take as failed with `keiko.panic` and its message, and the next take builds a fresh app.
- **The end.** A take passes when every step applied and every assertion passed.

## 5. The observation (K11–K13)

After each step the take builds an **observation** from the world. Building it reads app resources and the presentation's text nodes. It never exposes entities, components or system order (K13).

### 5.1 Fields

| Field | Content |
|---|---|
| `surface` | The §3.5 surface name. |
| `monitor` | `true` when the dev Monitor is open. |
| `clock` | Virtual time since boot, in milliseconds. |
| `scene`, `label`, `line` | The runner's scene id, the nearest preceding label (or `null`), and the 1-based source line of the current park (or `null`). |
| `speaker`, `text`, `revealed` | The name tab text, the full say-line, and whether the reveal has finished. |
| `choices` | `[{ "index": 1, "text": "..." }]`. |
| `notice` | The Notice text, or `null`. |
| `controls` | On menu surfaces: `[{ "id", "label", "enabled", "focused" }]`, the surface's control list (§3.2) in walk order. |
| `staging` | `{ "background", "sprites": { npc: pose }, "music" }`. |
| `auto`, `skip` | `true`/`false`, and `"seen"`, `"force"` or `"off"`. |
| `decision` | `null`, or `{ "question": "ace.greet", "status": "pending" \| "held" }`. |
| `error` | On the error screen: its messages, in order. |
| `state` | The state map. |
| `slots` | `[{ "slot": "1", "status": "empty" \| "unreadable" \| "ok", "scene" }]`. |
| `log` | The take's failure lines (§3.5) written since the previous observation: `[{ "level", "row", "text" }]`. Each is an `error` or `warn` line under `unmei::failure`, never a session log line. *(Amended 2026-11-19: not the session log.)* |

The read log is not printed in full. It is asserted with `expect read` and `expect unread`.

### 5.2 Text form

The text form prints the same fields, one group per line, omitting empty ones:

```text
play · 4.250s · first_meeting:14 (after converge)
Ace: "You're not from around here, are you?"  [revealed]
  1 "Ask about the letter"
  2 "Stay silent"
staging: bg school_gate · ace=neutral · music music_morning
state: affinity_level="low" letters_found=0 met_ace=true
```

## 6. Decisions (K15, K16)

*Decided 2026-11-17: in live mode, a landing set by the case wins.*

- **Offline (every take without `--live`).** The take installs a Keiko resolver through the [resolver seam](adr/0009-decision-resolver-seam.md). For a decision the case sets with `decide`, it answers with that landing key. For any other decision, it lands on the author's fallback and writes exactly the log line the player writes with no key (failure row 10), so a case observes the offline posture unchanged. No key and no network are needed.
- **Live (`--take --live`).** Decisions the case sets still use their landing. Every other decision goes to NanoGPT through the real `DecisionsClient`, with the key from `NANOGPT_API_KEY` only, never the credential file (K17). With no key in the environment, `--live` is a usage error. While a live answer is pending, the clock does not move and settling waits in real time, up to 60 s. The take's result is marked `live` and nondeterministic in text and JSON.
- **Faults** on decisions are in §7.2.

## 7. Fixtures and faults (K18, K19)

### 7.1 Fixtures

`fixture <kind> <path>` copies `tests/<path>` into the sandbox before boot. Without a fixture, a take starts with no sidecars and no credential (K18).

| Kind | Destination |
|---|---|
| `slot-1`, `slot-2`, `slot-3`, `autosave` | That save slot's sidecar path. |
| `legacy-save` | The legacy single-save path. |
| `read-log` | The read-log sidecar. |
| `settings` | The settings sidecar. `setting` headers are applied on top of it. |
| `credential` | `config/unmei/credentials`. Only synthetic keys belong in a fixture. |
| `env-key` | The file's first line becomes the take's environment-key override, as `NANOGPT_API_KEY` would. |

### 7.2 The fixed fault list

| Fault | Acts through | Effect |
|---|---|---|
| `write-blocked <target>` | Sandbox | Creates a directory at the target's temp path, so its atomic write fails. Targets: `slot-1`–`slot-3`, `autosave`, `read-log`, `settings`, `credential`. The `unblock` step (§3.4) removes it. |
| `asset-missing <path>` | Asset reader | Reading `assets/<path>` returns not-found. |
| `asset-corrupt <path>` | Asset reader | Reading `assets/<path>` returns bytes that cannot be decoded. |
| `pack-file-missing <path>` | Sandbox | Deletes the file from the sandbox pack. |
| `pack-file <path> <fixture>` | Sandbox | Replaces a sandbox pack file with `tests/<fixture>`, for a broken pack. |
| `credential-unreadable` | Sandbox | Makes the credential path unreadable (a directory where the file should be). |
| `decision-error <npc>.<question> transport\|answer\|refusal` | Resolver | The decision fails in that way and lands on its fallback with the matching diagnostic. |
| `decision-slow <npc>.<question> <duration>` | Resolver | The answer arrives once the virtual clock has moved by `<duration>` after the request. |
| `decision-hold <npc>.<question>` | Resolver | The answer is held until a `release-decision` step. |
| `key-check accepted\|rejected\|unreachable\|hold` | Key check | Every key check answers this way. `hold` answers only when a `release-key-check` step runs. |

The list is closed. A new fault is an additive change to this table (§2.1).

## 8. Stepping (K21, K22)

- **`--step ID STEP`** replays `tests/<ID>.keiko` from the start, applies `STEP`, and on success appends `STEP` to the file and prints the observation. If the file does not exist, it is created as `keiko 1` plus the step.
- **A header line as `STEP`** is accepted only while the case has no steps. Otherwise it is `keiko.case.header-order` and nothing is written.
- **Failure.** If the replay fails, or `STEP` fails (an assertion, a missing target), the file is left unchanged, the failure and the last observation are printed, and the exit code is 1. A stepped file therefore only ever holds steps that passed.
- **`--observe ID`** replays and prints the observation without changing the file.
- **Appending** keeps the file's existing line endings and adds a final newline.

Because takes are deterministic (§4.3, §6), each command lands exactly where the previous one left off. The stepped file is an ordinary case (K22).

## 9. Exploration (K23–K26)

*Decided 2026-11-17: the budget counts steps applied.*

### 9.1 The walk

- **Start.** A fresh take of the pack at `new-game`, or the end of case `--from ID`, inheriting its headers. Exploration adds `setting play.text_speed 0` (instant text) unless the case sets the text speed.
- **Moves.** At a say park, `advance`. At a choice, each option in index order. At a decision point, each landing key in ascending byte order, then `fallback`, each as a `decide` header on that branch. Depth first. *(Amended 2026-11-17: sorted, not declared, order. The engine holds a question's criteria in an unordered map, so declared order does not survive loading, and a sort keeps exploration deterministic.)*
- **Visited.** A park whose (scene, line, state map) was already reached is not expanded again (K23).
- **Replay.** Each branch is a fresh take replaying its own step list from the start, so findings reproduce from a plain case.
- **Budget.** `--budget N` caps the steps applied, replays included, across the whole walk. The default is 10,000. When the budget is spent, exploration stops and reports `budget spent` in the coverage summary. The same pack and budget always give the same result.

### 9.2 Findings (K24)

| Rule id | Finding |
|---|---|
| `keiko.panic` | A panic. |
| `keiko.notice` | A Notice was shown. |
| `keiko.defensive` | A defensive failure (failure posture `Defensive`). |
| `keiko.log.row-<n>` | A `warn` or `error` line from a failure row the pack causes: pack warnings, missing or undecodable assets, a missing decide point or decision label. |
| `keiko.stuck` | A park that is not the pack's end, where every move leaves the scene, line and wait unchanged. |
| `keiko.unsettled` | The settle rule did not reach quiet. |
| `keiko.focus.hidden` | The focus check failed (§4.3). *(Added 2026-11-27.)* |

Taking a fallback is never a finding (K24). Exploration offline never calls the model, so no transport or answer finding can occur.

### 9.3 Coverage (K25)

The summary lists, per scene, the say-lines and choices reached out of the total, and the scenes never entered. Coverage is information. It never fails the command.

### 9.4 Saved findings (K26)

Each finding is saved as `tests/findings/<rule>-<hash>.keiko`, where `<hash>` is the first 8 hex digits of the SHA-256 of its step list. A file that already exists is left alone, so a rerun adds nothing new. The case holds a comment with the finding's message, the branch's headers and steps, and a closing assertion that the finding does not occur (for example `expect notice none` or `expect no-log warn row 7`). The case therefore fails while the problem exists and passes once it is fixed, and the author keeps it as a regression test.

A finding with no closing assertion the case grammar can express is reported but not saved, because a case without one would pass while the problem exists (rule 29). This happens, for example, with a stuck park that has no say-line after it. *(Amended 2026-11-17.)*

A `keiko.focus.hidden` finding is saved with no closing assertion. The focus check (§4.3) runs in every take, so the saved case fails while the focus can go hidden and passes once it cannot. *(Amended 2026-11-27.)*

**One problem is one finding.** Exploration reports and saves a finding at most once per (rule, message) pair, keeping the first branch in walk order that hits it. A pack whose every branch replays the same failed asset load therefore yields one `keiko.log.row-<n>` finding, not one per branch, and a loop reached by two paths yields one `keiko.stuck` finding. The walk order and the budget accounting are unchanged. *(Amended 2026-11-18.)*

### 9.5 The menu walk (added 2026-11-27)

`--explore --menus` walks the menus instead of the story. It shares exploration's budget, replay, findings, saving and output. Only the moves, the visited rule, the findings it raises and the coverage differ.

- **Start.** A fresh take of the pack, settled on the title, or the end of case `--from ID`, inheriting its headers.
- **Stops.** A **stop** is the surface name, the focused control's id (or none) and the surface's control list (§3.2). A stop already reached is not expanded again. A slider step or any other change outside focus and the list therefore adds no stop.
- **Moves.** At each stop, the walk tries `press ArrowUp`, `press ArrowDown`, `press ArrowLeft`, `press ArrowRight`, `press Tab`, `press Enter` and `press Escape`, in that order. On the `play` surface, the only move is `press Escape`, because the story belongs to the story walk. The `error` surface has no moves. A move that exits the player ends its branch, and the exit is not a finding.
- **Order.** Breadth first, so a saved finding holds the shortest key sequence the walk found. Each branch is a fresh take replaying its own steps from the start, as in §9.1.
- **Unwinding.** From every stop on a surface other than `title` and `play`, the walk presses `Escape` up to four times. If none of those presses reaches `title` or `play`, the walk raises `keiko.unwind`. These presses count against the budget.
- **Findings.** The menu walk raises `keiko.panic`, `keiko.unsettled`, `keiko.focus.hidden` and `keiko.unwind`. Notices and log rows are the story walk's findings, so the menu walk does not raise them.
- **Saving.** `keiko.focus.hidden` is saved as §9.4 says. `keiko.unwind` has no closing assertion the case grammar can express, so it is reported but not saved (§9.4).
- **Coverage.** The summary lists, per surface, the controls the walk focused out of the controls listed, and names the ones never focused. As in §9.3, coverage never fails the command.

## 10. Output and exit codes (K27)

### 10.1 Text and JSON

Text is the default. With `--format json`, every mode prints one JSON document to stdout:

- `--take`: `{ "takes": [{ "case", "result": "pass" | "fail", "live", "failure": <finding> | null, "observation" }] }`.
- `--step` and `--observe`: `{ "result", "failure", "observation" }`.
- `--explore`: `{ "findings": [<finding> + "case": path], "coverage", "budget": { "limit", "used" } }`. An unsaved finding (§9.4) carries `"saved": false` in place of `case`, and the text form marks it `unsaved`. *(Amended 2026-11-17.)* With `--menus`, `coverage` is `{ "surfaces": [{ "surface", "focused", "listed", "never_focused": [id] }] }`. *(Amended 2026-11-27.)*

A finding has the shape of the check's findings ([plugin family C3](plugin-family.md#5-pack-wide-check-and-preview)): `{ "file", "path", "rule", "message" }`. For a case, `file` is the case path and `path` is `line <n>`.

### 10.2 Rule ids

`keiko.case.version`, `keiko.case.syntax`, `keiko.case.header-order`, `keiko.case.reference` (an unknown NPC, question, action, scene, state key, settings key, control id or fixture path, or a question id set for two NPCs, rule 38), `keiko.step.no-target`, `keiko.step.disabled`, `keiko.expect`, plus the §9.2 ids and `keiko.unwind` (§9.5). *(Amended 2026-11-27: `keiko.focus.hidden` and `keiko.unwind`.)*

### 10.3 Exit codes

| Code | Meaning |
|---|---|
| 0 | Every take passed; a step applied; exploration found nothing. |
| 1 | A take failed or a case was invalid; a step failed; exploration saved at least one finding. |
| 2 | Usage error. |

## 11. Check and packaging

*Decided 2026-11-17: the check validates cases, and the demo pack ships its tests.*

- **Validation lives in the engine.** `unmei-engine` gains a Bevy-free `keiko` module: the case parser and the static checks of §2.2 and §10.2, returning findings. Every Keiko mode runs it before boot. When the plugin family's `--check` is built, it runs the same function over every case in `tests/` and reports its findings as errors.
- **Packaging.** The release archive already ships every tracked file of the demo pack, so `demo-pack/tests/` ships as worked examples. Nothing changes in [`scripts/package_lib.py`](../scripts/package_lib.py). If the package tests pin the file list, the build adds the new files to it.

## 12. The shared replay (K29)

Keiko owns the one replay that reaches a line offline. The plugin family's preview uses it.

- **`start <scene-id>`** begins play at the scene's first park with the pack's initial state, plus any `state` header overrides, and empty staging. It reaches the runner through the same seam the Timeline uses to restart a scene, so it is a fixture-level start, not a step.
- **The preview's flags map onto a generated case.** `--scene <id>` and `--at <scene>:<line>` become `start <scene>` followed by `advance` and `pick` steps up to the target line. `--state <file>` is a RON map in `state.ron`'s form, and becomes `state` headers. `--pick <n>,<n>,...` lists the 1-based option to take at each successive choice. A choice past the end of the list takes option 1. Decisions take their fallback unless the preview later adds a `decide` form. This replaces the formats [plugin family C4 and C5](plugin-family.md#5-pack-wide-check-and-preview) left open.

## 13. Slice 2: frames and position clicks (K14, K9)

- **`frame [name]`** renders the current frame through the headless render path on the software rasterizer (wgpu's fallback adapter: WARP on Windows, llvmpipe on Linux), at the 1280×720 reference size. It writes `<pack-parent>/<pack-name>.keiko/frames/<case>/<n>[-<name>].png` and prints the path. It never compares the frame with anything (K14).
- **`click <control>`** moves the cursor to the centre of that control's laid-out node and presses and releases the left button, so the hit test is the player's own. **`click <x> <y>`** does the same at reference coordinates.
- **`expect control <id> rect`** (§3.5) reads the same laid-out node `click <control>` aims at: the topmost visible node carrying the control's action. It compares geometry only, so fonts, colours and anti-aliasing never enter it, and K14 holds. *(Added 2026-11-27.)*
- **The render path** is the one the preview's software rendering ([plugin family C7](plugin-family.md#5-pack-wide-check-and-preview)) reuses. A machine with no fallback adapter fails `frame`, `click` and `expect control <id> rect` with `keiko.render.unavailable`. It does not fail the rest of the case's steps before them. *(Amended 2026-11-27: the rect assertion.)*
- **CI.** The Linux `check` job installs the Mesa Vulkan driver for this slice.

**Where a frame lands and how it is numbered.** `<pack-parent>` is the folder that holds the real pack, so the frames sit beside it, never inside it, and outlive the take and its sandbox without `--keep`. In `<n>[-<name>].png`, `<n>` is the 1-based ordinal of the `frame` step in the case, so a rerun overwrites the same files. Every mode that runs a `frame` step writes its PNG, `--observe` and `--step` included, so rule 25's "never writes" means the case file. *(Amended 2026-11-18.)*

## 14. The gate and the docs (Done when)

- **Fixture packs.** The smoke checklist's recipes D, L, G and B become fixture packs under `app/tests/keiko/<name>/`, each with its own `tests/`. The S/K recipes (disposable sidecars and credential faults) run their cases over D's pack with their own fixtures, because every take already has its own sandbox. *(Amended 2026-11-18.)*
- **Menu states the demo pack cannot reach (slice 3, added 2026-11-27).** These packs and cases use only the grammar of §3:
  - **E** is a new fixture pack, `app/tests/keiko/e/`, that declares no Gallery entry and no Timeline order. Its cases open the empty Gallery from both doors and the empty Timeline. They check that `Enter` and every arrow leave the surface unchanged, and that `Esc` backs out.
  - **L** already declares a Timeline order. It gains a case that walks every card with `activate` and checks the walk's clamps at both ends.
  - **G** gains a case for the title's gallery door. The case opens the Gallery with `activate title.gallery`, activates every card, and presses each arrow from the first and last cards, checking the focus after each press.
  - Each pack also runs `--explore --menus` in the gate (below) and saves no finding.
- **Migrated cases.** Every smoke case that needs no real GPU, audio device or physical input becomes a case over a fixture pack. That means T1–T4, P1, S1–S3, K1–K4 (with `key-check` faults in place of network blocking), G2 without its visual check, R1–R3, L1–L2, M1, the input-ownership parts of M2, and B1. A1's held and slow answers become `decision-hold` and `decision-slow` cases.
- **The gate.** `app/tests/keiko_gate.rs` runs the built `unmei-play` (`CARGO_BIN_EXE_unmei-play`) with `--take` over the demo pack and each fixture pack, and with `--explore --budget 10000` over the demo pack, and fails on a non-zero exit. From slice 3 it also runs `--explore --menus --budget 10000` over the demo pack and over each fixture pack. *(Amended 2026-11-27.)* The `check` workflow's existing test step runs it, so `check.yml` needs no new job.
- **Docs.** The demo pack's `AGENT-GUIDE.md` gains a Keiko section and the Guide a Keiko page (K28). [`testing.md`](testing.md) names Keiko as the player-level end-to-end layer. The smoke checklist is retired, and its rendered, audio and physical-input checks become accepted unverified residuals (*amended 2026-11-17*, [K31](keiko.md#8-relation-to-other-work)). The [release runbook](release-runbook.md) gains the release run of §14.1. `GLOSSARY.md` gains the terms in [requirements §9](keiko.md#9-contracts-this-will-amend).

### 14.1 Release evidence (added 2026-11-17)

Keiko's takes are release evidence under [Keiko replaces the human walks](release-certification.md#keiko-replaces-the-human-walks-decided-2026-11-17). The release run uses the candidate's extracted `unmei-play` on each OS, not a source build: `--take` over the demo pack and over each fixture pack from the tagged source, and `--explore --budget 10000` over the demo pack, all with `--format json`. The release record keeps the JSON output. Any non-zero exit blocks the candidate.

## 15. Validation rules (the build's test list)

**Slice 1**

1. The launch grammar accepts each §1 form and rejects with exit 2: two modes together, `--live` without `--take`, `--budget` without `--explore`, `--dev` with a mode, `--step` without `STEP`, and a malformed `ID`.
2. A Keiko mode creates no window and never writes or reads the real pack's sidecars or the user's credential file. A test points `APPDATA`/`XDG_CONFIG_HOME` at a canary folder and asserts it is untouched.
3. Version line: `keiko 1` parses; a missing line, `keiko 2` and `keiko x` fail with `keiko.case.version`.
4. A header after a step fails with `keiko.case.header-order`. Each kind of unknown reference fails with `keiko.case.reference` before boot.
5. The same case run twice gives byte-identical JSON output (determinism).
6. `advance` on a revealing line finishes the reveal; a second `advance` moves on. With `setting play.text_speed 0`, one `advance` moves on.
7. `wait 2s` with auto on advances a say-line after its 2.0 s dwell and not before (`wait 1.98s` leaves it).
8. A Notice expires after its lifetime under `wait`, and not without it.
9. `pick "text"` with zero or two matching options fails with `keiko.step.no-target`. `activate` on a disabled control fails with `keiko.step.disabled`.
10. Every `MenuFocus` variant maps to exactly one control id, and the reverse.
11. An intent goes through input ownership: with the Monitor open (`dev`, `press F3`), `new-game` does not start play.
12. A held key at a load is quarantined until released, matching the player's rule.
13. `decide ace.greet cold` lands on `cold`. An unset decision lands on the fallback, with the player's categorical fallback log line and no Notice.
14. `decision-hold` keeps `expect decision pending` across waits, `menu` turns it `held`, and `release-decision` then `menu` lands it.
15. `decision-slow ace.greet 3s` lands only after `wait 3s`.
16. `decision-error ... refusal` lands on the fallback with the refusal diagnostic.
17. Every `key-check` verdict produces the Settings key row state the [Decisions settings](title-and-menu.md#decisions-settings) contract names.
18. `write-blocked slot-1` makes a save show the save-failure Notice and leaves the old slot's bytes unchanged. `unblock slot-1` then lets the save land.
19. `asset-missing` and `asset-corrupt` produce the asset rows' log lines and no Notice. The original pack's assets are never written.
20. `pack-file-missing pack.ron` reaches the error screen with `expect surface error`.
21. Fixtures land at their sidecar paths, and a take without fixtures starts with no slots, no read log and Continue disabled.
22. A take leaves no folder behind without `--keep`, and leaves exactly its take folder with `--keep`.
23. A system that panics (a test-only plugin in a test build) ends the take with `keiko.panic` and the next take passes.
24. A world that never becomes quiet ends with `keiko.unsettled` after 600 frames.
25. `--step` appends only a step that passed, creates a missing case with its version line, and leaves the file byte-identical on failure. `--observe` never writes the case file; a `frame` step still writes its PNG (§13). *(Amended 2026-11-18.)*
26. A case built by `--step` passes under `--take` unchanged.
27. Exploration of the demo pack reaches both choice options and every landing key of `ace.greet`, and saves no finding.
28. Exploration of a fixture pack with a missing sprite, an unreachable label and a scene that loops forever saves one `keiko.log.row-*` and one `keiko.stuck` finding, and lists the label as unreached coverage, not as a finding.
29. Each saved finding fails under `--take`, and passes once the fixture is repaired.
30. A rerun of exploration writes no new files.
31. The same budget gives the same findings and coverage. A smaller budget reports `budget spent`.
32. `--take --live` without `NANOGPT_API_KEY` exits 2. With a key (run only by the live test, like `app/tests/live_decision.rs`), a set landing is still used and the result is marked `live`.
33. `start first_meeting` with a `state` override settles at the scene's first park with that state.
34. The demo pack's cases and the fixture packs' cases pass under the gate.

**Slice 2**

35. `frame` writes a 1280×720 PNG at the documented path on the software rasterizer.
36. `click title.new-game` starts play through the hit test. `click` on a covered control does nothing, as for a player.
37. With no fallback adapter, `frame` fails with `keiko.render.unavailable`.

**Slice 1, added 2026-11-17** *(numbered after slice 2 so that earlier rule numbers stay stable)*

38. A case that sets `decide` or a decision fault for one question id on two different NPCs fails with `keiko.case.reference` before boot. The [resolver seam](adr/0009-decision-resolver-seam.md) identifies a decision point by its question id alone, so the take could not tell the two apart.

**Slice 3, added 2026-11-27**

39. Fixture pack E's cases pass: on the empty Gallery from both doors and on the empty Timeline, `Enter` and every arrow leave the observation unchanged, and `Esc` backs out to the door.
40. The menu's walk function (§3.1) gives the same focus as the player's input for every walk key, from every focus, on every surface the fixture packs reach. The player's menu input calls that function, and no other code moves the focus for a walk key.
41. `activate` reaches every control on every control list that the fixture packs' cases reach, except the pointer-only controls (§3.2). Each route is the shortest one, ties broken by the walk-key order, and Keiko presses no other key. For example, `activate overlay.timeline` from the slot grid's first card presses `Tab` three times, then `Enter`.
42. `activate settings.back`, `activate legacy.cancel` and `activate` on a control no route reaches each fail with `keiko.step.no-target`, naming the starting focus and the number of stops searched, and press nothing.
43. In a take that draws, every control on the control list of every surface the fixture packs reach has a visible laid-out node. The overlay's Settings, Gallery and Timeline lists hold `overlay.save-tab` and `overlay.load-tab`, and the title's door lists do not.
44. A test-only plugin that moves the focus onto a control the surface does not list (such as `overlay.resume` on the Gallery after `ArrowDown` from the last card) fails a take with `keiko.focus.hidden` at that step, in every mode.
45. Under the same plugin, `--explore --menus` raises only `keiko.focus.hidden` findings, one per distinct stop it reaches the defect from, and saves each one. Each saved case fails under `--take`, and passes once the plugin is removed. *(Amended 2026-11-27, in the build: the Gallery's two doors are two stops, so the plugin yields more than one finding.)*
46. A test-only plugin that makes `Esc` inert on the Settings view makes `--explore --menus` report only unsaved `keiko.unwind` findings, one of them for the Settings surface. *(Amended 2026-11-27, in the build: a confirm over Settings unwinds into the same inert view.)*
47. `--explore --menus` over the demo pack and over each fixture pack saves no finding, and a rerun gives byte-identical JSON. The same budget gives the same findings and coverage, and a smaller budget reports `budget spent`.
48. `--menus` without `--explore` exits 2.
49. `expect control title.new-game rect` with the node's own geometry passes. It fails with `keiko.expect` when any value is off by more than the tolerance, and reports the observed rect. An absent control reports `absent`. With no fallback adapter, it fails with `keiko.render.unavailable`.
50. A rect with a negative value, a tolerance without a number, or fewer than four numbers fails with `keiko.case.syntax`.
51. `wait 1e20s` and `fault decision-slow ace.greet 1e20s` fail with `keiko.case.syntax` on their line and do not panic.
52. `wait 61s`, `fault decision-slow ace.greet 61s` and `advance 1001` each fail with `keiko.case.syntax` on their line. `wait 60s` and `advance 1000` parse.

## 16. Amendments

Each of these was amended in place on 2026-11-17, with a dated pointer to the section here:

- [Plugin family C4 and C5](plugin-family.md#5-pack-wide-check-and-preview): the preview's replay and its `--pick` and `--state` formats are Keiko's (§12).
- [Plugin family §8](plugin-family.md#8-contracts-this-will-amend): `--check` validates `.keiko` files (§11).
- [Plugin readiness §4](plugin-readiness.md#4-suggested-spec-order): Keiko is specified first.
- [UI plugins R25](ui-plugins.md#8-versioning): control names reuse the Keiko control ids (§3.2).
- [Pack layout](content-format.md#2-pack-layout): the optional `tests/` folder.
- [Crate boundaries](architecture.md#crate-boundaries-and-invariants): the engine's `keiko` module and `unmei-play`'s Keiko modes.
- [Testing policy](testing.md) and the [smoke checklist](testing-human-smoke.md): the pointer to the layer that will take over the automatable cases. The text itself changes with the build (§14).
- **2026-11-18:** §3.2's control table gains `overlay.exit-to-title` (the rail control was mouse-only), §3.5's failing `log`/`no-log` assertion reports the log line it found, §9.4 reports one problem once instead of once per branch, and §14's fixture-pack set is D, L, G and B with the S/K cases over D's pack.
- **2026-11-18 (slice 2):** §13 numbers a frame by its step's ordinal, writes a PNG in every mode that runs the step and puts the frames beside the real pack, and rule 25's `--observe` sentence now means the case file.
- **2026-11-19:** §4.1's take session log holds the version header only, because a take installs no global log subscriber. §3.5's `log`/`no-log` assertions and §5's `log` field read the take's failure lines (`unmei::failure`, `error` and `warn` only), not the session log, so `expect log info` never matches and `expect no-log info` always passes. The grammar and the case version are unchanged.
- **2026-11-27 (slice 3, issue #179):** The maintainer decided all five of the issue's proposals, built in the order fixture packs, planned routes, menu walk, focus check, layout assertions. The decisions were layout assertions without golden images, so K14 holds; one rule id, `keiko.focus.hidden`, for the walk and the always-on check; and the menu walk as a mode of `--explore`. The changes:
  - §1 gains `--menus`.
  - §3.1's `activate` plans its route over the menu's one walk function.
  - §3.2 defines the control list, adds the sub-views' top-bar ids and names the pointer-only controls.
  - §3.5 and §13 gain `expect control <id> rect`.
  - §4.3 gains the focus check.
  - §9.2 and §10.2 gain `keiko.focus.hidden`, and §10.2 gains `keiko.unwind`.
  - §9.4 saves `keiko.focus.hidden` without a closing assertion.
  - §9.5 is new.
  - §14 gains pack E, the new G and L cases, and the gate's menu walk.
  - §15 gains rules 39–50.

  Every change is additive, so the case version stays 1.
- **2026-11-27 (slice 3 build):** §4.3 counts a card focus on an empty Gallery or Timeline as no focus, and rules 45 and 46 expect one finding per distinct stop instead of exactly one. The [requirements](keiko.md) K11, K23, K24 and K25 are amended in place, and the [glossary](../GLOSSARY.md) entries for Exploration and Finding name the menu walk.
- **2026-11-27 (issues #184, #194):** A duration too large for `Duration` is a `keiko.case.syntax` finding instead of a panic. The maintainer decided that `wait` and `decision-slow` take at most 60 s, that `advance` takes a count of at most 1000, and that going over a limit is a parse-time `keiko.case.syntax` finding. §3.1 and §3.4 state the limits, and §15 gains rules 51 and 52. The grammar is unchanged, so the case version stays 1.

The requirements' list also names `check.yml`. It needs no change, because the gate is an integration test (§14).

## 17. Out of scope

Everything in [requirements §11](keiko.md#11-out-of-scope). Also: choosing a language for a case, which the language-pack spec adds as an optional `language <tag>` header without a version bump (*decided 2026-11-17*); a `decide` form for the preview, and running takes in parallel. *(Amended 2026-11-17: Keiko results as release evidence moved into scope, §14.1.)*
