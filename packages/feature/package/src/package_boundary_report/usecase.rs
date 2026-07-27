//! Common Lisp package-boundary violation detection across explicit files.

pub use crate::package_boundary_report::domain::{
    PackageBoundaryItem, PackageBoundaryPolicy, PackageBoundaryPolicyOptions,
    PackageBoundaryReportFile, build_package_boundary_report, evaluate_package_boundary_policy,
};
