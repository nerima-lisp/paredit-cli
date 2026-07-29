use crate::error::RenamePlanError;
use paredit_core_cli::CommandResult;

use super::args::RenameSymbolMacroArgs;
use super::render::symbol_macro::print_rename_symbol_macro_report;
use super::types::{PendingRenameSymbolMacroFile, RenameSymbolMacroFileReport};
use crate::rename::usecase as rename_usecase;
use paredit_core_cli::shared::{read_input_and_dialect, write_files_with_rollback};

pub fn rename_symbol_macro(args: RenameSymbolMacroArgs) -> CommandResult {
    let mut pending = Vec::with_capacity(args.files.len());
    let mut definition_count = 0usize;

    for file in &args.files {
        let (input, dialect) = read_input_and_dialect(Some(file.clone()), args.dialect)?;
        let plan =
            rename_usecase::plan_rename_symbol_macro(rename_usecase::RenameSymbolMacroRequest {
                input: &input.text,
                dialect,
                from: args.from.clone(),
                to: args.to.clone(),
            })
            .map_err(|source| RenamePlanError {
                operation: "rename-symbol-macro".to_owned(),
                path: file.display().to_string(),
                source,
            })?;
        let definitions = plan.definitions;
        let references = plan.references;
        definition_count += definitions.len();
        pending.push(PendingRenameSymbolMacroFile {
            path: file.clone(),
            dialect: plan.dialect,
            definitions,
            references,
            rewritten: plan.rewritten,
            changed: plan.changed,
        });
    }

    if definition_count == 0 {
        return Err(paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::SelectionNoMatch,
            "rename-symbol-macro requires at least one matching define-symbol-macro definition",
        )
        .into());
    }

    let written_files = pending
        .iter()
        .filter(|file| args.write && file.changed)
        .map(|file| (file.path.clone(), file.rewritten.clone()))
        .collect::<Vec<_>>();
    if !written_files.is_empty() {
        write_files_with_rollback(written_files)?;
    }

    let mut reports = Vec::with_capacity(pending.len());
    for file in pending {
        let written = args.write && file.changed;
        reports.push(RenameSymbolMacroFileReport {
            path: file.path,
            dialect: file.dialect,
            definitions: file.definitions,
            references: file.references,
            changed: file.changed,
            written,
            rewritten: file.rewritten,
        });
    }

    let changed = reports.iter().any(|report| report.changed);
    print_rename_symbol_macro_report(&reports, &args.from, &args.to, args.write, args.output)?;
    super::shared::ensure_rename_changed(args.fail_on_no_change, changed, "rename-symbol-macro")
}
