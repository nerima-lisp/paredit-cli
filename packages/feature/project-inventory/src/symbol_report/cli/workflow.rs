use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, matching_symbol_occurrences, note_partial_file_failures,
    read_input_dialect_and_tree, total_file_failure,
};

use super::args::{SymbolQueryArgs, SymbolReportArgs};
use super::render::{print_symbol_occurrences, print_symbol_report};
use super::types::{SymbolOccurrenceContext, SymbolReportFile, SymbolReportOccurrence};

pub fn find_symbol(args: SymbolQueryArgs) -> CommandResult {
    let (_, dialect, tree) = read_input_dialect_and_tree(args.file, args.dialect)?;
    let occurrence_count = matching_symbol_occurrences(&tree, &args.symbol).len();
    print_symbol_occurrences(&tree, dialect, &args.symbol, args.output)?;
    require_occurrences(occurrence_count, args.require_occurrences)
}

fn require_occurrences(found: usize, required: Option<usize>) -> CommandResult {
    match required {
        Some(minimum) if found < minimum => Err(paredit_core_cli::gate::gate_failure(format!(
            "require-occurrences policy failed: found {found} occurrences, required at least {minimum}"
        ))),
        _ => Ok(()),
    }
}

pub fn symbol_report(args: SymbolReportArgs) -> CommandResult {
    let analysis = analyze_files(&args.files, args.dialect, |file, dialect, tree, _| {
        let outline = tree.outline(|head| dialect.is_definition_head(head));
        let occurrences = matching_symbol_occurrences(tree, &args.symbol)
            .into_iter()
            .map(|occurrence| SymbolReportOccurrence {
                path: occurrence.path.to_string(),
                span: occurrence.span,
                context: outline
                    .iter()
                    .filter(|entry| entry.span.contains_span(occurrence.span))
                    .min_by_key(|entry| entry.span.end().get() - entry.span.start().get())
                    .map(|entry| SymbolOccurrenceContext {
                        path: entry.path.to_string(),
                        span: entry.span,
                        head: entry.head.clone(),
                        definition_like: entry.definition_like,
                    }),
            })
            .collect::<Vec<_>>();

        CliResult::Ok(SymbolReportFile {
            path: file.to_path_buf(),
            dialect,
            occurrences,
        })
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let occurrence_count = reports
        .iter()
        .map(|report| report.occurrences.len())
        .sum::<usize>();
    print_symbol_report(&reports, &args.symbol, args.output)?;
    require_occurrences(occurrence_count, args.require_occurrences)
}
