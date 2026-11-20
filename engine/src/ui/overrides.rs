//! The manifest's copy overrides against the copy table (spec §5): a key the
//! table holds, a value that keeps the key's placeholders, and the §7 cap on one
//! override.
//!
//! The placeholder scan mirrors [`crate::copy::render`], the one reader of a
//! placeholder: one `{` at a time up to the next `}`, so a `{` no `}` closes is
//! text. A value may move a placeholder within its sentence; what it may not do
//! is drop one the key declares or name one the key does not, because the engine
//! fills a key's declared placeholders and the rest would reach the screen.

use crate::copy;
use crate::finding::{rules, Finding, Location};

use super::caps::MAX_COPY_CHARS;
use super::manifest::{self, Manifest};
use super::source::display;

/// Judge every override the manifest holds.
pub(super) fn check(manifest: &Manifest, findings: &mut Vec<Finding>) {
    let file = display(manifest::FILE);
    for (id, text) in &manifest.copy {
        let path = format!("copy[\"{id}\"]");
        let Some(key) = copy::key(id) else {
            push(
                &file,
                &path,
                rules::UI_COPY_UNKNOWN,
                format!("the copy table has no key `{id}`"),
                findings,
            );
            continue;
        };
        let length = text.chars().count();
        if length > MAX_COPY_CHARS {
            push(
                &file,
                &path,
                rules::UI_CAP,
                format!(
                    "the override of `{id}` is {length} characters; the cap is {MAX_COPY_CHARS}"
                ),
                findings,
            );
        }
        let found = placeholders(text);
        for declared in key.placeholders {
            if !found.contains(declared) {
                push(
                    &file,
                    &path,
                    rules::UI_COPY_PLACEHOLDER,
                    format!("the override of `{id}` drops the placeholder `{{{declared}}}`"),
                    findings,
                );
            }
        }
        for name in &found {
            if !key.placeholders.contains(name) {
                push(
                    &file,
                    &path,
                    rules::UI_COPY_PLACEHOLDER,
                    format!(
                        "the override of `{id}` names the placeholder `{{{name}}}`, which the key does not declare"
                    ),
                    findings,
                );
            }
        }
    }
}

/// Every `{name}` `text` carries, in the order it carries them.
fn placeholders(text: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some((_, after)) = rest.split_once('{') {
        let Some((name, tail)) = after.split_once('}') else {
            break;
        };
        found.push(name);
        rest = tail;
    }
    found
}

fn push(file: &str, path: &str, rule: &str, message: String, findings: &mut Vec<Finding>) {
    findings.push(Finding::new(
        file,
        Location::Field(path.to_string()),
        rule,
        message,
    ));
}
