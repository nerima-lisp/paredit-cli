//! `migrate list`, `migrate explain`, and `migrate run`.
//!
//! The three exist as a set because a codemod is not a thing anybody should
//! run before reading. `list` says what is available and where each recipe
//! came from; `explain` prints the steps and the dialect scope; `run` prints
//! the plan and writes only under `--write`. A recipe that a caller could
//! only discover by running it would be a worse tool than no recipe at all.

use std::path::{Path, PathBuf};

use anyhow::Result;

use paredit_core_cli::shared::{emit_document, read_input_dialect_and_tree};

use crate::catalog::{self, DEFAULT_RECIPE_DIRECTORY};
use crate::run::cli::args::{
    MigrateCommand, MigrateExplainArgs, MigrateListArgs, MigrateRunArgs, RecipeSourceArgs,
};
use crate::run::cli::render::{print_explain, print_list, print_run_report};
use crate::run::usecase::{FileOutcome, MigrationTotals, run_migration};
use crate::scan::selected_files;

/// Dispatches the namespace's three leaves.
pub fn migrate(command: MigrateCommand) -> Result<()> {
    match command {
        MigrateCommand::List(args) => list(args),
        MigrateCommand::Explain(args) => explain(args),
        MigrateCommand::Run(args) => run(*args),
    }
}

fn recipe_directory(source: &RecipeSourceArgs) -> PathBuf {
    source
        .recipes
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_RECIPE_DIRECTORY))
}

fn list(args: MigrateListArgs) -> Result<()> {
    let entries = catalog::resolve(&recipe_directory(&args.source))?;
    print_list(&entries, args.output)
}

fn explain(args: MigrateExplainArgs) -> Result<()> {
    let entries = catalog::resolve(&recipe_directory(&args.source))?;
    let entry = catalog::find(&entries, &args.recipe)?;
    print_explain(&entry, args.output)
}

fn run(args: MigrateRunArgs) -> Result<()> {
    let entries = catalog::resolve(&recipe_directory(&args.source))?;
    let entry = catalog::find(&entries, &args.recipe)?;
    let files = selected_files(&args.input, &args.roots)?;

    let mut outcomes: Vec<(FileOutcome, _)> = Vec::with_capacity(files.len());
    for file in &files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let outcome = run_migration(
            Path::new(file),
            dialect,
            tree.source(),
            &entry.migration,
            args.allow_comment_loss,
        )?;
        outcomes.push((outcome, (input, dialect)));
    }

    let plans: Vec<FileOutcome> = outcomes.iter().map(|(plan, _)| plan.clone()).collect();
    let totals = MigrationTotals::of(&plans);

    if args.write || args.diff {
        for (plan, (input, dialect)) in &outcomes {
            let Some(rewritten) = plan.rewritten.clone() else {
                continue;
            };
            emit_document(input, *dialect, args.write, args.diff, rewritten)?;
        }
        if !args.diff {
            print_run_report(&entry, &plans, &totals, args.write, args.output)?;
        }
    } else {
        print_run_report(&entry, &plans, &totals, args.write, args.output)?;
    }

    if args.check && totals.replacements > 0 {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "migrate run policy failed: --check, {} replacement(s) pending in {} file(s)",
            totals.replacements, totals.files_touched
        )));
    }
    Ok(())
}
