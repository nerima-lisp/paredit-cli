#![doc = include_str!("../README.md")]

pub mod replace_forms;
pub mod sort_definitions;
pub mod split_file;
pub mod thread_expression;
pub mod unthread_expression;
pub mod unwrap_call;

// The contract with the composition root (section 4.2): each slice that owns a
// subcommand publishes its `clap` argument type and the function that runs it.
pub use replace_forms::cli::{ReplaceFormsArgs, replace_forms};
pub use thread_expression::cli::{ThreadExpressionArgs, thread_expression};
pub use unthread_expression::cli::{UnthreadExpressionArgs, unthread_expression};
pub use unwrap_call::cli::{UnwrapCallArgs, unwrap_call};
