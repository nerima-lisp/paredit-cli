//! Unwind-protect-no-cleanup ((unwind-protect x) is x) detection.

pub use crate::unwind_protect_no_cleanup::domain::{
    UnwindProtectNoCleanupItem, UnwindProtectNoCleanupPolicy, UnwindProtectNoCleanupPolicyOptions,
    UnwindProtectNoCleanupSummary, collect_unwind_protect_no_cleanup,
    evaluate_unwind_protect_no_cleanup_policy, summarize_unwind_protect_no_cleanup,
};
