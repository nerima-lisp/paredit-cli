use std::path::PathBuf;

use clap::{Args, ValueEnum};

use crate::application::refactor::plan::RefactorOperation as ApplicationRefactorOperation;
use crate::domain::sexpr::SymbolName;

use super::super::super::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
#[command(
    after_help = "Examples:\n  paredit refactor plan --symbol old-name src/foo.lisp src/bar.lisp\n  paredit refactor workspace-plan --symbol old-name ."
)]
pub(in crate::presentation::cli) struct WorkspaceRefactorPlanArgs {
    /// Files or directories to scan recursively.
    #[arg(required = true)]
    pub(in crate::presentation::cli) roots: Vec<PathBuf>,
    /// Include files whose extension does not identify a known Lisp dialect.
    #[arg(long)]
    pub(in crate::presentation::cli) include_unknown: bool,
    /// Include hidden directories and files.
    #[arg(long)]
    pub(in crate::presentation::cli) include_hidden: bool,
    /// Include generated or dependency directories such as target and node_modules.
    #[arg(long)]
    pub(in crate::presentation::cli) include_generated: bool,
    /// Maximum directory recursion depth from each root directory.
    #[arg(long)]
    pub(in crate::presentation::cli) max_depth: Option<usize>,
    /// Exact symbol to evaluate before generating a refactoring plan.
    #[arg(long)]
    pub(in crate::presentation::cli) symbol: SymbolName,
    /// Refactoring intent used to choose gates and recommended commands.
    #[arg(long, value_enum, default_value_t = RefactorOperation::Rename)]
    pub(in crate::presentation::cli) operation: RefactorOperation,
    /// Fail after printing the plan when any gate blocks automated editing.
    #[arg(long)]
    pub(in crate::presentation::cli) fail_on_blocking_gate: bool,
    /// Minimum number of discovered definitions required for the plan to pass.
    #[arg(long)]
    pub(in crate::presentation::cli) require_definitions: Option<usize>,
    /// Minimum number of discovered references required for the plan to pass.
    #[arg(long)]
    pub(in crate::presentation::cli) require_references: Option<usize>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli) output: OutputFormat,
}

#[derive(Debug, Args)]
#[command(
    after_help = "Examples:\n  paredit refactor plan --symbol old-name src/foo.lisp src/bar.lisp"
)]
pub(in crate::presentation::cli) struct RefactorPlanArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub(in crate::presentation::cli) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(in crate::presentation::cli) dialect: Option<DialectArg>,
    /// Exact symbol to evaluate before generating a refactoring plan.
    #[arg(long)]
    pub(in crate::presentation::cli) symbol: SymbolName,
    /// Refactoring intent used to choose gates and recommended commands.
    #[arg(long, value_enum, default_value_t = RefactorOperation::Rename)]
    pub(in crate::presentation::cli) operation: RefactorOperation,
    /// Fail after printing the plan when any gate blocks automated editing.
    #[arg(long)]
    pub(in crate::presentation::cli) fail_on_blocking_gate: bool,
    /// Minimum number of discovered definitions required for the plan to pass.
    #[arg(long)]
    pub(in crate::presentation::cli) require_definitions: Option<usize>,
    /// Minimum number of discovered references required for the plan to pass.
    #[arg(long)]
    pub(in crate::presentation::cli) require_references: Option<usize>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli) output: OutputFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(in crate::presentation::cli) enum RefactorOperation {
    Rename,
    Remove,
    Move,
    Signature,
}

impl From<RefactorOperation> for ApplicationRefactorOperation {
    fn from(operation: RefactorOperation) -> Self {
        match operation {
            RefactorOperation::Rename => Self::Rename,
            RefactorOperation::Remove => Self::Remove,
            RefactorOperation::Move => Self::Move,
            RefactorOperation::Signature => Self::Signature,
        }
    }
}
