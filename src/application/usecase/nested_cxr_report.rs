//! Nested-`cXr` (`(car (cdr x))`, better written `(cadr x)`) detection across
//! explicit files.

pub use crate::domain::nested_cxr_report::{
    NestedCxrItem, NestedCxrPolicy, NestedCxrPolicyOptions, NestedCxrSummary, collect_nested_cxrs,
    evaluate_nested_cxr_policy, summarize_nested_cxrs,
};
