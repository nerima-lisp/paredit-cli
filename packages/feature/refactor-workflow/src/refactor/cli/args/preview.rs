use std::path::PathBuf;

use clap::{Args, ValueEnum};

use crate::domain::sexpr::SymbolName;

use super::super::super::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
#[command(
    after_help = "Examples:\n  paredit refactor workspace-preview --from old-name --to new-name .\n  paredit refactor workspace-execute --from old-name --to new-name --write ."
)]
pub(in crate::presentation::cli) struct WorkspaceRefactorPreviewArgs {
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
    /// Exact symbol to replace in the preview.
    #[arg(long)]
    pub(in crate::presentation::cli) from: SymbolName,
    /// Exact replacement symbol to use in the preview.
    #[arg(long)]
    pub(in crate::presentation::cli) to: SymbolName,
    /// Rewrite strategy to preview.
    #[arg(long, value_enum, default_value_t = RefactorPreviewMode::Function)]
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
    /// Write the JSON preview manifest to this file and print a summary with its hash instead of the manifest itself.
    #[arg(long)]
    pub(in crate::presentation::cli) manifest_out: Option<PathBuf>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli) output: OutputFormat,
}

#[derive(Debug, Args)]
#[command(
    after_help = "Examples:\n  paredit refactor preview --from old-name --to new-name src/foo.lisp src/bar.lisp"
)]
pub(in crate::presentation::cli) struct RefactorPreviewArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub(in crate::presentation::cli) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(in crate::presentation::cli) dialect: Option<DialectArg>,
    /// Exact symbol to replace in the preview.
    #[arg(long)]
    pub(in crate::presentation::cli) from: SymbolName,
    /// Exact replacement symbol to use in the preview.
    #[arg(long)]
    pub(in crate::presentation::cli) to: SymbolName,
    /// Rewrite strategy to preview.
    #[arg(long, value_enum, default_value_t = RefactorPreviewMode::Function)]
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
    /// Write the JSON preview manifest to this file and print a summary with its hash instead of the manifest itself.
    #[arg(long)]
    pub(in crate::presentation::cli) manifest_out: Option<PathBuf>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli) output: OutputFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(in crate::presentation::cli) enum RefactorPreviewMode {
    Symbol,
    Function,
}

impl RefactorPreviewMode {
    pub(in crate::presentation::cli) const fn label(self) -> &'static str {
        match self {
            Self::Symbol => "symbol",
            Self::Function => "function",
        }
    }
}
