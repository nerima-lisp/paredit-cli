//! Manual-`push` (`(setf x (cons item x))`, better written `(push item x)`)
//! detection across explicit files.

pub use crate::domain::manual_push_report::{
    ManualPushItem, ManualPushPolicy, ManualPushPolicyOptions, ManualPushSummary,
    collect_manual_pushes, evaluate_manual_push_policy, summarize_manual_pushes,
};
