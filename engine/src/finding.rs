//! The finding shape, its severity and the fixed rule-id vocabulary.
//!
//! A finding is the one diagnostic shape the loader, the check, the preview
//! and Keiko all report: the pack file, the path inside it, one rule id, a
//! severity and a message. `unmei_engine::keiko` re-exports it, so a case
//! finding and a pack problem share one type and one text form.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Whether a finding stops the load or only reports something real but
/// non-fatal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    /// The one spelling of the severity on the wire (`"error"`, `"warning"`),
    /// which the check's text and JSON forms both use
    /// (check-and-preview spec §8).
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

/// Where a finding sits inside its file.
///
/// On the wire (the check's JSON and Keiko's JSON) it is one string, and that
/// text is a compatibility contract: `""` for the file as a whole, `line <n>`
/// for a line, anything else for a field path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Location {
    /// The file as a whole.
    File,
    /// One line of the file, counted from 1.
    Line(usize),
    /// A named place: a dotted RON path such as `decisions.greet.fallback`, or
    /// the Keiko step a finding is about. Never empty and never `line <n>`;
    /// those read back as [`Location::File`] and [`Location::Line`].
    Field(String),
}

impl Location {
    /// The line number, for a [`Location::Line`] only.
    pub fn line(&self) -> Option<usize> {
        match self {
            Location::Line(line) => Some(*line),
            Location::File | Location::Field(_) => None,
        }
    }

    fn from_wire(text: String) -> Self {
        if text.is_empty() {
            return Location::File;
        }
        let line = text
            .strip_prefix("line ")
            .filter(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|digits| digits.parse().ok());
        match line {
            Some(line) => Location::Line(line),
            None => Location::Field(text),
        }
    }
}

/// The wire text. [`sort_findings`] breaks its last ties on these bytes, so
/// the order is fixed by what the JSON shows.
impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Location::File => Ok(()),
            Location::Line(line) => write!(f, "line {line}"),
            Location::Field(path) => f.write_str(path),
        }
    }
}

impl Serialize for Location {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Location {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Location::from_wire)
    }
}

/// One problem, reported in the one shape every engine surface speaks.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Finding {
    /// The pack-relative file, such as `scenes/reunion.unmei`, `npcs/ace.ron`
    /// or `tests/opening.keiko`.
    pub file: String,
    /// Where inside the file. The JSON key stays `path`.
    pub path: Location,
    /// One of the fixed rule ids ([`rules`]).
    pub rule: String,
    /// Whether the problem stops the load.
    pub severity: Severity,
    /// The human-readable detail.
    pub message: String,
}

impl Finding {
    /// An error finding: what the load rules, the check, the preview and Keiko
    /// report unless the rule is a warning.
    pub fn new(
        file: impl Into<String>,
        path: Location,
        rule: &str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            file: file.into(),
            path,
            rule: rule.to_string(),
            severity: Severity::Error,
            message: message.into(),
        }
    }

    /// A warning finding: real, reported, but not a rejection.
    pub fn warning(
        file: impl Into<String>,
        path: Location,
        rule: &str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: Severity::Warning,
            ..Self::new(file, path, rule, message)
        }
    }

    /// A finding about one line of a file.
    pub fn at_line(
        file: impl Into<String>,
        line: usize,
        rule: &str,
        message: impl Into<String>,
    ) -> Self {
        Self::new(file, Location::Line(line), rule, message)
    }

    /// A warning about one line of a file.
    pub fn warning_at_line(
        file: impl Into<String>,
        line: usize,
        rule: &str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: Severity::Warning,
            ..Self::at_line(file, line, rule, message)
        }
    }

    /// The same finding re-filed under another rule (a fault's rule id applied
    /// to a case line).
    pub fn with_rule(mut self, rule: &str) -> Self {
        self.rule = rule.to_string();
        self
    }
}

