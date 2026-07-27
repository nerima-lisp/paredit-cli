use std::path::PathBuf;

use clap::Args;

use crate::domain::sexpr::SymbolName;

use super::super::super::OutputFormat;
use super::plan::RefactorOperation;
use super::preview::RefactorPreviewMode;

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct WorkspaceRefactorExecuteArgs {
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
    /// Exact symbol to replace.
    #[arg(long)]
    pub(in crate::presentation::cli) from: SymbolName,
    /// Exact replacement symbol.
    #[arg(long)]
    pub(in crate::presentation::cli) to: SymbolName,
    /// Refactoring intent used to choose execute verification invariants.
    #[arg(long, value_enum, default_value_t = RefactorOperation::Rename)]
    pub(in crate::presentation::cli) operation: RefactorOperation,
    /// Rewrite strategy.
    #[arg(long, value_enum, default_value_t = RefactorPreviewMode::Symbol)]
    pub(in crate::presentation::cli) mode: RefactorPreviewMode,
    /// Maximum rewritten text bytes included per file in JSON/text output.
    #[arg(long, default_value_t = 160)]
    pub(in crate::presentation::cli) max_preview_bytes: usize,
    /// Rewrite changed files after policy gates pass and all rewritten outputs parse.
    #[arg(long)]
    pub(in crate::presentation::cli) write: bool,
    /// Fail when the preview would not change any file.
    #[arg(long)]
    pub(in crate::presentation::cli) fail_on_no_change: bool,
    /// Fail when any rewritten output does not parse.
    #[arg(long)]
    pub(in crate::presentation::cli) fail_on_parse_error: bool,
    /// Fail when the replacement symbol already exists in the preview scope.
    #[arg(long)]
    pub(in crate::presentation::cli) fail_on_target_conflict: bool,
    /// Require at least this many changed files.
    #[arg(long)]
    pub(in crate::presentation::cli) require_changed_files: Option<usize>,
    /// Require exactly this many callable definitions in function mode.
    #[arg(long)]
    pub(in crate::presentation::cli) require_definitions: Option<usize>,
    /// Require at least this many total edits.
    #[arg(long)]
    pub(in crate::presentation::cli) require_edits: Option<usize>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli) output: OutputFormat,
}
