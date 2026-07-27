//! Exhaustive-case-otherwise (a forbidden t/otherwise clause in
//! ecase/ccase/etypecase/ctypecase) detection across explicit files.

pub use crate::domain::exhaustive_case_otherwise_report::{
    ExhaustiveCaseOtherwiseItem, ExhaustiveCaseOtherwisePolicy,
    ExhaustiveCaseOtherwisePolicyOptions, ExhaustiveCaseOtherwiseSummary,
    collect_exhaustive_case_otherwise, evaluate_exhaustive_case_otherwise_policy,
    summarize_exhaustive_case_otherwise,
};
