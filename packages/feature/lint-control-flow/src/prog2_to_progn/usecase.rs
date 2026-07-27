//! Prog2-to-progn ((prog2 a b) is (progn a b)) detection.

pub use crate::domain::prog2_to_progn_report::{
    Prog2ToPrognItem, Prog2ToPrognPolicy, Prog2ToPrognPolicyOptions, Prog2ToPrognSummary,
    collect_prog2_to_progn, evaluate_prog2_to_progn_policy, summarize_prog2_to_progn,
};
