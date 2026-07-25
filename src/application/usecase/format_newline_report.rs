//! Format-newline ((format t "~%") is (terpri)) detection.

pub use crate::domain::format_newline_report::{
    FormatNewlineItem, FormatNewlinePolicy, FormatNewlinePolicyOptions, FormatNewlineSummary,
    collect_format_newlines, evaluate_format_newline_policy, summarize_format_newlines,
};
