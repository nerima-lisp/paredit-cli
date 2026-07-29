use std::path::PathBuf;

use clap::{Args, Subcommand};

use paredit_core_cli::args::{DialectArg, OutputFormat};
use paredit_core_cli::workspace_args::WorkspaceInputArgs;

/// The `migrate` namespace.
#[derive(Debug, Subcommand)]
pub enum MigrateCommand {
    /// List the migration recipes this run can reach, built-in and project.
    List(MigrateListArgs),
    /// Print one recipe's steps, its dialect scope, and what it leaves alone.
    Explain(MigrateExplainArgs),
    /// Apply a recipe's steps, in order, to a file set.
    ///
    /// Boxed because it is the only variant carrying the whole workspace
    /// input surface, and an un-boxed enum would size every `migrate list`
    /// invocation after it.
    Run(Box<MigrateRunArgs>),
}

/// The recipe directory flag, shared by all three subcommands.
///
/// Flattened rather than repeated: `migrate explain` has to resolve the same
/// catalogue `migrate run` will, or it explains a recipe other than the one
/// that would run.
#[derive(Debug, Args)]
pub struct RecipeSourceArgs {
    /// Load the project's own recipes from this directory instead of the
    /// default `.paredit/migrations`.
    #[arg(long, value_name = "DIR")]
    pub recipes: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct MigrateListArgs {
    #[command(flatten)]
    pub source: RecipeSourceArgs,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct MigrateExplainArgs {
    /// The recipe name, as `migrate list` reports it.
    pub recipe: String,
    #[command(flatten)]
    pub source: RecipeSourceArgs,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
#[command(after_help = "Examples:\n  \
      paredit migrate run nil-conditionals src/\n  \
      paredit migrate run elisp-cl-lib --diff lisp/\n  \
      paredit migrate run elisp-cl-lib --write --since origin/main .\n  \
      paredit migrate run nil-conditionals --check .\n\n\
      Nothing is written without --write. A recipe skips every file outside its\n\
      :dialects and says how many, so a run that changed nothing says why.")]
pub struct MigrateRunArgs {
    /// The recipe name, as `migrate list` reports it.
    pub recipe: String,
    /// Files or directories to migrate recursively.
    #[arg(required = true)]
    pub roots: Vec<PathBuf>,
    #[command(flatten)]
    pub source: RecipeSourceArgs,
    /// Every input selector and filter this tool understands.
    #[command(flatten)]
    pub input: WorkspaceInputArgs,
    /// Override extension-based dialect detection for every file.
    ///
    /// This overrides what the recipe's `:dialects` is checked against, so it
    /// is also the way to run an Emacs Lisp recipe over a file whose extension
    /// does not say `.el`. It is not a way to bypass the scope check.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Apply the migration in place.
    #[arg(long)]
    pub write: bool,
    /// Print a unified diff of what would change.
    #[arg(long)]
    pub diff: bool,
    /// Write nothing and exit 3 if the migration is not already applied.
    #[arg(long, conflicts_with = "write")]
    pub check: bool,
    /// Rewrite matches even when doing so deletes a comment that no capture
    /// carries over.
    #[arg(long)]
    pub allow_comment_loss: bool,
    /// Rewrite matches that sit inside quoted data, changing a literal rather
    /// than code.
    #[arg(long)]
    pub include_quoted: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
