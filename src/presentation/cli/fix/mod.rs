//! The `fix` namespace: the auto-fixer, out from under `inspect`.
//!
//! Every capability here already existed as a flag on `inspect lint`, and
//! every one of them was hard to find there. Three separate things were wrong
//! with the old address:
//!
//! - **`inspect` means read-only.** `inspect lint --fix` writes source files.
//!   A namespace whose contract is "this does not touch your code" with a flag
//!   that touches your code is a contract that has to be read twice.
//! - **The flag was one of forty.** `--fix`, `--fix-plan`, `--check`,
//!   `--no-destructive-fixes` and `--diff` interact with each other and with
//!   almost none of the rest, which is the shape of a separate command.
//! - **`--list-rules` answers a different question than "what can be fixed".**
//!   It lists all 170-odd rules with fixability as a column. `fix list`
//!   answers the question a caller about to run a fixer actually has.
//!
//! Nothing is reimplemented. Each leaf builds the `LintReportArgs` its old
//! spelling would have produced and calls the same workflow, so a fix applied
//! through `fix apply` and one applied through `inspect lint --fix` are the
//! same bytes by construction rather than by testing. The older spellings
//! stay: scripts and the shipped GitHub Action pass them.

pub(super) mod args;

use anyhow::Result;

use crate::presentation::cli::lint_report::args::{FixMode, LintReportArgs};
use crate::presentation::cli::lint_report::workflow::lint_report;

pub(super) use args::FixCommand;

/// Dispatches the namespace's four leaves onto the one lint workflow.
pub(super) fn fix(command: FixCommand) -> Result<()> {
    let (selection, mode) = match command {
        FixCommand::Apply(selection) => (selection, FixMode::Apply),
        FixCommand::Check(selection) => (selection, FixMode::Check),
        FixCommand::Plan(selection) => (selection, FixMode::Plan),
        FixCommand::List(selection) => (selection, FixMode::List),
    };
    lint_report(LintReportArgs::for_fix(selection, mode))
}
