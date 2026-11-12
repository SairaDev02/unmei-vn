#!/usr/bin/env python3
"""Pure helpers shared by the repository's tooling scripts.

`scripts/package.py` owns most of this: the
archive layout, refusals and normalized writers, so its entry point stays small
and the logic is testable without cargo (`scripts/test-package.py`). The rest of
the repository's Python tooling imports the same base — `Refusal`, the digest
helpers and the JSON readers — so each of those exists once.
"""

from __future__ import annotations

import gzip
import hashlib
import json
import io
import re
import stat
import tarfile
import time
import zipfile
from pathlib import Path, PurePosixPath
from typing import NamedTuple

CARGO_ABOUT_VERSION = "0.9.3"
# The zip format's own floor; an older commit time is not representable there.
ZIP_MIN_MTIME = 315_532_800
STAMP = "REHEARSAL — not a release"

DEMO_PACK_SOURCE = "examples/demo-pack"
DEMO_PACK_PREFIX = DEMO_PACK_SOURCE + "/"
FONT_LICENSES = ("IBMPlexSans-license.txt", "FiraMono-OFL.txt", "FiraMono-LICENSE")
BINARY_EXE = {"player": "unmei-play", "cli": "unmei"}
BINARY_MANIFEST = {"player": "app/Cargo.toml", "cli": "cli/Cargo.toml"}
BINARY_README = {"player": "packaging/player-README.md", "cli": "packaging/author-README.md"}
BINARIES = ("player", "cli")
# The publishable crates, in publication order. The app is never packaged.
CRATE_NAMES = ("unmei-engine", "unmei")
PACKAGE_EXCLUDES = ("unmei-app",)

# Every input that reaches a build or an archive: a change here means the built
# archives no longer describe a commit, even when the changed file is not copied
# into one. `app`, `engine` and `cli` cover their whole trees.
BUILD_INPUT_PATHS = ("LICENSE", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml",
                     "app", "engine", "cli", "packaging", DEMO_PACK_SOURCE)

# The release set's never-ship names. A tracked one is still
# refused; selection reads the Git index, never the working tree, so an untracked
# sidecar, log, temp file or credential cannot enter either.
FORBIDDEN_DEMO_SUFFIXES = (".log", ".tmp", ".temp", ".bak", ".swp",
                           ".save.ron", ".read.ron", ".settings.ron")
FORBIDDEN_DEMO_NAMES = {"credentials", ".env", ".env.local", "unmei.author.ron"}
PLACEHOLDER = re.compile(r"@@[A-Za-z0-9_]+@@")
# A coordinator artifact carries the attempt that uploaded it: `<name>-attempt<N>`.
ATTEMPT_MARKER = "-attempt"

class Refusal(Exception):
    """Not safe to package; the message says what to fix."""


class Target(NamedTuple):
    platform: str
    triple: str
    archive_suffix: str
    exe_suffix: str


class PlannedEntry(NamedTuple):
    arcname: str
    mode: int
    origin: str


class ArchiveEntry(NamedTuple):
    arcname: str
    mode: int
    is_dir: bool
    data: bytes


def target_for(system: str, machine: str) -> Target:
    if machine.lower() not in {"amd64", "x86_64", "x64"}:
        raise Refusal(f"only x86_64 hosts are packaged; this host reports {machine!r}")
    if system == "win32":
        return Target("windows", "x86_64-pc-windows-msvc", ".zip", ".exe")
    if system.startswith("linux"):
        return Target("linux", "x86_64-unknown-linux-gnu", ".tar.gz", "")
    raise Refusal(f"no archive layout for this host ({system}); Windows and Linux only")


def archive_name(kind, version, target):
    return f"{BINARY_EXE[kind]}-{version}-x86_64-{target.platform}{target.archive_suffix}"


def archive_stem(name):
    for suffix in (".tar.gz", ".zip"):
        if name.endswith(suffix):
            return name[: -len(suffix)]
    raise Refusal(f"not an archive name: {name}")


def is_forbidden_demo_name(relative):
    name = PurePosixPath(relative).name.lower()
    return (name.endswith(FORBIDDEN_DEMO_SUFFIXES)
            or name in FORBIDDEN_DEMO_NAMES or name.startswith(".env"))


def demo_pack_files(listing):
    """The shipped demo-pack files, from `git ls-files` output (newline or NUL)."""
    files = []
    for path in re.split(r"[\0\n]", listing):
        path = path.strip()
        relative = path[len(DEMO_PACK_PREFIX):]
        if path.startswith(DEMO_PACK_PREFIX) and not is_forbidden_demo_name(relative):
            files.append(relative)
    return sorted(files)


