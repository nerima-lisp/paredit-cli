//! Flow-narrowing site reporting across a set of files.

pub use crate::narrowing_report::domain::{
    Branch, NarrowingReportFile, NarrowingSite, NarrowingSource, build_narrowing_report,
};

#[derive(Debug, Clone, Copy)]
pub struct NarrowingPolicyOptions {
    pub fail_on_none: bool,
}

impl NarrowingPolicyOptions {
    #[must_use]
    pub const fn new(fail_on_none: bool) -> Self {
        Self { fail_on_none }
    }
}

#[derive(Debug, Clone)]
pub struct NarrowingPolicy {
    pub fail_on_none: bool,
    pub site_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

#[must_use]
pub fn evaluate_narrowing_policy(
    options: NarrowingPolicyOptions,
    reports: &[NarrowingReportFile],
) -> NarrowingPolicy {
    let site_count = reports.iter().map(|report| report.sites.len()).sum();

    // The gate a caller wants here is the inverse of every other report's:
    // narrowing sites are not defects, so the useful assertion is "this file
    // is supposed to have some and does not", which catches a rewrite that
    // silently dropped a type guard.
    let violations = reports
        .iter()
        .filter(|report| options.fail_on_none && report.dialect_modelled && report.sites.is_empty())
        .map(|report| format!("{} narrows no binding in any branch", report.path.display()))
        .collect::<Vec<_>>();

    NarrowingPolicy {
        fail_on_none: options.fail_on_none,
        site_count,
        passed: violations.is_empty(),
        violations,
    }
}
