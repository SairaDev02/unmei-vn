---
title: Troubleshooting a failed launch or load
summary: Fix a player with no window on Linux, Windows blocking the unsigned player, a pack or save slot that will not load, find the session log and report a bug.
keywords:
  - failure-posture
  - diagnostics
  - external-dependency
---
# Troubleshooting a failed launch or load

This page covers four failures: no window on Linux, Windows blocking the unsigned player, a pack that
does not load and a save slot that does not load. It also says where the session log is and what to
attach to a bug report. A pack is the folder of content the player plays, such as the demo's
`demo-pack/`.

What the player does for each failure is
[`docs/runtime-hardening.md` § Runtime failure posture](../runtime-hardening.md#runtime-failure-posture).
Depending on the failure, it shows the Notice (a short message in a corner of the screen) or the
error screen, or only writes the log.

## Linux — the player starts but no window appears

The player renders through **Vulkan** and has no OpenGL fallback, so a machine without a working
Vulkan driver gets no window. Two other requirements stop the player before a window opens: a minimum
glibc version, and shared libraries that a minimal install can miss. To see which one is missing, run
`./unmei-play` from a terminal in the folder you extracted.

The `README` inside the archive lists the glibc version, the shared libraries and the Vulkan driver
for your release, with the one-line install command that supplies what a minimal Debian or Ubuntu
system is missing:
[`packaging/player-README.md` § Linux requirements](../../packaging/player-README.md#linux-requirements).

## Windows — the player will not start

The Windows executables are not code-signed, so Windows treats them as untrusted. Two screens can
stop the player:

- **Windows protected your PC** is the SmartScreen prompt. Choose **More info**, then **Run anyway**.
- **Smart App Control** on Windows 11 can refuse an unsigned program outright, with no prompt and no
  **Run anyway** option. [Smart App Control](#smart-app-control) has the facts.

If neither screen appears and the player does not start, open PowerShell in the folder you extracted,
run `.\unmei-play` and read what it prints. The archive `README` covers both screens:
[`packaging/player-README.md` § Start](../../packaging/player-README.md#start).

### Smart App Control

Smart App Control is a Windows security feature that blocks apps it cannot verify, and its rules come
from Windows, not from Unmei. Microsoft's
[Smart App Control FAQ](https://support.microsoft.com/en-us/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions)
states them, and this section applies them to Unmei:

- While Smart App Control is on, the 0.1.0 player and the authoring CLI cannot start, because both are
  unsigned.
- Windows offers no way to let a single app through.
- Smart App Control checks every app on the machine, so turning it off lowers that protection for all
  of them. The only way to run the 0.1.0 player and the authoring CLI is to turn Smart App Control off
  in the Windows Security app settings. Microsoft's
  [Smart App Control overview](https://learn.microsoft.com/en-us/windows/apps/develop/smart-app-control/overview)
  shows its state under **Settings** → **Windows Security** → **App and Browser Control**.
- The FAQ says that Windows updates allow Smart App Control to be turned back on without a clean
  installation of Windows. It names no update or version.

This page does not recommend either choice: turning Smart App Control off to run Unmei, or leaving it
on and not running Unmei.

## A pack will not load — the error screen

When a pack does not load, the player shows an error screen in its window instead of the game. The
screen holds the pack path, then up to 20 lines of `file:line: message` from the loader. If there are
more than 20 errors, a last line points to the session log for the rest. `Esc`, `Enter`, `Space` or a
click quits.

There is no pack picker and no retry. Fix the file that the first `file:line: message` line names,
then launch again. The session log lists every error ([Where the log is](#where-the-log-is)). If you
wrote the pack, the author CLI prints the same errors and points at the same files
([`install-the-cli.md`](install-the-cli.md)). The rules behind the screen are
[`docs/runtime-hardening.md` § Runtime failure posture, 4. The startup error screen](../runtime-hardening.md#4-the-startup-error-screen).

## A save will not load

An older version refuses a save slot that a newer version wrote. The menu overlay, which **Esc** opens, lists
such a slot with the reason and does not delete it. To load the slot:

1. Extract the newer release into its own folder.
2. Copy your `demo-pack.*` files into that folder, beside `demo-pack/`. The archive `README` names the
   files:
   [`packaging/player-README.md` § Saves and upgrading](../../packaging/player-README.md#saves-and-upgrading).
3. Load the slot there from the menu overlay
   ([`play-the-demo.md` § Load](play-the-demo.md#load)).

The version rule, and the other reasons a slot is unreadable, are
[`docs/runtime-hardening.md` § Save slots, 3. Version policy](../runtime-hardening.md#3-version-policy).

## Where the log is

The session log sits beside the pack folder and is named after it: `demo-pack.log` for the demo, or
`my-pack.log` for a pack in `my-pack/`. It is in the folder that holds `demo-pack/`. The player
writes its save slots, settings file and read log there too, with the same naming
([`docs/runtime-hardening.md` § Save slots, 1. Files on disk](../runtime-hardening.md#1-files-on-disk),
[§ Runtime failure posture, 6. Logging and `<pack-dir>.log`](../runtime-hardening.md#6-logging-and-pack-dirlog)).

Each launch truncates the log, so it holds one session only, and launching again erases the record of
the earlier session. Reproduce the problem, then copy the file before the next launch.

If there is no log at all, the player could not create it: the folder that holds `demo-pack/` is
not writable by your account, or the disk is full. If you started the player from a terminal, it
printed one warning there and carried on. Free disk space, or extract the archive into a folder your account can
write to.

## Filing a useful report

**Never attach anything from `%APPDATA%\unmei\`, `~/.config/unmei/` or `$XDG_CONFIG_HOME/unmei/`.**
That directory holds the credential file with your OpenRouter API key, and a bug report is public.
Attach the session log instead. The credential file and its path are
[`docs/title-and-menu.md` § Decisions settings, 1. File and format](../title-and-menu.md#1-file-and-format).

Report bugs at https://github.com/SairaDev02/unmei-vn/issues. Attach or quote:

- the session log from beside your pack folder ([Where the log is](#where-the-log-is));
- which archive you downloaded, and what `unmei-play --version` prints;
- what you were doing when it went wrong, and what you expected instead.

Paste the version exactly as printed, because it says which version wrote the log. In PowerShell:

```powershell
.\unmei-play --version
```

On Linux:

```console
$ ./unmei-play --version
unmei-play 0.1.0
```

Both print `unmei-play 0.1.0` for the 0.1.0 release. The reporting steps that ship with each release
are in
[`packaging/player-README.md` § Reporting a problem](../../packaging/player-README.md#reporting-a-problem).

## Next

- [`play-the-demo.md`](play-the-demo.md): the archive, the first launch and the menu overlay.
- [`live-decisions.md`](live-decisions.md): the key, its storage and the offline fallback.

Guide home: [`README.md`](README.md), with the routes for players, authors and contributors.
