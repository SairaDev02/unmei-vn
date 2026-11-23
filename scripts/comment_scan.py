"""Extract every comment from a source file, with its kind and line span.

One scanner per language family, so a call site that only needs the in-scope
comments shares one definition of what a comment is with the ratchet that
counts them.
"""

from __future__ import annotations

import io
import re
import tokenize
from dataclasses import dataclass
from pathlib import Path

# Directives are comments a tool reads. They never enter a sweep.
DIRECTIVES = (
    "shellcheck", "yaml-language-server", "noqa", "type:", "fmt:", "pylint:", "ruff:",
    "mypy:", "pragma:", "coding:", "-*-", "prettier-ignore", "eslint",
)

VERSION_PIN = re.compile(r"#\s*v\d+(?:\.\d+)*")
YAML_SUFFIXES = (".yml", ".yaml")


@dataclass(frozen=True)
class Comment:
    path: str
    start: int
    end: int
    text: str
    kind: str

    @property
    def alone_lines(self) -> int:
        """Lines the comment occupies with no code beside it."""
        return self.end - self.start + 1 if self.kind != "trailing" else 0

    @property
    def directive(self) -> bool:
        if self.kind.endswith(":"):
            return True
        body = self.text.lstrip("#/*!<> \t")
        return body.startswith("!") or body.startswith(DIRECTIVES)

    @property
    def version_pin(self) -> bool:
        """A trailing YAML comment whose whole body is a version tag (#151).

        Dependabot rewrites the tag on a pinned `uses:` line, so the comment is
        tool-maintained and must not enter the baseline.
        """
        if self.kind != "trailing" or Path(self.path).suffix not in YAML_SUFFIXES:
            return False
        return VERSION_PIN.fullmatch(self.text.strip()) is not None


def _rows(path: str, text: str, spans: list[tuple[int, int, str, str]]) -> list[Comment]:
    out = []
    for start, end, body, kind in spans:
        out.append(Comment(path, start, end, body, kind))
    return out


def scan_rust(path: str, src: str) -> list[Comment]:
    spans: list[tuple[int, int, str, str]] = []
    i, n, line = 0, len(src), 1
    chars = src

    def line_before(idx: int) -> str:
        head = src.rfind("\n", 0, idx)
        return src[head + 1 : idx]

    while i < n:
        ch = chars[i]

        if ch == "\n":
            line += 1
            i += 1
            continue

        if ch == "/" and i + 1 < n and chars[i + 1] == "/":
            j = src.find("\n", i)
            j = n if j < 0 else j
            body = src[i:j]
            if body.startswith("//!"):
                kind = "rustdoc-inner:"
            elif body.startswith("///"):
                kind = "rustdoc:"
            elif line_before(i).strip():
                kind = "trailing"
            else:
                kind = "line"
            spans.append((line, line, body, kind))
            i = j
            continue

        if ch == "/" and i + 1 < n and chars[i + 1] == "*":
            depth, j = 1, i + 2
            while j < n and depth:
                if chars.startswith("/*", j):
                    depth += 1
                    j += 2
                elif chars.startswith("*/", j):
                    depth -= 1
                    j += 2
                else:
                    j += 1
            body = src[i:j]
            if body.startswith("/*!"):
                kind = "rustdoc-inner:"
            elif body.startswith("/**"):
                kind = "rustdoc:"
            else:
                kind = "block"
            end = line + body.count("\n")
            spans.append((line, end, body, kind))
            line = end
            i = j
            continue

        if ch == '"':
            j = i + 1
            while j < n:
                if chars[j] == "\\":
                    j += 2
                elif chars[j] == '"':
                    j += 1
                    break
                else:
                    j += 1
            line += src.count("\n", i, j)
            i = j
            continue

        if ch == "r" and i + 1 < n and (chars[i + 1] in "#\""):
            k = i + 1
            while k < n and chars[k] == "#":
                k += 1
            if k < n and chars[k] == '"':
                hashes = chars[i + 1 : k]
                close = '"' + hashes
                j = src.find(close, k + 1)
                j = n if j < 0 else j + len(close)
                line += src.count("\n", i, j)
                i = j
                continue

        if ch == "'":
            after = chars[i + 1 : i + 3]
            if after.startswith("\\"):
                j = src.find("'", i + 2)
                j = n if j < 0 else j + 1
                line += src.count("\n", i, j)
                i = j
                continue
            if len(after) >= 2 and after[1] == "'":
                i += 3
                continue
            i += 1
            continue

        i += 1

    return _rows(path, src, spans)


