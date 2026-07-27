//! Per-definition structural complexity reporting for refactor prioritization.

pub use crate::complexity_report::domain::{
    ComplexityReportFile, ComplexityReportItem, ComplexityReportPolicy,
    ComplexityReportPolicyOptions, build_complexity_report, evaluate_complexity_report_policy,
};
