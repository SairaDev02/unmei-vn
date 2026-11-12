//! The walk's accumulated pass/fail report and its final summary.

/// The walk's accumulated result: a step is only "passed" when its own check
/// says so, and the process exits non-zero if any check failed.
#[derive(Default)]
pub struct Report {
    pub checks: usize,
    pub failures: Vec<String>,
    printed: bool,
}

impl Report {
    pub fn pass(&mut self, what: impl std::fmt::Display) {
        self.checks += 1;
        println!("  ok: {what}");
    }

    pub fn check(&mut self, what: impl std::fmt::Display, ok: bool) {
        if ok {
            self.pass(what);
        } else {
            self.fail(format!("{what}"));
        }
    }

    pub fn near(&mut self, what: &str, actual: f32, expected: f32, tolerance: f32) {
        let delta = (actual - expected).abs();
        println!("  {what}: {actual:.1} (expected {expected:.1} ±{tolerance:.1})");
        if delta > tolerance {
            self.fail(format!(
                "{what} is {actual:.1}, expected {expected:.1} ±{tolerance:.1}"
            ));
        } else {
            self.checks += 1;
        }
    }

    pub fn fail(&mut self, what: impl std::fmt::Display) {
        let what = what.to_string();
        println!("FAILED: {what}");
        self.failures.push(what);
    }

    pub fn finish(&mut self) -> bool {
        if self.printed {
            return self.failures.is_empty();
        }
        self.printed = true;
        if self.failures.is_empty() {
            println!(
                "\nPASS: {} rendered acceptance checks across the Monitor walk.",
                self.checks
            );
            true
        } else {
            println!(
                "\nFAIL: {} of {} checks failed:",
                self.failures.len(),
                self.checks + self.failures.len()
            );
            for failure in &self.failures {
                println!("  - {failure}");
            }
            false
        }
    }
}
