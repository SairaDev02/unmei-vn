---
title: Release runbook
summary: Follow the ordered steps to release 0.1.0 or a later version from the public repository, with the failure paths and the checklists for each stage.
keywords:
  - release-certification
  - evidence-record
  - verification
  - ci
---
# Release runbook

Follow this page to release a version of Unmei from the public repository. It lists what to run, in what order and what to do when a step fails.

[Pre-beta release certification](release-certification.md) is the contract these steps execute. It defines what a [candidate](release-certification.md#candidate-identity-and-freshness-q3-q8q9-q12) is, which [evidence blocks a release](release-certification.md#release-set-and-blocking-evidence-q4-q6q7-q10) and what the [record must hold](release-certification.md#record-and-artifact-custody-q5-q9-q13). When the two disagree, the contract wins and this page has a defect.

Nothing here publishes by itself. [The contract](release-certification.md#staged-publication-not-an-atomic-transaction-q11-q15) owns when a publication is authorized, and the two crate steps (§ 6 and § 7) are separate decisions.

These terms recur:

- The coordinator is the `candidate.yml` workflow. It checks and packages one candidate commit and writes the record.
- The record is `candidate-record.json`. Its slots are named entries, and each holds the status of one piece of evidence.
- A residual is an item that a machine cannot settle. The record lists residuals beside its slots and never turns one into a pass.
- An appendix is a human record for one stage (§ 12).
- A release attempt is one pass through this page. Its release log is one issue (§ 14). A run attempt is one GitHub run of a workflow.

## Before you start

You need:

- a clone whose `origin` is the public repository and which already holds `$SHA`;
- `git`, `gh` (authenticated), `python3`, `cargo` and `curl`;
- a POSIX shell, because every `sh` block on this page assumes one;
- a Windows machine with a real GPU, for § 3;
- the GitHub and crates.io accounts that § 0 checks.

Set these variables once per release attempt, in the shell you use for every step:

```sh
REPO=owner/name                 # the public repository
SHA=<40-hex candidate commit>   # a full SHA on the public repository's main
VERSION=<x.y.z>                 # the version in the candidate's Cargo.toml
```

## 0. Before the candidate

Start only when the public repository's `main` holds the workflows, the scripts and this page.

Confirm each account check and record it in the release log (§ 14):

- GitHub two-factor authentication is on.
- The crates.io email is verified.
- No stored token has a wider scope than `publish-new` for the two crate names.

### Checks carried into the public repository

Run each check in the table in the public repository before § 1, and record its run link in the release log. Use rehearsal dispatches on a `main` commit, not the candidate:

```sh
gh workflow run candidate.yml --repo "$REPO" --ref main \
  -f sha=<40-hex main commit> -f rehearsal=true
```

A check that has not passed is an open item, and § 1 does not start until no item is open.

| Check | What proves it |
| --- | --- |
| The Linux walk passes twice in a row | The `linux-walk` slot reads `passed` in two consecutive rehearsal records. |
| An infrastructure re-run is recorded as a run attempt | In a rehearsal run that failed for infrastructure only, "Re-run failed jobs" writes a complete record for run attempt 2. That record cites the reused jobs' earlier artifacts and lists the earlier failure. |
| A replaced packaging job resets its slots | After one packaging job is re-run so its hashes change, the record for the new run attempt shows that job's slot and every slot citing its old hashes back at `pending`. |
| The `publish.yml` dry run passes | A `dry_run=true` dispatch (see [Later versions](#later-versions)) against a rehearsal coordinator run, for each crate, uploads an appendix whose two `local-crate` checks both match. |
| `publish.yml` refuses a wrong hash | A `dry_run=true` dispatch whose staged `.crate` does not match the record refuses at the verify step. |
| The weekly `deny` schedule fires | One `schedule`-event run of `deny.yml` publishes a `deny` slot and a job summary naming the advisory database commit. |

GitHub disables a schedule after 60 days without repository activity. Until the `deny` schedule fires, the dependency gate is not active.

## 1. Dispatch the coordinator

1. Dispatch `candidate.yml` at the candidate, not in rehearsal:

   ```sh
   gh workflow run candidate.yml --repo "$REPO" --ref main \
     -f sha="$SHA" -f rehearsal=false
   ```

   `candidate.yml` takes `sha` and `rehearsal`. It refuses a SHA that is not 40 lowercase hexadecimal characters, a commit that is not on `main` and a dispatch from another ref. A refusal means the input broke one of these rules, not that the workflow is broken.

2. Take the run ID of the newest `candidate.yml` run, which is the dispatch you just made, and watch the run:

   ```sh
   RUN="$(gh run list --repo "$REPO" --workflow candidate.yml --limit 1 \
     --json databaseId --jq '.[0].databaseId')"
   gh run watch --repo "$REPO" "$RUN"
   ```

3. Download the record and every slot artifact. The later steps read `candidate/` from the folder where you run this command. A re-run keeps each earlier run attempt's artifacts beside the new ones, so set one variable per artifact to its newest run attempt, in the same shell. The `ls` prints the path of `candidate-record.json`:

   ```sh
   gh run download "$RUN" --repo "$REPO" --dir candidate
   newest() { ls -d candidate/"$1"-"$SHA"-attempt* | sort -V | tail -n 1; }
   RECORD_DIR="$(newest candidate-record)"
   WINDOWS_DIR="$(newest packaging-windows)"
   LINUX_DIR="$(newest packaging-linux)"
   COVERAGE_DIR="$(newest coverage)"
   ls "$RECORD_DIR/candidate-record.json"
   ```

4. Print each slot's status:

   ```sh
   python3 - "$RECORD_DIR/candidate-record.json" <<'END'
   import json, sys
   for slot in json.load(open(sys.argv[1]))["slots"]:
       print(f"{slot['name']:<22}{slot['status']}")
   END
   ```

   The statuses are `passed`, `failed`, `pending`, `not-run` and `inconclusive` ([the contract](release-certification.md#item-eleven-plan-amendment-planned-2026-11-07) lists them under Record).

5. Check every slot before you go on. If `gate`, `packaging-windows`, `packaging-linux` or `crate-parity` reads anything but `passed`, there is no candidate. Fix `main`, dispatch a new candidate at the new `main` commit and note what you learned in the release log. Every other required slot blocks a release in the same way: a required slot that reads anything but `passed` means not certified. [The contract](release-certification.md#release-set-and-blocking-evidence-q4-q6q7-q10) lists the required evidence and its refusal rule.

6. Confirm the record's provenance. `candidate-record.json` carries a top-level `provenance` object, which the record job copies from the candidate commit's message:

   ```sh
   python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["provenance"])' \
     "$RECORD_DIR/candidate-record.json"
   ```

   A filled object holds four values: `source` is the private source SHA that was exported, `allowlist_digest` the digest of the reviewed export allowlist, `manifest_digest` the digest of the exported file manifest and `number` the export number. Together they name the tree this commit was exported from, and the reproduced tree hash is their evidence.

   For 0.1.0 the object must be filled, because that release is an export. `null` means the candidate commit is not an export commit: stop before § 2, because the record cannot show where the tree came from. Export again from the private source, dispatch a new candidate at the new `main` commit and record the reason in the release log. A later version reaches a `null` object by design, and [Later versions](#later-versions) is where it is handled.

## 2. Tag the frozen candidate

Tag before any human run (§ 3 to § 5). The Guide links shipped in the archives are `blob/v<version>/docs/guide/…`, and a reader who follows a link to an untagged branch reads a moving target.

This step pushes a tag to the public repository. Create the annotated tag at `$SHA` and push it:

```sh
git tag -a "v$VERSION" -m "Unmei $VERSION" "$SHA"
git push origin "v$VERSION"
```

[The contract's tag-timing rule](release-certification.md#item-eleven-plan-amendment-planned-2026-11-07) decides when the tag may still move. [Failure paths](#failure-paths) keeps the short version.

## 3. The Windows walk

To accept the Windows player window, extract the Windows player archive from `$WINDOWS_DIR` (§ 1 step 3) and follow the checklist in [§ 10 Windows release walk checklist](#10-windows-release-walk-checklist) on a real GPU. Run it on that archived player, not on a local build. Record the Windows build, the GPU, the driver and the display with the screenshots, in the `windows-walk` appendix (§ 12).

This walk is the only Windows GUI acceptance. The coordinator's `accept-windows` job checks the archive, not the window.

## 4. Residuals

The record lists four residuals beside its slots. Give each one a section in the `residuals` appendix, and record there what you did or what you accept:

- `linux-rendered-walk`: record only. Record the Linux full-rendered walk as unverified and accepted by you, never as passed ([the contract](release-certification.md#release-set-and-blocking-evidence-q4-q6q7-q10), Linux rendered residual).
- `smart-app-control`: act. On a Windows 11 machine where Smart App Control is on, try to start the archived player and CLI, and record whether the Guide's [Smart App Control](guide/troubleshooting.md#smart-app-control) sentence matches what Windows did. Without such a machine, record the sentence as unverified.
- `publish-environment-oidc`: act. Confirm that the public repository's `crates-io` environment requires your approval, and record the OIDC token exchange of the first real `publish.yml` run ([Later versions](#later-versions)).
- `coverage-app-exclusion`: record only. Record that the coverage report measures `engine/src/**` and `cli/src/**` and excludes the app ([`testing-coverage-evidence.md`](testing-coverage-evidence.md)).

A residual stays a residual: the record never turns one into a pass.

## 5. The first-time stranger run

Do [§ 11 Stranger run](#11-stranger-run) before any publication. The participant needs the candidate archives and the public Guide. A blocked required step sends you back to § 1 with a new candidate.

If you cannot recruit a participant, the contract's [first-time acceptance fallback](release-certification.md#first-time-acceptance-fallback-decided-2026-11-08) lets you use your Windows walk instead. It also lists the evidence the fallback needs. Only you can choose it. To use it:

1. Write the choice and its reason in the release log.
2. Record the fallback in the `stranger` appendix.
3. Name the human first-time run as an open item in the release notes.

## 6. Publish the engine

Run these commands in the clone, with `$SHA` checked out and no changes to tracked files.

1. Create the token at <https://crates.io/settings/tokens>: scope `publish-new`, patterns `unmei-engine` and `unmei`, seven-day expiry.

2. Log in to the registry:

   ```sh
   cargo login --registry crates-io
   ```

3. Package the engine locally:

   ```sh
   cargo package -p unmei-engine --locked
   ```

4. Start the `engine-publication` appendix:

   ```sh
   python3 scripts/candidate_record.py appendix new engine-publication \
     --record "$RECORD_DIR/candidate-record.json" \
     --out engine-publication.md --tester "$USER" \
     --environment "Windows 11, cargo $(cargo --version)"
   ```

5. Verify the local package against the staged hash. Every check line must read `ok`. A `mismatch` line makes the command exit 1:

   ```sh
   python3 scripts/candidate_record.py verify local-crate \
     --record "$RECORD_DIR/candidate-record.json" \
     --appendix engine-publication.md \
     --crate "target/package/unmei-engine-$VERSION.crate" --name unmei-engine
   ```

   This `verify` is the gate. If the locally packaged bytes differ from the staged engine `.crate`, stop and treat it as a packaging failure. Do not publish the locally packaged bytes instead.

6. Publish the engine. This step uploads to crates.io, and a published version cannot simply be replaced. Run it only after you have authorized this stage under [the contract](release-certification.md#staged-publication-not-an-atomic-transaction-q11-q15):

   ```sh
   cargo publish -p unmei-engine --locked
   ```

7. Prove what crates.io received. Fetch the sparse-index entry and verify its checksum. The check also requires the saved file's path to end in the sparse-index path, `un/me/unmei-engine`:

   ```sh
   mkdir -p registry/index/un/me
   curl -fsSL "https://index.crates.io/un/me/unmei-engine" \
     -o registry/index/un/me/unmei-engine
   python3 scripts/candidate_record.py verify index-cksum \
     --record "$RECORD_DIR/candidate-record.json" \
     --appendix engine-publication.md \
     --index-file registry/index/un/me/unmei-engine --name unmei-engine
   ```

8. Download the published `.crate` and verify it. The check also requires the file name `unmei-engine-$VERSION.crate`:

   ```sh
   curl -fsSL "https://static.crates.io/crates/unmei-engine/unmei-engine-$VERSION.crate" \
     -o "registry/unmei-engine-$VERSION.crate"
   python3 scripts/candidate_record.py verify download-crate \
     --record "$RECORD_DIR/candidate-record.json" \
     --appendix engine-publication.md \
     --crate "registry/unmei-engine-$VERSION.crate" --name unmei-engine
   ```

The index and the download lag an upload, so retry steps 7 and 8 for up to 2 minutes, as `publish.yml` does, before you treat a failure as real.

## 7. Publish the CLI

The CLI's own registry check could not run before the engine existed, so it runs here.

1. Run the registry check:

   ```sh
   cargo publish -p unmei --locked --dry-run
   ```

2. Package the CLI locally:

   ```sh
   cargo package -p unmei --locked
   ```

3. Start the `cli-publication` appendix:

   ```sh
   python3 scripts/candidate_record.py appendix new cli-publication \
     --record "$RECORD_DIR/candidate-record.json" \
     --out cli-publication.md --tester "$USER" \
     --environment "Windows 11, cargo $(cargo --version)"
   ```

4. Verify the local package against the staged hash:

   ```sh
   python3 scripts/candidate_record.py verify local-crate \
     --record "$RECORD_DIR/candidate-record.json" \
     --appendix cli-publication.md \
     --crate "target/package/unmei-$VERSION.crate" --name unmei
   ```

5. If that `verify` matches, authorize this stage under [the contract](release-certification.md#staged-publication-not-an-atomic-transaction-q11-q15) and publish. If it does not match, do not publish: treat it as a packaging failure, as in § 6. Publishing uploads to crates.io, and a published version cannot simply be replaced:

   ```sh
   cargo publish -p unmei --locked
   ```

6. Repeat the index and download checks of § 6 steps 7 and 8 for `unmei`, with the same retry:

   ```sh
   curl -fsSL "https://index.crates.io/un/me/unmei" -o registry/index/un/me/unmei
   python3 scripts/candidate_record.py verify index-cksum \
     --record "$RECORD_DIR/candidate-record.json" \
     --appendix cli-publication.md \
     --index-file registry/index/un/me/unmei --name unmei
   ```

   ```sh
   curl -fsSL "https://static.crates.io/crates/unmei/unmei-$VERSION.crate" \
     -o "registry/unmei-$VERSION.crate"
   python3 scripts/candidate_record.py verify download-crate \
     --record "$RECORD_DIR/candidate-record.json" \
     --appendix cli-publication.md \
     --crate "registry/unmei-$VERSION.crate" --name unmei
   ```

7. Install from the registry, as an author does:

   ```sh
   CARGO_HOME="$(mktemp -d)" cargo install unmei --version "$VERSION" --locked
   ```

   The install is advisory and cannot block the release. If it fails, record an issue before you sign anything.

## 8. Release assets

1. Write `notes.md` from [§ 13 Release notes](#13-release-notes).

2. Create a draft release:

   ```sh
   gh release create "v$VERSION" --repo "$REPO" --draft \
     --title "Unmei $VERSION" --notes-file notes.md
   ```

3. Pack the coverage bundle into one file, then upload the assets to the draft. The bundle holds nested folders, and `gh release upload` takes files only:

   ```sh
   tar -czf "unmei-$VERSION-coverage.tar.gz" -C "$COVERAGE_DIR" .
   gh release upload "v$VERSION" --repo "$REPO" \
     "$WINDOWS_DIR"/unmei-play-*-x86_64-windows.zip \
     "$WINDOWS_DIR"/unmei-[0-9]*-x86_64-windows.zip \
     "$LINUX_DIR"/unmei-play-*-x86_64-linux.tar.gz \
     "$LINUX_DIR"/unmei-[0-9]*-x86_64-linux.tar.gz \
     "$RECORD_DIR/SHA256SUMS" \
     "$RECORD_DIR/candidate-record.json" \
     "unmei-$VERSION-coverage.tar.gz" \
     windows-walk.md stranger.md residuals.md engine-publication.md cli-publication.md
   ```

   The upload is the four archives, the record's `SHA256SUMS`, the record, the appendices made so far and the coverage bundle as one `.tar.gz` file. The `release-assets` appendix follows in § 9, once you sign it. The two `.crate` files are not release assets, because crates.io serves them. `$RECORD_DIR/SHA256SUMS` is the merged list the record job wrote. It is the only sums file the release carries. Each packaging job's own `SHA256SUMS` stays in its artifact, where the record rechecked it. A rehearsal bundle and a rehearsal record stay behind, because they are not candidate evidence.

4. Start the `release-assets` appendix:

   ```sh
   python3 scripts/candidate_record.py appendix new release-assets \
     --record "$RECORD_DIR/candidate-record.json" \
     --out release-assets.md --tester "$USER" --environment "the draft release"
   ```

5. Download the draft:

   ```sh
   gh release download "v$VERSION" --repo "$REPO" --dir published
   ```

6. Check every sum in the `release-assets` appendix before you publish the draft. `SHA256SUMS` also lists the two `.crate` files, which the release does not carry, so the check also reads `registry/`, where § 6 and § 7 saved them:

   ```sh
   python3 scripts/candidate_record.py verify release-assets \
     --record "$RECORD_DIR/candidate-record.json" \
     --appendix release-assets.md \
     --dir published --dir registry --sums published/SHA256SUMS
   ```

7. Publish the release. This step makes the draft and its assets public:

   ```sh
   gh release edit "v$VERSION" --repo "$REPO" --draft=false
   ```

8. Download the release from its public URLs, not from your own upload, so that you check the bytes a stranger gets. `--clobber` overwrites any file already in `public/`:

   ```sh
   gh release download "v$VERSION" --repo "$REPO" --dir public --clobber
   ```

9. Verify the public download, with the `.crate` files from `registry/` as in step 6:

   ```sh
   python3 scripts/candidate_record.py verify release-assets \
     --record "$RECORD_DIR/candidate-record.json" \
     --appendix release-assets.md \
     --dir public --dir registry --sums public/SHA256SUMS
   ```

## 9. Close out

1. Revoke the token you created in § 6.
2. Configure trusted publishing for `unmei-engine` and `unmei`, and turn on "Require trusted publishing for all new versions", so no stored token is needed again.
3. Sign the `Signed by` and `Date` line of the `release-assets` appendix.
4. Upload the signed appendix to the release:

   ```sh
   gh release upload "v$VERSION" --repo "$REPO" release-assets.md
   ```

5. Close the release log and record the totals.

## 10. Windows release walk checklist

Take a screenshot for each surface. Record the Windows build, the GPU, the driver and the display.

- The archived normal boot.
- The archived `--dev` boot.
- The default-pack lookup when `unmei-play` starts with no `PACK_DIR` argument.
- The title screen.
- Play with choices.
- The offline fallback at a decision point.
- The menu overlay.
- Settings.
- Save, restart and load.
- Gallery.
- Timeline.
- No developer affordances in a normal build.
- The Monitor and input ownership under `--dev`.
- The Mark of the Web, the mark Windows puts on a downloaded file.
- SmartScreen's prompt.
- Launching from inside the zip.
- The missing pack.

Where a case in [Human smoke testing](testing-human-smoke.md) covers a surface, record its ID (for example `S1`) beside the screenshot.

## 11. Stranger run

1. Recruit one Windows user who has never seen Unmei and has no Rust toolchain. You cannot be that person.
2. Give them the candidate archives and the public Guide. They work from the [task card](#task-card) alone: real GUI play first, then the author half.
3. Observe silently and do not teach.
4. Record their notes and your observations in the `stranger` appendix.
5. Treat a blocked required step as a failure, not a pass: open an issue, fix it and build a new candidate. Friction notes become issues and do not block.

### Task card

Give the participant this card. It lists goals, never steps: the participant finds the steps in the documents.

> **Player.** Start from the player archive's `README`, and follow a link into the Guide only where the `README` points you. Start the demo, finish it offline, save, quit, relaunch, load your save and find where the API key goes.
>
> **Author.** Start from the CLI archive's `README`, and use the Guide where it points. Set up the authoring CLI from its archive, run the documented sample operation, then change one line of a demo scene and validate the pack again.
>
> For each goal, note every point where the documents left you unsure or did not tell you how. Mark it BLOCKED when you could not get through, or FRICTION when you got through but it was unclear or slow.

## 12. Appendices

Open one appendix per stage with `appendix new` against the record, so that its header cites the record's SHA-256, the candidate SHA and the hashes the stage exercised:

```sh
python3 scripts/candidate_record.py appendix new <stage> --record RECORD \
  --out <stage>.md --tester "$USER" --environment ENVIRONMENT
```

`RECORD` is the path of `candidate-record.json`. `ENVIRONMENT` is text naming the machine, OS and build the stage runs on.

The stages are `windows-walk`, `stranger`, `residuals`, `engine-publication`, `cli-publication` and `release-assets`. `verify` appends its own entries and keeps your sign-off line last.

## 13. Release notes

Include:

- The SHA, the tag and the check-run links.
- Platforms: Windows x64, and Linux x64 with glibc 2.39 or newer.
- The narrowed Linux claim: the rendered walk was not verified on Linux.
- The residuals, listed as residuals.
- If you used the stranger-run fallback, the human first-time run, named as an open item.
- The compatibility promise, in the words [`README.md`](../README.md) uses.
- `SHA256SUMS` and the two crate versions.
- Your sign-off.

## 14. Release log

Open one issue per release attempt in the public tracker. Record each step's time, every failure and retry, and the hours you spend coordinating evidence. Close the issue with totals. [The contract](release-certification.md#failure-recovery-and-post-beta-boundary-q12-q14) owns when the process is reviewed and which triggers fire. Record each trigger you hit in the log.

## Later versions

0.1.0 is published by hand, as § 6 and § 7 describe. A later version publishes through `publish.yml`, one crate per run, behind the approval of the `crates-io` environment, which the public repository has, with you as its required reviewer.

1. Bump the version and move the tag-pinned Guide links in the same change. Those links carry the release tag in their path: `<repository URL>/blob/v<version>/docs/guide`. Search for the link form `/blob/v<version>/docs/guide` — every tag-pinned Guide link holds it, with the version segment the published tag set — in the crate `README`s, the crate roots' rustdoc, the archive `README`s and the demo pack's agent guide, and change that segment with the bump. The bump and the links ship in one commit, because a candidate that freezes the links at the previous version ships archives that point at the previous release.

2. Set `SHA` and `RUN` from that version's candidate (§ 1), then dispatch a dry run for `unmei-engine`:

   ```sh
   gh workflow run publish.yml --repo "$REPO" --ref main \
     -f sha="$SHA" -f crate=unmei-engine -f coordinator_run_id="$RUN" -f dry_run=true
   ```

   `publish.yml` takes `sha`, `crate` (`unmei-engine`, then `unmei`), `coordinator_run_id` and `dry_run`. It refuses a dispatch from any ref but `main`. It downloads that run's record and staged payload, then refuses a record for another commit, and refuses a rehearsal record unless `dry_run` is true. It verifies the staged `.crate` against the record, packages both crates through the local overlay and verifies the bytes it would upload. Only then does it mint a crates.io token through OIDC.

   A `dry_run` stops before the token.

3. Read the appendix the dry run uploads, then dispatch the same command with `-f dry_run=false` for each crate in turn. A real run also checks the published `cksum` and the download against the staged hash, so its appendix proves what crates.io received.

## Failure paths

- **Before any publication.** Fix `main`, dispatch a new candidate and re-create the tag at the same version, on the new candidate's SHA. Record the tag movement in the release log.
- **After any publication.** The tag does not move. If the engine is published and the CLI stage fails, stop and record a partial release. Do not yank automatically, and decide the version path at that point. A partial release is a decision, not a retry.
- **A red coordinator.** Only an eligibility, gate, packaging or hash failure fails the run. A failed dependency, coverage or boot check leaves the run green and puts its verdict in its slot, so read the slot, not the checks page. Read the slots before you re-run jobs. A re-run that replaces a `.crate` returns every slot that cited it to `pending`. Each of those slots stays `pending` until its job runs against the new bytes. The record job finds the replacement by comparing with the earlier run attempt's record.
