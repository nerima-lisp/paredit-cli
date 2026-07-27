//! `eq`-on-a-character (`(eq c #\a)` — unreliable identity on characters)
//! detection across explicit files.

pub use crate::domain::eq_char_comparison_report::{
    EqCharComparisonItem, EqCharComparisonPolicy, EqCharComparisonPolicyOptions,
    EqCharComparisonSummary, collect_eq_char_comparisons, evaluate_eq_char_comparison_policy,
    summarize_eq_char_comparisons,
};
