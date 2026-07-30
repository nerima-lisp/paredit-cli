//! `migrate list`, `migrate explain`, and `migrate run`.
//!
//! The three exist as a set because a codemod is not a thing anybody should
//! run before reading. `list` says what is available and where each recipe
//! came from; `explain` prints the steps and the dialect scope; `run` prints
//! the plan and writes only under `--write`. A recipe that a caller could
//! only discover by running it would be a worse tool than no recipe at all.

use std::path::PathBuf;

use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, emit_document, note_partial_file_failures, total_file_failure,
};
use paredit_core_syntax::selector::RewriteAllowances;

use crate::catalog::{self, DEFAULT_RECIPE_DIRECTORY};
use crate::run::cli::args::{
    MigrateCommand, MigrateExplainArgs, MigrateListArgs, MigrateRunArgs, RecipeSourceArgs,
};
use crate::run::cli::render::{print_explain, print_list, print_run_report};
use crate::run::usecase::{FileOutcome, MigrationTotals, run_migration};
use crate::scan::selected_files;

/// Dispatches the namespace's three leaves.
pub fn migrate(command: MigrateCommand) -> CommandResult {
    match command {
        MigrateCommand::List(args) => Ok(list(args)?),
        MigrateCommand::Explain(args) => Ok(explain(args)?),
        MigrateCommand::Run(args) => run(*args),
    }
}

fn recipe_directory(source: &RecipeSourceArgs) -> PathBuf {
    source
        .recipes
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_RECIPE_DIRECTORY))
}

fn list(args: MigrateListArgs) -> CliResult<()> {
    let entries = catalog::resolve(&recipe_directory(&args.source))?;
    print_list(&entries, args.output)
}

fn explain(args: MigrateExplainArgs) -> CliResult<()> {
    let entries = catalog::resolve(&recipe_directory(&args.source))?;
    let entry = catalog::find(&entries, &args.recipe)?;
    print_explain(&entry, args.output)
}

fn run(args: MigrateRunArgs) -> CommandResult {
    let entries = catalog::resolve(&recipe_directory(&args.source))?;
    let entry = catalog::find(&entries, &args.recipe)?;
    let allow = RewriteAllowances {
        comment_loss: args.allow_comment_loss,
        quoted: args.include_quoted,
    };
    let files = selected_files(&args.input, &args.roots)?;

    // A file that will not parse is reported, not fatal, and the source text
    // is retained only for a file that will actually be written. Both matter
    // at repository scale: 10% of Emacs's own `lisp/` tree uses a reader
    // dispatch this parser declines, and one such file aborted a run over the
    // other 1450.
    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, input| {
        let outcome = run_migration(file, dialect, tree.source(), &entry.migration, allow)?;
        let source = outcome.is_touched().then(|| (input.clone(), dialect));
        CliResult::Ok((outcome, source))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let outcomes = analysis.succeeded;

    // References, not clones.
    let plans: Vec<&FileOutcome> = outcomes.iter().map(|(plan, _)| plan).collect();
    let totals = MigrationTotals::of(&plans);

    if args.write || args.diff {
        for (plan, source) in &outcomes {
            let (Some(rewritten), Some((input, dialect))) =
                (plan.rewritten.clone(), source.as_ref())
            else {
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
