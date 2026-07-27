#![doc = include_str!("../README.md")]

pub mod definition_movement;
pub mod definition_removal;
pub mod definition_report;
pub mod error;
pub mod remove_definition;
pub mod remove_unused_binding;
pub mod remove_unused_control;
pub mod remove_unused_definition;

// The contract with the composition root (section 4.2): each slice that
// owns a subcommand publishes its `clap` argument type and the function
// that runs it. command.rs and dispatch.rs need these two names and no more.
pub use definition_movement::cli::{InsertTopLevelArgs, insert_top_level};
pub use definition_movement::cli::{MoveDefinitionArgs, move_definition};
pub use definition_movement::cli::{MoveFormArgs, move_form};
pub use definition_movement::cli::{SortDefinitionsArgs, sort_definitions};
pub use definition_movement::cli::{SplitFileArgs, split_file};
pub use definition_removal::cli::{
    RemoveDefinitionArgs, remove_definition as run_remove_definition,
};
pub use definition_removal::cli::{RemoveUnusedDefinitionsArgs, remove_unused_definitions};
pub use definition_report::cli::{DefinitionReportArgs, definition_report};
pub use definition_report::cli::{UnusedDefinitionReportArgs, unused_definition_report};
pub use remove_unused_binding::cli::{RemoveUnusedBindingArgs, remove_unused_binding};
pub use remove_unused_control::cli::{RemoveUnusedBlockArgs, remove_unused_block};
pub use remove_unused_control::cli::{RemoveUnusedTagArgs, remove_unused_tag};

pub use error::{
    AnalysisWorkerError, BindingListError, RemoveControlError, RemoveRequestError,
    RemoveSelectionError, RemoveUnusedError, RemoveUnusedResult,
};
