use std::path::PathBuf;

use clap::{Args, ValueEnum};

use paredit_core_cli::args::{DialectArg, OutputFormat};

/// The version bump a release intends to make.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum IntendedBump {
    Major,
    Minor,
    Patch,
}

impl IntendedBump {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Major => "major",
            Self::Minor => "minor",
            Self::Patch => "patch",
        }
    }
}

#[derive(Debug, Args)]
pub struct ApiDiffReportArgs {
    /// Files or directories holding the current API.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// A previous `inspect api-surface --output json` document to compare
    /// against. A published snapshot rather than a git ref, so the comparison
    /// is reproducible and this command never shells out.
    #[arg(long, required = true, value_name = "JSON")]
    pub baseline: PathBuf,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Exit with failure when the diff requires a larger bump than this.
    #[arg(long, value_enum)]
    pub intended_bump: Option<IntendedBump>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
