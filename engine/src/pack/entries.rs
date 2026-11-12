//! Read-only views over a loaded pack: the gallery's ordered entries (spec
//! `docs/title-and-menu.md` § Gallery) and the declared scene order the Timeline
//! lists (§ Timeline). Both are derived from the pack's own data, so neither
//! depends on scene-file enumeration order or `HashMap` iteration.

use std::collections::BTreeSet;

use crate::content::{GalleryEntry, Line, Pack};

impl Pack {
    /// The gallery's ordered entries: one per distinct declared image, the
    /// manifest's `title_backdrop` merged into the background group by id.
    /// Backgrounds first, then sprites, each group alphabetical; images only,
    /// so `hide`, `play` and `stop` references yield nothing
    /// (spec `docs/title-and-menu.md` § Gallery `### Entries` §1–§4).
    ///
    /// The order is total and computed from the pack's own data: the
    /// `BTreeSet` sorts and dedupes, so the list never depends on `HashMap`
    /// iteration or scene-file enumeration order. This is the only producer of
    /// what an entry is, its label and its order — an entry carries no asset
    /// path, which the runtime app resolves per kind.
    pub fn gallery_entries(&self) -> Vec<GalleryEntry> {
        let mut entries: BTreeSet<GalleryEntry> = BTreeSet::new();
        for scene in self.scenes.values() {
            for line in &scene.lines {
                match line {
                    Line::Bg(id) => {
                        entries.insert(GalleryEntry::Background { id: id.clone() });
                    }
                    Line::Sprite { npc_id, pose } => {
                        entries.insert(GalleryEntry::Sprite {
                            npc_id: npc_id.clone(),
                            pose: pose.clone(),
                        });
                    }
                    // Images only: `hide`/`play`/`stop` and every other line
                    // contribute no entry.
                    _ => {}
                }
            }
        }
        if let Some(id) = &self.manifest.title_backdrop {
            entries.insert(GalleryEntry::Background { id: id.clone() });
        }
        entries.into_iter().collect()
    }

