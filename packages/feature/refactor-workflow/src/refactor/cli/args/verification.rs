use std::path::PathBuf;

use clap::{Args, ValueEnum};

use crate::application::refactor::plan::VerificationPhase as ApplicationVerificationPhase;
use crate::domain::sexpr::SymbolName;

use super::super::super::{DialectArg, OutputFormat};
use super::plan::RefactorOperation;

#[derive(Debug, Args)]
#[command(
    after_help = "Examples:\n  paredit refactor verify --symbol old-name src/foo.lisp src/bar.lisp\n  paredit refactor verify --symbol old-name --new-symbol new-name --phase post src/foo.lisp src/bar.lisp"
)]
pub(in crate::presentation::cli) struct VerifyRefactorArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub(in crate::presentation::cli) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(in crate::presentation::cli) dialect: Option<DialectArg>,
    /// Original symbol that the refactor targets.
    #[arg(long)]
    pub(in crate::presentation::cli) symbol: SymbolName,
    /// Expected replacement symbol for post-rename verification.
    #[arg(long)]
    pub(in crate::presentation::cli) new_symbol: Option<SymbolName>,
    /// Refactoring intent used to choose verification gates.
    #[arg(long, value_enum, default_value_t = RefactorOperation::Rename)]
    pub(in crate::presentation::cli) operation: RefactorOperation,
    /// Whether to verify before or after the edit.
    #[arg(long, value_enum, default_value_t = VerificationPhase::Pre)]
    pub(in crate::presentation::cli) phase: VerificationPhase,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli) output: OutputFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(in crate::presentation::cli) enum VerificationPhase {
    Pre,
    Post,
}

impl From<VerificationPhase> for ApplicationVerificationPhase {
    fn from(phase: VerificationPhase) -> Self {
        match phase {
            VerificationPhase::Pre => Self::Pre,
            VerificationPhase::Post => Self::Post,
        }
    }
}
