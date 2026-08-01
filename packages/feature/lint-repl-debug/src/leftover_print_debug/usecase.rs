//! LeftoverPrintDebug (a bare debug-print call left in committed source) detection.

pub use crate::leftover_print_debug::domain::{
    LeftoverPrintDebugItem, LeftoverPrintDebugPolicy, LeftoverPrintDebugPolicyOptions,
    LeftoverPrintDebugSummary, collect_leftover_print_debug, evaluate_leftover_print_debug_policy,
    summarize_leftover_print_debug,
};
