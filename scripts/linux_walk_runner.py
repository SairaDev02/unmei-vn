#!/usr/bin/env python3
"""The Linux walk's one seam to the display, the player processes and the disk.

`scripts/linux_walk.py` drives the walk through `Runner` alone, so its suite can
replace this object with a fake. Split from the walk to keep each authored file
under the size limit.
"""

from __future__ import annotations

import hashlib
import os
import shutil
import subprocess
import time
from pathlib import Path

WINDOW_NAME = "^unmei$"
KEY_HOLD_SECONDS = 0.4
WAIT_SECONDS = 30.0
SCREEN = "1920x1080x24"  # the size the first automated walk passed with


class Runner:
    """The one seam to the display, the player processes and the filesystem."""

    def __init__(self, clock=time.monotonic, sleep=time.sleep):
        self.clock = clock
        self.sleep = sleep
        self.display = None
        self.window_manager = None

    def sandbox_env(self, home, config, display):
        env = dict(os.environ)
        for name in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "OPENROUTER_API_KEY"):
            env.pop(name, None)
        env["HOME"] = str(home)
        env["XDG_CONFIG_HOME"] = str(config)
        if display:
            env["DISPLAY"] = display
        return env

    def spawn(self, args, cwd=None, env=None, output=None):
        """Start a process; `output` keeps its stdout and stderr for a failure's tail."""
        sink = subprocess.DEVNULL if output is None else open(output, "ab")
        try:
            return subprocess.Popen(
                [str(arg) for arg in args], cwd=None if cwd is None else str(cwd),
                env=env, stdout=sink, stderr=subprocess.STDOUT if output else sink)
        finally:
            if output is not None:
                sink.close()

    def run(self, args, cwd=None, env=None, timeout=None):
        result = subprocess.run(
            [str(arg) for arg in args], cwd=None if cwd is None else str(cwd),
            env=env, capture_output=True, text=True, timeout=timeout)
        return result.returncode, result.stdout, result.stderr

    def start_display(self, display):
        self.display = self.spawn(
            ["Xvfb", display, "-screen", "0", SCREEN, "-nolisten", "tcp"])

    def display_ready(self, env):
        """Whether the X server answers yet: Xvfb takes a moment to come up."""
        try:
            code, _, _ = self.run(["xdotool", "getdisplaygeometry"], env=env, timeout=5)
        except subprocess.TimeoutExpired:
            return False
        return code == 0

    def search_window(self, env):
        try:
            code, out, _ = self.run(
                ["xdotool", "search", "--sync", "--name", WINDOW_NAME],
                env=env, timeout=WAIT_SECONDS)
        except subprocess.TimeoutExpired:
            return None
        found = out.split()
        return found[-1] if code == 0 and found else None

    def focus_window(self, env, window):
        code, _, err = self.run(["xdotool", "windowfocus", "--sync", window], env=env)
        if code != 0:
            raise OSError(f"xdotool windowfocus {window} failed: {err.strip()}")

    def hold_key(self, env, key):
        # XTEST, not XSendEvent: apps reject the synthetic-event flag.
        code, _, err = self.run(["xdotool", "keydown", key], env=env)
        if code != 0:
            raise OSError(f"xdotool keydown {key} failed: {err.strip()}")
        self.sleep(KEY_HOLD_SECONDS)
        self.run(["xdotool", "keyup", key], env=env)

    def click(self, env, window, point):
        x, y = point
        code, _, err = self.run(
            ["xdotool", "mousemove", "--window", window, str(x), str(y), "click", "1"],
            env=env)
        if code != 0:
            raise OSError(f"xdotool click at {point} failed: {err.strip()}")

    def stop(self, handle):
        if handle is not None and handle.poll() is None:
            handle.terminate()

    def read_text(self, path):
        try:
            return Path(path).read_text(encoding="utf-8")
        except OSError:
            return None

    def sha256(self, path):
        try:
            return hashlib.sha256(Path(path).read_bytes()).hexdigest()
        except OSError:
            return None

    def make_dirs(self, path):
        Path(path).mkdir(parents=True, exist_ok=True)

    def remove_tree(self, path):
        shutil.rmtree(path, ignore_errors=True)
