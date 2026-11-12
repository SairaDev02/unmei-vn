"""Static checks on the filesystem path a generated index is written to.

`generate` resolves the repository root, then writes each active index at a
repository-relative path. A symbolic link or a Windows junction on that path,
or on a parent component below the root, redirects the write into another file:
inside the repository, outside it, or into a dangling target that did not exist.
The walk here is lexical and static — every prefix below the root is
`lstat`ed — because resolving the destination first would hide the link that is
the hazard.

The scope is that static hazard: concurrent path replacement and hard-link
aliases are not defended, and a refusal promises only that no index was written,
not that an unrelated later I/O failure is rolled back.
"""

import os
import stat

# Windows records a junction as a reparse point, which `S_ISLNK` does not report.
REPARSE_POINT = getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0)


def is_link(path):
    """Whether `path` itself is a symbolic link or a Windows reparse point.

    `os.lstat` reads the entry rather than its target, so a dangling link is
    still reported as a link.
    """
    info = os.lstat(path)
    if stat.S_ISLNK(info.st_mode):
        return True
    return bool(getattr(info, "st_file_attributes", 0) & REPARSE_POINT)


def link_problem(root, destination):
    """`(path, message, remedy)` for the link that would redirect `destination`.

    None means the destination is safe, including when it does not exist: a
    missing index is what generation creates. A component the filesystem does
    not hold ends the walk, because nothing below it exists either.
    """
    current = root
    parts = destination.split("/")
    for depth, part in enumerate(parts, start=1):
        current = current / part
        try:
            linked = is_link(current)
        except (FileNotFoundError, NotADirectoryError):
            return None
        if not linked:
            continue
        if depth == len(parts):
            return (destination,
                    "the index destination is a symbolic link or junction, so writing "
                    "it would rewrite another file",
                    "delete the link and re-run the generate command; %s must be a "
                    "regular file" % destination)
        parent = "/".join(parts[:depth])
        return (destination,
                "parent component '%s' is a symbolic link or junction, so the index "
                "would be written outside it" % parent,
                "replace '%s' with a real directory inside the repository, then re-run "
                "the generate command" % parent)
    return None


def link_problems(root, destinations):
    """Every destination a link would redirect, before the first write."""
    return [problem for problem in
            (link_problem(root, path) for path in sorted(destinations)) if problem]
