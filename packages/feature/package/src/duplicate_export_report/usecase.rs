//! Duplicate `defpackage` `:export` detection across explicit files.

pub use crate::duplicate_export_report::domain::{
    DuplicateExportItem, DuplicateExportPolicy, DuplicateExportPolicyOptions,
    DuplicateExportSummary, collect_duplicate_exports, evaluate_duplicate_export_policy,
    summarize_duplicate_exports,
};
