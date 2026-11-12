//! Tests for run addressing (`unmei author draft --list` / `--from`): the pure
//! seam against synthetic scenes, and the shipped demo pack (drafting contract
//! §1/§2).

use std::path::Path;

use unmei::draft;
use unmei_engine::{Parser, Scene};

#[path = "common/pack.rs"]
mod pack;

fn scene(text: &str) -> Scene {
    Parser::new("test.unmei")
        .parse("test", text)
        .expect("test scene parses")
}

fn handles(scene: &Scene) -> Vec<String> {
    draft::runs(scene)
        .into_iter()
        .map(|run| run.handle)
        .collect()
}

fn demo_scene(scene_id: &str) -> Scene {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/demo-pack");
    let loaded = unmei_engine::load_pack(&root).unwrap();
    loaded.pack.scenes.get(scene_id).cloned().unwrap()
}

/// A command ends a run; blank and comment lines between say-lines stay inside
/// it, but a trailing blank before a command does not.
#[test]
fn runs_are_say_blocks_bounded_by_commands() {
    let scene = scene(
        "Narrator: one\n\
         # an interstitial comment\n\
         Narrator: two\n\
         \n\
         sprite ace neutral\n\
         Ace: three\n",
    );
    let runs = draft::runs(&scene);

    assert_eq!(
        runs.iter().map(|r| r.handle.as_str()).collect::<Vec<_>>(),
        ["start", "start#2"]
    );
    assert_eq!(runs[0].start_line, 1);
    assert_eq!(
        runs[0].end_line, 3,
        "interstitial blank/comment stay inside"
    );
    assert_eq!(runs[0].first_say, "Narrator: one");
    assert_eq!(runs[0].span(), "L1-L3");
    assert_eq!(runs[1].start_line, 6);
    assert_eq!(runs[1].end_line, 6, "trailing blank is outside the run");
    assert_eq!(runs[1].span(), "L6");
}

/// `endif` is parser-controlled, so a say-line after a block must not merge
/// with the block's last say-line.
#[test]
fn endif_ends_a_run() {
    let scene = scene(
        "if met_ace\n\
         \x20 Ace: inside\n\
         endif\n\
         Ace: after\n",
    );
    let runs = draft::runs(&scene);

    assert_eq!(
        runs.iter().map(|r| r.handle.as_str()).collect::<Vec<_>>(),
        ["start", "start#2"],
        "the run after `endif` is a separate run"
    );
    assert_eq!(runs[0].span(), "L2");
    assert_eq!(runs[1].span(), "L4");
    assert_eq!(runs[0].first_say, "Ace: inside");
    assert_eq!(runs[1].first_say, "Ace: after");
}

/// Labels name the innermost anchor; several runs under one anchor take `#2`,
/// `#3`… and an `if`/`else` arm adds no handle of its own.
#[test]
fn labels_and_if_arms_fold_to_the_enclosing_anchor() {
    let scene = scene(
        "Narrator: pre\n\
         label branch\n\
         if met_ace\n\
         \x20 Ace: taken\n\
         else\n\
         \x20 Narrator: other\n\
         endif\n\
         label tail\n\
         Ace: after\n",
    );
    assert_eq!(handles(&scene), ["start", "branch", "branch#2", "tail"]);
}

/// Ordinals are keyed by handle name, so a label reusing `start` cannot shadow
/// the top-level anchor and every run stays uniquely addressable.
#[test]
fn a_label_reusing_start_cannot_shadow_the_top_level_anchor() {
    let scene = scene("Narrator: pre\nlabel start\nAce: in label\n");
    assert_eq!(handles(&scene), ["start", "start#2"]);
}

/// Addressing resolves the bare handle and a `#n` ordinal to exactly one run;
/// everything else names nothing.
#[test]
fn resolve_accepts_exact_handles_and_rejects_the_rest() {
    let scene = scene("Narrator: one\nsprite ace neutral\nNarrator: two\n");
    let runs = draft::runs(&scene);
    assert_eq!(handles(&scene), ["start", "start#2"]);

    for good in ["start", "start#2"] {
        assert_eq!(draft::resolve(&runs, "test", good).unwrap().handle, good);
    }
    for bad in ["start#1", "start#3", "start#", "#2", "nope", ""] {
        let err = draft::resolve(&runs, "test", bad).unwrap_err();
        assert_eq!(err.address, bad);
        assert!(err.to_string().contains("no run"), "{err}");
    }
}

