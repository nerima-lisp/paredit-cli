//! Redundant :radix 10 ((parse-integer s :radix 10) is (parse-integer s)) detection.

pub use crate::domain::parse_integer_default_radix_report::{
    ParseIntegerDefaultRadixItem, ParseIntegerDefaultRadixPolicy,
    ParseIntegerDefaultRadixPolicyOptions, ParseIntegerDefaultRadixSummary,
    collect_parse_integer_default_radixes, evaluate_parse_integer_default_radix_policy,
    summarize_parse_integer_default_radixes,
};
