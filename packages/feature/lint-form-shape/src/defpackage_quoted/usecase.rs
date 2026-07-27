//! Defpackage-quoted ((:export 'foo) in defpackage is a bug) detection.

pub use crate::defpackage_quoted::domain::{
    DefpackageQuotedItem, DefpackageQuotedPolicy, DefpackageQuotedPolicyOptions,
    DefpackageQuotedSummary, collect_defpackage_quoted, evaluate_defpackage_quoted_policy,
    summarize_defpackage_quoted,
};