#[test]
fn render_list_formats_rows_in_scene_order() {
    let scene = scene("Narrator: one\nsprite ace neutral\nAce: two\n");
    assert_eq!(
        draft::render_list(&draft::runs(&scene)),
        concat!(
            "  start       L1     Narrator: one\n",
            "  start#2     L3     Ace: two\n",
        )
    );
}

// ---------------------------------------------------------------------------
// The shipped demo pack
// ---------------------------------------------------------------------------

/// RED: `first_meeting` exposes the four runs the contract's `--list` example
/// names, with the exact handles, spans and first say-lines.
#[test]
fn first_meeting_list_matches_the_contract_example() {
    let scene = demo_scene("first_meeting");
    let runs = draft::runs(&scene);

    let handles: Vec<&str> = runs.iter().map(|r| r.handle.as_str()).collect();
    assert_eq!(handles, ["start", "start#2", "ask_letter", "silence"]);
    let spans: Vec<String> = runs.iter().map(|r| r.span()).collect();
    assert_eq!(spans, ["L7", "L10", "L21", "L28"]);
    let first_says: Vec<&str> = runs.iter().map(|r| r.first_say.as_str()).collect();
    assert_eq!(
        first_says,
        [
            "Narrator: Morning light. A folded letter on the ground, half under a shoe.",
            "Ace: You're not from around here, are you?",
            "Ace: A letter? Ah... you saw that.",
            "Ace: ...Fine, don't talk. See if I care.",
        ]
    );

    assert_eq!(
        draft::render_list(&runs),
        concat!(
            "  start       L7     Narrator: Morning light. A folded letter on the ground, half under a shoe.\n",
            "  start#2     L10    Ace: You're not from around here, are you?\n",
            "  ask_letter  L21    Ace: A letter? Ah... you saw that.\n",
            "  silence     L28    Ace: ...Fine, don't talk. See if I care.\n",
        )
    );
}

/// RED: `epilogue`'s runs sit inside `if`/`else` arms, so they all fold to
/// `start`, numbered in scene order.
#[test]
fn epilogue_runs_fold_if_arms_to_start() {
    let scene = demo_scene("epilogue");
    let runs = draft::runs(&scene);
    assert_eq!(
        runs.iter().map(|r| r.handle.as_str()).collect::<Vec<_>>(),
        ["start", "start#2", "start#3", "start#4", "start#5"]
    );
    let spans: Vec<String> = runs.iter().map(|r| r.span()).collect();
    assert_eq!(spans, ["L10", "L12", "L14", "L17", "L21"]);
}

/// RED: `reunion`'s branches are one run per label, plus the pre-`decide` run.
#[test]
fn reunion_runs_are_one_per_label() {
    let scene = demo_scene("reunion");
    let runs = draft::runs(&scene);
    assert_eq!(
        runs.iter().map(|r| r.handle.as_str()).collect::<Vec<_>>(),
        ["start", "reply.warm", "reply.cold", "reply.none"]
    );
    let spans: Vec<String> = runs.iter().map(|r| r.span()).collect();
    assert_eq!(spans, ["L7", "L14", "L19", "L24"]);
}

/// RED: addressing resolves `first_meeting`'s handles and rejects addresses
/// that name nothing.
#[test]
fn first_meeting_addresses_resolve_or_name_nothing() {
    let scene = demo_scene("first_meeting");
    let runs = draft::runs(&scene);

    assert_eq!(
        draft::resolve(&runs, "first_meeting", "start")
            .unwrap()
            .start_line,
        7
    );
    assert_eq!(
        draft::resolve(&runs, "first_meeting", "start#2")
            .unwrap()
            .start_line,
        10
    );
    assert_eq!(
        draft::resolve(&runs, "first_meeting", "ask_letter")
            .unwrap()
            .first_say,
        "Ace: A letter? Ah... you saw that."
    );

    for bad in ["start#1", "start#3", "ask_letter#2", "reply.warm", ""] {
        let err = draft::resolve(&runs, "first_meeting", bad).unwrap_err();
        assert_eq!(err.address, bad);
        assert!(err.to_string().contains("first_meeting"), "{err}");
    }
}

