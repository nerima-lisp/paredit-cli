use clap::Subcommand;

use crate::presentation::cli::lint_report::args::FixSelectionArgs;

/// The `fix` namespace.
#[derive(Debug, Subcommand)]
pub(in crate::presentation::cli) enum FixCommand {
    /// Apply every available auto-fix in place, and report what changed.
    #[command(after_help = "Examples:\n  \
      paredit fix apply src/\n  \
      paredit fix apply --diff src/\n  \
      paredit fix apply --rule redundant-progn --no-destructive-fixes src/")]
    Apply(FixSelectionArgs),
    /// Write nothing and exit 3 if any auto-fix is still pending (a CI gate).
    Check(FixSelectionArgs),
    /// Emit the machine-readable fix plan — every fixable finding's exact
    /// byte-region replacements — without writing.
    Plan(FixSelectionArgs),
    /// List the rules that carry an auto-fix.
    List(FixSelectionArgs),
}
