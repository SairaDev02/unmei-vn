#!/usr/bin/env python3
"""Drive `scripts/linux_walk.py` against a fake player.

The walk's only contact with X, the player processes and the filesystem is
`linux_walk.Runner`, so replacing that one object with `FakeRunner` runs the real
step logic against `Player` — a stand-in that answers the walk's keys the way the
app does and writes the same session log and sidecars.

Each test breaks one observable and proves the walk fails at the named step.
Nothing here claims a Linux run: this suite verifies the walk's logic and its
step naming, and only a hosted `ubuntu-24.04` runner can verify the walk itself.
"""

import contextlib
import hashlib
import importlib.util
import io
import os
import pathlib
import shutil
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent


def load(module_name, filename):
    spec = importlib.util.spec_from_file_location(module_name, HERE / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


walk = load("linux_walk", "linux_walk.py")

PLAYER_DIR = "unmei-play-0.1.0-x86_64-linux"
CLI_DIR = "unmei-0.1.0-x86_64-linux"
STAMP = "2026-11-07T00:00:00.000000Z"


class Clock:
    """A virtual monotonic clock; `jump_after` sleeps in, it leaps past the job bound."""

    def __init__(self, jump_after=None):
        self.now = 0.0
        self.sleeps = 0
        self.jump_after = jump_after

    def __call__(self):
        return self.now

    def sleep(self, seconds):
        self.now += seconds
        self.sleeps += 1
        if self.jump_after is not None and self.sleeps > self.jump_after:
            self.now += walk.JOB_SECONDS + 100.0


class FakeHandle:
    def __init__(self):
        self.code = None

    def poll(self):
        return self.code

    def terminate(self):
        self.code = -15


class Player:
    """The app's surfaces, as far as the walk can tell: keys in, evidence out."""

    DEFAULTS = {"choice_key": "1", "unvisited": None, "autosave_scene": None,
                "silent_save": False, "silent_load": False, "third_failure": False,
                "title_ready": True, "exit_code": 0, "cli_code": 0,
                "cli_output": walk.CLI_LIST_EXPECTED}

    def __init__(self, files, root, **options):
        self.files = files
        self.pack = root / PLAYER_DIR / "demo-pack"
        self.log_path = self.pack.parent / f"{self.pack.name}.log"
        self.slot1 = self.pack.parent / f"{self.pack.name}.slot1.save.ron"
        self.opt = dict(self.DEFAULTS, **options)
        self.handle = FakeHandle()
        self.surface = "closed"
        self.scene = None
        self.presses = 0
        self.row = 1
        self.view = "slots"
        self.mode = "save"
        self.focus = "card"
        self.confirm = None
        self.live = False
        self.read = {}
        self.visited = []

    # -- evidence -----------------------------------------------------------

    def write(self, suffix, text):
        self.files[str(self.pack.parent / f"{self.pack.name}.{suffix}")] = text

    def log(self, line):
        current = self.files.get(str(self.log_path), "")
        self.files[str(self.log_path)] = current + f"{STAMP}  {line}\n"

    def emit(self, message):
        self.log(f"INFO unmei: {message}")

    def failure(self, message):
        self.log(f"WARN unmei::failure: {message}")

    def write_read(self):
        if self.opt["unvisited"] in self.visited:
            self.visited.remove(self.opt["unvisited"])
        lines = ", ".join(f'"{scene}": [{", ".join(str(n) for n in sorted(self.read[scene]))}]'
                          for scene in sorted(self.read))
        visited = ", ".join(f'"{scene}"' for scene in self.visited)
        self.write("read.ron",
                   f"(\n  version: 1,\n  read: {{{lines}}},\n  visited: [{visited}],\n)\n")

    def write_autosave(self, scene_id):
        chosen = self.opt["autosave_scene"] or scene_id
        self.write("autosave.save.ron",
                   "(\n  version: 2,\n  state: {\"met_ace\": Bool(true)},\n"
                   f'  scene_id: "{chosen}",\n  pc: 12,\n  wait: Some(Say),\n'
                   "  saved_at: Some(1793000000),\n)\n")

    def mark_read(self, scene_id, line):
        self.read.setdefault(scene_id, set()).add(line)
        self.write_read()

    def autosave_visible(self):
        return str(self.pack.parent / f"{self.pack.name}.autosave.save.ron") in self.files

    def autosave_scene(self):
        return "epilogue"

    # -- process ------------------------------------------------------------

    def start(self):
        self.handle = FakeHandle()
        self.surface, self.scene, self.presses = "title", None, 0
        self.view, self.mode, self.focus, self.confirm = "title", "save", "card", None
        self.files[str(self.log_path)] = "unmei-play 0.1.0\n"
        self.failure("couldn't start audio: DeviceNotAvailable")
        if self.opt["title_ready"]:
            self.emit("title ready")
        self.row = 0 if self.autosave_visible() else 1
        return self.handle

    def exit(self, code):
        self.handle.code = code

    # -- input --------------------------------------------------------------

    def key(self, key):
        if self.handle.code is not None:
            return
        if self.surface == "title":
            self.on_title(key)
        elif self.surface == "play":
            self.on_play(key)
        else:
            self.on_overlay(key)

    def click(self, point):
        if (self.surface == "overlay" and self.view == "settings"
                and tuple(point) == walk.EXIT_TO_TITLE_POINT):
            self.confirm = "exit"

    def on_title(self, key):
        if self.confirm == "quit":
            if key == "Return":
                self.exit(self.opt["exit_code"])
            elif key == "Escape":
                self.confirm = None
            return
        if key == "Down":
            self.row = (self.row + 1) % 5
        elif key == "Up":
            self.row = (self.row - 1) % 5
        elif key == "Return":
            if self.row == 0 and self.autosave_visible():
                self.enter_scene(self.autosave_scene())
            elif self.row == 1:
                self.live = True
                self.enter_scene("first_meeting")
            elif self.row == 4:
                if self.live:
                    self.confirm = "quit"
                else:
                    self.exit(self.opt["exit_code"])

    def on_play(self, key):
        if key == "Escape":
            self.surface, self.view, self.mode = "overlay", "slots", "save"
            self.focus, self.confirm = "card", None
        elif self.scene == "first_meeting":
            self.on_first_meeting(key)
        elif self.scene == "reunion":
            self.on_reunion(key)
        elif key == "Return":
            self.presses += 1
            if self.presses <= 3:
                self.mark_read("epilogue", 10)

    def on_first_meeting(self, key):
        if key == "Return":
            if self.presses < 2:
                self.presses += 1
                self.mark_read("first_meeting", (7, 10)[self.presses - 1])
        elif key.isdigit() and self.presses >= 2 and key == self.opt["choice_key"]:
            self.mark_read("first_meeting", 21)
            self.enter_scene("reunion")

    def on_reunion(self, key):
        if key != "Return":
            return
        if self.presses == 0:
            self.presses = 1
            self.mark_read("reunion", 7)
            self.failure("`reply.cold`: configuration error: no Decisions client")
            self.write_autosave("reunion")
        else:
            self.enter_scene("epilogue")

    def enter_scene(self, scene_id):
        self.surface, self.scene, self.presses = "play", scene_id, 0
        if scene_id not in self.visited:
            self.visited.append(scene_id)
        self.write_read()
        self.emit(f"entered scene {scene_id}")
        self.write_autosave(scene_id)
        if self.opt["third_failure"] and scene_id == "epilogue":
            self.failure("couldn't load a sprite: assets/sprites/ace.png")

    def on_overlay(self, key):
        if self.confirm == "load":
            if key == "Return":
                self.confirm = None
                self.surface, self.scene = "play", "epilogue"
                if not self.opt["silent_load"]:
                    self.emit(f"loaded {self.slot1}")
            elif key == "Escape":
                self.confirm = None
            return
        if self.confirm == "exit":
            if key == "Return":
                self.confirm = None
                self.surface, self.view = "title", "title"
                self.row = 0 if (self.live or self.autosave_visible()) else 1
            return
        if self.view == "settings":
            if key == "Escape":
                self.view = "slots"
            return
        if key == "Escape":
            self.surface = "play"
        elif key == "Tab":
            order = ["Save", "Load", "Timeline", "Gallery", "Settings"]
            self.focus = (order[(order.index(self.focus) + 1) % len(order)]
                          if self.focus in order else "Save")
        elif key == "1":
            self.focus = "card"
        elif key == "Return":
            if self.focus in ("Save", "Load"):
                self.mode = self.focus.lower()
            elif self.focus == "Settings":
                self.view = "settings"
            elif self.focus == "card":
                self.activate_card()

    def activate_card(self):
        if self.mode == "save":
            self.write("slot1.save.ron",
                       "(\n  version: 2,\n  state: {},\n  scene_id: \"epilogue\",\n"
                       "  pc: 12,\n  wait: Some(Say),\n)\n")
            if not self.opt["silent_save"]:
                self.emit(f"saved to {self.slot1}")
        else:
            self.confirm = "load"


class FakeRunner:
    """`linux_walk.Runner` with the X server, the processes and the disk faked."""

    def __init__(self, player, files, clock):
        self.player = player
        self.files = files
        self.clock = clock
        self.sleep = clock.sleep
        self.keys = []
        self.clicks = []
        self.focuses = 0
        self.window_manager = None
        self.display = None
        self.events = []
        self.ready_checks = 0
        self.unready_checks = 0
        self.output = None

    def sandbox_env(self, home, config, display):
        return {"HOME": str(home), "XDG_CONFIG_HOME": str(config), "DISPLAY": display}

    def make_dirs(self, path):
        pathlib.Path(path).mkdir(parents=True, exist_ok=True)

    def remove_tree(self, path):
        shutil.rmtree(path, ignore_errors=True)

    def spawn(self, args, cwd=None, env=None, output=None):
        self.events.append("spawn")
        self.output = output
        return self.player.start()

    def start_display(self, display):
        self.events.append("display")
        self.display = FakeHandle()
        return self.display

    def display_ready(self, env):
        self.events.append("ready?")
        self.ready_checks += 1
        return self.ready_checks > self.unready_checks

    def search_window(self, env):
        return "2097153"

    def focus_window(self, env, window):
        self.focused = window
        self.focuses += 1

    def hold_key(self, env, key):
        self.keys.append(key)
        self.player.key(key)

    def click(self, env, window, point):
        self.clicks.append(tuple(point))
        self.player.click(point)

    def stop(self, handle):
        if handle is not None and handle.poll() is None:
            handle.code = 0

    def read_text(self, path):
        return self.files.get(str(path))

    def sha256(self, path):
        text = self.files.get(str(path))
        return None if text is None else hashlib.sha256(text.encode()).hexdigest()

    def run(self, args, cwd=None, env=None, timeout=None):
        if pathlib.Path(str(args[0])).name != "unmei":
            raise AssertionError(f"unexpected command: {args}")
        return self.player.opt["cli_code"], self.player.opt["cli_output"], "boom"


class WalkCase(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.tmp.name) / "extracted"
        for archive, name in ((PLAYER_DIR, "unmei-play"), (CLI_DIR, "unmei")):
            binary = self.root / archive / name
            binary.parent.mkdir(parents=True)
            binary.write_text("", encoding="utf-8")
        self.work = pathlib.Path(self.tmp.name) / "elsewhere"
        self.work.mkdir()

    def tearDown(self):
        self.tmp.cleanup()

    def make(self, jump_after=None, **options):
        files = {}
        player = Player(files, self.root, **options)
        runner = FakeRunner(player, files, Clock(jump_after))
        return walk.Walk(runner, self.root, work_dir=self.work), runner, player

    def run_walk(self, jump_after=None, **options):
        subject, runner, player = self.make(jump_after, **options)
        return subject, runner, player, subject.run()

    def failure(self, jump_after=None, **options):
        subject, runner, player = self.make(jump_after, **options)
        with self.assertRaises(walk.StepFailure) as raised:
            subject.run()
        return raised.exception, subject, runner


class RelativePaths(WalkCase):
    def test_a_relative_extracted_dir_still_launches_from_another_work_dir(self):
        # The coordinator passes `extracted` relative to the checkout and the
        # walk launches from elsewhere; the first rehearsal's walk then could
        # not find the player it had just found.
        here = os.getcwd()
        os.chdir(self.root.parent)
        try:
            subject = walk.Walk(FakeRunner(None, {}, Clock(None)), pathlib.Path("extracted"),
                                work_dir=self.work)
        finally:
            os.chdir(here)
        self.assertTrue(subject.player.is_absolute(), subject.player)
        self.assertTrue(subject.cli.is_absolute(), subject.cli)


class TheWholeWalk(WalkCase):
    def test_every_named_step_completes_in_scope_order(self):
        _, runner, _, completed = self.run_walk()
        self.assertEqual(completed, [
            "title-ready", "new-game", "scene-first_meeting", "choice-1",
            "scene-reunion", "scene-epilogue", "autosave-epilogue", "read-log",
            "fallback-warning", "save-slot1", "quit", "relaunch-continue",
            "load-slot1", "quit-after-load", "cli-list",
        ])

    def test_the_window_is_focused_once_per_launch(self):
        _, runner, _, _ = self.run_walk()
        self.assertEqual(runner.focused, "2097153")
        self.assertEqual(runner.focuses, 2)

    def test_the_walk_shells_out_to_no_screenshot_tool(self):
        source = "".join((HERE / name).read_text(encoding="utf-8")
                         for name in ("linux_walk.py", "linux_walk_runner.py"))
        for tool in ("xwd", "scrot", "magick", "import "):
            self.assertNotIn(f'"{tool}"', source)

    def test_a_launch_needs_its_own_log_not_the_previous_sessions(self):
        subject, runner, player = self.make()
        subject.run()
        live = runner.player.files[str(player.log_path)]
        self.assertIn("entered scene first_meeting", subject.previous_log)
        self.assertNotIn("entered scene first_meeting", live)
        self.assertIn("title ready", live)


class MissedWaits(WalkCase):
    def test_a_missed_title_ready_names_title_ready(self):
        failure, subject, _ = self.failure(title_ready=False)
        self.assertEqual(failure.step, "title-ready")
        self.assertIn("timed out after 30 s", failure.detail)

    def test_a_missing_window_names_title_ready(self):
        subject, runner, _ = self.make()
        runner.search_window = lambda env: None
        with self.assertRaises(walk.StepFailure) as raised:
            subject.run()
        self.assertEqual(raised.exception.step, "title-ready")
        self.assertIn("waiting for a window named ^unmei$", raised.exception.detail)

    def test_the_player_launches_only_once_the_display_answers(self):
        # The third rehearsal launched into an Xvfb that was not up yet, and
        # no window ever appeared.
        subject, runner, _ = self.make()
        runner.unready_checks = 2
        subject.run()
        first_spawn = runner.events.index("spawn")
        self.assertEqual(runner.events[:first_spawn],
                         ["display", "ready?", "ready?", "ready?"])

    def test_a_display_that_never_answers_names_title_ready(self):
        subject, runner, _ = self.make()
        runner.unready_checks = 10 ** 6
        with self.assertRaises(walk.StepFailure) as raised:
            subject.run()
        self.assertEqual(raised.exception.step, "title-ready")
        self.assertIn("the X display", raised.exception.detail)
        self.assertNotIn("spawn", runner.events)

    def test_a_failed_step_shows_the_player_s_own_output(self):
        # A crash before the session log starts is only in the player's output.
        subject, runner, _ = self.make()
        runner.search_window = lambda env: None
        spawn = runner.spawn
        def crashing(args, cwd=None, env=None, output=None):
            runner.files[str(output)] = "thread 'main' panicked at winit\n"
            return spawn(args, cwd=cwd, env=env, output=output)
        runner.spawn = crashing
        with self.assertRaises(walk.StepFailure) as raised:
            subject.run()
        self.assertIn("-- player output --", raised.exception.tail)
        self.assertIn("thread 'main' panicked at winit", raised.exception.tail)

    def test_a_wrong_key_at_the_choice_names_the_choice_step(self):
        # The player still wants the authored `1`; the walk's key is not it.
        failure, _, runner = self.failure(choice_key="2")
        self.assertEqual(failure.step, "choice-1")
        self.assertTrue(runner.keys.count("1") >= 1)


class Evidence(WalkCase):
    def test_a_third_failure_row_is_rejected(self):
        failure, _, _ = self.failure(third_failure=True)
        self.assertEqual(failure.step, "scene-epilogue")
        self.assertIn("couldn't load a sprite", failure.detail)

    def test_the_audio_and_fallback_rows_are_allowed(self):
        failure, _, _ = self.failure(choice_key="2")
        self.assertNotIn("unpermitted", failure.detail)

    def test_a_missing_scene_in_the_read_log_fails_read_log(self):
        failure, _, _ = self.failure(unvisited="reunion")
        self.assertEqual(failure.step, "read-log")
        self.assertIn("all three scenes visited", failure.detail)

    def test_a_wrong_autosave_scene_fails_the_epilogue_assertion(self):
        failure, _, _ = self.failure(autosave_scene="reunion")
        self.assertEqual(failure.step, "autosave-epilogue")
        self.assertIn('scene_id "epilogue"', failure.detail)

    def test_a_save_without_its_log_line_fails_save_slot1(self):
        failure, _, _ = self.failure(silent_save=True)
        self.assertEqual(failure.step, "save-slot1")
        self.assertIn("saved to demo-pack.slot1.save.ron", failure.detail)

    def test_a_load_without_its_log_line_fails_load_slot1(self):
        failure, _, _ = self.failure(silent_load=True)
        self.assertEqual(failure.step, "load-slot1")
        self.assertIn("loaded demo-pack.slot1.save.ron", failure.detail)

    def test_a_non_zero_player_exit_fails_the_quit_step(self):
        failure, _, _ = self.failure(exit_code=7)
        self.assertEqual(failure.step, "quit")
        self.assertIn("exited 7", failure.detail)

    def test_a_wrong_cli_sample_fails_cli_list(self):
        failure, _, _ = self.failure(cli_output="  start       L7\n")
        self.assertEqual(failure.step, "cli-list")

    def test_a_failing_cli_exit_fails_cli_list(self):
        failure, _, _ = self.failure(cli_code=1)
        self.assertEqual(failure.step, "cli-list")
        self.assertIn("exited 1", failure.detail)


class Bounds(WalkCase):
    def test_the_ten_minute_job_bound_fails(self):
        failure, subject, _ = self.failure(jump_after=0)
        self.assertIn("job bound", failure.detail)
        self.assertIn("each wait is 30 s, the job 10 min", subject.bounds())

    def test_a_missed_wait_carries_the_step_the_bound_and_the_log_tail(self):
        failure, _, _ = self.failure(unvisited="reunion")
        self.assertIn("30 s wait", failure.detail)
        self.assertTrue(any("entered scene epilogue" in line for line in failure.tail))


class Reporting(WalkCase):
    def cli(self, argv, runner):
        original = walk.Runner
        walk.Runner = lambda: runner
        out = io.StringIO()
        try:
            with contextlib.redirect_stdout(out):
                code = walk.main(argv)
        finally:
            walk.Runner = original
        return code, out.getvalue()

    def test_main_names_the_step_the_bounds_and_the_tail(self):
        _, runner, _ = self.make(unvisited="reunion")
        code, printed = self.cli([str(self.root), "--work-dir", str(self.work)], runner)
        self.assertEqual(code, 1)
        self.assertIn("linux-walk: FAILED at step `read-log`", printed)
        self.assertIn("each wait is 30 s, the job 10 min", printed)
        self.assertIn("  | ", printed)

    def test_main_reports_a_pass_with_every_step(self):
        _, runner, _ = self.make()
        code, printed = self.cli([str(self.root), "--work-dir", str(self.work)], runner)
        self.assertEqual(code, 0)
        self.assertIn("linux-walk: passed title-ready, new-game", printed)
        self.assertIn("cli-list", printed)

    def test_an_archive_without_the_executables_is_a_setup_failure(self):
        empty = pathlib.Path(self.tmp.name) / "empty"
        empty.mkdir()
        _, runner, _ = self.make()
        code, printed = self.cli([str(empty), "--work-dir", str(self.work)], runner)
        self.assertEqual(code, 1)
        self.assertIn("FAILED at step `setup`", printed)


class TheSeam(unittest.TestCase):
    def test_the_walk_declares_no_process_or_x_call_of_its_own(self):
        source = (HERE / "linux_walk.py").read_text(encoding="utf-8")
        body = source.split("class Walk:", 1)[1]
        for forbidden in ("subprocess.run(", "subprocess.Popen(", "os.environ",
                          "shutil.", "hashlib."):
            self.assertNotIn(forbidden, body)

    def test_the_decision_fallback_row_is_the_one_the_app_writes(self):
        row = (f'{STAMP}  WARN unmei::failure: `reply.cold`: configuration error: '
               "no Decisions client")
        self.assertTrue(walk.is_allowed_failure(row))
        audio = f"{STAMP}  WARN unmei::failure: couldn't start audio: DeviceNotAvailable"
        self.assertTrue(walk.is_allowed_failure(audio))
        sprite = f"{STAMP}  WARN unmei::failure: couldn't load a sprite: x.png"
        self.assertFalse(walk.is_allowed_failure(sprite))


if __name__ == "__main__":
    unittest.main(verbosity=2)
