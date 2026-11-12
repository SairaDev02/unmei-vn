---
title: Play the Unmei demo
summary: Choose the player archive for your platform, extract it into its own folder, launch the demo, then save and load from the menu overlay.
keywords:
  - save-slots
  - persistence
  - player-input
---
# Play the Unmei demo

This page takes you from the download to a running demo: choose the player archive for your platform,
extract it, launch it, then save, load and quit. There is no installer. One archive holds
`unmei-play`, the three-scene demo pack and the texts that ship with them.

## Before you start

- A Windows x64 or Linux x64 machine. There is no macOS player archive.
- A folder that your own account can write to.
- On Linux, a working Vulkan driver and a few system libraries. The archive `README` lists them for
  your release, with a one-line install command:
  [`packaging/player-README.md` § Linux requirements](../../packaging/player-README.md#linux-requirements).

You need no Rust toolchain and no API key.

## 1. Choose the archive

Download the player archive for your platform from the [releases](https://github.com/SairaDev02/unmei-vn/releases) page.
The version and platform are in the file name:

| Platform | Player archive |
|---|---|
| Windows x64 | `unmei-play-0.1.0-x86_64-windows.zip` |
| Linux x64 | `unmei-play-0.1.0-x86_64-linux.tar.gz` |

The version in a file name is the release you download. This page shows 0.1.0. Each release carries four archives: a player archive and
a CLI archive for each of those two platforms. The CLI archives are for authors: see
[`install-the-cli.md`](install-the-cli.md).

## 2. Extract it into its own folder

The archive holds a single folder named after itself, `unmei-play-0.1.0-x86_64-windows` on Windows.
Inside it are the executable, `demo-pack/`, `README`, `LICENSE`, `THIRD-PARTY-LICENSES` and the
bundled font licenses.

Extract that folder to a location that your own account can write to, such as Desktop, Documents or
a folder you make for it. The location must be outside the archive and outside `Program Files`.
The reason: the player writes your saves, settings, read log and session log into that folder, beside
`demo-pack/`. With one folder per version, you can keep the version you are playing while you try the
next one.

## 3. Launch the demo

Start the player from the extracted folder:

- **Windows:** double-click `unmei-play.exe`. The Windows executables are not code-signed, so Windows may show
  **Windows protected your PC** first. Choose **More info**, then **Run anyway**.
  [`troubleshooting.md`](troubleshooting.md) covers that screen and the case where the player does
  not start at all. The player also opens a console window beside the game. Closing that window
  closes the game.
- **Linux:** run `./unmei-play`.

The demo opens on the title screen, with the first scene's opening frame frozen behind it. Two
buttons start play ([`docs/title-and-menu.md` § Title](../title-and-menu.md#title)):

- **Continue** resumes your live session, the play you entered since launching the player. Without a
  live session, it loads the autosave, the one save the game writes for you. With no autosave yet,
  it is disabled and says why.
- **New Game** starts from the first scene. It asks first only when a live session is in progress.

Optional: to check which release you extracted, run the player with `--version` from a terminal in
that folder. In PowerShell:

```powershell
.\unmei-play --version
```

On Linux:

```sh
./unmei-play --version
```

Both print `unmei-play` and the release version, `unmei-play 0.1.0` for the 0.1.0 release.

The demo needs no API key. It plays fully offline when no key is stored and `OPENROUTER_API_KEY` is
unset or empty in the environment you launch from: every decision point then takes the fallback the
pack author wrote. With a stored key, or with the variable set, each decision point sends one request
that OpenRouter bills to your account
([`docs/title-and-menu.md` § Decisions settings](../title-and-menu.md#decisions-settings);
[`live-decisions.md` § What a live decision sends](live-decisions.md#what-a-live-decision-sends);
[`live-decisions.md` § The environment override](live-decisions.md#the-environment-override)).

## 4. Save and load in the app

**Esc** opens the menu overlay over a frozen frame. The overlay has a **Save / Load** toggle and four
cards: three slots that you write yourself, and the autosave, which only the game writes.
**Settings**, **Gallery** and **Timeline** are the overlay's other views. The overlay works from the mouse
or the keyboard, and its keys are in
[`docs/runtime-hardening.md` § Menu overlay § 6](../runtime-hardening.md#6-interaction-model). A
gamepad and key rebinding are not supported.

### Save

1. Press **Esc**.
2. Choose **Save** in the **Save / Load** toggle.
3. Choose one of the three slot cards. An empty slot is written at once. An occupied slot asks once
   before it is overwritten.

The game writes the autosave itself: at a scene crossing, when a choice you make resolves and when a
model answers a decision point. A decision that takes its fallback does not write it, and neither do
loading a save or quitting
([`docs/runtime-hardening.md` § Save slots § 4](../runtime-hardening.md#4-slot-semantics-and-autosave)).

### Load

1. Press **Esc**.
2. Choose **Load** in the **Save / Load** toggle.
3. Choose a slot card. Load asks once, then picks up from where the slot was written.

A slot that this version cannot read stays listed, with the reason, and is never deleted.

The title has no Load button. After a relaunch, **Continue** on the title loads the autosave, not one
of your own slots. To load a slot of your own, enter play first with **Continue**, or with **New
Game** when Continue is disabled, then follow the steps in [Load](#load).

For the cards, their states, what each confirmation means and what a load restores, see
[`docs/runtime-hardening.md` § Menu overlay](../runtime-hardening.md#menu-overlay). For the slot files
beside the pack, see
[`docs/runtime-hardening.md` § Save slots § 1](../runtime-hardening.md#1-files-on-disk). For the rule
that refuses a save written by a newer version, see
[`docs/runtime-hardening.md` § Save slots § 3](../runtime-hardening.md#3-version-policy).

## 5. Quit the demo

The demo ends with the `— end of scene —` marker. That marker is the finish, not an error
([`docs/runtime-hardening.md` § Runtime failure posture](../runtime-hardening.md#runtime-failure-posture),
row 22). Quitting drops the play since your last save. Your slots and the last autosave stay on disk
([`docs/title-and-menu.md` § Title](../title-and-menu.md#title)). To leave the game, use one of
these:

- On the title, choose **Quit**. It asks you to confirm when a live session is in progress.
- From play, go back to the title first: press **Esc**, choose **Settings**, choose **Exit to title**
  and confirm.
- Close the window.

## 6. Everything else

The `README` file inside the archive has the rest offline: the controls table, the Linux
requirements, moving your saves to a newer release and how to report a problem. The specs own the
controls: [`docs/runtime-hardening.md` § Player comfort](../runtime-hardening.md#player-comfort) and
[§ Text skip](../runtime-hardening.md#text-skip). [`troubleshooting.md`](troubleshooting.md) covers
what to do when something fails.

## Next

- [`live-decisions.md`](live-decisions.md): let a model decide how Ace, the demo's NPC, greets you.
- [`write-your-first-pack.md`](write-your-first-pack.md): point the player at a pack of your own.

Guide home: [`README.md`](README.md), with the routes for players, authors and contributors.
