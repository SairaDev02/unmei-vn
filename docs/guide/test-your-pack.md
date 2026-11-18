---
title: Test your pack with Keiko
summary: Write a case under tests/, run every case with one command, build one step at a time, then explore every path and keep each finding as a regression case.
keywords:
  - verification
  - content-pack
  - player-input
  - decisions
  - failure-posture
  - pack-validation
---
# Test your pack with Keiko

Keiko plays your pack the way a player does, in a window that never opens, and checks what that
player sees. This page takes you from an empty `tests/` folder to a case that runs with one command,
then through stepping and exploration. [`docs/keiko-spec.md`](../keiko-spec.md) owns the case format,
the commands and every rule on this page; when the two disagree, the spec is right.

## Before you start

- A pack folder. This page calls it `my-pack`, the copy of the demo pack that
  [`write-your-first-pack.md`](write-your-first-pack.md) makes.
- The player archive for your platform, extracted as in
  [`play-the-demo.md` § 2](play-the-demo.md#2-extract-it-into-its-own-folder). Keiko is a set of flags
  on `unmei-play`, and the archive ships the demo pack with a worked case at
  `demo-pack/tests/opening.keiko`. You need no Rust toolchain and no API key.
- A plain-text editor.

Where a command on this page shows `unmei-play`, type `.\unmei-play` in PowerShell or `./unmei-play`
in `sh`, in the folder that holds the executable. A Keiko mode opens no window, and it reads and
writes none of your saves, settings, read log, session log or API key: every take runs on a throwaway
copy of the pack ([`docs/keiko-spec.md` § 4.2](../keiko-spec.md#42-the-sandbox-k17)).

## 1. Write a case

A case is one file in the pack's `tests/` folder, named `<id>.keiko`, where the file stem is the case
id and one file holds one case. The player ignores `tests/` during play, so the folder ships with the
pack and changes nothing about it. This is the opening of the demo pack's own case:

```text
keiko 1
# The demo's opening, played offline: the choice, the reunion decision's authored
# fallback, and the epilogue it lands.
decide ace.greet fallback
setting play.text_speed 0

new-game
expect surface play
expect scene first_meeting
expect speaker "Narrator"
expect revealed
advance
expect speaker "Ace"
expect line "You're not from around here, are you?"
```

- **Line 1 is the version line.** `keiko 1` is the only version. A missing or unknown version fails
  the case with `keiko.case.version`, and nothing else in the file is read.
- **Comments start with `#`.** A line whose first non-blank character is `#` is a comment, and blank
  lines are ignored.
- **Headers come before the first step.** A header sets one decision landing (`decide`), one settings
  value (`setting`), one starting file (`fixture`), one fault (`fault`), dev mode (`dev`), or a start
  at a scene instead of the title (`start`, with optional `state` overrides). A header after a step
  fails with `keiko.case.header-order`,
  [`docs/keiko-spec.md` § 2.2](../keiko-spec.md#22-header-lines) lists every header.
- **Steps follow, one per line.** Tokens are separated by spaces, and a quoted string uses `"`, with
  `\"` and `\\` as its only escapes. A case is checked against the pack before the player starts, so
  an unknown NPC, question, action, scene, state key, settings key or control id fails before boot.

## 2. Run your cases

1. Run every case in the pack. With no id, `--take` runs every case under `tests/`, saved findings
   included, in byte order of their relative paths:

   ```powershell
   .\unmei-play my-pack --take
   ```

   ```sh
   ./unmei-play my-pack --take
   ```

2. Read each result. A passing case prints `PASS <id>`, its last surface and the observation it
   finished on. This is the demo pack's case, played offline:

   ```console
   $ ./unmei-play demo-pack --take
   PASS opening (play)
   play · 0.000s · epilogue:17
     Narrator: "She is already walking away."  [revealed]
     staging: bg school_gate · ace=neutral · music music_evening
     state: affinity_level="low" letters_found=0 met_ace=true
     slots: autosave=ok:epilogue
   ```

   A failing case prints `FAIL <id>`, a rule id, the line and the value it expected and the one it
   observed. The rule ids are in
   [`docs/keiko-spec.md` § 10.2](../keiko-spec.md#102-rule-ids).

3. Run one case by naming its id after `--take`, for example `--take opening`.

4. Read the exit code when a script runs the cases. The three codes are the same for every Keiko
   mode ([`docs/keiko-spec.md` § 10.3](../keiko-spec.md#103-exit-codes)):

   | Code | Meaning |
   |---|---|
   | 0 | Every take passed. |
   | 1 | A take failed or a case was invalid. |
   | 2 | The command line was wrong, such as two modes in one command. |

5. For a script or a report, add `--format json`. Every mode then prints one JSON document to stdout.
   A take document holds one entry per case with its `result`, its `failure` or `null`, and its
   `observation` ([`docs/keiko-spec.md` § 10.1](../keiko-spec.md#101-text-and-json)). The same case
   run twice gives byte-identical output.

6. When a failure depends on the throwaway copy a take ran on, add `--keep`: the take folder is kept
   and its path is printed, so you can read the sandbox's sidecars.

## 3. The steps and assertions you write

Each step does one thing. These are the four kinds a case uses most:

| Kind | Steps |
|---|---|
| Player intent | `new-game`, `continue`, `advance [n]`, `pick <n>`, `pick "<text>"`, `menu`, `activate <control>`, `save <n>`, `load <n>`, `load autosave`, `auto`, `skip` |
| Raw input | `press <key>`, `hold <key>`, `release <key>`, `type "<text>"`, `paste "<text>"` |
| Clock | `wait <duration>`, a decimal number with `s` or `ms`, such as `wait 2s` |
| Fault control | `release-decision <npc>.<question>`, `release-key-check`, `unblock <target>` |
| Rendering | `frame [name]`, `click <control>`, `click <x> <y>` |

An intent reaches the player as real input messages, exactly as a key press would, so input ownership
and the held-key rules hold inside a take. `activate` names a control by its role id, such as
`title.new-game`, `overlay.slot.2` or `confirm.yes`, never by its displayed text.
[`docs/keiko-spec.md` § 3](../keiko-spec.md#3-steps-k9k12) lists every step with its exact input, and
[§ 3.2](../keiko-spec.md#32-control-ids-k10) lists every control id.

A case can also look at the screen and click it. `frame [name]` writes the current frame as a
1280×720 PNG in the take's own copy of the pack, beside the pack under
`<pack-dir>.keiko/frames/<case>/`, and prints the path; add `--keep` to read it after the run.
`click <control>` presses a control by its role id, at the point the control is drawn, and
`click <x> <y>` presses a point in the same 1280×720 frame. Both go through the player's own hit
test, so a control that another surface covers takes no click, exactly as for a player. Both need a
software rasterizer, WARP on Windows and Mesa's llvmpipe on Linux. A machine without one fails the
step with `keiko.render.unavailable` and still runs the steps before it
([`docs/keiko-spec.md` § 13](../keiko-spec.md#13-slice-2-frames-and-position-clicks-k14-k9)).

An assertion starts with `expect` and compares one part of what the player sees or stores: the
surface; the Monitor; the speaker, the full say-line, whether the reveal finished or the choice
options; the Notice; one control's condition or label; the staging of background, sprites and music;
auto and skip mode; a pending decision; the scene and label; the state map; the read log; a save slot;
or a session-log line. `expect no-log` asserts that a log line did not appear. A failing assertion
ends the take with `keiko.expect` and prints the expected and the observed value.
[`docs/keiko-spec.md` § 3.5](../keiko-spec.md#35-assertions-k11-k12) has the table of forms.

Text matching is exact and case-sensitive. Use `expect line contains "<text>"` and
`expect notice contains "<text>"` when a line holds a state value you also assert.

## 4. Decisions and the offline posture

A take never calls the model and needs no key. For each decision the case cares about, write a
`decide` header with one of the question's actions, or with `fallback`. A decision the case leaves
unset lands on the author's fallback and writes the same log line the player writes with no key, so a
case observes the offline posture a player gets ([`docs/keiko-spec.md` § 6](../keiko-spec.md#6-decisions-k15-k16)).
That fallback is never a finding.

`--take --live` sends the decisions a case leaves unset to NanoGPT. It needs `NANOGPT_API_KEY` in the
environment, and it marks each result live and nondeterministic, so a live take is evidence that a
model answered and not a regression test. Use it to check a prompt change, not in the suite you keep.

## 5. Fixtures and faults

A take starts with no save slots, no read log, no settings and no credential, so Continue is disabled
unless the case supplies one. `fixture <kind> <path>` copies a file from the pack's `tests/` folder
into the place the player reads before boot. The kinds are `slot-1`, `slot-2`, `slot-3`, `autosave`,
`legacy-save`, `read-log`, `settings`, `credential` and `env-key`
([`docs/keiko-spec.md` § 7.1](../keiko-spec.md#71-fixtures)). Only synthetic keys belong in a fixture,
and the key in an `env-key` fixture stands in for the environment variable.

`fault <name> [args]` declares a failure from a fixed list: an obstructed sidecar write, a missing or
undecodable asset, a deleted or replaced pack file, an unreadable credential file, a decision that
fails, is slow or is held, or a key check with a fixed verdict. The list is closed, and a fault
outside it fails the case before boot. A held decision or key check is released with the matching
fault-control step. [`docs/keiko-spec.md` § 7.2](../keiko-spec.md#72-the-fixed-fault-list) lists every
fault, its argument and the effect it has.

## 6. Build a case one step at a time

Two commands build a case without editing it by hand, and both replay it from the first line, so each
one lands where the last one stopped.

1. Print what the player sees. `--observe` replays a case and prints the observation. It never writes
   the case file, and a `frame` step in the case still writes its PNG (§ 13):

   ```sh
   ./unmei-play my-pack --observe opening
   ```

2. Apply one step and keep it. `--step` replays the case, applies the step and appends it to the file
   only when it passed, then prints the observation
   ([`docs/keiko-spec.md` § 8](../keiko-spec.md#8-stepping-k21-k22)):

   ```sh
   ./unmei-play my-pack --step opening 'expect notice none'
   ```

   A step that fails leaves the file unchanged and exits 1, so a stepped case only holds steps that
   passed. A case id that names no file is created as `keiko 1` plus the step, and a header is
   accepted as a step only while the case has none.

The result is an ordinary case: `--take` runs it unchanged.

## 7. Explore every path

`--explore` walks the pack by itself. From New Game it takes every choice option and every landing
key of every decision point, depth first, replaying each branch as its own take. A park it already
reached is not expanded again.

```sh
./unmei-play my-pack --explore --budget 10000
```

- `--budget N` caps the steps applied, replays included. The default is 10000, and the same pack and
  budget always give the same result.
- `--from <id>` starts at the end of an existing case and inherits its headers.
- The text output lists each finding, then the coverage summary. This is the demo pack:

  ```console
  $ ./unmei-play demo-pack --explore
  reached 3 of 3 scenes
  budget: 94 of 10000
  ```

- A finding is a panic, a Notice, a defensive failure, a warning or error log row the pack causes, a
  park no input can leave, or a world that never settles. Taking a fallback is never a finding.
- Each finding is saved as `tests/findings/<rule>-<hash>.keiko`, holding that branch's headers and
  steps and a closing assertion that the problem does not occur. The saved case fails while the
  problem exists and passes once you fix it, so keep it as a regression case and run it with `--take`.
  A rerun of exploration adds nothing new ([`docs/keiko-spec.md` § 9.4](../keiko-spec.md#94-saved-findings-k26)).
- Coverage is information, never a failure. It names, per scene, the say-lines and choices reached
  and the scenes never entered, and a spent budget ends the walk with `budget spent`.
- `--explore` exits 1 when it saved at least one finding, so a script can gate on it.

## Next

- [`write-your-first-pack.md`](write-your-first-pack.md): the pack this page tests.
- [`format-overview.md`](format-overview.md): where each file a case asserts on fits.
- [`docs/keiko-spec.md`](../keiko-spec.md): the case format, the steps and the fault list.

Guide home: [`README.md`](README.md), with the routes for players, authors and contributors.
