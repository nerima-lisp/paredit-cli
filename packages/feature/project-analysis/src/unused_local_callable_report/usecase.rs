//! Unused `flet`/`labels` local-callable detection across explicit files.

pub use crate::unused_local_callable_report::domain::{
    UnusedLocalCallableItem, UnusedLocalCallablePolicy, UnusedLocalCallablePolicyOptions,
    UnusedLocalCallableReportFile, build_unused_local_callable_report,
    evaluate_unused_local_callable_policy,
};
