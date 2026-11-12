//! The temporary staged candidate pack: one owner of the `TempDir`, the
//! transplant, the candidate placement, the reload and the release
//! (author-cli contract §9).
//!
//! Both authoring commands validate a candidate by installing it in a staged
//! copy of the checked pack and reloading through the real engine loader.
//! Cleanup is ordinary [`tempfile::TempDir`] best-effort release, not crash or
//! OS-refusal immunity; confinement is [`crate::target`]'s lexical checked
//! target, not a symlink/reparse-point, race or multi-user sandbox.

use std::collections::HashMap;
use std::path::Path;

use unmei_engine::content::{DecisionConfig, Npc};
use unmei_engine::read_pack_text;

use crate::target::{CheckedPack, NpcTarget, SceneTarget};

/// The one owner of a call's temporary candidate pack: a fresh real
/// `TempDir` is the staged root, every staged write and the reload go through
/// it, and dropping it releases the directory. Each validator call builds its
/// own; it is never reused across attempts or feedback regenerations.
pub(super) struct Staging {
    dir: tempfile::TempDir,
}

impl Staging {
    /// A fresh owner in the OS temporary location — the production path.
    pub(super) fn fresh() -> std::io::Result<Self> {
        tempfile::TempDir::new().map(|dir| Self { dir })
    }

    /// The module-private construction seam for the cleanup witnesses: a fresh
    /// owner under a test-owned parent, so a witness can inspect the parent
    /// after release. Production never passes a parent and no caller can reach
    /// this root.
    #[cfg(test)]
    pub(super) fn under(parent: &Path) -> std::io::Result<Self> {
        tempfile::Builder::new()
            .tempdir_in(parent)
            .map(|dir| Self { dir })
    }

    /// The staged root. Private to this owner; callers never see it.
    pub(super) fn root(&self) -> &Path {
        self.dir.path()
    }

