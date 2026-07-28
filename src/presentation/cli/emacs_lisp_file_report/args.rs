use std::path::PathBuf;

use clap::Args;

use crate::presentation::cli::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct EmacsLispFileReportArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub(in crate::presentation::cli::emacs_lisp_file_report) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(in crate::presentation::cli::emacs_lisp_file_report) dialect: Option<DialectArg>,
    /// Exit with failure when a file has no lexical-binding setting on its first line.
    #[arg(long)]
    pub(in crate::presentation::cli::emacs_lisp_file_report) fail_on_missing_lexical_binding: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli::emacs_lisp_file_report) output: OutputFormat,
}
