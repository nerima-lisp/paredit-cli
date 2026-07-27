//! Code-char-of-char-code (`(code-char (char-code c))` is `c`) detection across
//! explicit files.

pub use crate::code_char_char_code::domain::{
    CodeCharCharCodeItem, CodeCharCharCodePolicy, CodeCharCharCodePolicyOptions,
    CodeCharCharCodeSummary, collect_code_char_char_codes, evaluate_code_char_char_code_policy,
    summarize_code_char_char_codes,
};