def plan_archive(kind, version, target, demo_files):
    """The archive's entries, relative to the archive root, in sorted order."""
    if kind not in BINARY_EXE:
        raise Refusal(f"unknown archive kind: {kind}")
    stem = archive_stem(archive_name(kind, version, target))
    origins: dict[str, str] = {stem: "dir"}
    add = lambda name, origin: origins.setdefault(name, origin)
    add(f"{stem}/README", "readme")
    add(f"{stem}/LICENSE", "license")
    add(f"{stem}/THIRD-PARTY-LICENSES", "notice")
    add(f"{stem}/{BINARY_EXE[kind]}{target.exe_suffix}", "exe")
    if kind == "player":
        for font in FONT_LICENSES:
            add(f"{stem}/{font}", "font")
    add(f"{stem}/demo-pack", "dir")
    for relative in demo_files:
        arcname = f"{stem}/demo-pack/{relative}"
        parent = PurePosixPath(arcname).parent
        while str(parent) != stem:
            add(str(parent), "dir")
            parent = parent.parent
        add(arcname, "demo")
    mode_of = {"dir": 0o755, "exe": 0o755}
    return sorted((PlannedEntry(name, mode_of.get(origin, 0o644), origin)
                   for name, origin in origins.items()), key=lambda entry: entry.arcname)


def archive_entries(plan, contents):
    entries = []
    for planned in plan:
        is_dir = planned.origin == "dir"
        data = b"" if is_dir else contents[planned.arcname]
        entries.append(ArchiveEntry(planned.arcname, planned.mode, is_dir, data))
    return entries


def is_build_input_path(path):
    path = path.rstrip("/")
    return any(path == prefix or path.startswith(prefix + "/")
               for prefix in BUILD_INPUT_PATHS)


def dirty_build_input_paths(status_text):
    """Paths a `git status --porcelain` line reports inside the build inputs."""
    paths = []
    for line in status_text.splitlines():
        if len(line) < 4:
            continue
        changed = line[3:]
        for candidate in (changed.split(" -> ") if " -> " in changed else [changed]):
            candidate = candidate.strip().strip('"').rstrip("/")
            if candidate and is_build_input_path(candidate):
                paths.append(candidate)
    return paths


def indented(items):
    return "\n".join(f"  {item}" for item in items)


def require_empty_out(out_dir):
    """Refuse a reused output directory, so stale archives cannot pass as this run's."""
    if out_dir.exists() and (not out_dir.is_dir() or any(out_dir.iterdir())):
        raise Refusal(
            f"output directory {out_dir} is not empty; choose a fresh --out so stale "
            "archives cannot be mistaken for this run's")


def require_clean(dirty):
    if dirty:
        raise Refusal(
            "uncommitted tracked or untracked files in the build inputs; commit or "
            "stash them, or pass --allow-dirty for a rehearsal:\n" + indented(dirty))


def package_args(*, allow_dirty=False):
    """`cargo package` over the workspace minus the app.

    Staging both crates through Cargo's overlay is what replaces the engine-only
    package and the CLI's `--list`: the CLI is packaged and verified locally
    instead of being listed, because the overlay resolves the engine from the
    workspace rather than the registry.
    """
    args = ["cargo", "package", "--workspace"]
    for name in PACKAGE_EXCLUDES:
        args += ["--exclude", name]
    args.append("--locked")
    if allow_dirty:
        args.append("--allow-dirty")
    return args


def crate_files(version):
    """The `.crate` file names that `package_args` stages for this version."""
    return tuple(f"{name}-{version}.crate" for name in CRATE_NAMES)


def staged_crates(package_dir, version):
    """The staged `(name, bytes)` pairs, refusing when cargo wrote only one crate."""
    staged = []
    for name in crate_files(version):
        path = Path(package_dir) / name
        if not path.is_file():
            raise Refusal(f"cargo package did not write {path}")
        staged.append((name, path.read_bytes()))
    return staged


def placeholder_sources(demo_files):
    paths = ["packaging/player-README.md", "packaging/author-README.md",
             "engine/README.md", "cli/README.md", "engine/Cargo.toml",
             "cli/Cargo.toml", "engine/src/lib.rs", "cli/src/lib.rs", "LICENSE"]
    paths += [f"app/assets/fonts/{font}" for font in FONT_LICENSES]
    return paths + [f"{DEMO_PACK_PREFIX}{relative}" for relative in demo_files]


def placeholder_findings(texts):
    """`@@TOKEN@@` occurrences other than `@@VERSION@@`, in file then token order."""
    findings = []
    for path, text in texts.items():
        seen = []
        for token in PLACEHOLDER.findall(text):
            if token != "@@VERSION@@" and token not in seen:
                seen.append(token)
        findings.extend(f"{path}: {token}" for token in seen)
    return findings


def require_no_placeholders(findings):
    if findings:
        raise Refusal(
            "unresolved @@…@@ placeholders in packaged texts or crate manifests; "
            "fill them in, or pass --allow-placeholders for a rehearsal:\n"
            + indented(findings))


def install_command():
    return f"cargo install --locked cargo-about@{CARGO_ABOUT_VERSION} --features cli"


def parse_about_version(text):
    match = re.search(r"^cargo-about (\d+\.\d+\.\d+)\s*$", text, re.MULTILINE)
    return match.group(1) if match else None


def require_about_version(found):
    if found != CARGO_ABOUT_VERSION:
        raise Refusal(
            f"cargo-about {CARGO_ABOUT_VERSION} is required; found "
            f"{found or 'not installed, or not on PATH'}.\n"
            f"Install the pinned build with:\n  {install_command()}\n"
            "This script never installs or updates it.")


