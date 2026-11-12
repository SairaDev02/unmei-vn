"""Restricted frontmatter parser and schema validation for authored documents.

This module owns the grammar and the schema. Syntax and schema are decided in
that order: a value that does not resolve to a permitted string is a syntax
error before any schema check runs, so `keywords: []` is syntactically valid and
fails its schema.

Standard library only (`re` is the single import). The pre-commit bootstrap
(`docs_guard`) imports this module and must keep that no-dependency contract;
the frozen 88-case corpus in `test-docs-parser.py` is what holds the grammar
still.
"""

import re
from dataclasses import dataclass, field

INDICATOR_ALWAYS = set('#&*!|>\'"%@`]}')
INDICATOR_CONDITIONAL = set('-?:')

# YAML 1.2.2 section 10.3.2 core tag resolution: these patterns decide whether a
# plain scalar is a string. Anything else would be a null, boolean or number.
NULL_RE = re.compile(r'^(?:null|Null|NULL|~)?$')
BOOL_RE = re.compile(r'^(?:true|True|TRUE|false|False|FALSE)$')
INT_RE = re.compile(r'^(?:[-+]?[0-9]+|0o[0-7]+|0x[0-9a-fA-F]+)$')
FLOAT_RE = re.compile(
    r'^(?:[-+]?(?:\.[0-9]+|[0-9]+(?:\.[0-9]*)?)(?:[eE][-+]?[0-9]+)?'
    r'|[-+]?\.(?:inf|Inf|INF)|\.(?:nan|NaN|NAN))$'
)

KEY_RE = re.compile(r'^([A-Za-z][A-Za-z0-9_-]*):(?:\s+(.*))?$')
SPACED_KEY_RE = re.compile(r'^([A-Za-z][A-Za-z0-9_-]*)\s*:')
INDENTED_KEY_RE = re.compile(r'^([A-Za-z][A-Za-z0-9_-]*):')
ITEM_RE = re.compile(r'^ {2}-(?: (.*))?$')
KEYWORD_RE = re.compile(r'^[a-z0-9]+(?:-[a-z0-9]+)*$')
SCHEME_RE = re.compile(r'^[A-Za-z][A-Za-z0-9+.-]*:')
HEX4_RE = re.compile(r'^[0-9A-Fa-f]{4}$')


@dataclass(frozen=True)
class Diagnostic:
    """One reason the document is not valid, positioned in the document."""

    stage: str          # "syntax" is decided before "schema"
    field: str | None   # the frontmatter field the reason belongs to
    line: int
    reason: str
    detail: str | None = None   # the offending value, when naming it helps the remedy

    def __str__(self) -> str:
        named = self.field or "frontmatter"
        return "%s (line %d): %s [%s]" % (named, self.line, self.reason, self.stage)


@dataclass
class ParseResult:
    values: dict = field(default_factory=dict)
    errors: list = field(default_factory=list)
    # 1-based line of the closing fence, so a remedy can say where to insert.
    end_line: int = 1

    @property
    def ok(self) -> bool:
        return not self.errors


def _control_reason(text):
    for ch in text:
        code = ord(ch)
        if code < 0x20 or 0x7F <= code <= 0x9F or code in (0x2028, 0x2029):
            return 'control-character'
    return None


def _decode_quoted(raw, line, field_name, errors):
    out = []
    i = 1
    closed = False
    while i < len(raw):
        ch = raw[i]
        if ch == '"':
            closed = True
            i += 1
            break
        if ch == '\\':
            nxt = raw[i + 1] if i + 1 < len(raw) else ''
            if nxt in '"\\/':
                out.append(nxt)
                i += 2
                continue
            if nxt == 'u':
                hex4 = raw[i + 2:i + 6]
                if not HEX4_RE.match(hex4):
                    errors.append(Diagnostic('syntax', field_name, line, 'malformed-unicode-escape'))
                    return None
                code = int(hex4, 16)
                if 0xD800 <= code <= 0xDFFF:
                    errors.append(Diagnostic('syntax', field_name, line, 'surrogate-escape'))
                    return None
                out.append(chr(code))
                i += 6
                continue
            errors.append(Diagnostic('syntax', field_name, line, 'unsupported-escape'))
            return None
        out.append(ch)
        i += 1
    if not closed:
        errors.append(Diagnostic('syntax', field_name, line, 'unterminated-quoted-value'))
        return None
    if raw[i:].strip():
        errors.append(Diagnostic('syntax', field_name, line, 'trailing-after-quote'))
        return None
    value = ''.join(out)
    reason = _control_reason(value)
    if reason:
        errors.append(Diagnostic('syntax', field_name, line, reason))
        return None
    return value


