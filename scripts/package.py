#!/usr/bin/env python3
"""Build the host OS's player and CLI archives.

    python3 scripts/package.py --out <dir>

Checks the tree and toolchain, builds `unmei-play` and `unmei` with
`cargo build --locked --release`, generates the per-binary third-party notices,
packages both publishable crates through the workspace overlay, and writes the
two archives, both `.crate`s and `SHA256SUMS` into `--out`. The layout, refusal
and archive writers live in `package_lib.py` so each file stays within the
authored-file budget.

`--allow-dirty` and `--allow-placeholders` are rehearsal-only and stamp
"REHEARSAL — not a release" into each archive README and the output. Only x86_64
Windows (MSVC) and Linux (GNU) hosts are packaged, per their fixed layout.
"""

from __future__ import annotations

import argparse
import json
import platform
import subprocess
import sys
import tempfile
from pathlib import Path

# Load `package_lib` beside this script, both as `python3 scripts/package.py`
# and when the test suite loads this file as a module from another directory.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from package_lib import *  # noqa: E402,F401,F403
from package_lib import Refusal, sha256_bytes  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parent.parent

def command(args):
    print("$ " + " ".join(args), flush=True)
    return subprocess.run(list(args), cwd=REPO_ROOT, text=True,
                          stdout=subprocess.PIPE, stderr=subprocess.STDOUT)


