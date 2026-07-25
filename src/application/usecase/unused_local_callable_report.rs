//! Unused `flet`/`labels` local-callable detection across explicit files.

pub use crate::domain::unused_local_callable_report::{
    UnusedLocalCallableItem, UnusedLocalCallablePolicy, UnusedLocalCallablePolicyOptions,
    UnusedLocalCallableReportFile, build_unused_local_callable_report,
    evaluate_unused_local_callable_policy,
};