def _decode_plain(raw, line, field_name, errors):
    value = raw.rstrip(' ')
    if not value:
        errors.append(Diagnostic('syntax', field_name, line, 'empty-value'))
        return None
    reason = _control_reason(value)
    if reason:
        errors.append(Diagnostic('syntax', field_name, line, reason))
        return None
    if value[0] in INDICATOR_ALWAYS:
        errors.append(Diagnostic('syntax', field_name, line, 'indicator-start'))
        return None
    if value[0] in INDICATOR_CONDITIONAL and (len(value) == 1 or value[1] == ' '):
        errors.append(Diagnostic('syntax', field_name, line, 'indicator-start'))
        return None
    for ch in '#:\\"':
        if ch in value:
            errors.append(Diagnostic('syntax', field_name, line, 'requires-quotes'))
            return None
    if NULL_RE.match(value) or BOOL_RE.match(value) or INT_RE.match(value) or FLOAT_RE.match(value):
        errors.append(Diagnostic('syntax', field_name, line, 'core-type-not-string'))
        return None
    return value


def _decode_scalar(raw, line, field_name, errors):
    if raw.startswith('"'):
        return _decode_quoted(raw, line, field_name, errors)
    return _decode_plain(raw, line, field_name, errors)


def parse_frontmatter(text, policy, owner_dir):
    """Parse and validate the frontmatter of one authored document.

    `owner_dir` is the document's directory relative to the repository root; it
    is the basis `related` entries are resolved and escape-checked against.
    """
    result = ParseResult()
    errors = result.errors
    if text.startswith('\ufeff'):
        text = text[1:]
    lines = text.replace('\r\n', '\n').replace('\r', '\n').split('\n')
    if not lines or lines[0] != '---':
        errors.append(Diagnostic('syntax', None, 1, 'missing-frontmatter'))
        return result
    closing = next((i for i in range(1, len(lines)) if lines[i] == '---'), None)
    if closing is None:
        errors.append(Diagnostic('syntax', None, 1, 'unterminated-frontmatter'))
        return result
    result.end_line = closing + 1

    mapping = result.values
    key_lines = {}
    pending_list = None       # field awaiting block-list items
    pending_line = 0
    current = None

    def close_pending():
        nonlocal pending_list
        if pending_list is not None and not mapping.get(pending_list):
            # A key with no inline value opens a block list; the next key, or the
            # closing fence, arriving with no items means the list stayed empty.
            errors.append(Diagnostic('syntax', pending_list, pending_line, 'empty-list-value'))
        pending_list = None

    for offset in range(1, closing):
        line = lines[offset]
        lineno = offset + 1
        if '\t' in line:
            named = INDENTED_KEY_RE.match(line.lstrip())
            errors.append(Diagnostic(
                'syntax', named.group(1) if named else current, lineno, 'tab-character'))
            continue
        if not line.strip():
            continue
        if line.lstrip().startswith('#'):
            continue
        if line.startswith(' '):
            item = ITEM_RE.match(line)
            if item:
                if pending_list is None:
                    errors.append(Diagnostic('syntax', current, lineno, 'list-without-key'))
                    continue
                raw_item = item.group(1)
                raw_item = raw_item.lstrip(' ') if raw_item is not None else None
                if raw_item is None or raw_item.strip() == '':
                    errors.append(Diagnostic('syntax', pending_list, lineno, 'empty-list-item'))
                    continue
                value = _decode_scalar(raw_item, lineno, pending_list, errors)
                if value is not None:
                    if mapping.get(pending_list) is None:
                        mapping[pending_list] = []
                    mapping[pending_list].append(value)
                continue
            close_pending()
            errors.append(Diagnostic('syntax', current, lineno, 'indented-line'))
            indented_key = INDENTED_KEY_RE.match(line.lstrip())
            if indented_key:
                errors.append(Diagnostic('syntax', indented_key.group(1), lineno, 'indented-key'))
            continue

        key_line = KEY_RE.match(line)
        if not key_line:
            close_pending()
            spaced = SPACED_KEY_RE.match(line)
            errors.append(Diagnostic(
                'syntax', spaced.group(1) if spaced else current, lineno, 'expected-key'))
            continue
        close_pending()
        key, raw_value = key_line.group(1), key_line.group(2)
        current = key
        key_lines.setdefault(key, lineno)
        if key in mapping:
            errors.append(Diagnostic('syntax', key, lineno, 'duplicate-key', key))
            continue
        if raw_value is None:
            mapping[key] = None
            pending_list = key
            pending_line = lineno
            continue
        if raw_value.startswith('['):
            if raw_value.strip() == '[]':
                mapping[key] = []
                continue
            errors.append(Diagnostic('syntax', key, lineno, 'flow-collection'))
            continue
        if raw_value[:1] in ('|', '>'):
            errors.append(Diagnostic('syntax', key, lineno, 'block-scalar'))
            continue
        if raw_value[:1] == '{':
            errors.append(Diagnostic('syntax', key, lineno, 'flow-mapping'))
            continue
        if raw_value[:1] in ('&', '*', '!', '%', '@', '`', "'", ']', '}'):
            errors.append(Diagnostic('syntax', key, lineno, 'indicator-start'))
            continue
        value = _decode_scalar(raw_value, lineno, key, errors)
        if value is not None:
            mapping[key] = value
    close_pending()

    failed_fields = {err.field for err in errors if err.stage == 'syntax'}
    validate_schema(
        mapping,
        errors,
        policy,
        owner_dir=owner_dir,
        key_lines=key_lines,
        failed_fields=failed_fields,
    )
    return result


