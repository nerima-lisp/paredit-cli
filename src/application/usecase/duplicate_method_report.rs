//! Cross-file duplicate CLOS-method detection across explicit files.

pub use crate::domain::duplicate_method_report::{
    DeclaredMethod, DuplicateMethodItem, DuplicateMethodOccurrence, DuplicateMethodPolicy,
    DuplicateMethodPolicyOptions, DuplicateMethodSummary, analyze_duplicate_methods,
    collect_declared_methods, evaluate_duplicate_method_policy,
};
