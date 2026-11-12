#!/usr/bin/env python3
"""Acceptance of one release archive on the OS it was built for.

    python3 scripts/archive_accept.py accept --archive-dir <dir> --os windows \
                                             --out accept-windows-slot.json

The archives are extracted to a fresh directory and every check runs from
another working directory with Cargo off `PATH`, so nothing resolves against the
checkout: the run exercises the bytes a user downloads. The checks are the
archive layout, `-V` on both executables, the CLI's `--help` and the CLI's
`--list` sample, whose expected text is the walk's own constant so the two
cannot drift.

The bounded boots of the archived player (Linux) and the walk stay in
`candidate.yml` and `linux_walk.py`; this script never starts a window, so it
needs no display and runs on Windows too.

Exit status is 0 when every check passes, 1 with a named failure otherwise, and
2 when the archive cannot be read at all.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import tarfile
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from package_lib import (BINARY_EXE, FONT_LICENSES, Refusal, archive_stem,  # noqa: E402
                         indented, sha256_file, target_for)
from linux_walk import CLI_LIST_EXPECTED, PACK_NAME  # noqa: E402

PROFILE_VARIABLES = ("RUSTUP_TOOLCHAIN", "CARGO", "CARGO_HOME", "RUSTC")
CHECK_SECONDS = 60.0
BOOT_SECONDS = 20
OS_PLATFORM = {"windows": ("win32", "AMD64"), "linux": ("linux", "x86_64")}


def extract(archive, into):
    """Extract one archive and return its single top-level directory."""
    stem = archive_stem(archive.name)
    if archive.name.endswith(".zip"):
        with zipfile.ZipFile(archive) as handle:
            handle.extractall(into)
    elif archive.name.endswith(".tar.gz"):
        with tarfile.open(archive, "r:gz") as handle:
            handle.extractall(into, filter="data")
    else:
        raise Refusal(f"{archive.name} is neither a .zip nor a .tar.gz")
    tops = sorted({path.name for path in Path(into).iterdir()})
    if tops != [stem]:
        raise Refusal(f"{archive.name} holds {tops or 'nothing'}, not one {stem}/ folder")
    return Path(into) / stem


def layout_problems(kind, root, version, exe_suffix):
    """The payload the archive README promises: one folder, the exe and the notices."""
    problems = []
    exe = f"{BINARY_EXE[kind]}{exe_suffix}"
    for name in ("README", "LICENSE", "THIRD-PARTY-LICENSES"):
        if not (root / name).is_file():
            problems.append(f"{root.name}/{name} is missing")
    if not (root / exe).is_file():
        return problems + [f"{root.name}/{exe} is missing"]
    if exe_suffix == "" and not os.access(root / exe, os.X_OK):
        problems.append(f"{root.name}/{exe} is not executable")
    readme = (root / "README").read_text(encoding="utf-8")
    if version not in readme:
        problems.append(f"{root.name}/README does not name {version}")
    if kind == "player":
        for font in FONT_LICENSES:
            if not (root / font).is_file():
                problems.append(f"{root.name}/{font} is missing")
    # Both archives bundle the demo pack: the player loads it beside the exe, and
    # the author README's first commands run `--pack demo-pack` from the CLI folder.
    if not (root / PACK_NAME / "pack.ron").is_file():
        problems.append(f"{root.name}/{PACK_NAME}/pack.ron is missing")
    return problems


def clean_env(home):
    """The environment a user has: no Cargo, no toolchain, no inherited display."""
    env = {key: value for key, value in os.environ.items()
           if key not in PROFILE_VARIABLES}
    env["HOME"] = str(home)
    env["XDG_CONFIG_HOME"] = str(Path(home) / ".config")
    env["PATH"] = os.pathsep.join(
        part for part in env.get("PATH", "").split(os.pathsep)
        if part and "cargo" not in part.lower() and "rustup" not in part.lower())
    return env


def checks_for(roots, version, exe_suffix):
    """`(name, argv, expected stdout)` for the two executables' self-description."""
    player = roots["player"] / f"{BINARY_EXE['player']}{exe_suffix}"
    cli = roots["cli"] / f"{BINARY_EXE['cli']}{exe_suffix}"
    pack = roots["cli"] / PACK_NAME
    return [
        ("player-version", [str(player), "-V"], f"{BINARY_EXE['player']} {version}\n"),
        ("cli-version", [str(cli), "-V"], f"{BINARY_EXE['cli']} {version}\n"),
        ("cli-list", [str(cli), "author", "draft", "first_meeting", "--list",
                      "--pack", str(pack)], CLI_LIST_EXPECTED),
    ]


