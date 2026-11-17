---
name: unmei-verify
description: Choose and run the smallest meaningful verification for an Unmei change (engine, CLI, Bevy player, shipping schedule, or UI and rendering) and report what was observed. Use before claiming any change works or is done.
---

# Verify an Unmei change

[`docs/testing.md`](../../../docs/testing.md) is the authority for what to test, how to write it and the CI gate. This skill only routes you to the smallest existing check that can prove the change. It adds no policy, no test suite and no CI step.

## 1. Name the behavior and the plausible regression

Before you choose a command, write down the observable behavior the change affects and the failure a check must detect:

- **Engine:** runner outcome, state transition, choice routing, save and load fidelity, a specific validation error.
- **CLI:** exit status, output, resulting files, or that a rejected run left protected files unchanged.
- **Player:** player-visible transition, persistence, decision lifecycle, input handling.
- **UI and rendering:** drawn geometry, text wrapping, hit testing, or what the player sees on a surface.

A check that cannot fail when that behavior regresses is not verification. Pick the smallest check that can.

## 2. Route to the focused check

Run from the repository root, and only the rows the change touches. A documentation-only change needs no Cargo command. It needs `python3 scripts/docs.py check` (see [CONTRIBUTING.md § Documentation structure checks](../../../CONTRIBUTING.md#documentation-structure-checks)).

| Change | Focused check |
| --- | --- |
| `engine/`: parser, loader, runner, content validation, saves | `cargo test --locked -p unmei-engine`. Add `--features network` for the Decisions client. The live test stays ignored and needs a credential. |
| `cli/`: commands, generation, edits, provider handling | `cargo test --locked -p unmei` |
| `app/`, not rendered: systems, state, persistence, decisions | `cargo test --locked -p unmei-app` |
| The player's schedule or plugin composition | `cargo test --locked -p unmei-app --test shipping_schedule` |
| UI, layout or rendering | `cargo test --locked -p unmei-app`, then the rendered evidence in step 4 |

A green focused check never speaks for another crate. `--locked` makes a stale `Cargo.lock` fail instead of being rewritten, as CI does.

## 3. Escalate to the workspace gate

Before merge, or when the change crosses crates, run the gate that CI runs:

```sh
cargo fmt --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
python3 scripts/check-invariants.py
```

[`docs/testing.md` § Validation](../../../docs/testing.md#validation) and [CONTRIBUTING.md § Local repository invariant checks](../../../CONTRIBUTING.md#local-repository-invariant-checks) own the gate's full wording, including the invariant self-test and the rule groups. [`.github/workflows/check.yml`](../../../.github/workflows/check.yml) runs it on every push.

## 4. Headless tests are not the shipped path

`cargo test -p unmei-app` installs a narrower plugin set than the `unmei-play` binary, so no headless result proves that the player boots, opens a window or draws. [`docs/testing.md` § Shipped Path](../../../docs/testing.md#shipped-path) owns that split and the evidence each layer needs:

- **Schedule construction:** `cargo test --locked -p unmei-app --test shipping_schedule`, in the per-push gate.
- **Real boot:** the on-demand `boot` workflow, [`.github/workflows/boot.yml`](../../../.github/workflows/boot.yml).
- **UI, layout and rendering:** run the affected example on a real window and GPU, and keep its observed output:

```sh
cargo run -p unmei-app --example monitor_smoke
MONITOR_SMOKE_FRAME_MS=150 cargo run -p unmei-app --example monitor_smoke
cargo run -p unmei-app --example monitor_acceptance                   # surfaces, reader, styles
cargo run -p unmei-app --example monitor_acceptance -- --mode hold
cargo run -p unmei-app --example monitor_acceptance -- --mode broken  # exits 1 by design
cargo run -p unmei-app --example monitor_acceptance -- --mode gate
```

`MONITOR_SMOKE_FRAME_MS` emulates a slow frame. The header of `app/examples/monitor_acceptance/main.rs` describes each mode. When a change alters what a player sees, a rendered run is the minimum evidence. `docs/testing.md` § Shipped Path owns the rendered residual that no command covers.

## 5. Report only what ran

Report the exact command, the observed result and what remains unverified, in the shape of [the reply report](../../../docs/agents/closing-comment.md#the-reply-report).

- Never claim a command passed unless you ran it and saw it pass. Reading code is not evidence.
- Never restate a CI run or a delegate's summary as your own observation.
- Name the residual risks: rendered geometry, a real API, another platform, an unrun mode.