/// The check-and-preview spec §3 order every consumer sees: file, then line with
/// a finding that has no line first, then rule id, then message. The path and the
/// severity break the remaining ties, so the order is total and two runs over the
/// same pack report identical bytes.
pub fn sort_findings(findings: &mut [Finding]) {
    findings.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then_with(|| a.path.line().cmp(&b.path.line()))
            .then_with(|| a.rule.cmp(&b.rule))
            .then_with(|| a.message.cmp(&b.message))
            .then_with(|| a.path.to_string().cmp(&b.path.to_string()))
            .then_with(|| severity_rank(a.severity).cmp(&severity_rank(b.severity)))
    });
}

fn severity_rank(severity: Severity) -> u8 {
    match severity {
        Severity::Error => 0,
        Severity::Warning => 1,
    }
}

/// The text the error screen, the session log and the author CLI already show:
/// `file:line: message` for a line, `file: path: message` for another path and
/// `file: message` for the file as a whole. The severity is carried, never
/// printed here.
impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.path {
            Location::Line(line) => write!(f, "{}:{}: {}", self.file, line, self.message),
            Location::File => write!(f, "{}: {}", self.file, self.message),
            Location::Field(path) => write!(f, "{}: {}: {}", self.file, path, self.message),
        }
    }
}

/// The fixed rule ids: Keiko's case and step rules, and the pack rules the
/// check-and-preview spec's §4 table fixes. Adding a rule is an additive change.
pub mod rules {
    pub const CASE_VERSION: &str = "keiko.case.version";
    pub const CASE_SYNTAX: &str = "keiko.case.syntax";
    pub const CASE_HEADER_ORDER: &str = "keiko.case.header-order";
    pub const CASE_REFERENCE: &str = "keiko.case.reference";
    pub const STEP_NO_TARGET: &str = "keiko.step.no-target";
    pub const STEP_DISABLED: &str = "keiko.step.disabled";
    pub const EXPECT: &str = "keiko.expect";
    pub const PANIC: &str = "keiko.panic";
    pub const NOTICE: &str = "keiko.notice";
    pub const DEFENSIVE: &str = "keiko.defensive";
    pub const STUCK: &str = "keiko.stuck";
    pub const UNSETTLED: &str = "keiko.unsettled";
    pub const RENDER_UNAVAILABLE: &str = "keiko.render.unavailable";

    pub const PACK_FILE_READ: &str = "pack.file.read";
    pub const PACK_FILE_RON: &str = "pack.file.ron";
    pub const PACK_MANIFEST: &str = "pack.manifest";
    pub const PACK_MANIFEST_UNLISTED: &str = "pack.manifest.unlisted";
    pub const PACK_MANIFEST_TAGLINE_LENGTH: &str = "pack.manifest.tagline-length";
    pub const PACK_SCENE_SYNTAX: &str = "pack.scene.syntax";
    pub const PACK_SCENE_TARGET: &str = "pack.scene.target";
    pub const PACK_SCENE_CYCLE: &str = "pack.scene.cycle";
    pub const PACK_STATE: &str = "pack.state";
    pub const PACK_NPC: &str = "pack.npc";
    pub const PACK_DECISION: &str = "pack.decision";
    pub const PACK_DECISION_NO_NONE: &str = "pack.decision.no-none";
    pub const PACK_ASSET_MISSING: &str = "pack.asset.missing";
    pub const PACK_ASSET_DECODE: &str = "pack.asset.decode";

    pub const FLOW_UNREACHABLE_SCENE: &str = "flow.unreachable-scene";
    pub const FLOW_UNREACHABLE_LABEL: &str = "flow.unreachable-label";
    pub const FLOW_NO_EXIT: &str = "flow.no-exit";

    pub const PREVIEW_STATE_UNREACHABLE: &str = "preview.state.unreachable";