// ---------------------------------------------------------------------------
// Layer-1 say-line gate (contract §4)
// ---------------------------------------------------------------------------

fn speaker_set(names: &[&str]) -> std::collections::HashSet<String> {
    names.iter().map(|name| name.to_string()).collect()
}

/// RED: well-formed say-lines from allowed speakers pass the gate and render
/// back exactly; blank lines in the reply are separators, not content.
#[test]
fn gate_accepts_say_lines_from_allowed_speakers() {
    let allowed = speaker_set(&["Narrator", "Ace"]);
    let candidates =
        draft::candidate_lines("  Ace: Who goes there?\n\nNarrator: The gate creaks.  \n");
    let lines = draft::gate(&candidates, &allowed).unwrap();

    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].speaker, "Ace");
    assert_eq!(lines[0].text, "Who goes there?");
    assert_eq!(lines[0].render(), "Ace: Who goes there?");
    assert_eq!(lines[1].render(), "Narrator: The gate creaks.");
}

/// RED: a keyword in the leading token is control flow or a command smuggled
/// into prose — rejected even when the line would parse as a say-line.
#[test]
fn gate_rejects_every_keyword_first_line() {
    let allowed = speaker_set(&["Narrator", "Ace"]);
    for line in [
        "bg: school_gate",
        "sprite: ace happy",
        "hide: ace",
        "play: music_morning",
        "stop: music_morning",
        "set: met_ace = true",
        "label: foo",
        "jump: foo",
        "choice: x",
        "end: x",
        "decide: ace.greet -> reply",
        "if: met_ace",
        "else: x",
        "endif: x",
        "sprite ace happy",
        "label foo: bar",
    ] {
        let err = draft::gate(&[line.to_string()], &allowed).unwrap_err();
        assert_eq!(err.len(), 1, "{line}: {err:?}");
        assert!(err[0].contains("keyword"), "{line}: {}", err[0]);
    }
}

/// RED: a malformed line or an undeclared speaker is a rejection with a
/// correction naming what to fix.
#[test]
fn gate_rejects_malformed_lines_and_unknown_speakers() {
    let allowed = speaker_set(&["Narrator", "Ace"]);

    let err = draft::gate(&["Ace has no colon".into()], &allowed).unwrap_err();
    assert!(err[0].contains("not a say-line"), "{err:?}");

    let err = draft::gate(&["Ace:No space after the colon".into()], &allowed).unwrap_err();
    assert!(err[0].contains("not a say-line"), "{err:?}");

    let err = draft::gate(&["Bob: Who goes there?".into()], &allowed).unwrap_err();
    assert!(
        err[0].contains("Bob") && err[0].contains("not allowed"),
        "{err:?}"
    );
}

/// RED: a reply with no say-lines is a rejection too.
#[test]
fn gate_rejects_an_empty_draft() {
    let err = draft::gate(&[], &speaker_set(&["Narrator"])).unwrap_err();
    assert!(err[0].contains("no say-lines"), "{err:?}");
}

/// RED: the allowed set is `Narrator`, the scene's speakers, and every NPC id.
#[test]
fn allowed_speakers_are_narrator_scene_speakers_and_npc_ids() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/demo-pack");
    let loaded = unmei_engine::load_pack(&root).unwrap();
    let scene = scene("Ace: hello\n");

    let allowed = draft::allowed_speakers(&scene, &loaded.pack);
    assert!(allowed.contains("Narrator"), "{allowed:?}");
    assert!(allowed.contains("Ace"), "scene speaker: {allowed:?}");
    assert!(allowed.contains("ace"), "npc id: {allowed:?}");
    assert!(!allowed.contains("Bob"), "{allowed:?}");
}

