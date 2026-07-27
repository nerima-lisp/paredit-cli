//! Per-definition structural complexity reporting for refactor prioritization.

pub use crate::domain::complexity_report::{
    ComplexityReportFile, ComplexityReportItem, ComplexityReportPolicy,
    ComplexityReportPolicyOptions, build_complexity_report, evaluate_complexity_report_policy,
};
