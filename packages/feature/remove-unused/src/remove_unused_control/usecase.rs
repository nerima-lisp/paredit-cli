//! Application facade for removing unused Common Lisp control forms.

pub use crate::remove_unused_control::domain::{
    RemoveUnusedControlPlan, RemoveUnusedControlRequest, plan_remove_unused_block,
    plan_remove_unused_tag,
};
