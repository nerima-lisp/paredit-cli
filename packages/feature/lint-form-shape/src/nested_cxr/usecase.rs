//! Nested-`cXr` (`(car (cdr x))`, better written `(cadr x)`) detection across
//! explicit files.

pub use crate::nested_cxr::domain::{
    NestedCxrItem, NestedCxrPolicy, NestedCxrPolicyOptions, NestedCxrSummary, collect_nested_cxrs,
    evaluate_nested_cxr_policy, summarize_nested_cxrs,
};