def scan_python(path: str, src: str) -> list[Comment]:
    spans: list[tuple[int, int, str, str]] = []
    try:
        for tok in tokenize.generate_tokens(io.StringIO(src).readline):
            if tok.type == tokenize.COMMENT:
                if tok.start[0] == 1 and tok.start[1] == 0 and tok.string.startswith("#!"):
                    kind = "shebang:"
                elif tok.line[: tok.start[1]].strip():
                    kind = "trailing"
                else:
                    kind = "line"
                spans.append((tok.start[0], tok.end[0], tok.string, kind))
    except (tokenize.TokenError, IndentationError):
        spans = [(i, i, ln.rstrip(), "line") for i, ln in enumerate(src.splitlines(), 1)
                 if ln.lstrip().startswith("#")]
    return _rows(path, src, spans)


def _hash_quote_scan(path: str, src: str, triples: tuple[str, ...], doubled: str | None) -> list[Comment]:
    """Find `#` comments, resetting quote state at every newline.

    A quote only shields a `#` on its own line: in YAML, TOML and shell a
    comment runs to end of line, so an unmatched apostrophe in prose must not
    hide every comment that follows it.
    """
    spans: list[tuple[int, int, str, str]] = []
    triple: str | None = None
    for line_no, raw in enumerate(src.splitlines(), 1):
        if triple is not None:
            if triple in raw:
                triple = None
            continue
        quote = next((t for t in triples if t in raw), None)
        if quote is not None and raw.count(quote) % 2 == 1:
            triple = quote
        i, n, in_single, in_double = 0, len(raw), False, False
        while i < n:
            ch = raw[i]
            if ch == "\\" and in_double:
                i += 2
                continue
            if doubled and in_single and raw.startswith(doubled, i):
                i += len(doubled)
                continue
            if ch == "'" and not in_double:
                in_single = not in_single
                i += 1
                continue
            if ch == '"' and not in_single:
                in_double = not in_double
                i += 1
                continue
            if ch == "#" and not in_single and not in_double and (i == 0 or raw[i - 1] in " \t"):
                body = raw[i:]
                if line_no == 1 and body.startswith("#!"):
                    kind = "shebang:"
                elif raw[:i].strip():
                    kind = "trailing"
                else:
                    kind = "line"
                spans.append((line_no, line_no, body, kind))
                break
            i += 1
    return _rows(path, src, spans)


def scan_toml(path: str, src: str) -> list[Comment]:
    return _hash_quote_scan(path, src, ('"""', "'''"), None)


def scan_yaml(path: str, src: str) -> list[Comment]:
    return _hash_quote_scan(path, src, (), "''")


def scan_sh(path: str, src: str) -> list[Comment]:
    return _hash_quote_scan(path, src, (), None)


SCANNERS = {
    ".rs": scan_rust,
    ".py": scan_python,
    ".toml": scan_toml,
    ".yml": scan_yaml,
    ".yaml": scan_yaml,
    ".sh": scan_sh,
}


def scan_file(rel: str) -> list[Comment]:
    suffix = Path(rel).suffix
    scanner = SCANNERS.get(suffix)
    if scanner is None:
        return []
    src = Path(rel).read_text(encoding="utf-8", errors="replace")
    return scanner(rel, src)


def scan_tracked(rels: list[str]) -> list[Comment]:
    """Every comment in the given files, including extensionless shell scripts."""
    out = []
    for rel in rels:
        if Path(rel).suffix in SCANNERS:
            out.extend(scan_file(rel))
        elif rel.startswith(".githooks/"):
            out.extend(scan_sh(rel, Path(rel).read_text(encoding="utf-8", errors="replace")))
    return out


def in_scope(comment: Comment) -> bool:
    """rustdoc and docstrings document an API; directives and a tool-rewritten pin are code."""
    return not (comment.directive or comment.version_pin or comment.kind.endswith(":"))


def scope_suffixes() -> tuple[str, ...]:
    return tuple(SCANNERS)