def validate_schema(mapping, errors, policy, owner_dir, key_lines=None,
                    failed_fields=frozenset()):
    key_lines = key_lines or {}

    def report(field_name, reason, detail=None):
        errors.append(Diagnostic('schema', field_name, key_lines.get(field_name, 2), reason, detail))

    allowed = list(policy['fields'])
    for key in mapping:
        if key not in allowed:
            report(key, 'unknown-field', key)
    for required in policy['required']:
        if required not in mapping and required not in failed_fields:
            report(required, 'missing-field')

    title = mapping.get('title')
    if isinstance(title, str) and not title.strip():
        report('title', 'empty-title')
    elif title is not None and not isinstance(title, str):
        report('title', 'wrong-type')

    summary = mapping.get('summary')
    if isinstance(summary, str):
        if not summary.strip():
            report('summary', 'whitespace-only-summary')
        if len(summary) > policy.get('summary_max_scalars', 160):
            report('summary', 'summary-too-long')
        if any(0xD800 <= ord(ch) <= 0xDFFF for ch in summary):
            report('summary', 'unpaired-surrogate')
    elif summary is not None:
        report('summary', 'wrong-type')

    keywords = mapping.get('keywords')
    if keywords is not None:
        if not isinstance(keywords, list):
            report('keywords', 'wrong-type')
        else:
            if not keywords:
                report('keywords', 'empty-keywords')
            if len(keywords) > 8:
                report('keywords', 'too-many-keywords')
            seen = set()
            allowlist = set(policy['keyword_allowlist'])
            for term in keywords:
                if not isinstance(term, str) or not term:
                    report('keywords', 'invalid-keyword')
                    continue
                if term in seen:
                    report('keywords', 'duplicate-keyword', term)
                seen.add(term)
                if not KEYWORD_RE.match(term):
                    report('keywords', 'keyword-spelling', term)
                elif term not in allowlist:
                    report('keywords', 'keyword-not-approved', term)

    status = mapping.get('status')
    if status is not None:
        if not isinstance(status, str):
            report('status', 'wrong-type')
        elif status == '':
            report('status', 'empty-status')

    related = mapping.get('related')
    if related is not None:
        if not isinstance(related, list):
            report('related', 'wrong-type')
        else:
            for path in related:
                reason = related_path_reason(path, owner_dir)
                if reason:
                    report('related', reason, path if isinstance(path, str) else None)
    return errors


def related_path_reason(path, owner_dir):
    """Why a `related` entry is not a valid repository-local path, if it is not."""
    if not isinstance(path, str) or not path:
        return 'invalid-path'
    if '\\' in path:
        return 'backslash-path'
    if SCHEME_RE.match(path) or path.startswith('//'):
        return 'url-path'
    if path.startswith('/'):
        return 'absolute-path'
    segments = path.split('/')
    if '' in segments[:-1]:
        return 'empty-segment'
    depth = len([part for part in owner_dir.split('/') if part])
    for segment in segments:
        if segment == '..':
            depth -= 1
            if depth < 0:
                return 'repository-escape'
        elif segment in ('', '.'):
            continue
        else:
            depth += 1
    return None


def resolve_related(owner_dir, path):
    """The repository-relative POSIX path a `related` entry points at."""
    parts = [part for part in owner_dir.split('/') if part]
    for segment in path.split('/'):
        if segment == '..':
            if parts:
                parts.pop()
        elif segment in ('', '.'):
            continue
        else:
            parts.append(segment)
    return '/'.join(parts)
