//! Cross-file undefined in-package target detection across explicit files.

pub use crate::domain::undefined_package_report::{
    InPackageReference, UndefinedPackageItem, UndefinedPackagePolicy,
    UndefinedPackagePolicyOptions, UndefinedPackageSummary, analyze_undefined_packages,
    collect_declared_package_names, collect_in_package_references,
    evaluate_undefined_package_policy,
};
