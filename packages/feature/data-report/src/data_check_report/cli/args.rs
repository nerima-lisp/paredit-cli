use std::path::PathBuf;

use clap::{Args, ValueEnum};

use paredit_core_cli::args::{DialectArg, ReportFormat};
use paredit_core_cli::runtime::Verbosity;

use crate::data_check_report::domain::DataFormat;

#[derive(Debug, Args)]
pub struct DataCheckReportArgs {
    /// Files or directories to scan.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Which format-specific checks to run, on top of the baseline ones
    /// every run performs. Auto-detected per file from its path and content
    /// when omitted.
    #[arg(long, value_enum)]
    pub format: Option<DataFormatArg>,
    /// Exit with failure when any file has a structural finding.
    #[arg(long)]
    pub fail_on_finding: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = ReportFormat::Json)]
    pub output: ReportFormat,
    /// How much detail the report includes.
    #[arg(long, value_enum, default_value_t = Verbosity::Normal)]
    pub verbosity: Verbosity,
}

/// A `clap` mirror of [`DataFormat`], keeping the argument parser out of the
/// domain module — the same split [`DialectArg`]/[`Dialect`](paredit_core_syntax::dialect::Dialect)
/// and [`ReaderPrefixArg`](paredit_core_cli::args::ReaderPrefixArg)/[`ReaderPrefix`](paredit_core_syntax::sexpr::ReaderPrefix)
/// already use in this codebase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum DataFormatArg {
    Baseline,
    EmacsCustomize,
    Edn,
    PareditConfig,
    DirLocals,
    RacketData,
}

impl From<DataFormatArg> for DataFormat {
    fn from(value: DataFormatArg) -> Self {
        match value {
            DataFormatArg::Baseline => Self::Baseline,
            DataFormatArg::EmacsCustomize => Self::EmacsCustomize,
            DataFormatArg::Edn => Self::Edn,
            DataFormatArg::PareditConfig => Self::PareditConfig,
            DataFormatArg::DirLocals => Self::DirLocals,
            DataFormatArg::RacketData => Self::RacketData,
        }
    }
}
