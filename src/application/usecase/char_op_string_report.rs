//! Character-function-on-a-string (`(char= "a" c)`, `(char-code "x")` — a
//! guaranteed type error) detection across explicit files.

pub use crate::domain::char_op_string_report::{
    CharOpStringItem, CharOpStringPolicy, CharOpStringPolicyOptions, CharOpStringSummary,
    collect_char_op_strings, evaluate_char_op_string_policy, summarize_char_op_strings,
};
