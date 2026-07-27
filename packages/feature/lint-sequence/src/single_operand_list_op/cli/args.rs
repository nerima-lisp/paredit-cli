use std::path::PathBuf;

use clap::Args;

use crate::presentation::cli::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct SingleOperandListOpReportArgs {
    /// Files or directories to scan.
    #[arg(required = true)]
    pub(in crate::presentation::cli::single_operand_list_op_report) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(in crate::presentation::cli::single_operand_list_op_report) dialect: Option<DialectArg>,
    /// Exit with failure when any single-argument append/nconc/list* is found.
    #[arg(long)]
    pub(in crate::presentation::cli::single_operand_list_op_report) fail_on_violation: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli::single_operand_list_op_report) output: OutputFormat,
}
