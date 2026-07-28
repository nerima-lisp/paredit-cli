//! Redundant-prog1 ((prog1 x) is x) detection.

pub use crate::redundant_prog1::domain::{
    RedundantProg1Item, RedundantProg1Policy, RedundantProg1PolicyOptions, RedundantProg1Summary,
    collect_redundant_prog1s, evaluate_redundant_prog1_policy, summarize_redundant_prog1s,
};