def command_split(args):
    print("$ " + " ".join(args), flush=True)
    return subprocess.run(list(args), cwd=REPO_ROOT, text=True,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def checked(label, args):
    result = command(args)
    require_success(label, result.returncode, result.stdout)
    return result.stdout


def checked_stdout(label, args):
    """Run a command whose stdout is data (JSON or a file list), so cargo's
    warnings on stderr cannot enter it; a failure still reports both streams."""
    result = command_split(args)
    if result.returncode != 0:
        require_success(label, result.returncode,
                        "\n".join(part for part in (result.stdout, result.stderr) if part))
    return result.stdout


def git(*args):
    return checked(f"git {' '.join(args)}", ["git", *args])


def write_text(path, text):
    path.write_bytes(text.encode("utf-8"))


def collect_notices(kind, target, scratch):
    notice = scratch / f"{kind}-THIRD-PARTY-LICENSES"
    output = checked(f"cargo about generate -m {BINARY_MANIFEST[kind]}", [
        "cargo", "about", "generate", "--fail", "--locked",
        "-c", "packaging/about.toml", "-m", BINARY_MANIFEST[kind],
        "--target", target.triple, "-o", str(notice),
        "packaging/third-party-licenses.hbs"])
    require_no_validate_failures(output)
    data = notice.read_bytes()
    print(f"notices for {BINARY_EXE[kind]} on {target.triple}: {len(data)} bytes, "
          f"sha256 {sha256_bytes(data)}", flush=True)
    return data


def package(args: argparse.Namespace) -> int:
    root = REPO_ROOT
    target = target_for(sys.platform, platform.machine())
    stamp = rehearsal_stamp(args.allow_dirty, args.allow_placeholders)
    version = workspace_version((root / "Cargo.toml").read_text(encoding="utf-8"))
    demo_files = demo_pack_files(git("ls-files", "-z", "--", DEMO_PACK_SOURCE))
    if not demo_files:
        raise Refusal(f"no tracked files under {DEMO_PACK_SOURCE}/")
    out_dir = args.out.resolve()
    require_empty_out(out_dir)
    for line in run_header_lines(version, target, out_dir, stamp):
        print(line, flush=True)

    if args.allow_dirty:
        print("note: --allow-dirty skips the dirty-tree check", flush=True)
    else:
        require_clean(dirty_build_input_paths(git(
            "status", "--porcelain", "--untracked-files=all", "--", *BUILD_INPUT_PATHS)))
    if args.allow_placeholders:
        print("note: --allow-placeholders skips the placeholder check", flush=True)
    else:
        texts = {path: (root / path).read_bytes().decode("utf-8", "replace")
                 for path in placeholder_sources(demo_files)}
        require_no_placeholders(placeholder_findings(texts))

    about = command(["cargo", "about", "--version"])
    require_about_version(
        parse_about_version(about.stdout) if about.returncode == 0 else None)

    metadata = json.loads(checked_stdout("cargo metadata --locked --no-deps", [
        "cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"]))
    require_versions(
        version, [(item["name"], item["version"]) for item in metadata["packages"]])
    target_dir = Path(metadata["target_directory"])
    checked("cargo build --locked --release", ["cargo", "build", "--locked", "--release"])

    with tempfile.TemporaryDirectory(prefix="unmei-package-") as scratch:
        notices = {kind: collect_notices(kind, target, Path(scratch))
                   for kind in BINARIES}

    checked("cargo package --workspace --exclude unmei-app --locked",
            package_args(allow_dirty=args.allow_dirty))

    out_dir.mkdir(parents=True, exist_ok=True)
    written = []
    crate_sums = []
    for name, data in staged_crates(target_dir / "package", version):
        (out_dir / name).write_bytes(data)
        written.append(name)
        crate_sums.append((name, sha256_bytes(data)))

    commit_epoch = int(git("log", "-1", "--format=%ct").strip())
    sums: list[tuple[str, str]] = []
    for kind in BINARIES:
        exe_name = f"{BINARY_EXE[kind]}{target.exe_suffix}"
        executable = target_dir / "release" / exe_name
        if not executable.is_file():
            raise Refusal(f"the release build did not produce {executable}")
        exe_bytes = executable.read_bytes()
        sums.append((exe_name, sha256_bytes(exe_bytes)))

        name = archive_name(kind, version, target)
        stem = archive_stem(name)
        readme = stamp_readme(substitute_version(
            (root / BINARY_README[kind]).read_bytes().decode("utf-8"), version), stamp)
        contents = {f"{stem}/README": readme.encode("utf-8"),
                    f"{stem}/LICENSE": (root / "LICENSE").read_bytes(),
                    f"{stem}/THIRD-PARTY-LICENSES": notices[kind],
                    f"{stem}/{exe_name}": exe_bytes}
        if kind == "player":
            contents.update({f"{stem}/{font}":
                             (root / "app/assets/fonts" / font).read_bytes()
                             for font in FONT_LICENSES})
        contents.update({f"{stem}/demo-pack/{relative}":
                         (root / DEMO_PACK_SOURCE / relative).read_bytes()
                         for relative in demo_files})
        plan = plan_archive(kind, version, target, demo_files)
        write_archive(out_dir / name, archive_entries(plan, contents),
                      commit_epoch, target)
        written.append(name)
        sums.append((name, sha256_bytes((out_dir / name).read_bytes())))

    sums_text = sha256sums_text([*sums, *crate_sums])
    write_text(out_dir / "SHA256SUMS", sums_text)
    print(f"\nwritten to {out_dir}:")
    for name in [*written, "SHA256SUMS"]:
        print(f"  {name}  {(out_dir / name).stat().st_size} bytes")
    print("\nSHA256SUMS:\n" + sums_text, end="")
    if stamp:
        print(f"\n{stamp}")
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(
        description="Build this host's Unmei player and CLI release archives.")
    parser.add_argument("--out", required=True, type=Path, help="output directory")
    parser.add_argument("--allow-dirty", action="store_true",
                        help="rehearsal only: skip the dirty-tree refusal")
    parser.add_argument("--allow-placeholders", action="store_true",
                        help="rehearsal only: skip the placeholder refusal")
    args = parser.parse_args(argv)
    try:
        return package(args)
    except Refusal as refusal:
        print(f"refused: {refusal}", file=sys.stderr)
        return 1

if __name__ == "__main__":
    sys.exit(main())
