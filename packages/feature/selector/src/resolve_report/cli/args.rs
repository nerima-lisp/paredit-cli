use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, OutputFormat, SelectorArgs};

use crate::resolve_report::usecase::DEFAULT_PREVIEW_BYTES;

#[derive(Debug, Args)]
pub struct ResolveReportArgs {
    /// Input file. Reads stdin when omitted.
    #[arg(short, long)]
    pub file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    #[command(flatten)]
    pub selector: SelectorArgs,
    /// Bytes of source text to show per match.
    #[arg(long, value_name = "N", default_value_t = DEFAULT_PREVIEW_BYTES)]
    pub preview_bytes: usize,
    /// Exit non-zero when the selector names no form.
    ///
    /// Off by default: "nothing matched" is an answer here, and a script that
    /// wants it to be a failure says so.
    #[arg(long)]
    pub fail_on_empty: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
