use std::path::PathBuf;

use clap::Args;

use crate::presentation::cli::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct SemanticCoverageReportArgs {
    /// Files or directories to measure.
    #[arg(required = true)]
    pub(in crate::presentation::cli::semantic_coverage_report) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(in crate::presentation::cli::semantic_coverage_report) dialect: Option<DialectArg>,
    /// Exit with failure when the corpus-wide variable-binding resolution rate falls below this percentage (0-100).
    #[arg(long, value_name = "PERCENT")]
    pub(in crate::presentation::cli::semantic_coverage_report) fail_under: Option<f64>,
    /// How many ranked opacity causes and uninitialized binders to list as suggested next operators to register.
    #[arg(long, default_value_t = 10)]
    pub(in crate::presentation::cli::semantic_coverage_report) top: usize,
    /// Output format.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli::semantic_coverage_report) output: OutputFormat,
}
