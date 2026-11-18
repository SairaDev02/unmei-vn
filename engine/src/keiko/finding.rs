//! The finding shape and the fixed rule-id vocabulary.
//!
//! A finding is the one output the case parser, the validator and the take
//! runner all speak: the pack check's shape (`file`, `path`, `rule`, `message`),
//! so one parser handles a check's findings and a Keiko finding alike.

/// A case, an assertion or a runtime fault reported in one shape.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Finding {
    /// The file the finding is about: a case path, or the pack file a fault
    /// names.
    pub file: String,
    /// Where inside the file, as the check family spells it: `line 12`, or an
    /// empty string when the finding is about the file as a whole.
    pub path: String,
    /// One of the fixed rule ids ([`rules`]).
    pub rule: String,
    /// The human-readable detail.
    pub message: String,
}

impl Finding {
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
            message: message.into(),
        }
    }

    /// A finding about one line of a case.
    pub fn at_line(
        file: impl Into<String>,
        line: usize,
        rule: &str,
        message: impl Into<String>,
    ) -> Self {
        Self::new(file, format!("line {line}"), rule, message)
    }

    /// The same finding re-filed under another path (a fault's rule id applied
    /// to a case line).
    pub fn with_rule(mut self, rule: &str) -> Self {
        self.rule = rule.to_string();
        self
    }
}

/// The fixed rule ids. Adding a rule is an additive change to the case format's
/// version 1.
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

    /// The finding rule a failure row the pack causes is filed under.
    pub fn log_row(row: u32) -> String {
        format!("keiko.log.row-{row}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_finding_names_its_line_and_rule() {
        let finding = Finding::at_line("tests/a.keiko", 4, rules::EXPECT, "expected 1");
        assert_eq!(finding.file, "tests/a.keiko");
        assert_eq!(finding.path, "line 4");
        assert_eq!(finding.rule, "keiko.expect");
    }

    #[test]
    fn the_log_row_rule_keeps_the_row_number() {
        assert_eq!(rules::log_row(7), "keiko.log.row-7");
    }
}
