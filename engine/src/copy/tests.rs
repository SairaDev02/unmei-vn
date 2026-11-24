//! The copy table's own tests: the key shape, the placeholder contract every
//! override must keep, and the one-pass rendering rule.

use super::*;

/// Every `{name}` `text` carries, in the order it carries them.
fn placeholders_in(text: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some((_, after)) = rest.split_once('{') {
        let Some((name, tail)) = after.split_once('}') else {
            panic!("{text:?} has a `{{` no `}}` closes");
        };
        found.push(name);
        rest = tail;
    }
    found
}

#[test]
fn every_id_is_unique() {
    let mut seen = std::collections::HashSet::new();
    for key in KEYS {
        assert!(seen.insert(key.id), "duplicate copy key {}", key.id);
    }
}

/// An id is a dotted lowercase path: `confirm.overwrite.prompt`. The player, a
/// plugin manifest and a finding all name a key by it, so its spelling is a
/// contract.
#[test]
fn every_id_is_a_dotted_lowercase_path() {
    for key in KEYS {
        assert!(!key.id.is_empty(), "an empty copy key id");
        for segment in key.id.split('.') {
            assert!(!segment.is_empty(), "{} has an empty segment", key.id);
            assert!(
                segment
                    .chars()
                    .all(|character| character.is_ascii_lowercase()
                        || character.is_ascii_digit()
                        || character == '-'),
                "{} has a segment that is not lowercase, digit or `-`",
                key.id
            );
        }
    }
}

/// A control and the text a player reads about it share one vocabulary, so a
/// copy key may never take a control's id: the interface tables list both, and
/// an override names one or the other. A colliding label names its text role
/// instead (`title.row.new-game`).
#[test]
fn no_id_is_a_keiko_control_id() {
    for key in KEYS {
        assert!(
            !crate::keiko::is_control_id(key.id),
            "{} collides with a Keiko control id",
            key.id
        );
    }
}

/// The contract a plugin's `copy` override is checked against later: a value
/// carries exactly the placeholders the key declares, in the declared order.
#[test]
fn every_text_carries_exactly_its_declared_placeholders() {
    for key in KEYS {
        assert_eq!(
            placeholders_in(key.text),
            key.placeholders,
            "{} declares {:?} but its text carries {:?}",
            key.id,
            key.placeholders,
            placeholders_in(key.text)
        );
        for placeholder in key.placeholders {
            assert!(
                !placeholder.is_empty()
                    && placeholder
                        .chars()
                        .all(|character| character.is_ascii_lowercase()
                            || character.is_ascii_digit()
                            || character == '_'),
                "{} declares the placeholder {placeholder:?}, which is not a plain word",
                key.id
            );
        }
    }
}

#[test]
fn key_finds_every_declared_key() {
    for entry in KEYS {
        assert_eq!(
            key(entry.id),
            Some(entry),
            "{} is not reachable by id",
            entry.id
        );
    }
}

#[test]
fn an_unknown_id_has_no_key() {
    assert_eq!(key("play.no-such-key"), None);
    assert_eq!(key(""), None);
}

#[test]
fn text_hands_back_the_declared_text() {
    assert_eq!(text("play.quick.auto"), "Auto A");
    assert_eq!(text("settings.text-speed.slowest"), "Slowest");
}

#[test]
fn the_quick_rows_labels_are_default_looks_words() {
    for (id, words) in [
        ("play.quick.save", "Save"),
        ("play.quick.load", "Load"),
        ("play.quick.auto", "Auto A"),
        ("play.quick.skip", "Skip Tab"),
        ("play.quick.menu", "Menu Esc"),
    ] {
        assert_eq!(text(id), words);
    }
    for removed in ["play.help", "play.cue.auto", "play.cue.skip"] {
        assert_eq!(key(removed), None, "`{removed}` is removed from the table");
    }
}

#[test]
#[should_panic(expected = "unknown copy key")]
fn text_panics_on_an_unknown_id() {
    text("play.no-such-key");
}

#[test]
fn render_replaces_each_declared_placeholder_it_is_given() {
    assert_eq!(
        render("confirm.overwrite.prompt", &[("slot", "Slot 2")]),
        "Overwrite Slot 2? Its current save is replaced."
    );
    assert_eq!(
        render(
            "title.continue.autosave",
            &[("scene", "school_gate"), ("saved_at", "01 Jan 2026 09:00")]
        ),
        "school_gate \u{00b7} 01 Jan 2026 09:00"
    );
}

#[test]
fn render_keeps_a_placeholder_it_was_not_given() {
    assert_eq!(
        render("title.continue.autosave", &[("scene", "school_gate")]),
        "school_gate \u{00b7} {saved_at}"
    );
}

/// A copied path can hold a `{`, so a substituted value is never read again: it
/// reaches the screen verbatim and cannot open a second placeholder.
#[test]
fn render_never_expands_a_value_it_substituted() {
    assert_eq!(
        render("confirm.clear-key.prompt", &[("path", "C:/a/{b}/key")]),
        "The stored key is deleted from C:/a/{b}/key. You can paste it again at any time."
    );
    assert_eq!(
        render(
            "title.continue.autosave",
            &[("scene", "a{"), ("saved_at", "b")]
        ),
        "a{ \u{00b7} b",
        "a substituted `{{` cannot swallow the next placeholder"
    );
}

/// A plugin's copy override is filled the same way as the table's text, so an
/// override that keeps `{slot}` reaches the screen with the slot in it.
#[test]
fn fill_substitutes_a_placeholder_in_a_template_that_is_not_the_table() {
    assert_eq!(
        fill("Replace the save in {slot}?", &[("slot", "Slot 2")]),
        "Replace the save in Slot 2?"
    );
    assert_eq!(
        fill("A { that no brace closes", &[("slot", "Slot 2")]),
        "A { that no brace closes",
        "the override's own text is read the same way the table's is"
    );
}
