#!/usr/bin/env python3
"""Play the extracted Linux player end to end under Xvfb.

    python3 scripts/linux_walk.py <extracted-archive-dir> [options]

The mechanism is the one the first automated walk proved: Xvfb with no window
manager and `WAYLAND_DISPLAY` unset, `xdotool search --sync --name '^unmei$'`,
one `xdotool windowfocus --sync <wid>` (XTEST alone delivered nothing because
focus stayed PointerRoot), and XTEST keys held about 0.4 s so a `just_pressed`
press spans a software-rendered frame; `--openbox` is its tested fallback.
Assertions read the session log, a sidecar or an exit code, never a screenshot;
a missed wait prints its step, both bounds and the log tail.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
import tempfile
from pathlib import Path

# The seam to the display, the processes and the disk; tests replace it.
from linux_walk_runner import WAIT_SECONDS, WINDOW_NAME, Runner

JOB_SECONDS = 600.0
POLL_SECONDS = 0.1
SETTLE_SECONDS = 0.5
DEFAULT_DISPLAY = ":99"
PACK_NAME = "demo-pack"
SCENES = ("first_meeting", "reunion", "epilogue")

# `Exit to title` is the rail's fourth child: 240 px rail, 20 px padding, 22/14/16
# px text above it (`app/src/presentation/overlay/chrome.rs`). `MenuFocus` has no
# stop for it, so keys cannot reach it — this click is the only way back.
EXIT_TO_TITLE_POINT = (120, 167)

# `unmei author draft first_meeting --list`, byte for byte (`cli/tests/draft_test.rs`).
CLI_LIST_EXPECTED = (
    "  start       L7     Narrator: Morning light. A folded letter on the ground,"
    " half under a shoe.\n"
    "  start#2     L10    Ace: You're not from around here, are you?\n"
    "  ask_letter  L21    Ace: A letter? Ah... you saw that.\n"
    "  silence     L28    Ace: ...Fine, don't talk. See if I care.\n"
)


class StepFailure(Exception):
    def __init__(self, step, detail, tail=()):
        super().__init__(f"{step}: {detail}")
        self.step = step
        self.detail = detail
        self.tail = list(tail)


def visited_scene_ids(text):
    match = re.search(r"visited:\s*\[(.*?)\]", text or "", re.S)
    if not match:
        return set()
    return {item.strip().strip('"') for item in match.group(1).split(",") if item.strip()}


def read_lines(text, scene_id):
    match = re.search(r'"%s"\s*:\s*\[([^\]]*)\]' % re.escape(scene_id), text or "")
    if not match:
        return set()
    return {int(item) for item in re.findall(r"\d+", match.group(1))}


def autosave_scene(text):
    match = re.search(r'scene_id:\s*"([^"]*)"', text or "")
    return match.group(1) if match else None


def failure_rows(text):
    return [line for line in (text or "").splitlines() if "unmei::failure" in line]


def is_allowed_failure(line):
    """Rows 7 and 10 only: no audio device, and no Decisions configuration."""
    body = line.split("unmei::failure", 1)[-1].lstrip(": ")
    return body.startswith("couldn't start audio") or "no Decisions client" in body


class Walk:
    def __init__(self, runner, extracted, work_dir=None,
                 exit_point=EXIT_TO_TITLE_POINT, openbox=False, keep=False):
        self.runner = runner
        # Absolute, because every launch runs from `work_dir`, not from here.
        self.extracted = Path(extracted).absolute()
        self.own_work = work_dir is None
        self.work_dir = Path(work_dir) if work_dir else Path(tempfile.mkdtemp(
            prefix="unmei-walk-work-"))
        self.sandbox = Path(tempfile.mkdtemp(prefix="unmei-walk-sandbox-"))
        # The player's stdout and stderr, every launch appended, for a failure's tail.
        self.output_path = self.sandbox / "player-output.txt"
        self.exit_point = exit_point
        self.openbox = openbox
        self.keep = keep
        self.player = self._find("unmei-play")
        self.cli = self._find("unmei")
        self.pack = self.player.parent / PACK_NAME if self.player else None
        self.env = None
        self.window = None
        self.handle = None
        self.saved_slot = None
        self.completed = []
        self.previous_log = ""
        self.current = "setup"
        self.job_deadline = 0.0

    def _find(self, name):
        matches = sorted(path for path in self.extracted.rglob(name) if path.is_file())
        return matches[0] if matches else None

    def sidecar(self, suffix):
        return self.pack.parent / f"{self.pack.name}.{suffix}"

    @property
    def log_path(self): return self.sidecar("log")

    @property
    def autosave_path(self): return self.sidecar("autosave.save.ron")

    @property
    def read_path(self): return self.sidecar("read.ron")

    @property
    def slot1_path(self): return self.sidecar("slot1.save.ron")

    # The walk's eight steps as fifteen named assertions; `-` maps to `_`.
    STEPS = (
        "title-ready", "new-game", "scene-first_meeting", "choice-1",
        "scene-reunion", "scene-epilogue", "autosave-epilogue", "read-log",
        "fallback-warning", "save-slot1", "quit", "relaunch-continue",
        "load-slot1", "quit-after-load", "cli-list",
    )

    def run(self):
        if self.player is None or self.cli is None:
            raise StepFailure(
                "setup", f"no `unmei-play`/`unmei` executable under {self.extracted}")
        self.job_deadline = self.runner.clock() + JOB_SECONDS
        self.runner.make_dirs(self.work_dir)
        self.runner.make_dirs(self.sandbox / "home")
        self.runner.make_dirs(self.sandbox / "config")
        self.env = self.runner.sandbox_env(
            self.sandbox / "home", self.sandbox / "config", DEFAULT_DISPLAY)
        try:
            for name in self.STEPS:
                self.current = name
                self.check_job_bound()
                getattr(self, name.replace("-", "_"))()
                self.assert_allowed_failures()
                self.completed.append(name)
        finally:
            self.teardown()
        return self.completed

    def teardown(self):
        for handle in (self.handle, self.runner.window_manager, self.runner.display):
            self.runner.stop(handle)
        if self.keep:
            return
        self.runner.remove_tree(self.sandbox)
        if self.own_work:
            self.runner.remove_tree(self.work_dir)

    def log_text(self):
        return self.runner.read_text(self.log_path) or ""

    def fail(self, detail):
        lines = [line for line in self.log_text().splitlines() if line.strip()][-12:]
        # The session log cannot hold a crash before logging starts, such as
        # winit failing to open the display; the player's own output can.
        output = [line for line in (self.runner.read_text(self.output_path) or "").splitlines()
                  if line.strip()][-12:]
        if output:
            lines += ["-- player output --", *output]
        raise StepFailure(self.current, detail, lines)

    def bounds(self):
        return f"each wait is {WAIT_SECONDS:.0f} s, the job {JOB_SECONDS / 60:.0f} min"

    def check_job_bound(self):
        if self.runner.clock() > self.job_deadline:
            self.fail(f"the job bound ran out ({self.bounds()})")

    def poll(self, check, described, excerpt=None):
        """Poll `check` inside the step's 30 s and the job's 10 min bounds."""
        deadline = self.runner.clock() + WAIT_SECONDS
        while True:
            value = check()
            if value:
                return value
            self.check_job_bound()
            if self.runner.clock() > deadline:
                detail = f"timed out after {WAIT_SECONDS:.0f} s waiting for {described}"
                self.fail(detail if excerpt is None else f"{detail} — {excerpt()}")
            self.runner.sleep(POLL_SECONDS)

    def wait_log(self, pattern, described):
        self.poll(lambda: re.search(pattern, self.log_text(), re.M), described)

    def wait_sidecar(self, path, check, described):
        def matched():
            text = self.runner.read_text(path)
            return text if text is not None and check(text) else None

        def excerpt():
            text = (self.runner.read_text(path) or "<missing>").strip()
            return f"{Path(path).name}: {text.replace(chr(10), ' ')[:240]}"

        self.poll(matched, described, excerpt)

    def wait_exit(self, expected=0):
        code = self.poll(self.exit_status, "player exit")[0]
        if code != expected:
            self.fail(f"the player exited {code}, expected {expected}")

    def exit_status(self):
        code = self.handle.poll()
        return None if code is None else (code,)

    def act(self, call):
        try:
            return call()
        except OSError as error:
            self.fail(str(error))

    def press(self, key):
        self.act(lambda: self.runner.hold_key(self.env, key))

    def click(self, point):
        self.act(lambda: self.runner.click(self.env, self.window, point))

    def launch_player(self):
        # Kept so the next launch can prove the log it reads is its own: the log
        # is truncated per launch, so a previous session's text cannot be new.
        self.previous_log = self.log_text() if self.handle else ""
        self.handle = self.runner.spawn([self.player], cwd=self.work_dir, env=self.env,
                                        output=self.output_path)

    def await_title(self):
        window = self.runner.search_window(self.env)
        if window is None:
            self.fail(f"timed out after {WAIT_SECONDS:.0f} s waiting for a window "
                      f"named {WINDOW_NAME}")
        self.window = window
        self.act(lambda: self.runner.focus_window(self.env, window))
        def fresh():
            text = self.log_text()
            if text == self.previous_log:
                return None
            return text if re.search(r"^.*unmei: title ready$", text, re.M) else None

        self.poll(fresh, "`unmei: title ready`")
        self.runner.sleep(SETTLE_SECONDS)
        banner = self.log_text().strip().splitlines()[0] if self.log_text().strip() else ""
        if not banner.startswith("unmei-play "):
            self.fail(f"the session log starts {banner!r}, not the version banner")

    def assert_allowed_failures(self):
        for line in failure_rows(self.log_text()):
            if not is_allowed_failure(line):
                self.fail(f"an unpermitted `unmei::failure` row: {line.strip()}")

    def title_ready(self):
        self.act(lambda: self.runner.start_display(DEFAULT_DISPLAY))
        # An early rehearsal launched before Xvfb was up and no window appeared;
        # the first walk slept 2 s here. Polling the server is the same wait.
        self.poll(lambda: self.runner.display_ready(self.env),
                  f"the X display {DEFAULT_DISPLAY} to answer")
        if self.openbox:  # the prototype's tested fallback, never the default
            self.act(lambda: setattr(
                self.runner, "window_manager", self.runner.spawn(["openbox"], env=self.env)))
        self.launch_player()
        self.await_title()

    def new_game(self):
        self.press("Return")
        self.wait_log(r"^.*unmei: entered scene first_meeting$",
                      "`unmei: entered scene first_meeting`")

    def scene_first_meeting(self):
        self.wait_sidecar(self.read_path,
                          lambda text: "first_meeting" in visited_scene_ids(text),
                          "a visited `first_meeting`")

    def choice_1(self):
        for _ in range(3):
            self.press("Return")  # two say-lines, then an inert press at the choice
        self.press("1")
        self.press("Return")  # the branch say-line, whose read is flushed at the crossing
        self.wait_sidecar(
            self.read_path, lambda text: 21 in read_lines(text, "first_meeting"),
            "the `ask_letter` say-line (21) read — what choosing 1 reaches")

    def scene_reunion(self):
        self.wait_log(r"^.*unmei: entered scene reunion$", "`entered scene reunion`")

    def scene_epilogue(self):
        for _ in range(3):
            self.press("Return")  # two say-lines, then an inert press
        self.wait_log(r"^.*unmei: entered scene epilogue$",
                      "`unmei: entered scene epilogue`")

    def autosave_epilogue(self):
        self.wait_sidecar(self.autosave_path,
                          lambda text: autosave_scene(text) == "epilogue",
                          'an autosave named by scene_id "epilogue"')

    def read_log(self):
        self.wait_sidecar(self.read_path,
                          lambda text: set(SCENES) <= visited_scene_ids(text),
                          f"all three scenes visited ({', '.join(SCENES)})")

    def fallback_warning(self):
        self.wait_log(r"^.*unmei::failure.*no Decisions client",
                      "decision-fallback warning")

    def save_slot1(self):
        self.press("Escape")  # the overlay opens on the grid, save mode, slot 1
        self.press("1")
        self.press("Return")  # save-over-empty is immediate: no confirmation
        name = re.escape(self.slot1_path.name)
        self.wait_log(rf"^.*unmei: saved to .*{name}$",
                      f"`saved to {self.slot1_path.name}`")
        self.saved_slot = self.runner.sha256(self.slot1_path)
        if self.saved_slot is None:
            self.fail(f"{self.slot1_path.name} was not written")

    def quit_to_title(self, overlay_open):
        if overlay_open:
            self.press("Escape")  # close the overlay (Resume)
        self.press("Escape")      # reopen it fresh: grid, save mode, slot 1 focused
        for _ in range(5):
            self.press("Tab")     # Save, Load, Timeline, Gallery, Settings
        self.press("Return")      # open the Settings view
        self.click(self.exit_point)   # the rail's click-only `Exit to title`
        self.press("Return")      # its confirmation, Yes focused
        for _ in range(4):
            self.press("Down")    # Continue, New Game, Settings, Gallery, Quit
        self.press("Return")      # Quit, which asks while a session is live
        self.press("Return")      # Yes

    def quit(self):
        self.quit_to_title(overlay_open=True)
        self.wait_exit(0)

    def relaunch_continue(self):
        self.launch_player()
        self.await_title()
        self.press("Return")  # Continue is the first row while the autosave loads
        self.wait_log(r"^.*unmei: entered scene epilogue$",
                      "`unmei: entered scene epilogue` from Continue")

    def load_slot1(self):
        self.press("Escape")  # the overlay, save mode
        self.press("Tab")     # Save
        self.press("Tab")     # Load
        self.press("Return")  # switch the grid to load mode
        self.press("1")
        self.press("Return")  # Load asks once
        self.press("Return")  # Yes
        name = re.escape(self.slot1_path.name)
        self.wait_log(rf"^.*unmei: loaded .*{name}$",
                      f"`loaded {self.slot1_path.name}`")
        if self.runner.sha256(self.slot1_path) != self.saved_slot:
            self.fail(f"{self.slot1_path.name} changed across the load")

    def quit_after_load(self):
        self.quit_to_title(overlay_open=False)  # a confirmed load closes the overlay
        self.wait_exit(0)

    def cli_list(self):
        try:
            # The CLI's pack default is the cwd, so its own `demo-pack` is named.
            pack = Path(self.cli).parent / PACK_NAME
            code, out, err = self.runner.run(
                [self.cli, "author", "draft", "first_meeting", "--list",
                 "--pack", pack],
                cwd=self.work_dir, env=self.env, timeout=WAIT_SECONDS)
        except subprocess.TimeoutExpired:
            self.fail(f"`unmei author draft --list` did not finish within the "
                      f"{WAIT_SECONDS:.0f} s wait")
        if code != 0:
            self.fail(f"the CLI exited {code}: {err.strip()[:200]}")
        if out != CLI_LIST_EXPECTED:
            self.fail("the CLI `--list` sample differs from the expected four rows: "
                      f"{out!r}")


def parse_args(argv):
    parser = argparse.ArgumentParser(
        description="Play the extracted Linux player under Xvfb.")
    parser.add_argument("extracted", type=Path,
                        help="the extracted player and CLI archives' directory")
    parser.add_argument("--work-dir", type=Path, help="launch from here (default: temp)")
    parser.add_argument("--exit-point", default=None, metavar="X,Y",
                        help="the rail's `Exit to title` click, in window pixels")
    parser.add_argument("--openbox", action="store_true", help="the tested fallback")
    parser.add_argument("--keep", action="store_true", help="keep sandbox and work dir")
    args = parser.parse_args(argv)
    args.exit_point = (tuple(int(part) for part in args.exit_point.split(","))
                       if args.exit_point else EXIT_TO_TITLE_POINT)
    return args


def main(argv=None):
    args = parse_args(argv)
    walk = Walk(Runner(), args.extracted, work_dir=args.work_dir,
                exit_point=args.exit_point, openbox=args.openbox, keep=args.keep)
    try:
        completed = walk.run()
    except StepFailure as failure:
        print(f"linux-walk: FAILED at step `{failure.step}`: {failure.detail}")
        print(f"linux-walk: bounds — {walk.bounds()}")
        for line in failure.tail:
            print(f"  | {line}")
        return 1
    print("linux-walk: passed " + ", ".join(completed))
    return 0


if __name__ == "__main__":
    sys.exit(main())
