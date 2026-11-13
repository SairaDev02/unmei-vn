# Contributing to Unmei

This page explains how to report an issue and how an accepted issue becomes a change to Unmei. It also covers the checks to run on your clone before you commit.

## How to report an issue

- **Bugs:** open a [bug report](.github/ISSUE_TEMPLATE/bug_report.yml) with steps to reproduce.
- **Features and ideas:** open a [feature request](.github/ISSUE_TEMPLATE/feature_request.yml).
- **Questions and open-ended ideas:** open a [question](.github/ISSUE_TEMPLATE/question.yml).
- **Security vulnerabilities:** do not open a public issue. Follow [SECURITY.md](SECURITY.md) and use GitHub's private vulnerability reporting.

An issue starts with the `needs-triage` label when it is opened. A maintainer triages each issue into one of these labels:

| Label | Meaning |
| --- | --- |
| `needs-triage` | Maintainer needs to evaluate this issue |
| `needs-info` | Waiting on reporter for more information |
| `ready-for-agent` | Fully specified, ready for an automated agent |
| `ready-for-human` | Requires human implementation |
| `wontfix` | Will not be actioned |

A maintainer writes a first response within 7 days of the issue opening. [Contribution paths](#contribution-paths) states what makes an issue accepted.

## Contribution paths

Issues come first. Report the problem with the templates in [How to report an issue](#how-to-report-an-issue) and state the expected behavior or the acceptance criteria. Maintainer triage decides whether the work proceeds and how.

An issue is accepted when it carries `ready-for-agent` or `ready-for-human`. Acceptance means the maintainer has written the observable problem, acceptance criteria that a test can check and what is out of scope.

Pull requests are accepted only from invited collaborators, and only against an accepted issue.

A submission is judged on the work itself: the diff, the tests and the validation evidence. The submitter is responsible for what they submit.

A maintainer decides the outcome of each pull request. The maintainer accepts it when it is validated, requests revisions when the problems are fixable, or declines the implementation. When a maintainer declines an implementation, the maintainer retains its useful problem or specification in an issue, with an explanation and appropriate credit. A retained proposal goes through issue triage and carries no promise of implementation.

The maintainer's side of this process, including the two review gates, triage, the trust ladder, the closing timeouts and the saved replies, is in [`docs/maintaining.md`](docs/maintaining.md).

## Development workflow

A pull request follows these rules:

- Link the accepted issue in the `Closes #<n>` line of the description.
- Change only what the issue requires, and keep the pull request to one logical change so that it stays reviewable.
- Include a test that fails without the change.
- State in the Validation section of the [pull request template](.github/PULL_REQUEST_TEMPLATE.md) what you ran, what you observed and any check you did not run.

The maintainer closes a pull request that breaks these rules without a review. These rules judge the process, not the person. Repeated violations lead to a block.

Branch from `main` with a short, descriptive name, such as `fix/foo`, `feat/bar` or `docs/baz`. Pull requests are squash-merged into `main`. Write commit messages that read as a clean changelog entry.

## Commit trailers

- `Co-authored-by:` in the message of a commit that fixes an issue, to credit the reporter whose issue led to the fix.

This trailer binds the maintainer's own commits in this repository. Export commits, the commits that bring a released source tree into this repository, carry no trailer.

## Code and docs conventions

- Use the domain vocabulary that [`GLOSSARY.md`](GLOSSARY.md) defines. Do not invent synonyms.
- If your change contradicts an [architecture decision record (ADR)](docs/adr/), say so in the PR instead of silently overriding the record.
- To write or review a reader-facing doc, follow [`LANGUAGE.md`](LANGUAGE.md).

## Licensing

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE) that covers this project.

## Requirements for the local checks

Run the commands on this page from the repository root, in a POSIX shell. On Windows, use Git Bash. You need Git and Python 3. The scripts use only the Python standard library. The commands call Python as `python3`. If your system has no `python3` command, use `python`. Commands that build or test also need the Rust toolchain that `rust-toolchain.toml` pins. The [Local environment doctor](#local-environment-doctor) checks that toolchain.

## Local environment doctor

Before a fresh clone builds, tests or hands work to an agent, run the read-only environment doctor:

```sh
python3 scripts/doctor.py
```

The doctor resolves the repository from its own location, so you can run it from any directory. It prints one line per prerequisite, starting with `PASS`, `WARN`, `FAIL` or `SKIP`, then a count of each. It checks whether:

- the pinned Rust toolchain (`rust-toolchain.toml`) and its `rustfmt` and `clippy` components are installed;
- `core.hooksPath` resolves to the committed `.githooks` directory;
- on Linux, the native build libraries that [`.github/workflows/check.yml`](.github/workflows/check.yml) installs are present.

Every `WARN` or `FAIL` line is followed by the command or document that addresses it.

The doctor only reads the repository. It installs nothing, writes no tracked file and updates no Git configuration.

The exit status is:

- `0` when nothing blocks a build. Warnings and skipped checks do not block.
- `1` when a blocking prerequisite is missing or the running Rust toolchain disagrees with the version that `rust-toolchain.toml` pins.
- `2` when the doctor itself could not run.

To check that the doctor classifies each situation correctly, run:

```sh
python3 scripts/doctor.py --self-test
```

The self-test simulates missing and mismatched tools, so it does not touch your environment or build anything. The self-test exits with `1` if any case fails.

## Local pre-commit checks

Unmei provides an optional Git hook that checks staged changes before each commit. It needs Git, a POSIX shell and Python 3. The hook scripts use only the Python standard library. The hook uses `python3`, or `python` when `python3` is not on `PATH`. To enable it, run this once per clone:

```sh
git config core.hooksPath .githooks
```

The hook runs these checks:

- `git diff --cached --check` for staged whitespace errors.
- A size limit on staged, authored production source files: it rejects a file larger than 5,000 tokens, where tokens = staged Git blob bytes / 4, so at most 20,000 bytes per file. The hook inspects only staged additions, copies, modifications, renames and type changes. Unstaged edits, untracked files, deletions, symlinks and submodules do not count.
- `cargo fmt --all -- --check`, when the staged additions, copies, modifications or renames include a `.rs` file. A commit with no staged `.rs` file, such as a documentation-only commit, skips Cargo.
- The documentation structure checks in [Documentation structure checks](#documentation-structure-checks). First a bootstrap rejects a validator whose working-tree copy differs from the staged one. Then the hook validates the staged snapshot with that same implementation. Neither step writes to the index or the working tree.

The checks do not edit files or stage changes. Fix the reported issues and retry the commit.

The size limit exempts test code, documentation, and generated or vendored files:

- Test code: `tests/`, `test/` and `__tests__/` directories, and test modules such as `app/src/tests.rs`, `test_*.py`, `*_test.rs`, `*_tests.rs`, `*.test.*` and `*.spec.*`.
- Documentation: `doc/`, `docs/` and `documentation/` directories, files with `.md`, `.mdx`, `.rst`, `.adoc`, `.asciidoc` or `.txt` suffixes, and extensionless project documentation names such as `README` and `CHANGELOG`.
- Lockfiles, generated and codegen directories, vendored directories, generated source suffixes, generated CLI fixtures, snapshots and common binary assets.

These exemptions affect only the token-size check, not the staged whitespace check or Rust formatting. The precise patterns live in `scripts/check-staged-token-limit.py`. Production files that are already oversized are not checked until you stage them. The whole-tree counterpart is `scripts/audit-source-size.py`.

A file may exceed the limit only when a maintainer decision is recorded in `scripts/source-size-exceptions.json`. The record holds the file path, the byte count the decision authorized and a reason. The hook and the audit read that one list. The recorded count is a **ceiling**: a file that grows past it is rejected again. The audit reports an exception that no longer describes an oversized file, so that you can remove it.

A violation lists each file, its staged size and its estimated tokens. It then explains how to extract cohesive source modules, update imports and tests, stage the resulting files and retry. To run the focused regression suite for the size check, run:

```sh
python3 scripts/test-staged-token-limit.py
```

The root `.gitattributes` pins `.githooks/*` and `*.sh` to `text eol=lf`. Git therefore checks the hook out with LF endings on Windows, Git Bash and Linux, even when `core.autocrlf` is enabled. A clone made before `.gitattributes` held that pin can still have a CRLF working copy of the hook, which the shell cannot run. The committed hook already has LF endings, so refreshing the hook file changes no tracked content. The refresh discards any uncommitted edit to the hook, so commit or stash the edit first. Then run:

```sh
rm .githooks/pre-commit && git checkout -- .githooks/pre-commit
```

To disable the repository hook, run the following command. If you used another hook directory before you enabled this one, restore your previous `core.hooksPath` value instead.

```sh
git config --unset core.hooksPath
```

The whitespace check reads the staged content. `cargo fmt` reads the working-tree Rust files, not the staged snapshot. When you stage only part of a Rust file, check the staged patch yourself. Continuous integration (CI) remains the authoritative gate for formatting, Clippy and the workspace tests.

Before you push, run focused local checks for the behavior you changed. CI runs formatting, Clippy and the workspace tests on every push and on each PR into `main` ([`.github/workflows/check.yml`](.github/workflows/check.yml)). A PR that changes only workflows or documentation does not need a separate local full-suite run.

## Documentation structure checks

The documentation tooling is `scripts/docs.py`, which uses only the Python standard library. CI runs it in the `check` job immediately after checkout. [`scripts/docs-policy.json`](scripts/docs-policy.json) is the one policy: it owns the metadata fields, the approved keyword vocabulary, the coverage rules and the index shapes. This section names the commands and the authoring order, not those values.

```sh
python3 scripts/docs.py check              # the working tree
python3 scripts/docs.py check --staged     # the Git index, as the hook runs it
python3 scripts/docs.py check --checkout   # a committed tree, HEAD unless --rev
python3 scripts/docs.py coverage           # one class per tracked Markdown path
python3 scripts/docs.py generate           # write the expected index bytes
for suite in scripts/test-docs-*.py; do python3 "$suite"; done   # the tooling's own test suites
```

To author a document, follow these steps in order:

1. Write or edit the document and its frontmatter.
2. Run `git add` on the document, so that the generated index can cite it.
3. Run `python3 scripts/docs.py generate`.
4. Run `git add <directory>/INDEX.md` for every index that the command rewrote.
5. Before you commit, run `python3 scripts/docs.py check --staged`. The command exits with `0` when every check passes.

`check` is read-only. It compares the expected bytes with the snapshot it was asked for. `generate` writes only the configured index paths. A failure names the repository-relative path, the rule it broke and the command that fixes it, so read its `fix:` line instead of guessing. `.gitattributes` pins generated indexes to LF line endings, and `check` compares them byte for byte. Do not edit an index by hand; run `generate` instead.

## Local workflow linting

CI lints the GitHub Actions workflows with [`actionlint`](https://github.com/rhysd/actionlint) in the `actionlint` job. The job runs alongside the Rust gate and compiles nothing. It pins actionlint 1.7.12, ShellCheck 0.11.0 and Ruff 0.17.4, and `.github/workflows/check.yml` records each release asset and its SHA-256.

The same job lints the gate's own tooling: Ruff's `E4,E7,E9,F` rules over `scripts/doctor.py`, `scripts/check-invariants.py` and the invariant modules beside it, and POSIX ShellCheck over `.githooks/pre-commit`.

actionlint also checks the shell inside every `run:` script through [ShellCheck](https://www.shellcheck.net/). To run the same check, install actionlint 1.7.12 and ShellCheck 0.11.0, put both on `PATH` and run this from the repository root:

```sh
actionlint -color -pyflakes=
```

With no file arguments, actionlint finds and lints every `.github/workflows/*.yml` and `*.yaml` file, including `check.yml` and `boot.yml`. The command disables Pyflakes (`-pyflakes=`) because the pinned actionlint release does not bundle it. Inline Python needs a separate syntax and targeted check. The repository invariants run from `scripts/check-invariants.py`. If ShellCheck is not installed, actionlint skips its shell rules without a message, so a clean run without ShellCheck checks less than CI does.

### Optional: run the same linters locally

Nothing here is required to contribute. These are the checks CI runs, so you can find a lint failure without a push. Install the pinned versions, or use the versions your platform packages, which can differ from the pinned ones. Then run, from the repository root:

```sh
ruff check --no-cache --isolated --select E4,E7,E9,F \
  scripts/doctor.py scripts/check-invariants.py \
  scripts/invariants_core.py scripts/invariants_eol.py scripts/license_policy.py
shellcheck -s sh .githooks/pre-commit
```

Ruff only reads. The `E4,E7,E9,F` rules cover import, syntax and Pyflakes defects, and the command has no `--fix`. The options `--isolated --no-cache` make the result depend only on the Ruff binary and the file contents. `check.yml` owns the rule list. Change the list there first.

## Local repository invariant checks

CI enforces four stable architecture invariants in the `check` job, before `fmt`, Clippy and the workspace tests, through [`scripts/check-invariants.py`](scripts/check-invariants.py):

- the `unmei-engine` crate pulls no Bevy package in any feature or target configuration;
- no tracked file lives under `app/prototype/`;
- no tracked text file is stored or checked out with CRLF line endings;
- `deny.toml` accepts exactly the licenses in the `accepted` list of `packaging/about.toml`.

To run the same checks, run this from the repository root:

```sh
python3 scripts/check-invariants.py
```

The command exits with `0` when every invariant holds, `1` when an invariant is violated and `2` when the checker itself could not run. A rule failure names the invariant and the offending dependency, path or license. A stale `Cargo.lock` fails the checker earlier, with Cargo's own message. Each rule reads these inputs:

- **Engine rule:** Cargo's resolved dependency graph (`cargo tree --locked -p unmei-engine`). The rule compiles nothing, and a stale lockfile makes it refuse to run instead of rewriting the lockfile.
- **Path rule:** the Git index (`git ls-files`), so an ignored local directory does not fail it.
- **Line-ending rule:** what `git ls-files --eol` reports for the index and the working tree, so a checkout with CRLF endings fails here instead of inside `cargo package`. Files marked `-text` in `.gitattributes` keep their bytes on purpose and are exempt.
- **License rule:** the two committed license lists. It fails when they differ.

To prove that each rule can fail, run this:

```sh
python3 scripts/check-invariants.py --self-test
```

The self-test feeds each rule a controlled invalid input and a clean one, and requires rejection and acceptance. CI runs it beside the real check. It uses no third-party framework and builds nothing.

The engine, line-ending and license rules run on every branch and PR. CI runs the path rule only for a push to `main` or a PR into it. The options `--rules engine`, `--rules eol`, `--rules licenses` and `--rules paths` select the groups individually.
