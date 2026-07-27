//! Cross-file package name/nickname identity conflict detection across explicit files.

pub use crate::domain::package_conflict_report::{
    DeclaredPackageIdentifier, PackageConflictItem, PackageConflictOccurrence,
    PackageConflictPolicy, PackageConflictPolicyOptions, PackageConflictSummary,
    analyze_package_conflicts, collect_declared_package_identifiers,
    evaluate_package_conflict_policy,
};
