//! The CI gate a lint run is judged against.

use super::Severity;

/// What the caller asked to fail the run on.
#[derive(Debug, Clone, Copy)]
pub struct LintPolicyOptions {
    fail_on_finding: bool,
    fail_on_severity: Option<Severity>,
}

impl LintPolicyOptions {
    #[must_use]
    pub fn new(fail_on_finding: bool, fail_on_severity: Option<Severity>) -> Self {
        Self {
            fail_on_finding,
            fail_on_severity,
        }
    }

    #[must_use]
    pub const fn fail_on_finding(self) -> bool {
        self.fail_on_finding
    }

    #[must_use]
    pub const fn fail_on_severity(self) -> Option<Severity> {
        self.fail_on_severity
    }
}

/// The verdict: which requested gates the run violated, if any.
#[derive(Debug)]
pub struct LintPolicy {
    pub fail_on_finding: bool,
    pub finding_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}
