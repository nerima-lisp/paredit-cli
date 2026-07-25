//! Identical `if`-branch (`(if c a a)`) detection across explicit files.

pub use crate::domain::identical_if_branch_report::{
    IdenticalIfBranchItem, IdenticalIfBranchPolicy, IdenticalIfBranchPolicyOptions,
    IdenticalIfBranchSummary, collect_identical_if_branches, evaluate_identical_if_branch_policy,
    summarize_identical_if_branches,
};
