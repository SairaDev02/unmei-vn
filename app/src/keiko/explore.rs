//! The bounded exploration walk (spec §9). Implemented in the exploration
//! ticket; this placeholder keeps the mode's wiring honest while the walk is
//! built.

use std::path::Path;

use serde_json::{json, Value};
use unmei_engine::Pack;

pub struct Report {
    pub findings: Vec<Value>,
    pub coverage: Value,
    pub used: u32,
    pub limit: u32,
    pub budget_spent: bool,
}

impl Report {
    pub fn to_json(&self) -> Value {
        json!({
            "findings": self.findings,
            "coverage": self.coverage,
            "budget": { "limit": self.limit, "used": self.used },
        })
    }

    pub fn print(&self) {
        for finding in &self.findings {
            println!("{finding}");
        }
        println!(
            "coverage: {}",
            self.coverage
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or("none")
        );
        println!(
            "budget: {} of {}{}",
            self.used,
            self.limit,
            if self.budget_spent { " (budget spent)" } else { "" }
        );
    }
}

pub fn run(
    _pack_root: &Path,
    _pack: &Pack,
    budget: u32,
    _from: Option<&str>,
    _keep: bool,
) -> Report {
    Report {
        findings: Vec::new(),
        coverage: json!({ "summary": "exploration is not built yet" }),
        used: 0,
        limit: budget,
        budget_spent: false,
    }
}
