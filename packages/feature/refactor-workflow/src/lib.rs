#![doc = include_str!("../README.md")]

pub mod error;
pub mod refactor;
pub mod refactor_step;

// The contract with the composition root (section 4.2): each slice that
// owns a subcommand publishes its `clap` argument type and the function
// that runs it. command.rs and dispatch.rs need these two names and no more.
pub use refactor::cli::{RefactorApplyArgs, refactor_apply};
pub use refactor::cli::{RefactorCheckArgs, refactor_check};
pub use refactor::cli::{RefactorDiffArgs, refactor_diff};
pub use refactor::cli::{RefactorPlanArgs, refactor_plan};
pub use refactor::cli::{RefactorPreviewArgs, refactor_preview};
pub use refactor::cli::{RefactorStatusArgs, refactor_status};
pub use refactor::cli::{VerifyRefactorArgs, verify_refactor};
pub use refactor::cli::{WorkspaceRefactorExecuteArgs, workspace_refactor_execute};
pub use refactor::cli::{WorkspaceRefactorPlanArgs, workspace_refactor_plan};
pub use refactor::cli::{WorkspaceRefactorPreviewArgs, workspace_refactor_preview};
pub use refactor_step::cli::{RefactorStepArgs, refactor_step};
