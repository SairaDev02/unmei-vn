# Agent instructions for Unmei

Follow [CONTRIBUTING.md](CONTRIBUTING.md). It owns the contribution rules, and nothing below adds a rule or overrides it.

This file only routes a coding agent to the pages that hold its operating instructions. Every page is under [`docs/agents/`](docs/agents/), and [`docs/agents/INDEX.md`](docs/agents/INDEX.md) lists them.

- **Before you change behavior:** read the accepted issue you are working on. The code defines current behavior. For an authorized change, the issue's acceptance criteria and the specs it cites define the intended behavior. [`docs/architecture.md`](docs/architecture.md) maps each contract to the page that owns it.
- **Vocabulary and design records:** [domain.md](docs/agents/domain.md), which routes to [`GLOSSARY.md`](GLOSSARY.md) and [`docs/adr/`](docs/adr/).
- **Tests and verification:** [testing.md](docs/agents/testing.md) and the `unmei-verify` skill at [`.agents/skills/unmei-verify/SKILL.md`](.agents/skills/unmei-verify/SKILL.md). [`docs/testing.md`](docs/testing.md) is the testing authority.
- **Reader-facing docs:** [`LANGUAGE.md`](LANGUAGE.md).
- **Code comments:** [comments.md](docs/agents/comments.md), before you add, reword or delete one, and when `scripts/no-new-comments.py` fails.
- **Dependency facts:** [documentation.md](docs/agents/documentation.md).
- **A fix that stops converging:** [recovery.md](docs/agents/recovery.md), after two failed attempts or when an external behavior is uncertain.
- **Issues and the `gh` CLI:** [github.md](docs/agents/github.md), [issue-tracker.md](docs/agents/issue-tracker.md) and [triage-labels.md](docs/agents/triage-labels.md).
- **Branches, scratch files and prototypes:** [branching.md](docs/agents/branching.md) and [temp-files.md](docs/agents/temp-files.md).
- **Tools, fallbacks and delegation:** [tooling.md](docs/agents/tooling.md).
- **Finishing an implementation issue:** [closing-comment.md](docs/agents/closing-comment.md), which also holds the end-of-task report.

A runtime with no skill launcher reads a skill as its `.agents/skills/<name>/SKILL.md` file.
