---
title: Version-matched documentation for the locked dependencies
summary: A map to the official documentation that matches each locked dependency, with the source-selection order and the refresh duty.
keywords:
  - external-dependency
  - navigation-tooling
---
# Documentation for the locked dependencies

Use this page as a map, not as a reading list for every task. The repository's contracts define intended Unmei behavior. Version-matched official documentation and upstream source define how a dependency behaves.

Check `Cargo.lock`, the relevant crate's `Cargo.toml` and its enabled features, and `rust-toolchain.toml` first. The versions below match the lockfile when this page was last refreshed. If the lockfile disagrees, the lockfile wins, and this page needs a refresh. Do not assume that examples for the latest release apply.

## Rust workspace

- Toolchain pinned in `rust-toolchain.toml`: **Rust 1.98.1**.
- [The Rust Reference](https://doc.rust-lang.org/reference/)
- [The Rust standard library](https://doc.rust-lang.org/std/)
- [The Cargo Book](https://doc.rust-lang.org/cargo/)

## Bevy runtime and audio (`app/`)

- **Bevy 0.19.1:** [API](https://docs.rs/bevy/0.19.1/bevy/)
- **bevy_kira_audio 0.26.0:** [API](https://docs.rs/bevy_kira_audio/0.26.0/bevy_kira_audio/)
- **winit 0.30.13** (transitive): [API](https://docs.rs/winit/0.30.13/winit/)

Prefer the installed crate source when a symbol or behavior stays ambiguous. Bevy's scene, scheduling and text-input APIs change between releases, so do not copy a pattern from an older release without verifying it.

## Content format, serialization and saves (`engine/`)

- **RON 0.10.1**, the version the workspace crates use: [API](https://docs.rs/ron/0.10.1/ron/). The lockfile also holds a newer RON that only Bevy uses.
- [Serde](https://serde.rs/)
- Unmei contracts: [`docs/content-format.md`](../content-format.md), [ADR 0001](../adr/0001-ron-for-structured-content.md), [ADR 0006](../adr/0006-save-slots-on-disk-format-and-version-policy.md) and [ADR 0007](../adr/0007-settings-file-beside-the-pack.md).

Bevy's reflected world serialization does not replace the engine's Bevy-free save format.

## HTTP, async runtime and CLI

- **reqwest 0.12.28:** [API](https://docs.rs/reqwest/0.12.28/reqwest/)
- **Tokio 1.54.1:** [API](https://docs.rs/tokio/1.54.1/tokio/)
- **Clap 4.6.7:** [API](https://docs.rs/clap/4.6.7/clap/)
- CLI contracts: [`docs/author-cli.md`](../author-cli.md) and [`docs/author-cli-drafting.md`](../author-cli-drafting.md).

Check the enabled features, including the engine's optional `network` feature, before you attribute a behavior to a dependency.

## NanoGPT, the one provider

- [`docs/nanogpt.md`](../nanogpt.md) owns the provider requirements and links the official NanoGPT API reference for each endpoint Unmei calls.
- Architecture: [ADR 0008](../adr/0008-player-first-failure-posture.md), [ADR 0009](../adr/0009-decision-resolver-seam.md), [ADR 0010](../adr/0010-credentials-live-in-a-user-level-file.md) and [ADR 0015](../adr/0015-nanogpt-is-the-one-provider.md).

When you investigate API drift, re-check the **current** official wire schema. Do not change the runtime's 30-second, no-retry, authored-fallback behavior, and do not add a provider abstraction, without revisiting the ADR that owns it. Never expose a credential in a log or a report. Routine tests must not need live API access.

## Source selection

1. **Repository behavior and invariants:** the specs, the glossary, the ADRs and the tests.
2. **Dependency and protocol behavior:** official documentation that matches the resolved versions, the enabled features and the current upstream API contract.
3. **Missing or ambiguous documentation:** upstream implementation, release notes and the locally cached crate source.
4. **Secondary sources:** maintainer explanations and community discussions, which corroborate authoritative evidence and never override it.

When a dependency is upgraded, refresh this page in the same change and re-check the API assumptions it affects.
