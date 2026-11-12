//! The two crate-visible candidate validators against the staged pack
//! (author-cli contract §9).
//!
//! Both take checked immutable inputs and return the engine's `LoadError`
//! display text verbatim; they own no provider, prompt, session, correction,
//! retry or outer-filter state. The NPC validator keeps a staged-preparation
//! failure distinct from a loader content rejection
//! ([`NpcValidationFailure`]), so the command stops terminally instead of
//! filtering a preparation message into an empty correction set. The
//! Scene path keeps its existing I/O-as-corrections mapping; the staged
//! validation decision in `docs/author-cli.md` § 9 preserves that
//! classification and hands its ownership to
//! `docs/author-cli-drafting.md` § 10.

use unmei_engine::content::DecisionConfig;

use crate::target::{CheckedPack, NpcTarget, SceneTarget};

use super::staged_pack::Staging;

/// Why staged NPC validation did not confirm the candidate, keeping a
/// preparation failure apart from a loader content rejection.
#[derive(Debug)]
pub(crate) enum NpcValidationFailure {
    /// The candidate was installed in a real staged pack and the real engine
    /// loader rejected it; these are the engine's content diagnostics for the
    /// command's outer filter.
    Content(Vec<String>),
    /// A staged-preparation step failed — the temporary directory, a transplant
    /// source read, the staged read/parse, serialization or the write — so the
    /// candidate was never installed or loader-validated. Terminal: never a
    /// correction, retry or attempt-budget consumer, and never eligible for the
    /// content substring filter. The message is the verbatim underlying I/O or
    /// parser text.
    Preparation(String),
}

/// Validate one NPC decision candidate against the whole pack (author-cli
/// contract §9): stage the checked pack, install the candidate and reload.
///
/// `Ok(())` now carries the acceptance guarantee: the candidate was
/// installed in the staged pack and the real loader ran over it, so an
/// `Accepted` downstream implies both. It still does not assert whole-pack
/// validity — unrelated pre-existing pack diagnostics stay filtered out.
/// Neither outcome writes any authored file.
pub(crate) fn validate_npc_decision(
    checked: &CheckedPack<'_>,
    target: &NpcTarget,
    question: &str,
    candidate: &DecisionConfig,
) -> Result<(), NpcValidationFailure> {
    let staging = Staging::fresh().map_err(|e| NpcValidationFailure::Preparation(e.to_string()))?;
    npc_decision_core(&staging, checked, target, question, candidate)
}

/// Validate one fully edited Scene against the whole pack (author-cli
/// contract §9): stage the checked pack, replace the Scene's staged bytes and
/// reload. Layer 1, say-line rendering and the run splice stay in the composing
/// attempt; this takes the finished Scene text and writes nothing authored.
pub(crate) fn validate_scene(
    checked: &CheckedPack<'_>,
    target: &SceneTarget,
    edited: &str,
) -> Result<(), Vec<String>> {
    let staging = Staging::fresh().map_err(|e| vec![e.to_string()])?;
    scene_core(&staging, checked, target, edited)
}

/// The one NPC validation core both the production entry above and the
/// module-private cleanup witness run. The `Staging` owner is dropped on every
/// ordinary return path here, including the `?` early returns.
fn npc_decision_core(
    staging: &Staging,
    checked: &CheckedPack<'_>,
    target: &NpcTarget,
    question: &str,
    candidate: &DecisionConfig,
) -> Result<(), NpcValidationFailure> {
    staging
        .transplant(checked)
        .map_err(NpcValidationFailure::Preparation)?;
    staging
        .place_npc_candidate(checked, target, question, candidate)
        .map_err(NpcValidationFailure::Preparation)?;
    staging.reload().map_err(NpcValidationFailure::Content)
}