    // The pack's UI plugin (ui-plugin spec §8). Later plugin tickets add their
    // own ids to this family.
    pub const UI_FILE_READ: &str = "ui.file.read";
    pub const UI_PARSE: &str = "ui.parse";
    pub const UI_VERSION: &str = "ui.version";
    pub const UI_PATH_ESCAPE: &str = "ui.path.escape";
    pub const UI_CAP: &str = "ui.cap";
    pub const UI_TOKEN_UNKNOWN: &str = "ui.token.unknown";
    pub const UI_TOKEN_CYCLE: &str = "ui.token.cycle";
    pub const UI_SURFACE_UNKNOWN: &str = "ui.surface.unknown";
    pub const UI_SLOT_UNKNOWN: &str = "ui.slot.unknown";
    pub const UI_SLOT_MISSING: &str = "ui.slot.missing";
    pub const UI_SLOT_DUPLICATE: &str = "ui.slot.duplicate";
    pub const UI_COPY_UNKNOWN: &str = "ui.copy.unknown";
    pub const UI_COPY_PLACEHOLDER: &str = "ui.copy.placeholder";
    pub const UI_FONT_LICENSE: &str = "ui.font.license";
    pub const UI_FONT_UNKNOWN: &str = "ui.font.unknown";
    pub const UI_MOTION_FORBIDDEN: &str = "ui.motion.forbidden";
    pub const UI_MOTION_FLASH: &str = "ui.motion.flash";

