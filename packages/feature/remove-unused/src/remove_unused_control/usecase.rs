//! Application facade for removing unused Common Lisp control forms.

pub use crate::domain::remove_unused_control::{
    RemoveUnusedControlPlan, RemoveUnusedControlRequest, plan_remove_unused_block,
    plan_remove_unused_tag,
};
