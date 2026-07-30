//! `query count` — how many, and how does that compare?
//!
//! The repeatable `--query` is the reason this is not `query find --output
//! summary`. Counting one pattern is a number; counting `(if ?t ?a nil)`
//! beside `(when ?t ?a)` is a migration's progress bar, and the comparison
//! only means anything when both were counted over the same file set in the
//! same run.

use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{analyze_files, note_partial_file_failures, total_file_failure};
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
    // A file that will not parse is reported, not fatal — see `query find`.
    let analysis = analyze_files(&files, args.dialect, |file, file_dialect, tree, _| {
        CliResult::Ok(tally_file(file, file_dialect, tree, &patterns))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);

    let report = build_count_report(&patterns, analysis.succeeded);
    let total = report.grand_total();
    print_count_report(&report, args.per_file, args.include_empty, args.output)?;

    if args.fail_on_match && total > 0 {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "query count policy failed: --fail-on-match, {total} match(es)"
        )));
    }
    Ok(())
}
