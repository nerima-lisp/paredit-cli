//! Format-to-string ((format nil "~A" x) is (princ-to-string x)) detection.

pub use crate::domain::format_to_string_report::{
    FormatToStringItem, FormatToStringPolicy, FormatToStringPolicyOptions, FormatToStringSummary,
    collect_format_to_strings, evaluate_format_to_string_policy, summarize_format_to_strings,
};
