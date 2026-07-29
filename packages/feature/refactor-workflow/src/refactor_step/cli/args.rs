use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::OutputFormat;

#[derive(Debug, Args)]
pub struct RefactorStepArgs {
    /// The preview manifest to step through, as written by
    /// `refactor preview --manifest-out`.
    #[arg(long, value_name = "FILE")]
    pub manifest: PathBuf,
    /// Refuse to run unless the manifest still hashes to this value.
    ///
    /// `refactor preview` prints the hash, and every manifest names this flag
    /// in its own `next_actions`. Without it, an edited or regenerated manifest
    /// would be stepped as if it were the one that was reviewed.
    #[arg(long, value_name = "HASH")]
    pub expect_manifest_hash: Option<String>,
    /// Take these steps: `all`, `none`, `3`, `1,4`, `2-5`, or a mix.
    #[arg(long, value_name = "STEPS")]
    pub accept: Option<String>,
    /// Leave these steps out. Applied after `--accept`, which defaults to
    /// `all` when only `--skip` is given.
    #[arg(long, value_name = "STEPS")]
    pub skip: Option<String>,
    /// Decide each step from standard input: `y` takes it, `n` leaves it,
    /// `a` takes it and everything after, `q` stops and takes nothing more.
    #[arg(long, conflicts_with_all = ["accept", "skip"])]
    pub interactive: bool,
    /// Print a unified diff of the selected steps instead of the step list.
    #[arg(long)]
    pub diff: bool,
    /// Apply the selected steps. Without it nothing is written and the step
    /// list is the output.
    #[arg(long)]
    pub write: bool,
    /// Exit with failure when the selection leaves any step out.
    ///
    /// A manifest's edits are usually one change — a rename applied at the
    /// definition and skipped at a call site does not compile. A person may
    /// mean to split one; a script should not be able to by accident.
    #[arg(long)]
    pub fail_on_partial: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
