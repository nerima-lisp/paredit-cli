//! Manual-`incf`/`decf` (`(setf x (1+ x))`, better written `(incf x)`) detection
//! across explicit files.

pub use crate::domain::manual_incf_report::{
    ManualIncfItem, ManualIncfPolicy, ManualIncfPolicyOptions, ManualIncfSummary,
    collect_manual_incfs, evaluate_manual_incf_policy, summarize_manual_incfs,
};