/// The one Scene validation core, shared the same way as
/// [`npc_decision_core`].
fn scene_core(
    staging: &Staging,
    checked: &CheckedPack<'_>,
    target: &SceneTarget,
    edited: &str,
) -> Result<(), Vec<String>> {
    staging
        .transplant(checked)
        .map_err(|e| vec![e.to_string()])?;
    staging.place_scene(target, edited)?;
    staging.reload()
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};
    use std::path::{Path, PathBuf};

    use unmei_engine::content::Npc;
    use unmei_engine::{load_pack, Pack};

    use super::*;

    /// A real source pack with one Scene, one NPC, one asset and one unrelated
    /// file, so byte-protection assertions cover every authored surface.
    fn write_fixture_pack(root: &Path) {
        std::fs::create_dir_all(root.join("scenes")).unwrap();
        std::fs::create_dir_all(root.join("npcs")).unwrap();
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(
            root.join("pack.ron"),
            "(name: \"fixture\", entry: \"main\")",
        )
        .unwrap();
        std::fs::write(root.join("state.ron"), "(met_ace: false,)").unwrap();
        std::fs::write(root.join("scenes/main.unmei"), "Narrator: A quiet room.\n").unwrap();
        std::fs::write(
            root.join("npcs/ace.ron"),
            "(id: \"ace\", name: \"Ace\", decisions: {})",
        )
        .unwrap();
        std::fs::write(root.join("assets/note.txt"), "asset bytes").unwrap();
        std::fs::write(root.join("other.txt"), "other bytes").unwrap();
    }

    /// A real source pack loaded from a test-owned directory.
    fn fixture() -> (tempfile::TempDir, Pack) {
        let source = tempfile::TempDir::new().unwrap();
        write_fixture_pack(source.path());
        let pack = load_pack(source.path()).expect("fixture loads").pack;
        (source, pack)
    }

    /// A `choice` candidate; `fallback` may be an unknown action.
    fn choice_config(state_keys: &[&str], fallback: &str) -> DecisionConfig {
        serde_json::from_value(serde_json::json!({
            "instructions": "How does Ace greet?",
            "state_keys": state_keys,
            "criteria": {
                "warm": "an open greeting",
                "cold": "a guarded greeting",
                "none": "neither fits",
            },
            "fallback": fallback,
        }))
        .expect("a well-formed choice config")
    }

    /// Every file under `root`, keyed by its relative path, with its bytes:
    /// covers manifest, State, Scenes, NPCs, assets and unrelated files, and
    /// records intentional absence by omission.
    fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
            let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect();
            entries.sort();
            for path in entries {
                if path.is_dir() {
                    walk(&path, root, out);
                } else {
                    out.insert(
                        path.strip_prefix(root).unwrap().to_path_buf(),
                        std::fs::read(&path).unwrap(),
                    );
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(root, root, &mut out);
        out
    }

    /// The parent holds no leftover staged directory: the owner was released.
    fn assert_parent_released(parent: &Path) {
        assert!(
            std::fs::read_dir(parent).unwrap().next().is_none(),
            "the staged owner was not released: {:?}",
            std::fs::read_dir(parent)
                .unwrap()
                .map(|e| e.unwrap().path())
                .collect::<Vec<_>>()
        );
    }

    /// Run the production NPC core with the owner under a test parent, exactly
    /// as production holds and releases it.
    fn run_npc_core(
        parent: &Path,
        checked: &CheckedPack<'_>,
        target: &NpcTarget,
        question: &str,
        candidate: &DecisionConfig,
    ) -> Result<(), NpcValidationFailure> {
        let staging = Staging::under(parent).expect("staging dir under the test parent");
        npc_decision_core(&staging, checked, target, question, candidate)
    }

    /// The loader's content diagnostics from a validator result, panicking on
    /// a preparation failure — these witnesses expect only content rejections.
    fn content_errors(result: Result<(), NpcValidationFailure>) -> Vec<String> {
        match result {
            Err(NpcValidationFailure::Content(errors)) => errors,
            other => panic!("expected loader content diagnostics, got {other:?}"),
        }
    }

    /// Run the production Scene core the same way.
    fn run_scene_core(
        parent: &Path,
        checked: &CheckedPack<'_>,
        target: &SceneTarget,
        edited: &str,
    ) -> Result<(), Vec<String>> {
        let staging = Staging::under(parent).expect("staging dir under the test parent");
        scene_core(&staging, checked, target, edited)
    }

    /// Both production validators cross the real TempDir / transplant /
    /// engine-loader boundary. Valid NPC and Scene candidates pass; an
    /// independently invalid candidate yields the engine's pack-relative file
    /// and message, with the Scene's known source line exact and no line where
    /// the engine supplies none.
    #[test]
    fn valid_candidates_pass_and_invalid_ones_report_loader_diagnostics() {
        let (source, pack) = fixture();
        let checked = CheckedPack::new(&pack).unwrap();

        // NPC: a valid candidate passes; an unknown fallback is the loader's
        // pack-relative diagnostic, and the engine gives it no line.
        let npc = NpcTarget::new("ace").unwrap();
        validate_npc_decision(
            &checked,
            &npc,
            "greet",
            &choice_config(&["met_ace"], "cold"),
        )
        .expect("a valid NPC candidate passes");
        let errors = content_errors(validate_npc_decision(
            &checked,
            &npc,
            "greet",
            &choice_config(&["met_ace"], "nope"),
        ));
        let error = &errors[0];
        assert!(error.starts_with("npcs/ace.ron:"), "NPC file: {error}");
        assert!(error.contains("fallback `nope`"), "NPC message: {error}");
        let after_file = error.strip_prefix("npcs/ace.ron:").unwrap();
        assert!(
            after_file.starts_with(' '),
            "the engine supplies no line for this NPC diagnostic: {error}"
        );

        // Create: an absent NPC gets the CLI's create head, then the candidate.
        let nova = NpcTarget::new("nova").unwrap();
        validate_npc_decision(
            &checked,
            &nova,
            "greet",
            &choice_config(&["met_ace"], "cold"),
        )
        .expect("a created NPC candidate passes");

        // Scene: a valid edit passes; an undeclared variable is reported at the
        // exact 1-based source line of the edited Scene.
        let scene = SceneTarget::new("main").unwrap();
        let original = std::fs::read_to_string(source.path().join("scenes/main.unmei")).unwrap();
        validate_scene(&checked, &scene, &original).expect("the unedited Scene passes");
        let edited = format!("{original}Narrator: {{missing_key}} waits.\n");
        let line = original.lines().count() + 1;
        let errors = validate_scene(&checked, &scene, &edited)
            .expect_err("an undeclared variable is rejected");
        assert!(
            errors
                .iter()
                .any(|e| e.starts_with(&format!("scenes/main.unmei:{line}: "))
                    && e.contains("missing_key")),
            "the Scene diagnostic at line {line}: {errors:?}"
        );
    }

    /// Corrupting a source NPC file after the Pack loaded lets the real
    /// transplant succeed while the staged read/parse fails. The validator must
    /// report a preparation failure, not a loader content rejection, so the
    /// command cannot filter it away.
    #[test]
    fn a_staged_npc_parse_failure_is_a_preparation_failure() {
        let (source, pack) = fixture();
        let checked = CheckedPack::new(&pack).unwrap();
        let target = NpcTarget::new("ace").unwrap();
        std::fs::write(source.path().join("npcs/ace.ron"), "this is not an Npc").unwrap();
        let parent = tempfile::TempDir::new().unwrap();

        match run_npc_core(
            parent.path(),
            &checked,
            &target,
            "greet",
            &choice_config(&["met_ace"], "cold"),
        ) {
            Err(NpcValidationFailure::Preparation(message)) => {
                assert!(!message.is_empty(), "the verbatim parser text is carried");
            }
            other => panic!("expected a preparation failure, got {other:?}"),
        }
        assert_parent_released(parent.path());
    }

    /// The real loader reports an unrelated NPC's content diagnostic
    /// through the validator. Proves the command-level filter witness — which
    /// asserts that diagnostic is dropped from the candidate's corrections —
    /// is not vacuous.
    #[test]
    fn an_unrelated_pack_diagnostic_reaches_the_validator() {
        let (source, pack) = fixture();
        let mut pack = pack;
        let ghost = Npc {
            id: "ghost".to_string(),
            name: "Ghost".to_string(),
            model: None,
            decisions: HashMap::from([(
                "haunt".to_string(),
                choice_config(&["ghost_only_key"], "cold"),
            )]),
        };
        std::fs::write(
            source.path().join("npcs/ghost.ron"),
            ron::ser::to_string(&ghost).unwrap(),
        )
        .unwrap();
        pack.npcs.insert("ghost".to_string(), ghost);

        let checked = CheckedPack::new(&pack).unwrap();
        let target = NpcTarget::new("ace").unwrap();
        let errors = content_errors(validate_npc_decision(
            &checked,
            &target,
            "greet",
            &choice_config(&["met_ace"], "cold"),
        ));
        assert!(
            errors
                .iter()
                .any(|e| e.contains("npcs/ghost.ron") && e.contains("ghost_only_key")),
            "the real loader reports the unrelated NPC: {errors:?}"
        );
    }

    /// The private real-TempDir-under-test-parent seam runs the same
    /// cores. After success, loader rejection and a portable transplant-read
    /// failure the owner is released (the parent is empty) and every authored
    /// byte — or its intentional absence — is unchanged.
    ///
    /// Ordinary ownership release is what this proves. Candidate-write,
    /// temp-creation and cleanup *faults* have no portable real-filesystem
    /// fixture here and stay explicitly unproven: this witness does not claim
    /// crash, OS-refusal, permission-denial or partial-write durability.
    #[test]
    fn staging_is_released_and_never_touches_the_source_pack() {
        let (source, pack) = fixture();
        let checked = CheckedPack::new(&pack).unwrap();
        let npc = NpcTarget::new("ace").unwrap();
        let scene = SceneTarget::new("main").unwrap();
        let scene_text = std::fs::read_to_string(source.path().join("scenes/main.unmei")).unwrap();
        let parent = tempfile::TempDir::new().unwrap();
        let parent = parent.path();

        // Success.
        let before = snapshot(source.path());
        run_npc_core(
            parent,
            &checked,
            &npc,
            "greet",
            &choice_config(&["met_ace"], "cold"),
        )
        .expect("a valid candidate passes");
        assert_parent_released(parent);
        assert_eq!(snapshot(source.path()), before, "authored bytes unchanged");

        // Loader rejection.
        let before = snapshot(source.path());
        run_npc_core(
            parent,
            &checked,
            &npc,
            "greet",
            &choice_config(&["met_ace"], "nope"),
        )
        .expect_err("an invalid candidate is rejected");
        assert_parent_released(parent);
        assert_eq!(snapshot(source.path()), before, "authored bytes unchanged");

        // Portable transplant-read failure: a source file goes missing after
        // the Pack was loaded, so the copy cannot read it. Capture the
        // baseline immediately before validation.
        std::fs::remove_file(source.path().join("scenes/main.unmei")).unwrap();
        let baseline = snapshot(source.path());
        run_scene_core(parent, &checked, &scene, &scene_text)
            .expect_err("a missing source file fails the transplant");
        assert_parent_released(parent);
        assert_eq!(
            snapshot(source.path()),
            baseline,
            "authored bytes unchanged"
        );
        assert!(
            !source.path().join("scenes/main.unmei").exists(),
            "intentional absence stays absent"
        );
    }

    /// A deterministic private fixture protects verbatim engine message
    /// display. The candidate's undeclared `state_keys` entry is the exact
    /// absolute staged NPC path, so the real loader echoes it inside its own
    /// message; the validator must show those bytes unchanged rather than
    /// rewrite the matching substring. This is not a no-absolute-text promise.
    #[test]
    fn a_staged_path_substring_in_a_loader_message_is_shown_verbatim() {
        let (_source, pack) = fixture();
        let checked = CheckedPack::new(&pack).unwrap();
        let target = NpcTarget::new("ace").unwrap();
        let parent = tempfile::TempDir::new().unwrap();

        let staging = Staging::under(parent.path()).unwrap();
        let staged_npc_path = target.path_in(staging.root()).display().to_string();
        let candidate = choice_config(&[staged_npc_path.as_str()], "cold");
        let errors = content_errors(npc_decision_core(
            &staging, &checked, &target, "greet", &candidate,
        ));
        assert!(
            errors.iter().any(|e| e.contains("state_keys entry")),
            "the loader's own message: {errors:?}"
        );
        assert!(
            errors.iter().any(|e| e.contains(&staged_npc_path)),
            "the engine's message bytes survive verbatim: {errors:?}"
        );
    }
}
