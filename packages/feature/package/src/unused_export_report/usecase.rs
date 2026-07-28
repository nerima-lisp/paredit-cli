//! Cross-file unused `defpackage` `:export` detection across explicit files.

pub use crate::unused_export_report::domain::{
    DeclaredExport, ReferencedSymbol, UnusedExportItem, UnusedExportPolicy,
    UnusedExportPolicyOptions, UnusedExportSummary, analyze_unused_exports,
    collect_declared_exports, collect_referenced_symbols, evaluate_unused_export_policy,
};
