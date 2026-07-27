use crate::impact_report::usecase::ImpactRiskLevel as ApplicationImpactRiskLevel;
use clap::Args;
use clap::ValueEnum;
use paredit_core_cli::args::DialectArg;
use paredit_core_cli::args::OutputFormat;
use paredit_core_edit::refactor_plan::RefactorRiskLevel;
use paredit_core_syntax::sexpr::SymbolName;
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct ImpactReportArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Exact symbol to evaluate before rename, move, remove, or signature refactors.
    #[arg(long)]
    pub symbol: SymbolName,
    /// Exit with failure when the report risk reaches this level or higher.
    #[arg(long, value_enum)]
    pub fail_on_risk_level: Option<ImpactRiskLevel>,
    /// Require at least this many matching definitions.
    #[arg(long)]
    pub require_definitions: Option<usize>,
    /// Require at least this many matching references.
    #[arg(long)]
    pub require_references: Option<usize>,
    /// Require at least this many matching call sites.
    #[arg(long)]
    pub require_calls: Option<usize>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ImpactRiskLevel {
    Info,
    Warning,
    Error,
}

impl From<ImpactRiskLevel> for ApplicationImpactRiskLevel {
    fn from(level: ImpactRiskLevel) -> Self {
        match level {
            ImpactRiskLevel::Info => Self::Info,
            ImpactRiskLevel::Warning => Self::Warning,
            ImpactRiskLevel::Error => Self::Error,
        }
    }
}

impl From<ImpactRiskLevel> for RefactorRiskLevel {
    fn from(level: ImpactRiskLevel) -> Self {
        match level {
            ImpactRiskLevel::Info => Self::Info,
            ImpactRiskLevel::Warning => Self::Warning,
            ImpactRiskLevel::Error => Self::Error,
        }
    }
}
