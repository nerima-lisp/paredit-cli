//! If-to-unless ((if c nil e) is (unless c e)) detection.

pub use crate::if_to_unless::domain::{
    IfToUnlessItem, IfToUnlessPolicy, IfToUnlessPolicyOptions, IfToUnlessSummary,
    collect_if_to_unless, evaluate_if_to_unless_policy, summarize_if_to_unless,
};
