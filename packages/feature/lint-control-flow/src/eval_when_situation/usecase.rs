//! Eval-when-situation (an eval-when with an invalid situation keyword)
//! detection across explicit files.

pub use crate::domain::eval_when_situation_report::{
    EvalWhenSituationItem, EvalWhenSituationPolicy, EvalWhenSituationPolicyOptions,
    EvalWhenSituationSummary, collect_eval_when_situations, evaluate_eval_when_situation_policy,
    summarize_eval_when_situations,
};
