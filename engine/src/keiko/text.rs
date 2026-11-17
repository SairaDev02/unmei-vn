//! The case format's one text rule: compare exactly, after both sides are NFC
//! normalised (spec §2.1).
//!
//! Composed and decomposed spellings of the same character are one text to a
//! player, so a case file written on one platform matches a pack written on
//! another. Nothing else about the comparison is relaxed.

use unicode_normalization::UnicodeNormalization;

/// The NFC form of `text`, which is what both sides of a comparison become.
pub fn normalize(text: &str) -> String {
    text.nfc().collect()
}

/// Whether two texts are the same to a player: exact after NFC normalisation.
pub fn exact(left: &str, right: &str) -> bool {
    normalize(left) == normalize(right)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_composed_and_a_decomposed_spelling_are_the_same_text() {
        // U+00E9 "é" versus "e" + U+0301.
        assert!(exact("caf\u{e9}", "cafe\u{301}"));
        assert!(!exact("Ace", "ace"), "case still matters");
        assert!(!exact("a b", "a  b"), "no whitespace folding");
    }
}
