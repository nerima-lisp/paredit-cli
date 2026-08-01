#![doc = include_str!("../README.md")]

pub mod pass;
pub mod pattern;
pub mod ruleset;

pub use pass::{CustomFinding, TestFailure, run, run_tests, skipped_by_dialect};
pub use ruleset::{CustomRule, RuleTest, Ruleset, RulesetError, parse_ruleset};