    /// Copy the checked inventory's pack into the staged root.
    ///
    /// Both the source reads and the staged writes are joined from the checked
    /// inventory's targets, never from raw map keys or embedded ids. Scene and
    /// NPC bytes are copied as authored — re-rendering the DSL would be lossy —
    /// while Manifest and State are re-serialized. Assets are not needed to
    /// load.
    pub(super) fn transplant(&self, checked: &CheckedPack<'_>) -> Result<(), String> {
        let pack = checked.pack();
        let dest = self.root();
        std::fs::create_dir_all(dest.join("scenes")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(dest.join("npcs")).map_err(|e| e.to_string())?;

        let manifest = ron::ser::to_string(&pack.manifest).map_err(|e| e.to_string())?;
        std::fs::write(dest.join("pack.ron"), manifest).map_err(|e| e.to_string())?;

        // The engine-owned pack RON bridge serializes the typed state map into
        // a quoted-key RON map: keys and string values are RON-escaped, so
        // quotes, backslashes, line breaks, control characters and Unicode
        // survive staging. This is the one writer the loader's decode reads
        // back, shared by both authoring commands.
        let state_src =
            unmei_engine::pack_ron::encode_state(&pack.state).map_err(|e| e.to_string())?;
        std::fs::write(dest.join("state.ron"), state_src).map_err(|e| e.to_string())?;

        for target in checked.scenes() {
            let bytes = std::fs::read(target.path_in(&pack.root)).map_err(|e| e.to_string())?;
            std::fs::write(target.path_in(dest), bytes).map_err(|e| e.to_string())?;
        }

        for target in checked.npcs() {
            let bytes = std::fs::read(target.path_in(&pack.root)).map_err(|e| e.to_string())?;
            std::fs::write(target.path_in(dest), bytes).map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    /// Install one NPC decision candidate in the staged pack.
    ///
    /// Membership selects create/replace: an absent NPC gets the CLI's create
    /// head. The staged file, not `pack.npcs`, stays the replacement's content
    /// snapshot, so the read and parse surface their failure instead of
    /// defaulting to empty/`None`: a candidate that cannot be read back and
    /// re-serialized was never installed. The `Err` is the verbatim
    /// underlying I/O or parser text.
    pub(super) fn place_npc_candidate(
        &self,
        checked: &CheckedPack<'_>,
        target: &NpcTarget,
        question: &str,
        candidate: &DecisionConfig,
    ) -> Result<(), String> {
        let npc_path = target.path_in(self.root());
        if !checked.pack().npcs.contains_key(target.id()) {
            let npc = Npc {
                id: target.id().to_string(),
                name: target.id().to_string(),
                model: None,
                decisions: HashMap::new(),
            };
            let head = ron::ser::to_string(&npc).map_err(|e| e.to_string())?;
            std::fs::write(&npc_path, head).map_err(|e| e.to_string())?;
        }
        let existing_src = read_pack_text(&npc_path).map_err(|e| e.to_string())?;
        let mut npc: Npc = ron::from_str(&existing_src).map_err(|e| e.to_string())?;
        npc.decisions
            .insert(question.to_string(), candidate.clone());
        let updated = ron::ser::to_string(&npc).map_err(|e| e.to_string())?;
        std::fs::write(&npc_path, updated).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Replace the checked Scene target's staged bytes with `edited`.
    pub(super) fn place_scene(
        &self,
        target: &SceneTarget,
        edited: &str,
    ) -> Result<(), Vec<String>> {
        std::fs::write(target.path_in(self.root()), edited).map_err(|e| vec![e.to_string()])
    }

    /// Reload the staged pack through the real engine loader.
    ///
    /// Every `LoadError` is displayed verbatim: the engine owns the
    /// pack-relative file, optional 1-based line and message, and this owner
    /// adds no scrubbing, deduplication, ranking or reordering. Warnings are
    /// not rejections.
    pub(super) fn reload(&self) -> Result<(), Vec<String>> {
        match unmei_engine::load_pack(self.root()) {
            Ok(_loaded) => Ok(()),
            Err(errors) => Err(errors.into_iter().map(|e| e.to_string()).collect()),
        }
    }
}

#[cfg(test)]
mod tests {
    use unmei_engine::content::Manifest;
    use unmei_engine::{load_pack, Pack, StateValue};

    use super::*;

    fn demo_pack() -> Pack {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/demo-pack");
        load_pack(&root).expect("demo pack loads").pack
    }

    /// Stage `pack` and return the staged `pack.ron` text plus the manifest the
    /// staged pack loads back to. The staged copy is a real pack, so
    /// `load_pack` exercises the same loader the runtime uses.
    fn stage(pack: &Pack) -> (String, Manifest) {
        let staging = Staging::fresh().expect("temp dir");
        let checked = CheckedPack::new(pack).expect("demo keys are safe");
        staging
            .transplant(&checked)
            .expect("staging copies the pack");
        let text =
            std::fs::read_to_string(staging.root().join("pack.ron")).expect("staged pack.ron");
        let reloaded = load_pack(staging.root())
            .expect("staged pack loads")
            .pack
            .manifest;
        (text, reloaded)
    }

    /// The staged manifest as a RON map, for shape assertions on the raw text.
    fn staged_map(text: &str) -> ron::Value {
        ron::from_str(text).expect("staged pack.ron is valid RON")
    }

    /// The `key` field of a staged manifest, absent when the manifest omits it.
    fn manifest_field<'a>(map: &'a ron::Value, key: &str) -> Option<&'a ron::Value> {
        let ron::Value::Map(entries) = map else {
            panic!("manifest serializes as a RON map");
        };
        entries.get(&ron::Value::String(key.to_string()))
    }

    /// The `key` field of a staged manifest, asserting it is a plain RON string
    /// (never `Some("…")`) when present.
    fn string_field<'a>(map: &'a ron::Value, key: &str) -> Option<&'a str> {
        manifest_field(map, key).map(|value| match value {
            ron::Value::String(s) => s.as_str(),
            other => panic!("`{key}` must be a plain string, got {other:?}"),
        })
    }