def validate_failure_lines(output):
    return [line.strip() for line in output.splitlines() if "failed to validate" in line]


def require_no_validate_failures(output):
    lines = validate_failure_lines(output)
    if lines:
        raise Refusal("cargo about could not validate a license; --fail only warns, "
                      "so this fails the run:\n" + indented(lines))


def workspace_version(manifest_text):
    section = re.search(r"(?ms)^\[workspace\.package\][^\n]*\n(.*?)(?=^\[|\Z)",
                        manifest_text)
    match = section and re.search(r'^version\s*=\s*"([^"]+)"', section.group(1),
                                  re.MULTILINE)
    if not match:
        raise Refusal("Cargo.toml has no [workspace.package] version to package under")
    return match.group(1)


def version_mismatches(version, packages):
    return [f"{name} {found} (workspace {version})"
            for name, found in packages if found != version]


def require_versions(version, packages):
    mismatches = version_mismatches(version, packages)
    if mismatches:
        raise Refusal(f"crate versions differ from [workspace.package] version "
                      f"{version}:\n" + indented(mismatches))


def require_success(label, returncode, output):
    if returncode != 0:
        tail = "\n".join(output.strip().splitlines()[-12:])
        raise Refusal(f"{label} failed with exit code {returncode}:\n{tail}")


def rehearsal_stamp(allow_dirty, allow_placeholders):
    flags = [flag for flag, chosen in (("--allow-dirty", allow_dirty),
                                       ("--allow-placeholders", allow_placeholders))
             if chosen]
    if not flags:
        return None
    return (f"{STAMP}. Built with {', '.join(flags)}: rehearsal archives, not a "
            "release and not certification evidence.")


def substitute_version(text, version):
    return text.replace("@@VERSION@@", version)


def stamp_readme(text, stamp):
    lines = text.split("\n")
    if not stamp or not lines:
        return text
    return "\n".join([lines[0], "", stamp, *lines[1:]])


def run_header_lines(version, target, out_dir, stamp):
    lines = [f"unmei packager: {version} for {target.platform} ({target.triple})",
             f"output directory: {out_dir}"]
    return lines + [stamp] if stamp else lines


def sha256sums_text(entries):
    return "".join(f"{digest}  {name}\n" for name, digest in sorted(entries))


def sha256_bytes(data):
    return hashlib.sha256(data).hexdigest()


def sha256_file(path):
    return sha256_bytes(Path(path).read_bytes())


def load_json(path, label):
    try:
        return json.loads(Path(path).read_text(encoding="utf-8"))
    except OSError as exc:
        raise Refusal(f"cannot read the {label} {path}: {exc}") from exc
    except json.JSONDecodeError as exc:
        raise Refusal(f"the {label} {path} is not JSON: {exc}") from exc


def write_json(path, payload):
    out = Path(path)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(payload, indent=2) + chr(10), encoding="utf-8", newline="")


def normalized_mtime(epoch):
    return max(int(epoch), ZIP_MIN_MTIME)


def zip_date_time(epoch):
    return tuple(time.gmtime(normalized_mtime(epoch))[:6])  # type: ignore[return-value]


def write_zip(path, entries, mtime):
    stamp = zip_date_time(mtime)
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for entry in sorted(entries, key=lambda item: item.arcname):
            info = zipfile.ZipInfo(entry.arcname + ("/" if entry.is_dir else ""),
                                   date_time=stamp)
            # Explicit metadata instead of `zipfile.write`: host-independent bytes.
            info.create_system = 3
            kind = stat.S_IFDIR if entry.is_dir else stat.S_IFREG
            info.external_attr = (kind | entry.mode) << 16
            info.compress_type = zipfile.ZIP_STORED if entry.is_dir else zipfile.ZIP_DEFLATED
            archive.writestr(info, b"" if entry.is_dir else entry.data)


def write_tar_gz(path, entries, mtime):
    stamp = normalized_mtime(mtime)
    with open(path, "wb") as raw:
        # gzip stamps the source filename and wall clock; clear both or reruns differ.
        with gzip.GzipFile(fileobj=raw, mode="wb", filename="", mtime=stamp,
                           compresslevel=9) as compressed:
            with tarfile.open(fileobj=compressed, mode="w",
                              format=tarfile.GNU_FORMAT) as archive:
                for entry in sorted(entries, key=lambda item: item.arcname):
                    info = tarfile.TarInfo(
                        entry.arcname + ("/" if entry.is_dir else ""))
                    info.mode = entry.mode
                    info.mtime = stamp
                    info.uid = info.gid = 0
                    info.uname = info.gname = ""
                    info.type = tarfile.DIRTYPE if entry.is_dir else tarfile.REGTYPE
                    info.size = 0 if entry.is_dir else len(entry.data)
                    archive.addfile(info, None if entry.is_dir else io.BytesIO(entry.data))


def write_archive(path, entries, mtime, target):
    (write_zip if target.archive_suffix == ".zip" else write_tar_gz)(
        path, entries, mtime)
