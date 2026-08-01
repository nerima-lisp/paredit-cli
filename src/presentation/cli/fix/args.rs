use clap::{Args, Subcommand};

use crate::presentation::cli::lint_report::args::FixSelectionArgs;

/// The `fix` namespace.
#[derive(Debug, Subcommand)]
pub(in crate::presentation::cli) enum FixCommand {
    /// Apply every available auto-fix in place, and report what changed.
    #[command(after_help = "Examples:\n  \
      paredit fix apply src/\n  \
      paredit fix apply --diff src/\n  \
      paredit fix apply --compact src/\n  \
      paredit fix apply --group-by-impact-area src/\n  \
      paredit fix apply --rule redundant-progn --no-destructive-fixes src/")]
    Apply(FixApplyArgs),
    /// Write nothing and exit 3 if any auto-fix is still pending (a CI gate).
    Check(FixSelectionArgs),
    /// Emit the machine-readable fix plan — every fixable finding's exact
    /// byte-region replacements — without writing.
    Plan(FixSelectionArgs),
    /// List the rules that carry an auto-fix.
    List(FixSelectionArgs),
}

/// `fix apply`'s own arguments: the selection every leaf shares, plus the two
/// flags that only make sense once something has actually been written.
///
/// Not folded into [`FixSelectionArgs`]: `check`, `plan`, and `list` never
/// write, so a `--compact` headline or a grouped, partially-resilient write
/// would either be dead weight on their command line or (worse) a flag that
/// silently does nothing depending on which leaf reads it. Mirrors
/// `refactor apply`'s own `--compact`/`--group-by-impact-area`, which
/// likewise live only on the leaf that writes.
#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct FixApplyArgs {
    #[command(flatten)]
    pub(in crate::presentation::cli) selection: FixSelectionArgs,
    /// In text mode, print only the one-line change headline instead of the
    /// full field-by-field report.
    #[arg(long, conflicts_with = "diff")]
    pub(in crate::presentation::cli) compact: bool,
    /// Write changed files one impact-area (declared package) group at a
    /// time instead of all at once, continuing to the next group when one
    /// group's write fails.
    #[arg(long, conflicts_with = "diff")]
    pub(in crate::presentation::cli) group_by_impact_area: bool,
}