    /// The finding rule a failure row the pack causes is filed under.
    pub fn log_row(row: u32) -> String {
        format!("keiko.log.row-{row}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_finding_displays_the_old_file_line_message_form() {
        let finding = Finding::at_line("scenes/a.unmei", 4, rules::PACK_SCENE_SYNTAX, "boom");
        assert_eq!(finding.path, Location::Line(4));
        assert_eq!(finding.to_string(), "scenes/a.unmei:4: boom");
    }

    #[test]
    fn another_path_displays_between_the_file_and_the_message() {
        let finding = Finding::new(
            "npcs/ace.ron",
            Location::Field("decisions.greet.fallback".to_string()),
            rules::PACK_DECISION,
            "boom",
        );
        assert_eq!(
            finding.to_string(),
            "npcs/ace.ron: decisions.greet.fallback: boom"
        );
    }

    #[test]
    fn an_empty_path_displays_the_file_and_the_message_only() {
        let finding = Finding::new("pack.ron", Location::File, rules::PACK_MANIFEST, "boom");
        assert_eq!(finding.to_string(), "pack.ron: boom");
    }

    /// The check's text and JSON forms spell a severity this one way
    /// (check-and-preview spec §8), so the spelling has one owner.
    #[test]
    fn a_severity_has_one_wire_spelling() {
        assert_eq!(Severity::Error.as_str(), "error");
        assert_eq!(Severity::Warning.as_str(), "warning");
    }

    #[test]
    fn a_built_finding_is_an_error() {
        let path = Finding::new(
            "case",
            Location::Field("step".to_string()),
            rules::CASE_REFERENCE,
            "boom",
        );
        assert_eq!(path.severity, Severity::Error);
        let line = Finding::at_line("case", 3, rules::EXPECT, "boom");
        assert_eq!(line.severity, Severity::Error);
    }

    /// A warning keeps the display text the error screen and the session log
    /// already showed: the severity is carried, never printed.
    #[test]
    fn a_warning_is_a_warning_and_prints_no_severity() {
        let finding = Finding::warning(
            "pack.ron",
            Location::File,
            rules::PACK_MANIFEST_UNLISTED,
            "unlisted",
        );
        assert_eq!(finding.severity, Severity::Warning);
        assert_eq!(finding.to_string(), "pack.ron: unlisted");
    }

    #[test]
    fn a_line_path_needs_a_number_after_the_word() {
        let finding = Finding::new(
            "case",
            Location::Field("line sideways".to_string()),
            rules::CASE_REFERENCE,
            "boom",
        );
        assert_eq!(finding.to_string(), "case: line sideways: boom");
    }

    #[test]
    fn a_keiko_line_finding_keeps_its_rule_and_path() {
        let finding = Finding::at_line("tests/a.keiko", 4, rules::EXPECT, "expected 1");
        assert_eq!(finding.file, "tests/a.keiko");
        assert_eq!(finding.path, Location::Line(4));
        assert_eq!(finding.rule, "keiko.expect");
    }

    /// The wire form is the one text the check's JSON and Keiko's JSON already
    /// carry: an empty string, `line <n>` or the field path, byte for byte.
    #[test]
    fn a_location_keeps_its_wire_form_in_both_directions() {
        let cases = [
            (Location::File, ""),
            (Location::Line(12), "line 12"),
            (
                Location::Field("decisions.greet.fallback".to_string()),
                "decisions.greet.fallback",
            ),
        ];
        for (location, wire) in cases {
            let finding = Finding::new("npcs/ace.ron", location.clone(), rules::PACK_DECISION, "m");
            let json = serde_json::to_string(&finding).unwrap();
            assert!(json.contains(&format!("\"path\":\"{wire}\"")), "{json}");
            let back: Finding = serde_json::from_str(&json).unwrap();
            assert_eq!(back, finding);
            assert_eq!(back.path, location);
        }
    }

    #[test]
    fn a_line_word_without_digits_reads_back_as_a_field() {
        for wire in ["line x", "line ", "line 1x", "line -1"] {
            let json = format!(
                r#"{{"file":"f","path":"{wire}","rule":"r","severity":"Error","message":"m"}}"#
            );
            let finding: Finding = serde_json::from_str(&json).unwrap();
            assert_eq!(finding.path, Location::Field(wire.to_string()), "{wire}");
        }
    }

    #[test]
    fn a_line_sorts_by_number_and_a_finding_without_a_line_sorts_first() {
        let mut findings = vec![
            Finding::at_line("a", 10, "r", "m"),
            Finding::at_line("a", 9, "r", "m"),
            Finding::new("a", Location::File, "r", "m"),
        ];
        sort_findings(&mut findings);
        let lines: Vec<Option<usize>> =
            findings.iter().map(|finding| finding.path.line()).collect();
        assert_eq!(lines, [None, Some(9), Some(10)]);
    }

    #[test]
    fn the_log_row_rule_keeps_the_row_number() {
        assert_eq!(rules::log_row(7), "keiko.log.row-7");
    }

    /// The spec §3 order every consumer sees: file, then line with a finding
    /// that has no line first, then rule id, then message. The no-line-first and
    /// numeric-line keys matter because the loader's findings reach other
    /// modules' paths too.
    #[test]
    fn sorting_uses_file_then_line_then_rule_then_message() {
        let mut findings = vec![
            Finding::at_line("b.unmei", 10, rules::PACK_SCENE_TARGET, "ten"),
            Finding::new(
                "a.ron",
                Location::Field("decisions.greet".to_string()),
                rules::PACK_DECISION,
                "no line",
            ),
            Finding::at_line("a.ron", 9, rules::PACK_SCENE_SYNTAX, "nine"),
            Finding::at_line("a.ron", 9, rules::PACK_STATE, "beta"),
            Finding::at_line("a.ron", 9, rules::PACK_STATE, "alpha"),
            Finding::new("a.ron", Location::File, rules::PACK_NPC, "file wide"),
            Finding::at_line("b.unmei", 2, rules::PACK_SCENE_SYNTAX, "two"),
        ];
        sort_findings(&mut findings);
        let wires: Vec<String> = findings.iter().map(|f| f.path.to_string()).collect();
        let order: Vec<(&str, &str, &str, &str)> = findings
            .iter()
            .zip(&wires)
            .map(|(f, wire)| {
                (
                    f.file.as_str(),
                    wire.as_str(),
                    f.rule.as_str(),
                    f.message.as_str(),
                )
            })
            .collect();
        assert_eq!(
            order,
            [
                ("a.ron", "decisions.greet", rules::PACK_DECISION, "no line"),
                ("a.ron", "", rules::PACK_NPC, "file wide"),
                ("a.ron", "line 9", rules::PACK_SCENE_SYNTAX, "nine"),
                ("a.ron", "line 9", rules::PACK_STATE, "alpha"),
                ("a.ron", "line 9", rules::PACK_STATE, "beta"),
                ("b.unmei", "line 2", rules::PACK_SCENE_SYNTAX, "two"),
                ("b.unmei", "line 10", rules::PACK_SCENE_TARGET, "ten"),
            ]
        );
    }

    /// Every id is the string §4 of the check-and-preview spec lists: a typo in
    /// a constant would otherwise only show up in a downstream consumer.
    #[test]
    fn the_pack_rule_ids_match_the_spec_table() {
        assert_eq!(rules::PACK_FILE_READ, "pack.file.read");
        assert_eq!(rules::PACK_FILE_RON, "pack.file.ron");
        assert_eq!(rules::PACK_MANIFEST, "pack.manifest");
        assert_eq!(rules::PACK_MANIFEST_UNLISTED, "pack.manifest.unlisted");
        assert_eq!(
            rules::PACK_MANIFEST_TAGLINE_LENGTH,
            "pack.manifest.tagline-length"
        );
        assert_eq!(rules::PACK_SCENE_SYNTAX, "pack.scene.syntax");
        assert_eq!(rules::PACK_SCENE_TARGET, "pack.scene.target");
        assert_eq!(rules::PACK_SCENE_CYCLE, "pack.scene.cycle");
        assert_eq!(rules::PACK_STATE, "pack.state");
        assert_eq!(rules::PACK_NPC, "pack.npc");
        assert_eq!(rules::PACK_DECISION, "pack.decision");
        assert_eq!(rules::PACK_DECISION_NO_NONE, "pack.decision.no-none");
        assert_eq!(rules::PACK_ASSET_MISSING, "pack.asset.missing");
        assert_eq!(rules::PACK_ASSET_DECODE, "pack.asset.decode");
        assert_eq!(rules::FLOW_UNREACHABLE_SCENE, "flow.unreachable-scene");
        assert_eq!(rules::FLOW_UNREACHABLE_LABEL, "flow.unreachable-label");
        assert_eq!(rules::FLOW_NO_EXIT, "flow.no-exit");
        assert_eq!(
            rules::PREVIEW_STATE_UNREACHABLE,
            "preview.state.unreachable"
        );
    }

    /// The slice-1 plugin ids, spelled as the UI plugin spec §8 table spells
    /// them.
    #[test]
    fn the_ui_rule_ids_match_the_spec_table() {
        assert_eq!(rules::UI_FILE_READ, "ui.file.read");
        assert_eq!(rules::UI_PARSE, "ui.parse");
        assert_eq!(rules::UI_VERSION, "ui.version");
        assert_eq!(rules::UI_PATH_ESCAPE, "ui.path.escape");
        assert_eq!(rules::UI_CAP, "ui.cap");
        assert_eq!(rules::UI_TOKEN_UNKNOWN, "ui.token.unknown");
        assert_eq!(rules::UI_TOKEN_CYCLE, "ui.token.cycle");
        assert_eq!(rules::UI_SURFACE_UNKNOWN, "ui.surface.unknown");
        assert_eq!(rules::UI_SLOT_UNKNOWN, "ui.slot.unknown");
        assert_eq!(rules::UI_SLOT_MISSING, "ui.slot.missing");
        assert_eq!(rules::UI_SLOT_DUPLICATE, "ui.slot.duplicate");
        assert_eq!(rules::UI_COPY_UNKNOWN, "ui.copy.unknown");
        assert_eq!(rules::UI_COPY_PLACEHOLDER, "ui.copy.placeholder");
        assert_eq!(rules::UI_FONT_LICENSE, "ui.font.license");
        assert_eq!(rules::UI_FONT_UNKNOWN, "ui.font.unknown");
        assert_eq!(rules::UI_MOTION_FORBIDDEN, "ui.motion.forbidden");
        assert_eq!(rules::UI_MOTION_FLASH, "ui.motion.flash");
    }
}
