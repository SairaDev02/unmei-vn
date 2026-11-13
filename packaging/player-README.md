# Unmei @@VERSION@@ (beta): demo player

This page gets the demo running from the extracted player archive. Unmei is a visual-novel engine
where an NPC's (non-player character's) reaction can be decided while you play instead of being
written in advance. This folder holds the player, `unmei-play`, and a three-scene demo,
`demo-pack/`.

> **Beta.** The pack, save and settings formats can change in any 0.x release. An older release
> refuses a save slot written in a newer save format
> ([`docs/release-certification.md` § Compatibility during 0.x](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/release-certification.md#compatibility-during-0x-decided-2026-11-10)).

## Start

You need Windows x64 or Linux x64. There is no macOS player archive and no installer.

1. Extract the whole archive into its own folder where your user account can write, such as Desktop
   or Documents. The folder must be outside the archive and not under `Program Files`. The player
   writes your saves, settings and session log into this folder, beside `demo-pack/`.
2. On Linux, install what [Linux requirements](#linux-requirements) lists. On Windows, skip this
   step.
3. Start the player from that folder:
   - **Windows:** double-click `unmei-play.exe`. The beta is not code-signed. If Windows shows
     **Windows protected your PC**, choose **More info**, then **Run anyway**. Windows 11's
     **Smart App Control**, where it is on, can block an unsigned app outright instead of showing
     that screen. If it does, the player does not start. The Guide's
     [Smart App Control section](https://github.com/SairaDev02/unmei-vn/blob/v0.1.0/docs/guide/troubleshooting.md#smart-app-control) covers what
     Windows offers. On Windows, a console window also opens beside the game. Closing that window
     closes the game.
   - **Linux:** run `./unmei-play` from a terminal opened in that folder.

   The demo opens on the title screen.
4. On the title screen, choose **New Game** to start from the first scene. Choose **Continue** to
   resume the game you have played since starting the player. If you have not played yet since
   starting the player, **Continue** loads the autosave (the save slot that the game writes itself).
   **Continue** is disabled while no autosave exists, as on a first launch
   ([`docs/title-and-menu.md` § Title](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/title-and-menu.md#title)).

The demo plays fully **offline** when no key is stored and `OPENROUTER_API_KEY` is unset or empty in
the environment the player launches from. Each decision point, where an NPC's reaction is decided,
then takes the pack's authored fallback. Ace, the demo's NPC, reacts as the pack author wrote.
When a key is stored or the variable is set, each decision point sends one request, which
OpenRouter bills to your account
([`docs/title-and-menu.md` § Decisions settings](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/title-and-menu.md#decisions-settings)).

The demo ends at the `— end of scene —` marker. The marker is the end of the demo, not an error. To
leave the game, follow the quit steps under [Saves and upgrading](#saves-and-upgrading)
([`docs/runtime-hardening.md` § Runtime failure posture](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/runtime-hardening.md#runtime-failure-posture),
row 22).

## Linux requirements

On Linux, the player needs:

- **glibc 2.39 or newer** (the system C library), for example Ubuntu 24.04 and later.
- **A working Vulkan driver, with `libvulkan1`.** The player renders through Vulkan and has no
  OpenGL fallback, so a machine without a working Vulkan driver gets no window.
- These shared libraries: `libasound2` (ALSA),
  `libudev1`, `libfontconfig1`, `libxkbcommon0`, `libxkbcommon-x11-0`, `libX11` and
  `libwayland-client`.

On Debian or Ubuntu, this command installs what a minimal system is missing:

```sh
sudo apt install libasound2 libudev1 libfontconfig1 libwayland-client0 libvulkan1 mesa-vulkan-drivers libxkbcommon-x11-0
```

The `mesa-vulkan-drivers` package covers Mesa graphics drivers only. On a machine with an NVIDIA
GPU, install NVIDIA's driver instead.

## Controls

| Keys | Action |
|---|---|
| Space, Enter or left-click | Advance the line |
| 1–9, or click a choice | Pick a choice |
| Tab | Skip lines you have already read |
| Hold Ctrl | Skip every line, read or not |
| A | Auto mode on or off |
| F11 | Fullscreen |
| Esc | Menu overlay: Save, Load, Timeline, Gallery, Settings |

The keys in this table are fixed by
[`docs/runtime-hardening.md` § Player comfort](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/runtime-hardening.md#player-comfort),
[`docs/runtime-hardening.md` § Text skip](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/runtime-hardening.md#text-skip) and
[`docs/runtime-hardening.md` § Menu overlay](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/runtime-hardening.md#menu-overlay).

The menu overlay works with the mouse or the keyboard. These keys work in the overlay:

- `↑`, `↓`, `←` and `→` move focus across the 2×2 grid.
- `Tab` cycles the top-bar controls.
- `Enter` or `Space` activates the focused control.
- `Esc` backs out.
- `1`, `2` and `3` focus a manual slot.

## Live decisions (optional)

To have a language model decide Ace's reactions live, you need an
[OpenRouter](https://openrouter.ai) API key. Each decision is one request. OpenRouter bills it to
your account at the price of the decision model that the demo pack names for Ace. To set the key,
press **Esc**, choose **Settings**, go to the **Decisions** group and paste the key. The Guide's
[Paste the key in Settings](https://github.com/SairaDev02/unmei-vn/blob/v0.1.0/docs/guide/live-decisions.md#paste-the-key-in-settings) lists each
step.

The key is stored for your user account only, never in this folder, so the folder can be shared
without carrying the key. It is stored at:

- Windows: `%APPDATA%\unmei\credentials`
- Linux: `~/.config/unmei/credentials`, or `$XDG_CONFIG_HOME/unmei/credentials` when that variable
  is set.

A non-empty `OPENROUTER_API_KEY` environment variable overrides the stored key
([`docs/title-and-menu.md` § Decisions settings](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/title-and-menu.md#decisions-settings)).

## Saves and upgrading

The player writes every file for a pack beside the pack folder, named after the pack. For a pack in
`my-pack/`, those are `my-pack.log` (the session log), `my-pack.settings.ron` (settings),
`my-pack.read.ron` (the read log, which records the lines you have read) and its save slots. The
demo's files sit beside the `demo-pack/` folder:

| File | Holds |
|---|---|
| `demo-pack.slot1.save.ron` to `demo-pack.slot3.save.ron` | The three manual save slots |
| `demo-pack.autosave.save.ron` | The autosave |
| `demo-pack.settings.ron` | Settings |
| `demo-pack.read.ron` | The read log |
| `demo-pack.log` | The session log |

These names are fixed by
[`docs/runtime-hardening.md` § Save slots](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/runtime-hardening.md#save-slots),
[`docs/runtime-hardening.md` § Audio settings](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/runtime-hardening.md#audio-settings),
[`docs/runtime-hardening.md` § Read log](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/runtime-hardening.md#read-log) and
[`docs/runtime-hardening.md` § Runtime failure posture](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/runtime-hardening.md#runtime-failure-posture).

After a relaunch, **Continue** loads the autosave, not one of your manual slots. To load a manual
slot:

1. On the title screen, choose **Continue**, or **New Game** when Continue is disabled.
2. Press **Esc**.
3. Choose **Load**, then choose the slot. The menu overlay asks once to confirm
   ([`docs/runtime-hardening.md` § Menu overlay](https://github.com/SairaDev02/unmei-vn/blob/v@@VERSION@@/docs/runtime-hardening.md#menu-overlay)).

Quitting discards play since the last save. Your slots and the last autosave stay. To quit, do one
of these:

- On the title screen, choose **Quit**.
- From play, press **Esc**, choose **Settings**, then choose **Exit to title**. That takes you to the
  title screen, where you choose **Quit**.

Closing the window also quits.

To move to a newer release:

1. Extract the newer release into a **new** folder.
2. Copy the `demo-pack.*` files from the old folder into the new folder, beside `demo-pack/`.
3. Keep the old folder until the new release works. If the newer release changes the save format,
   the older release cannot read the save slots that the newer release wrote.

## Reporting a problem

A bug report is public. Do not attach anything from `%APPDATA%\unmei\`, `~/.config/unmei/` or
`$XDG_CONFIG_HOME/unmei/`, because that is where your API key lives. Attach only the session log.

Report bugs at https://github.com/SairaDev02/unmei-vn/issues and include:

- the session log from beside your pack folder (`demo-pack.log` for the demo);
- which archive you downloaded;
- what you were doing;
- what `unmei-play --version` prints. To print it, open a terminal in the folder and run
  `.\unmei-play --version` in PowerShell or `./unmei-play --version` on Linux.

## Licenses

Unmei is MIT-licensed. The text is in `LICENSE`. Third-party code notices are in
`THIRD-PARTY-LICENSES`. The bundled fonts are under the SIL Open Font License:
`IBMPlexSans-license.txt`, `FiraMono-OFL.txt` and `FiraMono-LICENSE`.
