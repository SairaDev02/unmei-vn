#!/usr/bin/env python3
"""The provenance record an export commit's message carries.

An export commit's message is the record a later export and the published
candidate read. Its subject numbers the export and names the exported version,
one fixed sentence says what the tree is, and three plain lines name the private
source commit, the reviewed allowlist's digest and the exported manifest's
digest. Nothing else belongs in it: no co-author and no sign-off trailer,
because a reader has to be able to take the record at face value rather than
parse it hopefully.

The format is public, because it is in every commit of the published history, so
it lives with the shipped scripts and both readers import it: the export tool
writes the record and reads the target's tip back through it, and the candidate
record copies a candidate commit's record into its own `provenance` object. The
shape is therefore decided once, and a message this module would not write is
not read as an export by either reader.
"""

from __future__ import annotations

import re
from dataclasses import dataclass

ABOUT = "This tree is an export from a private development repository."
LABELS = ("Source commit", "Allowlist digest", "Manifest digest")
SUBJECT = re.compile(r"^Export (\d+): Unmei (\S+)$")


@dataclass(frozen=True)
class Record:
    """What an export commit's message records, field for field."""

    number: int
    version: str
    source: str
    allowlist_digest: str
    manifest_digest: str

    @property
    def subject(self):
        """The commit subject: which export this is, and which version it ships."""
        return "Export %d: Unmei %s" % (self.number, self.version)

    def message(self):
        """The whole commit message, which is the record a reader copies."""
        lines = [self.subject, "", ABOUT]
        for label, value in zip(LABELS, (self.source, self.allowlist_digest,
                                         self.manifest_digest)):
            lines.append("%s: %s" % (label, value))
        return "\n".join(lines) + "\n"


def parse(text):
    """The record `text` carries, or None when `text` is not an export's message.

    Nothing is guessed. The subject, the fixed sentence and the three labelled
    lines have to be present, in that order, with nothing else; a message an
    editor extended or a hand written commit therefore reads as no export at
    all, which is what makes the tip's message usable as the previous export and
    a candidate commit's message usable as provenance.
    """
    lines = [line for line in text.splitlines() if line.strip()]
    if len(lines) != len(LABELS) + 2:
        return None
    subject = SUBJECT.match(lines[0])
    if subject is None or lines[1] != ABOUT:
        return None
    values = []
    for label, line in zip(LABELS, lines[2:]):
        prefix = label + ": "
        if not line.startswith(prefix):
            return None
        values.append(line[len(prefix):].strip())
    return Record(int(subject.group(1)), subject.group(2), *values)
