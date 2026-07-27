//! Identical `if`-branch (`(if c a a)`) detection across explicit files.

pub use crate::identical_if_branches::domain::{
    IdenticalIfBranchItem, IdenticalIfBranchPolicy, IdenticalIfBranchPolicyOptions,
    IdenticalIfBranchSummary, collect_identical_if_branches, evaluate_identical_if_branch_policy,
    summarize_identical_if_branches,
};
