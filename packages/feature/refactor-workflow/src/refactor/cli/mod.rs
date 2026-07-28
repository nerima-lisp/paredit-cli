pub mod args;
pub mod manifest;
pub mod render;
pub mod types;
pub mod workflow;

// Hoisted for the composition root (section 4.2): the argument type and run
// function of each subcommand, which live in different modules here.
pub use args::execute::WorkspaceRefactorExecuteArgs;
pub use args::manifest::RefactorApplyArgs;
pub use args::manifest::RefactorCheckArgs;
pub use args::manifest::RefactorDiffArgs;
pub use args::manifest::RefactorStatusArgs;
pub use args::plan::RefactorPlanArgs;
pub use args::plan::WorkspaceRefactorPlanArgs;
pub use args::preview::RefactorPreviewArgs;
pub use args::preview::WorkspaceRefactorPreviewArgs;
pub use args::verification::VerifyRefactorArgs;
pub use workflow::execute::workspace_refactor_execute;
pub use workflow::manifest::apply::refactor_apply;
pub use workflow::manifest::check::refactor_check;
pub use workflow::manifest::diff::refactor_diff;
pub use workflow::manifest::status::refactor_status;
pub use workflow::plan::refactor_plan;
pub use workflow::plan::workspace_refactor_plan;
pub use workflow::preview::refactor_preview;
pub use workflow::preview::workspace_refactor_preview;
pub use workflow::verification::verify_refactor;
