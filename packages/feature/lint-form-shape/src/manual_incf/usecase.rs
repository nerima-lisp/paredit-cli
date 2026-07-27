//! Manual-`incf`/`decf` (`(setf x (1+ x))`, better written `(incf x)`) detection
//! across explicit files.

pub use crate::manual_incf::domain::{
    ManualIncfItem, ManualIncfPolicy, ManualIncfPolicyOptions, ManualIncfSummary,
    collect_manual_incfs, evaluate_manual_incf_policy, summarize_manual_incfs,
};
