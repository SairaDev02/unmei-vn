---
status: accepted
title: CLI diagnostics are categorical; terminal content masks known credentials
summary: Provider diagnostics keep fixed categories only, and every exact occurrence of a configured key is masked in CLI output.
keywords:
  - diagnostics
  - credentials
  - terminal-mask
  - authoring-cli
---

# CLI diagnostics are categorical; terminal content masks known credentials

**Accepted by the maintainer; not implemented by this decision.** The authoring CLI's
diagnostic exception recorded in the credential-surface research is no longer
acceptable for beta: provider errors/degradation details must be categorical,
not retain raw bodies, URLs or uncontrolled decoder/transport strings; credential-
bearing client/config Debug is opaque, bearer headers stay sensitive, wire verbosity
is explicitly off and redirects are disabled. Transient response inspection may still
classify structured-output support, but its returned explanation is safe fixed detail.

Both non-empty recognized environment values (`UNMEI_API_KEY`, `OPENROUTER_API_KEY`)
are captured for the command and their exact literal occurrences are masked throughout
CLI stdout/stderr, including generated previews, dry runs and validation/best-output
reports. Generated terminal content is deliberately not exempt. This trades some
verbatim debugging/prose visibility and redirect compatibility for safer shareable
transcripts; opaque diagnostics alone would not cover intentional model output, while
masking alone would still retain unsafe provider payloads. The detailed contract,
proof obligations and limits are in the [behavioral protection ledger](../testing-behavioral-protection.md).

This is not a general secret scanner: unknown values, fragments, arbitrary encodings
and paraphrases are outside the literal-matching guarantee. Generated files, provider
prompts and correction payloads are not silently rewritten or certified secret-free;
a masked preview does not make a pack safe to share. Review that material before
sharing. [ADR 0010](0010-credentials-live-in-a-user-level-file.md)'s env-only CLI
credential sourcing, the CLI's 120-second timeout and retry classifications remain
unchanged. Implementing and validating this accepted policy is required before the
authoring CLI's beta item is complete, and belongs to a separately authorized
build effort.

**Implementation status (2026-10-18).** The **provider half** landed first:
retained provider errors and structured-degradation reasons are fixed categories,
credential-bearing client/config `Debug` is opaque, the bearer header stays
sensitive, wire verbosity is explicitly off and redirects are not followed. The
**terminal half** followed: both non-empty recognized values are snapshotted
before argument parsing and every exact literal occurrence is replaced with the
fixed marker `[redacted]` across the command's
stdout and stderr — Clap's argument/help rendering, the final `✖ {err}` line,
`--list`, generated previews and dry runs, exhaustion/best-output reports,
interactive prompts and the `env_logger` sink. Masking is deliberately not confined
to the runner's `Terminal`. A value split across formatting or write chunks is
masked whole: every writer on one stream shares that stream's buffer (the
`env_logger` sink shares stderr), holding a trailing suffix that a longer value
could still complete until it resolves and deliberately not draining it on
`flush`, so an explicit flush cannot split a literal. The command finalizes both
buffers after its last write; a literal split across the two different streams is
outside the rule. The guarantee stays an exact-literal match:
a recognized value that is itself a substring of the fixed marker is not
certified absent, and a dependency writing straight to the process's stderr rather
than through `env_logger`, OS input echo, unknown values, fragments, encodings and
paraphrases are outside it. Generated files, provider prompts and correction
payloads are still neither rewritten nor certified secret-free — a masked preview
is not a safe-file guarantee. Clap's help and argument rendering keeps its ordinary
(TTY-styled) output when no recognized value is configured; with one configured it is
re-rendered through the mask in plain form, since the styled variant is printed by
Clap straight to the process streams. `RUST_LOG` is parsed with `env_filter`'s
fallible, non-printing API: a malformed spec is ignored and reported through the
masked logger sink instead of `env_filter`'s own stderr warning, while a valid spec
is applied as before.
