use std::path::PathBuf;

use clap::{Args, ValueEnum};

use paredit_core_cli::args::{DialectArg, OutputFormat};

use crate::external_diagnostics_report::domain::Implementation;

/// Which implementation to invoke.
///
/// No default. Compiling a file runs its macros, its `eval-when
/// (:compile-toplevel)` forms and its `#.` read-time evaluation, so pointing
/// this command at code is the same act as running it. A caller should have to
/// say so, not fall into it because a flag defaulted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ImplementationArg {
    Sbcl,
}

impl From<ImplementationArg> for Implementation {
    fn from(value: ImplementationArg) -> Self {
        match value {
            ImplementationArg::Sbcl => Self::Sbcl,
        }
    }
}

#[derive(Debug, Args)]
#[command(
    after_help = "Compiling is executing: compile-file runs the file's macros, its eval-when (:compile-toplevel) forms, and its #. read-time evaluation.\nDo not point this at code you would not run. The fasl goes to a temporary directory, so the source tree is untouched.\n\nCross-checking a refactor:\n  paredit inspect external-diagnostics --implementation sbcl --save-baseline before.json src/*.lisp\n  # ... apply the refactor ...\n  paredit inspect external-diagnostics --implementation sbcl --baseline before.json --fail-on-introduced src/*.lisp"
)]
pub struct ExternalDiagnosticsReportArgs {
    /// Files or directories to compile.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Lisp implementation to invoke. Required: compiling a file runs it.
    #[arg(long, value_enum)]
    pub implementation: ImplementationArg,
    /// Path to the implementation binary, when it is not on PATH under its own name.
    #[arg(long, value_name = "PATH")]
    pub implementation_path: Option<String>,
    /// Wall-clock budget per file, in milliseconds.
    #[arg(long, value_name = "MILLIS", default_value_t = 60_000)]
    pub compile_timeout_ms: u64,
    /// Compare against a previously saved run and mark diagnostics absent from it.
    #[arg(long, value_name = "PATH")]
    pub baseline: Option<PathBuf>,
    /// Write this run's diagnostics as a baseline for a later comparison.
    #[arg(long, value_name = "PATH")]
    pub save_baseline: Option<PathBuf>,
    /// Exit with failure when a diagnostic is absent from --baseline.
    #[arg(long, requires = "baseline")]
    pub fail_on_introduced: bool,
    /// Exit with failure when the implementation reports anything at all.
    #[arg(long, conflicts_with = "fail_on_introduced")]
    pub fail_on_diagnostics: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
