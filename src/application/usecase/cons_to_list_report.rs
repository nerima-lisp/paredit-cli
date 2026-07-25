//! Cons-to-`list` (`(cons a nil)` is `(list a)`, `(cons a (list b))` is
//! `(list a b)`) detection across explicit files.

pub use crate::domain::cons_to_list_report::{
    ConsToListItem, ConsToListPolicy, ConsToListPolicyOptions, ConsToListSummary,
    collect_cons_to_lists, evaluate_cons_to_list_policy, summarize_cons_to_lists,
};
