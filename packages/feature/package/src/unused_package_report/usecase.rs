//! Cross-file unused `defpackage` detection across explicit files.

pub use crate::unused_package_report::domain::{
    DeclaredPackage, UnusedPackageItem, UnusedPackagePolicy, UnusedPackagePolicyOptions,
    UnusedPackageSummary, analyze_unused_packages, collect_declared_packages,
    collect_referenced_package_names, evaluate_unused_package_policy,
};
