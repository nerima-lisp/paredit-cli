//! `cargo xtask` — repository dev-experience tooling, in Rust rather than
//! the Python scripts under `scripts/` (see `docs/src/development.md`).
//!
//! Two generators live here today: `new-lint-rule` and `new-command`. Both
//! scaffold the mechanical, boilerplate-shaped files a new rule or command
//! needs, then print — rather than blindly edit — the handful of
//! pinned-count and dispatch-table edits that stay a deliberate, reviewed
//! change on purpose (see `checklist.rs`).
//!
//! The existing `scripts/*-package.py` scripts are a separate, larger
//! surface (moving code between whole packages, rewriting `crate::` paths
//! across a package boundary) and are intentionally out of scope here: that
//! is materially higher-risk to port than to leave alone, since it is
//! exercised only a few times a year and already works.

mod case;
mod checklist;
mod error;
mod fs_util;
mod new_command;
mod new_lint_rule;
mod repo;

use crate::error::Result;
use clap::{Parser, Subcommand};

use case::Name;
use checklist::Namespace;
use repo::Repo;

#[derive(Debug, Parser)]
#[command(
    name = "xtask",
    about = "paredit-cli repository dev-experience tooling"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Scaffold a new lint rule inside an existing `lint-<theme>` package.
    NewLintRule {
        /// The theme package's suffix, e.g. `string-char` for `lint-string-char`.
        theme: String,
        /// The new rule's name in kebab-case, e.g. `char-case-fold`.
        rule_name: String,
        #[arg(long)]
        description: String,
    },
    /// Scaffold a new command inside an existing feature package.
    NewCommand {
        /// Which top-level namespace the command belongs to.
        #[arg(value_enum)]
        namespace: NamespaceArg,
        /// The feature package's name, e.g. `project-inventory`.
        package: String,
        /// The new command's name in kebab-case, e.g. `blame-report`.
        command_name: String,
        #[arg(long)]
        description: String,
    },
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum NamespaceArg {
    Inspect,
    Edit,
    Refactor,
}

impl From<NamespaceArg> for Namespace {
    fn from(value: NamespaceArg) -> Self {
        match value {
            NamespaceArg::Inspect => Self::Inspect,
            NamespaceArg::Edit => Self::Edit,
            NamespaceArg::Refactor => Self::Refactor,
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let repo = Repo::discover()?;

    match cli.command {
        Command::NewLintRule {
            theme,
            rule_name,
            description,
        } => new_lint_rule::run(
            &repo,
            &new_lint_rule::NewLintRuleOptions {
                theme,
                name: Name::parse(&rule_name)?,
                description,
            },
        ),
        Command::NewCommand {
            namespace,
            package,
            command_name,
            description,
        } => new_command::run(
            &repo,
            &new_command::NewCommandOptions {
                namespace: namespace.into(),
                package,
                name: Name::parse(&command_name)?,
                description,
            },
        ),
    }
}
