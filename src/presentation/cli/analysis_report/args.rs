use std::path::PathBuf;

use clap::{Args, ValueEnum};

use paredit_core_cli::args::AnalyzeArgs;
use paredit_core_cli::runtime::Verbosity;

/// A `clap` mirror of [`Verbosity`], so the core package keeps no dependency
/// on the argument parser. The `From` below is the only place the two
/// spellings meet, and it is exhaustive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum VerbosityArg {
    /// Metrics and the dialect only. Enough to decide whether to look closer.
    Quiet,
    /// Metrics, the outline, and every atom occurrence. The default.
    Normal,
    /// Adds per-definition structure and the document digest.
    Detailed,
}

impl From<VerbosityArg> for Verbosity {
    fn from(value: VerbosityArg) -> Self {
        match value {
            VerbosityArg::Quiet => Self::Quiet,
            VerbosityArg::Normal => Self::Normal,
            VerbosityArg::Detailed => Self::Detailed,
        }
    }
}

#[derive(Debug, Args)]
pub struct CheckArgs {
    #[command(flatten)]
    pub analyze: AnalyzeArgs,
    /// Also validate `.paredit/rules/*.lisp` as a `defrule` migration aid.
    ///
    /// Those files are Lisp source that ordinary workspace discovery never
    /// sees (a `.paredit` directory is hidden, so it is skipped the same way
    /// any dot-directory is), so a syntax error there can otherwise sit
    /// unnoticed until the next `inspect lint` run loads it. This also flags
    /// a `:pattern`/`:fix` clause still written in `defrule`'s
    /// pre-unification style — a non-trailing `...` — as a nudge toward the
    /// clearer `?name...` spelling; it still matches exactly as it always
    /// has, so this is advice, not an error, and never fails a run that has
    /// nothing else wrong with it.
    #[arg(long)]
    pub paredit_config: bool,
}

#[derive(Debug, Args)]
pub struct AgentReportArgs {
    #[command(flatten)]
    pub analyze: AnalyzeArgs,
    /// How much detail to include. Defaults to output.verbosity from the
    /// configuration, and to `normal` without one.
    #[arg(long, value_enum, value_name = "LEVEL")]
    pub verbosity: Option<VerbosityArg>,
    /// Approximate token budget. Lists are trimmed to fit and the report says
    /// exactly what it dropped. Zero, the default, means no budget.
    #[arg(long, value_name = "TOKENS")]
    pub max_tokens: Option<usize>,
    /// Compare against a previous `--output json` report from this command and
    /// add what changed since. Ignored with --output text.
    #[arg(long, value_name = "FILE")]
    pub since: Option<PathBuf>,
}
