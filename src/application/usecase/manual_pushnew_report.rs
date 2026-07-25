//! Manual-`pushnew` (`(setf x (adjoin item x))`, better written
//! `(pushnew item x)`) detection across explicit files.

pub use crate::domain::manual_pushnew_report::{
    ManualPushnewItem, ManualPushnewPolicy, ManualPushnewPolicyOptions, ManualPushnewSummary,
    collect_manual_pushnews, evaluate_manual_pushnew_policy, summarize_manual_pushnews,
};
