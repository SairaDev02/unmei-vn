//! The finding shape, its severity and the fixed rule-id vocabulary.
//!
//! A finding is the one diagnostic shape the loader, the check, the preview
//! and Keiko all report: the pack file, the path inside it, one rule id, a
//! severity and a message. `unmei_engine::keiko` re-exports it, so a case
//! finding and a pack problem share one type and one text form.

use std::fmt;

/// Whether a finding stops the load or only reports something real but
/// non-fatal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Severity {
    Error,
    Warning,
}

/// One problem, reported in the one shape every engine surface speaks.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Finding {
    /// The pack-relative file, such as `scenes/reunion.unmei`, `npcs/ace.ron`
    /// or `tests/opening.keiko`.
    pub file: String,
    /// Where inside the file: `line <n>` for a line, a dotted RON path such as
    /// `decisions.greet.fallback` when the rule knows one, or an empty string
    /// for the file as a whole.
    pub path: String,
    /// One of the fixed rule ids ([`rules`]).
    pub rule: String,
    /// Whether the problem stops the load.
    pub severity: Severity,
    /// The human-readable detail.
    pub message: String,
}

impl Finding {
    /// An error finding, the severity every Keiko finding carries.
    pub fn new(
        file: impl Into<String>,
        path: impl Into<String>,
        rule: &str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            file: file.into(),
            path: path.into(),
            rule: rule.to_string(),
            severity: Severity::Error,
            message: message.into(),
        }
    }

    /// A warning finding: real, reported, but not a rejection.
    pub fn warning(
        file: impl Into<String>,
        path: impl Into<String>,
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
        Self::new(file, format!("line {line}"), rule, message)
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

    /// The number in a `line <n>` path; `None` for any other path.
    pub fn line_number(&self) -> Option<usize> {
        line_digits(&self.path)?.parse().ok()
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
            .then_with(|| a.line_number().cmp(&b.line_number()))
            .then_with(|| a.rule.cmp(&b.rule))
            .then_with(|| a.message.cmp(&b.message))
            .then_with(|| a.path.cmp(&b.path))
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
        match line_digits(&self.path) {
            Some(line) => write!(f, "{}:{}: {}", self.file, line, self.message),
            None if self.path.is_empty() => write!(f, "{}: {}", self.file, self.message),
            None => write!(f, "{}: {}: {}", self.file, self.path, self.message),
        }
    }
}

/// The digits in a `line <n>` path, for the `file:line: message` display form.
fn line_digits(path: &str) -> Option<&str> {
    let digits = path.strip_prefix("line ")?;
    (!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())).then_some(digits)
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

    pub const FLOW_UNREACHABLE_SCENE: &str = "flow.unreachable-scene";
    pub const FLOW_UNREACHABLE_LABEL: &str = "flow.unreachable-label";
    pub const FLOW_NO_EXIT: &str = "flow.no-exit";

    pub const PREVIEW_STATE_UNREACHABLE: &str = "preview.state.unreachable";

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
        assert_eq!(finding.path, "line 4");
        assert_eq!(finding.to_string(), "scenes/a.unmei:4: boom");
    }

    #[test]
    fn another_path_displays_between_the_file_and_the_message() {
        let finding = Finding::new(
            "npcs/ace.ron",
            "decisions.greet.fallback",
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
        let finding = Finding::new("pack.ron", "", rules::PACK_MANIFEST, "boom");
        assert_eq!(finding.to_string(), "pack.ron: boom");
    }

    #[test]
    fn a_built_finding_is_an_error() {
        let path = Finding::new("case", "step", rules::CASE_REFERENCE, "boom");
        assert_eq!(path.severity, Severity::Error);
        let line = Finding::at_line("case", 3, rules::EXPECT, "boom");
        assert_eq!(line.severity, Severity::Error);
    }

    /// A warning keeps the display text the error screen and the session log
    /// already showed: the severity is carried, never printed.
    #[test]
    fn a_warning_is_a_warning_and_prints_no_severity() {
        let finding = Finding::warning("pack.ron", "", rules::PACK_MANIFEST_UNLISTED, "unlisted");
        assert_eq!(finding.severity, Severity::Warning);
        assert_eq!(finding.to_string(), "pack.ron: unlisted");
    }

    #[test]
    fn a_line_path_needs_a_number_after_the_word() {
        let finding = Finding::new("case", "line sideways", rules::CASE_REFERENCE, "boom");
        assert_eq!(finding.to_string(), "case: line sideways: boom");
    }

    #[test]
    fn a_keiko_line_finding_keeps_its_rule_and_path() {
        let finding = Finding::at_line("tests/a.keiko", 4, rules::EXPECT, "expected 1");
        assert_eq!(finding.file, "tests/a.keiko");
        assert_eq!(finding.path, "line 4");
        assert_eq!(finding.rule, "keiko.expect");
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
            Finding::new("a.ron", "decisions.greet", rules::PACK_DECISION, "no line"),
            Finding::at_line("a.ron", 9, rules::PACK_SCENE_SYNTAX, "nine"),
            Finding::at_line("a.ron", 9, rules::PACK_STATE, "beta"),
            Finding::at_line("a.ron", 9, rules::PACK_STATE, "alpha"),
            Finding::new("a.ron", "", rules::PACK_NPC, "file wide"),
            Finding::at_line("b.unmei", 2, rules::PACK_SCENE_SYNTAX, "two"),
        ];
        sort_findings(&mut findings);
        let order: Vec<(&str, &str, &str, &str)> = findings
            .iter()
            .map(|f| {
                (
                    f.file.as_str(),
                    f.path.as_str(),
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
        assert_eq!(rules::FLOW_UNREACHABLE_SCENE, "flow.unreachable-scene");
        assert_eq!(rules::FLOW_UNREACHABLE_LABEL, "flow.unreachable-label");
        assert_eq!(rules::FLOW_NO_EXIT, "flow.no-exit");
    }
}
