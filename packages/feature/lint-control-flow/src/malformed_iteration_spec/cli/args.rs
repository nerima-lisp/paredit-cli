use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, ReportFormat};

#[derive(Debug, Args)]
pub struct MalformedIterationSpecReportArgs {
    /// Files or directories to scan.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    // rustdoc reads `[result]` as an intra-doc link and warns that it does not
    // resolve. Escaping it would silence the warning and change this line,
    // which clap renders verbatim as `--fail-on-violation`'s help text - and
    // the CLI surface is pinned by the capabilities golden. The warning is the
    // cheaper of the two.
    /// Exit with failure when any dolist/dotimes spec is not (var form [result]).
    #[arg(long)]
    pub fail_on_violation: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = ReportFormat::Json)]
    pub output: ReportFormat,
}