def archive_suffix(os_name):
    return target_for(*OS_PLATFORM[os_name]).archive_suffix


def find_archive(directory, kind, os_name):
    """The one archive of this kind for this OS, by the names packaging writes."""
    suffix = f"-x86_64-{'windows' if os_name == 'windows' else 'linux'}" + archive_suffix(os_name)
    found = [path for path in sorted(Path(directory).iterdir())
             if path.name.endswith(suffix)
             and path.name.startswith(f"{BINARY_EXE[kind]}-")
             and not (kind == "cli" and path.name.startswith(f"{BINARY_EXE['player']}-"))]
    if len(found) != 1:
        raise Refusal(f"{directory} holds {len(found)} {kind} archives for {os_name}, not one")
    return found[0]


def release_version(archives):
    """The version the archive names carry, which is what the README must state."""
    versions = {}
    for path in archives:
        stem = archive_stem(path.name)
        for prefix in (f"{BINARY_EXE['player']}-", f"{BINARY_EXE['cli']}-"):
            if stem.startswith(prefix):
                versions[path.name] = stem[len(prefix):].rsplit("-x86_64-", 1)[0]
                break
    found = set(versions.values())
    if len(found) != 1:
        raise Refusal(f"the archives disagree about their version: {versions}")
    return found.pop()


class Runner:
    """Every external effect, so the checks can run against a fake player."""

    def temp_dir(self):
        return tempfile.TemporaryDirectory(prefix="unmei-accept-")

    def run(self, argv, cwd, env, timeout):
        return subprocess.run(argv, cwd=str(cwd), env=env, timeout=timeout,
                              text=True, stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE)

    def extract(self, archive, into):
        return extract(archive, into)


def accept_archives(runner, archive_dir, os_name, out):
    exe_suffix = ".exe" if os_name == "windows" else ""
    slot = f"accept-{os_name}"
    problems = []
    archives = {kind: find_archive(archive_dir, kind, os_name)
                for kind in ("player", "cli")}
    problems.extend(sums_problems(archive_dir, [path.name for path in archives.values()]))
    version = release_version(list(archives.values()))
    with runner.temp_dir() as scratch:
        scratch = Path(scratch)
        roots = {}
        for kind, archive in archives.items():
            roots[kind] = runner.extract(archive, scratch / kind)
            problems.extend(layout_problems(kind, roots[kind], version, exe_suffix))

        work = scratch / "work"
        work.mkdir()
        home = scratch / "home"
        home.mkdir()
        env = clean_env(home)
        cli = roots["cli"] / f"{BINARY_EXE['cli']}{exe_suffix}"
        helped = runner.run([str(cli), "--help"], work, env, CHECK_SECONDS)
        if helped.returncode != 0:
            problems.append(f"`unmei --help` exited {helped.returncode}")
        elif "usage" not in (helped.stdout or "").lower():
            problems.append("`unmei --help` printed no usage line")
        for name, argv, expected in checks_for(roots, version, exe_suffix):
            result = runner.run(argv, work, env, CHECK_SECONDS)
            if result.returncode != 0:
                problems.append(f"{name}: exited {result.returncode}: "
                                f"{(result.stderr or '').strip()[:200]}")
            elif (result.stdout or "") != expected:
                problems.append(f"{name}: printed {result.stdout!r}, expected {expected!r}")
    return write_slot(slot, problems, out)


