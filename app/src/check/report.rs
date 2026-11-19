//! The check's output: §3's order and the two §8 forms.

use serde_json::{json, Value};
use unmei_engine::{Finding, Severity};

use crate::launch::Format;

/// Sort the findings into §3's order and return the error and warning counts.
/// The order itself is the engine's, so the loader, the check and every later
/// consumer cannot drift apart.
pub fn sort(findings: &mut [Finding]) -> (usize, usize) {
    unmei_engine::finding::sort_findings(findings);
    let errors = findings
        .iter()
        .filter(|finding| finding.severity == Severity::Error)
        .count();
    (errors, findings.len() - errors)
}

/// Sort the findings, print the chosen form, and return the error and warning
/// counts. `label` names the mode whose summary line this is (§8).
pub fn print(findings: &mut [Finding], format: Format, label: &str) -> (usize, usize) {
    let (errors, warnings) = sort(findings);
    if matches!(format, Format::Json) {
        println!("{}", document(findings, errors, warnings));
    } else {
        for finding in findings.iter() {
            println!("{}", text_line(finding));
        }
        println!("{}", summary(errors, warnings, label));
    }
    (errors, warnings)
}

/// One text line: `<severity>: <display> [<rule>]`.
pub fn text_line(finding: &Finding) -> String {
    format!(
        "{}: {} [{}]",
        severity_name(finding.severity),
        finding,
        finding.rule
    )
}

/// The text form's closing line.
pub fn summary(errors: usize, warnings: usize, label: &str) -> String {
    format!("{label}: {errors} errors, {warnings} warnings")
}

/// The JSON document: findings under the four C3 fields plus `severity`, with
/// the two counts (§8).
pub fn document(findings: &[Finding], errors: usize, warnings: usize) -> String {
    let findings: Vec<Value> = findings
        .iter()
        .map(|finding| {
            json!({
                "file": &finding.file,
                "path": &finding.path,
                "rule": &finding.rule,
                "severity": severity_name(finding.severity),
                "message": &finding.message,
            })
        })
        .collect();
    json!({
        "findings": findings,
        "errors": errors,
        "warnings": warnings,
    })
    .to_string()
}

/// The lower-case spelling the check's output uses for a severity.
pub fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unmei_engine::finding::rules;

    /// `file|line|rule|message`, so a failing order names the pair that moved.
    fn key(finding: &Finding) -> String {
        format!(
            "{}|{}|{}|{}",
            finding.file,
            finding.line_number().unwrap_or(0),
            finding.rule,
            finding.message
        )
    }

    #[test]
    fn the_order_is_file_then_line_then_rule_then_message() {
        let mut findings = vec![
            Finding::at_line("scenes/b.unmei", 2, rules::PACK_STATE, "b"),
            Finding::new("scenes/a.unmei", "", rules::PACK_MANIFEST, "whole file"),
            Finding::at_line("scenes/a.unmei", 9, rules::PACK_STATE, "late"),
            Finding::at_line("scenes/a.unmei", 9, rules::PACK_SCENE_TARGET, "rule order"),
            Finding::at_line(
                "scenes/a.unmei",
                9,
                rules::PACK_SCENE_TARGET,
                "message order",
            ),
            Finding::at_line("scenes/a.unmei", 3, rules::PACK_STATE, "early"),
        ];
        let (errors, warnings) = sort(&mut findings);
        assert_eq!((errors, warnings), (6, 0));
        let got: Vec<String> = findings.iter().map(key).collect();
        assert_eq!(
            got,
            [
                "scenes/a.unmei|0|pack.manifest|whole file",
                "scenes/a.unmei|3|pack.state|early",
                "scenes/a.unmei|9|pack.scene.target|message order",
                "scenes/a.unmei|9|pack.scene.target|rule order",
                "scenes/a.unmei|9|pack.state|late",
                "scenes/b.unmei|2|pack.state|b",
            ]
        );
    }

    /// A dotted RON path is not a line, so it sorts with the no-line findings.
    #[test]
    fn a_path_that_is_not_a_line_sorts_before_every_line() {
        let mut findings = vec![
            Finding::at_line("npcs/ace.ron", 4, rules::PACK_DECISION, "line"),
            Finding::new(
                "npcs/ace.ron",
                "decisions.greet.fallback",
                rules::PACK_DECISION,
                "path",
            ),
        ];
        sort(&mut findings);
        assert_eq!(findings[0].path, "decisions.greet.fallback");
    }

    #[test]
    fn a_warning_counts_as_a_warning_and_keeps_its_display_text() {
        let warning = Finding::warning(
            "pack.ron",
            "",
            rules::PACK_MANIFEST_TAGLINE_LENGTH,
            "tagline is too long",
        );
        assert_eq!(
            text_line(&warning),
            "warning: pack.ron: tagline is too long [pack.manifest.tagline-length]"
        );
        assert_eq!(severity_name(warning.severity), "warning");
        assert_eq!(summary(2, 1, "check"), "check: 2 errors, 1 warnings");
    }

    #[test]
    fn the_json_document_has_the_five_fields_and_lower_case_severity() {
        let error = Finding::at_line("tests/a.keiko", 2, rules::EXPECT, "expected 1");
        let warning = Finding::warning("pack.ron", "", rules::PACK_MANIFEST_UNLISTED, "unlisted");
        let document = document(&[error, warning], 1, 1);
        let value: Value = serde_json::from_str(&document).expect("one JSON document");

        let mut keys: Vec<&str> = value
            .as_object()
            .expect("one object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["errors", "findings", "warnings"]);
        assert_eq!(value["errors"], 1);
        assert_eq!(value["warnings"], 1);

        let findings = value["findings"].as_array().expect("a findings list");
        let mut fields: Vec<&str> = findings[0]
            .as_object()
            .expect("one finding")
            .keys()
            .map(String::as_str)
            .collect();
        fields.sort_unstable();
        assert_eq!(fields, ["file", "message", "path", "rule", "severity"]);
        assert_eq!(findings[0]["severity"], "error");
        assert_eq!(findings[1]["severity"], "warning");
    }
}
