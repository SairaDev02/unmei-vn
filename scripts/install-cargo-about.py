"""Install the pinned prebuilt cargo-about into Cargo's bin directory.

The packager refuses any other version. The archive comes from the official
EmbarkStudios release and must match the SHA-256 pinned here, the same way
`check.yml` pins its linters, so the candidate jobs never compile cargo-about.
"""

from __future__ import annotations

import hashlib
import io
import os
import sys
import tarfile
import urllib.request
from pathlib import Path

VERSION = "0.9.3"

ARCHIVES = {
    "win32": (
        "x86_64-pc-windows-msvc",
        "cargo-about.exe",
        "9e2942533ef1c592c4a70bdb384cae1550a3f5fc9ae40379b6662e3d2f16fe02",
    ),
    "linux": (
        "x86_64-unknown-linux-musl",
        "cargo-about",
        "3ad1e94ea1e63d69d1961f4fb15ccbbe9575d906111f96d2cb4dce6df48f71b4",
    ),
}


def main() -> int:
    if sys.platform not in ARCHIVES:
        print(f"no pinned cargo-about archive for {sys.platform}", file=sys.stderr)
        return 1
    target, binary, expected = ARCHIVES[sys.platform]
    name = f"cargo-about-{VERSION}-{target}"
    url = f"https://github.com/EmbarkStudios/cargo-about/releases/download/{VERSION}/{name}.tar.gz"
    with urllib.request.urlopen(url, timeout=60) as response:
        data = response.read()
    actual = hashlib.sha256(data).hexdigest()
    if actual != expected:
        print(f"{name}.tar.gz: SHA-256 {actual} does not match the pinned {expected}", file=sys.stderr)
        return 1
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        member = archive.extractfile(f"{name}/{binary}")
        if member is None:
            print(f"{name}.tar.gz holds no {binary}", file=sys.stderr)
            return 1
        payload = member.read()
    bin_dir = Path(os.environ.get("CARGO_HOME") or Path.home() / ".cargo") / "bin"
    bin_dir.mkdir(parents=True, exist_ok=True)
    installed = bin_dir / binary
    installed.write_bytes(payload)
    installed.chmod(0o755)
    print(f"installed cargo-about {VERSION} at {installed}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