    /// The `key` field of a staged manifest as a plain RON list of strings,
    /// asserting the value is a bare sequence (never `Some([…])`).
    fn string_list_field<'a>(map: &'a ron::Value, key: &str) -> Option<Vec<&'a str>> {
        manifest_field(map, key).map(|value| match value {
            ron::Value::Seq(items) => items
                .iter()
                .map(|item| match item {
                    ron::Value::String(s) => s.as_str(),
                    other => panic!("`{key}` items must be plain strings, got {other:?}"),
                })
                .collect(),
            other => panic!("`{key}` must be a plain list, got {other:?}"),
        })
    }

    /// Every optional manifest field stages as a plain RON value —
    /// the two strings and the declared order list — and the staged pack
    /// reloads to an equal manifest with the authored order intact.
    #[test]
    fn staging_round_trips_all_manifest_fields() {
        let mut pack = demo_pack();
        pack.manifest.scenes = vec![
            "reunion".to_string(),
            "epilogue".to_string(),
            "first_meeting".to_string(),
        ];
        let (text, reloaded) = stage(&pack);
        let map = staged_map(&text);
        assert_eq!(string_field(&map, "tagline"), Some("a demo pack"), "{text}");
        assert_eq!(
            string_field(&map, "title_backdrop"),
            Some("school_gate"),
            "{text}"
        );
        assert_eq!(
            string_list_field(&map, "scenes"),
            Some(vec!["reunion", "epilogue", "first_meeting"]),
            "{text}"
        );
        assert!(!text.contains("Some("), "never RON `Some(...)`: {text}");
        assert_eq!(reloaded, pack.manifest, "staged manifest equals the source");
    }

    /// A pack declaring no optional field stages a `pack.ron` with
    /// none of the keys; an empty declared order collapses to omitted and
    /// reloads as no declared order.
    #[test]
    fn staging_omits_absent_manifest_fields() {
        let mut pack = demo_pack();
        pack.manifest.tagline = None;
        pack.manifest.title_backdrop = None;
        pack.manifest.scenes = Vec::new();
        let (text, reloaded) = stage(&pack);
        let map = staged_map(&text);
        assert_eq!(string_field(&map, "tagline"), None, "{text}");
        assert_eq!(string_field(&map, "title_backdrop"), None, "{text}");
        assert_eq!(string_list_field(&map, "scenes"), None, "{text}");
        assert!(!text.contains("Some("), "never RON `Some(...)`: {text}");
        assert!(reloaded.scenes.is_empty(), "empty order stays no order");
        assert_eq!(reloaded, pack.manifest);
    }

    /// The shared writer-to-loader fidelity witness. A real source pack
    /// is loaded, staged by the actual transplant operation and reloaded by
    /// the real engine loader; the entire State map survives — a key and value
    /// carrying **both a quote and a backslash**, an escaped non-identifier
    /// string key, a newline/Unicode/control-character value, and ordinary
    /// bool/finite-number scalars. The authored `state.ron` and `pack.ron`
    /// bytes are left exactly as written.
    ///
    /// This is the regression the old hand-renderer fails: it interpolated the
    /// key unwrapped and the value as `"{value}"`, so any of these strings
    /// staged into invalid RON and the reload refused the pack.
    #[test]
    fn staging_preserves_the_whole_state_map_and_authored_bytes() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let root = tmp.path();
        std::fs::create_dir_all(root.join("scenes")).unwrap();
        std::fs::create_dir_all(root.join("npcs")).unwrap();
        let pack_ron = "(name: \"fidelity pack\", entry: \"main\")";
        // A real `state.ron` in explicit map form, every key quoted, with the
        // exact strings the staging path must not reinterpret.
        let state_ron = r#"{
    "weird key": "value",
    "quote\"and\\backslash": "quote\" and backslash\\ and newline\n and bell\u{7} and 中\u{1F600}",
    "met_ace": true,
    "letters_found": 0,
    "affinity": 3.5,
}"#;
        std::fs::write(root.join("pack.ron"), pack_ron).unwrap();
        std::fs::write(root.join("state.ron"), state_ron).unwrap();
        std::fs::write(root.join("scenes/main.unmei"), "Narrator: hi\n").unwrap();

        let source_pack = load_pack(root).expect("source pack loads").pack;

        // Independently expected values, not a copy of the loader's output.
        let mut expected: HashMap<String, StateValue> = HashMap::new();
        expected.insert("weird key".to_string(), StateValue::Str("value".into()));
        expected.insert(
            "quote\"and\\backslash".to_string(),
            StateValue::Str(
                "quote\" and backslash\\ and newline\n and bell\u{7} and 中\u{1F600}".into(),
            ),
        );
        expected.insert("met_ace".to_string(), StateValue::Bool(true));
        expected.insert("letters_found".to_string(), StateValue::Num(0.0));
        expected.insert("affinity".to_string(), StateValue::Num(3.5));
        assert_eq!(source_pack.state, expected, "the source reads as declared");

        let checked = CheckedPack::new(&source_pack).expect("source keys are safe");
        let staging = Staging::fresh().expect("temp dir");
        staging
            .transplant(&checked)
            .expect("staging copies the pack");

        let staged_state =
            std::fs::read_to_string(staging.root().join("state.ron")).expect("staged state.ron");
        let reloaded = load_pack(staging.root())
            .unwrap_or_else(|e| {
                panic!("staged pack loads: {e:?}\nstaged state.ron:\n{staged_state}")
            })
            .pack;
        assert_eq!(
            reloaded.state, expected,
            "the whole State map survives staging:\n{staged_state}"
        );

        // The source authored bytes are never rewritten or migrated.
        assert_eq!(
            std::fs::read_to_string(root.join("state.ron")).unwrap(),
            state_ron
        );
        assert_eq!(
            std::fs::read_to_string(root.join("pack.ron")).unwrap(),
            pack_ron
        );
    }
}
