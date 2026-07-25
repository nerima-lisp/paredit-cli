//! Common Lisp package-boundary violation detection across explicit files.

pub use crate::domain::package_boundary_report::{
    PackageBoundaryItem, PackageBoundaryPolicy, PackageBoundaryPolicyOptions,
    PackageBoundaryReportFile, build_package_boundary_report, evaluate_package_boundary_policy,
};
