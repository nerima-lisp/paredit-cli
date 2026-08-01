use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, ReportFormat};
use paredit_core_cli::runtime::Verbosity;

use crate::line_metrics_report::usecase::LineThresholds;

#[derive(Debug, Args)]
pub struct LineMetricsReportArgs {
    /// Files or directories to scan.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Report lines wider than this many characters.
    #[arg(long, default_value_t = 100)]
    pub max_line_length: usize,
    /// Report files longer than this many lines.
    #[arg(long, default_value_t = 1000)]
    pub max_file_lines: usize,
    /// Report definitions taller than this many lines.
    #[arg(long, default_value_t = 50)]
    pub max_definition_lines: usize,
    /// Exit with failure when any threshold is exceeded.
    #[arg(long)]
    pub fail_on_overflow: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = ReportFormat::Json)]
    pub output: ReportFormat,
    /// How much detail the report includes.
    #[arg(long, value_enum, default_value_t = Verbosity::Normal)]
    pub verbosity: Verbosity,
}

impl LineMetricsReportArgs {
    #[must_use]
    pub const fn thresholds(&self) -> LineThresholds {
        LineThresholds {
            max_line_length: self.max_line_length,
            max_file_lines: self.max_file_lines,
            max_definition_lines: self.max_definition_lines,
        }
    }
}
