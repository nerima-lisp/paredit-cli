//! Call-graph reachability analysis: dead-code islands unreachable from any
//! entry point, distinct from direct-reference-only unused-definition checks.

pub use crate::domain::reachability_report::{
    ReachabilityReportItem, ReachabilityReportPolicy, ReachabilityReportPolicyOptions,
    ReachabilityReportSummary, analyze_reachability, evaluate_reachability_policy,
};
