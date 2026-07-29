//! `query count` — how many, and how does that compare?
//!
//! The repeatable `--query` is the reason this is not `query find --output
//! summary`. Counting one pattern is a number; counting `(if ?t ?a nil)`
//! beside `(when ?t ?a)` is a migration's progress bar, and the comparison
//! only means anything when both were counted over the same file set in the
//! same run.

use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::read_input_dialect_and_tree;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::selector::Pattern;

use crate::count_report::cli::args::QueryCountArgs;
use crate::count_report::cli::render::print_count_report;
use crate::count_report::usecase::{CountedPattern, build_count_report, tally_file};
use crate::scan::selected_files;

pub fn query_count(args: QueryCountArgs) -> CommandResult {
    let dialect = args.dialect.map_or(Dialect::CommonLisp, Dialect::from);
    let patterns = args
        .query
        .iter()
        .map(|text| {
            Ok(CountedPattern {
                text: text.clone(),
                pattern: Pattern::parse(text, dialect)?,
            })
        })
        .collect::<CliResult<Vec<_>>>()?;

    let files = selected_files(&args.input, &args.roots)?;
    let mut tallies = Vec::with_capacity(files.len());
    for file in &files {
        let (_, file_dialect, tree) =
            read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        tallies.push(tally_file(file, file_dialect, &tree, &patterns));
    }

    let report = build_count_report(&patterns, tallies);
    let total = report.grand_total();
    print_count_report(&report, args.per_file, args.include_empty, args.output)?;

    if args.fail_on_match && total > 0 {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "query count policy failed: --fail-on-match, {total} match(es)"
        )));
    }
    Ok(())
}
