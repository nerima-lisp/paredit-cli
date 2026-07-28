use std::path::PathBuf;

use clap::{Args, ValueEnum};

use crate::refactor::usecase::plan::VerificationPhase as ApplicationVerificationPhase;
use paredit_core_syntax::sexpr::SymbolName;

use super::plan::RefactorOperation;
use paredit_core_cli::args::{DialectArg, OutputFormat};

#[derive(Debug, Args)]
#[command(
    after_help = "Examples:\n  paredit refactor verify --symbol old-name src/foo.lisp src/bar.lisp\n  paredit refactor verify --symbol old-name --new-symbol new-name --phase post src/foo.lisp src/bar.lisp"
)]
pub struct VerifyRefactorArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Original symbol that the refactor targets.
    #[arg(long)]
    pub symbol: SymbolName,
    /// Expected replacement symbol for post-rename verification.
    #[arg(long)]
    pub new_symbol: Option<SymbolName>,
    /// Refactoring intent used to choose verification gates.
    #[arg(long, value_enum, default_value_t = RefactorOperation::Rename)]
    pub operation: RefactorOperation,
    /// Whether to verify before or after the edit.
    #[arg(long, value_enum, default_value_t = VerificationPhase::Pre)]
    pub phase: VerificationPhase,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum VerificationPhase {
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