    /// The pack's declared scene order: the manifest's `scenes` list verbatim,
    /// absent/empty yielding no entries (spec `docs/title-and-menu.md`
    /// § Timeline §2). Read-only enumeration only — no visited join, card,
    /// lock state, alternate order, directory scan or `HashMap` sort enters the
    /// engine.
    pub fn timeline_entries(&self) -> &[String] {
        &self.manifest.scenes
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    use crate::content::Manifest;
    use crate::pack::load_pack;
    use crate::parser::Parser;

    use super::*;

    fn demo_pack() -> Pack {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/demo-pack");
        load_pack(&root).expect("demo pack loads").pack
    }

    /// Build a pack from parsed scenes, no NPCs and an empty state map. The
    /// gallery derivation reads only `scenes` and `manifest.title_backdrop`.
    fn test_pack(scenes: &[(&str, &str)]) -> Pack {
        let mut map = HashMap::new();
        for (id, src) in scenes {
            let parsed = Parser::new(&format!("{id}.unmei"))
                .parse(id, src)
                .expect("test scene parses");
            map.insert(id.to_string(), parsed);
        }
        Pack {
            manifest: Manifest {
                name: "test".to_string(),
                entry: scenes[0].0.to_string(),
                tagline: None,
                title_backdrop: None,
                scenes: Vec::new(),
            },
            state: HashMap::new(),
            scenes: map,
            npcs: HashMap::new(),
            root: PathBuf::from("."),
        }
    }

    fn labels(pack: &Pack) -> Vec<String> {
        pack.gallery_entries().iter().map(|e| e.label()).collect()
    }

    /// The demo pack's declared images, backgrounds first then sprites, each
    /// group alphabetical (spec `### Entries` §5).
    #[test]
    fn demo_pack_lists_its_declared_images_in_order() {
        let pack = demo_pack();
        assert_eq!(
            labels(&pack),
            vec![
                "school_gate".to_string(),
                "ace \u{00B7} happy".to_string(),
                "ace \u{00B7} neutral".to_string(),
            ]
        );
    }

    /// `bg school_gate` in three scenes and as the manifest's backdrop is one
    /// background entry, with no marker.
    #[test]
    fn title_backdrop_merges_into_one_background_entry() {
        let mut pack = test_pack(&[
            ("a", "bg school_gate\nNarrator: hi\n"),
            ("b", "bg school_gate\nNarrator: hi\n"),
            ("c", "bg school_gate\nNarrator: hi\n"),
        ]);
        pack.manifest.title_backdrop = Some("school_gate".to_string());
        assert_eq!(
            pack.gallery_entries(),
            vec![GalleryEntry::Background {
                id: "school_gate".to_string()
            }]
        );
    }

    /// The backdrop-only fixture: a backdrop id no scene declares is its own
    /// background entry.
    #[test]
    fn a_backdrop_no_scene_declares_is_its_own_entry() {
        let mut pack = test_pack(&[("a", "Narrator: hi\n")]);
        pack.manifest.title_backdrop = Some("rooftop".to_string());
        assert_eq!(
            pack.gallery_entries(),
            vec![GalleryEntry::Background {
                id: "rooftop".to_string()
            }]
        );
    }

    /// A `(npc_id, pose)` pair declared many times yields one entry per pair,
    /// and the same id in both arms of an `if` yields one entry.
    #[test]
    fn repeated_and_branch_declarations_dedupe_to_one_entry_each() {
        let pack = test_pack(&[(
            "a",
            "sprite ace neutral\n\
             sprite ace neutral\n\
             sprite ace happy\n\
             if met_ace\n\
             bg school_gate\n\
             sprite ace neutral\n\
             else\n\
             bg school_gate\n\
             sprite ace happy\n\
             endif\n",
        )]);
        assert_eq!(
            pack.gallery_entries(),
            vec![
                GalleryEntry::Background {
                    id: "school_gate".to_string()
                },
                GalleryEntry::Sprite {
                    npc_id: "ace".to_string(),
                    pose: "happy".to_string()
                },
                GalleryEntry::Sprite {
                    npc_id: "ace".to_string(),
                    pose: "neutral".to_string()
                },
            ]
        );
    }

    /// `hide`, `play` and `stop` yield no entry, and no demo-pack audio id ever
    /// appears in the list (spec `### Entries` §1).
    #[test]
    fn audio_and_hide_references_yield_no_entries() {
        let pack = test_pack(&[(
            "a",
            "hide ace\nplay music_morning\nstop music_morning\nbg school_gate\n",
        )]);
        assert_eq!(labels(&pack), vec!["school_gate".to_string()]);

        let demo_labels = labels(&demo_pack());
        for audio in [
            "music_morning",
            "music_evening",
            "sfx_paper_rustle",
            "sfx_select",
        ] {
            assert!(
                !demo_labels.iter().any(|label| label == audio),
                "`{audio}` must not appear in the gallery"
            );
        }
    }

    /// A background label is the raw id, a sprite label is `npc_id · pose` with
    /// U+00B7, no label leaves `[a-z0-9_ ·]` and no two entries share a label.
    #[test]
    fn labels_use_only_the_allowed_characters_and_are_unique() {
        let entries = demo_pack().gallery_entries();
        assert!(
            entries
                .iter()
                .any(|e| matches!(e, GalleryEntry::Background { id } if id == "school_gate")),
            "a background label is its raw id"
        );
        assert!(
            entries.iter().any(|e| e.label() == "ace \u{00B7} happy"),
            "a sprite label joins the ids with U+00B7"
        );

        let mut labels = labels(&demo_pack());
        for label in &labels {
            assert!(
                label.chars().all(|c| c.is_ascii_lowercase()
                    || c.is_ascii_digit()
                    || c == '_'
                    || c == ' '
                    || c == '\u{00B7}'),
                "`{label}` has a character outside [a-z0-9_ ·]"
            );
        }
        let total = labels.len();
        labels.sort();
        labels.dedup();
        assert_eq!(labels.len(), total, "no two entries share a label");
    }

    /// The order is total and computed from the pack's data: permuting the
    /// scene names changes nothing, and two calls return the same list.
    #[test]
    fn order_is_total_and_independent_of_scene_enumeration() {
        let expected = vec![
            GalleryEntry::Background {
                id: "alpha".to_string(),
            },
            GalleryEntry::Background {
                id: "school_gate".to_string(),
            },
            GalleryEntry::Background {
                id: "zeta".to_string(),
            },
            GalleryEntry::Sprite {
                npc_id: "ace".to_string(),
                pose: "happy".to_string(),
            },
            GalleryEntry::Sprite {
                npc_id: "zoe".to_string(),
                pose: "sad".to_string(),
            },
        ];
        let a = test_pack(&[
            ("one", "bg zeta\nsprite zoe sad\nbg alpha\n"),
            ("two", "sprite ace happy\nbg school_gate\n"),
        ]);
        let b = test_pack(&[
            ("z_last", "bg zeta\nsprite zoe sad\nbg alpha\n"),
            ("a_first", "sprite ace happy\nbg school_gate\n"),
        ]);
        assert_eq!(a.gallery_entries(), expected);
        assert_eq!(b.gallery_entries(), expected);
        assert_eq!(a.gallery_entries(), a.gallery_entries());
    }

    /// Entries carry their kind and ids and yield a label — never an asset path.
    #[test]
    fn entries_carry_kind_and_ids_and_no_asset_path() {
        let pack = test_pack(&[("a", "bg school_gate\nsprite ace neutral\n")]);
        let entries = pack.gallery_entries();
        match &entries[0] {
            GalleryEntry::Background { id } => assert_eq!(id, "school_gate"),
            other => panic!("expected a background entry, got {other:?}"),
        }
        match &entries[1] {
            GalleryEntry::Sprite { npc_id, pose } => {
                assert_eq!(npc_id, "ace");
                assert_eq!(pose, "neutral");
            }
            other => panic!("expected a sprite entry, got {other:?}"),
        }
        assert_eq!(entries[1].label(), "ace \u{00B7} neutral");
        for entry in &entries {
            let label = entry.label();
            assert!(
                !label.contains('/') && !label.contains(".png"),
                "`{label}` looks like a path, not a label"
            );
        }
    }

    /// An entry whose file is absent from `assets/` still appears: the
    /// derivation is structural and never touches the filesystem.
    #[test]
    fn an_entry_whose_file_is_absent_still_appears() {
        let mut pack = test_pack(&[("a", "bg not_shipped\nsprite ghost vanished\n")]);
        pack.manifest.title_backdrop = Some("also_not_shipped".to_string());
        assert_eq!(
            labels(&pack),
            vec![
                "also_not_shipped".to_string(),
                "not_shipped".to_string(),
                "ghost \u{00B7} vanished".to_string(),
            ]
        );
    }

    /// A pack declaring no image yields no entries — the empty-gallery state.
    #[test]
    fn a_pack_declaring_no_images_yields_no_entries() {
        let pack = test_pack(&[("a", "Narrator: hi\nplay music_morning\n")]);
        assert!(pack.gallery_entries().is_empty());
    }
}