// ---------------------------------------------------------------------------
// Run-region splice (contract §6)
// ---------------------------------------------------------------------------

/// RED: the splice replaces the run's say-lines plus interstitial blank/comment
/// lines, and every line outside survives byte-for-byte.
#[test]
fn splice_replaces_the_run_and_preserves_everything_else() {
    let src = "# scene header\n\
               Narrator: one\n\
               # interstitial comment\n\
               Narrator: two\n\
               sprite ace neutral\n\
               Ace: three\n\
               # trailer\n";
    let scene = scene(src);
    let runs = draft::runs(&scene);
    assert_eq!(
        runs.iter().map(|r| r.handle.as_str()).collect::<Vec<_>>(),
        ["start", "start#2"]
    );

    let edited = draft::splice_run(src, &runs[0], &["Narrator: REWRITTEN".to_string()]);
    assert_eq!(
        edited, "# scene header\nNarrator: REWRITTEN\nsprite ace neutral\nAce: three\n# trailer\n",
        "the interstitial comment is inside the run; the rest is untouched"
    );

    let edited = draft::splice_run(
        src,
        &runs[1],
        &["Ace: REWRITTEN".to_string(), "Narrator: more".to_string()],
    );
    assert_eq!(
        edited,
        "# scene header\nNarrator: one\n# interstitial comment\nNarrator: two\n\
         sprite ace neutral\nAce: REWRITTEN\nNarrator: more\n# trailer\n"
    );
}

/// RED: a run at EOF without a trailing newline splices cleanly.
#[test]
fn splice_handles_a_run_at_eof_without_a_trailing_newline() {
    let src = "Narrator: one\nAce: two";
    let scene = scene(src);
    let runs = draft::runs(&scene);
    assert_eq!(runs.len(), 1);

    let edited = draft::splice_run(src, &runs[0], &["Narrator: rewritten".to_string()]);
    assert_eq!(edited, "Narrator: rewritten");
}

/// RED: a CRLF scene keeps its line endings — the replacement uses the file's
/// own style and the run's terminator survives (contract §6).
#[test]
fn splice_preserves_a_crlf_scenes_line_endings() {
    let src = "Narrator: one\r\nsprite ace neutral\r\nAce: two\r\n";
    let scene = scene(src);
    let runs = draft::runs(&scene);
    let run = draft::resolve(&runs, "test", "start#2").unwrap();

    let edited = draft::splice_run(
        src,
        run,
        &["Ace: REWRITTEN".to_string(), "Narrator: more".to_string()],
    );
    assert_eq!(
        edited,
        "Narrator: one\r\nsprite ace neutral\r\nAce: REWRITTEN\r\nNarrator: more\r\n"
    );
}

/// A scene saved with a UTF-8 byte order mark reads exactly like the same
/// scene without one: the same runs at the same line numbers, and its first
/// comment line is never a say-line (content format §2).
#[test]
fn bom_scene_yields_the_same_runs_and_line_numbers() {
    let original = demo_scene("first_meeting");
    let demo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/demo-pack");
    let temp = tempfile::tempdir().unwrap();

    for (relative, bytes) in pack::pack_bytes(&demo) {
        let path = temp.path().join(&relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let bytes = if relative == Path::new("scenes/first_meeting.unmei") {
            let mut with_bom = String::from('\u{feff}').into_bytes();
            with_bom.extend_from_slice(&bytes);
            with_bom
        } else {
            bytes
        };
        std::fs::write(&path, bytes).unwrap();
    }

    let loaded = unmei_engine::load_pack(temp.path()).expect("a BOM scene loads");
    let with_bom = loaded.pack.scenes.get("first_meeting").unwrap();
    let runs = draft::runs(with_bom);

    assert_eq!(runs, draft::runs(&original), "the BOM changed the runs");
    assert_eq!(
        with_bom.line_numbers, original.line_numbers,
        "the BOM changed the line numbers"
    );
    assert_eq!(runs[0].start_line, 7, "the opening comment is not a run");
    assert!(
        runs.iter().all(|run| !run.first_say.contains("scene id:")),
        "the opening comment became prose: {runs:#?}"
    );
}
