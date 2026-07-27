use anyhow::Result;

use super::super::{read_input_dialect_and_tree, write_files_with_rollback};
use super::args::RenameSymbolsArgs;
use super::render::symbols::print_rename_symbols_report;
use super::types::RenameFileReport;
use crate::presentation::cli::shared::{apply_byte_span_edits, matching_symbol_occurrences};

pub(in crate::presentation::cli) fn rename_symbols(args: RenameSymbolsArgs) -> Result<()> {
    let mut reports = Vec::with_capacity(args.files.len());
    let mut written_files = Vec::new();
    for file in &args.files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let occurrences = matching_symbol_occurrences(&tree, &args.from)
            .into_iter()
            .map(|occurrence| occurrence.span)
            .collect::<Vec<_>>();
        let edits = occurrences
            .iter()
            .map(|span| (*span, args.to.as_str().to_owned()))
            .collect::<Vec<_>>();
        let rewritten = apply_byte_span_edits(&input.text, edits)?;
        let changed = rewritten != input.text;
        let written = args.write && changed;
        if written {
            written_files.push((file.clone(), rewritten.clone()));
        }
        reports.push(RenameFileReport {
            path: file.clone(),
            dialect,
            occurrences,
            changed,
            written,
            rewritten,
        });
    }

    if !written_files.is_empty() {
        write_files_with_rollback(written_files)?;
    }

    let changed = reports.iter().any(|report| report.changed);
    print_rename_symbols_report(&reports, &args.from, &args.to, args.write, args.output)?;
    super::shared::ensure_rename_changed(args.fail_on_no_change, changed, "rename-symbols")
}
