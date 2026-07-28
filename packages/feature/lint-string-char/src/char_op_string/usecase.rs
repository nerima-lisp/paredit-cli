//! Character-function-on-a-string (`(char= "a" c)`, `(char-code "x")` — a
//! guaranteed type error) detection across explicit files.

pub use crate::char_op_string::domain::{
    CharOpStringItem, CharOpStringPolicy, CharOpStringPolicyOptions, CharOpStringSummary,
    collect_char_op_strings, evaluate_char_op_string_policy, summarize_char_op_strings,
};
