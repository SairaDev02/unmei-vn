---
title: Write your first Unmei pack
summary: Copy the demo pack, change a scene line, add a choice with its label, validate the whole pack with the CLI, then play it.
keywords:
  - content-pack
  - dialogue-dsl
  - pack-validation
---
# Write your first Unmei pack

This page takes you from a copy of the demo pack to a changed pack that passes validation and plays in the player. You rewrite one line, add a choice option with its label, validate the pack with the CLI and play it.

## Before you start

- The CLI archive, extracted as in [`install-the-cli.md`](install-the-cli.md). Its folder holds `unmei` and `demo-pack/`, the pack you copy in step 1.
- A terminal open in that folder. Where a command shows `unmei`, type `.\unmei` in PowerShell or `./unmei` in `sh`.
- Any plain-text editor.
- For step 6, the player archive, extracted as in [`play-the-demo.md`](play-the-demo.md).

Both archives exist for Windows x64 and Linux x64 only. macOS has neither
([`install-the-cli.md` § Two ways to get `unmei`](install-the-cli.md#two-ways-to-get-unmei);
[`play-the-demo.md` § 1 Choose the archive](play-the-demo.md#1-choose-the-archive)). With the
archives you need no Rust toolchain. Do the steps in order.

## 1. Copy the demo pack

Copy the `demo-pack` folder to `my-pack`, and keep every file name inside it unchanged. `my-pack` must not exist yet. Otherwise the command copies into it and creates `my-pack/demo-pack`.

```powershell
Copy-Item -Recurse demo-pack my-pack
```

```sh
cp -r demo-pack my-pack
```

`my-pack` is now a pack in the shape of
[`docs/content-format.md` § 2 Pack layout](../content-format.md#2-pack-layout):
`pack.ron`, `state.ron`, one file per scene, one file per NPC and `assets/`.
[`format-overview.md`](format-overview.md) maps each part.

## 2. Change a line

Open `my-pack/scenes/first_meeting.unmei` in your plain-text editor. Its opening narration is a say-line, a line of the shape `Speaker: text`
([`GLOSSARY.md` § Scripts](../../GLOSSARY.md#scripts)):

```unmei
Narrator: Morning light. A folded letter on the ground, half under a shoe.
```

Replace the text after `Narrator:`, keep the `Speaker: text` shape and save the file as UTF-8, because a scene is plain UTF-8 text
([`docs/content-format.md` § 2 Pack layout](../content-format.md#2-pack-layout)). To match the output in step 5, use this line:

```unmei
Narrator: Cold morning light. A folded letter lies on the gravel.
```

The other kinds of `.unmei` line are in
[`docs/content-format.md` § 3 Dialogue DSL](../content-format.md#3-dialogue-dsl-unmei).

## 3. Add a choice option

The scene already has a `choice` block with two options, each of the form `"<text>" -> <target>`. Add a third option between the two, so the block reads:

```unmei
choice
  "Ask about the letter" -> ask_letter
  "Hand the letter back" -> offer_letter
  "Stay silent" -> silence
end
```

An option's target is a label, a named position in the scene. It resolves exactly like a `jump` target: a label in this scene first, then a scene file.
[`docs/content-format.md` § 5 Transitions](../content-format.md#5-transitions) is the rule for both.

## 4. Add the label for the new option

Add the label that the new option points at, immediately before `label silence`, and leave a blank line after it, as the other labels have:

```unmei
label offer_letter
set met_ace = true
set letters_found = 1
set affinity_level = "high"
play sfx_paper_rustle
Ace: You picked it up? ...Thank you.
jump converge
```

Every key you `set` must already be declared in `state.ron`, with the kind (string, number or bool) you assign. `met_ace`, `letters_found` and `affinity_level` are the demo's own keys, already declared in its `state.ron`
([`docs/content-format.md` § 6 Manifest and state](../content-format.md#6-manifest-and-state)).

## 5. Validate the pack

To validate the whole pack, list the runs of `first_meeting` with `--list`. A run is a group of consecutive say-lines ([`GLOSSARY.md` § Scripts](../../GLOSSARY.md#scripts)). `--list` loads and checks the whole pack before it prints the runs. It is offline: no key, no network, no writes.

```console
$ unmei author draft first_meeting --list --pack my-pack
  start       L7     Narrator: Cold morning light. A folded letter lies on the gravel.
  start#2     L10    Ace: You're not from around here, are you?
  ask_letter  L22    Ace: A letter? Ah... you saw that.
  offer_letter  L31    Ace: You picked it up? ...Thank you.
  silence     L37    Ace: ...Fine, don't talk. See if I care.
```

Each line shows a run's handle, its line span (`L7`, or `L7-L10` for a run over several lines) and its first say-line. The new `offer_letter` run is listed, so the whole pack loaded. A clean `--list` prints only the runs and exits with code 0, and no separate line says the pack loaded. Each run is a unit you can draft prose into with the model. [`author-with-the-cli.md`](author-with-the-cli.md) drafts prose into a run, and
[`docs/author-cli-drafting.md` § 1 Command surface](../author-cli-drafting.md#1-command-surface) owns
the `--list` and `--from` surface.

When the pack fails to load, `--list` prints a `✖` line and any further errors, each naming its file, and its line where it has one. It writes nothing and exits with code 1
([`docs/author-cli.md` § 5 Failure, retry, and editing UX](../author-cli.md#5-failure-retry-and-editing-ux)).

Optionally, see a failure. In the `ask_letter` block, misspell `letters_found` as `letter_found`, save the file and run the same command again. After step 3, that `set` line is line 20:

```console
$ unmei author draft first_meeting --list --pack my-pack
✖ pack failed to load: scenes/first_meeting.unmei:20: `set letter_found` but `letter_found` is not declared in state.ron
```

Change `letter_found` back to `letters_found` before step 6.

## 6. Play it

The player takes the pack folder as its one argument. With no argument it runs the `demo-pack` beside the player executable, so the path is what selects your pack
([`docs/runtime-hardening.md` § Dev mode](../runtime-hardening.md#dev-mode), under Launch grammar).

This step can send data to OpenRouter and cost money. If you stored a key in the player's Settings, or if `OPENROUTER_API_KEY` is set to a non-empty value in the terminal you launch from, the demo's decision point calls the model on your account
([`live-decisions.md` § Paste the key in Settings](live-decisions.md#paste-the-key-in-settings);
[`live-decisions.md` § The environment override](live-decisions.md#the-environment-override)).
The decision point is the `decide` line in the `reunion` scene
([`GLOSSARY.md` § Decisions](../../GLOSSARY.md#decisions)). With neither a stored key nor the variable, playing needs no API key, and the decision point takes its fallback
([`live-decisions.md` § With no key](live-decisions.md#with-no-key)).

To play your pack, open a terminal in the player's folder and pass the path to `my-pack`: the folder you copied in step 1, inside the CLI folder you extracted. Replace the example path in the command with the full path to your `my-pack`.

```powershell
.\unmei-play C:\Users\you\Downloads\unmei-cli\my-pack
```

```sh
./unmei-play /home/you/Downloads/unmei-cli/my-pack
```

The title screen opens. Choose **New Game** to start `first_meeting`, the entry scene, with your changes: it opens on your new narration line, and its choice lists three options.

## Next

- [`format-overview.md`](format-overview.md): the map of a pack, part by part.
- [`author-with-the-cli.md`](author-with-the-cli.md): generate decision configs and draft prose.
- [`install-the-cli.md`](install-the-cli.md): the key and model the CLI needs.

Guide home: [`README.md`](README.md), with the routes for players, authors and contributors.
