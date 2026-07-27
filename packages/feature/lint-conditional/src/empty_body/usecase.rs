//! Empty-body (`(when x)`, `(dolist (x l))` — the test/spec runs then nothing
//! happens) detection across explicit files.

pub use crate::domain::empty_body_report::{
    EmptyBodyItem, EmptyBodyPolicy, EmptyBodyPolicyOptions, EmptyBodySummary, collect_empty_bodies,
    evaluate_empty_body_policy, summarize_empty_bodies,
};