def boot_command(player, seconds=BOOT_SECONDS, dev=False):
    """A bounded GUI launch under a virtual display, as `boot.yml` runs it."""
    argv = ["xvfb-run", "-a", "-s", "-screen 0 1920x1080x24",
            "timeout", "--signal=KILL", str(seconds), str(player)]
    return argv + (["--dev"] if dev else [])


def find_player(extracted):
    """The extracted player executable, by the two names packaging gives it."""
    names = {BINARY_EXE["player"], f"{BINARY_EXE['player']}.exe"}
    # Absolute, because the boots launch it from another working directory.
    matches = sorted(path for path in Path(extracted).absolute().rglob(f"{BINARY_EXE['player']}*")
                     if path.is_file() and path.name in names)
    if not matches:
        raise Refusal(f"no {BINARY_EXE['player']} under {extracted}")
    return matches[0]


def boot_problems(runner, player, seconds=BOOT_SECONDS):
    """Both bounded launches: the bound killing the player is the pass."""
    problems = []
    work = Path(player).parent
    for label, dev in (("normal", False), ("--dev", True)):
        result = runner.run(boot_command(player, seconds, dev), work,
                            clean_env(work), seconds + 30)
        output = (result.stdout or "") + (result.stderr or "")
        if result.returncode not in (124, 137):
            problems.append(f"the {label} boot exited {result.returncode} instead of "
                            f"running to the {seconds}s bound")
        elif "panicked" in output or "Error when initializing schedule" in output:
            problems.append(f"the {label} boot panicked")
    return problems


def sums_problems(directory, expected_names):
    """Every `SHA256SUMS` line the downloaded archives must satisfy.

    The acceptance job rechecks the sums its own packaging job wrote, so a
    corrupted download fails here rather than in the walk.
    """
    path = Path(directory) / "SHA256SUMS"
    if not path.is_file():
        return [f"{directory} carries no SHA256SUMS"]
    problems = []
    listed = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        match = re.match(r"^([0-9a-f]{64}) [ *](.+)$", line)
        if match is None:
            problems.append(f"SHA256SUMS has a line that is not a sum: {line!r}")
            continue
        listed[match.group(2)] = match.group(1)
    for name in expected_names:
        if name not in listed:
            problems.append(f"SHA256SUMS does not list {name}")
        elif listed[name] != sha256_file(Path(directory) / name):
            problems.append(f"{name} does not match SHA256SUMS")
    return problems


def write_slot(slot, problems, out):
    entry = {"slot": slot, "status": "failed" if problems else "passed"}
    if problems:
        entry["reason"] = indented(problems).replace("\n", " ")
    Path(out).write_bytes((json.dumps(entry, indent=2) + "\n").encode("utf-8"))
    if problems:
        print(f"{slot}: failed", file=sys.stderr)
        for problem in problems:
            print(f"  {problem}", file=sys.stderr)
        return 1
    print(f"{slot}: passed")
    return 0


def accept(args):
    return accept_archives(Runner(), args.archive_dir, args.os, args.out)


def boot(args):
    """The Linux shipping boots: a bounded normal launch and a bounded `--dev` one."""
    problems = boot_problems(Runner(), find_player(args.extracted), args.seconds)
    for problem in problems:
        print(f"boot: {problem}", file=sys.stderr)
    if problems:
        return 1
    print(f"boot: both launches ran to the {args.seconds}s bound")
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(description="Accept one release archive.")
    commands = parser.add_subparsers(dest="command", required=True)
    run = commands.add_parser("accept", help="extract and check one OS's archives")
    run.add_argument("--archive-dir", required=True)
    run.add_argument("--os", required=True, choices=("windows", "linux"))
    run.add_argument("--out", required=True)
    run.set_defaults(func=accept)

    boot_parser = commands.add_parser("boot", help="bounded launches of the extracted player")
    boot_parser.add_argument("--extracted", required=True)
    boot_parser.add_argument("--seconds", type=int, default=BOOT_SECONDS)
    boot_parser.set_defaults(func=boot)

    args = parser.parse_args(argv)
    try:
        return args.func(args)
    except Refusal as refusal:
        print(f"refused: {refusal}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
