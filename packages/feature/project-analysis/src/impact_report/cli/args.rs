use super::super::*;
use crate::application::usecase::impact_report::ImpactRiskLevel as ApplicationImpactRiskLevel;
use crate::domain::refactor_plan::RefactorRiskLevel;

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct ImpactReportArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub(super) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(super) dialect: Option<DialectArg>,
    /// Exact symbol to evaluate before rename, move, remove, or signature refactors.
    #[arg(long)]
    pub(super) symbol: SymbolName,
    /// Exit with failure when the report risk reaches this level or higher.
    #[arg(long, value_enum)]
    pub(super) fail_on_risk_level: Option<ImpactRiskLevel>,
    /// Require at least this many matching definitions.
    #[arg(long)]
    pub(super) require_definitions: Option<usize>,
    /// Require at least this many matching references.
    #[arg(long)]
    pub(super) require_references: Option<usize>,
    /// Require at least this many matching call sites.
    #[arg(long)]
    pub(super) require_calls: Option<usize>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(super) output: OutputFormat,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(in crate::presentation::cli) enum ImpactRiskLevel {
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
